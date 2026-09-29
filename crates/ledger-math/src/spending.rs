//! What was actually spent, read from the itemised lines on card statements,
//! and what settling them actually took out of the savings buckets.
//!
//! Port of `core/Spending.js`, checked against `tools/check-spending.mjs`.
//! Only reconciliation charges count as spending: holdings, transfers and the
//! budget are plans or movements, not purchases.

use crate::calendar::{Day, days_in_month};
use ledger_domain::{Ledger, Money};
use rust_decimal::Decimal;
use std::cmp::Ordering;
use std::collections::HashMap;

/// The dates a report covers, both ends included. Unbounded is "all time".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Window {
    pub start: Option<Day>,
    pub end: Option<Day>,
    /// False for a custom range that is not a range, which reports nothing.
    pub valid: bool,
}

impl Window {
    pub const ALL: Window = Window {
        start: None,
        end: None,
        valid: true,
    };

    fn bounded(&self) -> bool {
        self.start.is_some() || self.end.is_some()
    }

    fn contains(&self, day: Day) -> bool {
        self.start.is_none_or(|s| day >= s) && self.end.is_none_or(|e| day <= e)
    }
}

/// The periods offered, as the plugin names them.
pub const PERIODS: [&str; 6] = ["1m", "3m", "6m", "1y", "all", "custom"];

/// The window a period covers, ending today. A month back from the 31st of
/// March is the 29th of February, not the 2nd of March.
pub fn range(period: &str, from: &str, to: &str, today: Day) -> Window {
    let months = match period {
        "all" => return Window::ALL,
        "custom" => {
            let (start, end) = (Day::parse(from), Day::parse(to));
            let valid = matches!((start, end), (Some(s), Some(e)) if s <= e);
            return Window { start, end, valid };
        }
        "3m" => 3,
        "6m" => 6,
        "1y" => 12,
        _ => 1,
    };
    let (y, m, d) = today.ymd();
    let (fy, fm, _) = Day::from_ymd(y, m - months, 1).ymd();
    Window {
        start: Some(Day::from_ymd(fy, fm, d.min(days_in_month(fy, fm)))),
        end: Some(today),
        valid: true,
    }
}

/// Which reconciliations count.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Status {
    #[default]
    All,
    Settled,
    Open,
}

impl Status {
    pub fn parse(text: &str) -> Status {
        match text {
            "settled" => Status::Settled,
            "open" => Status::Open,
            _ => Status::All,
        }
    }
}

/// Trimmed, with runs of whitespace as one space.
fn text(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Case-insensitive first, so "chris" sits beside "Chris" rather than after
/// every capital, the way `localeCompare` orders them; then exactly, so the
/// order never depends on which came first.
fn by_name(a: &str, b: &str) -> Ordering {
    a.to_lowercase()
        .cmp(&b.to_lowercase())
        .then_with(|| a.cmp(b))
}

/// Every family member name worth offering: those configured on this machine,
/// every income owner, and every name already used on a charge. One of each,
/// ignoring case, keeping the first spelling seen.
pub fn members(ledger: &Ledger, configured: &[String]) -> Vec<String> {
    let candidates = configured
        .iter()
        .map(String::as_str)
        .chain(ledger.income.iter().map(|i| i.owner.as_str()))
        .chain(
            ledger
                .reconciliations
                .iter()
                .flat_map(|r| &r.lines)
                .map(|l| l.member.as_str()),
        );
    let mut seen = std::collections::HashSet::new();
    let mut names: Vec<String> = candidates
        .map(text)
        .filter(|name| !name.is_empty() && seen.insert(name.to_lowercase()))
        .collect();
    names.sort_by(|a, b| by_name(a, b));
    names
}

/// One row of a breakdown: who, what, which card, which month.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Group {
    pub key: String,
    pub name: String,
    pub amount: Money,
    pub count: usize,
}

#[derive(Default)]
struct Grouper {
    rows: Vec<Group>,
    at: HashMap<String, usize>,
}

