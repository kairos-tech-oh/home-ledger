//! The ledger document: one JSON object, the unit both the store and the
//! writer deal in.

use crate::lineage::Lineage;
use crate::records::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const SCHEMA_VERSION: u32 = 3;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Ledger {
    #[serde(default = "default_version")]
    pub v: u32,
    // Written when this app saves, but not invented on a document that has
    // none: reading and writing back must not add keys.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub updated_at: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub updated_by: String,

    /// Which copy of the document this is. Absent on one written by something
    /// that does not stamp it, which is why it is optional rather than
    /// defaulted — inventing a lineage would claim a history that is not there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub lineage: Option<Lineage>,

    #[serde(default)]
    pub accounts: Vec<Account>,
    #[serde(default)]
    pub income: Vec<IncomeStream>,
    #[serde(default)]
    pub budget: Vec<BudgetItem>,
    #[serde(default)]
    pub buckets: Vec<Bucket>,
    #[serde(default)]
    pub investments: Vec<Holding>,
    #[serde(default)]
    pub goals: Vec<Goal>,
    #[serde(default)]
    pub templates: Vec<Template>,
    #[serde(default)]
    pub reconciliations: Vec<Reconciliation>,

    #[serde(default = "default_budget_types")]
    pub budget_types: Vec<String>,
    #[serde(default = "default_investment_types")]
    pub investment_types: Vec<String>,

    /// Collections this build does not understand, preserved verbatim.
    #[serde(default, skip_serializing_if = "Passthrough::is_empty")]
    pub passthrough: Passthrough,

    /// Any top-level key a newer build added. Without this, an older client
    /// writing the document would silently delete a collection a newer one
    /// introduced — which has actually happened to this data before.
    #[serde(flatten)]
    pub unknown: BTreeMap<String, Value>,
}

fn default_version() -> u32 {
    SCHEMA_VERSION
}

fn default_budget_types() -> Vec<String> {
    DEFAULT_BUDGET_TYPES.iter().map(|s| s.to_string()).collect()
}

fn default_investment_types() -> Vec<String> {
    DEFAULT_INVESTMENT_TYPES
        .iter()
        .map(|s| s.to_string())
        .collect()
}

impl Default for Ledger {
    fn default() -> Self {
        Self {
            v: SCHEMA_VERSION,
            updated_at: String::new(),
            updated_by: String::new(),
            lineage: None,
            accounts: Vec::new(),
            income: Vec::new(),
            budget: Vec::new(),
            buckets: Vec::new(),
            investments: Vec::new(),
            goals: Vec::new(),
            templates: Vec::new(),
            reconciliations: Vec::new(),
            budget_types: default_budget_types(),
            investment_types: default_investment_types(),
            passthrough: Passthrough::new(),
            unknown: BTreeMap::new(),
        }
    }
}

impl Ledger {
    pub fn account(&self, id: &str) -> Option<&Account> {
        self.accounts.iter().find(|a| a.id == id)
    }
    pub fn bucket(&self, id: &str) -> Option<&Bucket> {
        self.buckets.iter().find(|b| b.id == id)
    }
    pub fn budget_item(&self, id: &str) -> Option<&BudgetItem> {
        self.budget.iter().find(|b| b.id == id)
    }
    pub fn holding(&self, id: &str) -> Option<&Holding> {
        self.investments.iter().find(|h| h.id == id)
    }

    /// Serialise the way the document is stored: compact, stable key order.
    pub fn to_bytes(&self) -> Result<Vec<u8>, serde_json::Error> {
        serde_json::to_vec(self)
    }

    pub fn from_bytes(bytes: &[u8]) -> Result<Self, serde_json::Error> {
        serde_json::from_slice(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_ledger_ships_the_default_types() {
        let ledger = Ledger::default();
        assert_eq!(ledger.budget_types.len(), 6);
        assert!(ledger.budget_types.contains(&"Living".to_string()));
        assert_eq!(ledger.investment_types.len(), 7);
    }

    #[test]
    fn a_collection_this_build_does_not_know_survives_a_round_trip() {
        // The regression this guards: an older helper once returned only the
        // collections it knew, deleting a newer one and pushing that loss on.
        let raw = r#"{
            "v": 3,
            "accounts": [],
            "somethingNewer": [{"id": "1", "kept": true}]
        }"#;
        let ledger = Ledger::from_bytes(raw.as_bytes()).expect("parses");
        let out = String::from_utf8(ledger.to_bytes().expect("serialises")).unwrap();
        assert!(
            out.contains("somethingNewer"),
            "unknown key was dropped: {out}"
        );
        assert!(out.contains("\"kept\":true"));
    }

    #[test]
    fn passthrough_collections_survive_too() {
        let raw = r#"{"v":3,"passthrough":{"recipes":[{"name":"soup"}]}}"#;
        let ledger = Ledger::from_bytes(raw.as_bytes()).expect("parses");
        assert!(ledger.passthrough.contains_key("recipes"));
        let out = String::from_utf8(ledger.to_bytes().unwrap()).unwrap();
        assert!(out.contains("soup"));
    }

    #[test]
    fn records_are_found_by_id() {
        let mut ledger = Ledger::default();
        ledger.accounts.push(Account {
            id: "a".repeat(32),
            name: "Checking".into(),
            kind: "checking".into(),
            ..Default::default()
        });
        assert!(ledger.account(&"a".repeat(32)).is_some());
        assert!(ledger.account("missing").is_none());
    }
}
