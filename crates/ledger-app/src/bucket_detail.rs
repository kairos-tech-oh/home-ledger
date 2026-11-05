//! One savings bucket, in depth: what moved in and out of it lately, how fast
//! it has been growing, and what it is spending a month.
//!
//! Nothing here is stored for the purpose. A bucket keeps only its balance, so
//! its movements are read back out of the change history, where every edit
//! that moves a bucket records the bucket's balance before and after, and its
//! growth out of the daily snapshots, which record each bucket's total
//! (invested money included) once a day.

use crate::commands::{Answer, CommandError};
use crate::state::AppState;
use crate::views::amount;
use ledger_domain::records::AuditEntry;
use ledger_domain::{Money, valid_id};
use ledger_math::calendar::Day;
use ledger_math::snapshots::Point;
use rust_decimal::Decimal;
use serde::Serialize;

/// How far back "recent activity" reaches.
pub const RECENT_DAYS: i64 = 31;
/// How far back the growth figures and the chart reach.
const GROWTH_DAYS: i64 = 365;
/// How many calendar months the month-by-month table shows.
const MONTHS: usize = 6;
/// An average month, in days, as a decimal: 30.4375.
fn days_per_month() -> Decimal {
    Decimal::new(304_375, 4)
}

/// A total over `days` as a month's worth. Anything shorter than a month is
/// not stretched to one: ten days' spending is not called a month's three
/// times over.
fn monthly(total: Money, days: i64) -> Money {
    let days = Decimal::from(days.max(1)).max(days_per_month());
    Money::new(total.inner() * days_per_month() / days)
}

