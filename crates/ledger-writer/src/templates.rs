//! Putting a saved budget back. Goals and templates themselves are ordinary
//! records, set and deleted through `Op::Set` and `Op::Delete`.

use crate::{WriteError, Writer, clean, now_iso};
use ledger_domain::Ledger;
use ledger_domain::records::{AuditChange, AuditEntry, BudgetItem, Template, TemplateItem, caps};
use ledger_domain::text::valid_id;

impl Writer {
    /// Replaces the live budget with the template's lines. The budget it
    /// replaces is saved as a template first, unless `keep_current` is false.
    pub(crate) fn activate(
        &self,
        ledger: &mut Ledger,
        id: &str,
        keep_current: bool,
    ) -> Result<AuditEntry, WriteError> {
        let wanted = valid_id(id);
        let Some(template) = ledger
            .templates
            .iter()
            .find(|t| !wanted.is_empty() && t.id == wanted)
            .cloned()
        else {
            return Err(WriteError::Missing("no such template".into()));
        };

        // Built in full before the budget is touched, so a refusal changes nothing.
        let saved = if keep_current && !ledger.budget.is_empty() {
            if ledger.templates.len() >= caps::TEMPLATES {
                return Err(WriteError::Refused(
                    "no room to save the budget being replaced; delete a template first".into(),
                ));
            }
            let items = ledger
                .budget
                .iter()
                .map(|b| TemplateItem {
                    id: String::new(),
                    name: b.name.clone(),
                    kind: b.kind.clone(),
                    monthly_amount: b.monthly_amount,
                    bucket_id: b.bucket_id.clone(),
                })
                .collect();
            Some(clean::template(
                Template {
                    name: format!("Replaced {}", &now_iso()[..10]),
                    notes: format!("The budget that {} replaced.", template.name),
                    items,
                    ..Default::default()
                },
                "",
            )?)
        } else {
            None
        };

        // A line keeps its bucket only while that bucket still exists.
        let mut budget = Vec::with_capacity(template.items.len());
        for item in &template.items {
            let bucket_id = if ledger.buckets.iter().any(|b| b.id == item.bucket_id) {
                item.bucket_id.clone()
            } else {
                String::new()
            };
            budget.push(clean::budget(
                BudgetItem {
                    name: item.name.clone(),
                    kind: item.kind.clone(),
                    monthly_amount: item.monthly_amount,
                    bucket_id,
                    ..Default::default()
                },
                "",
            )?);
        }

        if let Some(saved) = saved {
            ledger.templates.push(saved);
        }
        let was = ledger.budget.len();
        ledger.budget = budget;
        Ok(self.finish(
            AuditEntry {
                action: "Manual".into(),
                subject: "budget".into(),
                name: template.name,
                changes: vec![AuditChange {
                    field: "lines".into(),
                    from: was.to_string(),
                    to: ledger.budget.len().to_string(),
                }],
                ..Default::default()
            },
            "template-activate",
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Kind, Op, Writer};
    use ledger_domain::{Ledger, Money};
    use serde_json::{Value, json};

    fn apply(ledger: &mut Ledger, op: Op) {
        Writer::new("test").apply(ledger, &op).expect("applies");
    }

    fn set(ledger: &mut Ledger, kind: Kind, record: Value) {
        apply(
            ledger,
            Op::Set {
                kind,
                id: String::new(),
                record,
            },
        );
    }

    // The cases below are check-helper.py's, with its inputs and answers.

    #[test]
    fn a_goal_links_to_a_bucket() {
        let mut ledger = Ledger::default();
        set(&mut ledger, Kind::Bucket, json!({ "name": "Alloc bucket" }));
        let bucket = ledger.buckets[0].id.clone();
        set(
            &mut ledger,
            Kind::Goal,
            json!({ "name": "House deposit", "targetAmount": 60000, "bucketId": bucket }),
        );
        let goal = &ledger.goals[0];
        assert_eq!(
            (
                goal.name.as_str(),
                goal.target_amount,
                goal.bucket_id.as_str()
            ),
            ("House deposit", Some(Money::from(60000)), bucket.as_str())
        );
    }

    fn lean_month(ledger: &mut Ledger) -> String {
        set(
            ledger,
            Kind::Template,
            json!({ "name": "Lean month", "items": [
                { "name": "Rent", "type": "Living", "monthlyAmount": 2000 },
                { "name": "Food", "type": "Living", "monthlyAmount": 400 },
                { "name": "  ", "monthlyAmount": 99 } ] }),
        );
        ledger.templates.last().unwrap().id.clone()
    }

    #[test]
    fn a_template_drops_nameless_lines() {
        let mut ledger = Ledger::default();
        lean_month(&mut ledger);
        assert_eq!(ledger.templates[0].items.len(), 2);
    }

    #[test]
    fn activating_replaces_the_budget_and_saves_the_old_one_first() {
        let mut ledger = Ledger::default();
        set(
            &mut ledger,
            Kind::Budget,
            json!({ "name": "Holiday", "monthlyAmount": 300 }),
        );
        let tpl = lean_month(&mut ledger);

        apply(
            &mut ledger,
            Op::TemplateActivate {
                id: tpl.clone(),
                keep_current: true,
            },
        );
        let mut lines: Vec<(String, Money)> = ledger
            .budget
            .iter()
            .map(|b| (b.name.clone(), b.monthly_amount))
            .collect();
        lines.sort();
        assert_eq!(
            lines,
            [
                ("Food".into(), Money::from(400)),
                ("Rent".into(), Money::from(2000))
            ]
        );
        assert_eq!(ledger.templates.len(), 2);
        assert_eq!(ledger.templates[1].items[0].name, "Holiday");

        apply(
            &mut ledger,
            Op::TemplateActivate {
                id: tpl,
                keep_current: false,
            },
        );
        assert_eq!(ledger.templates.len(), 2, "saved again when told not to");
    }

    #[test]
    fn keep_current_is_the_default_when_left_out() {
        let op: Op =
            serde_json::from_value(json!({ "op": "template-activate", "id": "x" })).unwrap();
        assert!(matches!(
            op,
            Op::TemplateActivate {
                keep_current: true,
                ..
            }
        ));
    }

    #[test]
    fn a_line_whose_bucket_is_gone_loses_the_link() {
        let mut ledger = Ledger::default();
        set(
            &mut ledger,
            Kind::Template,
            json!({ "name": "T", "items": [{ "name": "Car", "bucketId": "e".repeat(32) }] }),
        );
        let tpl = ledger.templates[0].id.clone();
        apply(
            &mut ledger,
            Op::TemplateActivate {
                id: tpl,
                keep_current: false,
            },
        );
        assert_eq!(ledger.budget[0].bucket_id, "");
    }

    #[test]
    fn deleting_a_goal_takes_it_off_the_dashboard() {
        let mut ledger = Ledger::default();
        set(&mut ledger, Kind::Goal, json!({ "name": "House" }));
        let goal = ledger.goals[0].id.clone();
        ledger.unknown.insert(
            "dashboard".into(),
            json!({ "widgets": [{ "kind": "goal", "refs": [goal, "f".repeat(32)] }] }),
        );
        apply(
            &mut ledger,
            Op::Delete {
                kind: Kind::Goal,
                id: goal,
            },
        );
        assert_eq!(
            ledger.unknown["dashboard"]["widgets"][0]["refs"],
            json!(["f".repeat(32)])
        );
    }
}
