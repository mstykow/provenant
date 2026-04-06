use std::collections::BTreeSet;
use std::fs;
use std::io::{BufReader, Cursor, Read};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use chrono::{TimeZone, Utc};
use file_format::{FileFormat, Kind as FileFormatKind};
use flate2::read::ZlibDecoder;
use glob::Pattern;
use image::{ImageDecoder, ImageFormat, ImageReader};
use mime_guess::from_path;
use quick_xml::events::Event;
use quick_xml::reader::Reader as XmlReader;

use crate::utils::language::detect_language;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractedTextKind {
    None,
    Decoded,
    Pdf,
    BinaryStrings,
    ImageMetadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileInfoClassification {
    pub mime_type: String,
    pub file_type: String,
    pub programming_language: Option<String>,
    pub is_binary: bool,
    pub is_text: bool,
    pub is_archive: bool,
    pub is_media: bool,
    pub is_source: bool,
    pub is_script: bool,
}

const MAX_IMAGE_METADATA_VALUES: usize = 64;
const MAX_IMAGE_METADATA_TEXT_BYTES: usize = 32 * 1024;
const BINARY_CONTROL_CHAR_THRESHOLD_DIVISOR: usize = 10;
const PLAIN_TEXT_EXTENSIONS: &[&str] = &[
    "rst", "rest", "md", "txt", "log", "json", "xml", "yaml", "yml", "toml", "ini",
];
const BINARY_EXTENSIONS: &[&str] = &[
    "pyc", "pyo", "pgm", "pbm", "ppm", "mp3", "mp4", "mpeg", "mpg", "emf",
];
const ARCHIVE_EXTENSIONS: &[&str] = &[
    "zip", "jar", "war", "ear", "tar", "gz", "tgz", "bz2", "xz", "7z", "rar", "apk", "deb", "rpm",
    "whl", "crate", "egg", "gem", "nupkg", "sqs", "squashfs",
];

/// Get the last modified date of a file as a `YYYY-MM-DD` string.
pub fn get_creation_date(metadata: &fs::Metadata) -> Option<String> {
    metadata.modified().ok().map(|time: std::time::SystemTime| {
        let seconds_since_epoch = time
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        Utc.timestamp_opt(seconds_since_epoch, 0)
            .single()
            .unwrap_or_else(Utc::now)
            .format("%Y-%m-%d")
            .to_string()
    })
}

/// Check if a path should be excluded based on a list of glob patterns.
pub fn is_path_excluded(path: &Path, exclude_patterns: &[Pattern]) -> bool {
    let path_str = path.to_string_lossy();
    let file_name = path
        .file_name()
        .map(|name| name.to_string_lossy())
        .unwrap_or_default();

    for pattern in exclude_patterns {
        // Match against full path
        if pattern.matches(&path_str) {
            return true;
        }

        // Match against just the file/directory name
        if pattern.matches(&file_name) {
            return true;
        }
    }

    false
}

/// Decode a byte buffer to a String, trying UTF-8 first, then Latin-1.
///
/// Latin-1 (ISO-8859-1) maps bytes 0x00-0xFF directly to Unicode U+0000-U+00FF,
/// so it can decode any byte sequence. This matches Python ScanCode's use of
/// `UnicodeDammit` which auto-detects encoding with Latin-1 as fallback.
pub fn decode_bytes_to_string(bytes: &[u8]) -> String {
    match String::from_utf8(bytes.to_vec()) {
        Ok(s) => s,
        Err(e) => {
            let bytes = e.into_bytes();
            if has_binary_control_chars(&bytes) {
                return String::new();
            }
            bytes.iter().map(|&b| b as char).collect()
        }
    }
}

pub fn extract_text_for_detection(path: &Path, bytes: &[u8]) -> (String, ExtractedTextKind) {
    let (text, kind, _) = extract_text_for_detection_with_diagnostics(path, bytes);
    (text, kind)
}

pub(crate) fn extract_text_for_detection_with_diagnostics(
    path: &Path,
    bytes: &[u8],
) -> (String, ExtractedTextKind, Option<String>) {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());
    let detected_format = detect_file_format(bytes);

    if looks_like_rtf(bytes, ext.as_deref()) {
        let text = extract_rtf_text(bytes);
        return if text.trim().is_empty() {
            (String::new(), ExtractedTextKind::None, None)
        } else {
            (text, ExtractedTextKind::Decoded, None)
        };
    }

    if looks_like_pdf(bytes) || detected_format.short_name() == Some("PDF") {
        let (text, scan_error) = extract_pdf_text(path, bytes);
        return if text.is_empty() {
            (String::new(), ExtractedTextKind::None, scan_error)
        } else {
            (text, ExtractedTextKind::Pdf, None)
        };
    }

    if let Some(format) = supported_image_metadata_format(ext.as_deref(), detected_format) {
        let text = extract_image_metadata_text(bytes, format);
        return if text.is_empty() {
            if is_supported_image_container(bytes, format) {
                (String::new(), ExtractedTextKind::None, None)
            } else {
                let decoded = decode_bytes_to_string(bytes);
                if decoded.is_empty() {
                    (String::new(), ExtractedTextKind::None, None)
                } else {
                    (decoded, ExtractedTextKind::Decoded, None)
                }
            }
        } else {
            (text, ExtractedTextKind::ImageMetadata, None)
        };
    }

    if should_skip_binary_string_extraction(path, bytes, detected_format) {
        return (String::new(), ExtractedTextKind::None, None);
    }

    let decoded = decode_bytes_to_string(bytes);
    if !decoded.is_empty() {
        return (decoded, ExtractedTextKind::Decoded, None);
    }

    let text = extract_printable_strings(bytes);
    if text.is_empty() {
        (String::new(), ExtractedTextKind::None, None)
    } else {
        (text, ExtractedTextKind::BinaryStrings, None)
    }
}

pub fn classify_file_info(path: &Path, bytes: &[u8]) -> FileInfoClassification {
    let detected_format = detect_file_format(bytes);
    let detected_language = detect_language(path, bytes);
    let is_binary = detect_is_binary(path, bytes, detected_format, detected_language.as_deref());
    let is_text = !is_binary;
    let mime_type = detect_mime_type(path, bytes, detected_format, detected_language.as_deref());
    let is_archive = detect_is_archive(path, bytes, &mime_type, is_text, detected_format);
    let is_media = detect_is_media(path, bytes, &mime_type, detected_format);
    let is_script = detect_is_script(path, bytes, detected_language.as_deref(), is_text);
    let is_source = detect_is_source(path, detected_language.as_deref(), is_text, is_script);
    let programming_language = is_source.then(|| detected_language.clone()).flatten();
    let file_type = detect_file_type(
        path,
        bytes,
        detected_format,
        &mime_type,
        programming_language.as_deref(),
        is_binary,
        is_text,
        is_archive,
        is_media,
        is_script,
    );

    FileInfoClassification {
        mime_type,
        file_type,
        programming_language,
        is_binary,
        is_text,
        is_archive,
        is_media,
        is_source,
        is_script,
    }
}

fn detect_file_format(bytes: &[u8]) -> FileFormat {
    FileFormat::from_reader(Cursor::new(bytes)).unwrap_or(FileFormat::ArbitraryBinaryData)
}

fn is_utf8_text(bytes: &[u8]) -> bool {
    std::str::from_utf8(bytes).is_ok()
}

fn has_binary_control_chars(bytes: &[u8]) -> bool {
    let control_count = bytes
        .iter()
        .filter(|&&b| b < 0x09 || (b > 0x0D && b < 0x20))
        .count();
    control_count > bytes.len() / BINARY_CONTROL_CHAR_THRESHOLD_DIVISOR
}

