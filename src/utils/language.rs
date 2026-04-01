use std::collections::HashSet;
use std::path::Path;

use file_identify::tags_from_filename;

pub fn detect_language(path: &Path, content: &[u8]) -> Option<String> {
    if let Some(language) = detect_shebang_language(content) {
        return Some(language);
    }

    if let Some(language) = detect_file_identify_language(path) {
        return Some(language);
    }

    if let Some(language) = detect_repo_special_file_name_language(path) {
        return Some(language);
    }

    if let Some(language) = detect_manual_extension_language(path) {
        return Some(language);
    }

    detect_content_hint_language(content)
}

fn detect_content_hint_language(content: &[u8]) -> Option<String> {
    let sample_end = std::cmp::min(content.len(), 1000);
    let text_sample = std::str::from_utf8(&content[..sample_end]).ok()?;

    if text_sample.contains("<?php") {
        Some("PHP".to_string())
    } else if text_sample.contains("<html") || text_sample.contains("<!DOCTYPE html") {
        Some("HTML".to_string())
    } else if text_sample.contains("plugins {")
        || (text_sample.contains("dependencies {") && text_sample.contains("repositories {"))
    {
        Some("Groovy".to_string())
    } else if text_sample.contains("import React") || text_sample.contains("import {") {
        Some("JavaScript/TypeScript".to_string())
    } else if text_sample.contains("def ") && text_sample.contains(':') {
        Some("Python".to_string())
    } else if text_sample.contains("package ")
        && text_sample.contains("import ")
        && text_sample.contains('{')
    {
        Some("Go".to_string())
    } else {
        None
    }
}

fn detect_shebang_language(content: &[u8]) -> Option<String> {
    if content.len() <= 2 || content[0] != b'#' || content[1] != b'!' {
        return None;
    }

    let shebang_end = content
        .iter()
        .position(|&b| b == b'\n')
        .unwrap_or(content.len());
    let shebang = String::from_utf8_lossy(&content[0..shebang_end]).to_ascii_lowercase();

    if shebang.contains("python") {
        Some("Python".to_string())
    } else if shebang.contains("node") || shebang.contains("deno") || shebang.contains("bun") {
        Some("JavaScript".to_string())
    } else if shebang.contains("ruby") {
        Some("Ruby".to_string())
    } else if shebang.contains("perl") {
        Some("Perl".to_string())
    } else if shebang.contains("php") {
        Some("PHP".to_string())
    } else if shebang.contains("pwsh") || shebang.contains("powershell") {
        Some("PowerShell".to_string())
    } else if shebang.contains("awk") {
        Some("Awk".to_string())
    } else if shebang.contains("bash")
        || shebang.contains("zsh")
        || shebang.contains("fish")
        || shebang.contains("ksh")
        || shebang.contains("/sh")
    {
        Some("Shell".to_string())
    } else {
        None
    }
}

fn detect_file_identify_language(path: &Path) -> Option<String> {
    let file_name = path.file_name()?.to_str()?;
    let tags = tags_from_filename(file_name);

    map_file_identify_tags(&tags).map(str::to_string)
}

