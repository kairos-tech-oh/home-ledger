//! Arranging the screens, where the arrangement is kept in the ledger.

use crate::{WriteError, Writer};
use ledger_domain::Ledger;
use ledger_domain::layout::{ACCOUNT_ORDER_KEY, clean_account_order};
use ledger_domain::records::{AuditChange, AuditEntry};
use serde_json::Value;

impl Writer {
    /// Sets the order of the Accounts page's sections. An empty list goes back
    /// to the default, most accounts first, and leaves no key behind.
    pub(crate) fn account_order_set(
        &self,
        ledger: &mut Ledger,
        order: &[String],
    ) -> Result<AuditEntry, WriteError> {
        let before = ledger.account_order();
        let after = clean_account_order(order.iter().map(String::as_str));
        if after == before {
            return Err(WriteError::Unchanged("that is already the order".into()));
        }
        if after.is_empty() {
            ledger.unknown.remove(ACCOUNT_ORDER_KEY);
        } else {
            ledger.unknown.insert(
                ACCOUNT_ORDER_KEY.into(),
                Value::Array(after.iter().cloned().map(Value::String).collect()),
            );
        }
        let describe = |kinds: &[String]| {
            if kinds.is_empty() {
                "most accounts first".to_string()
            } else {
                kinds.join(", ")
            }
        };
        Ok(self.finish(
            AuditEntry {
                action: "Update".into(),
                subject: "accounts".into(),
                name: "section order".into(),
                changes: vec![AuditChange {
                    field: "order".into(),
                    from: describe(&before),
                    to: describe(&after),
                }],
                ..Default::default()
            },
            "account-order-set",
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Op, WriteError, Writer};
    use ledger_domain::Ledger;
    use serde_json::json;

    fn set(ledger: &mut Ledger, order: serde_json::Value) -> Result<(), WriteError> {
        let op: Op =
            serde_json::from_value(json!({ "op": "account-order-set", "order": order })).unwrap();
        Writer::new("test").apply(ledger, &op).map(|_| ())
    }

    #[test]
    fn an_order_is_kept_and_a_reset_leaves_nothing_behind() {
        let mut ledger = Ledger::default();
        set(&mut ledger, json!(["credit", "bogus", "checking"])).unwrap();
        assert_eq!(ledger.account_order(), ["credit", "checking"]);
        // Kept through a write and a read, which is what survives a restart.
        let again = Ledger::from_bytes(&ledger.to_bytes().unwrap()).unwrap();
        assert_eq!(again.account_order(), ["credit", "checking"]);

        set(&mut ledger, json!([])).unwrap();
        assert!(!ledger.unknown.contains_key("accountSectionOrder"));
    }

    #[test]
    fn the_same_order_again_changes_nothing() {
        let mut ledger = Ledger::default();
        set(&mut ledger, json!(["loan"])).unwrap();
        assert!(matches!(
            set(&mut ledger, json!(["loan", "loan"])),
            Err(WriteError::Unchanged(_))
        ));
    }
}
