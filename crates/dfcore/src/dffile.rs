//! `.df` project files: DF Studio's own binary container.
//!
//! A project file carries the equations, the application state (units,
//! tables, plots, ...), the last solution, numeric tables and any attached
//! files, so that a project opens identically on another computer.
//!
//! # Layout (version 1.0, all integers little-endian)
//!
//! ```text
//! offset size  field
//! 0      8     magic 89 44 46 0D 0A 1A 0A 00  ("\x89DF\r\n\x1a\n\0")
//! 8      2     format major version (a reader rejects a newer major)
//! 10     2     format minor version (newer minors only add section kinds)
//! 12     4     flags (reserved, 0)
//! 16     4     number of sections N
//! 20     8     byte offset of the section table
//! 28     4     CRC-32 of bytes 0..28
//! 32     ...   section payloads
//! T      ...   section table, N entries of
//!              4 tag (ASCII)  1 codec (0 stored, 1 zlib)  1 reserved  2 name length L
//!              8 payload offset  8 stored length  8 raw length
//!              4 CRC-32 of the raw (uncompressed) payload  L name (UTF-8)
//! ```
//!
//! The magic number follows the PNG signature design (ISO/IEC 15948): the
//! non-ASCII first byte and the CR-LF / Ctrl-Z / LF sequence detect files
//! damaged by text-mode transfers. Every payload is checked with CRC-32
//! (ISO 3309, the zlib polynomial), and compressed payloads use the zlib
//! format (RFC 1950/1951), so any language with zlib can read the file.
//!
//! # Sections
//!
//! | tag  | name      | content |
//! |------|-----------|---------|
//! | META |           | JSON [`Meta`] |
//! | SCRP |           | equations, UTF-8 |
//! | PROJ |           | application state, JSON (opaque to the core) |
//! | RSLT |           | last solution, binary [`Results`] |
//! | TABL | table name| numeric table, binary [`Table`] |
//! | FILE | file name | attached file, raw bytes |
//!
//! Unknown sections are kept and written back unchanged.

use crate::{Model, Solution};
use std::io::{Read, Write};
use std::path::Path;

pub const MAGIC: [u8; 8] = [0x89, b'D', b'F', b'\r', b'\n', 0x1A, b'\n', 0];
pub const VERSION: (u16, u16) = (1, 0);
const HEADER: usize = 32;

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Meta {
    /// Application that wrote the file, e.g. `DF Studio 0.1.0`.
    pub generator: String,
    /// Unix time (s) of creation and of the last save.
    pub created: u64,
    pub modified: u64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub description: String,
}

/// Values of all variables, in the model unit system.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Results {
    pub converged: bool,
    pub unit_system: String,
    /// Variable names as written by the user, e.g. `T[3]`.
    pub names: Vec<String>,
    /// Unit symbol of each value (empty if unknown or dimensionless).
    pub units: Vec<String>,
    pub values: Vec<f64>,
    pub known: Vec<bool>,
}

impl Results {
    pub fn from_solution(model: &Model, sol: &Solution) -> Self {
        let n = model.vars.len();
        Results {
            converged: sol.converged,
            unit_system: model.unit_system.name.clone(),
            names: model.vars.iter().map(|v| v.display.clone()).collect(),
            units: (0..n).map(|i| model.display_unit(i as u32).map(|u| u.0.symbol).unwrap_or_default()).collect(),
            values: sol.x[..n].to_vec(),
            known: sol.known[..n].to_vec(),
        }
    }
    /// Value of a variable by name (case-insensitive).
    pub fn get(&self, name: &str) -> Option<f64> {
        let i = self.names.iter().position(|n| n.eq_ignore_ascii_case(name))?;
        self.known[i].then_some(self.values[i])
    }
}

