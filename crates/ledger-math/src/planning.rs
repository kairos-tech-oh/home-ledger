//! Where every bucket lands by a date: what the budget lines feeding it put in
//! between now and then, what the planned draws under those lines take out,
//! and where that leaves it against its target.
//!
//! Port of `monthsBetween`, `addInterval`, `occurrences` and `project` in
//! `core/Model.js`, checked against the same cases in `check-math.mjs`.
//!
//! The prototype works in epoch milliseconds with "now" carrying the time of
//! day. This works in calendar dates, so a draw dated today counts as falling
//! in the window rather than dropping out once midnight has passed. At
//! midnight — which is where every oracle case sits — the two agree.

use crate::bucket_worth;
pub use crate::calendar::Day;
use ledger_domain::records::PlannedExpense;
use ledger_domain::{Ledger, Money};
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;

/// Months between two dates, fractional, on the 30.4375-day month the web app
/// projects with. A 365-day year is 11.99 months, not 12.
pub fn months_between(from: Day, to: Day) -> Decimal {
    Decimal::from(from.days_until(to)) / Decimal::new(304_375, 4)
}

/// The next time a repeating draw comes round. None for an interval that is
/// not one of the six, which the caller treats as "cannot place another".
fn step(day: Day, interval: &str) -> Option<Day> {
    Some(match interval {
        "weekly" => day.plus_days(7),
        "biweekly" => day.plus_days(14),
        "monthly" => day.plus_months(1),
        "quarterly" => day.plus_months(3),
        "every6months" => day.plus_months(6),
        "yearly" => day.plus_months(12),
        _ => return None,
    })
}

/// How many times a planned draw falls between two dates, both included.
pub fn occurrences(expense: &PlannedExpense, from: Day, to: Day) -> u32 {
    let Some(start) = Day::parse(&expense.draw_date) else {
        return 0;
    };
    if !expense.recurring {
        return u32::from(start >= from && start <= to);
    }
    let end = match Day::parse(&expense.end_date) {
        Some(stop) => stop.min(to),
        None => to,
    };

    // The guards are the prototype's: they bound a malformed schedule, not a
    // real one, which never comes near them.
    let mut cursor = start;
    let mut guard = 0;
    while cursor < from && guard < 2000 {
        match step(cursor, &expense.interval) {
            Some(next) if next > cursor => cursor = next,
            _ => return 0,
        }
        guard += 1;
    }
    let mut hits = 0;
    while cursor <= end && guard < 4000 {
        hits += 1;
        match step(cursor, &expense.interval) {
            Some(next) if next > cursor => cursor = next,
            _ => break,
        }
        guard += 1;
    }
    hits
}

/// One line of a planned draw's schedule, in the app's own wording.
pub fn schedule_label(expense: &PlannedExpense) -> String {
    if expense.draw_date.is_empty() {
        return if expense.recurring {
            "Recurring, no date set".into()
        } else {
            "One-time, no date set".into()
        };
    }
    if !expense.recurring {
        return format!("One-time on {}", expense.draw_date);
    }
    let every = match expense.interval.as_str() {
        "weekly" => "weekly",
        "biweekly" => "every 2 weeks",
        "quarterly" => "quarterly",
        "every6months" => "every 6 months",
        "yearly" => "yearly",
        _ => "monthly",
    };
    let body = format!("{every} from {}", expense.draw_date);
    if expense.end_date.is_empty() {
        body
    } else {
        format!("{body} until {}", expense.end_date)
    }
}

