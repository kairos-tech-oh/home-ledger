//! Importing a bank export onto an open statement: what the file says, set
//! against what the statement already has.

use crate::commands::{Answer, CommandError};
use crate::state::AppState;
use ledger_domain::Ledger;
use ledger_domain::records::Reconciliation;
use ledger_writer::bank_csv::{self, Mapping, Transaction};
use serde::Serialize;
use std::collections::HashMap;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Row {
    #[serde(flatten)]
    pub transaction: Transaction,
    /// Amount as the screen shows and sends it.
    pub value: String,
    /// Already on the statement: same day, amount and description.
    pub duplicate: bool,
    /// Dated after the statement date, so probably next cycle's.
    pub after_statement: bool,
    /// Worth adding unless the person says otherwise.
    pub suggested: bool,
    /// Dated before this statement's dates: within the card's last statement,
    /// so most likely paid there, or before a first statement's weeks. Only a bank fetch, which reaches back that far,
    /// sets it.
    pub earlier: bool,
    /// The bank connection's id for it, which the import keeps on the line.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bank_ref: String,
    /// The bank's own description, when the description shown is the
    /// cleaner merchant name.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bank_text: String,
    /// Where this merchant's charges went last time: a bucket's id, or empty
    /// for everyday spending. None when the merchant is new.
    pub suggested_bucket: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub headers: Vec<String>,
    pub mapping: Mapping,
    pub had_headers: bool,
    pub notes: Vec<String>,
    pub rows: Vec<Row>,
    pub charges: usize,
    pub duplicates: usize,
    pub set_aside: usize,
}

type Key = (String, String, String);

fn key(date: &str, amount: &str, label: &str) -> Key {
    (
        date.to_string(),
        amount.to_string(),
        label
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .to_lowercase(),
    )
}

pub fn preview(
    text: &str,
    mapping: Option<Mapping>,
    record: &Reconciliation,
) -> Result<ImportPreview, String> {
    let read = bank_csv::read(text, mapping)?;

    // Counted, not just seen: two identical coffees on one day are two charges,
    // and a file holding three of them against a statement holding two adds one.
    let mut held: HashMap<Key, usize> = HashMap::new();
    for line in &record.lines {
        *held
            .entry(key(&line.spent_on, &line.amount.to_string(), &line.label))
            .or_default() += 1;
    }

    let statement_date = record.statement_date.as_str();
    let rows: Vec<Row> = read
        .rows
        .into_iter()
        .map(|t| {
            let value = t.amount.to_string();
            let duplicate = t.problem.is_empty()
                && match held.get_mut(&key(&t.date, &value, &t.description)) {
                    Some(n) if *n > 0 => {
                        *n -= 1;
                        true
                    }
                    _ => false,
                };
            let after_statement = !statement_date.is_empty()
                && !t.date.is_empty()
                && t.date.as_str() > statement_date;
            Row {
                suggested: t.problem.is_empty() && !duplicate,
                value,
                duplicate,
                after_statement,
                earlier: false,
                bank_ref: String::new(),
                bank_text: String::new(),
                suggested_bucket: None,
                transaction: t,
            }
        })
        .collect();

    Ok(summarise(
        rows,
        read.headers,
        read.mapping,
        read.had_headers,
        read.notes,
    ))
}

/// Counts what a preview found.
pub fn summarise(
    rows: Vec<Row>,
    headers: Vec<String>,
    mapping: Mapping,
    had_headers: bool,
    notes: Vec<String>,
) -> ImportPreview {
    ImportPreview {
        charges: rows.iter().filter(|r| r.suggested).count(),
        duplicates: rows.iter().filter(|r| r.duplicate).count(),
        set_aside: rows
            .iter()
            .filter(|r| !r.transaction.problem.is_empty())
            .count(),
        headers,
        mapping,
        had_headers,
        notes,
        rows,
    }
}

/// A merchant as charges from it are told apart: "KROGER #920 COLUMBUS OH",
/// "KROGER 5005" and Plaid's "Kroger" are all "kroger". The words before the
/// first store number, lower case, at most three of them.
pub fn merchant_key(label: &str) -> String {
    label
        .split(|c: char| c.is_whitespace() || c == '*')
        .filter(|w| !w.is_empty())
        .take_while(|w| !w.starts_with('#') && !w.chars().any(|c| c.is_ascii_digit()))
        .take(3)
        .map(|w| w.to_lowercase())
        .collect::<Vec<_>>()
        .join(" ")
}

/// Gives each row the bucket its merchant's charges went to most recently,
/// on any statement. A suggestion only: the person sees it and can change it.
pub fn suggest_buckets(rows: &mut [Row], ledger: &Ledger) {
    let mut last: HashMap<String, (&str, &str)> = HashMap::new();
    for line in ledger.reconciliations.iter().flat_map(|r| r.lines.iter()) {
        let bucket = if ledger.bucket(&line.bucket_id).is_some() {
            line.bucket_id.as_str()
        } else {
            ""
        };
        for name in [&line.label, &line.bank_text] {
            let key = merchant_key(name);
            if key.is_empty() {
                continue;
            }
            let newer = last
                .get(&key)
                .is_none_or(|(on, _)| line.spent_on.as_str() >= *on);
            if newer {
                last.insert(key, (line.spent_on.as_str(), bucket));
            }
        }
    }
    for row in rows {
        row.suggested_bucket = [&row.transaction.description, &row.bank_text]
            .into_iter()
            .map(|n| merchant_key(n))
            .filter(|k| !k.is_empty())
            .find_map(|k| last.get(&k).map(|(_, b)| b.to_string()));
    }
}

