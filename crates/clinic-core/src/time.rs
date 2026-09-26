//! Calendar dates in the clinic's local time, without a date/time dependency.
//!
//! The clinic's UTC offset is a setting (default +05:30, India, no daylight saving). Dates are
//! used for "today", expiry checks, report ranges and the financial year in bill numbers.

use std::fmt;

/// India Standard Time, UTC+05:30.
pub const DEFAULT_UTC_OFFSET_MINUTES: i32 = 330;

const SECONDS_PER_DAY: i64 = 86_400;
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

fn is_leap(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        _ => 28,
    }
}

impl Date {
    pub fn new(year: i32, month: u32, day: u32) -> Option<Date> {
        ((1..=12).contains(&month) && day >= 1 && day <= days_in_month(year, month) && (1900..=9999).contains(&year))
            .then_some(Date { year, month, day })
    }

    /// Strict `YYYY-MM-DD`.
    pub fn parse(text: &str) -> Option<Date> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        let year = text.get(0..4)?.parse().ok()?;
        let month = text.get(5..7)?.parse().ok()?;
        let day = text.get(8..10)?.parse().ok()?;
        Date::new(year, month, day)
    }

    /// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
    pub fn days_since_epoch(self) -> i64 {
        let (month, day) = (i64::from(self.month), i64::from(self.day));
        let year = i64::from(self.year) - i64::from(self.month <= 2);
        let era = year.div_euclid(400);
        let yoe = year.rem_euclid(400);
        let mp = (month + 9) % 12;
        let doy = (153 * mp + 2) / 5 + day - 1;
        let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
        era * 146_097 + doe - 719_468
    }

    /// Inverse of `days_since_epoch` (Howard Hinnant's `civil_from_days`).
    pub fn from_days(days: i64) -> Date {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z.rem_euclid(146_097);
        let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
        let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
        let year = (yoe + era * 400 + i64::from(month <= 2)) as i32;
        Date { year, month, day }
    }

    pub fn add_days(self, days: i64) -> Date {
        Date::from_days(self.days_since_epoch() + days)
    }

    /// Indian financial year (1 April - 31 March) as used in bill numbers: `26-27`.
    pub fn financial_year(self) -> String {
        let start = if self.month >= 4 { self.year } else { self.year - 1 };
        format!("{:02}-{:02}", start.rem_euclid(100), (start + 1).rem_euclid(100))
    }

    /// `25-Sep-2026`, as printed on receipts.
    pub fn display(self) -> String {
        format!("{:02}-{}-{}", self.day, MONTHS[(self.month - 1) as usize], self.year)
    }
}

impl fmt::Display for Date {
    /// `YYYY-MM-DD`, the stored form.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// The clinic's local calendar date at `now_utc` (Unix seconds).
pub fn local_date(now_utc: i64, utc_offset_minutes: i32) -> Date {
    Date::from_days((now_utc + i64::from(utc_offset_minutes) * 60).div_euclid(SECONDS_PER_DAY))
}

/// Unix seconds (UTC) at the start of `date` in the clinic's local time.
pub fn local_day_start_utc(date: Date, utc_offset_minutes: i32) -> i64 {
    date.days_since_epoch() * SECONDS_PER_DAY - i64::from(utc_offset_minutes) * 60
}

/// `25-Sep-2026 10:42` in the clinic's local time.
pub fn format_local_datetime(timestamp_utc: i64, utc_offset_minutes: i32) -> String {
    let local = timestamp_utc + i64::from(utc_offset_minutes) * 60;
    let seconds_of_day = local.rem_euclid(SECONDS_PER_DAY);
    format!(
        "{} {:02}:{:02}",
        Date::from_days(local.div_euclid(SECONDS_PER_DAY)).display(),
        seconds_of_day / 3600,
        (seconds_of_day % 3600) / 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_only_real_dates() {
        assert_eq!(Date::parse("2026-09-25"), Some(Date { year: 2026, month: 9, day: 25 }));
        assert_eq!(Date::parse("2028-02-29").map(|d| d.day), Some(29));
        for bad in ["2026-02-29", "2026-13-01", "2026-9-25", "25-09-2026", "2026-09-31", ""] {
            assert_eq!(Date::parse(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn day_numbers_round_trip() {
        assert_eq!(Date::new(1970, 1, 1).map(Date::days_since_epoch), Some(0));
        for text in ["2000-02-29", "2026-03-31", "2026-09-25", "2099-12-31"] {
            let date = Date::parse(text).expect("valid");
            assert_eq!(Date::from_days(date.days_since_epoch()), date);
            assert_eq!(date.to_string(), text);
        }
        assert_eq!(Date::parse("2026-02-28").map(|d| d.add_days(1).to_string()).as_deref(), Some("2026-03-01"));
    }

    #[test]
    fn local_time_in_india() {
        // 2026-09-25T20:00:00Z is already 26 September 01:30 in India.
        let ts = Date::parse("2026-09-25").map(|d| d.days_since_epoch() * 86_400 + 20 * 3600).unwrap_or_default();
        assert_eq!(local_date(ts, DEFAULT_UTC_OFFSET_MINUTES).to_string(), "2026-09-26");
        assert_eq!(format_local_datetime(ts, DEFAULT_UTC_OFFSET_MINUTES), "26-Sep-2026 01:30");
        let start = local_day_start_utc(local_date(ts, 330), 330);
        assert!(start <= ts && ts - start < 86_400);
    }

    #[test]
    fn financial_year_changes_on_1_april() {
        assert_eq!(Date::parse("2026-03-31").map(Date::financial_year).as_deref(), Some("25-26"));
        assert_eq!(Date::parse("2026-04-01").map(Date::financial_year).as_deref(), Some("26-27"));
        assert_eq!(Date::parse("2099-06-01").map(Date::financial_year).as_deref(), Some("99-00"));
    }
}
