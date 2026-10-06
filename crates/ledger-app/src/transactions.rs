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
                transaction: t,
            }
        })
        .collect();

    Ok(ImportPreview {
        charges: rows.iter().filter(|r| r.suggested).count(),
        duplicates: rows.iter().filter(|r| r.duplicate).count(),
        set_aside: rows
            .iter()
            .filter(|r| !r.transaction.problem.is_empty())
            .count(),
        headers: read.headers,
        mapping: read.mapping,
        had_headers: read.had_headers,
        notes: read.notes,
        rows,
    })
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
    preview(&text, mapping, record).map_err(CommandError::Message)
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
}
