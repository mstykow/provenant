use std::path::Path;

use content_inspector::{ContentType, inspect};
use tokei::LanguageType;

fn is_utf8_text(content_type: ContentType) -> bool {
    content_type == ContentType::UTF_8 || content_type == ContentType::UTF_8_BOM
}

pub fn detect_language(path: &Path, content: &[u8]) -> Option<String> {
    let inspected = inspect(content);

    if let Some(language) = detect_shebang_language(content) {
        return Some(language);
    }

    if let Some(language) = detect_special_file_name_language(path) {
        return Some(language);
    }

    if let Some(language) = detect_tokei_extension_language(path) {
        return Some(language);
    }

    if let Some(language) = detect_manual_extension_language(path) {
        return Some(language);
    }

    if is_utf8_text(inspected) {
        let text_sample = String::from_utf8_lossy(&content[..std::cmp::min(content.len(), 1000)]);

        if text_sample.contains("<?php") {
            return Some("PHP".to_string());
        } else if text_sample.contains("<html") || text_sample.contains("<!DOCTYPE html") {
            return Some("HTML".to_string());
        } else if text_sample.contains("plugins {")
            || (text_sample.contains("dependencies {") && text_sample.contains("repositories {"))
        {
            return Some("Groovy".to_string());
        } else if text_sample.contains("import React") || text_sample.contains("import {") {
            return Some("JavaScript/TypeScript".to_string());
        } else if text_sample.contains("def ") && text_sample.contains(':') {
            return Some("Python".to_string());
        } else if text_sample.contains("package ")
            && text_sample.contains("import ")
            && text_sample.contains('{')
        {
            return Some("Go".to_string());
        }
    }

    None
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

fn detect_special_file_name_language(path: &Path) -> Option<String> {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();

    if matches!(
        file_name.as_str(),
        "dockerfile" | "containerfile" | "containerfile.core"
    ) {
        Some("Dockerfile".to_string())
    } else if matches!(file_name.as_str(), "makefile" | "makefile.inc") {
        Some("Makefile".to_string())
    } else if matches!(
        file_name.as_str(),
        "gemfile" | "rakefile" | "podfile" | "vagrantfile" | "brewfile"
    ) {
        Some("Ruby".to_string())
    } else if matches!(file_name.as_str(), "apkbuild" | "pkgbuild" | "gradlew") {
        Some("Shell".to_string())
    } else if matches!(file_name.as_str(), "meson.build") {
        Some("Meson".to_string())
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

fn detect_tokei_extension_language(path: &Path) -> Option<String> {
    let extension = path.extension()?.to_str()?.to_ascii_lowercase();
    let language = LanguageType::from_file_extension(&extension)?;
    map_tokei_language(language).map(str::to_string)
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

fn map_tokei_language(language: LanguageType) -> Option<&'static str> {
    match language {
        LanguageType::Rust => Some("Rust"),
        LanguageType::Python => Some("Python"),
        LanguageType::JavaScript | LanguageType::Jsx => Some("JavaScript"),
        LanguageType::TypeScript | LanguageType::Tsx => Some("TypeScript"),
        LanguageType::Html => Some("HTML"),
        LanguageType::Css => Some("CSS"),
        LanguageType::C | LanguageType::CHeader => Some("C"),
        LanguageType::Cpp | LanguageType::CppHeader | LanguageType::CppModule => Some("C++"),
        LanguageType::AssemblyGAS => Some("GAS"),
        LanguageType::Java => Some("Java"),
        LanguageType::Go => Some("Go"),
        LanguageType::Ruby | LanguageType::Rakefile => Some("Ruby"),
        LanguageType::Php => Some("PHP"),
        LanguageType::Perl => Some("Perl"),
        LanguageType::Swift => Some("Swift"),
        LanguageType::Bash
        | LanguageType::CShell
        | LanguageType::Fish
        | LanguageType::Ksh
        | LanguageType::Sh
        | LanguageType::Zsh => Some("Shell"),
        LanguageType::Kotlin => Some("Kotlin"),
        LanguageType::Dart => Some("Dart"),
        LanguageType::Scala => Some("Scala"),
        LanguageType::CSharp => Some("C#"),
        LanguageType::FSharp => Some("F#"),
        LanguageType::R => Some("R"),
        LanguageType::Lua => Some("Lua"),
        LanguageType::Julia => Some("Julia"),
        LanguageType::Elixir => Some("Elixir"),
        LanguageType::Clojure | LanguageType::ClojureC => Some("Clojure"),
        LanguageType::Haskell => Some("Haskell"),
        LanguageType::Erlang => Some("Erlang"),
        LanguageType::Sql => Some("SQL"),
        LanguageType::Tex => Some("TeX"),
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
}
