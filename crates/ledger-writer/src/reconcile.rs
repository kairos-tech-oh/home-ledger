//! Card statements: start one, edit it, settle it, undo the settle. Settling
//! moves real money, so every check runs before anything moves.

use crate::{WriteError, Writer, clean, from_json};
use ledger_domain::records::{
    Account, Applied, AppliedMove, AuditChange, AuditEntry, CREDIT_TYPES, Reconciliation, caps,
};
use ledger_domain::text::{NAME_MAX, plain, valid_id};
use ledger_domain::{Ledger, Money};
use serde_json::Value;

fn refuse<T>(message: impl Into<String>) -> Result<T, WriteError> {
    Err(WriteError::Refused(message.into()))
}

fn title(record: &Reconciliation) -> String {
    let date = if record.statement_date.is_empty() {
        "undated"
    } else {
        &record.statement_date
    };
    format!("{} · {date}", record.card)
}

fn is_card(account: &Account) -> bool {
    CREDIT_TYPES.contains(&account.kind.as_str())
}

/// The card account a statement belongs to. Records from before
/// `cardAccountId` match by name, exact first, then one unambiguous prefix.
pub fn card_account(accounts: &[Account], record: &Reconciliation) -> Option<usize> {
    let id = valid_id(&record.card_account_id);
    if !id.is_empty() {
        return accounts.iter().position(|a| a.id == id && is_card(a));
    }
    let card = plain(&record.card, NAME_MAX).to_lowercase();
    if card.is_empty() {
        return None;
    }
    let name = |a: &Account| a.name.trim().to_lowercase();
    if let Some(exact) = accounts.iter().position(|a| is_card(a) && name(a) == card) {
        return Some(exact);
    }
    let matches: Vec<usize> = accounts
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            let n = name(a);
            is_card(a) && (n.starts_with(&format!("{card} ")) || card.starts_with(&format!("{n} ")))
        })
        .map(|(i, _)| i)
        .collect();
    (matches.len() == 1).then(|| matches[0])
}

/// The statement balance becomes what the card owes, and the room left on it
/// moves by the same difference.
fn set_credit_balance(account: &mut Account, balance: Money) {
    let debt = match account.total {
        Some(total) => total,
        None => account.debts.iter().map(|d| d.balance).sum(),
    }
    .abs();
    account.total = Some(balance);
    if let Some(room) = account.available_credit {
        account.available_credit = Some(room + debt - balance);
    }
}

/// Each card owes what its latest open statement says, as the prototype sets it
/// on every read. Covers statements saved without setting the card, like old ones.
pub fn card_balances(ledger: &mut Ledger) {
    let mut latest: Vec<(usize, &Reconciliation)> = Vec::new();
    for rec in ledger.reconciliations.iter().filter(|r| r.status == "open") {
        let Some(card) = card_account(&ledger.accounts, rec) else {
            continue;
        };
        match latest.iter_mut().find(|(i, _)| *i == card) {
            Some(slot) if rec.statement_date > slot.1.statement_date => slot.1 = rec,
            Some(_) => {}
            None => latest.push((card, rec)),
        }
    }
    let owed: Vec<(usize, Money)> = latest
        .into_iter()
        .map(|(card, rec)| (card, rec.balance.floor_at_zero()))
        .collect();
    for (card, balance) in owed {
        set_credit_balance(&mut ledger.accounts[card], balance);
    }
}

fn position(recs: &[Reconciliation], id: &str) -> Option<usize> {
    if id.is_empty() {
        return None;
    }
    recs.iter().position(|r| r.id == id)
}

