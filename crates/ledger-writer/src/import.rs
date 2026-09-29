//! Reading a ledger written by something else.
//!
//! The document this app stores is the same shape the prototype wrote, so an
//! import is not a translation — it is a validation pass. Every record goes
//! through the same cleaning a hand edit gets, so anything malformed is caught
//! here rather than on the first screen that tries to add it up.

use crate::{WriteError, clean};
use ledger_domain::Ledger;

/// What an import would do, shown before it is allowed to do it.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportReport {
    pub accounts: usize,
    pub income: usize,
    pub budget: usize,
    pub buckets: usize,
    pub holdings: usize,
    pub goals: usize,
    pub templates: usize,
    pub reconciliations: usize,
    /// Rows that could not be cleaned, named so they can be chased in the
    /// source rather than silently vanishing.
    pub skipped: Vec<String>,
    /// Top-level keys this build does not model. Carried through untouched.
    pub carried: Vec<String>,
}

impl ImportReport {
    pub fn records(&self) -> usize {
        self.accounts
            + self.income
            + self.budget
            + self.buckets
            + self.holdings
            + self.goals
            + self.templates
            + self.reconciliations
    }
}

pub struct Imported {
    pub ledger: Ledger,
    pub report: ImportReport,
}

/// How many bad rows are named before the list is cut.
const MAX_SKIPPED: usize = 20;

/// Parse and validate a document. One unusable row is skipped and reported;
/// only a document that cannot be read at all is an error, because failing a
/// 200-record import over a single bad row helps nobody.
pub fn read(raw: &[u8]) -> Result<Imported, WriteError> {
    if raw.is_empty() {
        return Err(WriteError::Refused("that file is empty".into()));
    }
    let mut ledger = Ledger::from_bytes(raw).map_err(|e| {
        WriteError::Refused(format!("that file is not a ledger this app can read: {e}"))
    })?;

    let mut report = ImportReport {
        holdings: ledger.investments.len(),
        goals: ledger.goals.len(),
        templates: ledger.templates.len(),
        reconciliations: ledger.reconciliations.len(),
        carried: ledger.unknown.keys().cloned().collect(),
        ..Default::default()
    };

    let mut skipped: Vec<String> = Vec::new();

    let accounts = std::mem::take(&mut ledger.accounts);
    for record in accounts {
        let id = record.id.clone();
        let label = describe("account", &record.name, &id);
        match clean::account(record, &id) {
            Ok(clean) => ledger.accounts.push(clean),
            Err(e) => skipped.push(format!("{label}: {e}")),
        }
    }

    let income = std::mem::take(&mut ledger.income);
    for record in income {
        let id = record.id.clone();
        let label = describe("income stream", &record.name, &id);
        match clean::income(record, &id) {
            Ok(clean) => ledger.income.push(clean),
            Err(e) => skipped.push(format!("{label}: {e}")),
        }
    }

    let budget = std::mem::take(&mut ledger.budget);
    for record in budget {
        let id = record.id.clone();
        let label = describe("budget line", &record.name, &id);
        match clean::budget(record, &id) {
            Ok(clean) => ledger.budget.push(clean),
            Err(e) => skipped.push(format!("{label}: {e}")),
        }
    }

    let buckets = std::mem::take(&mut ledger.buckets);
    for record in buckets {
        let id = record.id.clone();
        let label = describe("savings bucket", &record.name, &id);
        match clean::bucket(record, &id) {
            Ok(clean) => ledger.buckets.push(clean),
            Err(e) => skipped.push(format!("{label}: {e}")),
        }
    }

    report.accounts = ledger.accounts.len();
    report.income = ledger.income.len();
    report.budget = ledger.budget.len();
    report.buckets = ledger.buckets.len();
    report.skipped = skipped.into_iter().take(MAX_SKIPPED).collect();

    if report.records() == 0 {
        return Err(WriteError::Refused(
            "that file holds no records this app understands".into(),
        ));
    }

    Ok(Imported { ledger, report })
}

