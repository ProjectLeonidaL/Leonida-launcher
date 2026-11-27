//! Dates for file names, reports and labels, without a date crate. All in UTC: a date that is
//! off by one around midnight changes nothing a reader acts on.

use std::time::{SystemTime, UNIX_EPOCH};

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Year, month and day of a day count since 1970-01-01 (Howard Hinnant's civil_from_days).
pub fn civil(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

fn seconds(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(0))
}

/// "2026-11-27"
pub fn iso_date(time: SystemTime) -> String {
    let (y, m, d) = civil(seconds(time).div_euclid(86_400));
    format!("{y:04}-{m:02}-{d:02}")
}

/// "20261127-180512", for folder names.
pub fn stamp(time: SystemTime) -> String {
    let secs = seconds(time);
    let (y, m, d) = civil(secs.div_euclid(86_400));
    let s = secs.rem_euclid(86_400);
    format!(
        "{y:04}{m:02}{d:02}-{:02}{:02}{:02}",
        s / 3600,
        s / 60 % 60,
        s % 60
    )
}

/// "27 Nov"
pub fn day_month(time: SystemTime) -> String {
    let (_, m, d) = civil(seconds(time).div_euclid(86_400));
    format!("{d} {}", MONTHS[(m - 1) as usize])
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn dates_from_day_counts() {
        assert_eq!(civil(0), (1970, 1, 1));
        assert_eq!(civil(20_419), (2025, 11, 27));
        assert_eq!(civil(11_016), (2000, 2, 29));
        let t = UNIX_EPOCH + Duration::from_secs(1_795_802_712);
        assert_eq!(iso_date(t), "2026-11-27");
        assert_eq!(stamp(t), "20261127-180512");
        assert_eq!(day_month(t), "27 Nov");
    }
}
