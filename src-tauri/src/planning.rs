//! The Planning screen: every bucket carried forward to a date.

use crate::commands::{Answer, CommandError};
use crate::state::AppState;
use ledger_domain::{Ledger, Money};
use ledger_math::planning::{self, Day};
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DrawView {
    pub name: String,
    pub amount: String,
    pub occurrences: u32,
    pub impact: String,
    pub schedule: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketPlanView {
    pub id: String,
    pub name: String,
    pub current: String,
    pub projected: String,
    /// Projected less current: what the window does to this bucket.
    pub change: String,
    pub velocity: Option<String>,
    pub contributions: String,
    pub deductions: String,
    pub target: Option<String>,
    pub percent: Option<f64>,
    /// To one decimal place.
    pub months_to_goal: Option<f64>,
    pub draws: Vec<DrawView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlanningView {
    pub from: String,
    pub to: String,
    /// To one decimal place.
    pub months: f64,
    /// Only the buckets the plan moves: one with no balance, nothing feeding it
    /// and nothing drawn from it has nothing to say.
    pub buckets: Vec<BucketPlanView>,
    pub current: String,
    pub contributions: String,
    pub deductions: String,
    pub projected: String,
    pub change: String,
}

/// `from` is today as the window sees it, so "today" is the person's own
/// calendar day rather than the date in UTC.
#[tauri::command]
pub async fn planning(
    state: State<'_, AppState>,
    from: String,
    to: String,
) -> Answer<PlanningView> {
    let (Some(from), Some(to)) = (Day::parse(&from), Day::parse(&to)) else {
        return Err(CommandError::Message("dates must be yyyy-mm-dd".into()));
    };
    if to <= from {
        return Err(CommandError::Message("choose a date in the future".into()));
    }
    let loaded = state.live().await.engine.load().await?;
    let doc = match &loaded.snapshot {
        Some(s) => ledger_writer::read(&s.body)?,
        None => Ledger::default(),
    };
    Ok(planning_view(&doc, from, to))
}

fn one_place(value: rust_decimal::Decimal) -> f64 {
    value.round_dp(1).to_f64().unwrap_or(0.0)
}

pub fn planning_view(doc: &Ledger, from: Day, to: Day) -> PlanningView {
    let plan = planning::project(doc, from, to);
    let moved: Vec<_> = plan
        .buckets
        .into_iter()
        .filter(|b| !(b.current.is_zero() && b.contributions.is_zero() && b.deductions.is_zero()))
        .collect();

    let sum = |pick: fn(&planning::BucketPlan) -> Money| -> Money { moved.iter().map(pick).sum() };
    let current = sum(|b| b.current);
    let contributions = sum(|b| b.contributions);
    let deductions = sum(|b| b.deductions);
    let projected = sum(|b| b.projected);

    PlanningView {
        from: from.iso(),
        to: to.iso(),
        months: one_place(plan.months),
        current: current.to_string(),
        contributions: contributions.to_string(),
        deductions: deductions.to_string(),
        projected: projected.to_string(),
        change: (projected - current).to_string(),
        buckets: moved
            .into_iter()
            .map(|b| BucketPlanView {
                change: (b.projected - b.current).to_string(),
                id: b.id,
                name: b.name,
                current: b.current.to_string(),
                projected: b.projected.to_string(),
                velocity: b.velocity.map(|v| v.to_string()),
                contributions: b.contributions.to_string(),
                deductions: b.deductions.to_string(),
                target: b.target.map(|t| t.to_string()),
                percent: b.percent,
                months_to_goal: b.months_to_goal.map(one_place),
                draws: b
                    .draws
                    .into_iter()
                    .map(|d| DrawView {
                        name: d.name,
                        amount: d.amount.to_string(),
                        occurrences: d.occurrences,
                        impact: d.impact.to_string(),
                        schedule: d.schedule,
                    })
                    .collect(),
            })
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::records::{Bucket, BudgetItem};

    #[test]
    fn a_bucket_the_plan_does_not_move_is_left_out_of_the_rows_and_the_totals() {
        let mut doc = Ledger::default();
        doc.buckets.push(Bucket {
            id: "idle".into(),
            name: "Idle".into(),
            ..Default::default()
        });
        doc.buckets.push(Bucket {
            id: "fed".into(),
            name: "Fed".into(),
            current_total: Money::from(100),
            ..Default::default()
        });
        doc.budget.push(BudgetItem {
            name: "Top-up".into(),
            bucket_id: "fed".into(),
            monthly_amount: Money::from(10),
            ..Default::default()
        });
        let from = Day::parse("2026-01-01").unwrap();
        let to = Day::parse("2027-01-01").unwrap();
        let view = planning_view(&doc, from, to);
        assert_eq!(view.buckets.len(), 1);
        assert_eq!(view.buckets[0].id, "fed");
        assert_eq!(view.current, "100.00");
        assert_eq!(view.months, 12.0);
        // 10 a month over 11.99 months.
        assert_eq!(view.contributions, "119.92");
        assert_eq!(view.change, "119.92");
    }
}