impl Writer {
    pub(crate) fn reconcile_set(
        &self,
        ledger: &mut Ledger,
        id: &str,
        supplied: &Value,
    ) -> Result<AuditEntry, WriteError> {
        if !supplied.is_object() {
            return refuse("reconciliation is not an object");
        }
        let wanted = valid_id(id);
        let existing = position(&ledger.reconciliations, &wanted);
        if let Some(i) = existing
            && ledger.reconciliations[i].status == "settled"
        {
            return refuse("a settled reconciliation is history; undo it to change it");
        }
        if existing.is_none() && ledger.reconciliations.len() >= caps::RECONCILIATIONS {
            return refuse("too many reconciliations");
        }
        let keep = if existing.is_some() {
            wanted.as_str()
        } else {
            ""
        };
        let mut record = clean::reconciliation(from_json(supplied)?, keep, false)?;

        let card = card_account(&ledger.accounts, &record);
        if !record.card_account_id.is_empty() && card.is_none() {
            return refuse("choose an existing credit card account");
        }
        if record.balance.is_negative() {
            return refuse("the balance to pay cannot be negative");
        }
        if let Some(card) = card {
            let accounts = &ledger.accounts;
            let taken = ledger.reconciliations.iter().any(|r| {
                r.id != wanted && r.status == "open" && card_account(accounts, r) == Some(card)
            });
            if taken {
                return refuse(
                    "this card already has an open reconciliation; edit or settle it first",
                );
            }
        }

        // The statement balance is what the card owes from now on, so the card
        // account and every total built on it agree immediately.
        let mut changes = Vec::new();
        if let Some(card) = card {
            let account = &mut ledger.accounts[card];
            record.card_account_id = account.id.clone();
            let before = account.total;
            set_credit_balance(account, record.balance);
            if before != account.total {
                changes.push(AuditChange {
                    field: account.name.clone(),
                    from: before.unwrap_or_default().to_string(),
                    to: record.balance.to_string(),
                });
            }
        }

        let name = title(&record);
        let amount = record.balance;
        match existing {
            Some(i) => ledger.reconciliations[i] = record,
            None => ledger.reconciliations.insert(0, record),
        }
        Ok(self.finish(
            AuditEntry {
                action: if existing.is_some() { "Edit" } else { "Create" }.into(),
                subject: "reconciliation".into(),
                name,
                amount: Some(amount),
                changes,
                ..Default::default()
            },
            "reconcile-set",
        ))
    }

    pub(crate) fn reconcile_delete(
        &self,
        ledger: &mut Ledger,
        id: &str,
    ) -> Result<AuditEntry, WriteError> {
        let Some(i) = position(&ledger.reconciliations, &valid_id(id)) else {
            return Err(WriteError::Missing("no such reconciliation".into()));
        };
        if ledger.reconciliations[i].status == "settled" {
            return refuse("a settled reconciliation cannot be deleted; undo it first");
        }
        let gone = ledger.reconciliations.remove(i);
        Ok(self.finish(
            AuditEntry {
                action: "Delete".into(),
                subject: "reconciliation".into(),
                name: title(&gone),
                amount: Some(gone.balance),
                ..Default::default()
            },
            "reconcile-delete",
        ))
    }

