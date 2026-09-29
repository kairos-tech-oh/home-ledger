//! The dashboard's kinds of widget, the layout a first open shows, and the
//! figures only the dashboard needs.
//!
//! Port of `DASHBOARD_KINDS`, `newWidget`, `defaultDashboard`, `dashboardOf`,
//! `creditRollup`, `reconRollup` and the per-kind parts of `widgetData` in
//! `core/Model.js`, checked against the dashboard cases in `check-math.mjs`.

use crate::calendar::Day;
use crate::{account_net, bucket_worth, gross_owed, holding_gain, holding_worth};
use ledger_domain::dashboard::{Dashboard, SpendingOptions, Widget};
use ledger_domain::{Ledger, Money};
use rust_decimal::Decimal;

/// One kind of widget. `picks` names what it is pointed at, if anything.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Kind {
    pub kind: &'static str,
    pub label: &'static str,
    pub picks: &'static str,
    pub span: u8,
    pub description: &'static str,
}

pub const KINDS: [Kind; 10] = [
    Kind {
        kind: "networth",
        label: "Net worth",
        picks: "",
        span: 2,
        description: "Your net worth, with its change over the last 30 days.",
    },
    Kind {
        kind: "buckets",
        label: "Savings buckets",
        picks: "buckets",
        span: 1,
        description: "The buckets you choose, each with its progress to target.",
    },
    Kind {
        kind: "accounts",
        label: "Accounts",
        picks: "accounts",
        span: 1,
        description: "The accounts you choose, with what each holds or owes.",
    },
    Kind {
        kind: "goals",
        label: "Goals",
        picks: "goals",
        span: 1,
        description: "The goals you choose, with how far along each one is.",
    },
    Kind {
        kind: "retirement",
        label: "Retirement",
        picks: "",
        span: 1,
        description: "Every retirement account, split into Roth and Traditional.",
    },
    Kind {
        kind: "reconciliation",
        label: "Reconciliation",
        picks: "",
        span: 1,
        description: "Open reconciliations, days since the last one, and this year's spending.",
    },
    Kind {
        kind: "cashflow",
        label: "Cash flow",
        picks: "",
        span: 1,
        description: "Monthly income against what is budgeted, and what is left.",
    },
    Kind {
        kind: "credit",
        label: "Available credit",
        picks: "",
        span: 1,
        description: "Room left across credit cards and HELOCs, and how much is in use.",
    },
    Kind {
        kind: "holdings",
        label: "Holdings",
        picks: "",
        span: 1,
        description: "What your holdings are worth and what they have gained.",
    },
    Kind {
        kind: "spending",
        label: "Spending",
        picks: "",
        span: 2,
        description: "Reconciled spending for a period you choose, with the figures and breakdowns you pick.",
    },
];

pub fn kind(kind: &str) -> Option<&'static Kind> {
    KINDS.iter().find(|k| k.kind == kind)
}

/// A fixed 32-hex id from a short seed, as the plugin makes it. The default
/// dashboard is rebuilt every time the ledger changes; random ids would make
/// an edit aimed at one of its widgets miss.
pub fn stable_id(seed: &str) -> String {
    let mut hex: String = seed
        .encode_utf16()
        .map(|unit| format!("{:02x}", unit % 256))
        .collect::<String>();
    hex.truncate(32);
    while hex.len() < 32 {
        hex.push('0');
    }
    hex
}

/// Buckets largest first.
fn buckets_by_total(ledger: &Ledger) -> Vec<(&ledger_domain::Bucket, Money)> {
    let mut rows: Vec<_> = ledger
        .buckets
        .iter()
        .map(|b| (b, bucket_worth(ledger, &b.id).total))
        .collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.1));
    rows
}

/// What a new widget of a kind is pointed at to begin with: up to four of the
/// things worth watching.
pub fn suggested_refs(ledger: &Ledger, kind: &str) -> Vec<String> {
    match kind {
        "buckets" => {
            // Buckets working towards a target first: those are worth watching.
            let all = buckets_by_total(ledger);
            let targeted: Vec<_> = all
                .iter()
                .filter(|(b, _)| b.target_amount.is_some_and(|t| t.inner() > Decimal::ZERO))
                .collect();
            let pool: Vec<_> = if targeted.is_empty() {
                all.iter().collect()
            } else {
                targeted
            };
            pool.iter().take(4).map(|(b, _)| b.id.clone()).collect()
        }
        "accounts" => {
            let mut rows: Vec<_> = ledger
                .accounts
                .iter()
                .map(|a| (a, account_net(ledger, a).abs()))
                .collect();
            rows.sort_by_key(|row| std::cmp::Reverse(row.1));
            rows.iter().take(4).map(|(a, _)| a.id.clone()).collect()
        }
        "goals" => ledger.goals.iter().take(4).map(|g| g.id.clone()).collect(),
        _ => Vec::new(),
    }
}

