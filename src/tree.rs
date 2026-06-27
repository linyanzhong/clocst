use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::scanner::FileEntry;

pub struct TreeNode {
    pub path: PathBuf,
    pub is_file: bool,
    pub total_lines: usize,
    pub lang_breakdown: HashMap<&'static str, usize>,
    pub children: Vec<TreeNode>,
}

pub struct ColorScheme {
    pub assignments: Vec<(&'static str, crossterm::style::Color)>,
}

/// Build a `TreeNode` hierarchy mirroring the filesystem under `root`,
/// with `total_lines` and `lang_breakdown` pre-aggregated at every level.
pub fn build_tree(root: &Path, entries: &[FileEntry]) -> TreeNode {
    let mut root_node = TreeNode {
        path: root.to_path_buf(),
        is_file: false,
        total_lines: 0,
        lang_breakdown: HashMap::new(),
        children: Vec::new(),
    };
    for entry in entries {
        if let Ok(rel) = entry.path.strip_prefix(root) {
            insert(&mut root_node, root, rel, entry);
        }
    }
    aggregate(&mut root_node);
    root_node
}

/// Recursively insert `entry` into the subtree rooted at `node`.
/// `base` is the absolute path of `node`; `rel` is the path yet to be consumed.
/// When `rel` has a single component, the entry is a direct child file; otherwise
/// the first component is a directory that is created on-demand and descended into.
fn insert(node: &mut TreeNode, base: &Path, rel: &Path, entry: &FileEntry) {
    let mut comps = rel.components();
    let first = match comps.next() {
        Some(c) => c,
        None => return,
    };
    let remaining: PathBuf = comps.collect();
    let child_path = base.join(first);

    if remaining.as_os_str().is_empty() {
        node.children.push(TreeNode {
            path: child_path,
            is_file: true,
            total_lines: entry.lines,
            lang_breakdown: {
                let mut m = HashMap::new();
                m.insert(entry.language, entry.lines);
                m
            },
            children: Vec::new(),
        });
    } else {
        let pos = node.children.iter().position(|c| c.path == child_path);
        if let Some(i) = pos {
            insert(&mut node.children[i], &child_path, &remaining, entry);
        } else {
            let mut dir = TreeNode {
                path: child_path.clone(),
                is_file: false,
                total_lines: 0,
                lang_breakdown: HashMap::new(),
                children: Vec::new(),
            };
            insert(&mut dir, &child_path, &remaining, entry);
            node.children.push(dir);
        }
    }
}

/// Post-order pass: propagate each file's line counts up to its ancestor directories.
fn aggregate(node: &mut TreeNode) {
    if node.is_file {
        return;
    }
    for child in &mut node.children {
        aggregate(child);
        node.total_lines += child.total_lines;
        for (&lang, &lines) in &child.lang_breakdown {
            *node.lang_breakdown.entry(lang).or_insert(0) += lines;
        }
    }
}

/// Rank all languages in the tree by total line count and assign one color each,
/// cycling through `colors` if there are more top languages than color slots.
pub fn compute_color_scheme(
    root: &TreeNode,
    top_n: usize,
    colors: &[crossterm::style::Color],
) -> ColorScheme {
    let mut totals: Vec<(&'static str, usize)> = root
        .lang_breakdown
        .iter()
        .map(|(&lang, &lines)| (lang, lines))
        .collect();
    totals.sort_by(|a, b| b.1.cmp(&a.1));
    totals.truncate(top_n);

    let assignments = totals
        .into_iter()
        .enumerate()
        .map(|(i, (lang, _))| (lang, colors[i % colors.len()]))
        .collect();

    ColorScheme { assignments }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::style::Color;

    fn entry(path: &str, lang: &'static str, lines: usize) -> FileEntry {
        FileEntry { path: PathBuf::from(path), language: lang, lines }
    }

    #[test]
    fn builds_flat_directory() {
        let root = PathBuf::from("/repo");
        let entries = vec![
            entry("/repo/main.rs", "Rust", 10),
            entry("/repo/lib.py", "Python", 5),
        ];
        let tree = build_tree(&root, &entries);

        assert_eq!(tree.total_lines, 15);
        assert_eq!(tree.children.len(), 2);
        assert_eq!(*tree.lang_breakdown.get("Rust").unwrap(), 10);
        assert_eq!(*tree.lang_breakdown.get("Python").unwrap(), 5);
    }

    #[test]
    fn builds_nested_directory() {
        let root = PathBuf::from("/repo");
        let entries = vec![
            entry("/repo/src/main.rs", "Rust", 20),
            entry("/repo/src/lib.rs", "Rust", 10),
            entry("/repo/README.md", "Markdown", 3),
        ];
        let tree = build_tree(&root, &entries);

        assert_eq!(tree.total_lines, 33);
        assert_eq!(tree.children.len(), 2);

        let src = tree.children.iter().find(|c| c.path.ends_with("src")).unwrap();
        assert_eq!(src.total_lines, 30);
        assert_eq!(src.children.len(), 2);
        assert_eq!(*src.lang_breakdown.get("Rust").unwrap(), 30);
    }

    #[test]
    fn assigns_top_languages_by_lines() {
        let root = PathBuf::from("/repo");
        let entries = vec![
            entry("/repo/a.rs", "Rust", 100),
            entry("/repo/b.rs", "Rust", 50),
            entry("/repo/c.py", "Python", 30),
            entry("/repo/d.js", "JavaScript", 10),
            entry("/repo/e.go", "Go", 5),
        ];
        let tree = build_tree(&root, &entries);
        let colors = vec![Color::Blue, Color::Green, Color::Yellow];
        let scheme = compute_color_scheme(&tree, 3, &colors);

        assert_eq!(scheme.assignments.len(), 3);
        assert_eq!(scheme.assignments[0].0, "Rust");
        assert_eq!(scheme.assignments[0].1, Color::Blue);
        assert_eq!(scheme.assignments[1].0, "Python");
        assert_eq!(scheme.assignments[1].1, Color::Green);
        assert_eq!(scheme.assignments[2].0, "JavaScript");
    }
}
