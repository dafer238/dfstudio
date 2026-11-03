"""Read DF Studio ``.df`` project files for post-processing in Python.

The reader only needs the standard library; ``numpy`` and ``pandas`` are used
when available (``Table.to_numpy``, ``Table.to_pandas``,
``Results.to_pandas``).

Example
-------
>>> import dfpy
>>> p = dfpy.load("rankine.df")
>>> p.results["eta_th"]                # value in the model unit system
0.3706...
>>> p.results.unit("h[3]")
'kJ/kg'
>>> p.results.array("h")               # {index: value} for an array
{1: 191.8, 2: 209.6, 3: 3450.4, 4: 2231.5}
>>> df = p.tables["Parametric"].to_pandas()
>>> p.files["pump.csv"]                # attached file, bytes

The binary layout is documented in ``crates/dfcore/src/dffile.rs``; this
module mirrors it.
"""

from __future__ import annotations

import json
import re
import struct
import zlib
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, Iterator, List, Optional, Tuple, Union

__all__ = ["load", "loads", "Project", "Results", "Table", "DfError", "MAGIC", "VERSION"]

MAGIC = b"\x89DF\r\n\x1a\n\x00"
VERSION = (1, 0)
_HEADER = struct.Struct("<8sHHIIQI")  # 32 bytes
_ENTRY = struct.Struct("<4sBBHQQQI")  # 36 bytes + name


class DfError(ValueError):
    """The file is not a valid (or not a supported) ``.df`` project."""


class _Reader:
    def __init__(self, b: bytes):
        self.b, self.p = b, 0

    def take(self, n: int) -> bytes:
        if self.p + n > len(self.b):
            raise DfError("truncated section")
        s = self.b[self.p : self.p + n]
        self.p += n
        return s

    def unpack(self, fmt: str):
        s = struct.calcsize(fmt)
        return struct.unpack(fmt, self.take(s))

    def str(self) -> str:
        (n,) = self.unpack("<H")
        return self.take(n).decode("utf-8")

    def f64s(self, n: int) -> List[float]:
        return list(struct.unpack(f"<{n}d", self.take(8 * n)))


_INDEX = re.compile(r"^(.*)\[\s*(-?\d+)\s*\]$")


@dataclass
class Results:
    """Last solution: every variable in the model unit system."""

    converged: bool
    unit_system: str
    names: List[str]
    units: List[str]
    values: List[float]
    known: List[bool]

    def _index(self, name: str) -> int:
        key = name.replace(" ", "").lower()
        for i, n in enumerate(self.names):
            if n.replace(" ", "").lower() == key:
                return i
        raise KeyError(name)

    def __getitem__(self, name: str) -> float:
        i = self._index(name)
        if not self.known[i]:
            raise KeyError(f"{name} has no value (not determined)")
        return self.values[i]

    def get(self, name: str, default=None):
        try:
            return self[name]
        except KeyError:
            return default

    def __contains__(self, name: str) -> bool:
        return self.get(name) is not None

    def __iter__(self) -> Iterator[str]:
        return (n for n, k in zip(self.names, self.known) if k)

    def __len__(self) -> int:
        return sum(self.known)

    def unit(self, name: str) -> str:
        return self.units[self._index(name)]

    def items(self):
        return ((n, v) for n, v, k in zip(self.names, self.values, self.known) if k)

    def array(self, base: str) -> Dict[int, float]:
        """Elements of a 1-D array, ``{index: value}``, sorted by index."""
        out = {}
        for n, v, k in zip(self.names, self.values, self.known):
            m = _INDEX.match(n)
            if k and m and m.group(1).lower() == base.lower():
                out[int(m.group(2))] = v
        return dict(sorted(out.items()))

    def to_pandas(self):
        import pandas as pd

        return pd.DataFrame(
            {"value": [v if k else float("nan") for v, k in zip(self.values, self.known)], "unit": self.units, "known": self.known},
            index=pd.Index(self.names, name="variable"),
        )


@dataclass
class Table:
    """Numeric table (e.g. a parametric study), row-major."""

    name: str
    columns: List[str]
    units: List[str]
    rows: int
    data: List[float]

    def column(self, name: str) -> List[float]:
        j = [c.lower() for c in self.columns].index(name.lower())
        n = len(self.columns)
        return self.data[j::n]

    def __getitem__(self, name: str) -> List[float]:
        return self.column(name)

    def to_numpy(self):
        import numpy as np

        return np.asarray(self.data, dtype=float).reshape(self.rows, len(self.columns))

    def to_pandas(self):
        import pandas as pd

        return pd.DataFrame(self.to_numpy(), columns=self.columns)