fn has_decodable_text(bytes: &[u8]) -> bool {
    bytes.is_empty() || is_utf8_text(bytes) || !has_binary_control_chars(bytes)
}

fn looks_like_textual_bytes(bytes: &[u8]) -> bool {
    if bytes.is_empty() || is_utf8_text(bytes) {
        return true;
    }

    let printable_count = bytes
        .iter()
        .filter(|&&b| matches!(b, b'\n' | b'\r' | b'\t') || (0x20..=0x7e).contains(&b))
        .count();
    printable_count * 2 >= bytes.len()
}

fn is_textual_media_type(media_type: &str) -> bool {
    media_type.starts_with("text/")
        || matches!(
            media_type,
            "application/json" | "application/xml" | "text/xml"
        )
        || media_type.ends_with("+json")
        || media_type.ends_with("+xml")
}

fn is_textual_format(detected_format: FileFormat) -> bool {
    matches!(detected_format, FileFormat::Empty | FileFormat::PlainText)
        || is_textual_media_type(detected_format.media_type())
}

fn is_known_binary_format(detected_format: FileFormat) -> bool {
    !matches!(detected_format, FileFormat::ArbitraryBinaryData)
        && !is_textual_format(detected_format)
}

pub fn detect_mime_type(
    path: &Path,
    bytes: &[u8],
    detected_format: FileFormat,
    programming_language: Option<&str>,
) -> String {
    if bytes.is_empty() {
        return "inode/x-empty".to_string();
    }

    if is_zip_archive(bytes) {
        return detect_zip_like_mime(path);
    }

    if looks_like_deb(bytes, path) {
        return "application/vnd.debian.binary-package".to_string();
    }

    if looks_like_rpm(bytes, path) {
        return "application/x-rpm".to_string();
    }

    let guessed_mime = from_path(path)
        .first_or_octet_stream()
        .essence_str()
        .to_string();

    let mime_type = match detected_format {
        FileFormat::Empty => "inode/x-empty".to_string(),
        FileFormat::PlainText => {
            if guessed_mime == "application/octet-stream" || guessed_mime.starts_with("video/") {
                "text/plain".to_string()
            } else {
                guessed_mime.clone()
            }
        }
        _ => {
            let detected_mime = detected_format.media_type();
            if detected_mime == "application/octet-stream"
                && guessed_mime != "application/octet-stream"
            {
                guessed_mime.clone()
            } else {
                detected_mime.to_string()
            }
        }
    };

    normalize_mime_type(path, bytes, programming_language, &mime_type)
}

fn normalize_mime_type(
    path: &Path,
    bytes: &[u8],
    programming_language: Option<&str>,
    mime_type: &str,
) -> String {
    if should_prefer_text_mime(path, bytes, programming_language, mime_type) {
        return "text/plain".to_string();
    }

    mime_type.to_string()
}

fn should_prefer_text_mime(
    path: &Path,
    bytes: &[u8],
    programming_language: Option<&str>,
    mime_type: &str,
) -> bool {
    has_decodable_text(bytes)
        && looks_like_textual_bytes(bytes)
        && is_textual_source_candidate(path, programming_language)
        && (mime_type.starts_with("video/") || mime_type == "application/octet-stream")
}

fn detect_is_binary(
    path: &Path,
    bytes: &[u8],
    detected_format: FileFormat,
    programming_language: Option<&str>,
) -> bool {
    if is_textual_format(detected_format) {
        return false;
    }

    if lower_extension(path)
        .as_deref()
        .is_some_and(|ext| BINARY_EXTENSIONS.contains(&ext))
    {
        return true;
    }

    if should_treat_binary_bytes_as_text(path, bytes, programming_language) {
        return false;
    }

    has_binary_control_chars(bytes)
        || is_known_binary_format(detected_format)
        || (matches!(detected_format, FileFormat::ArbitraryBinaryData)
            && !looks_like_textual_bytes(bytes))
}

fn should_treat_binary_bytes_as_text(
    path: &Path,
    bytes: &[u8],
    programming_language: Option<&str>,
) -> bool {
    has_decodable_text(bytes)
        && looks_like_textual_bytes(bytes)
        && (bytes.starts_with(b"#!") || is_textual_source_candidate(path, programming_language))
}

fn detect_is_archive(
    path: &Path,
    bytes: &[u8],
    mime_type: &str,
    is_text: bool,
    detected_format: FileFormat,
) -> bool {
    if is_text {
        return false;
    }

    lower_extension(path)
        .as_deref()
        .is_some_and(|ext| ARCHIVE_EXTENSIONS.contains(&ext))
        || matches!(
            detected_format.kind(),
            FileFormatKind::Archive | FileFormatKind::Compressed | FileFormatKind::Package
        )
        || is_zip_archive(bytes)
        || looks_like_gzip(bytes)
        || looks_like_bzip2(bytes)
        || looks_like_xz(bytes)
        || looks_like_deb(bytes, path)
        || looks_like_rpm(bytes, path)
        || looks_like_squashfs(bytes, path)
        || mime_type.contains("zip")
        || mime_type.contains("compressed")
        || mime_type.contains("tar")
        || mime_type.contains("x-rpm")
        || mime_type.contains("debian")
}

fn detect_is_media(
    path: &Path,
    bytes: &[u8],
    mime_type: &str,
    detected_format: FileFormat,
) -> bool {
    media_mime_from_content(bytes).is_some()
        || matches!(
            detected_format.kind(),
            FileFormatKind::Audio | FileFormatKind::Image | FileFormatKind::Video
        )
        || mime_type.starts_with("image/")
        || mime_type.starts_with("audio/")
        || mime_type.starts_with("video/")
        || (mime_type == "application/octet-stream"
            && lower_extension(path).as_deref() == Some("tga")
            && !has_binary_control_chars(bytes))
}

fn detect_is_script(
    path: &Path,
    bytes: &[u8],
    programming_language: Option<&str>,
    is_text: bool,
) -> bool {
    if !is_text || is_makefile(path) {
        return false;
    }

    bytes.starts_with(b"#!")
        || lower_extension(path).as_deref().is_some_and(|ext| {
            matches!(
                ext,
                "sh" | "bash" | "zsh" | "fish" | "ksh" | "ps1" | "psm1" | "psd1" | "awk"
            )
        })
        || matches!(
            programming_language,
            Some("Shell" | "Python" | "Ruby" | "Perl" | "PHP" | "PowerShell" | "Awk")
        )
}

fn detect_is_source(
    path: &Path,
    programming_language: Option<&str>,
    is_text: bool,
    is_script: bool,
) -> bool {
    if !is_text || is_plain_text(path) || is_makefile(path) || is_source_map(path) {
        return false;
    }

    if is_c_like_source(path) || is_java_like_source(path) {
        return true;
    }

    programming_language.is_some() || is_script
}

#[allow(clippy::too_many_arguments)]
fn detect_file_type(
    path: &Path,
    bytes: &[u8],
    detected_format: FileFormat,
    mime_type: &str,
    programming_language: Option<&str>,
    is_binary: bool,
    is_text: bool,
    is_archive: bool,
    is_media: bool,
    is_script: bool,
) -> String {
    if bytes.is_empty() {
        return "empty".to_string();
    }

    if looks_like_pdf(bytes) {
        return "PDF document".to_string();
    }

    if let Some(file_type) = media_file_type_from_content(bytes) {
        return file_type.to_string();
    }

    if is_archive {
        return archive_file_type(path, bytes, detected_format);
    }

    if is_script {
        return script_file_type(programming_language, bytes);
    }

    if is_text {
        if lower_extension(path).as_deref() == Some("json") {
            return "JSON text data".to_string();
        }
        if lower_extension(path).as_deref() == Some("xml") {
            return "XML text data".to_string();
        }
        if matches!(lower_extension(path).as_deref(), Some("yaml" | "yml")) {
            return "YAML text data".to_string();
        }
        if lower_extension(path).as_deref() == Some("toml") {
            return "TOML text data".to_string();
        }
        if matches!(
            lower_extension(path).as_deref(),
            Some("ini" | "cfg" | "conf")
        ) {
            return "INI text data".to_string();
        }
        if matches!(lower_file_name(path).as_str(), ".gitmodules" | ".gitconfig") {
            return "Git configuration text".to_string();
        }
        if matches!(lower_extension(path).as_deref(), Some("md" | "markdown")) {
            return text_file_type(bytes);
        }
        if programming_language.is_some() && !is_media {
            return text_file_type(bytes);
        }
        return text_file_type(bytes);
    }

    if let Some(file_type) = format_based_file_type(detected_format) {
        return file_type;
    }

    if is_binary && mime_type == "application/octet-stream" {
        return "data".to_string();
    }

    mime_type.to_string()
}

