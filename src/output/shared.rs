// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use std::io;

use crate::output_schema::OutputFileInfo;

pub(crate) fn xml_escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub(crate) fn io_other<E: std::fmt::Display>(error: E) -> io::Error {
    io::Error::other(error.to_string())
}

pub(crate) fn sorted_files(files: &[OutputFileInfo]) -> Vec<&OutputFileInfo> {
    let mut refs = files.iter().collect::<Vec<_>>();
    refs.sort_by(|a, b| a.path.cmp(&b.path));
    refs
}
