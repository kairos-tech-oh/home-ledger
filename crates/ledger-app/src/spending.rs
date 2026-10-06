//! The Spending screen: what the itemised card charges add up to, and what
//! settling them took out of the buckets.

use crate::commands::{Answer, CommandError};
use crate::state::AppState;
use ledger_domain::{Ledger, Money};
use ledger_math::calendar::Day;
use ledger_math::spending::{self, Charge, Group, Report, Status};
use rust_decimal::prelude::ToPrimitive;
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GroupView {
    pub key: String,
    pub name: String,
    pub amount: String,
    pub count: usize,
    /// Against the largest row in the same breakdown, 0..=1, for its bar.
    pub share: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChargeView {
    /// Empty when undated.
    pub date: String,
    pub inferred: bool,
    pub name: String,
    pub member: String,
    pub bucket: String,
    pub card: String,
    pub status: &'static str,
    pub amount: String,
    pub reconciliation_id: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendingView {
    pub valid: bool,
    /// Empty for all time.
    pub start: String,
    pub end: String,
    pub total: String,
    pub count: usize,
    pub open: String,
    pub settled: String,
    pub from_buckets: String,
    pub everyday: String,
    pub withdrawn: String,
    pub unattributed: String,
    pub average: String,
    pub undated: usize,
    pub inferred_dates: usize,
    pub unitemized: String,
    pub missing_settlement_history: usize,
    pub undated_withdrawals: usize,
    pub people: Vec<GroupView>,
    pub items: Vec<GroupView>,
    pub months: Vec<GroupView>,
    pub cards: Vec<GroupView>,
    pub bucket_spending: Vec<GroupView>,
    pub withdrawals: Vec<GroupView>,
    pub transactions: Vec<ChargeView>,
    /// Names to offer: this machine's list, income owners, and names in use.
    pub members: Vec<String>,
    /// This machine's own list, for editing.
    pub family_members: Vec<String>,
}

/// `today` and `offset_minutes` come from the window, so periods end on the
/// person's own day and settlements land on it.
pub async fn spending(
    state: &AppState,
    period: String,
    from: String,
    to: String,
    status: String,
    today: String,
    offset_minutes: i64,
) -> Answer<SpendingView> {
    let Some(today) = Day::parse(&today) else {
        return Err(CommandError::Message("today must be yyyy-mm-dd".into()));
    };
    let loaded = state.live().await.engine.load().await?;
    let doc = match &loaded.snapshot {
        Some(s) => ledger_writer::read(&s.body)?,
        None => Ledger::default(),
    };
    let family = state.family_members().await;
    let window = spending::range(&period, &from, &to, today);
    let report = spending::analyse(&doc, window, Status::parse(&status), offset_minutes);
    Ok(spending_view(&doc, window, report, family))
}

pub async fn set_family_members(state: &AppState, names: Vec<String>) -> Answer<Vec<String>> {
    Ok(state.set_family_members(&names).await?)
}

fn groups(rows: Vec<Group>, by_count: bool) -> Vec<GroupView> {
    let measure = |g: &Group| {
        if by_count {
            g.count as f64
        } else {
            g.amount.inner().to_f64().unwrap_or(0.0)
        }
    };
    let top = rows.iter().map(measure).fold(0.0, f64::max);
    rows.into_iter()
        .map(|g| GroupView {
            share: if top > 0.0 {
                (measure(&g) / top).clamp(0.0, 1.0)
            } else {
                0.0
            },
            key: g.key,
            name: g.name,
            amount: g.amount.to_string(),
            count: g.count,
        })
        .collect()
}

fn charge(c: Charge) -> ChargeView {
    ChargeView {
        date: c.date.map(Day::iso).unwrap_or_default(),
        inferred: c.inferred,
        name: c.name,
        member: c.member,
        bucket: c.bucket,
        card: c.card,
        status: if c.settled { "settled" } else { "open" },
        amount: c.amount.to_string(),
        reconciliation_id: c.reconciliation_id,
    }
}

pub fn spending_view(
    doc: &Ledger,
    window: spending::Window,
    r: Report,
    family: Vec<String>,
) -> SpendingView {
    let money = |m: Money| m.to_string();
    SpendingView {
        valid: window.valid,
        start: window.start.map(Day::iso).unwrap_or_default(),
        end: window.end.map(Day::iso).unwrap_or_default(),
        total: money(r.total),
        count: r.count,
        open: money(r.open),
        settled: money(r.settled),
        from_buckets: money(r.from_buckets),
        everyday: money(r.everyday),
        withdrawn: money(r.withdrawn),
        unattributed: money(r.unattributed),
        average: money(r.average),
        undated: r.undated,
        inferred_dates: r.inferred_dates,
        unitemized: money(r.unitemized),
        missing_settlement_history: r.missing_settlement_history,
        undated_withdrawals: r.undated_withdrawals,
        people: groups(r.people, false),
        items: groups(r.items, true),
        months: groups(r.months, false),
        cards: groups(r.cards, false),
        bucket_spending: groups(r.bucket_spending, false),
        withdrawals: groups(r.withdrawals, false),
        transactions: r.transactions.into_iter().map(charge).collect(),
        members: spending::members(doc, &family),
        family_members: family,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::records::{ReconLine, Reconciliation};

    #[test]
    fn a_breakdown_bar_is_measured_against_its_largest_row() {
        let mut doc = Ledger::default();
        doc.reconciliations.push(Reconciliation {
            id: "r".into(),
            card: "Card".into(),
            statement_date: "2026-09-01".into(),
            status: "open".into(),
            lines: ["Coffee", "Coffee", "Coffee", "Fuel"]
                .into_iter()
                .zip([3, 3, 3, 40])
                .map(|(label, amount)| ReconLine {
                    label: label.into(),
                    amount: Money::from(amount),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        });
        let window = spending::Window::ALL;
        let report = spending::analyse(&doc, window, Status::All, 0);
        let view = spending_view(&doc, window, report, vec![]);
        // Items go by how often: Coffee three times is the full bar.
        assert_eq!(view.items[0].name, "Coffee");
        assert_eq!(view.items[0].share, 1.0);
        assert!((view.items[1].share - 1.0 / 3.0).abs() < 1e-9);
        // Months go by amount: one month holds everything.
        assert_eq!(view.months[0].share, 1.0);
        assert_eq!(view.transactions.len(), 4);
        assert_eq!(view.transactions[0].status, "open");
    }
}
