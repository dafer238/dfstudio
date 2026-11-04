//! Tokenizer for the DeltaF Studio equation language.
//!
//! Lexical rules: `{...}` and
//! `"..."` are comments, `//` starts a line comment, a bracket that follows a
//! numeric literal is a unit annotation (`25 [C]`), whereas a bracket that
//! follows an identifier is an array index (`T[3]`). Identifiers are
//! case-insensitive; the parser canonicalises them.

use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
    /// 1-based line of `start`.
    pub line: u32,
}

impl Span {
    pub fn to(self, other: Span) -> Span {
        Span { start: self.start, end: other.end.max(self.end), line: self.line }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Tok {
    Num(f64),
    /// Unit annotation text found inside `[...]` right after a number.
    Unit(String),
    Ident(String),
    /// Single-quoted string literal.
    Str(String),
    /// `$Directive rest of line` (name, raw rest).
    Directive(String, String),
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Eq,
    Assign, // :=
    Lt,
    Le,
    Gt,
    Ge,
    Ne,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
    Colon,
    Semicolon,
    Newline,
    Eof,
}

impl fmt::Display for Tok {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Tok::Num(v) => write!(f, "number {v}"),
            Tok::Unit(u) => write!(f, "unit [{u}]"),
            Tok::Ident(s) => write!(f, "identifier '{s}'"),
            Tok::Str(s) => write!(f, "string '{s}'"),
            Tok::Directive(d, _) => write!(f, "directive ${d}"),
            Tok::Newline => write!(f, "end of line"),
            Tok::Eof => write!(f, "end of input"),
            other => {
                let s = match other {
                    Tok::Plus => "+",
                    Tok::Minus => "-",
                    Tok::Star => "*",
                    Tok::Slash => "/",
                    Tok::Caret => "^",
                    Tok::Eq => "=",
                    Tok::Assign => ":=",
                    Tok::Lt => "<",
                    Tok::Le => "<=",
                    Tok::Gt => ">",
                    Tok::Ge => ">=",
                    Tok::Ne => "<>",
                    Tok::LParen => "(",
                    Tok::RParen => ")",
                    Tok::LBracket => "[",
                    Tok::RBracket => "]",
                    Tok::Comma => ",",
                    Tok::Colon => ":",
                    Tok::Semicolon => ";",
                    _ => "?",
                };
                write!(f, "'{s}'")
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    pub tok: Tok,
    pub span: Span,
}

/// A comment found while lexing; kept so the editor can render it and so
/// that trailing `{unit}` comments (EES habit) can be used as unit hints.
#[derive(Debug, Clone, PartialEq)]
pub struct Comment {
    pub text: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LexError {
    pub msg: String,
    pub span: Span,
}

pub struct Lexed {
    pub tokens: Vec<Token>,
    pub comments: Vec<Comment>,
    pub errors: Vec<LexError>,
}

/// Lex a script; a script written with decimal commas and `;` list
/// separators is read through [`crate::locale::normalize`] (same offsets).
pub fn lex(src: &str) -> Lexed {
    let norm = crate::locale::normalize(src);
    lex_us(&norm)
}

fn lex_us(src: &str) -> Lexed {
    let b = src.as_bytes();
    let mut i = 0usize;
    let mut line: u32 = 1;
    let mut tokens = Vec::new();
    let mut comments = Vec::new();
    let mut errors = Vec::new();
    // Paren depth: newlines inside (...) do not terminate statements.
    let mut depth: i32 = 0;

    macro_rules! push {
        ($t:expr, $s:expr, $e:expr, $l:expr) => {
            tokens.push(Token { tok: $t, span: Span { start: $s, end: $e, line: $l } })
        };
    }

    while i < b.len() {
        let c = b[i];
        let start = i;
        let start_line = line;
        match c {
            b'\n' => {
                i += 1;
                if depth <= 0 {
                    push!(Tok::Newline, start, i, start_line);
                }
                line += 1;
            }
            b' ' | b'\t' | b'\r' => i += 1,
            b'{' => {
                // nested brace comment
                let mut lvl = 0;
                while i < b.len() {
                    match b[i] {
                        b'{' => lvl += 1,
                        b'}' => {
                            lvl -= 1;
                            if lvl == 0 {
                                i += 1;
                                break;
                            }
                        }
                        b'\n' => line += 1,
                        _ => {}
                    }
                    i += 1;
                }
                if lvl > 0 {
                    errors.push(LexError { msg: "unterminated '{' comment".into(), span: Span { start, end: i, line: start_line } });
                }
                let inner_end = if lvl == 0 { i - 1 } else { i };
                comments.push(Comment { text: src[start + 1..inner_end].to_string(), span: Span { start, end: i, line: start_line } });
            }
            b'"' => {
                i += 1;
                while i < b.len() && b[i] != b'"' {
                    if b[i] == b'\n' {
                        line += 1;
                    }
                    i += 1;
                }
                let inner_end = i.min(b.len());
                if i >= b.len() {
                    errors.push(LexError { msg: "unterminated '\"' comment".into(), span: Span { start, end: i, line: start_line } });
                } else {
                    i += 1;
                }
                comments.push(Comment { text: src[start + 1..inner_end].to_string(), span: Span { start, end: i, line: start_line } });
            }
            b'/' if i + 1 < b.len() && b[i + 1] == b'/' => {
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                comments.push(Comment { text: src[start + 2..i].to_string(), span: Span { start, end: i, line: start_line } });
            }
            b'\'' => {
                i += 1;
                let s0 = i;
                while i < b.len() && b[i] != b'\'' && b[i] != b'\n' {
                    i += 1;
                }
                if i >= b.len() || b[i] != b'\'' {
                    errors.push(LexError { msg: "unterminated string".into(), span: Span { start, end: i, line: start_line } });
                    push!(Tok::Str(src[s0..i].to_string()), start, i, start_line);
                } else {
                    push!(Tok::Str(src[s0..i].to_string()), start, i + 1, start_line);
                    i += 1;
                }
            }
            b'$' => {
                i += 1;
                let s0 = i;
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                let name = src[s0..i].to_string();
                let r0 = i;
                while i < b.len() && b[i] != b'\n' {
                    i += 1;
                }
                push!(Tok::Directive(name, src[r0..i].trim().to_string()), start, i, start_line);
            }
            b'0'..=b'9' | b'.' if c != b'.' || (i + 1 < b.len() && b[i + 1].is_ascii_digit()) => {
                while i < b.len() && (b[i].is_ascii_digit() || b[i] == b'.') {
                    i += 1;
                }
                if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
                    let save = i;
                    i += 1;
                    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
                        i += 1;
                    }
                    if i < b.len() && b[i].is_ascii_digit() {
                        while i < b.len() && b[i].is_ascii_digit() {
                            i += 1;
                        }
                    } else {
                        i = save;
                    }
                }
                let text = &src[start..i];
                match text.parse::<f64>() {
                    Ok(v) => push!(Tok::Num(v), start, i, start_line),
                    Err(_) => errors.push(LexError { msg: format!("invalid number '{text}'"), span: Span { start, end: i, line: start_line } }),
                }
                // unit annotation right after a number
                let mut j = i;
                while j < b.len() && (b[j] == b' ' || b[j] == b'\t') {
                    j += 1;
                }
                if j < b.len() && b[j] == b'[' {
                    let u0 = j + 1;
                    let mut k = u0;
                    while k < b.len() && b[k] != b']' && b[k] != b'\n' {
                        k += 1;
                    }
                    if k < b.len() && b[k] == b']' {
                        push!(Tok::Unit(src[u0..k].trim().to_string()), j, k + 1, start_line);
                        i = k + 1;
                    }
                }
            }
            c if c.is_ascii_alphabetic() || c == b'_' || c >= 0x80 => {
                // `|` inside names: EES formatting marks (e.g. `h|o_f`)
                while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_' || b[i] == b'$' || b[i] == b'#' || b[i] == b'|' || b[i] >= 0x80) {
                    // allow non-ASCII letters (e.g. Greek) in identifiers
                    if b[i] >= 0x80 {
                        let ch = src[i..].chars().next().unwrap();
                        if !ch.is_alphanumeric() {
                            break;
                        }
                        i += ch.len_utf8();
                        continue;
                    }
                    i += 1;
                }
                if i == start {
                    // a non-alphanumeric unicode char
                    let ch = src[i..].chars().next().unwrap();
                    i += ch.len_utf8();
                    errors.push(LexError { msg: format!("unexpected character '{ch}'"), span: Span { start, end: i, line: start_line } });
                } else {
                    push!(Tok::Ident(src[start..i].to_string()), start, i, start_line);
                }
            }
            _ => {
                let (t, n) = match (c, b.get(i + 1).copied()) {
                    (b':', Some(b'=')) => (Tok::Assign, 2),
                    (b'<', Some(b'=')) => (Tok::Le, 2),
                    (b'>', Some(b'=')) => (Tok::Ge, 2),
                    (b'<', Some(b'>')) => (Tok::Ne, 2),
                    (b'!', Some(b'=')) => (Tok::Ne, 2),
                    (b'*', Some(b'*')) => (Tok::Caret, 2),
                    (b'+', _) => (Tok::Plus, 1),
                    (b'-', _) => (Tok::Minus, 1),
                    (b'*', _) => (Tok::Star, 1),
                    (b'/', _) => (Tok::Slash, 1),
                    (b'^', _) => (Tok::Caret, 1),
                    (b'=', _) => (Tok::Eq, 1),
                    (b'<', _) => (Tok::Lt, 1),
                    (b'>', _) => (Tok::Gt, 1),
                    (b'(', _) => {
                        depth += 1;
                        (Tok::LParen, 1)
                    }
                    (b')', _) => {
                        depth -= 1;
                        (Tok::RParen, 1)
                    }
                    (b'[', _) => {
                        // `expr [bar]` at the end of an equation: the unit of
                        // the variable the equation computes. Recognised when
                        // the bracket follows a space, is outside parentheses,
                        // holds a unit (with a letter) and ends the statement;
                        // array indices are written without a space (`T[i]`).
                        if depth <= 0 && i > 0 && (b[i - 1] == b' ' || b[i - 1] == b'\t') {
                            if let Some(k) = src[i + 1..].find(|ch: char| ch == ']' || ch == '\n').map(|k| i + 1 + k).filter(|k| b[*k] == b']') {
                                let u = src[i + 1..k].trim();
                                let rest = src[k + 1..].trim_start_matches([' ', '\t', '\r']);
                                let ends = rest.is_empty() || rest.starts_with(['\n', ';', '{', '"']) || rest.starts_with("//");
                                if ends && u.chars().next().is_some_and(|c| c.is_alphabetic()) && crate::units::parse_unit(u).is_ok() {
                                    push!(Tok::Unit(u.to_string()), start, k + 1, start_line);
                                    i = k + 1;
                                    continue;
                                }
                            }
                        }
                        (Tok::LBracket, 1)
                    }
                    (b']', _) => (Tok::RBracket, 1),
                    (b',', _) => (Tok::Comma, 1),
                    (b':', _) => (Tok::Colon, 1),
                    (b';', _) => (Tok::Semicolon, 1),
                    _ => {
                        let ch = src[i..].chars().next().unwrap();
                        i += ch.len_utf8();
                        errors.push(LexError { msg: format!("unexpected character '{ch}'"), span: Span { start, end: i, line: start_line } });
                        continue;
                    }
                };
                i += n;
                push!(t, start, i, start_line);
            }
        }
    }
    tokens.push(Token { tok: Tok::Eof, span: Span { start: b.len(), end: b.len(), line } });
    Lexed { tokens, comments, errors }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn toks(s: &str) -> Vec<Tok> {
        lex(s).tokens.into_iter().map(|t| t.tok).collect()
    }
    #[test]
    fn units_vs_indices() {
        assert_eq!(
            toks("T[1] = 25 [C] {comment {nested}} \"c2\""),
            vec![Tok::Ident("T".into()), Tok::LBracket, Tok::Num(1.0), Tok::RBracket, Tok::Eq, Tok::Num(25.0), Tok::Unit("C".into()), Tok::Eof]
        );
    }
    #[test]
    fn newline_in_parens_ignored() {
        let t = toks("a = f(1,\n 2)\nb=1");
        assert_eq!(t.iter().filter(|t| **t == Tok::Newline).count(), 1);
    }
    #[test]
    fn numbers() {
        assert_eq!(toks("1.5e-3 .5 2E3")[..3], [Tok::Num(1.5e-3), Tok::Num(0.5), Tok::Num(2000.0)]);
    }
}
