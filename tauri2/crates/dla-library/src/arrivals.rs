//! Dates for "Date added": the times dlminer writes into manifests and the
//! servers report, and the months they fall in. Month folders hold works by
//! release date, so a work added today can land in any of them.

use std::time::{SystemTime, UNIX_EPOCH};

pub fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs() as i64)
}

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Seconds since 1970 for a file time as the archive's services report it:
/// an HTTP date (`Tue, 29 Sep 2026 20:23:00 GMT`) from WebDAV listings, or
/// RFC 3339 (`2026-09-29T20:23:00Z`) from the download service. `None` for
/// anything else.
pub fn server_time_seconds(value: &str) -> Option<i64> {
    http_date(value).or_else(|| rfc3339(value))
}

fn http_date(value: &str) -> Option<i64> {
    let mut parts = value.split_whitespace();
    parts.next().filter(|weekday| weekday.ends_with(','))?;
    let day: i64 = parts.next()?.parse().ok()?;
    let month = parts.next()?;
    let month = MONTHS.iter().position(|name| *name == month)? as i64 + 1;
    let year: i64 = parts.next()?.parse().ok()?;
    let mut clock = parts
        .next()?
        .split(':')
        .map(|part| part.parse::<i64>().ok());
    let (hour, minute, second) = (clock.next()??, clock.next()??, clock.next()??);
    if parts.next()? != "GMT" || parts.next().is_some() || clock.next().is_some() {
        return None;
    }
    seconds(year, month, day, hour, minute, second)
}

/// `2026-09-29T20:23:00Z`, with optional fractions of a second and either `Z`
/// or an offset such as `+09:00`.
fn rfc3339(value: &str) -> Option<i64> {
    let (date, rest) = value.split_once(['T', 't'])?;
    let mut parts = date.split('-');
    let year = digits(parts.next()?, 4)?;
    let month = digits(parts.next()?, 2)?;
    let day = digits(parts.next()?, 2)?;
    if parts.next().is_some() {
        return None;
    }
    let (clock, zone) = rest.split_at(rest.find(['Z', 'z', '+', '-'])?);
    let (clock, fraction) = clock.split_once('.').unwrap_or((clock, "0"));
    if fraction.is_empty() || !fraction.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    let mut fields = clock.split(':');
    let hour = digits(fields.next()?, 2)?;
    let minute = digits(fields.next()?, 2)?;
    let second = digits(fields.next()?, 2)?;
    if fields.next().is_some() {
        return None;
    }
    let offset = match zone {
        "Z" | "z" => 0,
        _ => {
            let (hours, minutes) = zone[1..].split_once(':')?;
            let offset = digits(hours, 2)? * 3600 + digits(minutes, 2)? * 60;
            if zone.starts_with('-') {
                -offset
            } else {
                offset
            }
        }
    };
    Some(seconds(year, month, day, hour, minute, second)? - offset)
}

/// A number written with exactly `width` digits.
fn digits(text: &str, width: usize) -> Option<i64> {
    if text.len() == width && text.bytes().all(|byte| byte.is_ascii_digit()) {
        text.parse().ok()
    } else {
        None
    }
}