fn is_textual_source_candidate(path: &Path, programming_language: Option<&str>) -> bool {
    if matches!(programming_language, Some(language) if is_source_like_language(language)) {
        return true;
    }

    if matches!(
        lower_file_name(path).as_str(),
        "dockerfile"
            | "containerfile"
            | "containerfile.core"
            | "apkbuild"
            | "podfile"
            | "meson.build"
            | "build"
            | "workspace"
            | "buck"
            | "default.nix"
            | "flake.nix"
            | "shell.nix"
    ) {
        return true;
    }

    path.extension()
        .and_then(|ext| ext.to_str())
        .is_some_and(|ext| {
            matches!(
                ext.to_ascii_lowercase().as_str(),
                "rs" | "py"
                    | "js"
                    | "mjs"
                    | "cjs"
                    | "jsx"
                    | "ts"
                    | "mts"
                    | "cts"
                    | "tsx"
                    | "c"
                    | "cpp"
                    | "cc"
                    | "cxx"
                    | "h"
                    | "hpp"
                    | "m"
                    | "mm"
                    | "s"
                    | "asm"
                    | "java"
                    | "go"
                    | "rb"
                    | "php"
                    | "pl"
                    | "swift"
                    | "sh"
                    | "bash"
                    | "zsh"
                    | "fish"
                    | "ksh"
                    | "ps1"
                    | "psm1"
                    | "psd1"
                    | "awk"
                    | "kt"
                    | "kts"
                    | "dart"
                    | "scala"
                    | "groovy"
                    | "gradle"
                    | "gvy"
                    | "gy"
                    | "gsh"
                    | "cs"
                    | "fs"
                    | "fsx"
                    | "r"
                    | "lua"
                    | "jl"
                    | "ex"
                    | "exs"
                    | "clj"
                    | "cljs"
                    | "cljc"
                    | "hs"
                    | "erl"
                    | "nix"
                    | "zig"
                    | "bzl"
                    | "bazel"
                    | "star"
                    | "sky"
                    | "ml"
                    | "mli"
                    | "tex"
            )
        })
}

fn is_source_like_language(language: &str) -> bool {
    matches!(
        language,
        "Rust"
            | "Python"
            | "JavaScript"
            | "TypeScript"
            | "JavaScript/TypeScript"
            | "C"
            | "C++"
            | "Objective-C"
            | "Objective-C++"
            | "GAS"
            | "Java"
            | "Go"
            | "Ruby"
            | "PHP"
            | "Perl"
            | "Swift"
            | "Shell"
            | "PowerShell"
            | "Awk"
            | "Kotlin"
            | "Dart"
            | "Scala"
            | "C#"
            | "F#"
            | "R"
            | "Lua"
            | "Julia"
            | "Elixir"
            | "Clojure"
            | "Haskell"
            | "Erlang"
            | "Groovy"
            | "Nix"
            | "Zig"
            | "Starlark"
            | "OCaml"
            | "Meson"
            | "TeX"
            | "Dockerfile"
            | "Makefile"
    )
}

fn extension(path: &Path) -> Option<&str> {
    path.extension().and_then(|ext| ext.to_str())
}

fn lower_extension(path: &Path) -> Option<String> {
    extension(path).map(|ext| ext.to_ascii_lowercase())
}

fn lower_file_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase())
        .unwrap_or_default()
}

fn is_plain_text(path: &Path) -> bool {
    lower_extension(path)
        .as_deref()
        .is_some_and(|ext| PLAIN_TEXT_EXTENSIONS.contains(&ext))
}

fn is_makefile(path: &Path) -> bool {
    matches!(lower_file_name(path).as_str(), "makefile" | "makefile.inc")
}

fn is_source_map(path: &Path) -> bool {
    let path_lower = path.to_string_lossy().to_ascii_lowercase();
    path_lower.ends_with(".js.map") || path_lower.ends_with(".css.map")
}

fn is_c_like_source(path: &Path) -> bool {
    lower_extension(path).as_deref().is_some_and(|ext| {
        matches!(
            ext,
            "c" | "cc"
                | "cp"
                | "cpp"
                | "cxx"
                | "c++"
                | "h"
                | "hh"
                | "hpp"
                | "hxx"
                | "h++"
                | "i"
                | "ii"
                | "m"
                | "s"
                | "asm"
        )
    })
}

fn is_java_like_source(path: &Path) -> bool {
    lower_extension(path)
        .as_deref()
        .is_some_and(|ext| matches!(ext, "java" | "aj" | "jad" | "ajt"))
}

fn format_based_file_type(detected_format: FileFormat) -> Option<String> {
    match detected_format {
        FileFormat::ArbitraryBinaryData | FileFormat::Empty | FileFormat::PlainText => None,
        format if format.short_name() == Some("PDF") => Some("PDF document".to_string()),
        format => Some(match format.kind() {
            FileFormatKind::Image => short_name_or_name(&format, "image data"),
            FileFormatKind::Audio => short_name_or_name(&format, "audio data"),
            FileFormatKind::Video => short_name_or_name(&format, "video data"),
            _ => format.name().to_string(),
        }),
    }
}

fn short_name_or_name(format: &FileFormat, suffix: &str) -> String {
    format
        .short_name()
        .map(|short_name| format!("{short_name} {suffix}"))
        .unwrap_or_else(|| format!("{} {suffix}", format.name()))
}

fn detect_zip_like_mime(path: &Path) -> String {
    match extension(path).map(|ext| ext.to_ascii_lowercase()) {
        Some(ext) if ext == "apk" => "application/vnd.android.package-archive".to_string(),
        Some(ext) if matches!(ext.as_str(), "jar" | "war" | "ear") => {
            "application/java-archive".to_string()
        }
        _ => "application/zip".to_string(),
    }
}

fn media_mime_from_content(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"II\x2a\x00") || bytes.starts_with(b"MM\x00\x2a") {
        Some("image/tiff")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

fn media_file_type_from_content(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("PNG image data")
    } else if bytes.starts_with(&[0xff, 0xd8, 0xff]) {
        Some("JPEG image data")
    } else if bytes.starts_with(b"II\x2a\x00") || bytes.starts_with(b"MM\x00\x2a") {
        Some("TIFF image data")
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some("WebP image data")
    } else {
        None
    }
}

fn looks_like_pdf(bytes: &[u8]) -> bool {
    bytes.starts_with(b"%PDF-")
}

fn looks_like_rtf(bytes: &[u8], ext: Option<&str>) -> bool {
    ext == Some("rtf") || bytes.starts_with(b"{\\rtf")
}

