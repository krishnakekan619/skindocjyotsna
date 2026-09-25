//! UTC timestamps without an extra date/time dependency.

use std::time::{SystemTime, UNIX_EPOCH};

pub struct UtcStamp {
    /// `20260925-081530`, used in file names (sorts chronologically).
    pub compact: String,
    /// `2026-09-25T08:15:30Z`, stored in backup metadata.
    pub iso: String,
}

pub fn utc_stamp(time: SystemTime) -> UtcStamp {
    let secs = time.duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0) as i64;
    let (year, month, day) = civil_from_days(secs.div_euclid(86_400));
    let rem = secs.rem_euclid(86_400);
    let (h, m, s) = (rem / 3600, (rem % 3600) / 60, rem % 60);
    UtcStamp {
        compact: format!("{year:04}{month:02}{day:02}-{h:02}{m:02}{s:02}"),
        iso: format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z"),
    }
}

/// Days since 1970-01-01 -> (year, month, day). Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn formats_known_instants() {
        let at = |secs| utc_stamp(UNIX_EPOCH + Duration::from_secs(secs));
        assert_eq!(at(0).iso, "1970-01-01T00:00:00Z");
        assert_eq!(at(951_782_400).iso, "2000-02-29T00:00:00Z"); // leap day
        let stamp = at(1_790_323_530); // 2026-09-25T08:05:30Z
        assert_eq!(stamp.iso, "2026-09-25T08:05:30Z");
        assert_eq!(stamp.compact, "20260925-080530");
    }
}
