//! File > Open recent: the files last opened or saved, newest first.
//!
//! Kept in the app storage under its own key, apart from the project
//! settings, so that opening a file never marks the project as changed.

use std::path::{Path, PathBuf};

/// Storage key of the list.
pub const KEY: &str = "dfstudio_recent";
/// Files kept in the list.
pub const MAX: usize = 10;

#[derive(serde::Serialize, serde::Deserialize, Default, Clone, Debug, PartialEq)]
pub struct Recent {
    pub files: Vec<PathBuf>,
}

/// Same file: paths compare without case on Windows, where the file
/// system ignores it.
fn same(a: &Path, b: &Path) -> bool {
    if cfg!(windows) {
        a.as_os_str().to_string_lossy().to_lowercase() == b.as_os_str().to_string_lossy().to_lowercase()
    } else {
        a == b
    }
}

impl Recent {
    /// Put `p` first (moving it up if listed) and keep the newest `MAX`.
    pub fn push(&mut self, p: &Path) {
        let p = std::path::absolute(p).unwrap_or_else(|_| p.to_path_buf());
        self.files.retain(|f| !same(f, &p));
        self.files.insert(0, p);
        self.files.truncate(MAX);
    }

    pub fn remove(&mut self, p: &Path) {
        self.files.retain(|f| !same(f, p));
    }

    /// Menu label of each file: its name, followed by the name of its
    /// folder when another listed file has the same name (the full path
    /// is in the tooltip).
    pub fn labels(&self) -> Vec<String> {
        let name = |p: &PathBuf| p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| p.display().to_string());
        let names: Vec<String> = self.files.iter().map(name).collect();
        self.files
            .iter()
            .zip(&names)
            .map(|(p, n)| {
                let dup = names.iter().filter(|m| m.to_lowercase() == n.to_lowercase()).count() > 1;
                match p.parent().and_then(|d| d.file_name()).filter(|_| dup) {
                    Some(dir) => format!("{n}  —  {}", dir.to_string_lossy()),
                    None => n.clone(),
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(s: &str) -> PathBuf {
        std::env::temp_dir().join(s)
    }

    #[test]
    fn newest_first_without_duplicates() {
        let mut r = Recent::default();
        r.push(&p("a.df"));
        r.push(&p("b.dfs"));
        r.push(&p("a.df"));
        assert_eq!(r.files, vec![p("a.df"), p("b.dfs")]);
    }

    #[test]
    fn keeps_the_newest_max() {
        let mut r = Recent::default();
        for i in 0..MAX + 3 {
            r.push(&p(&format!("m{i}.ees")));
        }
        assert_eq!(r.files.len(), MAX);
        assert_eq!(r.files[0], p(&format!("m{}.ees", MAX + 2)));
        assert_eq!(r.files[MAX - 1], p("m3.ees"));
    }

    #[test]
    fn relative_paths_are_stored_absolute() {
        let mut r = Recent::default();
        r.push(Path::new("rel.df"));
        assert!(r.files[0].is_absolute());
    }

    #[test]
    fn remove_drops_the_file() {
        let mut r = Recent::default();
        r.push(&p("a.df"));
        r.push(&p("b.df"));
        r.remove(&p("a.df"));
        assert_eq!(r.files, vec![p("b.df")]);
    }

    #[test]
    fn labels_show_the_folder_only_for_equal_names() {
        let mut r = Recent::default();
        r.push(&p("x").join("model.df"));
        r.push(&p("y").join("model.df"));
        r.push(&p("other.dfs"));
        let l = r.labels();
        assert_eq!(l[0], "other.dfs");
        assert_eq!(l[1], "model.df  —  y");
        assert_eq!(l[2], "model.df  —  x");
    }
}
