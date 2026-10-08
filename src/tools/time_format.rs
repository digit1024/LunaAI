//! Shared time stack for the embedded `time` tool and time-awareness context.
//!
//! Both paths resolve the local IANA zone and format datetimes through this
//! module so there is a single source of truth for timezone handling.

use chrono::{DateTime, Local, Utc};

/// Resolve the system's local IANA timezone name (e.g. "Europe/Warsaw").
///
/// Falls back to `"UTC"` when the platform cannot report a zone.
pub fn resolve_local_tz_name() -> String {
    match iana_time_zone::get_timezone() {
        Ok(name) if !name.trim().is_empty() => name,
        _ => "UTC".to_string(),
    }
}

/// Resolve a configured timezone label: `"local"`/empty => system zone, else as-is.
pub fn resolve_configured_tz(label: &str) -> String {
    let trimmed = label.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("local") {
        resolve_local_tz_name()
    } else {
        trimmed.to_string()
    }
}

/// Parse an IANA timezone name into a `chrono_tz::Tz`.
pub fn parse_tz(name: &str) -> Result<chrono_tz::Tz, String> {
    name.trim()
        .parse::<chrono_tz::Tz>()
        .map_err(|e| format!("Invalid timezone '{}': {}", name, e))
}

/// Format a UTC instant as either local or UTC, matching time-awareness output.
pub fn format_utc_or_local(dt: DateTime<Utc>, local: bool) -> String {
    if local {
        dt.with_timezone(&Local)
            .format("%Y-%m-%d %H:%M %Z (%A)")
            .to_string()
    } else {
        dt.format("%Y-%m-%d %H:%M UTC (%A)").to_string()
    }
}
