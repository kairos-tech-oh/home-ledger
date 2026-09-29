//! Calendar dates, for the figures that depend on one: planning windows and
//! spending periods. No date library: the crate stays a pure function of the
//! ledger, and the rules needed here are few enough to write down and test.

/// A calendar date. Only what the figures need: parse, compare, step.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Day(i64);

impl Day {
    /// `yyyy-mm-dd`, and only a date that exists: `2026-02-30` is refused.
    pub fn parse(text: &str) -> Option<Day> {
        let bytes = text.as_bytes();
        if bytes.len() != 10 || bytes[4] != b'-' || bytes[7] != b'-' {
            return None;
        }
        let y: i64 = text[0..4].parse().ok()?;
        let m: i64 = text[5..7].parse().ok()?;
        let d: i64 = text[8..10].parse().ok()?;
        if !(1..=12).contains(&m) || d < 1 || d > days_in_month(y, m) {
            return None;
        }
        Some(Day(days_from_civil(y, m, d)))
    }

    pub fn from_ymd(y: i64, m: i64, d: i64) -> Day {
        // Normalises the month first, then lets the day run over, the way
        // JavaScript's Date does: 31 January plus a month is 3 March.
        let (y, m) = (y + (m - 1).div_euclid(12), (m - 1).rem_euclid(12) + 1);
        Day(days_from_civil(y, m, 1) + d - 1)
    }

    /// The calendar day a stored timestamp fell on, for someone whose clock
    /// is `offset_minutes` ahead of UTC.
    ///
    /// Read the way JavaScript's `Date` reads it, because that is how the
    /// prototype turned a settlement time into a day: a bare date is midnight
    /// UTC, a time with `Z` or an offset is that instant, and a time with
    /// neither is already local.
    pub fn of_timestamp(text: &str, offset_minutes: i64) -> Option<Day> {
        let day = Day::parse(text.get(..10)?)?;
        let rest = &text[10..];
        if rest.is_empty() {
            return Some(Day((day.0 * 1440 + offset_minutes).div_euclid(1440)));
        }
        let time = rest.strip_prefix('T').or_else(|| rest.strip_prefix(' '))?;
        let hh: i64 = time.get(0..2)?.parse().ok()?;
        let mm: i64 = time.get(3..5)?.parse().ok()?;
        if time.as_bytes().get(2) != Some(&b':') || hh > 23 || mm > 59 {
            return None;
        }
        // Seconds and a fraction may follow; the zone, if any, comes last.
        let zone_at = time[5..].find(['Z', 'z', '+', '-']).map(|i| i + 5);
        let zone = match zone_at {
            None => return Some(day),
            Some(i) => &time[i..],
        };
        let zone_minutes = if zone.eq_ignore_ascii_case("z") {
            0
        } else {
            let sign = if zone.starts_with('-') { -1 } else { 1 };
            let digits: String = zone[1..].chars().filter(char::is_ascii_digit).collect();
            if digits.len() != 4 {
                return None;
            }
            let h: i64 = digits[..2].parse().ok()?;
            let m: i64 = digits[2..].parse().ok()?;
            sign * (h * 60 + m)
        };
        let minutes = day.0 * 1440 + hh * 60 + mm - zone_minutes + offset_minutes;
        Some(Day(minutes.div_euclid(1440)))
    }

    pub fn ymd(self) -> (i64, i64, i64) {
        civil_from_days(self.0)
    }

    pub fn iso(self) -> String {
        let (y, m, d) = self.ymd();
        format!("{y:04}-{m:02}-{d:02}")
    }

    pub fn days_until(self, later: Day) -> i64 {
        later.0 - self.0
    }

    pub fn plus_days(self, n: i64) -> Day {
        Day(self.0 + n)
    }

    pub fn plus_months(self, n: i64) -> Day {
        let (y, m, d) = self.ymd();
        Day::from_ymd(y, m + n, d)
    }
}

fn is_leap(y: i64) -> bool {
    (y % 4 == 0 && y % 100 != 0) || y % 400 == 0
}

pub fn days_in_month(y: i64, m: i64) -> i64 {
    match m {
        2 if is_leap(y) => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        _ => 31,
    }
}

// Howard Hinnant's civil-from-days and its inverse. Day zero is 1970-01-01.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(z: i64) -> (i64, i64, i64) {
    let z = z + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settlement_time_lands_on_the_local_day() {
        let at = "2026-09-10T02:30:00.000Z";
        assert_eq!(Day::of_timestamp(at, 0).unwrap().iso(), "2026-09-10");
        // Evening of the 9th in New York.
        assert_eq!(Day::of_timestamp(at, -240).unwrap().iso(), "2026-09-09");
        assert_eq!(
            Day::of_timestamp("2026-09-10T23:30:00Z", 120)
                .unwrap()
                .iso(),
            "2026-09-11"
        );
        assert_eq!(
            Day::of_timestamp("2026-09-10T23:30:00+02:00", 0)
                .unwrap()
                .iso(),
            "2026-09-10"
        );
    }

    #[test]
    fn a_bare_date_is_midnight_utc_and_a_bare_time_is_already_local() {
        assert_eq!(
            Day::of_timestamp("2026-09-10", -240).unwrap().iso(),
            "2026-09-09"
        );
        assert_eq!(
            Day::of_timestamp("2026-09-10", 60).unwrap().iso(),
            "2026-09-10"
        );
        assert_eq!(
            Day::of_timestamp("2026-09-10T23:59:00", -600)
                .unwrap()
                .iso(),
            "2026-09-10"
        );
    }

    #[test]
    fn nonsense_is_no_day() {
        for bad in [
            "",
            "soon",
            "2026-02-30T10:00:00Z",
            "2026-09-10T25:00:00Z",
            "2026-09-10Tnoon",
        ] {
            assert_eq!(Day::of_timestamp(bad, 0), None, "{bad}");
        }
    }
}
