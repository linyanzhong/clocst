pub fn extension_to_language(ext: &str) -> Option<&'static str> {
    match ext {
        "rs" => Some("Rust"),
        "py" | "pyi" => Some("Python"),
        "js" | "jsx" | "mjs" | "cjs" => Some("JavaScript"),
        "ts" | "tsx" | "mts" | "cts" => Some("TypeScript"),
        "go" => Some("Go"),
        "c" | "h" => Some("C"),
        "cpp" | "cc" | "cxx" | "hpp" | "hxx" => Some("C++"),
        "java" => Some("Java"),
        "rb" => Some("Ruby"),
        "sh" | "bash" | "zsh" | "fish" => Some("Shell"),
        "md" | "markdown" => Some("Markdown"),
        "html" | "htm" => Some("HTML"),
        "css" | "scss" | "sass" | "less" => Some("CSS"),
        "json" | "jsonc" => Some("JSON"),
        "yaml" | "yml" => Some("YAML"),
        "toml" => Some("TOML"),
        "sql" => Some("SQL"),
        "kt" | "kts" => Some("Kotlin"),
        "swift" => Some("Swift"),
        "php" => Some("PHP"),
        "lua" => Some("Lua"),
        "r" | "R" => Some("R"),
        "scala" | "sc" => Some("Scala"),
        "cs" => Some("C#"),
        "ex" | "exs" => Some("Elixir"),
        "erl" | "hrl" => Some("Erlang"),
        "hs" => Some("Haskell"),
        "ml" | "mli" => Some("OCaml"),
        "clj" | "cljs" => Some("Clojure"),
        "dart" => Some("Dart"),
        "zig" => Some("Zig"),
        "vue" => Some("Vue"),
        "svelte" => Some("Svelte"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_known_extensions() {
        assert_eq!(extension_to_language("rs"), Some("Rust"));
        assert_eq!(extension_to_language("py"), Some("Python"));
        assert_eq!(extension_to_language("ts"), Some("TypeScript"));
        assert_eq!(extension_to_language("js"), Some("JavaScript"));
        assert_eq!(extension_to_language("go"), Some("Go"));
        assert_eq!(extension_to_language("md"), Some("Markdown"));
    }

    #[test]
    fn returns_none_for_unknown() {
        assert_eq!(extension_to_language("xyz"), None);
        assert_eq!(extension_to_language(""), None);
        assert_eq!(extension_to_language("pdf"), None);
    }
}
