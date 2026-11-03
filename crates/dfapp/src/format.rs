//! Source formatter for the equation language.
//!
//! Layout rules (applied line by line; the meaning of the script never changes):
//! * statements inside `Function`/`Procedure`, `Duplicate`, `If ... Then`,
//!   `Repeat ... Until`, `While ... End` and `For ... End` blocks are
//!   indented by `indent` spaces per level; `Else`/`ElseIf` line up with
//!   their `If`;
//! * a top-level `=` or `:=` gets one space on each side, `;` and `,` (outside
//!   `[...]`) one space after (keyword arguments such as `enthalpy(CO2, T=T[1])` and the
//!   inside of strings, units `[...]` and comments are left alone);
//! * trailing comments start at column `comment_col` (or two spaces after the
//!   code when it is longer);
//! * trailing spaces are removed, runs of blank lines are reduced to one and
//!   the file ends with a single newline.
//!
//! Multi-line comments are kept verbatim.

use dfcore::lexer::lex;

#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FormatOptions {
    pub indent: usize,
    /// 0-based column where trailing comments start.
    pub comment_col: usize,
}

impl Default for FormatOptions {
    fn default() -> Self {
        FormatOptions { indent: 4, comment_col: 60 }
    }
}

/// First word of a statement, lower case.
fn first_word(code: &str) -> String {
    code.trim_start().split(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap_or("").to_lowercase()
}

fn last_word(code: &str) -> String {
    code.trim_end().rsplit(|c: char| !c.is_alphanumeric() && c != '_').next().unwrap_or("").to_lowercase()
}

/// Spacing around top-level `=`, `:=`, `;` and `,` of a code fragment
/// (no comments inside).
fn space_code(code: &str) -> String {
    let c: Vec<char> = code.chars().collect();
    let mut out = String::with_capacity(code.len() + 8);
    let mut depth = 0i32; // () and []
    let mut sq = 0i32; // [] only
    let mut quote = false;
    let mut i = 0;
    let push_op = |out: &mut String, op: &str| {
        while out.ends_with(' ') {
            out.pop();
        }
        out.push(' ');
        out.push_str(op);
        out.push(' ');
    };
    while i < c.len() {
        let ch = c[i];
        if quote {
            out.push(ch);
            if ch == '\'' {
                quote = false;
            }
            i += 1;
            continue;
        }
        match ch {
            '\'' => {
                quote = true;
                out.push(ch);
            }
            '(' | '[' => {
                depth += 1;
                sq += (ch == '[') as i32;
                out.push(ch);
            }
            ')' | ']' => {
                depth -= 1;
                sq -= (ch == ']') as i32;
                out.push(ch);
            }
            ':' if depth == 0 && c.get(i + 1) == Some(&'=') => {
                push_op(&mut out, ":=");
                i += 2;
                while i < c.len() && c[i] == ' ' {
                    i += 1;
                }
                continue;
            }
            '=' if depth == 0 => {
                let prev = out.trim_end().chars().last();
                let next = c.get(i + 1).copied();
                if matches!(prev, Some('<') | Some('>') | Some('=') | Some('!')) || next == Some('=') {
                    out.push(ch);
                } else {
                    push_op(&mut out, "=");
                    i += 1;
                    while i < c.len() && c[i] == ' ' {
                        i += 1;
                    }
                    continue;
                }
            }
            ';' | ',' if depth == 0 || (ch == ',' && sq == 0) => {
                while out.ends_with(' ') {
                    out.pop();
                }
                out.push(ch);
                out.push(' ');
                i += 1;
                while i < c.len() && c[i] == ' ' {
                    i += 1;
                }
                continue;
            }
            _ => out.push(ch),
        }
        i += 1;
    }
    out.trim_end().to_string()
}

/// Format a script (a decimal-comma script is formatted in point form and
/// converted back, so it keeps its separators).
pub fn format(text: &str, o: &FormatOptions) -> String {
    use dfcore::locale::{detect, to_comma, to_us, Separators};
    match detect(text) {
        Separators::Point => format_us(text, o),
        Separators::Comma => to_comma(&format_us(&to_us(text), o)),
    }
}

fn format_us(text: &str, o: &FormatOptions) -> String {
    let lexed = lex(text);
    // byte spans of comments and of string literals
    let comments: Vec<(usize, usize)> = lexed.comments.iter().map(|c| (c.span.start, c.span.end)).collect();
    let mut out: Vec<String> = Vec::new();
    let mut level: usize = 0;
    let mut pos = 0;
    let mut blank_run = 0;
    for raw in text.split('\n') {
        let start = pos;
        pos += raw.len() + 1;
        let line = raw.trim_end_matches('\r');
        // inside a multi-line comment that started on an earlier line: verbatim
        if comments.iter().any(|(s, e)| *s < start && start < *e) {
            out.push(line.trim_end().to_string());
            blank_run = 0;
            continue;
        }
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run == 1 && !out.is_empty() {
                out.push(String::new());
            }
            continue;
        }
        blank_run = 0;
        // split code / trailing comment at the first comment starting on this line
        let lead = line.len() - line.trim_start().len();
        let cpos = comments.iter().filter(|(s, _)| *s >= start + lead && *s < start + line.len()).map(|(s, _)| *s - start).min();
        let (code, comment) = match cpos {
            Some(k) => (line[..k].trim().to_string(), line[k..].trim_end().to_string()),
            None => (line.trim().to_string(), String::new()),
        };
        if code.is_empty() {
            // full-line comment: indent at the current level
            out.push(format!("{}{}", " ".repeat(level * o.indent), comment));
            continue;
        }
        let fw = first_word(&code);
        let lw = last_word(&code);
        // closers and middles dedent this line
        let dedent = matches!(fw.as_str(), "end" | "endif" | "until" | "else" | "elseif");
        if dedent {
            level = level.saturating_sub(1);
        }
        let directive = code.starts_with('$');
        let body = if directive { code.clone() } else { space_code(&code) };
        let mut l = format!("{}{}", " ".repeat(level * o.indent), body);
        if !comment.is_empty() {
            let w = l.chars().count();
            let col = if w + 2 <= o.comment_col { o.comment_col } else { w + 2 };
            l.push_str(&" ".repeat(col - w));
            l.push_str(&comment);
        }
        out.push(l);
        // openers indent the following lines
        let opens = match fw.as_str() {
            "function" | "procedure" | "duplicate" | "repeat" | "while" | "for" => true,
            "if" | "elseif" => lw == "then",
            "else" => true,
            _ => false,
        };
        if opens {
            level += 1;
        }
    }
    while out.last().is_some_and(|l| l.is_empty()) {
        out.pop();
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blocks_comments_and_spacing() {
        let src = "Function f(x)\nIf x>0 Then\ny:=x   \"positive\"\nElse\ny:=-x\nEndIf\nf:=y\nEnd\n\n\n\nT[1]=300 [K]   {kJ/kg}\nh=enthalpy(Water,T=T[1],P=1e5);s=1 // note\n";
        let o = FormatOptions { indent: 2, comment_col: 20 };
        let f = format(src, &o);
        let want = "Function f(x)\n  If x>0 Then\n    y := x          \"positive\"\n  Else\n    y := -x\n  EndIf\n  f := y\nEnd\n\nT[1] = 300 [K]      {kJ/kg}\nh = enthalpy(Water, T=T[1], P=1e5); s = 1  // note\n";
        assert_eq!(f, want, "\n{f}");
        // idempotent, and the model is unchanged
        assert_eq!(format(&f, &o), f);
    }

    #[test]
    fn multiline_comment_verbatim() {
        let src = "\"first line\n   second   line\"\nx=1\n";
        let f = format(src, &FormatOptions::default());
        assert_eq!(f, "\"first line\n   second   line\"\nx = 1\n");
    }

    #[test]
    fn examples_keep_their_meaning() {
        for src in [include_str!("../../../examples/rankine.dfs"), include_str!("../../../examples/brayton.dfs"), include_str!("../../../examples/refrigeration.dfs")] {
            let f = format(src, &FormatOptions::default());
            let a = dfcore::compile(src, &Default::default());
            let b = dfcore::compile(&f, &Default::default());
            assert_eq!(a.eqs.len(), b.eqs.len());
            assert_eq!(a.vars.len(), b.vars.len());
            assert!(!b.has_errors());
            assert_eq!(format(&f, &FormatOptions::default()), f);
        }
    }
}
