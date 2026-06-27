# AGENTS.md

## Project Overview

clocst is a code line counting tool that combines cloc's per-language statistics with dust's tree-style visualization, outputting a directory tree with colored progress bars.

## Architecture

Three-stage pipeline:

```
Scanner (parallel file scan) → TreeBuilder (aggregate line counts) → Renderer (terminal output)
```

| File | Responsibility |
|------|----------------|
| `src/main.rs` | CLI argument parsing (clap), drives the three-stage pipeline |
| `src/languages.rs` | `match`-based mapping from file extension → language name |
| `src/scanner.rs` | Parallel directory traversal using `rayon` + `ignore`, counts lines per file |
| `src/tree.rs` | Builds directory tree, aggregates language line counts, assigns colors |
| `src/renderer.rs` | Formatted output: color bars, percentages, terminal-width adaptation, auto-pruning |

## Build & Test

```bash
cargo build --release
cargo test
```

Unit tests are co-located with each module (`#[cfg(test)]`); integration tests are in `tests/integration.rs`.

## Common Modification Points

**Adding a new language**: Add a new arm to the `match` expression in `extension_to_language` in `src/languages.rs`, e.g. `"ext" => Some("Language Name")`.

**Changing output format**: The `collect_rows` function in `src/renderer.rs` controls tree traversal and row ordering; `build_bar` controls the colored bar segments; `format_lines` controls the line-count column format.
