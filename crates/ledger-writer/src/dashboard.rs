//! Saving the dashboard layout. Port of the helper's `dashboard-set`.

use crate::{WriteError, Writer};
use ledger_domain::Ledger;
use ledger_domain::dashboard::clean_dashboard;
use ledger_domain::records::AuditEntry;
use ledger_domain::text::new_id;
use serde_json::Value;

impl Writer {
    /// Replaces the whole layout. Every move, resize and removal on the
    /// dashboard sends the layout whole, as the plugin does.
    pub(crate) fn dashboard_set(
        &self,
        ledger: &mut Ledger,
        raw: &Value,
    ) -> Result<AuditEntry, WriteError> {
        let Some(dashboard) = clean_dashboard(raw, |_| new_id()) else {
            return Err(WriteError::Refused("dashboard is not an object".into()));
        };
        let count = dashboard.widgets.len();
        let value =
            serde_json::to_value(&dashboard).map_err(|e| WriteError::Refused(e.to_string()))?;
        let before = ledger.unknown.insert("dashboard".into(), value);
        Ok(self.finish(
            AuditEntry {
                action: if before.is_some() { "Update" } else { "Create" }.into(),
                subject: "dashboard".into(),
                name: format!("{count} widget{}", if count == 1 { "" } else { "s" }),
                ..Default::default()
            },
            "dashboard-set",
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Kind, Op, Writer};
    use ledger_domain::Ledger;
    use serde_json::json;

    fn apply(ledger: &mut Ledger, op: Op) -> ledger_domain::records::AuditEntry {
        Writer::new("test").apply(ledger, &op).expect("applies")
    }

    #[test]
    fn a_layout_is_stored_cleaned_and_named_by_its_size() {
        let mut ledger = Ledger::default();
        let entry = apply(
            &mut ledger,
            serde_json::from_value(json!({ "op": "dashboard-set", "dashboard": { "widgets": [
                { "kind": "networth", "span": 2 },
                { "kind": "BAD" },
            ]}}))
            .unwrap(),
        );
        assert_eq!(
            (entry.action.as_str(), entry.name.as_str()),
            ("Create", "1 widget")
        );
        let stored = ledger.dashboard().unwrap();
        assert_eq!(stored.widgets.len(), 1);
        // A widget sent without an id is given a real one, not a positional one.
        assert_eq!(stored.widgets[0].id.len(), 32);
        assert_ne!(
            stored.widgets[0].id,
            ledger_domain::dashboard::positional_id(0)
        );

        let entry = apply(
            &mut ledger,
            serde_json::from_value(
                json!({ "op": "dashboard-set", "dashboard": { "widgets": [] } }),
            )
            .unwrap(),
        );
        assert_eq!(
            (entry.action.as_str(), entry.name.as_str()),
            ("Update", "0 widgets")
        );
    }

    #[test]
    fn something_that_is_not_a_layout_is_refused_and_changes_nothing() {
        let mut ledger = Ledger::default();
        let op: Op =
            serde_json::from_value(json!({ "op": "dashboard-set", "dashboard": [1] })).unwrap();
        assert!(Writer::new("test").apply(&mut ledger, &op).is_err());
        assert!(!ledger.unknown.contains_key("dashboard"));
    }

    #[test]
    fn deleting_a_bucket_takes_it_off_a_saved_layout() {
        let mut ledger = Ledger::default();
        apply(
            &mut ledger,
            Op::Set {
                kind: Kind::Bucket,
                id: String::new(),
                record: json!({ "name": "Travel" }),
            },
        );
        let bucket = ledger.buckets[0].id.clone();
        apply(
            &mut ledger,
            serde_json::from_value(json!({ "op": "dashboard-set", "dashboard": { "widgets": [
                { "kind": "buckets", "refs": [bucket] } ]}}))
            .unwrap(),
        );
        apply(
            &mut ledger,
            Op::Delete {
                kind: Kind::Bucket,
                id: bucket,
            },
        );
        assert!(ledger.dashboard().unwrap().widgets[0].refs.is_empty());
    }
}
