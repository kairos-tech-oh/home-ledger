//! Goals. A goal holds no money of its own: it names a target and the bucket
//! the money is actually going into, and reads its progress from that bucket.
//!
//! Port of `goalSaved` and `goalProgress` in `core/Model.js`.

use crate::bucket_worth;
use ledger_domain::records::Goal;
use ledger_domain::{Ledger, Money};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

/// What the bucket behind a goal is worth. Nothing when it names no bucket,
/// or one that has since been deleted.
pub fn goal_saved(ledger: &Ledger, goal: &Goal) -> Money {
    if goal.bucket_id.is_empty() || ledger.bucket(&goal.bucket_id).is_none() {
        return Money::ZERO;
    }
    bucket_worth(ledger, &goal.bucket_id).total
}

/// How far along, 0..=100, or None when there is no target to measure against.
pub fn goal_progress(ledger: &Ledger, goal: &Goal) -> Option<f64> {
    let target = goal.target_amount.filter(|t| t.inner() > Decimal::ZERO)?;
    Some(percent_of(goal_saved(ledger, goal), target))
}

fn percent_of(saved: Money, target: Money) -> f64 {
    let pct = saved.inner() / target.inner() * Decimal::from(100);
    pct.min(Decimal::from(100)).to_f64().unwrap_or(0.0)
}

/// The three figures across every goal.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GoalTotals {
    /// What the goals' buckets hold between them.
    pub saved: Money,
    /// What the goals with a target want between them.
    pub target: Money,
    pub with_target: usize,
    /// Saved against wanted, capped at 100. None when no goal has a target.
    pub percent: Option<f64>,
}

/// Summed goal by goal, as the prototype does. Two goals on one bucket count
/// that bucket twice, which is what "saved towards them" means for each.
pub fn goal_totals(ledger: &Ledger) -> GoalTotals {
    let mut totals = GoalTotals::default();
    for goal in &ledger.goals {
        totals.saved += goal_saved(ledger, goal);
        if let Some(target) = goal.target_amount.filter(|t| t.inner() > Decimal::ZERO) {
            totals.target += target;
            totals.with_target += 1;
        }
    }
    if !totals.target.is_zero() {
        totals.percent = Some(percent_of(totals.saved, totals.target));
    }
    totals
}

