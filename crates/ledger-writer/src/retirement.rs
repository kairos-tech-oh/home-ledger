//! An employer account's schedule, and the automatic top-ups it drives.

use crate::{WriteError, Writer, clean, from_json, merge};
use ledger_domain::records::{AuditChange, AuditEntry, Retirement, caps};
use ledger_domain::text::valid_id;
use ledger_domain::{Ledger, Money};
use rust_decimal::Decimal;
use serde_json::Value;

/// Whole months from `from` to `to`, both `yyyy-mm`.
fn months_between(from: &str, to: &str) -> i64 {
    let parts =
        |m: &str| -> (i64, i64) { (m[..4].parse().unwrap_or(0), m[5..7].parse().unwrap_or(0)) };
    let (fy, fm) = parts(from);
    let (ty, tm) = parts(to);
    (ty - fy) * 12 + (tm - fm)
}

impl Writer {
    pub(crate) fn retirement_set(
        &self,
        ledger: &mut Ledger,
        id: &str,
        supplied: &Value,
    ) -> Result<AuditEntry, WriteError> {
        let wanted = valid_id(id);
        let Some(account) = ledger.accounts.iter_mut().find(|a| a.id == wanted) else {
            return Err(WriteError::Missing("no such account".into()));
        };
        if !account.is_retirement() {
            return Err(WriteError::Refused(format!(
                "{} is not a retirement account",
                account.name
            )));
        }

        // A field left out keeps what is stored: sending only an amount must
        // not wipe the weights.
        let before = account.retirement.clone();
        let stored = serde_json::to_value(before.clone().unwrap_or_default())
            .map_err(|e| WriteError::Corrupt(e.to_string()))?;
        let merged = if supplied.is_object() {
            merge(&stored, supplied)
        } else {
            stored.clone()
        };
        let mut block = clean::retirement(from_json::<Retirement>(&merged)?, &crate::this_month())?;
        // Keep the month already reached, so editing the amount does not
        // replay every month since top-ups were switched on.
        if block.auto_contribute
            && let Some(kept) = before
                .as_ref()
                .map(|b| clean::month_key(&b.accrued_through))
            && !kept.is_empty()
        {
            block.accrued_through = kept;
        }

        let after = serde_json::to_value(&block).map_err(|e| WriteError::Corrupt(e.to_string()))?;
        let names = Default::default();
        let changes = crate::audit::diff_fields(before.as_ref().map(|_| &stored), &after, &names);
        account.retirement = Some(block);

        Ok(self.finish(
            AuditEntry {
                action: "Update".into(),
                subject: "retirement plan".into(),
                name: account.name.clone(),
                changes,
                ..Default::default()
            },
            "retirement-set",
        ))
    }

    /// Only the figure typed on the account is added: a budget line is money
    /// the budget already moves, so adding it here would count it twice.
    pub(crate) fn accrue(
        &self,
        ledger: &mut Ledger,
        id: &str,
        now: &str,
    ) -> Result<AuditEntry, WriteError> {
        let only = valid_id(id);
        let mut changes = Vec::new();
        let mut added = Money::ZERO;
        let mut touched = 0usize;
        let mut first = String::new();

        for account in ledger.accounts.iter_mut() {
            if !account.is_retirement() || (!only.is_empty() && account.id != only) {
                continue;
            }
            let Some(block) = account.retirement.as_mut() else {
                continue;
            };
            let through = clean::month_key(&block.accrued_through);
            let Some(amount) = block.monthly_contribution else {
                continue;
            };
            if !block.auto_contribute || amount <= Money::ZERO || through.is_empty() {
                continue;
            }
            let months = months_between(&through, now);
            if months <= 0 {
                continue;
            }
            // A clock that says 1998 is a broken value, not 300 missed months.
            let months = months.min(i64::from(caps::ACCRUAL_MONTHS));

            let before = account.total.unwrap_or(Money::ZERO);
            let after = before + amount.scale(Decimal::from(months));
            account.total = Some(after);
            block.accrued_through = now.to_string();
            added += after - before;
            touched += 1;
            if first.is_empty() {
                first = account.name.clone();
            }
            if changes.len() < caps::CHANGES {
                changes.push(AuditChange {
                    field: account.name.clone(),
                    from: before.to_string(),
                    to: after.to_string(),
                });
            }
        }

        if touched == 0 {
            return Err(WriteError::Unchanged("no top-up is due".into()));
        }
        Ok(self.finish(
            AuditEntry {
                action: "Add".into(),
                subject: "retirement".into(),
                name: if touched == 1 {
                    first
                } else {
                    format!("{touched} accounts topped up")
                },
                amount: Some(added),
                changes,
                ..Default::default()
            },
            "retirement-accrue",
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Kind, Op, WriteError, Writer};
    use ledger_domain::records::Retirement;
    use ledger_domain::{Ledger, Money};
    use serde_json::{Value, json};

    const NOW: &str = "2026-09";

    fn writer() -> Writer {
        Writer::new("test")
    }

    fn account(ledger: &mut Ledger, record: Value) -> String {
        writer()
            .apply(
                ledger,
                &Op::Set {
                    kind: Kind::Account,
                    id: String::new(),
                    record,
                },
            )
            .unwrap();
        ledger.accounts.last().unwrap().id.clone()
    }

    fn plan_set(ledger: &mut Ledger, id: &str, record: Value) -> Result<(), WriteError> {
        writer()
            .apply(
                ledger,
                &Op::RetirementSet {
                    id: id.into(),
                    record,
                },
            )
            .map(|_| ())
    }

    fn plan<'a>(ledger: &'a Ledger, id: &str) -> &'a Retirement {
        ledger.account(id).unwrap().retirement.as_ref().unwrap()
    }