fn describe(kind: &str, name: &str, id: &str) -> String {
    if name.trim().is_empty() {
        format!("an unnamed {kind} ({})", &id[..id.len().min(8)])
    } else {
        format!("{kind} \"{name}\"")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::Money;

    fn document(body: &str) -> Vec<u8> {
        body.as_bytes().to_vec()
    }

    #[test]
    fn an_empty_file_is_refused_by_name() {
        let refused = read(b"");
        assert!(matches!(refused, Err(WriteError::Refused(_))));
    }

    #[test]
    fn something_that_is_not_a_ledger_is_refused() {
        assert!(read(b"not json at all").is_err());
        assert!(read(document("{\"accounts\":[]}").as_slice()).is_err());
    }

    #[test]
    fn a_real_looking_document_imports_with_counts() {
        let raw = document(
            r#"{
              "v": 2,
              "accounts": [{ "id": "a", "name": "Checking", "type": "checking", "total": 100 }],
              "buckets": [{ "id": "b", "name": "Emergency", "currentTotal": 50 }],
              "budget": [{ "id": "c", "name": "Rent", "monthlyAmount": 1200 }]
            }"#,
        );
        let out = read(&raw).expect("imported");
        assert_eq!(out.report.accounts, 1);
        assert_eq!(out.report.buckets, 1);
        assert_eq!(out.report.budget, 1);
        assert_eq!(out.report.records(), 3);
        assert!(out.report.skipped.is_empty());
    }

    #[test]
    fn one_bad_row_is_skipped_and_named_rather_than_failing_the_import() {
        // Failing 200 records over a single unnamed row helps nobody, but
        // dropping it silently is worse.
        let raw = document(
            r#"{
              "accounts": [
                { "id": "a", "name": "Checking", "type": "checking" },
                { "id": "b", "name": "", "type": "checking" }
              ]
            }"#,
        );
        let out = read(&raw).expect("imported");
        assert_eq!(out.report.accounts, 1);
        assert_eq!(out.report.skipped.len(), 1);
        assert!(out.report.skipped[0].contains("unnamed account"));
    }

    #[test]
    fn imported_amounts_are_normalised_to_cents() {
        let raw = document(
            r#"{ "buckets": [{ "id": "b", "name": "Emergency", "currentTotal": 10.005 }] }"#,
        );
        let out = read(&raw).expect("imported");
        assert_eq!(
            out.ledger.buckets[0].current_total,
            Money::from(10) + Money::new("0.01".parse().unwrap())
        );
    }

    #[test]
    fn a_negative_bucket_balance_is_floored_on_the_way_in() {
        let raw =
            document(r#"{ "buckets": [{ "id": "b", "name": "Emergency", "currentTotal": -40 }] }"#);
        let out = read(&raw).expect("imported");
        assert_eq!(out.ledger.buckets[0].current_total, Money::ZERO);
    }

    #[test]
    fn a_collection_this_build_does_not_model_is_carried_and_reported() {
        let raw = document(
            r#"{
              "buckets": [{ "id": "b", "name": "Emergency" }],
              "dashboard": { "widgets": [1, 2] }
            }"#,
        );
        let out = read(&raw).expect("imported");
        assert!(out.report.carried.contains(&"dashboard".to_string()));
        // And it survives being written back out.
        let round = String::from_utf8(out.ledger.to_bytes().unwrap()).unwrap();
        assert!(round.contains("dashboard"));
    }

    #[test]
    fn ids_are_kept_so_references_between_records_still_resolve() {
        let raw = document(
            r#"{
              "accounts": [{ "id": "00000000000000000000000000000001", "name": "Checking" }],
              "income": [{ "id": "00000000000000000000000000000002", "name": "Salary",
                           "accountId": "00000000000000000000000000000001" }]
            }"#,
        );
        let out = read(&raw).expect("imported");
        assert_eq!(out.ledger.income[0].account_id, out.ledger.accounts[0].id);
    }
}
