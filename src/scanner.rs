use ignore::WalkBuilder;
use rayon::prelude::*;
use std::path::Path;

use crate::languages::extension_to_language;

pub struct FileEntry {
    pub path: std::path::PathBuf,
    pub language: &'static str,
    pub lines: usize,
}

/// Walk `root` and return one `FileEntry` per recognized source file.
/// Files whose extension is not in the language map are silently skipped.
/// When `no_ignore` is true, `.gitignore` / `.ignore` rules are disabled.
pub fn scan(root: &Path, no_ignore: bool) -> Vec<FileEntry> {
    let paths: Vec<_> = WalkBuilder::new(root)
        .hidden(false)
        // The `ignore` crate's builder methods accept `true` to *enable* a rule,
        // so we negate the user's `--no-ignore` flag to get the right polarity.
        .ignore(!no_ignore)
        .git_ignore(!no_ignore)
        .git_global(!no_ignore)
        .git_exclude(!no_ignore)
        .build()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|f| f.is_file()).unwrap_or(false))
        .filter_map(|e| {
            let path = e.into_path();
            let ext = path.extension().and_then(|s| s.to_str())?;
            let lang = extension_to_language(ext)?;
            Some((path, lang))
        })
        .collect();

    paths
        .into_par_iter()
        .filter_map(|(path, language)| {
            let content = std::fs::read(&path).ok()?;
            let lines = count_lines(&content);
            Some(FileEntry { path, language, lines })
        })
        .collect()
}

/// Count logical lines in a byte buffer.
/// A file with no trailing newline still counts its last line.
fn count_lines(content: &[u8]) -> usize {
    if content.is_empty() {
        return 0;
    }
    let newlines = content.iter().filter(|&&b| b == b'\n').count();
    // A file like "a\nb" has one '\n' but two lines; only skip the +1 when the
    // file ends with '\n', meaning the last newline has no content after it.
    if content.last() == Some(&b'\n') { newlines } else { newlines + 1 }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn make_fixture() -> TempDir {
        let dir = TempDir::new().unwrap();
        fs::write(dir.path().join("main.rs"), "fn main() {\n    println!(\"hi\");\n}\n").unwrap();
        fs::write(dir.path().join("lib.py"), "def foo():\n    pass\n").unwrap();
        fs::write(dir.path().join("README.md"), "# Title\n").unwrap();
        fs::write(dir.path().join("binary.bin"), b"\x00\x01\x02".as_ref()).unwrap();
        dir
    }

    #[test]
    fn scans_known_files_only() {
        let dir = make_fixture();
        let entries = scan(dir.path(), true);
        assert_eq!(entries.len(), 3); // binary.bin excluded
        assert!(entries.iter().any(|e| e.language == "Rust" && e.lines == 3));
        assert!(entries.iter().any(|e| e.language == "Python" && e.lines == 2));
        assert!(entries.iter().any(|e| e.language == "Markdown" && e.lines == 1));
    }

    #[test]
    fn counts_lines_without_trailing_newline() {
        assert_eq!(count_lines(b"fn main() {}"), 1);
        assert_eq!(count_lines(b"a\nb"), 2);
        assert_eq!(count_lines(b"a\nb\n"), 2);
        assert_eq!(count_lines(b""), 0);
    }
}