impl Grouper {
    /// The first name seen for a key is the one shown.
    fn add(&mut self, key: String, name: &str, amount: Money) {
        let index = *self.at.entry(key.clone()).or_insert_with(|| {
            self.rows.push(Group {
                key,
                name: name.to_string(),
                amount: Money::ZERO,
                count: 0,
            });
            self.rows.len() - 1
        });
        self.rows[index].amount += amount;
        self.rows[index].count += 1;
    }

    /// Largest first, ties by name.
    fn by_amount(mut self) -> Vec<Group> {
        self.rows.sort_by(|a, b| {
            b.amount
                .cmp(&a.amount)
                .then_with(|| by_name(&a.name, &b.name))
        });
        self.rows
    }
}

/// One charge that falls in the window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Charge {
    /// None when neither the charge nor its statement carries a date.
    pub date: Option<Day>,
    /// The date is the statement's or the settlement's, not the purchase's.
    pub inferred: bool,
    pub name: String,
    pub member: String,
    pub bucket: String,
    pub card: String,
    pub settled: bool,
    pub amount: Money,
    pub reconciliation_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Report {
    pub total: Money,
    pub count: usize,
    pub open: Money,
    pub settled: Money,
    /// Charges assigned to a savings bucket.
    pub from_buckets: Money,
    /// Charges assigned to no bucket.
    pub everyday: Money,
    /// What settlements actually took out of buckets.
    pub withdrawn: Money,
    /// Charges with no family member named.
    pub unattributed: Money,
    pub average: Money,
    /// Charges with no date of their own or their statement's.
    pub undated: usize,
    /// Charges dated by their statement or settlement instead of the purchase.
    pub inferred_dates: usize,
    /// Statement balances not itemised into lines, which are not spending.
    pub unitemized: Money,
    /// Settled before settlements recorded what they took.
    pub missing_settlement_history: usize,
    /// Settled with no settlement date; their debits show only in all time.
    pub undated_withdrawals: usize,
    pub people: Vec<Group>,
    /// Most often first: frequency counts lines, not units bought.
    pub items: Vec<Group>,
    pub bucket_spending: Vec<Group>,
    pub withdrawals: Vec<Group>,
    pub cards: Vec<Group>,
    /// Oldest first, undated last.
    pub months: Vec<Group>,
    /// Newest first, undated last; the larger first within a day.
    pub transactions: Vec<Charge>,
}

