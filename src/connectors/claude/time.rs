//! Time utilities for Claude session tracking.

/// Staleness threshold in seconds (4 hours)
pub const STALE_THRESHOLD_SECS: i64 = 14400;

/// Get current time as ISO 8601 string
pub fn now_iso8601() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};

    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();

    let secs = duration.as_secs();

    // Calculate date/time components
    let days = secs / 86400;
    let remaining = secs % 86400;
    let hours = remaining / 3600;
    let minutes = (remaining % 3600) / 60;
    let seconds = remaining % 60;

    // Simplified date calculation (days since 1970-01-01)
    // This is a rough approximation
    let mut year = 1970i32;
    let mut remaining_days = days as i32;

    loop {
        let days_in_year = if is_leap_year(year) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        year += 1;
    }

    let days_in_months: [i32; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };

    let mut month = 1;
    for days_in_month in days_in_months {
        if remaining_days < days_in_month {
            break;
        }
        remaining_days -= days_in_month;
        month += 1;
    }
    let day = remaining_days + 1;

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

pub fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

/// Parse ISO 8601 timestamp to Unix epoch seconds
#[allow(clippy::result_unit_err)]
pub fn parse_iso8601(s: &str) -> Result<i64, ()> {
    // Parse format: "2026-01-18T22:30:00Z"
    if s.len() < 19 {
        return Err(());
    }

    let year: i32 = s[0..4].parse().map_err(|_| ())?;
    let month: i32 = s[5..7].parse().map_err(|_| ())?;
    let day: i32 = s[8..10].parse().map_err(|_| ())?;
    let hour: i32 = s[11..13].parse().map_err(|_| ())?;
    let minute: i32 = s[14..16].parse().map_err(|_| ())?;
    let second: i32 = s[17..19].parse().map_err(|_| ())?;

    // Days from year
    let mut days: i64 = 0;
    for y in 1970..year {
        days += if is_leap_year(y) { 366 } else { 365 };
    }

    // Days from months
    let days_in_months: [i32; 12] = if is_leap_year(year) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    for m in 1..month {
        days += days_in_months[(m - 1) as usize] as i64;
    }

    // Days of month
    days += (day - 1) as i64;

    // Convert to seconds
    let secs = days * 86400 + hour as i64 * 3600 + minute as i64 * 60 + second as i64;

    Ok(secs)
}

/// Format a timestamp as relative time (e.g., "2m ago", "1h ago")
pub fn relative_time(timestamp: &str) -> String {
    let Ok(event_secs) = parse_iso8601(timestamp) else {
        return timestamp.to_string();
    };

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    let diff = now - event_secs;

    if diff < 0 {
        "now".to_string()
    } else if diff < 60 {
        format!("{}s ago", diff)
    } else if diff < 3600 {
        format!("{}m ago", diff / 60)
    } else if diff < 86400 {
        format!("{}h ago", diff / 3600)
    } else {
        format!("{}d ago", diff / 86400)
    }
}