/// The order the goals screen lists them in: the nearest to done first, goals
/// with no target last and by name, so the order never shuffles.
pub fn goals_in_order(ledger: &Ledger) -> Vec<&Goal> {
    let mut rows: Vec<(&Goal, Option<f64>)> = ledger
        .goals
        .iter()
        .map(|g| (g, goal_progress(ledger, g)))
        .collect();
    rows.sort_by(|(a, ap), (b, bp)| match (ap, bp) {
        (None, None) => a.name.cmp(&b.name),
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (Some(_), None) => std::cmp::Ordering::Less,
        (Some(x), Some(y)) => y.total_cmp(x).then_with(|| a.name.cmp(&b.name)),
    });
    rows.into_iter().map(|(g, _)| g).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::records::{Account, Allocation, Bucket, Holding};

    /// `rothBuckets`, `holds` and `rothAccts` from `tools/check-math.mjs`.
    fn oracle() -> Ledger {
        let mut ledger = Ledger::default();
        ledger.buckets.push(Bucket {
            id: "k1".into(),
            name: "Emergency".into(),
            current_total: Money::from(5_000),
            target_amount: Some(Money::from(20_000)),
            ..Default::default()
        });
        ledger.buckets.push(Bucket {
            id: "kR".into(),
            name: "Retirement".into(),
            current_total: Money::from(40_000),
            locked: true,
            ..Default::default()
        });
        ledger.investments.push(Holding {
            id: "h1".into(),
            account_id: "a1".into(),
            quantity: Money::from(100),
            avg_cost: Money::from(250),
            price: Some(Money::from(300)),
            ..Default::default()
        });
        ledger.investments.push(Holding {
            id: "h2".into(),
            account_id: "a1".into(),
            bucket_id: "k1".into(),
            quantity: Money::from(200),
            avg_cost: Money::from(70),
            price: Some(Money::from(72)),
            ..Default::default()
        });
        ledger.investments.push(Holding {
            id: "h3".into(),
            bucket_id: "k1".into(),
            quantity: Money::from(1),
            avg_cost: Money::from(4_000),
            price: None,
            ..Default::default()
        });
        ledger.accounts.push(Account {
            id: "aR".into(),
            kind: "retirement-roth".into(),
            contributions_amount: Some(Money::from(18_000)),
            contribution_allocations: vec![Allocation {
                bucket_id: "k1".into(),
                amount: Money::from(5_000),
            }],
            ..Default::default()
        });
        ledger.accounts.push(Account {
            id: "aX".into(),
            kind: "checking".into(),
            contribution_allocations: vec![Allocation {
                bucket_id: "k1".into(),
                amount: Money::from(999),
            }],
            ..Default::default()
        });
        ledger
    }

    fn goal(bucket: &str, target: Option<i64>) -> Goal {
        Goal {
            id: ledger_domain::new_id(),
            name: "House".into(),
            bucket_id: bucket.into(),
            target_amount: target.map(Money::from),
            ..Default::default()
        }
    }

    #[test]
    fn a_goal_reads_the_bucket_it_is_linked_to() {
        // 5000 cash + 14400 bond fund + 4000 gold + 5000 of Roth basis.
        let ledger = oracle();
        assert_eq!(
            goal_saved(&ledger, &goal("k1", Some(60_000))),
            Money::from(28_400)
        );
    }

    #[test]
    fn goal_progress_is_capped_at_a_hundred() {
        let ledger = oracle();
        let p = goal_progress(&ledger, &goal("k1", Some(60_000))).unwrap();
        assert_eq!((p * 10.0).round() / 10.0, 47.3);
        assert_eq!(goal_progress(&ledger, &goal("k1", None)), None);
        assert_eq!(goal_progress(&ledger, &goal("k1", Some(100))), Some(100.0));
    }

    #[test]
    fn a_goal_linked_to_nothing_has_saved_nothing() {
        let ledger = oracle();
        assert_eq!(goal_saved(&ledger, &goal("", Some(1))), Money::ZERO);
    }

    #[test]
    fn a_goal_on_a_deleted_bucket_has_saved_nothing() {
        // Holdings can still carry the id of a bucket that is gone; they must
        // not be credited to a goal pointing at the same id.
        let mut ledger = oracle();
        ledger.buckets.retain(|b| b.id != "k1");
        assert_eq!(goal_saved(&ledger, &goal("k1", Some(1))), Money::ZERO);
    }

    #[test]
    fn a_zero_target_is_no_target() {
        let ledger = oracle();
        assert_eq!(goal_progress(&ledger, &goal("k1", Some(0))), None);
    }

    #[test]
    fn totals_add_goal_by_goal() {
        let mut ledger = oracle();
        ledger.goals.push(goal("k1", Some(60_000)));
        ledger.goals.push(goal("kR", None));
        let totals = goal_totals(&ledger);
        assert_eq!(totals.saved, Money::from(28_400 + 40_000));
        assert_eq!(totals.target, Money::from(60_000));
        assert_eq!(totals.with_target, 1);
        // Capped: more is saved across the goals than the one target wants.
        assert_eq!(totals.percent, Some(100.0));
    }

    #[test]
    fn no_targets_means_no_overall_progress() {
        let mut ledger = oracle();
        ledger.goals.push(goal("kR", None));
        assert_eq!(goal_totals(&ledger).percent, None);
    }

    #[test]
    fn the_nearest_to_done_comes_first_and_untargeted_goals_last() {
        let mut ledger = oracle();
        let mut far = goal("k1", Some(1_000_000));
        far.name = "Far".into();
        let mut done = goal("kR", Some(10));
        done.name = "Done".into();
        let mut loose_b = goal("k1", None);
        loose_b.name = "B loose".into();
        let mut loose_a = goal("", None);
        loose_a.name = "A loose".into();
        ledger.goals = vec![loose_b, far, loose_a, done];
        let names: Vec<_> = goals_in_order(&ledger)
            .iter()
            .map(|g| g.name.as_str())
            .collect();
        assert_eq!(names, ["Done", "Far", "A loose", "B loose"]);
    }
}
