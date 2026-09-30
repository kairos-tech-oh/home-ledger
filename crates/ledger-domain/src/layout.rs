//! How the screens are arranged, where that is worth sharing between machines.
//! Kept in the document's `unknown` map, like the dashboard, so a document
//! that has never been arranged reads and writes back unchanged.

use crate::document::Ledger;
use crate::records::ACCOUNT_TYPES;
use serde_json::Value;

/// The key the account section order is stored under.
pub const ACCOUNT_ORDER_KEY: &str = "accountSectionOrder";

/// Known account kinds, once each, in the order given. Anything else is
/// dropped rather than refused: a layout must never make a ledger unreadable.
pub fn clean_account_order<'a>(kinds: impl IntoIterator<Item = &'a str>) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for kind in kinds {
        let kind = kind.trim();
        if ACCOUNT_TYPES.contains(&kind) && !out.iter().any(|k| k == kind) {
            out.push(kind.to_string());
        }
    }
    out
}

impl Ledger {
    /// The order the Accounts page puts its sections in, as the person
    /// arranged it. Empty means the default: most accounts first.
    pub fn account_order(&self) -> Vec<String> {
        match self
            .unknown
            .get(ACCOUNT_ORDER_KEY)
            .and_then(Value::as_array)
        {
            Some(list) => clean_account_order(list.iter().filter_map(Value::as_str)),
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn only_known_kinds_once_each_in_the_order_given() {
        assert_eq!(
            clean_account_order(["credit", "nonsense", "checking", "credit", " loan "]),
            ["credit", "checking", "loan"]
        );
    }

    #[test]
    fn a_ledger_never_arranged_has_no_order_and_writes_none() {
        let ledger = Ledger::default();
        assert!(ledger.account_order().is_empty());
        let out = String::from_utf8(ledger.to_bytes().unwrap()).unwrap();
        assert!(!out.contains(ACCOUNT_ORDER_KEY));
    }

    #[test]
    fn a_damaged_order_reads_as_what_can_be_kept() {
        let mut ledger = Ledger::default();
        ledger.unknown.insert(
            ACCOUNT_ORDER_KEY.into(),
            json!(["loan", 7, null, "checking"]),
        );
        assert_eq!(ledger.account_order(), ["loan", "checking"]);
        ledger
            .unknown
            .insert(ACCOUNT_ORDER_KEY.into(), json!("not a list"));
        assert!(ledger.account_order().is_empty());
    }
}