fn map_file_identify_tags(tags: &HashSet<&'static str>) -> Option<&'static str> {
    if tags.contains("dockerfile") {
        return Some("Dockerfile");
    }
    if tags.contains("makefile") {
        return Some("Makefile");
    }
    if tags.contains("rust") {
        return Some("Rust");
    }
    if tags.contains("python") {
        return Some("Python");
    }
    if tags.contains("javascript") || tags.contains("jsx") {
        return Some("JavaScript");
    }
    if tags.contains("ts") || tags.contains("tsx") {
        return Some("TypeScript");
    }
    if tags.contains("html") {
        return Some("HTML");
    }
    if tags.contains("css") {
        return Some("CSS");
    }
    if tags.contains("c") {
        return Some("C");
    }
    if tags.contains("cpp") {
        return Some("C++");
    }
    if tags.contains("java") {
        return Some("Java");
    }
    if tags.contains("go") {
        return Some("Go");
    }
    if tags.contains("ruby") {
        return Some("Ruby");
    }
    if tags.contains("php") {
        return Some("PHP");
    }
    if tags.contains("perl") {
        return Some("Perl");
    }
    if tags.contains("swift") {
        return Some("Swift");
    }
    if tags.contains("shell") || tags.contains("bash") || tags.contains("zsh") {
        return Some("Shell");
    }
    if tags.contains("kotlin") {
        return Some("Kotlin");
    }
    if tags.contains("dart") {
        return Some("Dart");
    }
    if tags.contains("scala") {
        return Some("Scala");
    }
    if tags.contains("csharp") {
        return Some("C#");
    }
    if tags.contains("fsharp") {
        return Some("F#");
    }
    if tags.contains("r") {
        return Some("R");
    }
    if tags.contains("lua") {
        return Some("Lua");
    }
    if tags.contains("julia") {
        return Some("Julia");
    }
    if tags.contains("elixir") {
        return Some("Elixir");
    }
    if tags.contains("clojure") {
        return Some("Clojure");
    }
    if tags.contains("haskell") {
        return Some("Haskell");
    }
    if tags.contains("erlang") {
        return Some("Erlang");
    }
    if tags.contains("sql") {
        return Some("SQL");
    }
    if tags.contains("tex") {
        return Some("TeX");
    }
    if tags.contains("groovy") || tags.contains("gradle") {
        return Some("Groovy");
    }
    if tags.contains("nix") {
        return Some("Nix");
    }
    if tags.contains("zig") {
        return Some("Zig");
    }
    if tags.contains("powershell") {
        return Some("PowerShell");
    }
    if tags.contains("starlark") {
        return Some("Starlark");
    }
    if tags.contains("awk") {
        return Some("Awk");
    }
    if tags.contains("ocaml") {
        return Some("OCaml");
    }
    if tags.contains("meson") {
        return Some("Meson");
    }

    None
}

fn detect_repo_special_file_name_language(path: &Path) -> Option<String> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    if matches!(
        file_name.as_str(),
        "gemfile" | "rakefile" | "podfile" | "vagrantfile" | "brewfile"
    ) {
        Some("Ruby".to_string())
    } else if matches!(file_name.as_str(), "apkbuild" | "pkgbuild" | "gradlew") {
        Some("Shell".to_string())
    } else if matches!(file_name.as_str(), "meson.build") {
        Some("Meson".to_string())
    } else if matches!(file_name.as_str(), "containerfile.core") {
        Some("Dockerfile".to_string())
    } else if matches!(file_name.as_str(), "build" | "workspace" | "buck") {
        Some("Starlark".to_string())
    } else if matches!(
        file_name.as_str(),
        "default.nix" | "flake.nix" | "shell.nix"
    ) {
        Some("Nix".to_string())
    } else {
        None
    }
}