fn extract_rtf_text(bytes: &[u8]) -> String {
    let text = String::from_utf8_lossy(bytes);
    let chars: Vec<char> = text.chars().collect();
    let mut output = String::new();
    let mut index = 0usize;

    while index < chars.len() {
        match chars[index] {
            '{' | '}' => {
                index += 1;
            }
            '\\' => {
                index += 1;
                if index >= chars.len() {
                    break;
                }

                match chars[index] {
                    '\\' | '{' | '}' => {
                        output.push(chars[index]);
                        index += 1;
                    }
                    '\'' => {
                        if index + 2 < chars.len() {
                            let hex = [chars[index + 1], chars[index + 2]];
                            let hex: String = hex.iter().collect();
                            if let Ok(value) = u8::from_str_radix(&hex, 16) {
                                output.push(value as char);
                                index += 3;
                                continue;
                            }
                        }
                        index += 1;
                    }
                    control if control.is_ascii_alphabetic() => {
                        let start = index;
                        while index < chars.len() && chars[index].is_ascii_alphabetic() {
                            index += 1;
                        }
                        let control_word: String = chars[start..index].iter().collect();

                        let number_start = index;
                        if index < chars.len()
                            && (chars[index] == '-' || chars[index].is_ascii_digit())
                        {
                            index += 1;
                            while index < chars.len() && chars[index].is_ascii_digit() {
                                index += 1;
                            }
                        }
                        let parameter: String = chars[number_start..index].iter().collect();

                        if index < chars.len() && chars[index] == ' ' {
                            index += 1;
                        }

                        match control_word.as_str() {
                            "par" | "line" => output.push('\n'),
                            "tab" => output.push('\t'),
                            "emdash" => output.push('—'),
                            "endash" => output.push('–'),
                            "bullet" => output.push('•'),
                            "lquote" | "rquote" => output.push('\''),
                            "ldblquote" | "rdblquote" => output.push('"'),
                            "u" => {
                                if let Ok(codepoint) = parameter.parse::<i32>() {
                                    let normalized = if codepoint < 0 {
                                        codepoint + 65_536
                                    } else {
                                        codepoint
                                    };
                                    if let Ok(normalized) = u32::try_from(normalized)
                                        && let Some(ch) = char::from_u32(normalized)
                                    {
                                        output.push(ch);
                                    }
                                }

                                if index < chars.len()
                                    && !matches!(chars[index], '\\' | '{' | '}' | '\n' | '\r')
                                {
                                    index += 1;
                                }
                            }
                            _ => {}
                        }
                    }
                    _ => {
                        index += 1;
                    }
                }
            }
            ch => {
                output.push(ch);
                index += 1;
            }
        }
    }

    output
        .replace(['\r', '\u{0c}'], "\n")
        .lines()
        .map(str::trim_end)
        .collect::<Vec<_>>()
        .join("\n")
}

fn looks_like_gzip(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0x1f, 0x8b])
}

fn looks_like_bzip2(bytes: &[u8]) -> bool {
    bytes.starts_with(b"BZh")
}

fn looks_like_xz(bytes: &[u8]) -> bool {
    bytes.starts_with(&[0xfd, b'7', b'z', b'X', b'Z', 0x00])
}

fn looks_like_deb(bytes: &[u8], path: &Path) -> bool {
    lower_extension(path).as_deref() == Some("deb") && bytes.starts_with(b"!<arch>\n")
}

fn looks_like_rpm(bytes: &[u8], path: &Path) -> bool {
    lower_extension(path).as_deref() == Some("rpm") && bytes.starts_with(&[0xed, 0xab, 0xee, 0xdb])
}

fn looks_like_squashfs(bytes: &[u8], path: &Path) -> bool {
    lower_extension(path)
        .as_deref()
        .is_some_and(|ext| matches!(ext, "sqs" | "squashfs"))
        && (bytes.starts_with(&[0x68, 0x73, 0x71, 0x73])
            || bytes.starts_with(&[0x73, 0x71, 0x73, 0x68]))
}

fn archive_file_type(path: &Path, bytes: &[u8], detected_format: FileFormat) -> String {
    if looks_like_deb(bytes, path) {
        "debian binary package (format 2.0)".to_string()
    } else if looks_like_rpm(bytes, path) {
        "RPM package".to_string()
    } else if looks_like_squashfs(bytes, path) {
        "Squashfs filesystem".to_string()
    } else if looks_like_gzip(bytes) {
        "gzip compressed data".to_string()
    } else if looks_like_bzip2(bytes) {
        "bzip2 compressed data".to_string()
    } else if looks_like_xz(bytes) {
        "XZ compressed data".to_string()
    } else if is_zip_archive(bytes) {
        "Zip archive data".to_string()
    } else if lower_extension(path).as_deref() == Some("gem") {
        "POSIX tar archive".to_string()
    } else if let Some(file_type) = format_based_file_type(detected_format) {
        file_type
    } else {
        "archive data".to_string()
    }
}

fn script_file_type(programming_language: Option<&str>, bytes: &[u8]) -> String {
    let suffix = text_executable_label(bytes);

    match programming_language {
        Some("Python") => format!("python script, {suffix}"),
        Some("Ruby") => format!("ruby script, {suffix}"),
        Some("Perl") => format!("perl script, {suffix}"),
        Some("PHP") => format!("php script, {suffix}"),
        Some("Shell") => format!("shell script, {suffix}"),
        Some("JavaScript") => format!("javascript script, {suffix}"),
        Some("TypeScript") => format!("typescript script, {suffix}"),
        Some("PowerShell") => format!("powershell script, {suffix}"),
        Some("Awk") => format!("awk script, {suffix}"),
        _ => format!("script, {suffix}"),
    }
}

fn text_file_type(bytes: &[u8]) -> String {
    text_label(bytes).to_string()
}

fn text_label(bytes: &[u8]) -> &'static str {
    if std::str::from_utf8(bytes).is_ok() {
        if bytes.contains(&b'\n') {
            "UTF-8 Unicode text"
        } else {
            "UTF-8 Unicode text, with no line terminators"
        }
    } else if bytes.contains(&b'\n') {
        "text"
    } else {
        "text, with no line terminators"
    }
}

fn text_executable_label(bytes: &[u8]) -> &'static str {
    if std::str::from_utf8(bytes).is_ok() {
        if bytes.contains(&b'\n') {
            "UTF-8 Unicode text executable"
        } else {
            "UTF-8 Unicode text executable, with no line terminators"
        }
    } else if bytes.contains(&b'\n') {
        "text executable"
    } else {
        "text executable, with no line terminators"
    }
}

fn supported_image_metadata_format(
    ext: Option<&str>,
    detected_format: FileFormat,
) -> Option<ImageFormat> {
    match ext {
        Some("jpg" | "jpeg") => Some(ImageFormat::Jpeg),
        Some("png") => Some(ImageFormat::Png),
        Some("tif" | "tiff") => Some(ImageFormat::Tiff),
        Some("webp") => Some(ImageFormat::WebP),
        _ => match detected_format.media_type() {
            "image/jpeg" => Some(ImageFormat::Jpeg),
            "image/png" => Some(ImageFormat::Png),
            "image/tiff" => Some(ImageFormat::Tiff),
            "image/webp" => Some(ImageFormat::WebP),
            _ => None,
        },
    }
}

fn should_skip_binary_string_extraction(
    path: &Path,
    bytes: &[u8],
    detected_format: FileFormat,
) -> bool {
    matches!(lower_extension(path).as_deref(), Some("pdf"))
        || supported_image_metadata_format(lower_extension(path).as_deref(), detected_format)
            .is_some()
        || (matches!(
            detected_format.kind(),
            FileFormatKind::Audio | FileFormatKind::Image | FileFormatKind::Video
        ) && !is_textual_format(detected_format))
        || media_mime_from_content(bytes).is_some()
        || is_zip_archive(bytes)
        || looks_like_gzip(bytes)
        || looks_like_bzip2(bytes)
        || looks_like_xz(bytes)
        || looks_like_deb(bytes, path)
        || looks_like_rpm(bytes, path)
        || looks_like_squashfs(bytes, path)
}

