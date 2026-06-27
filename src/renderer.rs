use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::ExecutableCommand;
use std::cmp::Ordering;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::io::{stdout, Write};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

use crate::tree::{ColorScheme, TreeNode};

const RESERVED_TERM_ROWS: usize = 8;
const TREE_GAP: &str = "   ";
const ROOT_TREE_GAP: &str = " ";

#[derive(Clone, Debug)]
struct VisibleNode {
    label: String,
    is_file: bool,
    total_lines: usize,
    lang_breakdown: HashMap<&'static str, usize>,
    children: Vec<VisibleNode>,
}

/// Format a raw line count into a compact 4-character string (e.g. "1.5K", " 12K", "1.2M").
///
/// The one-decimal forms ("X.YK" / "X.YM") are only 4 columns wide while the
/// integer part is a single digit. A count whose mantissa rounds up to 10.0
/// (e.g. 9_999 → "10.0K") would be 5 columns and break column alignment, so the
/// thresholds below hand such counts to the next-wider, integer-formatted unit
/// instead (9_999 → " 10K", 999_999 → "1.0M"). The integer branches round rather
/// than truncate so the spilled-over value is accurate.
pub(crate) fn format_lines(n: usize) -> String {
    if n < 1_000 {
        format!("{:4}", n)
    } else if n < 9_950 {
        format!("{:.1}K", n as f64 / 1_000.0)
    } else if n < 999_500 {
        format!("{:3}K", (n as f64 / 1_000.0).round() as usize)
    } else if n < 9_950_000 {
        format!("{:.1}M", n as f64 / 1_000_000.0)
    } else {
        format!("{:3}M", (n as f64 / 1_000_000.0).round() as usize)
    }
}

/// Fit `s` into exactly `max_width` terminal columns: truncate with a trailing
/// '…' if it overflows, otherwise pad with spaces.
///
/// Width is measured in display columns via `unicode-width` (CJK and other wide
/// glyphs count as 2), not in `char`s, so that filenames containing wide
/// characters keep the bar column aligned with ASCII-named rows.
pub(crate) fn fit_to_width(s: &str, max_width: usize) -> String {
    if s.width() <= max_width {
        return format!("{}{}", s, " ".repeat(max_width - s.width()));
    }
    // Reserve one column for the ellipsis, then take whole chars until the next
    // one would not fit (a wide char on the boundary is dropped, not split).
    let budget = max_width.saturating_sub(1);
    let mut out = String::new();
    let mut used = 0;
    for ch in s.chars() {
        let cw = ch.width().unwrap_or(0);
        if used + cw > budget {
            break;
        }
        out.push(ch);
        used += cw;
    }
    out.push('…');
    // Pad any column left over when a wide char was dropped at the boundary.
    format!("{}{}", out, " ".repeat(max_width.saturating_sub(used + 1)))
}

/// Compute how many bar characters (`█`) this node should fill, proportional
/// to its share of the root's total lines, rounded to the nearest character.
pub(crate) fn bar_fill_width(node_lines: usize, root_lines: usize, bar_width: usize) -> usize {
    if root_lines == 0 {
        return 0;
    }
    ((node_lines as f64 / root_lines as f64) * bar_width as f64).round() as usize
}

struct Row<'a> {
    node: &'a VisibleNode,
    prefix: String,
    connector: String,
    is_root: bool,
}

#[derive(Clone)]
struct Candidate<'a> {
    path: Vec<usize>,
    node: &'a TreeNode,
    depth: usize,
}

impl PartialEq for Candidate<'_> {
    fn eq(&self, other: &Self) -> bool {
        self.node.path == other.node.path && self.node.total_lines == other.node.total_lines
    }
}

impl Eq for Candidate<'_> {}

impl Ord for Candidate<'_> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.node
            .total_lines
            .cmp(&other.node.total_lines)
            .then_with(|| self.node.path.cmp(&other.node.path))
    }
}

impl PartialOrd for Candidate<'_> {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