fn detect_manual_extension_language(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();

    match extension.as_str() {
        "rs" => Some("Rust".to_string()),
        "py" => Some("Python".to_string()),
        "js" | "mjs" | "cjs" => Some("JavaScript".to_string()),
        "ts" | "tsx" | "mts" | "cts" => Some("TypeScript".to_string()),
        "jsx" => Some("JavaScript".to_string()),
        "html" | "htm" => Some("HTML".to_string()),
        "css" => Some("CSS".to_string()),
        "c" => Some("C".to_string()),
        "cpp" | "cc" | "cxx" | "hh" | "hxx" => Some("C++".to_string()),
        "h" => Some("C".to_string()),
        "hpp" => Some("C++".to_string()),
        "m" => Some("Objective-C".to_string()),
        "mm" => Some("Objective-C++".to_string()),
        "s" | "asm" => Some("GAS".to_string()),
        "java" => Some("Java".to_string()),
        "go" => Some("Go".to_string()),
        "rb" => Some("Ruby".to_string()),
        "php" => Some("PHP".to_string()),
        "pl" => Some("Perl".to_string()),
        "swift" => Some("Swift".to_string()),
        "sql" => Some("SQL".to_string()),
        "sh" | "bash" | "zsh" | "fish" | "ksh" => Some("Shell".to_string()),
        "kt" | "kts" => Some("Kotlin".to_string()),
        "dart" => Some("Dart".to_string()),
        "scala" => Some("Scala".to_string()),
        "cs" => Some("C#".to_string()),
        "fs" | "fsx" => Some("F#".to_string()),
        "r" => Some("R".to_string()),
        "lua" => Some("Lua".to_string()),
        "jl" => Some("Julia".to_string()),
        "ex" | "exs" => Some("Elixir".to_string()),
        "clj" | "cljs" | "cljc" => Some("Clojure".to_string()),
        "hs" => Some("Haskell".to_string()),
        "erl" => Some("Erlang".to_string()),
        "tex" => Some("TeX".to_string()),
        "groovy" | "gradle" | "gvy" | "gy" | "gsh" => Some("Groovy".to_string()),
        "nix" => Some("Nix".to_string()),
        "zig" => Some("Zig".to_string()),
        "ps1" | "psm1" | "psd1" => Some("PowerShell".to_string()),
        "bzl" | "bazel" | "star" | "sky" => Some("Starlark".to_string()),
        "awk" => Some("Awk".to_string()),
        "ml" | "mli" => Some("OCaml".to_string()),
        _ => None,
    }
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
    fn detect_language_handles_manifest_dsl_filenames() {
        assert_eq!(
            detect_language(Path::new("APKBUILD"), b"pkgname=demo\n"),
            Some("Shell".to_string())
        );
        assert_eq!(
            detect_language(Path::new("Podfile"), b"source 'https://rubygems.org'\n"),
            Some("Ruby".to_string())
        );
        assert_eq!(
            detect_language(Path::new("meson.build"), b"project('demo')\n"),
            Some("Meson".to_string())
        );
        assert_eq!(
            detect_language(Path::new("BUILD"), b"cc_library(name = 'demo')\n"),
            Some("Starlark".to_string())
        );
        assert_eq!(
            detect_language(Path::new("flake.nix"), b"{ inputs, ... }: {}\n"),
            Some("Nix".to_string())
        );
    }

    #[test]
    fn detect_language_handles_common_build_extensions() {
        assert_eq!(
            detect_language(Path::new("build.gradle"), b"plugins { id 'java' }\n"),
            Some("Groovy".to_string())
        );
        assert_eq!(
            detect_language(Path::new("main.nix"), b"{ pkgs }: pkgs.hello\n"),
            Some("Nix".to_string())
        );
        assert_eq!(
            detect_language(Path::new("rules.bzl"), b"def _impl(ctx):\n    pass\n"),
            Some("Starlark".to_string())
        );
        assert_eq!(
            detect_language(Path::new("script.ps1"), b"Write-Host 'hello'\n"),
            Some("PowerShell".to_string())
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

    #[test]
    fn detect_language_ignores_yaml_as_programming_language() {
        assert_eq!(
            detect_language(Path::new("config.yaml"), b"key: value\n"),
            None
        );
    }

    #[test]
    fn detect_language_keeps_extension_detection_for_non_utf8_python() {
        let latin1_python = b"# coding: latin-1\nprint(\"caf\xe9\")\n# comment padding\n";

        assert_eq!(
            detect_language(Path::new("script.py"), latin1_python),
            Some("Python".to_string())
        );
    }

    #[test]
    fn detect_language_uses_utf8_content_hints_for_extensionless_files() {
        assert_eq!(
            detect_language(
                Path::new("index"),
                b"<!DOCTYPE html><html><body></body></html>"
            ),
            Some("HTML".to_string())
        );
    }

    #[test]
    fn detect_language_does_not_use_content_hints_for_invalid_utf8() {
        assert_eq!(
            detect_language(
                Path::new("index"),
                &[0xff, b'<', b'h', b't', b'm', b'l', b'>']
            ),
            None
        );
    }
}
