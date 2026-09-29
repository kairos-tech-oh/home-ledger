//! A snapshot is the one thing stored rather than derived: yesterday's
//! balances cannot be recomputed from today's document. One point a day, taken
//! when the app opens, is enough to say how net worth has moved.
//!
//! Port of `buildSnapshot`, `snapshotChange` and `snapshotNormalised` in
//! `core/Model.js`, and of the helper's `clean_snapshot` and
//! `merge_snapshots`, so a point the plugin recorded reads back the same here.

use crate::calendar::Day;
use crate::{bucket_worth, holdings_total, monthly_budget, monthly_income, net_worth};
use ledger_domain::records::caps;
use ledger_domain::{Ledger, Money, valid_id};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Ten years of daily points.
pub const MAX_POINTS: usize = 3660;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BucketPoint {
    pub id: String,
    pub total: Money,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Point {
    /// The day it describes, `yyyy-mm-dd`.
    pub at: String,
    /// When it was taken. Of two points for one day, the later one is kept.
    pub taken_at: String,
    pub net: Money,
    pub assets: Money,
    pub debts: Money,
    pub savings: Money,
    pub holdings: Money,
    pub basis: Money,
    pub income_monthly: Money,
    pub budget_monthly: Money,
    pub buckets: Vec<BucketPoint>,
}

/// Where the ledger stands today.
pub fn build(ledger: &Ledger, at: Day, taken_at: &str) -> Point {
    let sheet = net_worth(ledger);
    let held = holdings_total(&ledger.investments);
    // Basis only where a cost was recorded, as the plugin's rollup counts it.
    let basis: Money = ledger.investments.iter().filter_map(|h| h.cost_basis).sum();
    let buckets: Vec<BucketPoint> = ledger
        .buckets
        .iter()
        .map(|b| BucketPoint {
            id: b.id.clone(),
            total: bucket_worth(ledger, &b.id).total,
        })
        .collect();
    Point {
        at: at.iso(),
        taken_at: taken_at.to_string(),
        net: sheet.net,
        assets: sheet.assets,
        debts: sheet.debts,
        savings: buckets.iter().map(|b| b.total).sum(),
        holdings: held.value,
        basis,
        income_monthly: monthly_income(ledger),
        budget_monthly: monthly_budget(ledger),
        buckets,
    }
}

/// A point fit to keep, or None. Amounts are already cents by the time they
/// are a `Money`. A missing `takenAt` is the start of that day, not now: now
/// would make the same stored point differ every time it was cleaned.
pub fn clean(mut point: Point) -> Option<Point> {
    let day = Day::parse(&point.at)?;
    point.at = day.iso();
    point.taken_at = ledger_domain::plain(&point.taken_at, 32);
    if point.taken_at.is_empty() {
        point.taken_at = format!("{}T00:00:00Z", point.at);
    }
    point.buckets.truncate(caps::BUCKETS);
    point.buckets.retain(|b| !valid_id(&b.id).is_empty());
    Some(point)
}

/// Every point from every source, one per day, oldest first. Points are
/// independent dated records, so a union is always safe; of two for one day
/// the one taken later wins.
pub fn merge<'a>(sources: impl IntoIterator<Item = &'a [Point]>) -> Vec<Point> {
    let mut by_day: BTreeMap<String, Point> = BTreeMap::new();
    for point in sources.into_iter().flatten() {
        let Some(point) = clean(point.clone()) else {
            continue;
        };
        match by_day.get(&point.at) {
            Some(held) if held.taken_at >= point.taken_at => {}
            _ => {
                by_day.insert(point.at.clone(), point);
            }
        }
    }
    let mut merged: Vec<Point> = by_day.into_values().collect();
    if merged.len() > MAX_POINTS {
        merged.drain(0..merged.len() - MAX_POINTS);
    }
    merged
}

/// Which figure of a point to read.
pub type Field = fn(&Point) -> Money;