fn visible_row_budget(term_height: usize) -> usize {
    term_height.saturating_sub(RESERVED_TERM_ROWS)
}

/// Return a node's children sorted ascending by line count.
/// Ascending order means the smallest entries appear first in the output list,
/// which — because the tree is printed bottom-up — places them at the top of
/// the terminal, matching dust's visual convention.
fn sorted_children(node: &TreeNode) -> Vec<&TreeNode> {
    let mut children: Vec<&TreeNode> = node.children.iter().collect();
    children.sort_by(|a, b| a.total_lines.cmp(&b.total_lines));
    children
}

fn label_for(path: &std::path::Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("?")
        .to_string()
}

/// Convert the full `TreeNode` tree into a `VisibleNode` tree that fits within
/// the available terminal rows.
///
/// `select_visible_paths` greedily selects the highest-value nodes, but the
/// synthetic "N others" rows it produces during `rebuild_visible_children` can
/// push the actual row count above the budget.  The loop tightens `node_budget`
/// by one row at a time until the final row count fits, then returns that tree.
fn build_visible_tree(root: &TreeNode, max_depth: Option<usize>, term_height: usize) -> VisibleNode {
    let budget = visible_row_budget(term_height);
    let child_budget = budget.saturating_sub(1); // root row is not counted here
    let mut node_budget = budget;

    loop {
        let allowed = select_visible_paths(root, max_depth, node_budget);
        let visible = VisibleNode {
            label: root
                .path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or(root.path.to_str().unwrap_or("?"))
                .to_string(),
            is_file: false,
            total_lines: root.total_lines,
            lang_breakdown: root.lang_breakdown.clone(),
            children: rebuild_visible_children(root, &allowed, &[]),
        };

        // Re-count actual rows after "N others" nodes are inserted; if it still
        // overflows, shrink the budget and try again.
        if count_rows_runtime(&visible.children, 1, max_depth) <= child_budget || node_budget == 0 {
            return visible;
        }
        node_budget -= 1;
    }
}

/// Recursively count how many terminal rows `nodes` would occupy, respecting
/// the optional depth limit.  Used after tree construction to verify the
/// candidate visible tree actually fits the row budget.
///
/// Callers pass `depth = 1` for the root's children (the root row is counted
/// separately), so the `depth > limit` cutoff here prunes the same rows that
/// `collect_rows` skips with its `depth >= max_depth` check at `depth = 0` — the
/// two stay in agreement, so the budget computed here matches what is rendered.
fn count_rows_runtime(nodes: &[VisibleNode], depth: usize, max_depth: Option<usize>) -> usize {
    if max_depth.map(|limit| depth > limit).unwrap_or(false) {
        return 0;
    }

    nodes.iter()
        .map(|node| 1 + count_rows_runtime(&node.children, depth + 1, max_depth))
        .sum()
}

/// Select which tree paths to display using a greedy max-heap strategy,
/// mirroring how dust decides what to show.
///
/// Algorithm:
/// 1. Seed the heap with the root's direct children.
/// 2. Each iteration pops the highest-line-count frontier node and marks it
///    visible (adds its index path to `allowed`).
/// 3. If the node is an expandable directory and the depth limit allows, push
///    its children onto the heap so they compete for future budget slots.
/// 4. Stop when `budget` paths are selected or the heap is empty.
///
/// Paths are encoded as index sequences (e.g. [1, 0, 2]) rather than pointers
/// so they can be stored in a `HashSet` without lifetime issues. Each index is a
/// position within `sorted_children`, so `rebuild_visible_children` must walk the
/// same `sorted_children` order for the paths to line up — both do, and
/// `sort_by` is stable, so equal-sized siblings keep a deterministic order.
///
/// Note the resulting set is prefix-closed: a child path is only ever pushed
/// onto the heap after its parent has been popped and inserted, so an allowed
/// node's ancestors are always allowed too. `rebuild_visible_children` relies on
/// this when it recurses only into allowed children.
fn select_visible_paths(root: &TreeNode, max_depth: Option<usize>, budget: usize) -> HashSet<Vec<usize>> {
    let mut heap = BinaryHeap::new();
    for (index, child) in sorted_children(root).into_iter().enumerate() {
        heap.push(Candidate { path: vec![index], node: child, depth: 1 });
    }

    let mut allowed = HashSet::new();
    while allowed.len() < budget {
        let Some(candidate) = heap.pop() else {
            break;
        };
        allowed.insert(candidate.path.clone());

        let can_descend = !candidate.node.is_file
            && max_depth.map(|limit| candidate.depth < limit).unwrap_or(true);
        if can_descend {
            for (index, child) in sorted_children(candidate.node).into_iter().enumerate() {
                let mut child_path = candidate.path.clone();
                child_path.push(index);
                heap.push(Candidate {
                    path: child_path,
                    node: child,
                    depth: candidate.depth + 1,
                });
            }
        }
    }

    allowed
}

