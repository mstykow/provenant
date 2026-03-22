use std::io::{self, Write};

use serde_json::{Value, json};

use crate::models::Output;

use super::shared::{io_other, sorted_files};

pub(crate) fn write_json_lines(output: &Output, writer: &mut dyn Write) -> io::Result<()> {
    write_jsonl_line(writer, &json!({ "headers": output.headers }))?;

    if let Some(summary) = &output.summary {
        write_jsonl_line(writer, &json!({ "summary": summary }))?;
    }

    if let Some(tallies) = &output.tallies {
        write_jsonl_line(writer, &json!({ "tallies": tallies }))?;
    }

    if !output.packages.is_empty() {
        write_jsonl_line(writer, &json!({ "packages": output.packages }))?;
    }

    if !output.dependencies.is_empty() {
        write_jsonl_line(writer, &json!({ "dependencies": output.dependencies }))?;
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

    for file in sorted_files(&output.files) {
        write_jsonl_line(writer, &json!({ "files": [file] }))?;
    }

    Ok(())
}

fn write_jsonl_line(writer: &mut dyn Write, value: &Value) -> io::Result<()> {
    let line = serde_json::to_string(value).map_err(io_other)?;
    writer.write_all(line.as_bytes())?;
    writer.write_all(b"\n")
}