#[derive(Clone, Debug, PartialEq)]
pub struct Change {
    pub from: Money,
    pub to: Money,
    pub delta: Money,
    pub from_at: String,
    pub to_at: String,
    /// None when the starting figure was not above zero.
    pub percent: Option<f64>,
}

/// The change over the last `days`, measured from the oldest point inside the
/// window to the newest. With nothing inside the window it falls back to the
/// oldest point there is, so a sparse history still says something. None with
/// fewer than two points.
///
/// `points` must be in date order, as [`merge`] leaves them.
pub fn change(points: &[Point], field: Field, days: i64, today: Day) -> Option<Change> {
    let last = points.last()?;
    if points.len() < 2 {
        return None;
    }
    let floor = today.plus_days(-days);
    let mut first = points
        .iter()
        .find(|p| Day::parse(&p.at).is_some_and(|d| d > floor))
        .unwrap_or(&points[0]);
    if std::ptr::eq(first, last) {
        first = &points[0];
    }
    if std::ptr::eq(first, last) {
        return None;
    }
    let (from, to) = (field(first), field(last));
    let delta = to - from;
    Some(Change {
        from,
        to,
        delta,
        from_at: first.at.clone(),
        to_at: last.at.clone(),
        percent: (from.inner() > rust_decimal::Decimal::ZERO)
            .then(|| delta.to_f64() / from.to_f64() * 100.0),
    })
}

/// The last `limit` values scaled to 0..=1 for a sparkline, oldest first. A
/// flat series sits on the midline rather than on the floor.
pub fn normalised(points: &[Point], field: Field, limit: usize) -> Vec<f64> {
    let start = points.len().saturating_sub(limit);
    let values: Vec<f64> = points[start..].iter().map(|p| field(p).to_f64()).collect();
    let lo = values.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = hi - lo;
    values
        .into_iter()
        .map(|v| if span > 0.0 { (v - lo) / span } else { 0.5 })
        .collect()
}

