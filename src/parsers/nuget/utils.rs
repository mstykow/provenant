use std::collections::HashMap;

pub(super) fn resolve_string_property_reference(
    value: &str,
    properties: &HashMap<String, String>,
) -> Option<String> {
    let trimmed = value.trim();
    if let Some(property_name) = trimmed
        .strip_prefix("$(")
        .and_then(|value| value.strip_suffix(')'))
    {
        properties.get(property_name).cloned()
    } else {
        Some(trimmed.to_string())
    }
}

pub(super) fn resolve_bool_property_reference(
    value: Option<&str>,
    properties: &HashMap<String, String>,
) -> Option<bool> {
    let resolved = resolve_string_property_reference(value?, properties)?;
    Some(resolved.eq_ignore_ascii_case("true"))
}

pub(super) fn resolve_optional_property_value(
    value: Option<&str>,
    properties: &HashMap<String, String>,
) -> Option<String> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }

    if value.starts_with("$(") && value.ends_with(')') {
        resolve_string_property_reference(value, properties)
    } else {
        Some(value.to_string())
    }
}