    fn four_weights() -> Value {
        json!([{ "name": "S&P 500 Index Fund", "percent": 55 },
               { "name": "Large Cap Growth Index", "percent": 20 },
               { "name": "International Large Cap Growth", "percent": 20 },
               { "name": "Core Bond Fund", "percent": 5 }])
    }

    fn k401(ledger: &mut Ledger) -> String {
        account(
            ledger,
            json!({ "name": "Empower 401k", "type": "retirement-traditional", "total": 100000 }),
        )
    }

    // The cases below are check-helper.py's, with its inputs and answers.

    #[test]
    fn nameless_or_zero_share_weights_are_dropped_and_the_rest_given_ids() {
        let mut ledger = Ledger::default();
        let id = k401(&mut ledger);
        let mut weights = four_weights();
        weights.as_array_mut().unwrap().extend([
            json!({ "name": "", "percent": 10 }),
            json!({ "name": "Zero share", "percent": 0 }),
        ]);
        plan_set(
            &mut ledger,
            &id,
            json!({ "monthlyContribution": 456, "sleeves": weights }),
        )
        .unwrap();

        let p = plan(&ledger, &id);
        let kept: Vec<(&str, Money)> = p
            .sleeves
            .iter()
            .map(|s| (s.name.as_str(), s.percent))
            .collect();
        assert_eq!(
            kept,
            [
                ("S&P 500 Index Fund", Money::from(55)),
                ("Large Cap Growth Index", Money::from(20)),
                ("International Large Cap Growth", Money::from(20)),
                ("Core Bond Fund", Money::from(5)),
            ]
        );
        assert!(p.sleeves.iter().all(|s| s.id.len() == 32));
        assert_eq!((p.auto_contribute, p.accrued_through.as_str()), (false, ""));
    }

    #[test]
    fn weights_over_100_percent_are_refused_and_the_stored_ones_kept() {
        let mut ledger = Ledger::default();
        let id = k401(&mut ledger);
        plan_set(&mut ledger, &id, json!({ "sleeves": four_weights() })).unwrap();
        let refused = plan_set(
            &mut ledger,
            &id,
            json!({ "sleeves": [{ "name": "A", "percent": 70 }, { "name": "B", "percent": 40 }] }),
        );
        assert!(matches!(refused, Err(WriteError::Refused(_))));
        assert_eq!(plan(&ledger, &id).sleeves.len(), 4);
    }

    #[test]
    fn a_negative_contribution_is_refused() {
        let mut ledger = Ledger::default();
        let id = k401(&mut ledger);
        assert!(plan_set(&mut ledger, &id, json!({ "monthlyContribution": -50 })).is_err());
    }

    #[test]
    fn a_schedule_on_a_non_retirement_account_is_refused() {
        let mut ledger = Ledger::default();
        let id = account(
            &mut ledger,
            json!({ "name": "Plain checking", "type": "checking" }),
        );
        assert!(plan_set(&mut ledger, &id, json!({ "monthlyContribution": 100 })).is_err());
        assert!(ledger.account(&id).unwrap().retirement.is_none());
    }

