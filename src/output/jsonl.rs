// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::io::{self, Write};

use crate::output_schema::Output;

use super::public_serialize::{
    PublicPackages, PublicTopLevelDependencies, SingleField, SinglePublicFile,
};
use super::shared::{io_other, sorted_files};

pub(crate) fn write_json_lines(output: &Output, writer: &mut dyn Write) -> io::Result<()> {
    write_jsonl_line(writer, &SingleField::new("headers", &output.headers))?;

    if let Some(summary) = &output.summary {
        write_jsonl_line(writer, &SingleField::new("summary", summary))?;
    }

    if let Some(tallies) = &output.tallies {
        write_jsonl_line(writer, &SingleField::new("tallies", tallies))?;
    }

    if let Some(tallies_of_key_files) = &output.tallies_of_key_files {
        write_jsonl_line(
            writer,
            &SingleField::new("tallies_of_key_files", tallies_of_key_files),
        )?;
    }

    if let Some(tallies_by_facet) = &output.tallies_by_facet {
        write_jsonl_line(
            writer,
            &SingleField::new("tallies_by_facet", tallies_by_facet),
        )?;
    }

    if !output.packages.is_empty() {
        write_jsonl_line(
            writer,
            &SingleField::new("packages", PublicPackages(&output.packages)),
        )?;
    }

    if !output.dependencies.is_empty() {
        write_jsonl_line(
            writer,
            &SingleField::new(
                "dependencies",
                PublicTopLevelDependencies(&output.dependencies),
            ),
        )?;
    }

    write_jsonl_line(
        writer,
        &SingleField::new("license_detections", &output.license_detections),
    )?;

    if !output.license_references.is_empty() {
        write_jsonl_line(
            writer,
            &SingleField::new("license_references", &output.license_references),
        )?;
    }

    if !output.license_rule_references.is_empty() {
        write_jsonl_line(
            writer,
            &SingleField::new("license_rule_references", &output.license_rule_references),
        )?;
    }

    for file in sorted_files(&output.files) {
        write_jsonl_line(writer, &SingleField::new("files", SinglePublicFile(file)))?;
    }

    Ok(())
}

fn write_jsonl_line<T>(writer: &mut dyn Write, value: &T) -> io::Result<()>
where
    T: serde::Serialize,
{
    serde_json::to_writer(&mut *writer, value).map_err(io_other)?;
    writer.write_all(b"\n")
}
