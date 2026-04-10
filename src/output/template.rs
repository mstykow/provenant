use std::fs;
use std::io::{self, Write};

use tera::{Context, Tera};

use crate::output_schema::Output;

use super::OutputWriteConfig;
use super::shared::io_other;

pub(crate) fn write_custom_template(
    output: &Output,
    writer: &mut dyn Write,
    config: &OutputWriteConfig,
) -> io::Result<()> {
    let template_path = config.custom_template.as_ref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "--custom-template path is required for custom template output",
        )
    })?;

    let template = fs::read_to_string(template_path)?;
    let output_value = serde_json::to_value(output).map_err(io_other)?;
    let mut context = Context::new();
    context.insert("output", &output_value);
    context.insert("headers", &output.headers);
    context.insert("files", &output.files);
    context.insert("packages", &output.packages);
    context.insert("dependencies", &output.dependencies);

    let rendered = Tera::one_off(&template, &context, false).map_err(io_other)?;
    writer.write_all(rendered.as_bytes())
}
