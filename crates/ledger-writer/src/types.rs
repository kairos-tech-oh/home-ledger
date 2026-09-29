//! The names budget lines and holdings are filed under. The defaults always
//! lead and cannot be removed, so there is always somewhere to file a record.

use crate::{WriteError, Writer};
use ledger_domain::Ledger;
use ledger_domain::records::{
    AuditChange, AuditEntry, DEFAULT_BUDGET_TYPES, DEFAULT_INVESTMENT_TYPES, caps,
};
use ledger_domain::text::plain;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum TypeList {
    Budget,
    Investment,
}

impl TypeList {
    fn defaults(self) -> &'static [&'static str] {
        match self {
            TypeList::Budget => DEFAULT_BUDGET_TYPES,
            TypeList::Investment => DEFAULT_INVESTMENT_TYPES,
        }
    }

    fn names(self, ledger: &mut Ledger) -> &mut Vec<String> {
        match self {
            TypeList::Budget => &mut ledger.budget_types,
            TypeList::Investment => &mut ledger.investment_types,
        }
    }

    fn subject(self) -> &'static str {
        match self {
            TypeList::Budget => "budget type",
            TypeList::Investment => "type",
        }
    }
}

impl Writer {
    pub(crate) fn type_add(
        &self,
        ledger: &mut Ledger,
        list: TypeList,
        name: &str,
    ) -> Result<AuditEntry, WriteError> {
        let name = plain(name, 80);
        if name.is_empty() {
            return Err(WriteError::Refused("a type needs a name".into()));
        }
        let names = list.names(ledger);
        if names.contains(&name) {
            return Err(WriteError::Refused("that type already exists".into()));
        }
        if names.len() >= caps::TYPES {
            return Err(WriteError::Refused("no room for another type".into()));
        }
        names.push(name.clone());
        Ok(self.finish(
            AuditEntry {
                action: "Create".into(),
                subject: list.subject().into(),
                name,
                ..Default::default()
            },
            "type-add",
        ))
    }

    /// Records filed under a deleted type move to the first default rather
    /// than disappearing from every group.
    pub(crate) fn type_delete(
        &self,
        ledger: &mut Ledger,
        list: TypeList,
        name: &str,
    ) -> Result<AuditEntry, WriteError> {
        let name = plain(name, 80);
        let fallback = list.defaults()[0];
        if list.defaults().contains(&name.as_str()) {
            return Err(WriteError::Refused(
                "a default type cannot be deleted".into(),
            ));
        }
        let names = list.names(ledger);
        let Some(at) = names.iter().position(|n| *n == name) else {
            return Err(WriteError::Missing("no such type".into()));
        };
        names.remove(at);

        let mut refiled = 0usize;
        let mut refile = |kind: &mut String| {
            if *kind == name {
                *kind = fallback.to_string();
                refiled += 1;
            }
        };
        match list {
            TypeList::Budget => ledger.budget.iter_mut().for_each(|b| refile(&mut b.kind)),
            TypeList::Investment => ledger
                .investments
                .iter_mut()
                .for_each(|h| refile(&mut h.kind)),
        }

        let noun = match list {
            TypeList::Budget => "lines",
            TypeList::Investment => "holdings",
        };
        Ok(self.finish(
            AuditEntry {
                action: "Delete".into(),
                subject: list.subject().into(),
                name,
                changes: if refiled > 0 {
                    vec![AuditChange {
                        field: format!("refiled under {fallback}"),
                        from: format!("{refiled} {noun}"),
                        to: String::new(),
                    }]
                } else {
                    Vec::new()
                },
                ..Default::default()
            },
            "type-delete",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::TypeList;
    use crate::{Kind, Op, WriteError, Writer};
    use ledger_domain::Ledger;
    use serde_json::json;

    fn apply(ledger: &mut Ledger, op: Op) -> Result<(), WriteError> {
        Writer::new("test").apply(ledger, &op).map(|_| ())
    }

    fn add(ledger: &mut Ledger, list: TypeList, name: &str) -> Result<(), WriteError> {
        apply(
            ledger,
            Op::TypeAdd {
                list,
                name: name.into(),
            },
        )
    }

    fn delete(ledger: &mut Ledger, list: TypeList, name: &str) -> Result<(), WriteError> {
        apply(
            ledger,
            Op::TypeDelete {
                list,
                name: name.into(),
            },
        )
    }

    // The cases below are check-helper.py's, with its inputs and answers.

    #[test]
    fn a_duplicate_is_refused_and_a_custom_type_goes_after_the_defaults() {
        let mut ledger = Ledger::default();
        assert!(add(&mut ledger, TypeList::Budget, "Living").is_err());
        add(&mut ledger, TypeList::Budget, "Pets").unwrap();
        assert_eq!(ledger.budget_types.last().unwrap(), "Pets");
    }

    #[test]
    fn a_default_type_cannot_be_deleted() {
        let mut ledger = Ledger::default();
        assert!(delete(&mut ledger, TypeList::Budget, "Living").is_err());
        assert!(delete(&mut ledger, TypeList::Investment, "Bond").is_err());
    }

    #[test]
    fn deleting_a_type_refiles_its_records_rather_than_orphaning_them() {
        let mut ledger = Ledger::default();
        add(&mut ledger, TypeList::Budget, "Pets").unwrap();
        apply(
            &mut ledger,
            Op::Set {
                kind: Kind::Budget,
                id: String::new(),
                record: json!({ "name": "Rent", "type": "Pets", "monthlyAmount": 2400 }),
            },
        )
        .unwrap();
        delete(&mut ledger, TypeList::Budget, "Pets").unwrap();
        assert_eq!(ledger.budget[0].kind, "Living");
        assert!(!ledger.budget_types.contains(&"Pets".to_string()));

        add(&mut ledger, TypeList::Investment, "Private stake").unwrap();
        apply(
            &mut ledger,
            Op::Set {
                kind: Kind::Holding,
                id: String::new(),
                record: json!({ "name": "Cousin's cafe", "type": "Private stake" }),
            },
        )
        .unwrap();
        delete(&mut ledger, TypeList::Investment, "Private stake").unwrap();
        assert_eq!(ledger.investments[0].kind, "Stock/Security");
    }

    #[test]
    fn deleting_a_type_that_is_not_there_is_reported() {
        let mut ledger = Ledger::default();
        let missing = delete(&mut ledger, TypeList::Investment, "Nope");
        assert!(matches!(missing, Err(WriteError::Missing(_))));
    }

    #[test]
    fn the_list_is_capped() {
        let mut ledger = Ledger::default();
        let room = ledger_domain::records::caps::TYPES - ledger.budget_types.len();
        for n in 0..room {
            add(&mut ledger, TypeList::Budget, &format!("Type {n}")).unwrap();
        }
        assert!(add(&mut ledger, TypeList::Budget, "One more").is_err());
    }
}
