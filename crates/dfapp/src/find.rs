//! Find and replace on the editor text. Positions are char indices, the
//! unit the text widget uses for its cursor.

/// Search options.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Opts {
    /// Match case (otherwise `T` finds `t`).
    pub case: bool,
    /// Whole words only (`T` does not find the `T` in `T_1`).
    pub word: bool,
}

/// State of the find / replace bar.
#[derive(Default)]
pub struct Bar {
    pub open: bool,
    /// Show the replace row.
    pub replace: bool,
    pub query: String,
    pub with: String,
    pub opts: Opts,
    /// Search only inside this char range (taken from the selection).
    pub in_sel: Option<(usize, usize)>,
    /// Give the query field the keyboard focus next frame.
    pub focus: bool,
    /// Show the list of all matches.
    pub list: bool,
    /// Result of the last replacement.
    pub note: String,
}

/// Which matches a replacement touches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    All,
    /// Matches that end at or before this position.
    Before(usize),
    /// Matches that start at or after this position.
    After(usize),
    /// Matches that lie inside `[a, b)`.
    Range(usize, usize),
}

impl Scope {
    fn holds(self, (s, e): (usize, usize)) -> bool {
        match self {
            Scope::All => true,
            Scope::Before(c) => e <= c,
            Scope::After(c) => s >= c,
            Scope::Range(a, b) => s >= a && e <= b,
        }
    }
}

fn word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Non-overlapping matches of `pat` in `text`, left to right, as char
/// ranges `[start, end)`.
pub fn matches(text: &str, pat: &str, o: Opts) -> Vec<(usize, usize)> {
    let t: Vec<char> = text.chars().collect();
    let p: Vec<char> = pat.chars().collect();
    let (n, m) = (t.len(), p.len());
    let mut out = Vec::new();
    if m == 0 || m > n {
        return out;
    }
    let eq = |a: char, b: char| if o.case { a == b } else { a == b || fold(a) == fold(b) };
    let mut i = 0;
    while i + m <= n {
        let hit = (0..m).all(|k| eq(t[i + k], p[k])) && (!o.word || ((i == 0 || !word_char(t[i - 1])) && (i + m == n || !word_char(t[i + m]))));
        if hit {
            out.push((i, i + m));
            i += m;
        } else {
            i += 1;
        }
    }
    out
}

/// Index of the match to go to from the selection `[a, b)`: forward, the
/// first match that starts at or after `b`; backward, the last one that
/// starts before `a`. Wraps around the end of the text.
pub fn next(ms: &[(usize, usize)], (a, b): (usize, usize), forward: bool) -> Option<usize> {
    if ms.is_empty() {
        return None;
    }
    if forward {
        Some(ms.iter().position(|m| m.0 >= b).unwrap_or(0))
    } else {
        Some(ms.iter().rposition(|m| m.0 < a).unwrap_or(ms.len() - 1))
    }
}

/// Replace the matches in `scope`. Returns the new text and the replaced
/// matches (char ranges in the old text).
pub fn replace(text: &str, pat: &str, with: &str, o: Opts, scope: Scope) -> (String, Vec<(usize, usize)>) {
    let ms: Vec<_> = matches(text, pat, o).into_iter().filter(|m| scope.holds(*m)).collect();
    if ms.is_empty() {
        return (text.to_string(), ms);
    }
    // char index -> byte offset
    let mut byte: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
    byte.push(text.len());
    let mut out = String::with_capacity(text.len());
    let mut pos = 0;
    for &(s, e) in &ms {
        out.push_str(&text[pos..byte[s]]);
        out.push_str(with);
        pos = byte[e];
    }
    out.push_str(&text[pos..]);
    (out, ms)
}

/// Where char position `c` of the old text is after `replaced` matches of
/// length `m` were each replaced by `r` chars.
pub fn shift(c: usize, replaced: &[(usize, usize)], m: usize, r: usize) -> usize {
    let before = replaced.iter().filter(|x| x.1 <= c).count();
    (c + before * r).saturating_sub(before * m)
}

#[cfg(test)]
mod tests {
    use super::*;

    const O: Opts = Opts { case: false, word: false };

    #[test]
    fn find_options() {
        let t = "T[1] = t_1 + T; Tt";
        assert_eq!(matches(t, "t", O).len(), 5);
        assert_eq!(matches(t, "t", Opts { case: true, word: false }), vec![(7, 8), (17, 18)]);
        assert_eq!(matches(t, "T", Opts { case: false, word: true }), vec![(0, 1), (13, 14)]);
        assert_eq!(matches("aaaa", "aa", O), vec![(0, 2), (2, 4)]);
        assert!(matches(t, "", O).is_empty());
        // char, not byte, positions
        assert_eq!(matches("ΔT = T", "t", O), vec![(1, 2), (5, 6)]);
    }

    #[test]
    fn next_wraps() {
        let ms = [(2, 3), (5, 6), (9, 10)];
        assert_eq!(next(&ms, (0, 0), true), Some(0));
        assert_eq!(next(&ms, (2, 3), true), Some(1));
        assert_eq!(next(&ms, (9, 10), true), Some(0));
        assert_eq!(next(&ms, (5, 6), false), Some(0));
        assert_eq!(next(&ms, (2, 3), false), Some(2));
        assert_eq!(next(&[], (0, 0), true), None);
    }

    #[test]
    fn replace_scopes() {
        let t = "a x a x a";
        assert_eq!(replace(t, "a", "bb", O, Scope::All).0, "bb x bb x bb");
        assert_eq!(replace(t, "a", "bb", O, Scope::Before(4)).0, "bb x a x a");
        assert_eq!(replace(t, "a", "bb", O, Scope::After(4)).0, "a x bb x bb");
        assert_eq!(replace(t, "a", "bb", O, Scope::Range(2, 7)).0, "a x bb x a");
        assert_eq!(replace("ΔT·ΔT", "δt", "dT", O, Scope::All).0, "dT·dT");
        let (_, r) = replace(t, "a", "bb", O, Scope::All);
        assert_eq!(shift(4, &r, 1, 2), 5);
        assert_eq!(shift(9, &r, 1, 2), 12);
    }
}