/// Reads an export against one statement. `mapping` is the person's choice of
/// columns and sign, when the guess was wrong.
pub async fn transactions_preview(
    state: &AppState,
    id: String,
    text: String,
    mapping: Option<Mapping>,
) -> Answer<ImportPreview> {
    let loaded = state.live().await.engine.load().await?;
    let doc = match &loaded.snapshot {
        Some(s) => ledger_writer::read(&s.body)?,
        None => Ledger::default(),
    };
    let Some(record) = doc.reconciliations.iter().find(|r| r.id == id) else {
        return Err(CommandError::Message("no such reconciliation".into()));
    };
    let mut found = preview(&text, mapping, record).map_err(CommandError::Message)?;
    suggest_buckets(&mut found.rows, &doc);
    Ok(found)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::Money;
    use ledger_domain::records::ReconLine;
    use rust_decimal::Decimal;

    const CHASE: &str = "Transaction Date,Post Date,Description,Category,Type,Amount,Memo\n\
09/28/2026,09/28/2026,GOOGLE *YouTube TV,Shopping,Sale,-5.39,\n\
09/25/2026,09/27/2026,AMK JPMC EASTON CAFE,Food & Drink,Sale,-9.67,\n\
09/25/2026,09/27/2026,AMK JPMC EASTON CAFE,Food & Drink,Sale,-9.67,\n\
09/20/2026,09/20/2026,Payment Thank You-Mobile,,Payment,1500.00,\n";

    fn statement(date: &str) -> Reconciliation {
        Reconciliation {
            card: "Sapphire".into(),
            statement_date: date.into(),
            lines: vec![ReconLine {
                label: "amk  jpmc easton cafe".into(),
                amount: Money::new(Decimal::new(967, 2)),
                spent_on: "2026-09-25".into(),
                ..Default::default()
            }],
            ..Default::default()
        }
    }

    #[test]
    fn a_charge_already_on_the_statement_is_not_suggested_again_but_its_twin_is() {
        let p = preview(CHASE, None, &statement("")).unwrap();
        let flags: Vec<_> = p.rows.iter().map(|r| (r.duplicate, r.suggested)).collect();
        assert_eq!(
            flags,
            [(false, true), (true, false), (false, true), (false, false)]
        );
        assert_eq!((p.charges, p.duplicates, p.set_aside), (2, 1, 1));
        assert_eq!(p.rows[0].value, "5.39");
    }

    #[test]
    fn a_charge_after_the_statement_date_is_marked() {
        let p = preview(CHASE, None, &statement("2026-09-26")).unwrap();
        assert!(p.rows[0].after_statement);
        assert!(!p.rows[1].after_statement);
    }

    #[test]
    fn one_store_under_many_numbers_is_one_merchant() {
        assert_eq!(merchant_key("KROGER #920 COLUMBUS OH"), "kroger");
        assert_eq!(merchant_key("KROGER 5005"), "kroger");
        assert_eq!(merchant_key("Kroger"), "kroger");
        assert_eq!(merchant_key("GOOGLE *YouTube TV"), "google youtube tv");
        assert_eq!(merchant_key("AMK JPMC EASTON CAFE"), "amk jpmc easton");
        assert_eq!(merchant_key("#1234"), "");
    }

    #[test]
    fn a_merchant_is_suggested_the_bucket_it_went_to_last() {
        use ledger_domain::records::Bucket;
        let groceries = "a".repeat(32);
        let line = |label: &str, on: &str, bucket: &str| ReconLine {
            label: label.into(),
            spent_on: on.into(),
            bucket_id: bucket.into(),
            amount: Money::from(1),
            ..Default::default()
        };
        let ledger = Ledger {
            buckets: vec![Bucket {
                id: groceries.clone(),
                name: "Groceries".into(),
                ..Default::default()
            }],
            reconciliations: vec![Reconciliation {
                lines: vec![
                    line("KROGER #920", "2026-08-01", ""),
                    line("KROGER 5005", "2026-09-01", &groceries),
                    line("Shell", "2026-09-02", &"b".repeat(32)),
                ],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut p = preview(CHASE, None, &statement("")).unwrap();
        p.rows[0].transaction.description = "Kroger".into();
        p.rows[1].transaction.description = "Shell Oil 1234".into();
        suggest_buckets(&mut p.rows, &ledger);
        assert_eq!(
            p.rows[0].suggested_bucket.as_deref(),
            Some(groceries.as_str())
        );
        assert_eq!(
            p.rows[1].suggested_bucket.as_deref(),
            None,
            "\"shell oil\" is not \"shell\""
        );
        assert_eq!(p.rows[2].suggested_bucket, None);
    }

    #[test]
    fn a_bucket_since_deleted_suggests_everyday() {
        let ledger = Ledger {
            reconciliations: vec![Reconciliation {
                lines: vec![ReconLine {
                    label: "Kroger".into(),
                    bucket_id: "c".repeat(32),
                    amount: Money::from(1),
                    ..Default::default()
                }],
                ..Default::default()
            }],
            ..Default::default()
        };
        let mut p = preview(CHASE, None, &statement("")).unwrap();
        p.rows[0].transaction.description = "KROGER #1".into();
        suggest_buckets(&mut p.rows, &ledger);
        assert_eq!(p.rows[0].suggested_bucket.as_deref(), Some(""));
    }
}
