//! The bounded diff written alongside every change.
//!
//! Changed fields only, lists reported by length, and a reference rendered as
//! the thing it points at rather than as 32 hex characters nobody can place.

use ledger_domain::Ledger;
use ledger_domain::records::{AuditChange, caps};
use ledger_domain::text::plain;
use serde_json::Value;
use std::collections::BTreeMap;

const FIELD_MAX: usize = 160;

/// One short, printable rendering of a field, whatever its type.
pub fn brief(value: &Value) -> String {
    match value {
        Value::Null => String::new(),
        Value::Bool(true) => "yes".into(),
        Value::Bool(false) => "no".into(),
        Value::Number(n) => {
            let text = format!("{:.2}", n.as_f64().unwrap_or(0.0));
            text.trim_end_matches('0').trim_end_matches('.').to_string()
        }
        Value::Array(items) => {
            format!(
                "{} item{}",
                items.len(),
                if items.len() == 1 { "" } else { "s" }
            )
        }
        Value::Object(_) => "…".into(),
        Value::String(s) => plain(s, FIELD_MAX),
    }
}

/// id -> name, so a change to a reference field reads as "Emergency fund".
pub fn name_index(ledger: &Ledger) -> BTreeMap<String, String> {
    let mut names = BTreeMap::new();
    for a in &ledger.accounts {
        names.insert(a.id.clone(), a.name.clone());
    }
    for s in &ledger.income {
        names.insert(s.id.clone(), s.name.clone());
    }
    for b in &ledger.budget {
        names.insert(b.id.clone(), b.name.clone());
    }
    for b in &ledger.buckets {
        names.insert(b.id.clone(), b.name.clone());
    }
    names
}

/// A reference field renders as the thing it points at; everything else
/// renders as itself.
fn render(value: &Value, key: &str, names: &BTreeMap<String, String>) -> String {
    if key.ends_with("Id")
        && let Value::String(s) = value
    {
        if let Some(name) = names.get(s) {
            return plain(name, FIELD_MAX);
        }
        if s.is_empty() {
            return "none".into();
        }
    }
    brief(value)
}

/// Whether a value is the sort of empty that is not worth naming on a create.
fn is_nothing(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Bool(b) => !b,
        Value::String(s) => s.is_empty(),
        Value::Array(a) => a.is_empty(),
        Value::Number(n) => n.as_f64().is_some_and(|f| f == 0.0),
        Value::Object(_) => false,
    }
}

/// Only what actually changed, and only the fields worth naming. A create
/// skips the fields left empty rather than listing every one.
pub fn diff_fields(
    before: Option<&Value>,
    after: &Value,
    names: &BTreeMap<String, String>,
) -> Vec<AuditChange> {
    let creating = before.is_none();
    let empty = serde_json::Map::new();
    let old = before.and_then(Value::as_object).unwrap_or(&empty);
    let Some(new) = after.as_object() else {
        return Vec::new();
    };

    let mut out = Vec::new();
    for (key, value) in new {
        if key == "id" {
            continue;
        }
        let previous = old.get(key).unwrap_or(&Value::Null);
        if previous == value {
            continue;
        }
        if creating && is_nothing(value) {
            continue;
        }
        out.push(AuditChange {
            field: key.clone(),
            from: render(previous, key, names),
            to: render(value, key, names),
        });
        if out.len() >= caps::CHANGES {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn names() -> BTreeMap<String, String> {
        BTreeMap::from([("abc".to_string(), "Emergency fund".to_string())])
    }

    #[test]
    fn a_reference_renders_as_what_it_points_at() {
        let before = json!({ "bucketId": "" });
        let after = json!({ "bucketId": "abc" });
        let changes = diff_fields(Some(&before), &after, &names());
        assert_eq!(changes[0].from, "none");
        assert_eq!(changes[0].to, "Emergency fund");
    }

    #[test]
    fn a_list_is_reported_by_length_not_contents() {
        let before = json!({ "debts": [] });
        let after = json!({ "debts": [ {"name": "a"}, {"name": "b"} ] });
        let changes = diff_fields(Some(&before), &after, &names());
        assert_eq!(changes[0].to, "2 items");
    }

    #[test]
    fn a_create_skips_the_fields_left_empty() {
        let after = json!({ "name": "Rent", "notes": "", "amount": 0, "bucketId": "" });
        let changes = diff_fields(None, &after, &names());
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "name");
    }

    #[test]
    fn an_unchanged_field_is_not_named() {
        let before = json!({ "name": "Rent", "amount": 100 });
        let after = json!({ "name": "Rent", "amount": 120 });
        let changes = diff_fields(Some(&before), &after, &names());
        assert_eq!(changes.len(), 1);
        assert_eq!(changes[0].field, "amount");
    }

    #[test]
    fn the_diff_is_bounded() {
        let mut before = serde_json::Map::new();
        let mut after = serde_json::Map::new();
        for i in 0..40 {
            before.insert(format!("f{i}"), json!(i));
            after.insert(format!("f{i}"), json!(i + 1));
        }
        let changes = diff_fields(
            Some(&Value::Object(before)),
            &Value::Object(after),
            &names(),
        );
        assert_eq!(changes.len(), caps::CHANGES);
    }

    #[test]
    fn amounts_render_without_trailing_zeros() {
        assert_eq!(brief(&json!(1200.0)), "1200");
        assert_eq!(brief(&json!(1200.50)), "1200.5");
        assert_eq!(brief(&json!(1200.55)), "1200.55");
    }
}
