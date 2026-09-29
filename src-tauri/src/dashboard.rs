//! The Dashboard screen: the stored layout (or the default), and everything
//! each widget draws, worked out here so the screen only renders.

use crate::commands::{Answer, CommandError};
use crate::state::AppState;
use crate::views::amount;
use ledger_domain::dashboard::{SPENDING_BREAKDOWNS, SPENDING_STATS, SpendingOptions, Widget};
use ledger_domain::{Ledger, Money};
use ledger_math::calendar::Day;
use ledger_math::dashboard as dash;
use ledger_math::snapshots::{self, Point};
use ledger_math::spending::{self, Status};
use serde::Serialize;
use tauri::State;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    pub id: String,
    pub name: String,
    /// A second line: the bucket behind a goal.
    pub note: String,
    /// An account's kind, for the screen to label.
    pub kind: String,
    /// Room left on a card or HELOC, when recorded.
    pub available: Option<String>,
    pub value: String,
    pub target: Option<String>,
    /// 0..=100 against the target, None without one.
    pub percent: Option<f64>,
    /// Money owed rather than held.
    pub owes: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stat {
    pub key: String,
    pub label: String,
    /// A count when false, money when true.
    pub money: bool,
    pub value: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BreakdownRow {
    pub name: String,
    pub amount: String,
    pub count: usize,
    pub share: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Breakdown {
    pub key: String,
    pub label: String,
    pub frequency: bool,
    pub rows: Vec<BreakdownRow>,
    /// How many more there are than the widget shows.
    pub more: usize,
}

/// What a widget draws, by kind.
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum Figures {
    #[serde(rename_all = "camelCase")]
    Networth {
        net: String,
        assets: String,
        debts: String,
        /// Over the last 30 days. None until there are two days of history.
        change: Option<String>,
        /// Oldest first, scaled 0..=1, up to 30 points.
        history: Vec<f64>,
    },
    #[serde(rename_all = "camelCase")]
    Rows {
        rows: Vec<Row>,
        /// Across the rows, where adding them up means something.
        total: Option<String>,
        /// Chosen things that have since been deleted.
        missing: usize,
    },
    #[serde(rename_all = "camelCase")]
    Retirement {
        total: String,
        roth: String,
        traditional: String,
        roth_share: f64,
        roth_contributions: String,
    },
    #[serde(rename_all = "camelCase")]
    Reconciliation {
        days_since: Option<i64>,
        open: usize,
        open_balance: String,
        year: i64,
        year_spending: String,
        year_buckets: String,
    },
    #[serde(rename_all = "camelCase")]
    Cashflow {
        income: String,
        budgeted: String,
        left: String,
        percent: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    Credit {
        available: String,
        limit: String,
        count: usize,
        recorded: usize,
        utilisation: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    Holdings {
        value: String,
        basis: String,
        gain: String,
        percent: Option<f64>,
        count: usize,
    },
    #[serde(rename_all = "camelCase")]
    Spending {
        valid: bool,
        period: String,
        count: usize,
        stats: Vec<Stat>,
        breakdowns: Vec<Breakdown>,
    },
    /// A kind from a newer version of the app, kept but not drawn.
    Unknown,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WidgetView {
    #[serde(flatten)]
    pub widget: Widget,
    /// The kind's own name, for the header when no title is set.
    pub label: String,
    /// The screen this widget summarises, empty when none.
    pub page: &'static str,
    pub figures: Figures,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KindView {
    pub kind: &'static str,
    pub label: &'static str,
    pub picks: &'static str,
    pub span: u8,
    pub description: &'static str,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choice {
    pub id: String,
    pub name: String,
    /// What it holds, owes or has saved.
    pub value: String,
    pub target: Option<String>,
    pub percent: Option<f64>,
    /// An account's kind, for the screen to label.
    pub kind: String,
    pub owes: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Choices {
    pub buckets: Vec<Choice>,
    pub accounts: Vec<Choice>,
    pub goals: Vec<Choice>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DashboardView {
    pub widgets: Vec<WidgetView>,
    /// Nothing is saved yet; this is the layout built from the ledger.
    pub is_default: bool,
    /// Every kind that can be added.
    pub kinds: Vec<KindView>,
    /// What a widget can be pointed at, by name.
    pub choices: Choices,
    /// What a new widget of each picking kind starts on.
    pub suggested: SuggestedRefs,
    /// A fresh default layout, for "start again".
    pub default_layout: Vec<Widget>,
    /// Days of snapshot history held.
    pub history_days: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SuggestedRefs {
    pub buckets: Vec<String>,
    pub accounts: Vec<String>,
    pub goals: Vec<String>,
}

#[tauri::command]
pub async fn dashboard(
    state: State<'_, AppState>,
    today: String,
    offset_minutes: i64,
) -> Answer<DashboardView> {
    let Some(today) = Day::parse(&today) else {
        return Err(CommandError::Message("today must be yyyy-mm-dd".into()));
    };
    let loaded = state.live().await.engine.load().await?;
    let doc = match &loaded.snapshot {
        Some(s) => ledger_writer::read(&s.body)?,
        None => Ledger::default(),
    };
    let points = crate::snapshots::LocalPoints::new(&state.places.data_dir)
        .read()
        .await;
    Ok(dashboard_view(&doc, &points, today, offset_minutes))
}

pub fn dashboard_view(doc: &Ledger, points: &[Point], today: Day, offset: i64) -> DashboardView {
    let (layout, is_default) = dash::dashboard_of(doc);
    let positive = |t: Option<Money>| t.filter(|t| t.inner() > rust_decimal::Decimal::ZERO);
    let mut buckets: Vec<(Money, Choice)> = doc
        .buckets
        .iter()
        .map(|b| {
            let worth = ledger_math::bucket_worth(doc, &b.id).total;
            let choice = Choice {
                id: b.id.clone(),
                name: b.name.clone(),
                value: amount(worth),
                target: positive(b.target_amount).map(amount),
                percent: None,
                kind: String::new(),
                owes: false,
            };
            (worth, choice)
        })
        .collect();
    buckets.sort_by(|a, b| b.0.cmp(&a.0));
    let mut accounts: Vec<Choice> = doc
        .accounts
        .iter()
        .map(|a| {
            let owes = a.is_liability();
            Choice {
                id: a.id.clone(),
                name: a.name.clone(),
                value: amount(if owes {
                    ledger_math::gross_owed(a)
                } else {
                    ledger_math::account_net(doc, a)
                }),
                target: None,
                percent: None,
                kind: a.kind.clone(),
                owes,
            }
        })
        .collect();
    accounts.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    let goals: Vec<Choice> = doc
        .goals
        .iter()
        .map(|g| Choice {
            id: g.id.clone(),
            name: g.name.clone(),
            value: amount(ledger_math::goal_saved(doc, g)),
            target: positive(g.target_amount).map(amount),
            percent: ledger_math::goal_progress(doc, g),
            kind: String::new(),
            owes: false,
        })
        .collect();
    DashboardView {
        widgets: layout
            .widgets
            .into_iter()
            .map(|widget| {
                let figures = figures(doc, &widget, points, today, offset);
                WidgetView {
                    label: dash::kind(&widget.kind)
                        .map(|k| k.label.to_string())
                        .unwrap_or_else(|| "Unknown widget".into()),
                    page: page_for(&widget.kind),
                    figures,
                    widget,
                }
            })
            .collect(),
        is_default,
        kinds: dash::KINDS
            .iter()
            .map(|k| KindView {
                kind: k.kind,
                label: k.label,
                picks: k.picks,
                span: k.span,
                description: k.description,
            })
            .collect(),
        choices: Choices {
            buckets: buckets.into_iter().map(|(_, c)| c).collect(),
            accounts,
            goals,
        },
        suggested: SuggestedRefs {
            buckets: dash::suggested_refs(doc, "buckets"),
            accounts: dash::suggested_refs(doc, "accounts"),
            goals: dash::suggested_refs(doc, "goals"),
        },
        default_layout: dash::default_dashboard(doc).widgets,
        history_days: points.len(),
    }
}

fn page_for(kind: &str) -> &'static str {
    match kind {
        "buckets" => "savings",
        "accounts" | "networth" | "credit" => "accounts",
        "retirement" => "retirement",
        "goals" => "goals",
        "reconciliation" => "reconcile",
        "cashflow" => "budget",
        "holdings" => "holdings",
        "spending" => "spending",
        _ => "",
    }
}

fn percent_of(value: Money, target: Money) -> f64 {
    (value.to_f64() / target.to_f64() * 100.0).clamp(0.0, 100.0)
}

fn figures(doc: &Ledger, widget: &Widget, points: &[Point], today: Day, offset: i64) -> Figures {
    match widget.kind.as_str() {
        "networth" => {
            let sheet = ledger_math::net_worth(doc);
            Figures::Networth {
                net: amount(sheet.net),
                assets: amount(sheet.assets),
                debts: amount(sheet.debts),
                change: snapshots::change(points, snapshots::net, 30, today)
                    .map(|c| amount(c.delta)),
                history: snapshots::normalised(points, snapshots::net, 30),
            }
        }
        "buckets" => {
            let mut total = Money::ZERO;
            let rows: Vec<Row> = widget
                .refs
                .iter()
                .filter_map(|id| doc.bucket(id))
                .map(|b| {
                    let worth = ledger_math::bucket_worth(doc, &b.id).total;
                    total += worth;
                    let target = b
                        .target_amount
                        .filter(|t| t.inner() > rust_decimal::Decimal::ZERO);
                    Row {
                        id: b.id.clone(),
                        name: b.name.clone(),
                        note: String::new(),
                        kind: String::new(),
                        available: None,
                        value: amount(worth),
                        target: target.map(amount),
                        percent: target.map(|t| percent_of(worth, t)),
                        owes: false,
                    }
                })
                .collect();
            Figures::Rows {
                missing: widget.refs.len() - rows.len(),
                total: (rows.len() > 1).then(|| amount(total)),
                rows,
            }
        }
        "accounts" => {
            let rows: Vec<Row> = widget
                .refs
                .iter()
                .filter_map(|id| doc.account(id))
                .map(|a| {
                    let owes = a.is_liability();
                    let value = if owes {
                        ledger_math::gross_owed(a)
                    } else {
                        ledger_math::account_net(doc, a)
                    };
                    Row {
                        id: a.id.clone(),
                        name: a.name.clone(),
                        note: String::new(),
                        kind: a.kind.clone(),
                        available: a
                            .available_credit
                            .filter(|_| a.takes_credit_limit())
                            .map(amount),
                        value: amount(value),
                        target: None,
                        percent: None,
                        owes,
                    }
                })
                .collect();
            Figures::Rows {
                missing: widget.refs.len() - rows.len(),
                total: None,
                rows,
            }
        }
        "goals" => {
            let rows: Vec<Row> = widget
                .refs
                .iter()
                .filter_map(|id| doc.goals.iter().find(|g| &g.id == id))
                .map(|g| Row {
                    id: g.id.clone(),
                    name: g.name.clone(),
                    note: doc
                        .bucket(&g.bucket_id)
                        .map(|b| b.name.clone())
                        .unwrap_or_default(),
                    kind: String::new(),
                    available: None,
                    value: amount(ledger_math::goal_saved(doc, g)),
                    target: g
                        .target_amount
                        .filter(|t| t.inner() > rust_decimal::Decimal::ZERO)
                        .map(amount),
                    percent: ledger_math::goal_progress(doc, g),
                    owes: false,
                })
                .collect();
            Figures::Rows {
                missing: widget.refs.len() - rows.len(),
                total: None,
                rows,
            }
        }
        "retirement" => {
            let r = dash::retirement_split(doc);
            Figures::Retirement {
                roth_share: if r.total.inner() > rust_decimal::Decimal::ZERO {
                    (r.roth.to_f64() / r.total.to_f64()).clamp(0.0, 1.0)
                } else {
                    0.0
                },
                total: amount(r.total),
                roth: amount(r.roth),
                traditional: amount(r.traditional),
                roth_contributions: amount(r.roth_contributions),
            }
        }
        "reconciliation" => {
            let r = dash::reconciling(doc, today, offset);
            Figures::Reconciliation {
                days_since: r.days_since,
                open: r.open,
                open_balance: amount(r.open_balance),
                year: r.year,
                year_spending: amount(r.year_spending),
                year_buckets: amount(r.year_buckets),
            }
        }
        "cashflow" => {
            let income = ledger_math::monthly_income(doc);
            let budgeted = ledger_math::monthly_budget(doc);
            Figures::Cashflow {
                percent: (income.inner() > rust_decimal::Decimal::ZERO)
                    .then(|| budgeted.to_f64() / income.to_f64() * 100.0),
                income: amount(income),
                budgeted: amount(budgeted),
                left: amount(income - budgeted),
            }
        }
        "credit" => {
            let c = dash::credit(doc);
            Figures::Credit {
                available: amount(c.available),
                limit: amount(c.limit),
                count: c.count,
                recorded: c.recorded,
                utilisation: c.utilisation,
            }
        }
        "holdings" => {
            let h = dash::holdings_summary(doc);
            Figures::Holdings {
                value: amount(h.value),
                basis: amount(h.basis),
                gain: amount(h.gain),
                percent: h.percent,
                count: h.count,
            }
        }
        "spending" => spending_figures(
            doc,
            widget.options.clone().unwrap_or_default(),
            today,
            offset,
        ),
        _ => Figures::Unknown,
    }
}

const STAT_LABELS: [(&str, &str, bool); 9] = [
    ("total", "Itemized spending", true),
    ("count", "Charges", false),
    ("open", "Open charges", true),
    ("settled", "Settled charges", true),
    ("withdrawn", "Buckets withdrawn", true),
    ("fromBuckets", "Bucket-funded", true),
    ("everyday", "Everyday spending", true),
    ("average", "Average charge", true),
    ("unattributed", "Unattributed", true),
];

const BREAKDOWN_LABELS: [(&str, &str, bool); 6] = [
    ("people", "Who spent the most", false),
    ("items", "Most frequent items", true),
    ("months", "Monthly spending", false),
    ("cards", "Spending by card", false),
    ("bucketSpending", "Charges assigned to buckets", false),
    ("withdrawals", "Actual bucket withdrawals", false),
];

/// Port of `spendingWidgetData` in `core/Spending.js`.
pub fn spending_figures(
    doc: &Ledger,
    options: SpendingOptions,
    today: Day,
    offset: i64,
) -> Figures {
    debug_assert_eq!(STAT_LABELS.len(), SPENDING_STATS.len());
    debug_assert_eq!(BREAKDOWN_LABELS.len(), SPENDING_BREAKDOWNS.len());
    let window = spending::range(&options.period, &options.from, &options.to, today);
    let report = spending::analyse(doc, window, Status::parse(&options.status), offset);
    let status = match options.status.as_str() {
        "settled" => "Settled only",
        "open" => "Open only",
        _ => "Open and settled",
    };
    let period = if !window.valid {
        "Choose a valid custom range".to_string()
    } else if options.period == "all" {
        format!("All time · {status}")
    } else {
        format!(
            "{} to {} · {status}",
            window.start.map(Day::iso).unwrap_or_default(),
            window.end.map(Day::iso).unwrap_or_default()
        )
    };

    let stat_value = |key: &str| -> String {
        match key {
            "count" => report.count.to_string(),
            "total" => amount(report.total),
            "open" => amount(report.open),
            "settled" => amount(report.settled),
            "withdrawn" => amount(report.withdrawn),
            "fromBuckets" => amount(report.from_buckets),
            "everyday" => amount(report.everyday),
            "average" => amount(report.average),
            _ => amount(report.unattributed),
        }
    };
    let stats = STAT_LABELS
        .iter()
        .filter(|(key, _, _)| options.stats.iter().any(|s| s == key))
        .map(|(key, label, money)| Stat {
            key: key.to_string(),
            label: label.to_string(),
            money: *money,
            value: stat_value(key),
        })
        .collect();

    let limit = options.limit as usize;
    let breakdowns = BREAKDOWN_LABELS
        .iter()
        .filter(|(key, _, _)| options.breakdowns.iter().any(|b| b == key))
        .map(|(key, label, frequency)| {
            let rows = match *key {
                "people" => &report.people,
                "items" => &report.items,
                "months" => &report.months,
                "cards" => &report.cards,
                "bucketSpending" => &report.bucket_spending,
                _ => &report.withdrawals,
            };
            let measure = |g: &spending::Group| {
                if *frequency {
                    g.count as f64
                } else {
                    g.amount.to_f64()
                }
            };
            let top = rows.iter().map(measure).fold(0.0, f64::max);
            Breakdown {
                key: key.to_string(),
                label: label.to_string(),
                frequency: *frequency,
                more: rows.len().saturating_sub(limit),
                rows: rows
                    .iter()
                    .take(limit)
                    .map(|g| BreakdownRow {
                        name: g.name.clone(),
                        amount: amount(g.amount),
                        count: g.count,
                        share: if top > 0.0 { measure(g) / top } else { 0.0 },
                    })
                    .collect(),
            }
        })
        .collect();

    Figures::Spending {
        valid: window.valid,
        period,
        count: report.count,
        stats,
        breakdowns,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::dashboard::clean_spending_options;
    use ledger_domain::records::{Bucket, ReconLine, Reconciliation};
    use serde_json::json;

    fn day(text: &str) -> Day {
        Day::parse(text).unwrap()
    }

    /// `records` from `check-spending.mjs`, cut down to what the widget cases use.
    fn doc() -> Ledger {
        let mut doc = Ledger::default();
        doc.buckets.push(Bucket {
            id: "b".into(),
            name: "Travel".into(),
            ..Default::default()
        });
        let line = |label: &str, member: &str, cents: i64, on: &str, bucket: &str| ReconLine {
            label: label.into(),
            member: member.into(),
            amount: Money::new(rust_decimal::Decimal::new(cents, 2)),
            spent_on: on.into(),
            bucket_id: bucket.into(),
            ..Default::default()
        };
        doc.reconciliations.push(Reconciliation {
            id: "one".into(),
            card: "Card".into(),
            statement_date: "2026-09-16".into(),
            balance: Money::from(100),
            status: "open".into(),
            lines: vec![
                line("  Groceries  ", "Chris", 2010, "2026-08-31", "b"),
                line("groceries", "chris", 3020, "2026-09-01", ""),
                line("Fuel", "Sam", 970, "", ""),
            ],
            ..Default::default()
        });
        doc
    }

    #[test]
    fn a_spending_widget_shows_what_was_chosen_in_the_plugins_order() {
        let options = clean_spending_options(Some(&json!({
            "period": "all", "stats": ["count", "total"], "breakdowns": ["items", "people"], "limit": 3
        })));
        let Figures::Spending {
            valid,
            period,
            stats,
            breakdowns,
            ..
        } = spending_figures(&doc(), options, day("2026-09-16"), 0)
        else {
            panic!("not a spending widget");
        };
        assert!(valid);
        assert_eq!(period, "All time · Open and settled");
        let keys: Vec<_> = stats.iter().map(|s| (s.key.as_str(), s.money)).collect();
        assert_eq!(keys, [("total", true), ("count", false)]);
        assert_eq!(stats[1].value, "3");
        let order: Vec<_> = breakdowns.iter().map(|b| b.key.as_str()).collect();
        assert_eq!(order, ["people", "items"]);
        let items = &breakdowns[1];
        assert!(items.frequency);
        assert_eq!(items.rows[0].share, 1.0);
        assert!(items.rows.iter().all(|r| r.share > 0.0 && r.share <= 1.0));
    }

    #[test]
    fn a_bad_custom_range_says_so_and_counts_nothing() {
        let options = clean_spending_options(Some(&json!({
            "period": "custom", "from": "2026-09-30", "to": "2026-09-01"
        })));
        let Figures::Spending {
            valid,
            period,
            count,
            ..
        } = spending_figures(&doc(), options, day("2026-09-16"), 0)
        else {
            panic!("not a spending widget");
        };
        assert!(!valid);
        assert_eq!(count, 0);
        assert_eq!(period, "Choose a valid custom range");
    }

    #[test]
    fn a_period_names_its_dates() {
        let options =
            clean_spending_options(Some(&json!({ "period": "3m", "breakdowns": ["months"] })));
        let Figures::Spending {
            period, breakdowns, ..
        } = spending_figures(&Ledger::default(), options, day("2026-09-16"), 0)
        else {
            panic!("not a spending widget");
        };
        assert_eq!(period, "2026-06-16 to 2026-09-16 · Open and settled");
        assert!(breakdowns[0].rows.is_empty());
    }

    #[test]
    fn a_buckets_widget_lists_its_buckets_in_the_order_chosen_and_counts_the_deleted() {
        let mut doc = Ledger::default();
        let (trip, fund) = ("1".repeat(32), "2".repeat(32));
        doc.buckets.push(Bucket {
            id: trip.clone(),
            name: "Trip".into(),
            current_total: Money::from(900),
            target_amount: Some(Money::from(1_000)),
            ..Default::default()
        });
        doc.buckets.push(Bucket {
            id: fund.clone(),
            name: "Emergency".into(),
            current_total: Money::from(20_000),
            target_amount: Some(Money::from(15_000)),
            ..Default::default()
        });
        let widget = Widget {
            id: "w".into(),
            kind: "buckets".into(),
            title: String::new(),
            span: 1,
            refs: vec![trip, fund, "3".repeat(32)],
            options: None,
        };
        let Figures::Rows {
            rows,
            missing,
            total,
        } = figures(&doc, &widget, &[], day("2026-09-15"), 0)
        else {
            panic!("not rows");
        };
        let names: Vec<_> = rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(names, ["Trip", "Emergency"]);
        // Progress stops at full.
        assert_eq!(rows[1].percent, Some(100.0));
        assert_eq!(missing, 1);
        assert_eq!(total.as_deref(), Some("20900.00"));
    }

    #[test]
    fn an_unknown_kind_has_no_figures_rather_than_failing() {
        let widget = Widget {
            id: "w".into(),
            kind: "future-widget".into(),
            title: String::new(),
            span: 1,
            refs: vec![],
            options: None,
        };
        assert!(matches!(
            figures(&Ledger::default(), &widget, &[], day("2026-09-15"), 0),
            Figures::Unknown
        ));
    }

    #[test]
    fn net_worth_moves_with_its_history() {
        let mut doc = Ledger::default();
        doc.accounts.push(ledger_domain::records::Account {
            id: "a".repeat(32),
            kind: "checking".into(),
            total: Some(Money::from(1_000)),
            ..Default::default()
        });
        let point = |at: &str, net: i64| Point {
            at: at.into(),
            net: Money::from(net),
            ..Default::default()
        };
        let points = [point("2026-09-01", 900), point("2026-09-14", 1_000)];
        let widget = dash::new_widget(&doc, "networth", "w".into());
        let Figures::Networth {
            net,
            change,
            history,
            ..
        } = figures(&doc, &widget, &points, day("2026-09-15"), 0)
        else {
            panic!("not net worth");
        };
        assert_eq!(net, "1000.00");
        assert_eq!(change.as_deref(), Some("100.00"));
        assert_eq!(history, [0.0, 1.0]);
    }
}
