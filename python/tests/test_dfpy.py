"""Tests for the dfpy reader against a file written by the Rust core.

Generate the fixture with
    DF_FILE_OUT=python/tests/rankine.df cargo test --release -p dfapp project
"""

from pathlib import Path

import pytest

import dfpy

FIXTURE = Path(__file__).with_name("rankine.df")


@pytest.fixture(scope="module")
def project():
    return dfpy.load(FIXTURE)


def test_results(project):
    r = project.results
    assert r.converged
    assert abs(r["eta_th"] - 0.3706) < 1e-3
    assert r.unit("h[3]") == "kJ/kg"
    assert abs(r["T[3]"] - 550.0) < 1e-9
    h = r.array("h")
    assert list(h) == [1, 2, 3, 4]


def test_script_and_state(project):
    assert "eta_th" in project.script
    assert project.project["display_units"]["t[3]"] == "K"
    prefs = project.project["unit_prefs"]
    assert prefs["preset"] == "Engineering"
    assert prefs["quantities"] == {"mass_flow": "t/h"}
    assert project.meta["generator"].startswith("DF Studio")


def test_table_and_files(project):
    t = project.tables["Parametric"]
    assert t.columns == ["P[2]", "eta_th", "converged"]
    assert t["eta_th"] == [0.30, 0.33]
    assert project.files["notes.txt"] == b"hello"


def test_damage_detected():
    b = bytearray(FIXTURE.read_bytes())
    b[40] ^= 0x01
    with pytest.raises(dfpy.DfError):
        dfpy.loads(bytes(b))
    with pytest.raises(dfpy.DfError, match="not a DF Studio"):
        dfpy.loads(b"x = 1")


def test_extract(project, tmp_path):
    paths = project.extract(tmp_path)
    assert paths[0].read_bytes() == b"hello"
