use content_inspector::{ContentType, inspect};
use std::path::Path;

fn is_utf8_text(content_type: ContentType) -> bool {
    content_type == ContentType::UTF_8 || content_type == ContentType::UTF_8_BOM
}

pub fn detect_language(path: &Path, content: &[u8]) -> Option<String> {
    if content.len() > 32 && !is_utf8_text(inspect(content)) {
        return None;
    }

    // Check for shebang in script files
    if content.len() > 2 && content[0] == b'#' && content[1] == b'!' {
        let shebang_end = content
            .iter()
            .position(|&b| b == b'\n')
            .unwrap_or(content.len());
        let shebang = String::from_utf8_lossy(&content[0..shebang_end]);

        if shebang.contains("python") {
            return Some("Python".to_string());
        } else if shebang.contains("node") {
            return Some("JavaScript".to_string());
        } else if shebang.contains("ruby") {
            return Some("Ruby".to_string());
        } else if shebang.contains("perl") {
            return Some("Perl".to_string());
        } else if shebang.contains("php") {
            return Some("PHP".to_string());
        } else if shebang.contains("bash") || shebang.contains("sh") {
            return Some("Shell".to_string());
        }
    }

    // Check file extension
    if let Some(extension) = path.extension().and_then(|e| e.to_str()) {
        match extension.to_lowercase().as_str() {
            "rs" => return Some("Rust".to_string()),
            "py" => return Some("Python".to_string()),
            "js" => return Some("JavaScript".to_string()),
            "ts" | "tsx" => return Some("TypeScript".to_string()),
            "jsx" => return Some("JavaScript".to_string()),
            "html" | "htm" => return Some("HTML".to_string()),
            "css" => return Some("CSS".to_string()),
            "c" => return Some("C".to_string()),
            "cpp" | "cc" | "cxx" => return Some("C++".to_string()),
            "h" => return Some("C".to_string()),
            "hpp" => return Some("C++".to_string()),
            "s" => return Some("GAS".to_string()),
            "java" => return Some("Java".to_string()),
            "go" => return Some("Go".to_string()),
            "rb" => return Some("Ruby".to_string()),
            "php" => return Some("PHP".to_string()),
            "pl" => return Some("Perl".to_string()),
            "swift" => return Some("Swift".to_string()),
            "json" => return Some("JSON".to_string()),
            "xml" => return Some("XML".to_string()),
            "yml" | "yaml" => return Some("YAML".to_string()),
            "sql" => return Some("SQL".to_string()),
            "sh" | "bash" | "zsh" | "fish" => return Some("Shell".to_string()),
            "kt" | "kts" => return Some("Kotlin".to_string()),
            "dart" => return Some("Dart".to_string()),
            "scala" => return Some("Scala".to_string()),
            "cs" => return Some("C#".to_string()),
            "fs" => return Some("F#".to_string()),
            "r" => return Some("R".to_string()),
            "lua" => return Some("Lua".to_string()),
            "jl" => return Some("Julia".to_string()),
            "ex" | "exs" => return Some("Elixir".to_string()),
            "clj" => return Some("Clojure".to_string()),
            "hs" => return Some("Haskell".to_string()),
            "erl" => return Some("Erlang".to_string()),
            "sc" => return Some("SuperCollider".to_string()),
            "tex" => return Some("TeX".to_string()),
            _ => {}
        }
    }

    // Check file name for special cases
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_lowercase())
        .unwrap_or_default();

    if matches!(
        file_name.as_str(),
        "dockerfile" | "containerfile" | "containerfile.core"
    ) {
        return Some("Dockerfile".to_string());
    } else if file_name == "makefile" {
        return Some("Makefile".to_string());
    } else if file_name == "gemfile" || file_name == "rakefile" {
        return Some("Ruby".to_string());
    }

    if is_utf8_text(inspect(content)) {
        let text_sample = String::from_utf8_lossy(&content[..std::cmp::min(content.len(), 1000)]);

        if text_sample.contains("<?php") {
            return Some("PHP".to_string());
        } else if text_sample.contains("<html") || text_sample.contains("<!DOCTYPE html") {
            return Some("HTML".to_string());
        } else if text_sample.contains("import React") || text_sample.contains("import {") {
            return Some("JavaScript/TypeScript".to_string());
        } else if text_sample.contains("def ") && text_sample.contains(":") {
            return Some("Python".to_string());
        } else if text_sample.contains("package ")
            && text_sample.contains("import ")
            && text_sample.contains("{")
        {
            return Some("Go".to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::detect_language;
    use std::path::Path;

    #[test]
    fn detect_language_supports_containerfile_names() {
        assert_eq!(
            detect_language(Path::new("Containerfile"), b"FROM scratch\n"),
            Some("Dockerfile".to_string())
        );
        assert_eq!(
            detect_language(Path::new("containerfile.core"), b"FROM scratch\n"),
            Some("Dockerfile".to_string())
        );
    }

    #[test]
    fn detect_language_maps_c_headers_to_c() {
        assert_eq!(
            detect_language(Path::new("zlib.h"), b"/* header */\n"),
            Some("C".to_string())
        );
    }

    #[test]
    fn detect_language_maps_uppercase_s_to_gas() {
        assert_eq!(
            detect_language(Path::new("gvmat64.S"), b"; asm\n"),
            Some("GAS".to_string())
        );
    }

    #[test]
    fn detect_language_omits_generic_text_fallbacks() {
        assert_eq!(
            detect_language(Path::new("README.txt"), b"plain text\n"),
            None
        );
        assert_eq!(
            detect_language(Path::new("data.bin"), &[0, 159, 146, 150]),
            None
        );
    }
}
