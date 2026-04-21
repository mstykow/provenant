// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

pub const MAX_BUILD_VERSION_LEN: usize = 128;

pub fn sanitize_build_version(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.len() > MAX_BUILD_VERSION_LEN {
        return None;
    }

    if trimmed
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '-' | '_' | '+'))
    {
        Some(trimmed.to_string())
    } else {
        None
    }
}

pub fn derive_build_version(
    package_version: &str,
    describe: Option<&str>,
    short_sha: Option<&str>,
) -> String {
    let expected_tag = format!("v{package_version}");

    match describe.and_then(sanitize_build_version) {
        Some(describe) if describe == expected_tag => package_version.to_string(),
        Some(describe) if describe == format!("{expected_tag}-dirty") => short_sha
            .and_then(sanitize_build_version)
            .map(|sha| format!("{package_version}-0-g{sha}-dirty"))
            .unwrap_or_else(|| format!("{package_version}-dirty")),
        Some(describe) if describe.starts_with(&expected_tag) => {
            format!("{package_version}{}", &describe[expected_tag.len()..])
        }
        Some(describe) => format!("{package_version}-g{}", describe.trim_start_matches('g')),
        None => package_version.to_string(),
    }
}
