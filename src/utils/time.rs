use chrono::{DateTime, Datelike, NaiveDateTime, Timelike, Utc};

const ISO_UTC_TIMESTAMP_FALLBACK: &str = "1970-01-01T00:00:00Z";

pub(crate) fn fallback_iso_utc_timestamp() -> &'static str {
    ISO_UTC_TIMESTAMP_FALLBACK
}

pub(crate) fn convert_header_timestamp_to_iso_utc(value: &str) -> Option<String> {
    parse_header_timestamp(value).map(|timestamp| format_iso_utc_timestamp(&timestamp))
}

fn format_iso_utc_timestamp(timestamp: &DateTime<Utc>) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        timestamp.year(),
        timestamp.month(),
        timestamp.day(),
        timestamp.hour(),
        timestamp.minute(),
        timestamp.second()
    )
}

fn parse_header_timestamp(value: &str) -> Option<DateTime<Utc>> {
    if let Ok(timestamp) = DateTime::parse_from_rfc3339(value) {
        return Some(timestamp.with_timezone(&Utc));
    }

    NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H%M%S%.f")
        .ok()
        .map(|timestamp| DateTime::<Utc>::from_naive_utc_and_offset(timestamp, Utc))
}

#[cfg(test)]
mod tests {
    use super::convert_header_timestamp_to_iso_utc;

    #[test]
    fn convert_header_timestamp_to_iso_utc_accepts_scancode_and_rfc3339_inputs() {
        assert_eq!(
            convert_header_timestamp_to_iso_utc("2026-04-11T091828.024390"),
            Some("2026-04-11T09:18:28Z".to_string())
        );
        assert_eq!(
            convert_header_timestamp_to_iso_utc("2026-04-11T09:18:28.024390124+00:00"),
            Some("2026-04-11T09:18:28Z".to_string())
        );
        assert_eq!(
            convert_header_timestamp_to_iso_utc("2026-04-11T09:18:28.024390124Z"),
            Some("2026-04-11T09:18:28Z".to_string())
        );
    }
}