    #[test]
    fn switching_on_starts_the_clock_and_a_partial_save_keeps_the_weights() {
        let mut ledger = Ledger::default();
        let id = k401(&mut ledger);
        plan_set(&mut ledger, &id, json!({ "sleeves": four_weights() })).unwrap();
        plan_set(
            &mut ledger,
            &id,
            json!({ "monthlyContribution": 456, "autoContribute": true }),
        )
        .unwrap();
        let p = plan(&ledger, &id);
        assert_eq!(p.accrued_through, crate::this_month());
        assert_eq!(p.sleeves.len(), 4);

        plan_set(&mut ledger, &id, json!({ "sleeves": [] })).unwrap();
        assert!(
            plan(&ledger, &id).sleeves.is_empty(),
            "an empty list is how they are cleared"
        );

        plan_set(&mut ledger, &id, json!({ "sleeves": four_weights() })).unwrap();
        let p = plan(&ledger, &id);
        assert_eq!(
            (
                p.monthly_contribution,
                p.auto_contribute,
                p.accrued_through.clone()
            ),
            (Some(Money::from(456)), true, crate::this_month())
        );
    }

    fn switched_on(ledger: &mut Ledger, through: &str) -> String {
        let id = k401(ledger);
        plan_set(
            ledger,
            &id,
            json!({ "monthlyContribution": 456, "autoContribute": true }),
        )
        .unwrap();
        ledger
            .accounts
            .last_mut()
            .unwrap()
            .retirement
            .as_mut()
            .unwrap()
            .accrued_through = through.into();
        id
    }

    #[test]
    fn nothing_is_owed_in_the_month_it_was_switched_on() {
        let mut ledger = Ledger::default();
        let id = switched_on(&mut ledger, NOW);
        let quiet = writer().accrue(&mut ledger, "", NOW);
        assert!(matches!(quiet, Err(WriteError::Unchanged(_))));
        assert_eq!(
            ledger.account(&id).unwrap().total,
            Some(Money::from(100000))
        );
    }

    #[test]
    fn three_missed_months_are_caught_up_at_once_and_only_once() {
        let mut ledger = Ledger::default();
        let id = switched_on(&mut ledger, "2026-06");
        let entry = writer().accrue(&mut ledger, "", NOW).unwrap();
        assert_eq!(
            ledger.account(&id).unwrap().total,
            Some(Money::from(101368))
        );
        assert_eq!(plan(&ledger, &id).accrued_through, NOW);
        assert_eq!(
            (entry.amount, entry.name.as_str()),
            (Some(Money::from(1368)), "Empower 401k")
        );

        assert!(
            writer().accrue(&mut ledger, "", NOW).is_err(),
            "a second run added more"
        );
        assert_eq!(
            ledger.account(&id).unwrap().total,
            Some(Money::from(101368))
        );
    }

    #[test]
    fn the_catch_up_crosses_a_year_and_is_capped() {
        let mut ledger = Ledger::default();
        let id = switched_on(&mut ledger, "2025-11");
        writer().accrue(&mut ledger, "", "2026-02").unwrap();
        assert_eq!(
            ledger.account(&id).unwrap().total,
            Some(Money::from(100000 + 3 * 456))
        );

        let mut ledger = Ledger::default();
        let id = switched_on(&mut ledger, "1998-01");
        writer().accrue(&mut ledger, "", NOW).unwrap();
        assert_eq!(
            ledger.account(&id).unwrap().total,
            Some(Money::from(100000 + 120 * 456))
        );
    }

    #[test]
    fn editing_the_amount_keeps_the_clock_and_switching_off_clears_it() {
        let mut ledger = Ledger::default();
        let id = switched_on(&mut ledger, "2026-06");
        plan_set(
            &mut ledger,
            &id,
            json!({ "monthlyContribution": 600, "autoContribute": true }),
        )
        .unwrap();
        assert_eq!(plan(&ledger, &id).accrued_through, "2026-06");

        plan_set(&mut ledger, &id, json!({ "autoContribute": false })).unwrap();
        assert_eq!(plan(&ledger, &id).accrued_through, "");
    }

    #[test]
    fn changing_the_type_away_from_retirement_drops_the_schedule() {
        let mut ledger = Ledger::default();
        let id = switched_on(&mut ledger, NOW);
        writer()
            .apply(
                &mut ledger,
                &Op::Set {
                    kind: Kind::Account,
                    id: id.clone(),
                    record: json!({ "type": "checking" }),
                },
            )
            .unwrap();
        assert!(ledger.account(&id).unwrap().retirement.is_none());
    }
}