fn is_supported_image_container(bytes: &[u8], format: ImageFormat) -> bool {
    match format {
        ImageFormat::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        ImageFormat::Jpeg => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        ImageFormat::Tiff => bytes.starts_with(b"II\x2a\x00") || bytes.starts_with(b"MM\x00\x2a"),
        ImageFormat::WebP => {
            bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP"
        }
        _ => false,
    }
}

fn extract_image_metadata_text(bytes: &[u8], format: ImageFormat) -> String {
    let mut values = Vec::new();
    values.extend(extract_exif_metadata_values(bytes));
    values.extend(extract_xmp_metadata_values(bytes, format));
    values_to_text(values)
}

fn extract_exif_metadata_values(bytes: &[u8]) -> Vec<String> {
    let mut cursor = BufReader::new(Cursor::new(bytes));
    let exif = match exif::Reader::new().read_from_container(&mut cursor) {
        Ok(exif) => exif,
        Err(_) => return Vec::new(),
    };

    let mut values = Vec::new();
    for field in exif.fields() {
        let rendered = match field.tag {
            exif::Tag::ImageDescription | exif::Tag::Copyright | exif::Tag::UserComment => {
                Some(field.display_value().with_unit(&exif).to_string())
            }
            exif::Tag::Artist => Some(format!(
                "Author: {}",
                field.display_value().with_unit(&exif)
            )),
            _ => None,
        };

        if let Some(rendered) = rendered {
            values.push(rendered);
        }
    }

    values
}

fn extract_xmp_metadata_values(bytes: &[u8], format: ImageFormat) -> Vec<String> {
    let xmp = match extract_raw_xmp_packet(bytes, format) {
        Some(xmp) => xmp,
        None => return Vec::new(),
    };

    parse_xmp_values(&xmp)
}

fn extract_raw_xmp_packet(bytes: &[u8], format: ImageFormat) -> Option<Vec<u8>> {
    let reader = ImageReader::with_format(BufReader::new(Cursor::new(bytes)), format);
    if let Ok(mut decoder) = reader.into_decoder()
        && let Ok(Some(xmp)) = decoder.xmp_metadata()
    {
        return Some(xmp);
    }

    match format {
        ImageFormat::Png => extract_png_xmp_packet(bytes),
        _ => None,
    }
}

fn extract_png_xmp_packet(bytes: &[u8]) -> Option<Vec<u8>> {
    const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";

    if bytes.len() < PNG_SIGNATURE.len() || &bytes[..PNG_SIGNATURE.len()] != PNG_SIGNATURE {
        return None;
    }

    let mut offset = PNG_SIGNATURE.len();
    while offset + 12 <= bytes.len() {
        let length = u32::from_be_bytes([
            bytes[offset],
            bytes[offset + 1],
            bytes[offset + 2],
            bytes[offset + 3],
        ]) as usize;
        let chunk_start = offset + 8;
        let chunk_end = chunk_start + length;
        if chunk_end + 4 > bytes.len() {
            return None;
        }

        let chunk_type = &bytes[offset + 4..offset + 8];
        if chunk_type == b"iTXt" {
            let data = &bytes[chunk_start..chunk_end];
            if let Some(xmp) = parse_png_itxt_xmp(data) {
                return Some(xmp);
            }
        }

        offset = chunk_end + 4;
    }

    None
}

fn parse_png_itxt_xmp(data: &[u8]) -> Option<Vec<u8>> {
    const XMP_KEYWORD: &[u8] = b"XML:com.adobe.xmp";

    let keyword_end = data.iter().position(|&b| b == 0)?;
    if &data[..keyword_end] != XMP_KEYWORD {
        return None;
    }

    let mut cursor = keyword_end + 1;
    let compression_flag = *data.get(cursor)?;
    cursor += 1;
    let compression_method = *data.get(cursor)?;
    cursor += 1;
    if compression_flag > 1 || (compression_flag == 1 && compression_method != 0) {
        return None;
    }

    let language_end = cursor + data[cursor..].iter().position(|&b| b == 0)?;
    cursor = language_end + 1;

    let translated_end = cursor + data[cursor..].iter().position(|&b| b == 0)?;
    cursor = translated_end + 1;

    let text_bytes = &data[cursor..];
    if compression_flag == 1 {
        let mut decoder = ZlibDecoder::new(text_bytes);
        let mut decoded = Vec::new();
        decoder.read_to_end(&mut decoded).ok()?;
        Some(decoded)
    } else {
        Some(text_bytes.to_vec())
    }
}

fn parse_xmp_values(xmp: &[u8]) -> Vec<String> {
    let mut reader = XmlReader::from_reader(xmp);
    reader.config_mut().trim_text(true);

    let mut buf = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut values = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                stack.push(local_xml_name(e.name().as_ref()));
            }
            Ok(Event::End(_)) => {
                stack.pop();
            }
            Ok(Event::Empty(_)) => {}
            Ok(Event::Text(text)) => {
                if let Some(field) = stack
                    .iter()
                    .rev()
                    .find_map(|name| allowed_xmp_field(name.as_str()))
                    && let Ok(decoded) = text.decode()
                {
                    let decoded = decoded.into_owned();
                    if !decoded.trim().is_empty() {
                        values.push(format_xmp_value(field, &decoded));
                    }
                }
            }
            Ok(Event::CData(text)) => {
                if let Some(field) = stack
                    .iter()
                    .rev()
                    .find_map(|name| allowed_xmp_field(name.as_str()))
                    && let Ok(decoded) = text.decode()
                {
                    let decoded = decoded.into_owned();
                    if !decoded.trim().is_empty() {
                        values.push(format_xmp_value(field, &decoded));
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
        buf.clear();
    }

    values
}

fn local_xml_name(name: &[u8]) -> String {
    let name = std::str::from_utf8(name).unwrap_or_default();
    name.rsplit(':').next().unwrap_or(name).to_string()
}

fn allowed_xmp_field(name: &str) -> Option<&'static str> {
    match name {
        "creator" => Some("creator"),
        "rights" => Some("rights"),
        "description" => Some("description"),
        "title" => Some("title"),
        "subject" => Some("subject"),
        "UsageTerms" => Some("usage_terms"),
        "WebStatement" => Some("web_statement"),
        _ => None,
    }
}

fn format_xmp_value(field: &str, value: &str) -> String {
    match field {
        "creator" => format!("Author: {value}"),
        _ => value.to_string(),
    }
}

fn values_to_text(values: Vec<String>) -> String {
    let mut seen = BTreeSet::new();
    let mut lines = Vec::new();
    let mut total_bytes = 0usize;

    for value in values {
        if lines.len() >= MAX_IMAGE_METADATA_VALUES {
            break;
        }

        let normalized = normalize_metadata_value(&value);
        if normalized.is_empty() || !seen.insert(normalized.clone()) {
            continue;
        }

        let added_bytes = normalized.len() + usize::from(!lines.is_empty());
        if total_bytes + added_bytes > MAX_IMAGE_METADATA_TEXT_BYTES {
            break;
        }

        total_bytes += added_bytes;
        lines.push(normalized);
    }

    lines.join("\n")
}

fn normalize_metadata_value(value: &str) -> String {
    value
        .chars()
        .filter(|&ch| ch != '\0')
        .collect::<String>()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim()
        .to_string()
}