#[cfg(test)]
fn count_rows(nodes: &[VisibleNode]) -> usize {
    nodes.iter().map(|node| 1 + count_rows(&node.children)).sum()
}

/// Build the `VisibleNode` children for `node`, including only paths present in
/// `allowed`.  Pruned siblings are collapsed into a single synthetic "N others"
/// node so the user can still see their aggregate line weight and language mix.
fn rebuild_visible_children(
    node: &TreeNode,
    allowed: &HashSet<Vec<usize>>,
    base_path: &[usize],
) -> Vec<VisibleNode> {
    let children = sorted_children(node);
    let mut visible = Vec::new();
    let mut hidden_total_lines = 0;
    let mut hidden_lang_breakdown = HashMap::new();
    let mut hidden_count = 0usize;

    for (index, child) in children.into_iter().enumerate() {
        let mut child_path = base_path.to_vec();
        child_path.push(index);

        if allowed.contains(&child_path) {
            visible.push(VisibleNode {
                label: label_for(&child.path),
                is_file: child.is_file,
                total_lines: child.total_lines,
                lang_breakdown: child.lang_breakdown.clone(),
                children: rebuild_visible_children(child, allowed, &child_path),
            });
        } else {
            hidden_total_lines += child.total_lines;
            hidden_count += 1;
            for (&lang, &lines) in &child.lang_breakdown {
                *hidden_lang_breakdown.entry(lang).or_insert(0) += lines;
            }
        }
    }

    if hidden_count > 0 {
        visible.push(VisibleNode {
            label: format!("{hidden_count} others"),
            is_file: false,
            total_lines: hidden_total_lines,
            lang_breakdown: hidden_lang_breakdown,
            children: Vec::new(),
        });
    }

    visible
}

/// Traverse the visible tree and append each node to `rows` in bottom-up order
/// (children before their parent) so that the printed list reads like dust's
/// inverted tree, with the root at the bottom of the terminal.
///
/// Each directory recurses into its children first, then appends itself,
/// ensuring a child always appears above its parent in the output.
fn collect_rows<'a>(
    node: &'a VisibleNode,
    depth: usize,
    max_depth: Option<usize>,
    prefix: &str,
    rows: &mut Vec<Row<'a>>,
) {
    if max_depth.map(|d| depth >= d).unwrap_or(false) {
        return;
    }
    let children: Vec<&VisibleNode> = node.children.iter().collect();

    for (i, child) in children.iter().enumerate() {
        let branch = if i == 0 { "┌" } else { "├" };
        let tail = if child.children.is_empty() { "── " } else { "─┴ " };
        let connector = format!("{branch}{tail}");
        if !child.is_file {
            let child_prefix = format!("{}{}", prefix, if i == 0 { "  " } else { "│ " });
            collect_rows(child, depth + 1, max_depth, &child_prefix, rows);
        }
        rows.push(Row { node: child, prefix: prefix.to_string(), connector, is_root: false });
    }
}

/// The spacing string inserted between the line-count column and the tree connector.
/// The root row uses a narrower gap because its connector ("┌─┴ ") is already
/// wider than the per-level indent used for non-root rows.
fn tree_gap(row: &Row) -> &'static str {
    if row.is_root && !row.connector.is_empty() {
        ROOT_TREE_GAP
    } else {
        TREE_GAP
    }
}