@dataclass
class Project:
    """Contents of a ``.df`` file."""

    version: Tuple[int, int]
    meta: dict
    script: str
    project: dict = field(default_factory=dict)
    results: Optional[Results] = None
    tables: Dict[str, Table] = field(default_factory=dict)
    files: Dict[str, bytes] = field(default_factory=dict)
    #: sections this reader does not know: (tag, name) -> bytes
    unknown: Dict[Tuple[str, str], bytes] = field(default_factory=dict)

    def extract(self, folder: Union[str, Path]) -> List[Path]:
        """Write the attached files into ``folder``; returns their paths."""
        out = []
        root = Path(folder).resolve()
        for name, data in self.files.items():
            p = (root / name).resolve()
            if root not in p.parents and p != root:  # no path traversal
                raise DfError(f"unsafe file name in project: {name!r}")
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_bytes(data)
            out.append(p)
        return out


def _results(b: bytes) -> Results:
    r = _Reader(b)
    (n,) = r.unpack("<I")
    if n > len(b) // 13 + 1:
        raise DfError("results: bad variable count")
    (conv,) = r.unpack("<B")
    us = r.str()
    names, units = [], []
    for _ in range(n):
        names.append(r.str())
        units.append(r.str())
    values = r.f64s(n)
    known = [x != 0 for x in r.take(n)]
    return Results(bool(conv), us, names, units, values, known)


def _table(name: str, b: bytes) -> Table:
    r = _Reader(b)
    rows, cols = r.unpack("<II")
    if rows * cols * 8 > len(b):
        raise DfError(f"table {name!r}: truncated")
    columns, units = [], []
    for _ in range(cols):
        columns.append(r.str())
        units.append(r.str())
    return Table(name, columns, units, rows, r.f64s(rows * cols))


def loads(b: bytes) -> Project:
    """Parse the bytes of a ``.df`` file."""
    if len(b) < _HEADER.size or b[:8] != MAGIC:
        if b[1:3] == b"DF":
            raise DfError("damaged .df file (signature altered, e.g. by a text-mode transfer)")
        raise DfError("not a DF Studio project file")
    _, major, minor, _flags, n, toc, hcrc = _HEADER.unpack_from(b)
    if major > VERSION[0]:
        raise DfError(f"file format {major}.{minor} is newer than this reader ({VERSION[0]}.{VERSION[1]})")
    if zlib.crc32(b[:28]) != hcrc:
        raise DfError("damaged .df file (header checksum mismatch)")
    if toc > len(b):
        raise DfError("damaged .df file (truncated)")
    p = Project(version=(major, minor), meta={}, script="")
    have_script = False
    pos = toc
    for _ in range(n):
        if pos + _ENTRY.size > len(b):
            raise DfError("damaged .df file (truncated section table)")
        tag, codec, _res, nl, off, stored, raw_len, crc = _ENTRY.unpack_from(b, pos)
        pos += _ENTRY.size
        name = b[pos : pos + nl].decode("utf-8")
        pos += nl
        label = tag.decode("ascii", "replace")
        if off + stored > toc:
            raise DfError(f"damaged .df file (section {label} out of range)")
        payload = b[off : off + stored]
        if codec == 0:
            raw = payload
        elif codec == 1:
            raw = zlib.decompress(payload)
        else:
            raise DfError(f"section {label}: unknown codec {codec}")
        if len(raw) != raw_len or zlib.crc32(raw) != crc:
            raise DfError(f"damaged .df file (checksum mismatch in section {label} {name!r})")
        if tag == b"META":
            p.meta = json.loads(raw)
        elif tag == b"SCRP":
            p.script = raw.decode("utf-8")
            have_script = True
        elif tag == b"PROJ":
            p.project = json.loads(raw)
        elif tag == b"RSLT":
            p.results = _results(raw)
        elif tag == b"TABL":
            p.tables[name] = _table(name, raw)
        elif tag == b"FILE":
            p.files[name] = raw
        else:
            p.unknown[(label, name)] = raw
    if not have_script:
        raise DfError("project file has no equations section")
    return p


def load(path: Union[str, Path]) -> Project:
    """Read a ``.df`` project file."""
    return loads(Path(path).read_bytes())