fn extract_pdf_text(path: &Path, bytes: &[u8]) -> (String, Option<String>) {
    if bytes.len() < 5 || &bytes[..5] != b"%PDF-" {
        return (String::new(), None);
    }

    let mut failures = Vec::new();
    let mut saw_success = false;

    let extracted = catch_unwind(AssertUnwindSafe(
        || -> Result<String, Box<dyn std::error::Error>> {
            let mut document = pdf_oxide::document::PdfDocument::from_bytes(bytes.to_vec())?;
            extract_first_pdf_page_text(&mut document)
        },
    ));
    match extracted {
        Ok(Ok(text)) => {
            saw_success = true;
            if let Some(normalized) = normalize_pdf_text(text) {
                return (normalized, None);
            }
        }
        Ok(Err(err)) => failures.push(format!("from-bytes first-page: {err}")),
        Err(payload) => failures.push(format!(
            "from-bytes first-page panic: {}",
            panic_payload_to_string(payload.as_ref())
        )),
    }

    let extracted = catch_unwind(AssertUnwindSafe(
        || -> Result<String, Box<dyn std::error::Error>> {
            let mut document = pdf_oxide::document::PdfDocument::open(path)?;
            extract_pdf_text_from_document(&mut document)
        },
    ));
    match extracted {
        Ok(Ok(text)) => {
            saw_success = true;
            if let Some(normalized) = normalize_pdf_text(text) {
                return (normalized, None);
            }
        }
        Ok(Err(err)) => failures.push(format!("open full-document: {err}")),
        Err(payload) => failures.push(format!(
            "open full-document panic: {}",
            panic_payload_to_string(payload.as_ref())
        )),
    }

    let extracted = catch_unwind(AssertUnwindSafe(
        || -> Result<String, Box<dyn std::error::Error>> {
            let mut document = pdf_oxide::document::PdfDocument::from_bytes(bytes.to_vec())?;
            extract_pdf_text_from_document(&mut document)
        },
    ));
    match extracted {
        Ok(Ok(text)) => {
            saw_success = true;
            if let Some(normalized) = normalize_pdf_text(text) {
                return (normalized, None);
            }
        }
        Ok(Err(err)) => failures.push(format!("from-bytes full-document: {err}")),
        Err(payload) => failures.push(format!(
            "from-bytes full-document panic: {}",
            panic_payload_to_string(payload.as_ref())
        )),
    }

    if saw_success {
        (String::new(), None)
    } else {
        (
            String::new(),
            Some(format!(
                "PDF text extraction failed after {} attempts: {}",
                failures.len(),
                failures.join("; ")
            )),
        )
    }
}