#[cfg(test)]
fn row_labels(root: &VisibleNode, max_depth: Option<usize>) -> Vec<String> {
    let mut rows = Vec::new();
    collect_rows(root, 0, max_depth, "", &mut rows);
    rows.push(Row {
        node: root,
        prefix: String::new(),
        connector: if root.children.is_empty() { String::new() } else { "┌─┴ ".to_string() },
        is_root: true,
    });
    rows.into_iter()
        .map(|row| format!("{}{}{}", row.prefix, row.connector, row.node.label))
        .collect()
}

/// Build the list of colored bar segments for a node's progress bar.
/// Each top language gets a proportional colored segment; the remaining filled
/// width (lines from unlisted languages) becomes a single un-colored segment
/// that the caller renders in dark grey.
fn build_bar(
    node: &VisibleNode,
    root_lines: usize,
    bar_width: usize,
    scheme: &ColorScheme,
) -> Vec<(usize, Option<Color>)> {
    let filled = bar_fill_width(node.total_lines, root_lines, bar_width);
    let mut segs: Vec<(usize, Option<Color>)> = Vec::new();
    let mut used = 0usize;

    for (lang, color) in &scheme.assignments {
        let lang_lines = node.lang_breakdown.get(lang).copied().unwrap_or(0);
        let w = bar_fill_width(lang_lines, root_lines, bar_width).min(filled.saturating_sub(used));
        if w > 0 {
            segs.push((w, Some(*color)));
            used += w;
        }
    }
    let other = filled.saturating_sub(used);
    if other > 0 {
        segs.push((other, None));
    }
    segs
}

fn render_legend(out: &mut std::io::Stdout, scheme: &ColorScheme) {
    let _ = out.execute(Print("\n"));
    let _ = out.execute(Print("legend: "));
    for (index, (lang, color)) in scheme.assignments.iter().enumerate() {
        if index > 0 {
            let _ = out.execute(Print("  "));
        }
        let _ = out.execute(SetForegroundColor(*color));
        let _ = out.execute(Print("██"));
        let _ = out.execute(ResetColor);
        let _ = out.execute(Print(format!(" {lang}")));
    }
    if !scheme.assignments.is_empty() {
        let _ = out.execute(Print("  "));
    }
    let _ = out.execute(SetForegroundColor(Color::DarkGrey));
    let _ = out.execute(Print("██"));
    let _ = out.execute(ResetColor);
    let _ = out.execute(Print(" Others"));
    let _ = out.execute(Print("\n"));
}