/// A planned draw that lands in the window.
#[derive(Clone, Debug, PartialEq)]
pub struct DrawHit {
    pub name: String,
    pub amount: Money,
    pub occurrences: u32,
    /// Amount times occurrences: what it takes out of the bucket.
    pub impact: Money,
    pub schedule: String,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BucketPlan {
    pub id: String,
    pub name: String,
    pub current: Money,
    pub projected: Money,
    /// What the budget puts in a month. None when no line feeds it, which is
    /// different from lines that add up to nothing.
    pub velocity: Option<Money>,
    pub contributions: Money,
    pub deductions: Money,
    pub target: Option<Money>,
    /// Projected against target, capped at 100.
    pub percent: Option<f64>,
    /// From the projected point, at the current rate, to reach the target.
    /// Zero once it is reached; None with no target or nothing feeding it.
    pub months_to_goal: Option<Decimal>,
    pub draws: Vec<DrawHit>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Plan {
    pub months: Decimal,
    /// Largest projected balance first.
    pub buckets: Vec<BucketPlan>,
}

/// Every bucket, carried forward from `from` to `to`.
pub fn project(ledger: &Ledger, from: Day, to: Day) -> Plan {
    let months = months_between(from, to);
    let mut buckets: Vec<BucketPlan> = ledger
        .buckets
        .iter()
        .map(|bucket| {
            let linked: Vec<_> = ledger
                .budget
                .iter()
                .filter(|item| item.bucket_id == bucket.id)
                .collect();
            let velocity: Money = linked.iter().map(|item| item.monthly_amount).sum();
            let contributions = if linked.is_empty() {
                Money::ZERO
            } else {
                Money::new(velocity.inner() * months)
            };

            let mut draws = Vec::new();
            let mut deductions = Money::ZERO;
            for expense in linked.iter().flat_map(|item| &item.expenses) {
                let hits = occurrences(expense, from, to);
                if hits == 0 {
                    continue;
                }
                let impact = expense.amount.scale(Decimal::from(hits));
                deductions += impact;
                draws.push(DrawHit {
                    name: expense.name.clone(),
                    amount: expense.amount,
                    occurrences: hits,
                    impact,
                    schedule: schedule_label(expense),
                });
            }

            let current = bucket_worth(ledger, &bucket.id).total;
            let projected = current + contributions - deductions;
            let target = bucket.target_amount.filter(|t| t.inner() > Decimal::ZERO);
            let percent = target.map(|t| {
                (projected.inner() / t.inner() * Decimal::from(100))
                    .min(Decimal::from(100))
                    .to_f64()
                    .unwrap_or(0.0)
            });
            let months_to_goal = target.and_then(|t| {
                let remaining = t - projected;
                if !remaining.inner().is_sign_positive() || remaining.is_zero() {
                    Some(Decimal::ZERO)
                } else if velocity.inner() > Decimal::ZERO {
                    Some(remaining.inner() / velocity.inner())
                } else {
                    None
                }
            });

            BucketPlan {
                id: bucket.id.clone(),
                name: bucket.name.clone(),
                current,
                projected,
                velocity: (!linked.is_empty()).then_some(velocity),
                contributions,
                deductions,
                target,
                percent,
                months_to_goal,
                draws,
            }
        })
        .collect();
    buckets.sort_by(|a, b| b.projected.cmp(&a.projected));
    Plan { months, buckets }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::records::{Bucket, BudgetItem};

    fn day(text: &str) -> Day {
        Day::parse(text).expect("a real date")
    }

    fn draw(recurring: bool, date: &str, interval: &str) -> PlannedExpense {
        PlannedExpense {
            name: "Draw".into(),
            amount: Money::from(100),
            recurring,
            draw_date: date.into(),
            interval: interval.into(),
            ..Default::default()
        }
    }

    const NOW: &str = "2026-01-01";
    const IN_A_YEAR: &str = "2027-01-01";

    fn count(expense: &PlannedExpense) -> u32 {
        occurrences(expense, day(NOW), day(IN_A_YEAR))
    }

    #[test]
    fn dates_round_trip_and_refuse_what_does_not_exist() {
        for text in ["1970-01-01", "2024-02-29", "2026-12-31", "2000-03-01"] {
            assert_eq!(day(text).iso(), text);
        }
        for bad in [
            "2026-02-29",
            "2026-13-01",
            "2026-00-10",
            "2026-1-01",
            "",
            "yesterday",
        ] {
            assert_eq!(Day::parse(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_month_on_from_the_31st_runs_over_the_way_javascript_does() {
        assert_eq!(day("2026-01-31").plus_months(1).iso(), "2026-03-03");
        assert_eq!(day("2024-02-29").plus_months(12).iso(), "2025-03-01");
        assert_eq!(day("2026-11-15").plus_months(3).iso(), "2027-02-15");
    }

    #[test]
    fn months_between_two_dates() {
        let months = months_between(day(NOW), day(IN_A_YEAR));
        assert_eq!(months.round_dp(2), Decimal::new(1199, 2));
    }

    #[test]
    fn a_one_time_draw_counts_once_if_it_lands_in_the_window() {
        assert_eq!(count(&draw(false, "2026-06-01", "")), 1);
        assert_eq!(count(&draw(false, "2028-06-01", "")), 0);
        assert_eq!(count(&draw(false, "", "")), 0);
    }

    #[test]
    fn a_monthly_draw_from_the_start_of_the_window_hits_thirteen_times() {
        // Both ends are included: 1 January this year and next.
        assert_eq!(count(&draw(true, "2026-01-01", "monthly")), 13);
    }

    #[test]
    fn a_six_monthly_draw_hits_three_times_over_a_year_from_january() {
        assert_eq!(count(&draw(true, "2026-01-01", "every6months")), 3);
    }

    #[test]
    fn an_end_date_cuts_the_series_short() {
        let mut expense = draw(true, "2026-01-01", "monthly");
        expense.end_date = "2026-03-15".into();
        assert_eq!(count(&expense), 3);
    }

    #[test]
    fn a_draw_that_started_before_the_window_still_counts_from_today() {
        assert_eq!(count(&draw(true, "2020-01-01", "yearly")), 2);
    }

    #[test]
    fn a_recurring_draw_with_no_interval_terminates() {
        assert_eq!(count(&draw(true, "2026-01-01", "")), 1);
    }

    #[test]
    fn a_draw_today_counts_even_after_midnight_has_passed() {
        // The one place this departs from the prototype, which compared a
        // midnight draw date against a "now" carrying the time of day.
        let today = day("2026-05-10");
        let expense = draw(false, "2026-05-10", "");
        assert_eq!(occurrences(&expense, today, day("2026-06-01")), 1);
    }

    #[test]
    fn schedules_read_the_way_the_plugin_writes_them() {
        let mut expense = draw(true, "2026-02-01", "every6months");
        assert_eq!(schedule_label(&expense), "every 6 months from 2026-02-01");
        expense.end_date = "2027-02-01".into();
        assert_eq!(
            schedule_label(&expense),
            "every 6 months from 2026-02-01 until 2027-02-01"
        );
        assert_eq!(
            schedule_label(&draw(false, "2026-06-01", "")),
            "One-time on 2026-06-01"
        );
        assert_eq!(
            schedule_label(&draw(true, "", "monthly")),
            "Recurring, no date set"
        );
    }

    /// `rothBuckets` and `planItems` from `check-math.mjs`. There k1 is worth
    /// 28,400 through holdings and Roth basis; here it is held as cash, since
    /// what makes up a bucket's worth is the goals tests' business.
    fn oracle() -> Ledger {
        let mut ledger = Ledger::default();
        ledger.buckets.push(Bucket {
            id: "k1".into(),
            name: "Emergency".into(),
            current_total: Money::from(28_400),
            target_amount: Some(Money::from(20_000)),
            ..Default::default()
        });
        ledger.buckets.push(Bucket {
            id: "kR".into(),
            name: "Retirement".into(),
            current_total: Money::from(40_000),
            ..Default::default()
        });
        ledger.budget.push(BudgetItem {
            id: "b1".into(),
            name: "Emergency top-up".into(),
            bucket_id: "k1".into(),
            monthly_amount: Money::from(500),
            expenses: vec![PlannedExpense {
                name: "Insurance".into(),
                amount: Money::from(1_200),
                recurring: true,
                draw_date: "2026-02-01".into(),
                interval: "every6months".into(),
                ..Default::default()
            }],
            ..Default::default()
        });
        ledger
    }

    #[test]
    fn a_projection_adds_the_velocity_and_takes_the_draws_out() {
        let plan = project(&oracle(), day(NOW), day(IN_A_YEAR));
        let emergency = plan.buckets.iter().find(|b| b.id == "k1").unwrap();
        assert_eq!(
            emergency.contributions.inner().round(),
            Decimal::from(5_996)
        );
        assert_eq!(emergency.deductions, Money::from(2_400));
        assert_eq!(emergency.velocity, Some(Money::from(500)));
        assert_eq!(
            emergency.projected.inner().round(),
            Decimal::from(28_400 + 5_996 - 2_400)
        );
        assert_eq!(emergency.draws.len(), 1);
        assert_eq!(emergency.draws[0].occurrences, 2);
    }

    #[test]
    fn a_bucket_nothing_feeds_has_no_velocity() {
        let plan = project(&oracle(), day(NOW), day(IN_A_YEAR));
        let retirement = plan.buckets.iter().find(|b| b.id == "kR").unwrap();
        assert_eq!(retirement.velocity, None);
        assert_eq!(retirement.months_to_goal, None);
        assert_eq!(retirement.percent, None);
    }

    #[test]
    fn a_target_already_reached_is_zero_months_away() {
        let plan = project(&oracle(), day(NOW), day(IN_A_YEAR));
        let emergency = plan.buckets.iter().find(|b| b.id == "k1").unwrap();
        assert_eq!(emergency.months_to_goal, Some(Decimal::ZERO));
        assert_eq!(emergency.percent, Some(100.0));
    }

    #[test]
    fn months_to_goal_runs_from_the_projected_point() {
        let mut ledger = oracle();
        ledger.buckets[0].target_amount = Some(Money::from(40_000));
        let plan = project(&ledger, day(NOW), day(IN_A_YEAR));
        let emergency = plan.buckets.iter().find(|b| b.id == "k1").unwrap();
        let expected = (Money::from(40_000) - emergency.projected).inner() / Decimal::from(500);
        assert_eq!(emergency.months_to_goal, Some(expected));
    }

    #[test]
    fn the_largest_projected_balance_comes_first() {
        let plan = project(&oracle(), day(NOW), day(IN_A_YEAR));
        let ids: Vec<_> = plan.buckets.iter().map(|b| b.id.as_str()).collect();
        assert_eq!(ids, ["kR", "k1"]);
    }
}
