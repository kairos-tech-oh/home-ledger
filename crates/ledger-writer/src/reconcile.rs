//! Card statements: start one, edit it, settle it, undo the settle. Settling
//! moves real money, so every check runs before anything moves.

use crate::{WriteError, Writer, clean, from_json};
use ledger_domain::records::{
    Account, Applied, AppliedMove, AuditChange, AuditEntry, CREDIT_TYPES, Reconciliation, caps,
};
use ledger_domain::text::{NAME_MAX, plain, valid_id};
use ledger_domain::{Ledger, Money};
use serde_json::Value;

/// How one short bucket is covered when a statement is settled.
#[derive(Clone, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cover {
    /// The bucket that is short.
    pub bucket_id: String,
    /// "bucket", "everyday" or "negative".
    pub how: String,
    /// With "bucket", where the rest comes from.
    pub from_bucket_id: String,
}

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

        // Where a charge came from is not something a form edits. A line kept
        // through an edit keeps its bank link even when the client sending it
        // knows nothing of bank links, so a re-fetch still recognises it.
        if let Some(i) = existing {
            let before = &ledger.reconciliations[i].lines;
            for line in &mut record.lines {
                if let Some(old) = before.iter().find(|o| o.id == line.id)
                    && line.bank_ref.is_empty()
                {
                    line.bank_ref = old.bank_ref.clone();
                    if line.bank_text.is_empty() {
                        line.bank_text = old.bank_text.clone();
                    }
                }
            }
        }

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

    /// Adds charges read from a bank export to an open statement. The lines
    /// are cleaned exactly as a hand-typed one is; the statement's balance and
    /// everything else on it are left alone.
    pub(crate) fn reconcile_import(
        &self,
        ledger: &mut Ledger,
        id: &str,
        supplied: &[Value],
    ) -> Result<AuditEntry, WriteError> {
        let wanted = valid_id(id);
        let Some(i) = position(&ledger.reconciliations, &wanted) else {
            return refuse("no such reconciliation");
        };
        if ledger.reconciliations[i].status == "settled" {
            return refuse("a settled reconciliation is history; undo it to add to it");
        }
        // A bank's transaction is added once, on whichever statement took it
        // first. Enforced here, not only in the preview, so a script that
        // imports twice, or two machines importing at once, add nothing twice.
        let mut seen: std::collections::HashSet<String> = ledger
            .reconciliations
            .iter()
            .flat_map(|r| r.lines.iter())
            .filter(|l| !l.bank_ref.is_empty())
            .map(|l| l.bank_ref.clone())
            .collect();
        let mut lines = Vec::with_capacity(supplied.len());
        for raw in supplied {
            let mut line: ledger_domain::records::ReconLine = from_json(raw)?;
            // Always a new line: an import never edits one already there.
            line.id = String::new();
            if !line.bucket_id.is_empty() && ledger.bucket(&line.bucket_id).is_none() {
                line.bucket_id.clear();
            }
            if let Some(line) = clean::recon_line(line) {
                if !line.bank_ref.is_empty() && !seen.insert(line.bank_ref.clone()) {
                    continue;
                }
                lines.push(line);
            }
        }
        if lines.is_empty() {
            return Err(WriteError::Unchanged("nothing to import".into()));
        }
        let record = &mut ledger.reconciliations[i];
        let before = record.lines.len();
        if before + lines.len() > caps::RECON_LINES {
            return refuse(format!(
                "a statement holds at most {} charges; this would make {}",
                caps::RECON_LINES,
                before + lines.len()
            ));
        }
        let added: Money = lines.iter().map(|l| l.amount).sum();
        let count = lines.len();
        record.lines.extend(lines);
        Ok(self.finish(
            AuditEntry {
                action: "Import".into(),
                subject: "reconciliation".into(),
                name: title(record),
                amount: Some(added),
                changes: vec![AuditChange {
                    field: "charges".into(),
                    from: before.to_string(),
                    to: (before + count).to_string(),
                }],
                ..Default::default()
            },
            "reconcile-import",
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

    pub(crate) fn settle(
        &self,
        ledger: &mut Ledger,
        id: &str,
        cover: &[Cover],
    ) -> Result<AuditEntry, WriteError> {
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

        // Every check runs here, before anything moves. A bucket holding less
        // than its lines take is covered as asked, or by its own default.
        let mut bucket_moves: Vec<(usize, Money)> = Vec::new();
        let mut may_go_negative: Vec<usize> = Vec::new();
        fn draw(moves: &mut Vec<(usize, Money)>, index: usize, amount: Money) {
            if amount <= Money::ZERO {
                return;
            }
            match moves.iter_mut().find(|(i, _)| *i == index) {
                Some(row) => row.1 += amount,
                None => moves.push((index, amount)),
            }
        }
        for (bucket_id, amount) in &by_bucket {
            let Some(b) = ledger.buckets.iter().position(|b| &b.id == bucket_id) else {
                return refuse("a line points at a bucket that no longer exists");
            };
            let bucket = &ledger.buckets[b];
            let held = bucket.current_total.floor_at_zero();
            if held >= *amount {
                draw(&mut bucket_moves, b, *amount);
                continue;
            }
            let short = *amount - held;
            let asked = cover.iter().find(|c| &c.bucket_id == bucket_id);
            let how = asked
                .map(|c| c.how.as_str())
                .unwrap_or(bucket.when_short.as_str());
            match how {
                "bucket" => {
                    let from_id = asked
                        .map(|c| c.from_bucket_id.as_str())
                        .filter(|f| !f.is_empty())
                        .unwrap_or(bucket.cover_bucket_id.as_str());
                    let Some(f) = ledger
                        .buckets
                        .iter()
                        .position(|x| x.id == from_id && x.id != *bucket_id)
                    else {
                        return refuse(format!(
                            "choose the bucket the other {short} for {} comes from",
                            bucket.name
                        ));
                    };
                    if ledger.buckets[f].locked {
                        return refuse(format!(
                            "{} is locked, so it cannot cover {}",
                            ledger.buckets[f].name, bucket.name
                        ));
                    }
                    draw(&mut bucket_moves, b, held);
                    draw(&mut bucket_moves, f, short);
                }
                "everyday" => {
                    draw(&mut bucket_moves, b, held);
                    spend += short;
                }
                "negative" => {
                    draw(&mut bucket_moves, b, *amount);
                    may_go_negative.push(b);
                }
                _ => {
                    return refuse(format!(
                        "{} holds {held} but this takes {amount}; choose where the other {short} comes from",
                        bucket.name
                    ));
                }
            }
        }
        // One bucket can be both short itself and the cover for another, so
        // what each gives up is checked once, in total.
        for (b, total) in &bucket_moves {
            let bucket = &ledger.buckets[*b];
            let held = bucket.current_total.floor_at_zero();
            if !may_go_negative.contains(b) && *total > held {
                return refuse(format!(
                    "{} holds {held} but would give up {total}",
                    bucket.name
                ));
            }
        }

        let mut account_moves: Vec<(usize, Money)> = Vec::new();
        fn add_move(moves: &mut Vec<(usize, Money)>, index: usize, amount: Money) {
            match moves.iter_mut().find(|(i, _)| *i == index) {
                Some(row) => row.1 += amount,
                None => moves.push((index, amount)),
            }
        }
        if rec.adjust_accounts {
            // What actually came out of buckets: a shortfall charged to
            // everyday spending comes out of the spending account instead.
            let taken: Money = bucket_moves.iter().map(|(_, a)| *a).sum();
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
        apply(
            ledger,
            Op::ReconcileSettle {
                id: id.into(),
                cover: Vec::new(),
            },
        )
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

#[cfg(test)]
mod import_tests {
    use crate::{Op, WriteError, Writer};
    use ledger_domain::{Ledger, Money};
    use serde_json::json;

    fn apply(
        ledger: &mut Ledger,
        op: serde_json::Value,
    ) -> Result<ledger_domain::records::AuditEntry, WriteError> {
        let op: Op = serde_json::from_value(op).unwrap();
        Writer::new("test").apply(ledger, &op)
    }

    fn statement(ledger: &mut Ledger) -> String {
        apply(
            ledger,
            json!({ "op": "reconcile-set", "record": { "card": "Sapphire", "balance": 100,
                "lines": [{ "label": "Kept", "amount": 10 }] } }),
        )
        .unwrap();
        ledger.reconciliations[0].id.clone()
    }

    #[test]
    fn imported_charges_are_added_after_the_ones_there() {
        let mut ledger = Ledger::default();
        let id = statement(&mut ledger);
        let entry = apply(
            &mut ledger,
            json!({ "op": "reconcile-import", "id": id, "lines": [
                { "label": "COSTCO WHSE #0632", "amount": "282.18", "spentOn": "2026-09-27",
                  "member": "All", "notes": "Shopping" },
                { "label": "Nothing", "amount": "0" },
                { "id": "ffffffffffffffffffffffffffffffff", "label": "KROGER", "amount": 48.59,
                  "spentOn": "09/26/2026", "bucketId": "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee" }
            ]}),
        )
        .unwrap();
        let lines = &ledger.reconciliations[0].lines;
        assert_eq!(lines.len(), 3, "the zero line is dropped");
        assert_eq!(lines[0].label, "Kept");
        assert_eq!(lines[1].member, "All");
        assert_eq!(lines[1].spent_on, "2026-09-27");
        assert_eq!(lines[1].notes, "Shopping");
        // A supplied id is not trusted, a bucket that does not exist is let go,
        // and a date not in yyyy-mm-dd is not guessed at here.
        assert_ne!(lines[2].id, "f".repeat(32));
        assert!(lines[2].bucket_id.is_empty());
        assert!(lines[2].spent_on.is_empty());
        assert_eq!(entry.action, "Import");
        assert_eq!(
            entry.amount,
            Some(Money::new(rust_decimal::Decimal::new(33077, 2)))
        );
        assert_eq!(
            ledger.reconciliations[0].balance,
            Money::from(100),
            "the balance is untouched"
        );
    }

    #[test]
    fn a_settled_statement_takes_no_imports() {
        let mut ledger = Ledger::default();
        let id = statement(&mut ledger);
        ledger.reconciliations[0].status = "settled".into();
        assert!(
            apply(
                &mut ledger,
                json!({ "op": "reconcile-import", "id": id,
            "lines": [{ "label": "x", "amount": 1 }] })
            )
            .is_err()
        );
        assert_eq!(ledger.reconciliations[0].lines.len(), 1);
    }

    #[test]
    fn a_bank_transaction_is_added_once_wherever_it_already_is() {
        let mut ledger = Ledger::default();
        let first = statement(&mut ledger);
        let charge = |r: &str| {
            json!({ "label": "Kroger", "amount": "48.59", "spentOn": "2026-09-26",
                    "bankRef": r, "bankText": "KROGER #920 COLUMBUS OH" })
        };
        apply(
            &mut ledger,
            json!({ "op": "reconcile-import", "id": first,
                    "lines": [charge("txn-A"), charge("txn-A"), charge("bad ref!")] }),
        )
        .unwrap();
        let lines = &ledger.reconciliations[0].lines;
        assert_eq!(
            lines.len(),
            3,
            "the repeat is dropped, the bad ref kept as a typed charge"
        );
        assert_eq!(lines[1].bank_ref, "txn-A");
        assert_eq!(lines[1].bank_text, "KROGER #920 COLUMBUS OH");
        assert!(lines[2].bank_ref.is_empty());

        // Again, onto a second statement: already on the first, so nothing.
        apply(
            &mut ledger,
            json!({ "op": "reconcile-set", "record": { "card": "Freedom", "balance": 5 } }),
        )
        .unwrap();
        let second = ledger
            .reconciliations
            .iter()
            .find(|r| r.card == "Freedom")
            .unwrap()
            .id
            .clone();
        let got = apply(
            &mut ledger,
            json!({ "op": "reconcile-import", "id": second, "lines": [charge("txn-A")] }),
        );
        assert!(matches!(got, Err(WriteError::Unchanged(_))));

        // And the fields survive a write and a read.
        let back = Ledger::from_bytes(&ledger.to_bytes().unwrap()).unwrap();
        let kept = back.reconciliations.iter().find(|r| r.id == first).unwrap();
        assert_eq!(kept.lines[1].bank_ref, "txn-A");
        let typed = serde_json::to_value(&kept.lines[0]).unwrap();
        assert!(typed.get("bankRef").is_none(), "absent on a typed charge");
    }

    #[test]
    fn editing_a_statement_keeps_its_charges_bank_links() {
        let mut ledger = Ledger::default();
        let id = statement(&mut ledger);
        apply(
            &mut ledger,
            json!({ "op": "reconcile-import", "id": id, "lines": [
                { "label": "Kroger", "amount": "48.59", "bankRef": "txn-A", "bankText": "KROGER #920" }
            ]}),
        )
        .unwrap();
        // What the statement form sends: every line, none of the bank fields.
        let lines: Vec<_> = ledger.reconciliations[0]
            .lines
            .iter()
            .map(|l| json!({ "id": l.id, "label": l.label, "amount": l.amount, "member": "Yuki" }))
            .collect();
        apply(
            &mut ledger,
            json!({ "op": "reconcile-set", "id": id,
                    "record": { "card": "Sapphire", "balance": 100, "lines": lines } }),
        )
        .unwrap();
        let line = &ledger.reconciliations[0].lines[1];
        assert_eq!(line.member, "Yuki", "the edit took");
        assert_eq!(line.bank_ref, "txn-A");
        assert_eq!(line.bank_text, "KROGER #920");
    }

    #[test]
    fn nothing_worth_adding_changes_nothing() {
        let mut ledger = Ledger::default();
        let id = statement(&mut ledger);
        let got = apply(
            &mut ledger,
            json!({ "op": "reconcile-import", "id": id, "lines": [] }),
        );
        assert!(matches!(got, Err(WriteError::Unchanged(_))));
    }
}

#[cfg(test)]
mod cover_tests {
    use super::Cover;
    use crate::{Kind, Op, WriteError, Writer};
    use ledger_domain::{Ledger, Money};
    use serde_json::{Value, json};

    struct Setup {
        ledger: Ledger,
        groceries: String,
        overflow: String,
        statement: String,
    }

    fn apply(ledger: &mut Ledger, op: Op) -> Result<(), WriteError> {
        Writer::new("test").apply(ledger, &op).map(|_| ())
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

    /// Groceries holds 120 and a statement takes 180 from it; checking pays
    /// everyday spending and savings pays bucket money.
    fn setup(groceries: Value) -> Setup {
        let mut ledger = Ledger::default();
        let checking = make(
            &mut ledger,
            Kind::Account,
            json!({ "name": "Checking", "type": "checking", "total": 1000 }),
        );
        let savings = make(
            &mut ledger,
            Kind::Account,
            json!({ "name": "Savings", "type": "savings", "total": 5000 }),
        );
        let overflow = make(
            &mut ledger,
            Kind::Bucket,
            json!({ "name": "Overflow", "currentTotal": 500 }),
        );
        let mut g = json!({ "name": "Groceries", "currentTotal": 120 });
        for (k, v) in groceries.as_object().unwrap() {
            g[k] = if v == "OVERFLOW" {
                json!(overflow)
            } else {
                v.clone()
            };
        }
        let groceries = make(&mut ledger, Kind::Bucket, g);
        apply(
            &mut ledger,
            Op::ReconcileSet {
                id: String::new(),
                record: json!({ "card": "Sapphire", "balance": 200, "bucketSourceId": savings,
                    "spendSourceId": checking, "lines": [
                        { "label": "Costco", "amount": 180, "bucketId": groceries },
                        { "label": "Gas", "amount": 20 } ] }),
            },
        )
        .unwrap();
        let statement = ledger.reconciliations[0].id.clone();
        Setup {
            ledger,
            groceries,
            overflow,
            statement,
        }
    }

    fn settle(s: &mut Setup, cover: Vec<Cover>) -> Result<(), WriteError> {
        apply(
            &mut s.ledger,
            Op::ReconcileSettle {
                id: s.statement.clone(),
                cover,
            },
        )
    }

    fn adjust(s: &mut Setup, delta: &str) {
        let op: Op = serde_json::from_value(json!({ "op": "bucket-adjust",
            "adjustments": [{ "id": s.groceries, "delta": delta }] }))
        .unwrap();
        apply(&mut s.ledger, op).unwrap();
    }

    fn cash(ledger: &Ledger, name: &str) -> Money {
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

    fn cover(s: &Setup, how: &str, from: &str) -> Vec<Cover> {
        vec![Cover {
            bucket_id: s.groceries.clone(),
            how: how.into(),
            from_bucket_id: from.into(),
        }]
    }

    fn undo_restores(s: &mut Setup) {
        apply(
            &mut s.ledger,
            Op::ReconcileUndo {
                id: s.statement.clone(),
            },
        )
        .unwrap();
        assert_eq!(cash(&s.ledger, "Groceries"), Money::from(120));
        assert_eq!(cash(&s.ledger, "Overflow"), Money::from(500));
        assert_eq!(cash(&s.ledger, "Checking"), Money::from(1000));
        assert_eq!(cash(&s.ledger, "Savings"), Money::from(5000));
    }

    #[test]
    fn with_no_instruction_and_no_default_a_short_bucket_still_stops_the_settle() {
        let mut s = setup(json!({}));
        let err = settle(&mut s, vec![]).unwrap_err().to_string();
        assert!(err.contains("Groceries") && err.contains("60"), "{err}");
        assert_eq!(cash(&s.ledger, "Groceries"), Money::from(120));
    }

    #[test]
    fn the_rest_can_come_from_another_bucket() {
        let mut s = setup(json!({}));
        let from = s.overflow.clone();
        {
            let c = cover(&s, "bucket", &from);
            settle(&mut s, c)
        }
        .unwrap();
        assert_eq!(cash(&s.ledger, "Groceries"), Money::ZERO);
        assert_eq!(cash(&s.ledger, "Overflow"), Money::from(440));
        // All 180 was bucket money, so it all came out of savings.
        assert_eq!(cash(&s.ledger, "Savings"), Money::from(4820));
        assert_eq!(cash(&s.ledger, "Checking"), Money::from(980));
        undo_restores(&mut s);
    }

    #[test]
    fn the_rest_can_be_charged_to_everyday_spending() {
        let mut s = setup(json!({}));
        {
            let c = cover(&s, "everyday", "");
            settle(&mut s, c)
        }
        .unwrap();
        assert_eq!(cash(&s.ledger, "Groceries"), Money::ZERO);
        // 120 from the bucket via savings; the other 60 joins the 20 of gas.
        assert_eq!(cash(&s.ledger, "Savings"), Money::from(4880));
        assert_eq!(cash(&s.ledger, "Checking"), Money::from(920));
        undo_restores(&mut s);
    }

    #[test]
    fn a_bucket_can_be_let_go_negative_and_money_added_fills_it_back() {
        let mut s = setup(json!({}));
        {
            let c = cover(&s, "negative", "");
            settle(&mut s, c)
        }
        .unwrap();
        assert_eq!(cash(&s.ledger, "Groceries"), Money::from(-60));
        // Spending from it while it is below zero moves nothing...
        adjust(&mut s, "-10");
        assert_eq!(cash(&s.ledger, "Groceries"), Money::from(-60));
        // ...and money added fills it back up.
        adjust(&mut s, "100");
        assert_eq!(cash(&s.ledger, "Groceries"), Money::from(40));
    }

    #[test]
    fn a_negative_bucket_stays_negative_when_renamed_and_undo_restores_it() {
        let mut s = setup(json!({}));
        {
            let c = cover(&s, "negative", "");
            settle(&mut s, c)
        }
        .unwrap();
        let g = s.groceries.clone();
        apply(
            &mut s.ledger,
            Op::Set {
                kind: Kind::Bucket,
                id: g,
                record: json!({ "name": "Groceries " }),
            },
        )
        .unwrap();
        assert_eq!(cash(&s.ledger, "Groceries"), Money::from(-60));
        undo_restores(&mut s);
    }

    #[test]
    fn a_bucket_remembers_how_it_is_covered() {
        let mut s = setup(json!({ "whenShort": "bucket", "coverBucketId": "OVERFLOW" }));
        settle(&mut s, vec![]).unwrap();
        assert_eq!(cash(&s.ledger, "Overflow"), Money::from(440));
        undo_restores(&mut s);
        // An instruction for this settle wins over the default.
        {
            let c = cover(&s, "everyday", "");
            settle(&mut s, c)
        }
        .unwrap();
        assert_eq!(cash(&s.ledger, "Overflow"), Money::from(500));
        assert_eq!(cash(&s.ledger, "Checking"), Money::from(920));
    }

    #[test]
    fn a_cover_that_is_itself_short_or_locked_is_refused() {
        let mut s = setup(json!({}));
        let o = s.overflow.clone();
        let set = |s: &mut Setup, record: Value| {
            apply(
                &mut s.ledger,
                Op::Set {
                    kind: Kind::Bucket,
                    id: o.clone(),
                    record,
                },
            )
            .unwrap()
        };
        set(&mut s, json!({ "currentTotal": 30 }));
        assert!(
            {
                let c = cover(&s, "bucket", &o);
                settle(&mut s, c)
            }
            .is_err()
        );
        set(&mut s, json!({ "currentTotal": 500, "locked": true }));
        let err = {
            let c = cover(&s, "bucket", &o);
            settle(&mut s, c)
        }
        .unwrap_err()
        .to_string();
        assert!(err.contains("locked"), "{err}");
        assert_eq!(cash(&s.ledger, "Groceries"), Money::from(120));
    }

    #[test]
    fn deleting_the_cover_bucket_sends_its_dependant_back_to_asking() {
        let mut s = setup(json!({ "whenShort": "bucket", "coverBucketId": "OVERFLOW" }));
        let o = s.overflow.clone();
        apply(
            &mut s.ledger,
            Op::Delete {
                kind: Kind::Bucket,
                id: o,
            },
        )
        .unwrap();
        let g = s.ledger.bucket(&s.groceries).unwrap();
        assert!(g.when_short.is_empty() && g.cover_bucket_id.is_empty());
    }

    #[test]
    fn a_bucket_cannot_cover_itself() {
        let mut s = setup(json!({}));
        let g = s.groceries.clone();
        let got = apply(
            &mut s.ledger,
            Op::Set {
                kind: Kind::Bucket,
                id: g.clone(),
                record: json!({ "whenShort": "bucket", "coverBucketId": g }),
            },
        );
        assert!(got.is_err());
    }
}