/// A new widget of a kind, already pointed at something sensible.
pub fn new_widget(ledger: &Ledger, kind_name: &str, id: String) -> Widget {
    Widget {
        id,
        kind: kind_name.to_string(),
        title: String::new(),
        span: kind(kind_name).map(|k| k.span).unwrap_or(1),
        refs: suggested_refs(ledger, kind_name),
        options: (kind_name == "spending").then(SpendingOptions::default),
    }
}

/// What a first open shows, built from the ledger so it is never empty.
pub fn default_dashboard(ledger: &Ledger) -> Dashboard {
    let mut kinds = vec!["networth", "cashflow"];
    if !ledger.buckets.is_empty() {
        kinds.push("buckets");
    }
    if ledger
        .accounts
        .iter()
        .any(|a| a.kind.starts_with("retirement"))
    {
        kinds.push("retirement");
    }
    if !ledger.goals.is_empty() {
        kinds.push("goals");
    }
    kinds.push("reconciliation");
    if !ledger.reconciliations.is_empty() {
        kinds.push("spending");
    }
    if ledger.accounts.iter().any(|a| a.takes_credit_limit()) {
        kinds.push("credit");
    }
    Dashboard {
        v: 1,
        widgets: kinds
            .into_iter()
            .map(|k| new_widget(ledger, k, stable_id(&format!("dflt-{k}"))))
            .collect(),
    }
}

/// The stored layout, or the default when nothing has been saved. The flag
/// says which, so the screen can offer to start again from the default.
pub fn dashboard_of(ledger: &Ledger) -> (Dashboard, bool) {
    match ledger.dashboard() {
        Some(stored) => (stored, false),
        None => (default_dashboard(ledger), true),
    }
}

// ============================================================ figures

/// Room left to borrow across credit cards and HELOCs. Capacity, not money
/// held, so it never enters assets, debts or net worth.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Credit {
    pub available: Money,
    pub owed: Money,
    /// Owed plus available, over only the accounts that recorded both, so a
    /// missing figure does not inflate utilisation.
    pub limit: Money,
    pub count: usize,
    pub recorded: usize,
    pub utilisation: Option<f64>,
}

pub fn credit(ledger: &Ledger) -> Credit {
    let mut out = Credit::default();
    for account in ledger.accounts.iter().filter(|a| a.takes_credit_limit()) {
        out.count += 1;
        let Some(room) = account.available_credit else {
            continue;
        };
        out.recorded += 1;
        out.available += room;
        out.owed += gross_owed(account);
    }
    out.limit = out.owed + out.available;
    if out.limit.inner() > Decimal::ZERO {
        out.utilisation = Some(out.owed.to_f64() / out.limit.to_f64() * 100.0);
    }
    out
}

/// Where reconciling stands, and what this year's settled statements spent.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reconciling {
    pub open: usize,
    pub open_balance: Money,
    pub settled: usize,
    pub year: i64,
    /// Lines on no bucket, on statements settled this year.
    pub year_spending: Money,
    /// Lines on a bucket, on the same statements.
    pub year_buckets: Money,
    /// Whole days since the latest settle. None when nothing has settled.
    pub days_since: Option<i64>,
}

pub fn reconciling(ledger: &Ledger, today: Day, offset_minutes: i64) -> Reconciling {
    let (year, _, _) = today.ymd();
    let mut out = Reconciling {
        year,
        ..Default::default()
    };
    let mut last: Option<&str> = None;
    for rec in &ledger.reconciliations {
        if rec.status != "settled" {
            out.open += 1;
            out.open_balance += rec.balance;
            continue;
        }
        out.settled += 1;
        let dated = if rec.statement_date.is_empty() {
            &rec.settled_at
        } else {
            &rec.statement_date
        };
        if dated.get(..4) == Some(year.to_string().as_str()) {
            for line in &rec.lines {
                if line.bucket_id.is_empty() {
                    out.year_spending += line.amount;
                } else {
                    out.year_buckets += line.amount;
                }
            }
        }
        let at = if rec.settled_at.is_empty() {
            rec.statement_date.as_str()
        } else {
            rec.settled_at.as_str()
        };
        if !at.is_empty() && last.is_none_or(|l| at > l) {
            last = Some(at);
        }
    }
    // A bare date is a local day; a settle time is an instant.
    out.days_since = last
        .and_then(|at| {
            if at.len() <= 10 {
                Day::parse(at)
            } else {
                Day::of_timestamp(at, offset_minutes)
            }
        })
        .map(|day| day.days_until(today).max(0));
    out
}

