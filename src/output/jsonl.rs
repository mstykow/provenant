use std::io::{self, Write};

use serde_json::{Value, json};

use crate::models::Output;

use super::{
    OutputWriteConfig, TimedFileInfo,
    shared::{io_other, sorted_files},
};

pub(crate) fn write_json_lines(
    output: &Output,
    writer: &mut dyn Write,
    config: &OutputWriteConfig,
) -> io::Result<()> {
    write_jsonl_line(writer, &json!({ "headers": output.headers }))?;

    if let Some(summary) = &output.summary {
        write_jsonl_line(writer, &json!({ "summary": summary }))?;
    }

    if let Some(tallies) = &output.tallies {
        write_jsonl_line(writer, &json!({ "tallies": tallies }))?;
    }

    if let Some(tallies_of_key_files) = &output.tallies_of_key_files {
        write_jsonl_line(
            writer,
            &json!({ "tallies_of_key_files": tallies_of_key_files }),
        )?;
    }

    if let Some(tallies_by_facet) = &output.tallies_by_facet {
        write_jsonl_line(writer, &json!({ "tallies_by_facet": tallies_by_facet }))?;
    }

    if !output.packages.is_empty() {
        write_jsonl_line(writer, &json!({ "packages": output.packages }))?;
    }

    if !output.dependencies.is_empty() {
        write_jsonl_line(writer, &json!({ "dependencies": output.dependencies }))?;
    }

    if !output.license_detections.is_empty() {
        write_jsonl_line(
            writer,
            &json!({ "license_detections": output.license_detections }),
        )?;
    }

    if !output.license_references.is_empty() {
        write_jsonl_line(
            writer,
            &json!({ "license_references": output.license_references }),
        )?;
    }

    if !output.license_rule_references.is_empty() {
        write_jsonl_line(
            writer,
            &json!({ "license_rule_references": output.license_rule_references }),
        )?;
    }

    let include_timings = config.file_scan_timings.is_some();
    for file in sorted_files(&output.files) {
        let file_scan_timing = config
            .file_scan_timings
            .as_deref()
            .and_then(|timings| timings.get(&file.path));
        let timed_file = serde_json::to_value(TimedFileInfo {
            file,
            file_scan_timing,
            include_timings,
        })
        .map_err(io_other)?;
        write_jsonl_line(writer, &json!({ "files": [timed_file] }))?;
    }

    Ok(())
}

fn write_jsonl_line(writer: &mut dyn Write, value: &Value) -> io::Result<()> {
    let line = serde_json::to_string(value).map_err(io_other)?;
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")
}