/// Column-oriented numeric table (e.g. a parametric study), row-major data.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Table {
    pub name: String,
    /// (column name, unit)
    pub columns: Vec<(String, String)>,
    pub rows: usize,
    pub data: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Artifact {
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RawSection {
    pub tag: [u8; 4],
    pub name: String,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct DfFile {
    pub meta: Meta,
    pub script: String,
    /// Application state as JSON text.
    pub project: Option<String>,
    pub results: Option<Results>,
    pub tables: Vec<Table>,
    pub files: Vec<Artifact>,
    /// Sections this version does not understand (kept on save).
    pub unknown: Vec<RawSection>,
}

/// Section summary for listings (`dfs info`).
#[derive(Debug, Clone)]
pub struct SectionInfo {
    pub tag: String,
    pub name: String,
    pub stored: u64,
    pub raw: u64,
    pub compressed: bool,
}

// ------------------------------------------------------------------ binary helpers

struct W(Vec<u8>);

impl W {
    fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn u64(&mut self, v: u64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn f64(&mut self, v: f64) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    fn str(&mut self, s: &str) -> Result<(), String> {
        let n = u16::try_from(s.len()).map_err(|_| format!("string too long: {}…", &s[..40.min(s.len())]))?;
        self.u16(n);
        self.0.extend_from_slice(s.as_bytes());
        Ok(())
    }
}

struct R<'a> {
    b: &'a [u8],
    p: usize,
}

impl<'a> R<'a> {
    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.p.checked_add(n).filter(|e| *e <= self.b.len()).ok_or("truncated section")?;
        let s = &self.b[self.p..end];
        self.p = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8, String> {
        Ok(self.take(1)?[0])
    }
    fn u16(&mut self) -> Result<u16, String> {
        Ok(u16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(u32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    fn u64(&mut self) -> Result<u64, String> {
        Ok(u64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn f64(&mut self) -> Result<f64, String> {
        Ok(f64::from_le_bytes(self.take(8)?.try_into().unwrap()))
    }
    fn str(&mut self) -> Result<String, String> {
        let n = self.u16()? as usize;
        String::from_utf8(self.take(n)?.to_vec()).map_err(|_| "invalid UTF-8".to_string())
    }
}

fn encode_results(r: &Results) -> Result<Vec<u8>, String> {
    let n = r.names.len();
    if r.units.len() != n || r.values.len() != n || r.known.len() != n {
        return Err("results: inconsistent lengths".into());
    }
    let mut w = W(Vec::with_capacity(n * 32));
    w.u32(n as u32);
    w.u8(r.converged as u8);
    w.str(&r.unit_system)?;
    for (name, unit) in r.names.iter().zip(&r.units) {
        w.str(name)?;
        w.str(unit)?;
    }
    for v in &r.values {
        w.f64(*v);
    }
    for k in &r.known {
        w.u8(*k as u8);
    }
    Ok(w.0)
}

fn decode_results(b: &[u8]) -> Result<Results, String> {
    let mut r = R { b, p: 0 };
    let n = r.u32()? as usize;
    // each variable needs at least 13 bytes: guards against absurd counts
    if n > b.len() / 13 + 1 {
        return Err("results: bad variable count".into());
    }
    let converged = r.u8()? != 0;
    let unit_system = r.str()?;
    let mut names = Vec::with_capacity(n);
    let mut units = Vec::with_capacity(n);
    for _ in 0..n {
        names.push(r.str()?);
        units.push(r.str()?);
    }
    let values = (0..n).map(|_| r.f64()).collect::<Result<_, _>>()?;
    let known = (0..n).map(|_| r.u8().map(|x| x != 0)).collect::<Result<_, _>>()?;
    Ok(Results { converged, unit_system, names, units, values, known })
}

fn encode_table(t: &Table) -> Result<Vec<u8>, String> {
    if t.data.len() != t.rows * t.columns.len() {
        return Err(format!("table '{}': data size does not match rows × columns", t.name));
    }
    let mut w = W(Vec::with_capacity(t.data.len() * 8 + 64));
    w.u32(t.rows as u32);
    w.u32(t.columns.len() as u32);
    for (c, u) in &t.columns {
        w.str(c)?;
        w.str(u)?;
    }
    for v in &t.data {
        w.f64(*v);
    }
    Ok(w.0)
}

fn decode_table(name: &str, b: &[u8]) -> Result<Table, String> {
    let mut r = R { b, p: 0 };
    let rows = r.u32()? as usize;
    let cols = r.u32()? as usize;
    if rows.saturating_mul(cols).saturating_mul(8) > b.len() {
        return Err(format!("table '{name}': truncated"));
    }
    let columns = (0..cols).map(|_| Ok((r.str()?, r.str()?))).collect::<Result<_, String>>()?;
    let data = (0..rows * cols).map(|_| r.f64()).collect::<Result<_, _>>()?;
    Ok(Table { name: name.into(), columns, rows, data })
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(6));
    e.write_all(data).expect("in-memory write");
    e.finish().expect("in-memory write")
}

fn unzlib(data: &[u8], raw_len: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(raw_len.min(1 << 30));
    flate2::read::ZlibDecoder::new(data).take(raw_len as u64 + 1).read_to_end(&mut out).map_err(|e| format!("decompression failed: {e}"))?;
    Ok(out)
}

// ------------------------------------------------------------------ container

impl DfFile {
    pub fn new(script: impl Into<String>) -> Self {
        DfFile { script: script.into(), ..Default::default() }
    }

    fn sections(&self) -> Result<Vec<([u8; 4], String, Vec<u8>)>, String> {
        let mut s = vec![(*b"META", String::new(), serde_json::to_vec(&self.meta).map_err(|e| e.to_string())?), (*b"SCRP", String::new(), self.script.as_bytes().to_vec())];
        if let Some(p) = &self.project {
            s.push((*b"PROJ", String::new(), p.as_bytes().to_vec()));
        }
        if let Some(r) = &self.results {
            s.push((*b"RSLT", String::new(), encode_results(r)?));
        }
        for t in &self.tables {
            s.push((*b"TABL", t.name.clone(), encode_table(t)?));
        }
        for f in &self.files {
            s.push((*b"FILE", f.name.clone(), f.data.clone()));
        }
        for u in &self.unknown {
            s.push((u.tag, u.name.clone(), u.data.clone()));
        }
        Ok(s)
    }

    pub fn to_bytes(&self) -> Result<Vec<u8>, String> {
        let secs = self.sections()?;
        let mut body = W(Vec::new());
        let mut table = W(Vec::new());
        for (tag, name, raw) in &secs {
            let crc = crc32fast::hash(raw);
            let z = if raw.len() > 128 { Some(zlib(raw)) } else { None };
            let (codec, stored) = match &z {
                Some(z) if z.len() < raw.len() => (1u8, z.as_slice()),
                _ => (0u8, raw.as_slice()),
            };
            let offset = (HEADER + body.0.len()) as u64;
            body.0.extend_from_slice(stored);
            table.0.extend_from_slice(tag);
            table.u8(codec);
            table.u8(0);
            table.u16(u16::try_from(name.len()).map_err(|_| "section name too long")?);
            table.u64(offset);
            table.u64(stored.len() as u64);
            table.u64(raw.len() as u64);
            table.u32(crc);
            table.0.extend_from_slice(name.as_bytes());
        }
        let mut h = W(Vec::with_capacity(HEADER + body.0.len() + table.0.len()));
        h.0.extend_from_slice(&MAGIC);
        h.u16(VERSION.0);
        h.u16(VERSION.1);
        h.u32(0);
        h.u32(secs.len() as u32);
        h.u64((HEADER + body.0.len()) as u64);
        let crc = crc32fast::hash(&h.0);
        h.u32(crc);
        h.0.extend_from_slice(&body.0);
        h.0.extend_from_slice(&table.0);
        Ok(h.0)
    }

    /// Parse the header and section table; returns (tag, name, codec, offset, stored, raw, crc).
    #[allow(clippy::type_complexity)]
    fn toc(b: &[u8]) -> Result<Vec<([u8; 4], String, u8, usize, usize, usize, u32)>, String> {
        if b.len() < HEADER || b[..8] != MAGIC {
            if b.len() >= 4 && &b[1..3] == b"DF" {
                return Err("damaged .df file (signature altered, e.g. by a text-mode transfer)".into());
            }
            return Err("not a DF Studio project file".into());
        }
        let mut r = R { b, p: 8 };
        let (major, minor) = (r.u16()?, r.u16()?);
        if major > VERSION.0 {
            return Err(format!("file format {major}.{minor} is newer than this program supports ({}.{}); please update DF Studio", VERSION.0, VERSION.1));
        }
        let _flags = r.u32()?;
        let n = r.u32()? as usize;
        let toc_at = r.u64()? as usize;
        let hcrc = r.u32()?;
        if crc32fast::hash(&b[..28]) != hcrc {
            return Err("damaged .df file (header checksum mismatch)".into());
        }
        if toc_at > b.len() {
            return Err("damaged .df file (truncated)".into());
        }
        let mut r = R { b, p: toc_at };
        let mut out = Vec::with_capacity(n.min(4096));
        for _ in 0..n {
            let tag: [u8; 4] = r.take(4)?.try_into().unwrap();
            let codec = r.u8()?;
            let _ = r.u8()?;
            let nl = r.u16()? as usize;
            let off = r.u64()? as usize;
            let stored = r.u64()? as usize;
            let raw = r.u64()? as usize;
            let crc = r.u32()?;
            let name = String::from_utf8(r.take(nl)?.to_vec()).map_err(|_| "invalid section name")?;
            if off.checked_add(stored).is_none_or(|e| e > toc_at) {
                return Err(format!("damaged .df file (section {} out of range)", String::from_utf8_lossy(&tag)));
            }
            out.push((tag, name, codec, off, stored, raw, crc));
        }
        Ok(out)
    }

    pub fn from_bytes(b: &[u8]) -> Result<Self, String> {
        let mut f = DfFile::default();
        let mut have_script = false;
        for (tag, name, codec, off, stored, raw_len, crc) in Self::toc(b)? {
            let label = String::from_utf8_lossy(&tag).to_string();
            let payload = &b[off..off + stored];
            let raw = match codec {
                0 => payload.to_vec(),
                1 => unzlib(payload, raw_len)?,
                c => return Err(format!("section {label}: unknown codec {c}")),
            };
            if raw.len() != raw_len || crc32fast::hash(&raw) != crc {
                return Err(format!("damaged .df file (checksum mismatch in section {label}{})", if name.is_empty() { String::new() } else { format!(" '{name}'") }));
            }
            let text = |v: Vec<u8>| String::from_utf8(v).map_err(|_| format!("section {label}: invalid UTF-8"));
            match &tag {
                b"META" => f.meta = serde_json::from_slice(&raw).map_err(|e| format!("META: {e}"))?,
                b"SCRP" => {
                    f.script = text(raw)?;
                    have_script = true;
                }
                b"PROJ" => f.project = Some(text(raw)?),
                b"RSLT" => f.results = Some(decode_results(&raw)?),
                b"TABL" => f.tables.push(decode_table(&name, &raw)?),
                b"FILE" => f.files.push(Artifact { name, data: raw }),
                _ => f.unknown.push(RawSection { tag, name, data: raw }),
            }
        }
        if !have_script {
            return Err("project file has no equations section".into());
        }
        Ok(f)
    }

    /// Section listing without decoding the payloads.
    pub fn inspect(b: &[u8]) -> Result<Vec<SectionInfo>, String> {
        Ok(Self::toc(b)?
            .into_iter()
            .map(|(tag, name, codec, _, stored, raw, _)| SectionInfo {
                tag: String::from_utf8_lossy(&tag).into(),
                name,
                stored: stored as u64,
                raw: raw as u64,
                compressed: codec == 1,
            })
            .collect())
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, String> {
        let p = path.as_ref();
        let b = std::fs::read(p).map_err(|e| format!("cannot read {}: {e}", p.display()))?;
        Self::from_bytes(&b)
    }

    /// Write atomically: a temporary file in the same folder is renamed over
    /// the target, so a crash never leaves a half-written project.
    pub fn save(&mut self, path: impl AsRef<Path>) -> Result<(), String> {
        let p = path.as_ref();
        let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        if self.meta.created == 0 {
            self.meta.created = now;
        }
        self.meta.modified = now;
        if self.meta.generator.is_empty() {
            self.meta.generator = format!("DF Studio {}", env!("CARGO_PKG_VERSION"));
        }
        let bytes = self.to_bytes()?;
        let tmp = p.with_extension("df.tmp");
        std::fs::write(&tmp, &bytes).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, p).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            format!("cannot write {}: {e}", p.display())
        })
    }
}

/// True if the bytes start with the `.df` signature.
pub fn is_df(bytes: &[u8]) -> bool {
    bytes.len() >= 8 && bytes[..8] == MAGIC
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DfFile {
        let mut f = DfFile::new("x = 2\ny = x^2 {comment}\n".repeat(20));
        f.meta = Meta { generator: "test".into(), created: 1, modified: 2, title: "Sample".into(), description: String::new() };
        f.project = Some(r#"{"units":"Eng"}"#.into());
        f.results = Some(Results {
            converged: true,
            unit_system: "Engineering".into(),
            names: vec!["x".into(), "T[3]".into()],
            units: vec![String::new(), "C".into()],
            values: vec![2.0, f64::NAN],
            known: vec![true, false],
        });
        f.tables.push(Table {
            name: "Parametric".into(),
            columns: vec![("P[2]".into(), "bar".into()), ("eta".into(), String::new())],
            rows: 2,
            data: vec![20.0, 0.30, 40.0, 0.33],
        });
        f.files.push(Artifact { name: "data/pump.csv".into(), data: b"q,h\n1,2\n".to_vec() });
        f.unknown.push(RawSection { tag: *b"XTRA", name: "future".into(), data: vec![1, 2, 3] });
        f
    }

    #[test]
    fn round_trip() {
        let f = sample();
        let b = f.to_bytes().unwrap();
        assert!(is_df(&b));
        let g = DfFile::from_bytes(&b).unwrap();
        // NaN != NaN: compare bit patterns of the values
        let (rf, rg) = (f.results.as_ref().unwrap(), g.results.as_ref().unwrap());
        assert_eq!(rf.values[1].to_bits(), rg.values[1].to_bits());
        let mut g2 = g.clone();
        g2.results.as_mut().unwrap().values[1] = 0.0;
        let mut f2 = f.clone();
        f2.results.as_mut().unwrap().values[1] = 0.0;
        assert_eq!(f2, g2);
        assert_eq!(g.results.unwrap().get("t[3]"), None);
        // the repetitive script is compressed
        let info = DfFile::inspect(&b).unwrap();
        assert!(info.iter().any(|s| s.tag == "SCRP" && s.compressed && s.stored < s.raw));
    }

    #[test]
    fn detects_damage() {
        let b = sample().to_bytes().unwrap();
        // flip one bit in every byte position of the body: must never be accepted silently
        for i in (HEADER..b.len()).step_by(7) {
            let mut c = b.clone();
            c[i] ^= 0x10;
            if let Ok(g) = DfFile::from_bytes(&c) {
                // only acceptable if the flip hit a reserved byte of the table
                assert_eq!(g.script, sample().script, "byte {i}");
            }
        }
        let mut c = b.clone();
        c[3] = b'\n'; // CR-LF -> LF conversion
        assert!(DfFile::from_bytes(&c).unwrap_err().contains("signature"));
        let mut c = b.clone();
        c[12] = 1;
        assert!(DfFile::from_bytes(&c).unwrap_err().contains("header checksum"));
        assert!(DfFile::from_bytes(&b[..b.len() - 5]).is_err());
        assert!(DfFile::from_bytes(b"x = 1").unwrap_err().contains("not a DF Studio"));
    }

    #[test]
    fn newer_major_is_rejected() {
        let mut b = sample().to_bytes().unwrap();
        b[8] = 9;
        let crc = crc32fast::hash(&b[..28]);
        b[28..32].copy_from_slice(&crc.to_le_bytes());
        assert!(DfFile::from_bytes(&b).unwrap_err().contains("newer"));
    }
}