/// Every retirement account's worth, Roth apart from Traditional.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RetirementSplit {
    pub roth: Money,
    pub traditional: Money,
    pub total: Money,
    /// What was put into the Roth accounts, which comes out without penalty.
    pub roth_contributions: Money,
}

pub fn retirement_split(ledger: &Ledger) -> RetirementSplit {
    let mut out = RetirementSplit::default();
    for account in ledger
        .accounts
        .iter()
        .filter(|a| a.kind.starts_with("retirement"))
    {
        let value = account_net(ledger, account);
        if account.kind == "retirement-roth" {
            out.roth += value;
            out.roth_contributions += account.contributions_amount.unwrap_or(Money::ZERO);
        } else {
            out.traditional += value;
        }
    }
    out.total = out.roth + out.traditional;
    out
}

/// What the holdings are worth, and what they have gained on what they cost.
/// A holding with no cost recorded adds to the value but not to the gain.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HoldingsSummary {
    pub value: Money,
    pub basis: Money,
    pub gain: Money,
    pub percent: Option<f64>,
    pub count: usize,
}

pub fn holdings_summary(ledger: &Ledger) -> HoldingsSummary {
    let mut out = HoldingsSummary {
        count: ledger.investments.len(),
        ..Default::default()
    };
    for holding in &ledger.investments {
        out.value += holding_worth(holding);
        if let Some(gain) = holding_gain(holding) {
            out.gain += gain;
            out.basis += holding.cost_basis.unwrap_or(Money::ZERO);
        }
    }
    if !out.basis.is_zero() {
        out.percent = Some(out.gain.to_f64() / out.basis.to_f64() * 100.0);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::records::{
        Account, Bucket, BudgetItem, Goal, IncomeStream, ReconLine, Reconciliation,
    };

    fn id(seed: &str) -> String {
        stable_id(seed)
    }

    /// The dashboard `doc` from `check-math.mjs`.
    fn oracle() -> Ledger {
        let mut doc = Ledger::default();
        let account = |seed: &str, name: &str, kind: &str, total: i64| Account {
            id: id(seed),
            name: name.into(),
            kind: kind.into(),
            total: Some(Money::from(total)),
            ..Default::default()
        };
        doc.accounts = vec![
            account("chk", "Checking", "checking", 2_000),
            Account {
                contributions_amount: Some(Money::from(12_000)),
                ..account("roth", "Roth IRA", "retirement-roth", 30_000)
            },
            account("trad", "401k", "retirement-traditional", 50_000),
            Account {
                available_credit: Some(Money::from(4_500)),
                ..account("card", "Card", "credit", 500)
            },
        ];
        let bucket = |seed: &str, name: &str, cash: i64, target: Option<i64>| Bucket {
            id: id(seed),
            name: name.into(),
            current_total: Money::from(cash),
            target_amount: target.map(Money::from),
            ..Default::default()
        };
        doc.buckets = vec![
            bucket("trip", "Trip", 900, Some(1_000)),
            bucket("fund", "Emergency", 20_000, Some(15_000)),
            bucket("misc", "Misc", 50, None),
        ];
        doc.goals = vec![Goal {
            id: id("g"),
            name: "Trip".into(),
            target_amount: Some(Money::from(1_000)),
            bucket_id: id("trip"),
            ..Default::default()
        }];
        doc.income = vec![IncomeStream {
            name: "Pay".into(),
            monthly_total: Money::from(6_000),
            frequency: "monthly".into(),
            ..Default::default()
        }];
        doc.budget = vec![BudgetItem {
            name: "Rent".into(),
            monthly_amount: Money::from(4_500),
            ..Default::default()
        }];
        let line = |amount: i64, bucket: &str| ReconLine {
            amount: Money::from(amount),
            bucket_id: bucket.into(),
            ..Default::default()
        };
        doc.reconciliations = vec![
            Reconciliation {
                id: "r1".into(),
                card: "Card".into(),
                status: "settled".into(),
                settled_at: "2026-09-05T12:00:00Z".into(),
                statement_date: "2026-09-05".into(),
                balance: Money::from(300),
                lines: vec![line(200, &id("trip")), line(100, "")],
                ..Default::default()
            },
            Reconciliation {
                id: "r2".into(),
                card: "Card".into(),
                status: "open".into(),
                statement_date: "2026-09-14".into(),
                balance: Money::from(80),
                ..Default::default()
            },
        ];
        doc
    }

    fn today() -> Day {
        Day::parse("2026-09-15").unwrap()
    }

    #[test]
    fn the_default_covers_what_the_ledger_has() {
        let kinds: Vec<_> = default_dashboard(&oracle())
            .widgets
            .iter()
            .map(|w| w.kind.clone())
            .collect();
        assert_eq!(
            kinds,
            [
                "networth",
                "cashflow",
                "buckets",
                "retirement",
                "goals",
                "reconciliation",
                "spending",
                "credit"
            ]
        );
    }

    #[test]
    fn the_default_leaves_spending_out_when_nothing_has_been_reconciled() {
        let mut doc = oracle();
        doc.reconciliations.clear();
        assert!(
            !default_dashboard(&doc)
                .widgets
                .iter()
                .any(|w| w.kind == "spending")
        );
    }

    #[test]
    fn the_default_is_rebuilt_with_the_same_ids() {
        let a = default_dashboard(&oracle());
        let b = default_dashboard(&oracle());
        assert_eq!(a, b);
        // The same id the plugin gives it, so a layout saved there lines up.
        assert_eq!(a.widgets[0].id, "64666c742d6e6574776f727468000000");
    }

    #[test]
    fn a_stored_dashboard_wins_over_the_default() {
        let mut doc = oracle();
        assert!(dashboard_of(&doc).1);
        doc.unknown
            .insert("dashboard".into(), serde_json::json!({ "widgets": [] }));
        let (dash, is_default) = dashboard_of(&doc);
        assert!(!is_default);
        assert!(dash.widgets.is_empty());
    }

    #[test]
    fn a_new_buckets_widget_starts_on_buckets_with_a_target_largest_first() {
        assert_eq!(
            suggested_refs(&oracle(), "buckets"),
            [id("fund"), id("trip")]
        );
    }

    #[test]
    fn a_new_accounts_widget_starts_on_the_largest_accounts() {
        assert_eq!(
            suggested_refs(&oracle(), "accounts"),
            [id("trad"), id("roth"), id("chk"), id("card")]
        );
    }

    #[test]
    fn a_new_spending_widget_carries_the_default_options() {
        let w = new_widget(&oracle(), "spending", "x".into());
        assert_eq!(w.span, 2);
        assert_eq!(w.options, Some(SpendingOptions::default()));
    }

    #[test]
    fn retirement_splits_roth_from_traditional_and_counts_roth_contributions() {
        let r = retirement_split(&oracle());
        assert_eq!(
            (r.roth, r.traditional, r.total),
            (
                Money::from(30_000),
                Money::from(50_000),
                Money::from(80_000)
            )
        );
        assert_eq!(r.roth_contributions, Money::from(12_000));
    }

    #[test]
    fn reconciliation_reports_days_since_the_last_settle_and_what_is_open() {
        let r = reconciling(&oracle(), today(), 0);
        assert_eq!(r.days_since, Some(10));
        assert_eq!((r.open, r.open_balance), (1, Money::from(80)));
        assert_eq!(
            (r.year_spending, r.year_buckets),
            (Money::from(100), Money::from(200))
        );
        assert_eq!(r.year, 2026);
    }

    #[test]
    fn nothing_settled_is_no_days_since() {
        let mut doc = oracle();
        doc.reconciliations.retain(|r| r.status != "settled");
        assert_eq!(reconciling(&doc, today(), 0).days_since, None);
    }

    #[test]
    fn credit_is_room_left_and_how_much_is_in_use() {
        let c = credit(&oracle());
        assert_eq!((c.count, c.recorded), (1, 1));
        assert_eq!(
            (c.available, c.owed, c.limit),
            (Money::from(4_500), Money::from(500), Money::from(5_000))
        );
        assert_eq!(c.utilisation, Some(10.0));
    }

    #[test]
    fn a_card_with_no_available_credit_recorded_does_not_inflate_utilisation() {
        let mut doc = oracle();
        doc.accounts[3].available_credit = None;
        let c = credit(&doc);
        assert_eq!((c.count, c.recorded, c.utilisation), (1, 0, None));
    }

    #[test]
    fn every_kind_the_plugin_knows_is_known() {
        for k in [
            "networth",
            "buckets",
            "accounts",
            "goals",
            "retirement",
            "reconciliation",
            "cashflow",
            "credit",
            "holdings",
            "spending",
        ] {
            assert!(kind(k).is_some(), "{k}");
        }
        assert!(kind("future-widget").is_none());
    }
}