    pub(crate) fn settle(&self, ledger: &mut Ledger, id: &str) -> Result<AuditEntry, WriteError> {
        let Some(i) = position(&ledger.reconciliations, &valid_id(id)) else {
            return Err(WriteError::Missing("no such reconciliation".into()));
        };
        let rec = ledger.reconciliations[i].clone();
        if rec.status != "open" {
            return refuse("this reconciliation is already settled");
        }
        let allocated: Money = rec.lines.iter().map(|l| l.amount).sum();
        if allocated != rec.balance {
            return refuse(format!(
                "the lines add up to {allocated} but the balance is {}",
                rec.balance
            ));
        }

        // Bucket lines grouped per bucket, in first-seen order; the rest is
        // everyday spending.
        let mut by_bucket: Vec<(String, Money)> = Vec::new();
        let mut spend = Money::ZERO;
        for line in &rec.lines {
            if line.bucket_id.is_empty() {
                spend += line.amount;
            } else if let Some(row) = by_bucket.iter_mut().find(|(b, _)| *b == line.bucket_id) {
                row.1 += line.amount;
            } else {
                by_bucket.push((line.bucket_id.clone(), line.amount));
            }
        }

        // Every check runs here, before anything moves.
        let mut bucket_moves = Vec::new();
        for (bucket_id, amount) in &by_bucket {
            let Some(b) = ledger.buckets.iter().position(|b| &b.id == bucket_id) else {
                return refuse("a line points at a bucket that no longer exists");
            };
            // Clamping at zero would lose the difference and break undo.
            let held = ledger.buckets[b].current_total;
            if held < *amount {
                return refuse(format!(
                    "{} holds {held} but this takes {amount}",
                    ledger.buckets[b].name
                ));
            }
            bucket_moves.push((b, *amount));
        }

        let mut account_moves: Vec<(usize, Money)> = Vec::new();
        fn add_move(moves: &mut Vec<(usize, Money)>, index: usize, amount: Money) {
            match moves.iter_mut().find(|(i, _)| *i == index) {
                Some(row) => row.1 += amount,
                None => moves.push((index, amount)),
            }
        }
        if rec.adjust_accounts {
            let taken: Money = by_bucket.iter().map(|(_, a)| *a).sum();
            for (source, amount, what) in [
                (&rec.bucket_source_id, taken, "bucket money"),
                (&rec.spend_source_id, spend, "spending"),
            ] {
                if amount <= Money::ZERO {
                    continue;
                }
                let found = (!source.is_empty())
                    .then(|| ledger.accounts.iter().position(|a| &a.id == source))
                    .flatten();
                let Some(a) = found else {
                    return refuse(format!("choose the account the {what} comes out of"));
                };
                if ledger.accounts[a].total.is_none() {
                    return refuse(format!(
                        "{} has no balance recorded to take this from",
                        ledger.accounts[a].name
                    ));
                }
                add_move(&mut account_moves, a, amount);
            }
        }
        // Paying the card is part of what is recorded, so undo restores it too.
        let card = card_account(&ledger.accounts, &rec);
        if let Some(card) = card {
            if account_moves.iter().any(|(i, _)| *i == card) {
                return refuse("the card being paid cannot also be a payment source");
            }
            add_move(&mut account_moves, card, rec.balance);
        }

        let mut changes = Vec::new();
        let mut applied = Applied::default();
        for (b, amount) in bucket_moves {
            let bucket = &mut ledger.buckets[b];
            let before = bucket.current_total;
            bucket.current_total = before - amount;
            applied.buckets.push(AppliedMove {
                id: bucket.id.clone(),
                amount,
                credit_delta: None,
            });
            changes.push(AuditChange {
                field: bucket.name.clone(),
                from: before.to_string(),
                to: bucket.current_total.to_string(),
            });
        }
        // An account may go below zero: the app's copy of a checking balance
        // can be stale, and refusing would block a real payment.
        for (a, amount) in account_moves {
            let account = &mut ledger.accounts[a];
            let before = account.total.unwrap_or_default();
            account.total = Some(before - amount);
            let mut moved = AppliedMove {
                id: account.id.clone(),
                amount,
                credit_delta: None,
            };
            if Some(a) == card
                && let Some(room) = account.available_credit
            {
                account.available_credit = Some(room + amount);
                moved.credit_delta = Some(amount);
            }
            applied.accounts.push(moved);
            changes.push(AuditChange {
                field: account.name.clone(),
                from: before.to_string(),
                to: (before - amount).to_string(),
            });
        }

        let rec = &mut ledger.reconciliations[i];
        rec.status = "settled".into();
        rec.settled_at = crate::now_iso();
        rec.applied = Some(applied);
        changes.truncate(caps::CHANGES);
        Ok(self.finish(
            AuditEntry {
                action: "Remove".into(),
                subject: "reconciliation".into(),
                name: format!("Settled {}", title(rec)),
                amount: Some(rec.balance),
                changes,
                ..Default::default()
            },
            "reconcile-settle",
        ))
    }