fn seconds(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: i64) -> Option<i64> {
    let valid = (1..=12).contains(&month)
        && (1..=31).contains(&day)
        && (0..24).contains(&hour)
        && (0..60).contains(&minute)
        && (0..=60).contains(&second);
    valid.then(|| days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

/// `YYYY-MM` of a time in seconds since 1970.
pub fn month_of(seconds: i64) -> String {
    let (year, month) = civil_from_days(seconds.div_euclid(86_400));
    format!("{year:04}-{month:02}")
}

/// The first second of a `YYYY-MM` month and the first of the next one.
pub fn month_range(month: &str) -> Option<(i64, i64)> {
    let (year, number) = month.split_once('-')?;
    let (year, number) = (digits(year, 4)?, digits(number, 2)?);
    if !(1..=12).contains(&number) {
        return None;
    }
    let (next_year, next) = if number == 12 {
        (year + 1, 1)
    } else {
        (year, number + 1)
    };
    Some((
        days_from_civil(year, number, 1) * 86_400,
        days_from_civil(next_year, next, 1) * 86_400,
    ))
}

/// Year and month of a day counted from 1970-01-01 (Howard Hinnant's
/// `civil_from_days`, without the day).
fn civil_from_days(days: i64) -> (i64, i64) {
    let shifted = days + 719_468;
    let era = shifted.div_euclid(146_097);
    let day_of_era = shifted - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let month = if shifted_month < 10 {
        shifted_month + 3
    } else {
        shifted_month - 9
    };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month)
}

/// Days since 1970-01-01 for a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = if month <= 2 { year - 1 } else { year };
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let day_of_year = (153 * ((month + 9) % 12) + 2) / 5 + day - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_dates_webdav_listings_use() {
        assert_eq!(
            server_time_seconds("Thu, 01 Jan 1970 00:00:00 GMT"),
            Some(0)
        );
        assert_eq!(
            server_time_seconds("Tue, 29 Sep 2026 20:23:00 GMT"),
            Some(1_790_713_380)
        );
        assert_eq!(
            server_time_seconds("Thu, 29 Feb 2024 23:59:59 GMT"),
            Some(1_709_251_199)
        );
        assert_eq!(
            server_time_seconds("Tue, 07 Feb 2023 08:47:25 GMT"),
            Some(1_675_759_645)
        );
    }

    #[test]
    fn reads_the_times_the_download_service_sends() {
        for value in [
            "2026-09-29T20:23:00Z",
            "2026-09-29T20:23:00.123456789Z",
            "2026-09-30T05:23:00+09:00",
            "2026-09-29T15:23:00-05:00",
        ] {
            assert_eq!(server_time_seconds(value), Some(1_790_713_380), "{value}");
        }
    }

    #[test]
    fn names_months_and_their_bounds() {
        assert_eq!(month_of(0), "1970-01");
        assert_eq!(month_of(1_790_713_380), "2026-09");
        assert_eq!(month_of(-1), "1969-12");
        assert_eq!(month_of(1_709_251_199), "2024-02");
        assert_eq!(month_of(1_709_251_200), "2024-03");
        let (start, end) = month_range("2024-02").unwrap();
        assert_eq!(
            (month_of(start), month_of(end - 1), month_of(end)),
            ("2024-02".into(), "2024-02".into(), "2024-03".into())
        );
        assert_eq!(end - start, 29 * 86_400);
        let (start, end) = month_range("2026-12").unwrap();
        assert_eq!(
            (month_of(start), month_of(end)),
            ("2026-12".into(), "2027-01".into())
        );
        for month in ["2026-13", "2026-00", "26-01", "2026-1", "2026", "nonsense"] {
            assert_eq!(month_range(month), None, "{month}");
        }
    }

    #[test]
    fn refuses_anything_else() {
        for value in [
            "",
            "t1",
            "2026-09-29",
            "2026-09-29T20:23:00",
            "2026-13-01T00:00:00Z",
            "2026-9-29T20:23:00Z",
            "2026-09-29T20:23:00.Z",
            "2026-09-29T20:23:00+0900",
            "2026-09-29T20:23Z",
            "Tue, 29 Sep 2026 20:23:00",
            "Tue, 29 Sep 2026 20:23:00 CET",
            "Tue, 29 Foo 2026 20:23:00 GMT",
            "Tue, 32 Sep 2026 20:23:00 GMT",
            "Tue, 29 Sep 2026 24:00:00 GMT",
            "Tue, 29 Sep 2026 20:23 GMT",
            "Tue, 29 Sep 2026 20:23:00:00 GMT",
            "Tue, 29 Sep 2026 20:23:00 GMT extra",
        ] {
            assert_eq!(server_time_seconds(value), None, "{value}");
        }
    }
}
