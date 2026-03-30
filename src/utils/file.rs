use std::collections::BTreeSet;
use std::fs;
use std::io::{BufReader, Cursor, Read};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;

use chrono::{TimeZone, Utc};
use flate2::read::ZlibDecoder;
use glob::Pattern;
use image::{ImageDecoder, ImageFormat, ImageReader};
use quick_xml::events::Event;
use quick_xml::reader::Reader as XmlReader;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtractedTextKind {
    None,
    Decoded,
    Pdf,
    BinaryStrings,
    ImageMetadata,
}

const MAX_IMAGE_METADATA_VALUES: usize = 64;
const MAX_IMAGE_METADATA_TEXT_BYTES: usize = 32 * 1024;

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
            // Binary heuristic: >10% control chars (0x00-0x08, 0x0E-0x1F) means binary.
            let control_count = bytes
                .iter()
                .filter(|&&b| b < 0x09 || (b > 0x0D && b < 0x20))
                .count();
            if control_count > bytes.len() / 10 {
                return String::new();
            }
            bytes.iter().map(|&b| b as char).collect()
        }
    }
}

pub fn extract_text_for_detection(path: &Path, bytes: &[u8]) -> (String, ExtractedTextKind) {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase());

    if matches!(ext.as_deref(), Some("pdf")) {
        let text = extract_pdf_text(path, bytes);
        return if text.is_empty() {
            (String::new(), ExtractedTextKind::None)
        } else {
            (text, ExtractedTextKind::Pdf)
        };
    }

    if let Some(format) = supported_image_metadata_format(ext.as_deref()) {
        let text = extract_image_metadata_text(bytes, format);
        return if text.is_empty() {
            if is_supported_image_container(bytes, format) {
                (String::new(), ExtractedTextKind::None)
            } else {
                let decoded = decode_bytes_to_string(bytes);
                if decoded.is_empty() {
                    (String::new(), ExtractedTextKind::None)
                } else {
                    (decoded, ExtractedTextKind::Decoded)
                }
            }
        } else {
            (text, ExtractedTextKind::ImageMetadata)
        };
    }

    let decoded = decode_bytes_to_string(bytes);
    if !decoded.is_empty() {
        return (decoded, ExtractedTextKind::Decoded);
    }

    if matches!(ext.as_deref(), Some("jar")) && is_zip_archive(bytes) {
        return (String::new(), ExtractedTextKind::None);
    }

    // Skip string extraction for PDFs - they have their own text extraction above
    // and we don't want to extract strings from PDF binary content
    if matches!(ext.as_deref(), Some("pdf")) {
        return (String::new(), ExtractedTextKind::None);
    }

    let text = extract_printable_strings(bytes);
    if text.is_empty() {
        (String::new(), ExtractedTextKind::None)
    } else {
        (text, ExtractedTextKind::BinaryStrings)
    }
}

fn supported_image_metadata_format(ext: Option<&str>) -> Option<ImageFormat> {
    match ext? {
        "jpg" | "jpeg" => Some(ImageFormat::Jpeg),
        "png" => Some(ImageFormat::Png),
        "tif" | "tiff" => Some(ImageFormat::Tiff),
        "webp" => Some(ImageFormat::WebP),
        _ => None,
    }
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

fn extract_pdf_text(path: &Path, bytes: &[u8]) -> String {
    if bytes.len() < 5 || &bytes[..5] != b"%PDF-" {
        return String::new();
    }

    let extracted = catch_unwind(AssertUnwindSafe(|| pdf_extract::extract_text(path)));
    if let Ok(Ok(text)) = extracted {
        let normalized = text.replace(['\r', '\u{0c}'], "\n");
        if !normalized.trim().is_empty() {
            return normalized;
        }
    }

    let extracted = catch_unwind(AssertUnwindSafe(
        || -> Result<String, Box<dyn std::error::Error>> {
            let mut document = pdf_oxide::document::PdfDocument::open(path)?;
            let page_count = document.page_count()?;
            let mut pages = Vec::new();
            for page_idx in 0..page_count {
                let text = document.extract_text(page_idx)?;
                if !text.trim().is_empty() {
                    pages.push(text);
                }
            }
            Ok(pages.join("\n"))
        },
    ));
    if let Ok(Ok(text)) = extracted {
        let normalized = text.replace(['\r', '\u{0c}'], "\n");
        if !normalized.trim().is_empty() {
            return normalized;
        }
    }

    let extracted = catch_unwind(AssertUnwindSafe(
        || -> Result<String, Box<dyn std::error::Error>> {
            let mut document = pdf_oxide::document::PdfDocument::from_bytes(bytes.to_vec())?;
            let page_count = document.page_count()?;
            let mut pages = Vec::new();
            for page_idx in 0..page_count {
                let text = document.extract_text(page_idx)?;
                if !text.trim().is_empty() {
                    pages.push(text);
                }
            }
            Ok(pages.join("\n"))
        },
    ));
    if let Ok(Ok(text)) = extracted {
        let normalized = text.replace(['\r', '\u{0c}'], "\n");
        if !normalized.trim().is_empty() {
            return normalized;
        }
    }

    let extracted = catch_unwind(AssertUnwindSafe(|| {
        pdf_extract::extract_text_from_mem(bytes)
    }));
    if let Ok(Ok(text)) = extracted {
        let normalized = text.replace(['\r', '\u{0c}'], "\n");
        if !normalized.trim().is_empty() {
            return normalized;
        }
    }

    let extracted = catch_unwind(AssertUnwindSafe(|| {
        pdf_extract::extract_text_from_mem_by_pages(bytes)
    }));
    match extracted {
        Ok(Ok(pages)) => {
            let normalized = pages
                .into_iter()
                .map(|page| page.replace(['\r', '\u{0c}'], "\n"))
                .collect::<Vec<_>>()
                .join("\n");
            if normalized.trim().is_empty() {
                String::new()
            } else {
                normalized
            }
        }
        Ok(Err(_)) | Err(_) => String::new(),
    }
}

fn is_zip_archive(bytes: &[u8]) -> bool {
    bytes.starts_with(b"PK\x03\x04")
        || bytes.starts_with(b"PK\x05\x06")
        || bytes.starts_with(b"PK\x07\x08")
}

pub fn extract_printable_strings(bytes: &[u8]) -> String {
    const MIN_LEN: usize = 4;
    const MAX_OUTPUT_BYTES: usize = 2_000_000;

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
            if out.len() >= MAX_OUTPUT_BYTES {
                return out;
            }
        }
    }
    flush_run(&mut out, &mut run);
    if out.len() >= MAX_OUTPUT_BYTES {
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
                if out.len() >= MAX_OUTPUT_BYTES {
                    return out;
                }
            }
            i += 2;
        }
        flush_run(&mut out, &mut run);
        if out.len() >= MAX_OUTPUT_BYTES {
            return out;
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{ExtractedTextKind, extract_text_for_detection};

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
}