fn panic_payload_to_string(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

fn extract_first_pdf_page_text(
    document: &mut pdf_oxide::document::PdfDocument,
) -> Result<String, Box<dyn std::error::Error>> {
    if document.page_count()? == 0 {
        return Ok(String::new());
    }

    let extracted_text = document.extract_text(0)?;
    let markdown_text =
        document.to_markdown(0, &pdf_oxide::converters::ConversionOptions::default())?;
    if pdf_markdown_heading_lines(&markdown_text).is_empty() {
        return Ok(extracted_text);
    }

    let pipeline_text =
        document.to_plain_text(0, &pdf_oxide::converters::ConversionOptions::default())?;

    Ok(merge_pdf_first_page_text(
        &extracted_text,
        &markdown_text,
        &pipeline_text,
    ))
}

fn extract_pdf_text_from_document(
    document: &mut pdf_oxide::document::PdfDocument,
) -> Result<String, Box<dyn std::error::Error>> {
    Ok(document.to_plain_text_all(&pdf_oxide::converters::ConversionOptions::default())?)
}

fn normalize_pdf_text(text: String) -> Option<String> {
    let normalized = text.replace(['\r', '\u{0c}'], "\n");
    (!normalized.trim().is_empty()).then_some(normalized)
}

fn merge_pdf_first_page_text(
    _extracted_text: &str,
    markdown_text: &str,
    pipeline_text: &str,
) -> String {
    let pipeline = pipeline_text.trim();
    if pipeline.is_empty() {
        return String::new();
    }

    let prefix = pdf_first_page_heading_prefix(markdown_text);
    let Some(prefix) = prefix else {
        return pipeline_text.to_string();
    };

    if pdf_text_contains_heading_prefix(pipeline, &prefix) {
        pipeline_text.to_string()
    } else {
        format!("{prefix}\n\n{pipeline}")
    }
}

fn pdf_text_contains_heading_prefix(text: &str, prefix: &str) -> bool {
    normalize_pdf_heading_comparison_text(text)
        .contains(&normalize_pdf_heading_comparison_text(prefix))
}

fn normalize_pdf_heading_comparison_text(text: &str) -> String {
    text.split_whitespace()
        .map(|part| part.to_ascii_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

fn pdf_first_page_heading_prefix(markdown_text: &str) -> Option<String> {
    let mut lines = Vec::new();

    for line in pdf_markdown_heading_lines(markdown_text) {
        push_unique_line(&mut lines, line);
    }

    (!lines.is_empty()).then(|| lines.join("\n"))
}

fn pdf_markdown_heading_lines(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter_map(|line| line.strip_prefix('#').map(str::trim_start))
        .map(|line| line.trim_matches('#').trim())
        .filter(|line| !line.is_empty())
        .filter(|line| !looks_like_numbered_section_heading(line))
        .take(4)
        .map(ToOwned::to_owned)
        .collect()
}

fn push_unique_line(lines: &mut Vec<String>, line: String) {
    if !lines.iter().any(|existing| existing == &line) {
        lines.push(line);
    }
}

fn looks_like_numbered_section_heading(line: &str) -> bool {
    let mut chars = line.chars();
    let Some(first) = chars.next() else {
        return false;
    };

    if !first.is_ascii_digit() {
        return false;
    }

    matches!(chars.next(), Some('.'))
}

fn is_zip_archive(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

pub fn extract_printable_strings(bytes: &[u8]) -> String {
    const MIN_LEN: usize = 4;
    const MIN_OUTPUT_BYTES: usize = 2_000_000;
    const MAX_OUTPUT_BYTES_CAP: usize = 16_000_000;

    let max_output_bytes = bytes.len().clamp(MIN_OUTPUT_BYTES, MAX_OUTPUT_BYTES_CAP);

    fn is_printable_ascii(b: u8) -> bool {
        matches!(b, 0x20..=0x7E)
    }

    let mut out = String::new();
    let mut run: Vec<u8> = Vec::new();

    let flush_run = |out: &mut String, run: &mut Vec<u8>| {
        if run.len() >= MIN_LEN {
            if !out.is_empty() {
                out.push('\n');
            }
            out.push_str(&String::from_utf8_lossy(run));
        }
        run.clear();
    };

    for &b in bytes {
        if is_printable_ascii(b) {
            run.push(b);
        } else {
            flush_run(&mut out, &mut run);
            if out.len() >= max_output_bytes {
                return out;
            }
        }
    }
    flush_run(&mut out, &mut run);
    if out.len() >= max_output_bytes {
        return out;
    }

    for start in 0..=1 {
        run.clear();
        let mut i = start;
        while i + 1 < bytes.len() {
            let b0 = bytes[i];
            let b1 = bytes[i + 1];
            let (ch, zero) = if start == 0 { (b0, b1) } else { (b1, b0) };
            if is_printable_ascii(ch) && zero == 0 {
                run.push(ch);
            } else {
                flush_run(&mut out, &mut run);
                if out.len() >= max_output_bytes {
                    return out;
                }
            }
            i += 2;
        }
        flush_run(&mut out, &mut run);
        if out.len() >= max_output_bytes {
            return out;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{
        ExtractedTextKind, classify_file_info, extract_printable_strings,
        extract_text_for_detection, extract_text_for_detection_with_diagnostics,
        normalize_mime_type, normalize_pdf_heading_comparison_text,
    };

    #[test]
    fn test_extract_text_for_detection_skips_jar_archives() {
        let path = Path::new(
            "testdata/license-golden/datadriven/lic1/do-not_detect-licenses-in-archive.jar",
        );
        let bytes = std::fs::read(path).expect("failed to read jar fixture");

        let (text, kind) = extract_text_for_detection(path, &bytes);

        assert!(text.is_empty());
        assert_eq!(kind, ExtractedTextKind::None);
    }

    #[test]
    fn test_extract_text_for_detection_reads_pdf_fixture_text() {
        let path = Path::new("testdata/license-golden/datadriven/lic2/bsd-new_156.pdf");
        let bytes = std::fs::read(path).expect("failed to read pdf fixture");

        let (text, kind) = extract_text_for_detection(path, &bytes);

        assert_eq!(kind, ExtractedTextKind::Pdf);
        assert!(text.contains("Redistribution and use in source and binary forms"));
    }

    #[test]
    fn test_extract_text_for_detection_prefers_first_pdf_page_before_full_document() {
        let path =
            Path::new("testdata/license-golden/datadriven/lic4/should_detect_something_5.pdf");
        let bytes = std::fs::read(path).expect("failed to read pdf fixture");

        let (text, kind) = extract_text_for_detection(path, &bytes);

        assert_eq!(kind, ExtractedTextKind::Pdf);
        assert!(text.contains("SUN INDUSTRY STANDARDS SOURCE LICENSE"));
        assert!(!text.contains("DISCLAIMER OF WARRANTY"));
    }

    #[test]
    fn test_extract_text_for_detection_does_not_duplicate_pdf_heading_prefix() {
        let path =
            Path::new("testdata/license-golden/datadriven/lic4/should_detect_something_5.pdf");
        let bytes = std::fs::read(path).expect("failed to read pdf fixture");

        let (text, kind) = extract_text_for_detection(path, &bytes);

        assert_eq!(kind, ExtractedTextKind::Pdf);

        let normalized = normalize_pdf_heading_comparison_text(&text);
        let heading =
            normalize_pdf_heading_comparison_text("SUN INDUSTRY STANDARDS SOURCE LICENSE");
        assert_eq!(normalized.matches(&heading).count(), 1);
    }

    #[test]
    fn test_extract_text_for_detection_reads_pdf_fixture_without_pdf_extension() {
        let path = Path::new("testdata/license-golden/datadriven/lic2/bsd-new_156.pdf");
        let bytes = std::fs::read(path).expect("failed to read pdf fixture");

        let (text, kind) = extract_text_for_detection(Path::new("renamed.bin"), &bytes);

        assert_eq!(kind, ExtractedTextKind::Pdf);
        assert!(text.contains("Redistribution and use in source and binary forms"));
    }

    #[test]
    fn test_extract_text_for_detection_reports_terminal_pdf_failure() {
        let malformed = b"%PDF-1.7\nthis is not a valid pdf object graph\n";

        let (text, kind, scan_error) =
            extract_text_for_detection_with_diagnostics(Path::new("broken.pdf"), malformed);

        assert!(text.is_empty());
        assert_eq!(kind, ExtractedTextKind::None);
        let scan_error = scan_error.expect("terminal pdf failure should be surfaced");
        assert!(scan_error.contains("PDF text extraction failed after"));
    }

    #[test]
    fn test_extract_text_for_detection_skips_zip_like_archives() {
        let zip_bytes = b"PK\x03\x04\x14\x00\x00\x00\x08\x00artifact";

        let (whl_text, whl_kind) = extract_text_for_detection(Path::new("demo.whl"), zip_bytes);
        let (crate_text, crate_kind) =
            extract_text_for_detection(Path::new("demo.crate"), zip_bytes);

        assert!(whl_text.is_empty());
        assert_eq!(whl_kind, ExtractedTextKind::None);
        assert!(crate_text.is_empty());
        assert_eq!(crate_kind, ExtractedTextKind::None);
    }

    #[test]
    fn test_extract_text_for_detection_keeps_binary_strings_for_lib_fixtures() {
        let path =
            Path::new("testdata/copyright-golden/copyrights/copyright_php_lib-php_embed_lib.lib");
        let bytes = std::fs::read(path).expect("failed to read lib fixture");

        let (text, kind) = extract_text_for_detection(path, &bytes);

        assert_ne!(kind, ExtractedTextKind::None);
        assert!(text.contains("Copyright nexB and others (c) 2012"));
    }

    #[test]
    fn test_extract_printable_strings_scales_cap_for_medium_binary_files() {
        let bytes = b"abcd\0".repeat(525_000);

        let text = extract_printable_strings(&bytes);

        assert!(
            text.len() > 2_000_000,
            "unexpected truncation at {}",
            text.len()
        );
        assert!(text.ends_with("abcd"));
    }

    #[test]
    fn test_extract_text_for_detection_decodes_svg_fixture_text() {
        let path = Path::new(
            "testdata/license-golden/datadriven/external/fossology-tests/Public-domain/biohazard.svg",
        );
        let bytes = std::fs::read(path).expect("failed to read svg fixture");

        let (text, kind) = extract_text_for_detection(path, &bytes);

        assert_eq!(kind, ExtractedTextKind::Decoded);
        assert!(text.contains("creativecommons.org/licenses/publicdomain"));
    }

    #[test]
    fn test_extract_text_for_detection_decodes_rtf_fixture_text() {
        let path = Path::new(
            "testdata/license-golden/datadriven/external/fossology-tests/LGPL/License.rtf",
        );
        let bytes = std::fs::read(path).expect("failed to read rtf fixture");

        let (text, kind) = extract_text_for_detection(path, &bytes);

        assert_eq!(kind, ExtractedTextKind::Decoded);
        assert!(text.contains("GNU Lesser General Public"));
        assert!(text.contains("version"));
        assert!(text.contains("2.1 of the License"));
    }

    #[test]
    fn test_normalize_mime_type_prefers_text_for_textual_video_guess() {
        assert_eq!(
            normalize_mime_type(
                Path::new("main.ts"),
                b"export const answer = 42;\n",
                Some("TypeScript"),
                "video/mp2t",
            ),
            "text/plain"
        );
    }

    #[test]
    fn test_normalize_mime_type_prefers_text_for_octet_stream_source_guess() {
        assert_eq!(
            normalize_mime_type(
                Path::new("main.js"),
                b"console.log('hello');\n",
                Some("JavaScript"),
                "application/octet-stream",
            ),
            "text/plain"
        );
    }

    #[test]
    fn test_normalize_mime_type_preserves_binary_video_guess() {
        assert_eq!(
            normalize_mime_type(
                Path::new("main.ts"),
                &[0, 159, 146, 150, 0, 1, 2, 3],
                Some("TypeScript"),
                "video/mp2t",
            ),
            "video/mp2t"
        );
    }

    #[test]
    fn test_normalize_mime_type_preserves_short_binary_octet_stream_guess() {
        assert_eq!(
            normalize_mime_type(
                Path::new("main.ts"),
                &[0, 159, 146, 150],
                Some("TypeScript"),
                "application/octet-stream",
            ),
            "application/octet-stream"
        );
    }

    #[test]
    fn test_classify_file_info_marks_empty_files_as_text_not_source() {
        let classification = classify_file_info(Path::new("test.txt"), b"");

        assert_eq!(classification.mime_type, "inode/x-empty");
        assert_eq!(classification.file_type, "empty");
        assert!(!classification.is_binary);
        assert!(classification.is_text);
        assert!(!classification.is_source);
        assert_eq!(classification.programming_language, None);
    }

    #[test]
    fn test_classify_file_info_keeps_json_out_of_programming_language() {
        let classification = classify_file_info(Path::new("package.json"), br#"{"name":"demo"}"#);

        assert_eq!(classification.mime_type, "application/json");
        assert_eq!(classification.file_type, "JSON text data");
        assert!(classification.is_text);
        assert!(!classification.is_source);
        assert_eq!(classification.programming_language, None);
    }

    #[test]
    fn test_classify_file_info_treats_dockerfile_as_source() {
        let classification = classify_file_info(Path::new("Dockerfile"), b"FROM scratch\n");

        assert_eq!(
            classification.programming_language.as_deref(),
            Some("Dockerfile")
        );
        assert!(classification.is_source);
        assert!(!classification.is_script);
        assert_eq!(classification.file_type, "UTF-8 Unicode text");
    }

    #[test]
    fn test_classify_file_info_treats_makefile_as_text_not_source() {
        let classification = classify_file_info(Path::new("Makefile"), b"all:\n\techo hi\n");

        assert_eq!(classification.programming_language, None);
        assert!(classification.is_text);
        assert!(!classification.is_source);
        assert!(!classification.is_script);
        assert_eq!(classification.file_type, "UTF-8 Unicode text");
    }

    #[test]
    fn test_classify_file_info_marks_supported_package_archives() {
        let zip_bytes = b"PK\x03\x04\x14\x00\x00\x00";

        let egg = classify_file_info(Path::new("demo.egg"), zip_bytes);
        let nupkg = classify_file_info(Path::new("demo.nupkg"), zip_bytes);

        assert!(egg.is_archive);
        assert_eq!(egg.mime_type, "application/zip");
        assert_eq!(egg.file_type, "Zip archive data");
        assert!(nupkg.is_archive);
        assert_eq!(nupkg.mime_type, "application/zip");
        assert_eq!(nupkg.file_type, "Zip archive data");
    }

    #[test]
    fn test_classify_file_info_marks_png_as_binary_media() {
        let png_bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR";

        let classification = classify_file_info(Path::new("logo.png"), png_bytes);

        assert_eq!(classification.mime_type, "image/png");
        assert_eq!(classification.file_type, "PNG image data");
        assert!(classification.is_binary);
        assert!(!classification.is_text);
        assert!(classification.is_media);
        assert!(!classification.is_archive);
        assert!(!classification.is_source);
    }

    #[test]
    fn test_classify_file_info_marks_pdf_as_binary_document() {
        let pdf_bytes = b"%PDF-1.7\n1 0 obj\n<< /Type /Catalog >>\n";

        let classification = classify_file_info(Path::new("report.pdf"), pdf_bytes);

        assert_eq!(classification.mime_type, "application/pdf");
        assert_eq!(classification.file_type, "PDF document");
        assert!(classification.is_binary);
        assert!(!classification.is_text);
        assert!(!classification.is_archive);
        assert!(!classification.is_media);
    }

    #[test]
    fn test_classify_file_info_marks_binary_blobs_as_binary() {
        let classification =
            classify_file_info(Path::new("blob.bin"), &[0, 159, 146, 150, 0, 1, 2, 3, 4, 5]);

        assert!(classification.is_binary);
        assert!(!classification.is_text);
        assert!(!classification.is_source);
        assert_eq!(classification.programming_language, None);
    }

    #[test]
    fn test_classify_file_info_treats_yaml_as_text_not_source() {
        let classification = classify_file_info(Path::new("config.yaml"), b"key: value\n");

        assert_eq!(classification.programming_language, None);
        assert!(classification.is_text);
        assert!(!classification.is_source);
        assert_eq!(classification.file_type, "YAML text data");
    }

    #[test]
    fn test_classify_file_info_classifies_common_build_manifests() {
        let gradle = classify_file_info(Path::new("build.gradle"), b"plugins { id 'java' }\n");
        let flake = classify_file_info(Path::new("flake.nix"), b"{ inputs, ... }: {}\n");
        let gitmodules = classify_file_info(
            Path::new(".gitmodules"),
            b"[submodule \"demo\"]\n\tpath = vendor/demo\n",
        );

        assert_eq!(gradle.programming_language.as_deref(), Some("Groovy"));
        assert!(gradle.is_source);
        assert_eq!(gradle.mime_type, "text/plain");

        assert_eq!(flake.programming_language.as_deref(), Some("Nix"));
        assert!(flake.is_source);
        assert_eq!(flake.mime_type, "text/plain");

        assert_eq!(gitmodules.programming_language, None);
        assert!(gitmodules.is_text);
        assert!(!gitmodules.is_source);
        assert_eq!(gitmodules.file_type, "Git configuration text");
    }

    #[test]
    fn test_classify_file_info_labels_javascript_shebang_scripts() {
        let classification = classify_file_info(
            Path::new("bin/run"),
            b"#!/usr/bin/env node\nconsole.log('hello');\n",
        );

        assert_eq!(
            classification.programming_language.as_deref(),
            Some("JavaScript")
        );
        assert!(classification.is_script);
        assert_eq!(
            classification.file_type,
            "javascript script, UTF-8 Unicode text executable"
        );
    }

    #[test]
    fn test_classify_file_info_uses_non_utf8_text_labels_for_latin1_scripts() {
        let classification = classify_file_info(
            Path::new("script.py"),
            b"# coding: latin-1\nprint(\"caf\xe9\")\n",
        );

        assert_eq!(
            classification.programming_language.as_deref(),
            Some("Python")
        );
        assert!(classification.is_script);
        assert_eq!(classification.file_type, "python script, text executable");
    }

    #[test]
    fn test_classify_file_info_treats_textual_tga_as_media() {
        let classification = classify_file_info(Path::new("texture.tga"), b"not really a tga\n");

        assert!(classification.is_media);
        assert!(classification.is_text);
        assert!(!classification.is_binary);
    }

    #[test]
    fn test_classify_file_info_keeps_binaryish_source_extension_out_of_text_path() {
        let classification =
            classify_file_info(Path::new("main.ts"), &[0x80, 0x81, 0x82, 0x83, 0x84, 0x85]);

        assert!(classification.is_binary);
        assert!(!classification.is_text);
        assert!(!classification.is_source);
        assert_eq!(classification.programming_language, None);
    }

    #[test]
    fn test_extract_text_for_detection_skips_unsupported_image_formats() {
        let gif_bytes = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff,\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02D\x01\x00;";

        let (text, kind) = extract_text_for_detection(Path::new("tiny.gif"), gif_bytes);

        assert!(text.is_empty());
        assert_eq!(kind, ExtractedTextKind::None);
    }

    #[test]
    fn test_classify_file_info_preserves_language_detection_precedence_matrix() {
        let cases = [
            (
                Path::new("bin/run"),
                b"#!/usr/bin/env node\nconsole.log('hello');\n".as_slice(),
                Some("JavaScript"),
                true,
                true,
            ),
            (
                Path::new("Dockerfile"),
                b"FROM scratch\n".as_slice(),
                Some("Dockerfile"),
                true,
                false,
            ),
            (
                Path::new("package.json"),
                br#"{"name":"demo"}"#.as_slice(),
                None,
                false,
                false,
            ),
            (
                Path::new("config.yaml"),
                b"key: value\n".as_slice(),
                None,
                false,
                false,
            ),
            (
                Path::new("Makefile"),
                b"all:\n\techo hi\n".as_slice(),
                None,
                false,
                false,
            ),
        ];

        for (path, bytes, expected_language, expected_is_source, expected_is_script) in cases {
            let classification = classify_file_info(path, bytes);

            assert_eq!(
                classification.programming_language.as_deref(),
                expected_language,
                "unexpected language for {}",
                path.display()
            );
            assert_eq!(
                classification.is_source,
                expected_is_source,
                "unexpected is_source for {}",
                path.display()
            );
            assert_eq!(
                classification.is_script,
                expected_is_script,
                "unexpected is_script for {}",
                path.display()
            );
        }
    }
}