    pub(crate) fn undo(&self, ledger: &mut Ledger, id: &str) -> Result<AuditEntry, WriteError> {
        let Some(i) = position(&ledger.reconciliations, &valid_id(id)) else {
            return Err(WriteError::Missing("no such reconciliation".into()));
        };
        let rec = ledger.reconciliations[i].clone();
        let Some(applied) = rec.applied.as_ref().filter(|_| rec.status == "settled") else {
            return refuse("only a settled reconciliation can be undone");
        };

        // Resolve everything first: an undo that could only partly run would
        // leave money moved in one place and not the other.
        let mut buckets = Vec::new();
        for row in &applied.buckets {
            let Some(b) = ledger.buckets.iter().position(|b| b.id == row.id) else {
                return refuse("a bucket this settled from has since been deleted");
            };
            buckets.push((b, row.amount));
        }
        let mut accounts = Vec::new();
        for row in &applied.accounts {
            let Some(a) = ledger.accounts.iter().position(|a| a.id == row.id) else {
                return refuse("an account this settled from has since been deleted");
            };
            if ledger.accounts[a].total.is_none() {
                return refuse(format!(
                    "{} no longer has a balance to return this to",
                    ledger.accounts[a].name
                ));
            }
            accounts.push((a, row.amount, row.credit_delta));
        }

        let mut changes = Vec::new();
        for (b, amount) in buckets {
            let bucket = &mut ledger.buckets[b];
            let before = bucket.current_total;
            bucket.current_total = before + amount;
            changes.push(AuditChange {
                field: bucket.name.clone(),
                from: before.to_string(),
                to: bucket.current_total.to_string(),
            });
        }
        for (a, amount, credit) in accounts {
            let account = &mut ledger.accounts[a];
            let before = account.total.unwrap_or_default();
            account.total = Some(before + amount);
            if let (Some(delta), Some(room)) = (credit, account.available_credit) {
                account.available_credit = Some(room - delta);
            }
            changes.push(AuditChange {
                field: account.name.clone(),
                from: before.to_string(),
                to: (before + amount).to_string(),
            });
        }

        let rec = &mut ledger.reconciliations[i];
        rec.status = "open".into();
        rec.settled_at.clear();
        rec.applied = None;
        changes.truncate(caps::CHANGES);
        Ok(self.finish(
            AuditEntry {
                action: "Add".into(),
                subject: "reconciliation".into(),
                name: format!("Undid {}", title(rec)),
                amount: Some(rec.balance),
                changes,
                ..Default::default()
            },
            "reconcile-undo",
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Kind, Op, WriteError, Writer};
    use ledger_domain::{Ledger, Money};
    use rust_decimal::Decimal;
    use serde_json::{Value, json};
    use std::str::FromStr;

    fn writer() -> Writer {
        Writer::new("test")
    }

    fn apply(ledger: &mut Ledger, op: Op) -> Result<(), WriteError> {
        writer().apply(ledger, &op).map(|_| ())
    }

    fn make(ledger: &mut Ledger, kind: Kind, record: Value) -> String {
        apply(
            ledger,
            Op::Set {
                kind,
                id: String::new(),
                record,
            },
        )
        .unwrap();
        match kind {
            Kind::Account => ledger.accounts.last().unwrap().id.clone(),
            _ => ledger.buckets.last().unwrap().id.clone(),
        }
    }

    fn set(ledger: &mut Ledger, id: &str, record: Value) -> Result<(), WriteError> {
        apply(
            ledger,
            Op::ReconcileSet {
                id: id.into(),
                record,
            },
        )
    }

    fn settle(ledger: &mut Ledger, id: &str) -> Result<(), WriteError> {
        apply(ledger, Op::ReconcileSettle { id: id.into() })
    }

    fn undo(ledger: &mut Ledger, id: &str) -> Result<(), WriteError> {
        apply(ledger, Op::ReconcileUndo { id: id.into() })
    }

    fn m(text: &str) -> Money {
        Money::new(Decimal::from_str(text).unwrap())
    }

    /// Every bucket and account balance, by name, for "nothing moved" checks.
    fn money_state(ledger: &Ledger) -> Vec<(String, Option<Money>, Option<Money>)> {
        let mut out: Vec<_> = ledger
            .buckets
            .iter()
            .map(|b| (b.name.clone(), Some(b.current_total), None))
            .collect();
        out.extend(
            ledger
                .accounts
                .iter()
                .map(|a| (a.name.clone(), a.total, a.available_credit)),
        );
        out
    }

    fn balance(ledger: &Ledger, name: &str) -> Money {
        if let Some(b) = ledger.buckets.iter().find(|b| b.name == name) {
            return b.current_total;
        }
        ledger
            .accounts
            .iter()
            .find(|a| a.name == name)
            .unwrap()
            .total
            .unwrap()
    }

    struct Setup {
        ledger: Ledger,
        sav: String,
        chk: String,
        car: String,
        bills: String,
    }

    fn setup() -> Setup {
        let mut ledger = Ledger::default();
        let sav = make(
            &mut ledger,
            Kind::Account,
            json!({ "name": "Savings", "type": "savings", "total": 1000 }),
        );
        let chk = make(
            &mut ledger,
            Kind::Account,
            json!({ "name": "Checking", "type": "checking", "total": 200 }),
        );
        make(
            &mut ledger,
            Kind::Account,
            json!({ "name": "Sapphire", "type": "credit", "total": 0 }),
        );
        let car = make(
            &mut ledger,
            Kind::Bucket,
            json!({ "name": "Car", "currentTotal": 500 }),
        );
        let bills = make(
            &mut ledger,
            Kind::Bucket,
            json!({ "name": "Bills", "currentTotal": 40 }),
        );
        Setup {
            ledger,
            sav,
            chk,
            car,
            bills,
        }
    }

    fn recon(s: &Setup, balance: f64, lines: Value) -> Value {
        json!({ "card": "Sapphire", "statementDate": "2026-09-14", "balance": balance,
                "bucketSourceId": s.sav, "spendSourceId": s.chk, "lines": lines })
    }

    // The cases below are check-helper.py's, with its inputs and answers.

    #[test]
    fn settling_moves_every_balance_and_undo_returns_each_exactly() {
        let mut s = setup();
        let lines = json!([{ "label": "Car service", "amount": 300, "bucketId": s.car },
                           { "label": "Kroger, not groceries", "amount": 40 }]);
        let record = recon(&s, 350.0, lines);
        set(&mut s.ledger, "", record).unwrap();
        let rid = s.ledger.reconciliations[0].id.clone();
        assert_eq!(s.ledger.reconciliations[0].status, "open");
        assert_eq!(
            balance(&s.ledger, "Sapphire"),
            Money::from(350),
            "saving did not set the card balance"
        );

        let before = money_state(&s.ledger);
        let Err(WriteError::Refused(why)) = settle(&mut s.ledger, &rid) else {
            panic!("an unbalanced settle was accepted");
        };
        assert!(why.contains("340") && why.contains("350"), "{why}");
        assert_eq!(
            money_state(&s.ledger),
            before,
            "an unbalanced settle moved something"
        );

        let record = recon(
            &s,
            90.0,
            json!([{ "label": "Bills", "amount": 90, "bucketId": s.bills }]),
        );
        set(&mut s.ledger, &rid, record).unwrap();
        let before_short = money_state(&s.ledger);
        let Err(WriteError::Refused(why)) = settle(&mut s.ledger, &rid) else {
            panic!("overdrawing a bucket was accepted");
        };
        assert!(why.contains("Bills"), "{why}");
        assert_eq!(money_state(&s.ledger), before_short);

        let lines = json!([{ "label": "Car service", "amount": 300, "bucketId": s.car },
                           { "label": "Kroger, not groceries", "amount": 50 }]);
        let record = recon(&s, 350.0, lines);
        set(&mut s.ledger, &rid, record).unwrap();
        let before = money_state(&s.ledger);
        settle(&mut s.ledger, &rid).unwrap();
        assert_eq!(balance(&s.ledger, "Car"), Money::from(200));
        assert_eq!(balance(&s.ledger, "Bills"), Money::from(40));
        assert_eq!(balance(&s.ledger, "Savings"), Money::from(700));
        assert_eq!(balance(&s.ledger, "Checking"), Money::from(150));
        assert_eq!(balance(&s.ledger, "Sapphire"), Money::ZERO);
        assert_eq!(s.ledger.reconciliations[0].status, "settled");

        let record = recon(&s, 1.0, json!([]));
        assert!(
            set(&mut s.ledger, &rid, record).is_err(),
            "a settled one was edited"
        );
        assert!(apply(&mut s.ledger, Op::ReconcileDelete { id: rid.clone() }).is_err());
        assert!(settle(&mut s.ledger, &rid).is_err(), "settled twice");

        undo(&mut s.ledger, &rid).unwrap();
        assert_eq!(
            money_state(&s.ledger),
            before,
            "undo did not return every figure"
        );
        assert_eq!(s.ledger.reconciliations[0].status, "open");
        assert!(undo(&mut s.ledger, &rid).is_err(), "an open one was undone");

        settle(&mut s.ledger, &rid).unwrap();
        let car = s.car.clone();
        apply(
            &mut s.ledger,
            Op::Delete {
                kind: Kind::Bucket,
                id: car,
            },
        )
        .unwrap();
        let after_delete = money_state(&s.ledger);
        assert!(
            undo(&mut s.ledger, &rid).is_err(),
            "undo ran with its bucket gone"
        );
        assert_eq!(
            money_state(&s.ledger),
            after_delete,
            "a refused undo moved something"
        );
    }

    #[test]
    fn an_edit_cannot_claim_it_is_settled_or_that_money_moved() {
        let mut s = setup();
        let forged = json!({ "card": "Forged", "balance": 1, "status": "settled",
                             "applied": { "buckets": [{ "id": s.car, "amount": 999 }] }, "lines": [] });
        set(&mut s.ledger, "", forged).unwrap();
        let rec = &s.ledger.reconciliations[0];
        assert_eq!(rec.status, "open");
        assert!(rec.applied.is_none());
        let id = rec.id.clone();
        assert!(undo(&mut s.ledger, &id).is_err());
    }

    fn card_with(statements: Value) -> Ledger {
        let doc = json!({
            "accounts": [{ "id": "c".repeat(32), "name": "Card Premier", "type": "credit",
                           "total": null, "availableCredit": 1000 }],
            "reconciliations": statements,
        });
        crate::read(doc.to_string().as_bytes()).unwrap()
    }

    #[test]
    fn a_legacy_statement_sets_its_card_when_read() {
        // check-credit.py's first step: matched by name, the card owes 200.
        let ledger = card_with(
            json!([{ "id": "a".repeat(32), "card": "Card", "balance": 200,
                                        "status": "open", "lines": [] }]),
        );
        let card = &ledger.accounts[0];
        assert_eq!(
            (card.total, card.available_credit),
            (Some(m("200")), Some(m("800")))
        );
    }

    #[test]
    fn the_latest_open_statement_wins_and_a_settled_one_is_ignored() {
        let ledger = card_with(json!([
            { "id": "a".repeat(32), "card": "Card Premier", "balance": 50,
              "statementDate": "2026-08-14", "status": "open" },
            { "id": "b".repeat(32), "card": "Card Premier", "balance": 120,
              "statementDate": "2026-09-14", "status": "open" },
            { "id": "d".repeat(32), "card": "Card Premier", "balance": 999,
              "statementDate": "2026-10-14", "status": "settled" }
        ]));
        assert_eq!(ledger.accounts[0].total, Some(m("120")));
    }

    #[test]
    fn a_replay_writes_the_derived_card_balance() {
        let doc = json!({
            "accounts": [{ "id": "c".repeat(32), "name": "Card Premier", "type": "credit" }],
            "reconciliations": [{ "id": "a".repeat(32), "card": "Card Premier", "balance": 75,
                                  "status": "open" }],
        });
        let queued = ledger_store::PendingOp {
            id: ledger_domain::text::new_id(),
            at: crate::now_iso(),
            device: "test".into(),
            op: json!({ "op": "set", "kind": "bucket", "record": { "name": "Any" } }),
        };
        use ledger_store::Document;
        let bytes = writer()
            .replay(Some(doc.to_string().as_bytes()), &[queued])
            .unwrap();
        let written = Ledger::from_bytes(&bytes).unwrap();
        assert_eq!(written.accounts[0].total, Some(m("75")));
    }

    // The cases below are check-credit.py's, in its order.

    #[test]
    fn a_card_balance_follows_its_statement_through_every_step() {
        let mut ledger = Ledger::default();
        let card = make(
            &mut ledger,
            Kind::Account,
            json!({ "name": "Card Premier", "type": "credit", "availableCredit": 1000 }),
        );
        let checking = make(
            &mut ledger,
            Kind::Account,
            json!({ "name": "Checking", "type": "checking", "total": 1000 }),
        );
        let rid = "a".repeat(32);

        let check = |ledger: &Ledger, debt: &str, room: &str, cash: &str| {
            let c = ledger.account(&card).unwrap();
            let k = ledger.account(&checking).unwrap();
            assert_eq!(
                (c.total, c.available_credit, k.total),
                (Some(m(debt)), Some(m(room)), Some(m(cash)))
            );
        };

        // A legacy record: no cardAccountId, matched to "Card Premier" by name.
        let mut record = json!({ "card": "Card", "balance": 200, "spendSourceId": checking,
                                 "lines": [{ "label": "Charge", "amount": 200 }] });
        let mut legacy: ledger_domain::records::Reconciliation =
            serde_json::from_value(record.clone()).unwrap();
        legacy.id = rid.clone();
        legacy.status = "open".into();
        ledger.reconciliations.push(legacy);

        set(&mut ledger, &rid, record.clone()).unwrap();
        check(&ledger, "200", "800", "1000");
        set(&mut ledger, &rid, record.clone()).unwrap();
        check(&ledger, "200", "800", "1000");

        record["balance"] = json!(300);
        record["lines"] = json!([{ "label": "Charge", "amount": 300 }]);
        record["cardAccountId"] = json!(card);
        set(&mut ledger, &rid, record.clone()).unwrap();
        check(&ledger, "300", "700", "1000");
        assert!(
            set(&mut ledger, "", record.clone()).is_err(),
            "a second open statement for one card"
        );
        check(&ledger, "300", "700", "1000");

        settle(&mut ledger, &rid).unwrap();
        check(&ledger, "0", "1000", "700");
        undo(&mut ledger, &rid).unwrap();
        check(&ledger, "300", "700", "1000");

        record["spendSourceId"] = json!(card);
        set(&mut ledger, &rid, record.clone()).unwrap();
        assert!(settle(&mut ledger, &rid).is_err(), "the card paid itself");
        check(&ledger, "300", "700", "1000");

        record["spendSourceId"] = json!(checking);
        record["adjustAccounts"] = json!(false);
        set(&mut ledger, &rid, record.clone()).unwrap();
        settle(&mut ledger, &rid).unwrap();
        check(&ledger, "0", "1000", "1000");
        undo(&mut ledger, &rid).unwrap();
        check(&ledger, "300", "700", "1000");

        apply(&mut ledger, Op::ReconcileDelete { id: rid.clone() }).unwrap();
        check(&ledger, "300", "700", "1000");

        record["balance"] = json!(42.5);
        record["lines"] = json!([{ "label": "Groceries", "amount": 42.5 }]);
        set(&mut ledger, "", record.clone()).unwrap();
        check(&ledger, "42.5", "957.5", "1000");
        let saved = ledger.reconciliations[0].clone();
        assert_eq!(saved.lines[0].label, "Groceries");
        assert_eq!(saved.lines[0].amount, m("42.5"));

        let mut lines = serde_json::to_value(&saved.lines).unwrap();
        lines
            .as_array_mut()
            .unwrap()
            .push(json!({ "label": "Fuel", "amount": 20 }));
        record["balance"] = json!(62.5);
        record["lines"] = lines;
        set(&mut ledger, &saved.id, record).unwrap();
        check(&ledger, "62.5", "937.5", "1000");
        let amounts: Vec<Money> = ledger.reconciliations[0]
            .lines
            .iter()
            .map(|l| l.amount)
            .collect();
        assert_eq!(amounts, [m("42.5"), m("20")]);
    }
}
