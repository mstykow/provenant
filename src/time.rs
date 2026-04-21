// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use chrono::{DateTime, Datelike, Timelike, Utc};

pub(crate) fn format_scancode_timestamp(timestamp: &DateTime<Utc>) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}{:02}{:02}.{:06}",
        timestamp.year(),
        timestamp.month(),
        timestamp.day(),
        timestamp.hour(),
        timestamp.minute(),
        timestamp.second(),
        timestamp.timestamp_subsec_micros()
    )
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Timelike, Utc};

    use super::format_scancode_timestamp;

    #[test]
    fn format_scancode_timestamp_uses_compact_microsecond_precision() {
        let timestamp = Utc
            .with_ymd_and_hms(2026, 4, 11, 9, 18, 28)
            .single()
            .expect("timestamp should be valid")
            .with_nanosecond(24_390_124)
            .expect("nanoseconds should be valid");

        assert_eq!(
            format_scancode_timestamp(&timestamp),
            "2026-04-11T091828.024390"
        );
    }
}