/// The edits that can change a bucket's balance, and what kind of movement
/// each one is.
fn kind_of(op: &str) -> Option<Kind> {
    Some(match op {
        "bucket-spend" | "reconcile-settle" | "reconcile-undo" => Kind::Spend,
        "bucket-move" => Kind::Move,
        "bucket-adjust" | "bucket-deposit" | "bucket-total" => Kind::Add,
        _ => return None,
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// Money put in or taken out by hand, a payday, or a balance set.
    Add,
    /// Spent, directly or by settling a statement (and an undo of one).
    Spend,
    /// Moved to or from another bucket.
    Move,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Movement {
    pub id: String,
    pub at: String,
    pub kind: Kind,
    pub action: String,
    /// What the edit was called: a label, a statement, "A → B".
    pub label: String,
    pub delta: String,
    /// The bucket's cash just after.
    pub balance: String,
    pub actor: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonthView {
    /// `yyyy-mm`.
    pub month: String,
    /// The bucket's total at the last snapshot of the month.
    pub end: String,
    /// Against the month before, or None for the first month with a point.
    pub change: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Growth {
    pub from_at: String,
    pub to_at: String,
    pub from: String,
    pub to: String,
    pub delta: String,
    /// The change spread evenly across the months between the two points.
    pub monthly: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketDetail {
    pub id: String,
    /// Newest first, the last [`RECENT_DAYS`].
    pub recent: Vec<Movement>,
    /// The first day "recent" covers.
    pub recent_from: String,
    /// How many days of change history the averages are taken over: up to the
    /// sixty the history keeps, fewer for a ledger that has not been edited
    /// that long.
    pub history_days: i64,
    /// A month's worth of each kind of movement, over `history_days`.
    pub added_monthly: String,
    pub spent_monthly: String,
    pub moved_monthly: String,
    /// What the total did across the snapshots of the last year, investments
    /// included. None with fewer than two points a week apart.
    pub growth: Option<Growth>,
    /// Oldest first.
    pub months: Vec<MonthView>,
    /// Daily totals for a chart: seconds since the epoch, and the total.
    pub series: Vec<(i64, f64)>,
}

pub async fn bucket_detail(
    state: &AppState,
    id: String,
    today: String,
    offset_minutes: i64,
) -> Answer<BucketDetail> {
    let Some(today) = Day::parse(&today) else {
        return Err(CommandError::Message("today must be yyyy-mm-dd".into()));
    };
    let doc = crate::commands::shown(state).await?.ledger;
    let wanted = valid_id(&id);
    let Some(bucket) = doc.buckets.iter().find(|b| b.id == wanted) else {
        return Err(CommandError::Message("no such bucket".into()));
    };
    let entries = crate::views::audit_entries(state).await;
    let mut points = state.points().read().await;
    if let Some(demo) = state.demo.lock().unwrap().as_ref() {
        points = demo.points(&points);
    }
    let history_from = Day::of_timestamp(&crate::audit::window_start(), offset_minutes);
    Ok(detail(
        &bucket.id,
        &bucket.name,
        &entries,
        &points,
        today,
        history_from.unwrap_or(today),
        offset_minutes,
    ))
}

/// The bucket's change in one history entry, or None when it did not move.
///
/// An entry names what it changed by name, the bucket's first, then the
/// accounts that followed it. A balance set names the bucket as the entry and
/// the change as "balance".
fn moved(entry: &AuditEntry, name: &str) -> Option<(Kind, Money, Money)> {
    let kind = kind_of(&entry.op)?;
    let change = if entry.op == "bucket-total" {
        (entry.name == name)
            .then(|| entry.changes.iter().find(|c| c.field == "balance"))
            .flatten()?
    } else {
        entry.changes.iter().find(|c| c.field == name)?
    };
    let read = |text: &str| text.trim().parse::<Decimal>().ok().map(Money::new);
    let (from, to) = (read(&change.from)?, read(&change.to)?);
    (from != to).then_some((kind, to - from, to))
}

/// Everything above, from the history and the snapshots as given. `entries`
/// are oldest first, as the history keeps them; so are `points`.
pub fn detail(
    id: &str,
    name: &str,
    entries: &[AuditEntry],
    points: &[Point],
    today: Day,
    history_from: Day,
    offset_minutes: i64,
) -> BucketDetail {
    let recent_from = today.plus_days(-(RECENT_DAYS - 1));
    // Averages run from the oldest history there is, so a ledger edited for a
    // fortnight is not averaged as if it had sat still for two months.
    let oldest = entries
        .iter()
        .filter_map(|e| Day::of_timestamp(&e.at, offset_minutes))
        .min();
    let start = oldest.map_or(today, |d| d.max(history_from)).min(today);
    let history_days = start.days_until(today) + 1;

    let mut recent = Vec::new();
    let (mut added, mut spent, mut moved_net) = (Money::ZERO, Money::ZERO, Money::ZERO);
    for entry in entries {
        let Some((kind, delta, balance)) = moved(entry, name) else {
            continue;
        };
        let Some(day) = Day::of_timestamp(&entry.at, offset_minutes) else {
            continue;
        };
        if day < start || day > today {
            continue;
        }
        match kind {
            Kind::Add => added += delta,
            Kind::Spend => spent -= delta,
            Kind::Move => moved_net += delta,
        }
        if day >= recent_from {
            recent.push(Movement {
                id: entry.id.clone(),
                at: entry.at.clone(),
                kind,
                action: entry.action.clone(),
                label: entry.name.clone(),
                delta: amount(delta),
                balance: amount(balance),
                actor: entry.actor.clone(),
            });
        }
    }
    recent.reverse();

    let per_month = |total: Money| amount(monthly(total, history_days));

    // The bucket's total on each day a snapshot was taken that knew it.
    let epoch = Day::from_ymd(1970, 1, 1);
    let floor = today.plus_days(-GROWTH_DAYS);
    let totals: Vec<(Day, Money)> = points
        .iter()
        .filter_map(|p| {
            let day = Day::parse(&p.at)?;
            let held = p.buckets.iter().find(|b| b.id == id)?;
            (day > floor && day <= today).then_some((day, held.total))
        })
        .collect();

    let growth = match (totals.first(), totals.last()) {
        (Some(&(from_day, from)), Some(&(to_day, to))) if from_day.days_until(to_day) >= 7 => {
            let delta = to - from;
            Some(Growth {
                from_at: from_day.iso(),
                to_at: to_day.iso(),
                from: amount(from),
                to: amount(to),
                delta: amount(delta),
                monthly: amount(monthly(delta, from_day.days_until(to_day))),
            })
        }
        _ => None,
    };

    // The last point of each month, and how it compares with the one before.
    let mut ends: Vec<(String, Money)> = Vec::new();
    for &(day, total) in &totals {
        let month = day.iso()[..7].to_string();
        match ends.last_mut() {
            Some((m, held)) if *m == month => *held = total,
            _ => ends.push((month, total)),
        }
    }
    let mut months: Vec<MonthView> = ends
        .iter()
        .enumerate()
        .map(|(i, (month, end))| MonthView {
            month: month.clone(),
            end: amount(*end),
            change: i.checked_sub(1).map(|p| amount(*end - ends[p].1)),
        })
        .collect();
    if months.len() > MONTHS {
        months.drain(0..months.len() - MONTHS);
    }

    BucketDetail {
        id: id.to_string(),
        recent,
        recent_from: recent_from.iso(),
        history_days,
        added_monthly: per_month(added),
        spent_monthly: per_month(spent),
        moved_monthly: per_month(moved_net),
        growth,
        months,
        series: totals
            .iter()
            .map(|&(day, total)| (epoch.days_until(day) * 86_400, total.to_f64()))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::records::AuditChange;
    use ledger_math::snapshots::BucketPoint;

    fn change(field: &str, from: &str, to: &str) -> AuditChange {
        AuditChange {
            field: field.into(),
            from: from.into(),
            to: to.into(),
        }
    }

    fn entry(at: &str, op: &str, name: &str, changes: Vec<AuditChange>) -> AuditEntry {
        AuditEntry {
            id: format!("{op}-{at}"),
            at: format!("{at}T12:00:00Z"),
            op: op.into(),
            action: "Did".into(),
            name: name.into(),
            changes,
            ..Default::default()
        }
    }

    fn point(at: &str, id: &str, total: i64) -> Point {
        Point {
            at: at.into(),
            buckets: vec![BucketPoint {
                id: id.into(),
                total: Money::from(total),
            }],
            ..Default::default()
        }
    }

    fn day(text: &str) -> Day {
        Day::parse(text).unwrap()
    }

    fn history() -> Vec<AuditEntry> {
        vec![
            // Older than a month: counts toward the averages, not "recent".
            entry(
                "2026-09-10",
                "bucket-deposit",
                "Payday",
                vec![
                    change("Travel", "0.00", "600.00"),
                    change("Checking", "", "600.00"),
                ],
            ),
            entry(
                "2026-10-20",
                "bucket-spend",
                "Flights",
                vec![change("Travel", "600.00", "200.00")],
            ),
            // Another bucket entirely.
            entry(
                "2026-10-22",
                "bucket-adjust",
                "Added",
                vec![change("Roof", "0.00", "50.00")],
            ),
            entry(
                "2026-10-25",
                "bucket-move",
                "Travel → Roof",
                vec![
                    change("Roof", "50.00", "100.00"),
                    change("Travel", "200.00", "150.00"),
                ],
            ),
            entry(
                "2026-10-28",
                "bucket-total",
                "Travel",
                vec![change("balance", "150.00", "175.00")],
            ),
            // Not a bucket movement, even though it names the bucket.
            entry(
                "2026-10-29",
                "set",
                "Travel",
                vec![change("notes", "", "Japan")],
            ),
        ]
    }

    #[test]
    fn recent_movements_are_this_buckets_and_newest_first() {
        let d = detail(
            "b1",
            "Travel",
            &history(),
            &[],
            day("2026-11-04"),
            day("2026-09-05"),
            0,
        );
        let got: Vec<(&str, &str, Kind)> = d
            .recent
            .iter()
            .map(|m| (m.delta.as_str(), m.balance.as_str(), m.kind))
            .collect();
        assert_eq!(
            got,
            vec![
                ("25.00", "175.00", Kind::Add),
                ("-50.00", "150.00", Kind::Move),
                ("-400.00", "200.00", Kind::Spend),
            ]
        );
        assert_eq!(d.recent_from, "2026-10-05");
    }

    #[test]
    fn averages_run_over_the_history_there_is() {
        let d = detail(
            "b1",
            "Travel",
            &history(),
            &[],
            day("2026-11-04"),
            day("2026-09-05"),
            0,
        );
        // 2026-09-10 to 2026-11-04 inclusive.
        assert_eq!(d.history_days, 56);
        // 625, 400 and -50 over 56 days, as a 30.4375-day month.
        assert_eq!(d.added_monthly, "339.70");
        assert_eq!(d.spent_monthly, "217.41");
        assert_eq!(d.moved_monthly, "-27.18");
    }

    #[test]
    fn a_settle_undone_is_spending_given_back() {
        let entries = vec![
            entry(
                "2026-11-01",
                "reconcile-settle",
                "Card",
                vec![change("Travel", "300.00", "100.00")],
            ),
            entry(
                "2026-11-02",
                "reconcile-undo",
                "Card",
                vec![change("Travel", "100.00", "300.00")],
            ),
        ];
        let d = detail(
            "b1",
            "Travel",
            &entries,
            &[],
            day("2026-11-04"),
            day("2026-09-05"),
            0,
        );
        assert_eq!(d.spent_monthly, "0.00");
        assert_eq!(d.recent.len(), 2);
    }

    #[test]
    fn growth_and_months_come_from_the_snapshots() {
        let points = vec![
            point("2026-08-15", "b1", 100),
            point("2026-08-31", "b1", 200),
            point("2026-09-30", "b1", 260),
            point("2026-10-15", "other", 999),
            point("2026-10-31", "b1", 230),
        ];
        let d = detail(
            "b1",
            "Travel",
            &[],
            &points,
            day("2026-11-04"),
            day("2026-09-05"),
            0,
        );
        let g = d.growth.unwrap();
        assert_eq!(
            (g.from_at.as_str(), g.to_at.as_str()),
            ("2026-08-15", "2026-10-31")
        );
        assert_eq!(g.delta, "130.00");
        // 130 over 77 days.
        assert_eq!(g.monthly, "51.39");
        let months: Vec<(&str, &str, Option<&str>)> = d
            .months
            .iter()
            .map(|m| (m.month.as_str(), m.end.as_str(), m.change.as_deref()))
            .collect();
        assert_eq!(
            months,
            vec![
                ("2026-08", "200.00", None),
                ("2026-09", "260.00", Some("60.00")),
                ("2026-10", "230.00", Some("-30.00")),
            ]
        );
        assert_eq!(d.series.len(), 4);
    }

    #[test]
    fn too_little_history_says_nothing_about_growth() {
        let points = vec![
            point("2026-11-01", "b1", 100),
            point("2026-11-03", "b1", 120),
        ];
        let d = detail(
            "b1",
            "Travel",
            &[],
            &points,
            day("2026-11-04"),
            day("2026-09-05"),
            0,
        );
        assert!(d.growth.is_none());
        assert_eq!(d.history_days, 1);
        assert_eq!(d.added_monthly, "0.00");
    }
}