pub fn net(point: &Point) -> Money {
    point.net
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::records::{Account, Bucket, Holding};
    use rust_decimal_macros::dec;

    fn point(at: &str, net: Money) -> Point {
        Point {
            at: at.into(),
            net,
            ..Default::default()
        }
    }

    fn day(text: &str) -> Day {
        Day::parse(text).unwrap()
    }

    /// `rec` from the snapshot cases in `check-math.mjs`, stored out of order.
    fn rec() -> Vec<Point> {
        merge([[
            point("2026-09-14", Money::new(dec!(189490.34))),
            point("2026-09-01", Money::from(185_000)),
            point("2026-09-08", Money::from(187_000)),
        ]
        .as_slice()])
    }

    #[test]
    fn snapshots_read_back_oldest_first_whatever_order_they_were_stored_in() {
        let at: Vec<_> = rec().iter().map(|p| p.at.clone()).collect();
        assert_eq!(at, ["2026-09-01", "2026-09-08", "2026-09-14"]);
    }

    #[test]
    fn the_change_is_measured_from_the_oldest_point_inside_the_window() {
        let ch = change(&rec(), net, 30, day("2026-09-14")).unwrap();
        assert_eq!(ch.delta, Money::new(dec!(4490.34)));
        assert_eq!(ch.from_at, "2026-09-01");
    }

    #[test]
    fn a_window_with_nothing_in_it_falls_back_to_the_oldest_point() {
        let ch = change(&rec(), net, 3, day("2026-12-01")).unwrap();
        assert_eq!(ch.from_at, "2026-09-01");
        assert_eq!(ch.to_at, "2026-09-14");
    }

    #[test]
    fn one_point_or_none_is_not_a_trend() {
        let one = [point("2026-09-14", Money::from(1))];
        assert_eq!(change(&one, net, 30, day("2026-09-14")), None);
        assert_eq!(change(&[], net, 30, day("2026-09-14")), None);
    }

    #[test]
    fn a_flat_series_sits_on_the_midline() {
        let flat = [
            point("2026-01-01", Money::from(5)),
            point("2026-01-02", Money::from(5)),
        ];
        assert_eq!(normalised(&flat, net, 30), [0.5, 0.5]);
    }

    #[test]
    fn normalised_values_span_nought_to_one() {
        let n = normalised(&rec(), net, 30);
        assert_eq!((n[0], n[n.len() - 1]), (0.0, 1.0));
        assert_eq!(normalised(&rec(), net, 2).len(), 2);
    }

    #[test]
    fn of_two_points_for_one_day_the_later_one_is_kept() {
        let mut early = point("2026-09-01", Money::from(1));
        early.taken_at = "2026-09-01T08:00:00Z".into();
        let mut late = point("2026-09-01", Money::from(2));
        late.taken_at = "2026-09-01T20:00:00Z".into();
        for order in [[early.clone(), late.clone()], [late.clone(), early.clone()]] {
            let merged = merge([order.as_slice()]);
            assert_eq!(merged.len(), 1);
            assert_eq!(merged[0].net, Money::from(2));
        }
    }

    #[test]
    fn a_point_without_a_real_date_is_dropped_and_one_without_a_time_gets_midnight() {
        let merged = merge([[
            point("2026-02-30", Money::from(1)),
            point("2026-09-01", Money::from(1)),
        ]
        .as_slice()]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].taken_at, "2026-09-01T00:00:00Z");
    }

    #[test]
    fn a_snapshot_agrees_with_the_live_rollups() {
        // check-math.mjs: one account, one holding in a bucket, one bucket.
        let mut doc = Ledger::default();
        doc.accounts.push(Account {
            id: "a".into(),
            kind: "checking".into(),
            total: Some(Money::from(1_000)),
            ..Default::default()
        });
        let b = "b".repeat(32);
        doc.investments.push(Holding {
            id: "i".into(),
            account_id: "a".into(),
            bucket_id: b.clone(),
            quantity: Money::from(10),
            avg_cost: Money::from(40),
            price: Some(Money::from(55)),
            cost_basis: Some(Money::from(400)),
            ..Default::default()
        });
        doc.buckets.push(Bucket {
            id: b.clone(),
            name: "Car".into(),
            current_total: Money::from(500),
            target_amount: Some(Money::from(2_000)),
            ..Default::default()
        });
        let snap = build(&doc, day("2026-09-14"), "2026-09-14T09:00:00Z");
        assert_eq!(snap.net, net_worth(&doc).net);
        assert_eq!(snap.net, Money::from(1_550));
        assert_eq!(snap.buckets.len(), 1);
        assert_eq!(snap.buckets[0].id, b);
        assert_eq!(snap.buckets[0].total, bucket_worth(&doc, &b).total);
        assert_eq!(snap.savings, Money::from(1_050));
        assert_eq!(
            (snap.holdings, snap.basis),
            (Money::from(550), Money::from(400))
        );
    }

    #[test]
    fn the_cap_keeps_the_newest() {
        let start = day("2000-01-01");
        let many: Vec<Point> = (0..(MAX_POINTS as i64 + 10))
            .map(|i| point(&start.plus_days(i).iso(), Money::from(i)))
            .collect();
        let merged = merge([many.as_slice()]);
        assert_eq!(merged.len(), MAX_POINTS);
        assert_eq!(
            merged.last().unwrap().net,
            Money::from(MAX_POINTS as i64 + 9)
        );
    }

    #[test]
    fn a_stored_plugin_point_round_trips() {
        let raw = r#"{"at":"2026-09-14","takenAt":"2026-09-14T08:00:00Z","net":189490.34,
            "assets":200000,"debts":10509.66,"savings":5000,"holdings":1000,"basis":900,
            "incomeMonthly":6000,"budgetMonthly":4500,
            "buckets":[{"id":"bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb","total":5000}]}"#;
        let p: Point = serde_json::from_str(raw).unwrap();
        let p = clean(p).unwrap();
        assert_eq!(p.net, Money::new(dec!(189490.34)));
        assert_eq!(p.buckets.len(), 1);
        let back: Point = serde_json::from_str(&serde_json::to_string(&p).unwrap()).unwrap();
        assert_eq!(back, p);
    }
}