#[cfg(test)]
fn legend_items(scheme: &ColorScheme) -> Vec<(&'static str, Option<Color>)> {
    let mut items: Vec<(&'static str, Option<Color>)> = scheme
        .assignments
        .iter()
        .map(|(lang, color)| (*lang, Some(*color)))
        .collect();
    items.push(("Others", None));
    items
}

pub fn render(
    root: &TreeNode,
    scheme: &ColorScheme,
    max_depth: Option<usize>,
    term_height: usize,
    term_width: usize,
) {
    let visible_root = build_visible_tree(root, max_depth, term_height);
    let mut rows: Vec<Row> = Vec::new();
    collect_rows(&visible_root, 0, max_depth, "", &mut rows);
    rows.push(Row {
        node: &visible_root,
        prefix: String::new(),
        connector: if visible_root.children.is_empty() {
            String::new()
        } else {
            "┌─┴ ".to_string()
        },
        is_root: true,
    });

    const LINES_W: usize = 4;   // "1.5K" / " 999"
    const BAR_W: usize = 20;    // the █/░ bar itself
    const PCT_W: usize = 5;     // " 100%"
    const BORDERS: usize = 2;   // the two │ delimiters around the bar
    // Whatever terminal width remains after all fixed-width columns goes to the
    // tree name column; the +1 accounts for the space before the percentage.
    let name_w = term_width.saturating_sub(LINES_W + TREE_GAP.len() + BAR_W + BORDERS + PCT_W + 1);

    let root_lines = root.total_lines;
    let mut out = stdout();

    for row in &rows {
        let name = row.node.label.as_str();
        let full_name = format!("{}{}{}", row.prefix, row.connector, name);
        let gap = tree_gap(row);
        // The root row uses a narrower `gap` (see `tree_gap`), so widen its name
        // column by the same amount; every row then spans an identical width and
        // the bar/percentage columns stay vertically aligned.
        let row_name_w = name_w + TREE_GAP.len().saturating_sub(gap.len());
        let display_name = fit_to_width(&full_name, row_name_w);

        let lines_str = format_lines(row.node.total_lines);
        let segs = build_bar(row.node, root_lines, BAR_W, scheme);
        let pct = if root_lines > 0 {
            (row.node.total_lines as f64 / root_lines as f64 * 100.0).round() as usize
        } else {
            0
        };

        let _ = out.execute(Print(format!("{}{}{}", lines_str, gap, display_name)));
        let _ = out.execute(Print("│"));
        for (width, color) in &segs {
            if let Some(c) = color {
                let _ = out.execute(SetForegroundColor(*c));
                let _ = out.execute(Print("█".repeat(*width)));
            } else {
                let _ = out.execute(SetForegroundColor(Color::DarkGrey));
                let _ = out.execute(Print("░".repeat(*width)));
            }
            let _ = out.execute(ResetColor);
        }
        let filled: usize = segs.iter().map(|(w, _)| w).sum();
        let _ = out.execute(Print(format!(
            "{}│ {:3}%\n",
            " ".repeat(BAR_W.saturating_sub(filled)),
            pct
        )));
    }
    render_legend(&mut out, scheme);
    let _ = out.flush();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_lines_small() {
        assert_eq!(format_lines(0), "   0");
        assert_eq!(format_lines(999), " 999");
        assert_eq!(format_lines(50), "  50");
    }

    #[test]
    fn format_lines_kilo() {
        assert_eq!(format_lines(1_500), "1.5K");
        assert_eq!(format_lines(10_000), " 10K");
        assert_eq!(format_lines(99_000), " 99K");
    }

    #[test]
    fn format_lines_mega() {
        assert_eq!(format_lines(1_200_000), "1.2M");
    }

    #[test]
    fn format_lines_stays_four_columns_at_rounding_boundaries() {
        // Counts whose mantissa rounds up to 10.0 must spill into the next unit
        // instead of producing a 5-character string.
        assert_eq!(format_lines(9_949), "9.9K");
        assert_eq!(format_lines(9_950), " 10K");
        assert_eq!(format_lines(9_999), " 10K");
        assert_eq!(format_lines(999_499), "999K");
        assert_eq!(format_lines(999_500), "1.0M");
        assert_eq!(format_lines(9_949_999), "9.9M");
        assert_eq!(format_lines(9_950_000), " 10M");

        for n in [0, 999, 1_000, 9_949, 9_950, 9_999, 999_499, 999_500, 9_949_999, 9_950_000] {
            assert_eq!(format_lines(n).chars().count(), 4, "n = {n}");
        }
    }

    #[test]
    fn fit_to_width_pads_and_truncates_by_display_width() {
        // ASCII: pad to width, and truncate with an ellipsis when too long.
        assert_eq!(fit_to_width("src", 6), "src   ");
        assert_eq!(fit_to_width("src", 3), "src");
        assert_eq!(fit_to_width("longname", 5), "long…");

        // Wide (CJK) glyphs count as two columns, so two of them fill width 4.
        assert_eq!(fit_to_width("文档", 4).width(), 4);
        // Padding accounts for the wide glyphs rather than the char count.
        assert_eq!(fit_to_width("文档", 6).width(), 6);
        // A wide glyph that would straddle the truncation boundary is dropped,
        // and the leftover column is space-padded so the width stays exact.
        assert_eq!(fit_to_width("文档ab", 4).width(), 4);
    }

    #[test]
    fn bar_fill_proportions() {
        assert_eq!(bar_fill_width(50, 100, 10), 5);
        assert_eq!(bar_fill_width(100, 100, 10), 10);
        assert_eq!(bar_fill_width(0, 100, 10), 0);
        assert_eq!(bar_fill_width(1, 3, 9), 3);
    }

    fn file(name: &str, lines: usize, lang: &'static str) -> TreeNode {
        let mut lang_breakdown = HashMap::new();
        lang_breakdown.insert(lang, lines);
        TreeNode {
            path: std::path::PathBuf::from(format!("/repo/{name}")),
            is_file: true,
            total_lines: lines,
            lang_breakdown,
            children: Vec::new(),
        }
    }

    fn dir(name: &str, children: Vec<TreeNode>) -> TreeNode {
        let mut total_lines = 0;
        let mut lang_breakdown = HashMap::new();
        for child in &children {
            total_lines += child.total_lines;
            for (&lang, &lines) in &child.lang_breakdown {
                *lang_breakdown.entry(lang).or_insert(0) += lines;
            }
        }
        TreeNode {
            path: std::path::PathBuf::from(format!("/repo/{name}")),
            is_file: false,
            total_lines,
            lang_breakdown,
            children,
        }
    }

    #[test]
    fn visible_tree_collapses_to_single_others_row_when_no_budget_remains() {
        let root = dir(
            "repo",
            vec![
                dir("a", vec![file("a1.go", 100, "Go"), file("a2.go", 90, "Go")]),
                dir("b", vec![file("b1.rs", 80, "Rust"), file("b2.rs", 70, "Rust")]),
                dir("c", vec![file("c1.md", 60, "Markdown"), file("c2.md", 50, "Markdown")]),
                dir("d", vec![file("d1.py", 40, "Python"), file("d2.py", 30, "Python")]),
            ],
        );

        let visible = build_visible_tree(&root, None, 8);

        assert_eq!(count_rows(&visible.children), 1);
        assert_eq!(visible.children[0].label, "4 others");
    }

    #[test]
    fn visible_tree_reserves_eight_terminal_rows() {
        let root = dir(
            "repo",
            vec![
                dir("a", vec![file("a1.go", 100, "Go"), file("a2.go", 90, "Go")]),
                dir("b", vec![file("b1.rs", 80, "Rust"), file("b2.rs", 70, "Rust")]),
                dir("c", vec![file("c1.md", 60, "Markdown"), file("c2.md", 50, "Markdown")]),
                dir("d", vec![file("d1.py", 40, "Python"), file("d2.py", 30, "Python")]),
            ],
        );

        let visible = build_visible_tree(&root, None, 11);

        assert_eq!(1 + count_rows(&visible.children), 2);
        assert_eq!(visible.children[0].label, "4 others");
    }

    #[test]
    fn visible_tree_aggregates_hidden_siblings_and_preserves_language_totals() {
        let root = dir(
            "repo",
            vec![
                dir("design", vec![file("spec.md", 300, "Markdown")]),
                dir("internal", vec![file("svc.go", 200, "Go")]),
                dir("cmd", vec![file("main.go", 150, "Go")]),
                dir("scripts", vec![file("run.py", 50, "Python")]),
            ],
        );

        let visible = build_visible_tree(&root, Some(1), 11);

        assert_eq!(visible.children.len(), 2);
        let aggregated = visible.children.last().unwrap();
        assert_eq!(aggregated.label, "3 others");
        assert_eq!(aggregated.total_lines, 400);
        assert_eq!(aggregated.lang_breakdown.get("Go"), Some(&350));
        assert_eq!(aggregated.lang_breakdown.get("Python"), Some(&50));
    }

    #[test]
    fn visible_tree_prioritizes_largest_frontier_nodes_like_dust() {
        let root = dir(
            "repo",
            vec![
                dir(
                    "design",
                    vec![
                        dir("implement", vec![file("plan.md", 300, "Markdown")]),
                        dir("origin", vec![file("notes.md", 200, "Markdown")]),
                    ],
                ),
                dir(
                    "internal",
                    vec![
                        dir("api", vec![file("service.go", 250, "Go")]),
                        dir("storage", vec![file("store.go", 240, "Go")]),
                    ],
                ),
                dir(
                    "webapp",
                    vec![
                        dir("src", vec![file("app.ts", 180, "TypeScript")]),
                        dir("test", vec![file("app.test.ts", 170, "TypeScript")]),
                    ],
                ),
                dir("tiny-a", vec![file("a.txt", 20, "Markdown")]),
                dir("tiny-b", vec![file("b.txt", 10, "Markdown")]),
            ],
        );

        let visible = build_visible_tree(&root, None, 18);

        assert_eq!(visible.children[0].label, "webapp");
        assert_eq!(visible.children[1].label, "internal");
        assert_eq!(visible.children[2].label, "design");
        assert!(visible.children.iter().all(|child| child.label != "tiny-a"));
        assert!(visible.children.iter().all(|child| child.label != "tiny-b"));
    }

    #[test]
    fn legend_includes_others_bucket() {
        let scheme = ColorScheme {
            assignments: vec![("Rust", Color::Blue), ("Python", Color::Green)],
        };

        assert_eq!(
            legend_items(&scheme),
            vec![
                ("Rust", Some(Color::Blue)),
                ("Python", Some(Color::Green)),
                ("Others", None)
            ]
        );
    }

    #[test]
    fn renders_children_before_parent_like_dust() {
        let root = dir(
            "repo",
            vec![dir("src", vec![dir("nested", vec![file("main.rs", 10, "Rust")])])],
        );

        let visible = build_visible_tree(&root, None, 20);

        assert_eq!(
            row_labels(&visible, None),
            vec![
                "    ┌── main.rs".to_string(),
                "  ┌─┴ nested".to_string(),
                "┌─┴ src".to_string(),
                "┌─┴ repo".to_string(),
            ]
        );
    }

    #[test]
    fn inverted_tree_keeps_vertical_guides_consistent() {
        let root = dir(
            "repo",
            vec![
                dir("alpha", vec![file("a.rs", 10, "Rust")]),
                dir("beta", vec![file("b.rs", 20, "Rust")]),
            ],
        );

        let visible = build_visible_tree(&root, None, 20);

        assert_eq!(
            row_labels(&visible, None),
            vec![
                "  ┌── a.rs".to_string(),
                "┌─┴ alpha".to_string(),
                "│ ┌── b.rs".to_string(),
                "├─┴ beta".to_string(),
                "┌─┴ repo".to_string(),
            ]
        );
    }

    #[test]
    fn inverted_tree_preserves_ancestor_guides_for_mixed_depth_siblings() {
        let root = dir(
            "repo",
            vec![
                dir("others", vec![file("misc.txt", 5, "Markdown")]),
                dir(
                    "design",
                    vec![
                        dir("origin", vec![file("o1.md", 10, "Markdown")]),
                        dir(
                            "implement",
                            vec![
                                dir("spec", vec![file("s1.md", 10, "Markdown")]),
                                dir("plan", vec![file("p1.md", 10, "Markdown")]),
                            ],
                        ),
                    ],
                ),
            ],
        );

        let visible = build_visible_tree(&root, None, 30);

        assert_eq!(
            row_labels(&visible, None),
            vec![
                "  ┌── misc.txt".to_string(),
                "┌─┴ others".to_string(),
                "│   ┌── o1.md".to_string(),
                "│ ┌─┴ origin".to_string(),
                "│ │   ┌── s1.md".to_string(),
                "│ │ ┌─┴ spec".to_string(),
                "│ │ │ ┌── p1.md".to_string(),
                "│ │ ├─┴ plan".to_string(),
                "│ ├─┴ implement".to_string(),
                "├─┴ design".to_string(),
                "┌─┴ repo".to_string(),
            ]
        );
    }
}
