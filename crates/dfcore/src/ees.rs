//! Reader for EES model files (`.ees`, `.EES`).
//!
//! Two layouts exist:
//! * a plain text file (old versions and "save as text");
//! * a binary file written by EES for Windows: a 16-byte header starting
//!   with the version as a Pascal string (`[n]["X10.561"]`, the rest of the
//!   16 bytes holding e.g. the save time), then `[i32 LE: text length]
//!   [equations text]` from byte 20, followed by binary records (variable
//!   information, tables, plots, diagram). The equations text is plain
//!   (Windows-1252) or RTF when the equations window used formatting.
//!
//! The variable information follows as fixed 338-byte records:
//! `[0] name (Pascal string)`, `[31] value (80-bit extended)`, `[41] guess`,
//! `[51] lower bound`, `[61] upper bound`, `[77] units (Pascal string)`,
//! `[122] upper-case name`. This layout was read from EES 10.5 files; the
//! records are recognised by the name/upper-case-name pair, so a file whose
//! layout differs still opens, without the variable information.
//!
//! EES appends hidden `{$XX$...}` comments (registration, display settings)
//! to the equations; they are removed.

/// One entry of the EES Variable Information.
#[derive(Clone, Debug, PartialEq)]
pub struct EesVar {
    pub name: String,
    /// Value at the last solve (EES also uses it as the next guess).
    pub value: f64,
    pub units: String,
    pub lower: f64,
    pub upper: f64,
}

#[derive(Clone, Debug, Default)]
pub struct EesFile {
    /// EES version that wrote the file ("" for a text file).
    pub version: String,
    pub equations: String,
    pub vars: Vec<EesVar>,
}

/// Windows-1252 to UTF-8 (the 0x80–0x9F block mapped as in cp1252).
pub fn cp1252(b: &[u8]) -> String {
    const HI: [char; 32] = [
        '€', '\u{81}', '‚', 'ƒ', '„', '…', '†', '‡', 'ˆ', '‰', 'Š', '‹', 'Œ', '\u{8d}', 'Ž', '\u{8f}', '\u{90}', '‘', '’', '“', '”', '•', '–', '—', '˜', '™', 'š', '›', 'œ',
        '\u{9d}', 'ž', 'Ÿ',
    ];
    if let Ok(s) = std::str::from_utf8(b) {
        return s.to_string();
    }
    b.iter().map(|&c| if (0x80..0xa0).contains(&c) { HI[(c - 0x80) as usize] } else { c as char }).collect()
}