/// Everything the Spending screen shows. `offset_minutes` is how far the
/// person's clock is ahead of UTC, so a settlement lands on their own day.
pub fn analyse(ledger: &Ledger, window: Window, status: Status, offset_minutes: i64) -> Report {
    let mut report = Report::default();
    if !window.valid {
        return report;
    }

    let bucket_name = |id: &str| -> String {
        match ledger.bucket(id) {
            Some(b) => b.name.clone(),
            None => format!(
                "Deleted bucket · {}",
                id.chars().take(8).collect::<String>()
            ),
        }
    };

    let mut people = Grouper::default();
    let mut items = Grouper::default();
    let mut bucket_spending = Grouper::default();
    let mut withdrawals = Grouper::default();
    let mut cards = Grouper::default();
    let mut months = Grouper::default();

    for rec in &ledger.reconciliations {
        let settled = rec.status == "settled";
        match status {
            Status::Settled if !settled => continue,
            Status::Open if settled => continue,
            _ => {}
        }
        let statement = Day::parse(&rec.statement_date);
        let settled_day = Day::of_timestamp(&rec.settled_at, offset_minutes);
        let fallback = statement.or(settled_day);

        let mut allocated = Money::ZERO;
        for line in &rec.lines {
            let amount = line.amount;
            if amount.inner() <= Decimal::ZERO {
                continue;
            }
            allocated += amount;
            let own = Day::parse(&line.spent_on);
            let date = own.or(fallback);
            match date {
                None => {
                    report.undated += 1;
                    if window.bounded() {
                        continue;
                    }
                }
                Some(day) if !window.contains(day) => continue,
                Some(_) => {}
            }
            if own.is_none() && date.is_some() {
                report.inferred_dates += 1;
            }

            let member = text(&line.member);
            let label = match text(&line.label) {
                l if l.is_empty() => "Unnamed charge".to_string(),
                l => l,
            };
            report.total += amount;
            report.count += 1;
            if settled {
                report.settled += amount;
            } else {
                report.open += amount;
            }
            if member.is_empty() {
                report.unattributed += amount;
            }
            let bucket = if line.bucket_id.is_empty() {
                report.everyday += amount;
                "Everyday spending".to_string()
            } else {
                report.from_buckets += amount;
                let name = bucket_name(&line.bucket_id);
                bucket_spending.add(line.bucket_id.clone(), &name, amount);
                name
            };
            let who = if member.is_empty() {
                "Unattributed"
            } else {
                &member
            };
            people.add(member.to_lowercase(), who, amount);
            items.add(label.to_lowercase(), &label, amount);
            let card_key = if rec.card_account_id.is_empty() {
                text(&rec.card).to_lowercase()
            } else {
                rec.card_account_id.clone()
            };
            let card_name = if rec.card.is_empty() {
                "Unnamed card"
            } else {
                &rec.card
            };
            cards.add(card_key, card_name, amount);
            match date {
                Some(day) => {
                    let month = day.iso()[..7].to_string();
                    months.add(month.clone(), &month, amount);
                }
                None => months.add("undated".into(), "Undated", amount),
            }
            report.transactions.push(Charge {
                date,
                inferred: own.is_none(),
                name: label,
                member: who.to_string(),
                bucket,
                card: rec.card.clone(),
                settled,
                amount,
                reconciliation_id: rec.id.clone(),
            });
        }

        let counts = match fallback {
            Some(day) => window.contains(day),
            None => !window.bounded(),
        };
        if counts {
            report.unitemized += (rec.balance - allocated).floor_at_zero();
        }

        if settled && settled_day.is_none() {
            report.undated_withdrawals += 1;
        }
        let withdrawn_here = match settled_day {
            Some(day) => window.contains(day),
            None => !window.bounded(),
        };
        if settled && withdrawn_here {
            match &rec.applied {
                None => report.missing_settlement_history += 1,
                Some(applied) => {
                    for movement in &applied.buckets {
                        if movement.amount.inner() <= Decimal::ZERO {
                            continue;
                        }
                        report.withdrawn += movement.amount;
                        withdrawals.add(
                            movement.id.clone(),
                            &bucket_name(&movement.id),
                            movement.amount,
                        );
                    }
                }
            }
        }
    }

    report.people = people.by_amount();
    report.bucket_spending = bucket_spending.by_amount();
    report.withdrawals = withdrawals.by_amount();
    report.cards = cards.by_amount();
    let mut items = items.by_amount();
    items.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| b.amount.cmp(&a.amount))
            .then_with(|| by_name(&a.name, &b.name))
    });
    report.items = items;
    let mut months = months.rows;
    months.sort_by(|a, b| a.key.cmp(&b.key));
    report.months = months;
    // Newest first; an undated charge sorts as the empty string did, last.
    report
        .transactions
        .sort_by(|a, b| b.date.cmp(&a.date).then_with(|| b.amount.cmp(&a.amount)));
    if report.count > 0 {
        report.average = Money::new(report.total.inner() / Decimal::from(report.count));
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::records::{
        Applied, AppliedMove, Bucket, IncomeStream, ReconLine, Reconciliation,
    };
    use rust_decimal_macros::dec;

    fn m(value: Decimal) -> Money {
        Money::new(value)
    }

    fn line(label: &str, member: &str, amount: Decimal, spent_on: &str, bucket: &str) -> ReconLine {
        ReconLine {
            label: label.into(),
            member: member.into(),
            amount: m(amount),
            spent_on: spent_on.into(),
            bucket_id: bucket.into(),
            ..Default::default()
        }
    }

    /// `records` and `buckets` from `check-spending.mjs`.
    fn oracle() -> Ledger {
        let mut ledger = Ledger::default();
        ledger.buckets.push(Bucket {
            id: "b".into(),
            name: "Travel".into(),
            ..Default::default()
        });
        ledger.reconciliations = vec![
            Reconciliation {
                id: "one".into(),
                card: "Card".into(),
                statement_date: "2026-09-16".into(),
                balance: Money::from(100),
                status: "open".into(),
                lines: vec![
                    line("  Groceries  ", "Chris", dec!(20.1), "2026-08-31", "b"),
                    line("groceries", "chris", dec!(30.2), "2026-09-01", ""),
                    line("Fuel", "Sam", dec!(9.7), "", ""),
                ],
                ..Default::default()
            },
            Reconciliation {
                id: "two".into(),
                card: "Second".into(),
                statement_date: "2026-08-01".into(),
                balance: Money::from(50),
                status: "settled".into(),
                settled_at: "2026-09-10T12:00:00Z".into(),
                applied: Some(Applied {
                    buckets: vec![AppliedMove {
                        id: "b".into(),
                        amount: Money::from(50),
                        ..Default::default()
                    }],
                    ..Default::default()
                }),
                lines: vec![line("Hotel", "Sam", dec!(50), "", "b")],
                ..Default::default()
            },
            Reconciliation {
                id: "three".into(),
                card: "Other".into(),
                balance: Money::from(5),
                status: "open".into(),
                lines: vec![line("", "", dec!(5), "", "")],
                ..Default::default()
            },
        ];
        ledger
    }

    fn day(text: &str) -> Day {
        Day::parse(text).unwrap()
    }

    fn september() -> Window {
        range("custom", "2026-09-01", "2026-09-30", day("2026-09-16"))
    }

    fn all(ledger: &Ledger) -> Report {
        analyse(ledger, Window::ALL, Status::All, 0)
    }

    #[test]
    fn all_time_adds_up_the_way_the_plugin_does() {
        let r = all(&oracle());
        assert_eq!(r.total, Money::from(115));
        assert_eq!(r.count, 5);
        assert_eq!(r.open, Money::from(65));
        assert_eq!(r.settled, Money::from(50));
        assert_eq!(r.average, Money::from(23));
        assert_eq!(r.unitemized, Money::from(40));
        assert_eq!(r.unattributed, Money::from(5));
        assert_eq!(r.undated, 1);
        assert_eq!(r.from_buckets, m(dec!(70.1)));
        assert_eq!(r.everyday, m(dec!(44.9)));
        assert_eq!(r.withdrawn, Money::from(50));
        assert_eq!(r.inferred_dates, 2);
    }

    #[test]
    fn names_group_ignoring_case_and_extra_spaces() {
        let r = all(&oracle());
        assert_eq!(r.people[0].name, "Sam");
        assert_eq!(r.people[0].amount, m(dec!(59.7)));
        let chris = r.people.iter().find(|g| g.key == "chris").unwrap();
        assert_eq!(chris.amount, m(dec!(50.3)));
        // The first spelling seen is the one shown.
        assert_eq!(chris.name, "Chris");
        assert_eq!(r.items[0].count, 2);
        assert_eq!(r.items[0].amount, m(dec!(50.3)));
        assert_eq!(r.items[0].name, "Groceries");
        assert_eq!(r.months[0].name, "2026-08");
    }

    #[test]
    fn a_window_takes_charges_by_their_own_date_and_withdrawals_by_settlement() {
        let r = analyse(&oracle(), september(), Status::All, 0);
        assert_eq!(r.total, m(dec!(39.9)));
        assert_eq!(r.withdrawn, Money::from(50));
        assert_eq!(r.count, 2);
        assert_eq!(r.undated, 1);
        assert_eq!(
            analyse(&oracle(), september(), Status::Open, 0).withdrawn,
            Money::ZERO
        );
        assert_eq!(
            analyse(&oracle(), september(), Status::Settled, 0).total,
            Money::ZERO
        );
        assert_eq!(
            analyse(&oracle(), september(), Status::Settled, 0).withdrawn,
            Money::from(50)
        );
    }

    #[test]
    fn an_undone_settlement_withdrew_nothing() {
        let mut undone = oracle();
        undone.reconciliations[1].status = "open".into();
        undone.reconciliations[1].settled_at = String::new();
        undone.reconciliations[1].applied = None;
        let r = all(&undone);
        assert_eq!(r.withdrawn, Money::ZERO);
        assert_eq!(r.total, Money::from(115));
    }

    #[test]
    fn a_settlement_from_before_debits_were_recorded_is_counted_as_missing() {
        let mut legacy = oracle();
        legacy.reconciliations[1].applied = None;
        let r = all(&legacy);
        assert_eq!(r.missing_settlement_history, 1);
        assert_eq!(r.withdrawn, Money::ZERO);
    }

    #[test]
    fn an_undated_settlement_shows_only_in_all_time() {
        let mut undated = oracle();
        undated.reconciliations[1].settled_at = String::new();
        assert_eq!(all(&undated).withdrawn, Money::from(50));
        assert_eq!(all(&undated).undated_withdrawals, 1);
        assert_eq!(
            analyse(&undated, september(), Status::All, 0).withdrawn,
            Money::ZERO
        );
    }

    #[test]
    fn a_deleted_bucket_is_named_as_one() {
        let mut ledger = oracle();
        ledger.buckets.clear();
        assert_eq!(all(&ledger).bucket_spending[0].name, "Deleted bucket · b");
    }

    #[test]
    fn a_custom_range_must_be_real_dates_in_order() {
        let today = day("2026-09-16");
        assert!(!range("custom", "2026-09-30", "2026-09-01", today).valid);
        assert!(!range("custom", "2026-02-30", "2026-09-01", today).valid);
        let bad = Window {
            start: None,
            end: None,
            valid: false,
        };
        assert_eq!(analyse(&oracle(), bad, Status::All, 0).total, Money::ZERO);
    }

    #[test]
    fn a_period_ends_today_and_starts_that_many_months_back() {
        let today = day("2026-09-16");
        for (period, start) in [
            ("1m", "2026-08-16"),
            ("3m", "2026-06-16"),
            ("6m", "2026-03-16"),
            ("1y", "2025-09-16"),
        ] {
            let w = range(period, "", "", today);
            assert_eq!(w.start.map(Day::iso).as_deref(), Some(start), "{period}");
            assert_eq!(w.end, Some(today));
            assert!(w.valid);
        }
        let leap = range("1m", "", "", day("2024-03-31"));
        assert_eq!(leap.start.unwrap().iso(), "2024-02-29");
    }

    #[test]
    fn members_are_one_of_each_ignoring_case_in_name_order() {
        let mut ledger = oracle();
        ledger.income.push(IncomeStream {
            owner: "Chris".into(),
            ..Default::default()
        });
        let configured: Vec<String> = ["Sam", "chris", "Pat", ""].map(String::from).to_vec();
        assert_eq!(members(&ledger, &configured), ["chris", "Pat", "Sam"]);
    }

    #[test]
    fn nothing_to_report_reports_nothing() {
        assert_eq!(all(&Ledger::default()).count, 0);
    }

    #[test]
    fn a_settlement_late_in_the_evening_lands_on_that_evening() {
        // 01:00 UTC on the 1st is the evening of the 31st in New York, so a
        // window of August alone includes it there and not in London.
        let mut ledger = oracle();
        ledger.reconciliations[1].settled_at = "2026-09-01T01:00:00Z".into();
        let august = range("custom", "2026-08-01", "2026-08-31", day("2026-09-16"));
        assert_eq!(
            analyse(&ledger, august, Status::All, -240).withdrawn,
            Money::from(50)
        );
        assert_eq!(
            analyse(&ledger, august, Status::All, 0).withdrawn,
            Money::ZERO
        );
    }

    #[test]
    fn charges_list_newest_first_with_undated_last() {
        let r = all(&oracle());
        let dates: Vec<_> = r
            .transactions
            .iter()
            .map(|c| c.date.map(Day::iso).unwrap_or_default())
            .collect();
        assert_eq!(
            dates,
            ["2026-09-16", "2026-09-01", "2026-08-31", "2026-08-01", ""]
        );
    }
}