/// Plain text of an RTF document: paragraphs, tabs, escapes (`\{`, `\'e9`,
/// `\uN`), with the font/colour tables and other destinations dropped.
pub fn rtf_to_text(rtf: &str) -> String {
    let b = rtf.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut i = 0;
    // stack of "skip this group" flags
    let mut skip = vec![false];
    let mut uc_skip = 0usize;
    while i < b.len() {
        let c = b[i];
        match c {
            b'{' => {
                let s = *skip.last().unwrap();
                skip.push(s);
                i += 1;
                // destinations to drop: {\*\...} and the tables
                let rest = &rtf[i..];
                if rest.starts_with("\\*") || ["\\fonttbl", "\\colortbl", "\\stylesheet", "\\info", "\\pict", "\\listtable", "\\generator"].iter().any(|d| rest.starts_with(d)) {
                    *skip.last_mut().unwrap() = true;
                }
            }
            b'}' => {
                if skip.len() > 1 {
                    skip.pop();
                }
                i += 1;
            }
            b'\\' => {
                i += 1;
                let Some(&n) = b.get(i) else { break };
                if n == b'\\' || n == b'{' || n == b'}' {
                    if !*skip.last().unwrap() {
                        out.push(n);
                    }
                    i += 1;
                    continue;
                }
                if n == b'\'' {
                    let h = rtf.get(i + 1..i + 3).and_then(|h| u8::from_str_radix(h, 16).ok());
                    if let Some(v) = h {
                        if !*skip.last().unwrap() {
                            out.extend(cp1252(&[v]).as_bytes());
                        }
                    }
                    i += 3;
                    continue;
                }
                if !n.is_ascii_alphabetic() {
                    // \~ \- \_ and other control symbols
                    if n == b'~' && !*skip.last().unwrap() {
                        out.push(b' ');
                    }
                    i += 1;
                    continue;
                }
                let w0 = i;
                while i < b.len() && b[i].is_ascii_alphabetic() {
                    i += 1;
                }
                let word = &rtf[w0..i];
                let p0 = i;
                if i < b.len() && (b[i] == b'-' || b[i].is_ascii_digit()) {
                    i += 1;
                    while i < b.len() && b[i].is_ascii_digit() {
                        i += 1;
                    }
                }
                let param: Option<i64> = rtf[p0..i].parse().ok();
                if i < b.len() && b[i] == b' ' {
                    i += 1;
                }
                if *skip.last().unwrap() {
                    continue;
                }
                match word {
                    "par" | "line" => out.push(b'\n'),
                    "tab" => out.push(b'\t'),
                    "uc" => uc_skip = param.unwrap_or(1).max(0) as usize,
                    "u" => {
                        let v = param.unwrap_or(0);
                        let v = if v < 0 { v + 65536 } else { v } as u32;
                        if let Some(ch) = char::from_u32(v) {
                            let mut buf = [0u8; 4];
                            out.extend(ch.encode_utf8(&mut buf).as_bytes());
                        }
                        // skip the fallback characters
                        let mut k = 0;
                        while k < uc_skip && i < b.len() {
                            if b[i] == b'\\' && b.get(i + 1) == Some(&b'\'') {
                                i += 4;
                            } else {
                                i += 1;
                            }
                            k += 1;
                        }
                    }
                    "emdash" => out.extend("—".as_bytes()),
                    "endash" => out.extend("–".as_bytes()),
                    "lquote" => out.extend("‘".as_bytes()),
                    "rquote" => out.extend("’".as_bytes()),
                    "ldblquote" => out.extend("“".as_bytes()),
                    "rdblquote" => out.extend("”".as_bytes()),
                    "bullet" => out.extend("•".as_bytes()),
                    _ => {}
                }
            }
            b'\r' | b'\n' => i += 1,
            _ => {
                if !*skip.last().unwrap() {
                    out.push(c);
                }
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

/// Remove EES's hidden `{$XX$...}` comments.
fn strip_hidden(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(k) = rest.find("{$") {
        let tail = &rest[k + 2..];
        let tag: String = tail.chars().take_while(|c| c.is_ascii_alphanumeric()).collect();
        if !tag.is_empty() && tail[tag.len()..].starts_with('$') {
            if let Some(e) = tail.find('}') {
                out.push_str(&rest[..k]);
                rest = &tail[e + 1..];
                continue;
            }
        }
        out.push_str(&rest[..k + 2]);
        rest = tail;
    }
    out.push_str(rest);
    // tidy line ends and trailing blanks
    let lines: Vec<&str> = out.lines().map(|l| l.trim_end()).collect();
    let mut t = lines.join("\n");
    while t.ends_with('\n') {
        t.pop();
    }
    t.push('\n');
    t
}

/// 80-bit x87 extended float (little endian).
fn ext80(b: &[u8]) -> f64 {
    let m = u64::from_le_bytes(b[..8].try_into().unwrap());
    let se = u16::from_le_bytes([b[8], b[9]]);
    let sign = if se & 0x8000 != 0 { -1.0 } else { 1.0 };
    let e = (se & 0x7fff) as i32;
    if e == 0 && m == 0 {
        return 0.0 * sign;
    }
    if e == 0x7fff {
        return if m << 1 == 0 { sign * f64::INFINITY } else { f64::NAN };
    }
    sign * (m as f64 / 2f64.powi(63)) * 2f64.powi(e - 16383)
}

fn pascal(b: &[u8], max: usize) -> Option<&str> {
    let n = *b.first()? as usize;
    if n == 0 || n > max || n + 1 > b.len() {
        return None;
    }
    std::str::from_utf8(&b[1..1 + n]).ok()
}

const REC: usize = 338;

fn variables(b: &[u8]) -> Vec<EesVar> {
    let mut v = Vec::new();
    let mut p = 0;
    while p + REC <= b.len() {
        let rec = &b[p..p + REC];
        let ok =
            pascal(rec, 30).filter(|n| n.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '_') && n.chars().all(|c| c.is_ascii_alphanumeric() || "_[],".contains(c)));
        let upper = pascal(&rec[122..], 30);
        match (ok, upper) {
            (Some(n), Some(u)) if u == n.to_ascii_uppercase() => {
                let units = pascal(&rec[77..], 40).unwrap_or("").trim().to_string();
                v.push(EesVar { name: n.to_string(), value: ext80(&rec[31..41]), units, lower: ext80(&rec[51..61]), upper: ext80(&rec[61..71]) });
                p += REC;
            }
            _ => p += 1,
        }
    }
    // a name can appear twice (e.g. in a function's local table): keep the first
    let mut seen = std::collections::HashSet::new();
    v.retain(|x| seen.insert(x.name.to_ascii_lowercase()));
    v
}

/// Read an EES file.
pub fn read(bytes: &[u8]) -> Result<EesFile, String> {
    let n = *bytes.first().ok_or("empty file")? as usize;
    let binary = (1..=15).contains(&n) && bytes.len() > n + 13 && bytes.get(1) == Some(&b'X') && bytes[1..1 + n].iter().all(|c| c.is_ascii_graphic());
    if !binary {
        if bytes.iter().take(4096).any(|&c| c == 0) {
            return Err("not an EES file (binary data without an EES header)".into());
        }
        return Ok(EesFile { version: String::new(), equations: strip_hidden(&cp1252(bytes)), vars: Vec::new() });
    }
    let version = String::from_utf8_lossy(&bytes[1..1 + n]).trim_end_matches(',').to_string();
    if bytes.len() < 20 {
        return Err("truncated EES file".into());
    }
    let len = i32::from_le_bytes(bytes[16..20].try_into().unwrap());
    let start = 20;
    if len < 0 || start + len as usize > bytes.len() {
        return Err(format!("EES file (version {version}) with an unexpected header"));
    }
    let raw = &bytes[start..start + len as usize];
    let raw = raw.strip_suffix(&[0]).unwrap_or(raw);
    let text = cp1252(raw);
    let eqs = if text.trim_start().starts_with("{\\rtf") { rtf_to_text(&text) } else { text };
    Ok(EesFile { version, equations: strip_hidden(&eqs), vars: variables(&bytes[start + len as usize..]) })
}

impl EesFile {
    /// DeltaF Studio script: the equations, then the Variable Information as
    /// `$Guess` (values of the last EES solve), `$Bounds` (finite limits) and
    /// `$Units` (units DeltaF Studio recognises) lines.
    pub fn to_script(&self) -> String {
        let mut s = self.equations.clone();
        // only variables of the model (the file also keeps old and table entries)
        let model = crate::compile(&self.equations, &Default::default());
        // (compile reads either separator form)
        let names: std::collections::HashSet<String> = model.vars.iter().map(|v| crate::parser::canon(&v.name).replace(' ', "")).collect();
        let vars: Vec<&EesVar> = self.vars.iter().filter(|v| names.contains(&crate::parser::canon(&v.name).replace(' ', ""))).collect();
        if vars.is_empty() {
            return s;
        }
        let fmt = |x: f64| {
            let t = format!("{x:.10}");
            let t = t.trim_end_matches('0').trim_end_matches('.').to_string();
            if t.is_empty() || t == "-" {
                "0".to_string()
            } else {
                t
            }
        };
        let guesses: Vec<String> = vars.iter().filter(|v| v.value.is_finite() && v.value != 1.0).map(|v| format!("{} = {}", v.name, fmt(v.value))).collect();
        let bounds: Vec<String> = vars
            .iter()
            .filter(|v| v.lower.is_finite() && v.upper.is_finite() && v.lower.abs() < 1e30 && v.upper.abs() < 1e30 && v.lower < v.upper)
            .map(|v| format!("{} = {} .. {}", v.name, fmt(v.lower), fmt(v.upper)))
            .collect();
        let units: Vec<String> =
            vars.iter().filter(|v| !v.units.is_empty() && v.units != "-" && crate::units::parse_unit(&v.units).is_ok()).map(|v| format!("{} = {}", v.name, v.units)).collect();
        let mut add = String::new();
        add.push_str("\n\"=== Variable Information imported from EES");
        if !self.version.is_empty() {
            add.push_str(&format!(" {}", self.version.trim_start_matches('X')));
        }
        add.push_str(": guesses are the values of the last EES solution ===\"\n");
        let mut block = |dir: &str, items: &[String]| {
            for chunk in items.chunks(6) {
                add.push_str(&format!("${dir} {}\n", chunk.join(", ")));
            }
        };
        block("Units", &units);
        block("Bounds", &bounds);
        block("Guess", &guesses);
        // same separators as the equations (EES on a Spanish locale: 1,5 and ';')
        if crate::locale::detect(&self.equations) == crate::locale::Separators::Comma {
            add = crate::locale::to_comma(&add);
        }
        s.push_str(&add);
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ext_bytes(x: f64) -> [u8; 10] {
        // exact for the test values
        let (m, e) = if x == 0.0 {
            (0u64, 0u16)
        } else {
            let e = x.abs().log2().floor() as i32;
            let m = (x.abs() / 2f64.powi(e) * 2f64.powi(63)) as u64;
            (m, (e + 16383) as u16 | if x < 0.0 { 0x8000 } else { 0 })
        };
        let mut b = [0u8; 10];
        b[..8].copy_from_slice(&m.to_le_bytes());
        b[8..].copy_from_slice(&e.to_le_bytes());
        b
    }

    fn record(name: &str, value: f64, units: &str) -> Vec<u8> {
        let mut r = vec![0u8; REC];
        r[0] = name.len() as u8;
        r[1..1 + name.len()].copy_from_slice(name.as_bytes());
        r[31..41].copy_from_slice(&ext_bytes(value));
        r[41..51].copy_from_slice(&ext_bytes(1.0));
        let inf = [0, 0, 0, 0, 0, 0, 0, 0x80, 0xff, 0x7f];
        r[51..61].copy_from_slice(&inf);
        r[51 + 9] |= 0x80;
        r[61..71].copy_from_slice(&inf);
        r[77] = units.len() as u8;
        r[78..78 + units.len()].copy_from_slice(units.as_bytes());
        let up = name.to_ascii_uppercase();
        r[122] = up.len() as u8;
        r[123..123 + up.len()].copy_from_slice(up.as_bytes());
        r
    }

    #[test]
    fn rtf_equations() {
        let rtf = r"{\rtf1\ansi{\fonttbl{\f0\fnil Arial;}}{\colortbl ;\red0\green0\blue255;}\cf1 $UnitSystem SI\par h[1] = \b enthalpy\b0 (\b\i Water\b0\i0 ,T=T[1],P=1)\tab\{kJ/kg\}\par T\'e9 = 2\par}";
        assert_eq!(rtf_to_text(rtf), "$UnitSystem SI\nh[1] = enthalpy(Water,T=T[1],P=1)\t{kJ/kg}\nTé = 2\n");
    }

    #[test]
    fn binary_layout() {
        let eq = b"T_0 = 15 [C]\r\nP_0 = 1.013\r\n{$ID$ registration}{$PX$96}\r\n";
        let mut f = vec![7u8];
        f.extend(b"X10.561");
        f.extend([0u8; 8]);
        f.extend((eq.len() as i32).to_le_bytes());
        f.extend(eq);
        f.extend([1, 2, 3]);
        f.extend(record("T_0", 15.0, "C"));
        f.extend(record("P_0", 1.013, "bar"));
        f.extend(record("eta", 0.5, "-"));
        let e = read(&f).unwrap();
        assert_eq!(e.version, "X10.561");
        assert_eq!(e.equations, "T_0 = 15 [C]\nP_0 = 1.013\n");
        assert_eq!(e.vars.len(), 3);
        assert_eq!(e.vars[1].name, "P_0");
        assert!((e.vars[1].value - 1.013).abs() < 1e-12);
        assert_eq!(e.vars[0].units, "C");
        assert!(e.vars[0].lower == f64::NEG_INFINITY && e.vars[0].upper == f64::INFINITY);
        let s = e.to_script();
        assert!(s.contains("$Units T_0 = C, P_0 = bar\n"), "{s}");
        // eta is not in the equations: left out
        assert!(s.contains("$Guess T_0 = 15, P_0 = 1.013\n"), "{s}");
        assert!(!s.contains("$Bounds"));
        // the imported script compiles without errors
        let m = crate::compile(&s, &Default::default());
        assert!(!m.diagnostics.iter().any(|d| d.severity == crate::Severity::Error), "{:?}", m.diagnostics);
    }

    #[test]
    fn plain_text_file() {
        let e = read(b"$UnitSystem SI\nx = 2 {m}\n").unwrap();
        assert_eq!(e.equations, "$UnitSystem SI\nx = 2 {m}\n");
        assert!(read(&[0u8, 1, 2, 0, 0]).is_err());
    }
}
