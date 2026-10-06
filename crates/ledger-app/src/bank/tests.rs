//! Bank connections end to end, against a stand-in for Plaid on this machine
//! that answers each call with what Plaid's documentation says it returns.

use super::*;
use crate::state::AppState;
use ledger_config::{Config, Places, Settings, StoreConfig};
use std::collections::VecDeque;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Answers queued per path; the last one for a path repeats. Every request's
/// path and body is kept for the test to look at.
type Answers = HashMap<String, VecDeque<(u16, Value)>>;

#[derive(Clone, Default)]
struct FakePlaid {
    answers: Arc<Mutex<Answers>>,
    seen: Arc<Mutex<Vec<(String, Value)>>>,
}

impl FakePlaid {
    fn answer(&self, path: &str, status: u16, body: Value) {
        self.answers
            .lock()
            .unwrap()
            .entry(path.into())
            .or_default()
            .push_back((status, body));
    }

    fn calls(&self, path: &str) -> Vec<Value> {
        self.seen
            .lock()
            .unwrap()
            .iter()
            .filter(|(p, _)| p == path)
            .map(|(_, b)| b.clone())
            .collect()
    }

    async fn start(&self) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let me = self.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut socket, _)) = listener.accept().await else {
                    return;
                };
                let me = me.clone();
                tokio::spawn(async move {
                    let mut raw = Vec::new();
                    let mut buf = [0u8; 4096];
                    let (head, body) = loop {
                        let n = socket.read(&mut buf).await.unwrap_or(0);
                        if n == 0 {
                            return;
                        }
                        raw.extend_from_slice(&buf[..n]);
                        let text = String::from_utf8_lossy(&raw).to_string();
                        if let Some(at) = text.find("\r\n\r\n") {
                            let head = text[..at].to_string();
                            let length = head
                                .lines()
                                .find_map(|l| {
                                    l.to_lowercase()
                                        .strip_prefix("content-length:")
                                        .map(|v| v.trim().parse::<usize>().unwrap_or(0))
                                })
                                .unwrap_or(0);
                            if raw.len() >= at + 4 + length {
                                break (head, raw[at + 4..at + 4 + length].to_vec());
                            }
                        }
                    };
                    let path = head
                        .split_whitespace()
                        .nth(1)
                        .unwrap_or_default()
                        .to_string();
                    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
                    me.seen.lock().unwrap().push((path.clone(), body));
                    let (status, answer) = {
                        let mut all = me.answers.lock().unwrap();
                        match all.get_mut(&path) {
                            Some(q) if q.len() > 1 => q.pop_front().unwrap(),
                            Some(q) if !q.is_empty() => q.front().unwrap().clone(),
                            _ => (
                                400,
                                json!({ "error_code": "NOT_FAKED", "error_message": path }),
                            ),
                        }
                    };
                    let text = answer.to_string();
                    let reply = format!(
                        "HTTP/1.1 {status} X\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{text}",
                        text.len()
                    );
                    let _ = socket.write_all(reply.as_bytes()).await;
                });
            }
        });
        url
    }
}

fn machine(root: &Path) -> AppState {
    let places = Places {
        config_file: root.join("config.json"),
        data_dir: root.join("data"),
    };
    places
        .save_config(&Config {
            stores: vec![StoreConfig {
                id: "local".into(),
                label: "local".into(),
                settings: Settings::Local {
                    path: root.join("ledger.json"),
                },
                accept_risk: false,
            }],
            device: "Office PC".into(),
            setup_complete: true,
            ..Config::default()
        })
        .unwrap();
    AppState::open(
        places,
        Arc::new(ledger_config::secrets::InMemory::default()),
        true,
        || None,
    )
    .unwrap()
}

const CLIENT_ID: &str = "5f1d0c0ffee0c0ffee000001";
const SECRET: &str = "0123456789abcdef0123456789abcd";

fn kroger(id: &str, posted: &str, made: &str, amount: f64) -> Value {
    json!({ "transaction_id": id, "account_id": "plaid-card", "amount": amount,
            "date": posted, "authorized_date": made, "name": "KROGER #920 COLUMBUS OH",
            "merchant_name": "Kroger", "pending": false,
            "personal_finance_category": { "primary": "FOOD_AND_DRINK",
                "detailed": "FOOD_AND_DRINK_GROCERIES" } })
}

fn accounts(card_owes: f64) -> Value {
    json!({ "accounts": [
        { "account_id": "plaid-card", "name": "Sapphire Preferred", "mask": "4421",
          "type": "credit", "subtype": "credit card",
          "balances": { "current": card_owes, "available": 8000.0, "limit": 10000,
                        "iso_currency_code": "USD" } },
        { "account_id": "plaid-checking", "name": "Total Checking", "mask": "0007",
          "type": "depository", "subtype": "checking",
          "balances": { "current": 2500.25, "available": 2400, "iso_currency_code": "USD" } }
    ], "item": { "item_id": "item-1" } })
}

async fn edit(state: &AppState, op: Value) {
    crate::commands::apply_value(state, op).await.unwrap();
}

#[tokio::test]
async fn a_bank_connects_fetches_and_fills_a_statement_once() {
    let plaid = FakePlaid::default();
    let url = plaid.start().await;
    let dir = tempfile::tempdir().unwrap();
    let state = machine(dir.path());

    // Keys are checked with Plaid before they are kept.
    plaid.answer(
        "/institutions/get",
        400,
        json!({ "error_code": "INVALID_API_KEYS",
                "error_message": "invalid client_id or secret provided" }),
    );
    plaid.answer("/institutions/get", 200, json!({ "institutions": [] }));
    let refused = bank_save_keys(&state, CLIENT_ID.into(), SECRET.into(), url.clone()).await;
    assert!(
        refused
            .err()
            .unwrap()
            .to_string()
            .contains("invalid client_id"),
        "Plaid's own words"
    );
    assert!(bank_status(&state).await.unwrap().environment.is_none());
    let saved = bank_save_keys(&state, CLIENT_ID.into(), SECRET.into(), url.clone())
        .await
        .unwrap();
    assert_eq!(saved.environment.as_deref(), Some(url.as_str()));

    // Connecting: Hosted Link, then polling until the bank is added.
    plaid.answer(
        "/link/token/create",
        200,
        json!({ "link_token": "link-1",
                "hosted_link_url": "https://secure.plaid.com/hl/abc" }),
    );
    plaid.answer("/link/token/get", 200, json!({ "link_sessions": [] }));
    plaid.answer(
        "/link/token/get",
        200,
        json!({ "link_sessions": [{ "finished_at": "2026-10-06T20:00:00Z", "results": {
            "item_add_results": [{ "public_token": "public-1",
                "institution": { "name": "Chase" } }] } }] }),
    );
    plaid.answer(
        "/item/public_token/exchange",
        200,
        json!({ "access_token": "access-1", "item_id": "item-1" }),
    );
    plaid.answer("/accounts/get", 200, accounts(512.34));

    let link = plaid::Plaid::new(CLIENT_ID, SECRET, &url)
        .unwrap()
        .link_token("install-1", None)
        .await
        .unwrap();
    assert_eq!(link.url, "https://secure.plaid.com/hl/abc");
    let asked = &plaid.calls("/link/token/create")[0];
    assert_eq!(asked["products"], json!(["transactions"]));
    assert!(asked["hosted_link"].is_object());
    assert_eq!(asked["client_id"], CLIENT_ID);

    let first = bank_connect_check(&state, "link-1".into(), None)
        .await
        .unwrap();
    assert_eq!(first.state, "waiting");
    let done = bank_connect_check(&state, "link-1".into(), None)
        .await
        .unwrap();
    assert_eq!(done.state, "connected");
    let item = done.item.unwrap();
    assert_eq!(
        (item.institution.as_str(), item.accounts.len()),
        ("Chase", 2)
    );
    assert!(matches!(
        state.secrets.get("plaid-item-item-1"),
        Ok(Some(Secret::BankToken { .. }))
    ));

    // The ledger's card and checking account, and an open statement.
    edit(
        &state,
        json!({ "op": "set", "kind": "account",
                "record": { "name": "Sapphire", "type": "credit", "total": "400" } }),
    )
    .await;
    edit(
        &state,
        json!({ "op": "set", "kind": "bucket", "record": { "name": "Groceries" } }),
    )
    .await;
    let ledger = document(&state).await.unwrap();
    let card = ledger.accounts[0].id.clone();
    let groceries = ledger.buckets[0].id.clone();
    edit(
        &state,
        json!({ "op": "reconcile-set", "record": { "card": "Sapphire",
            "cardAccountId": card, "statementDate": "2026-09-30", "balance": "300",
            "lines": [{ "label": "KROGER 5005", "amount": "20", "spentOn": "2026-08-20",
                        "bucketId": groceries }] } }),
    )
    .await;
    let statement = document(&state).await.unwrap().reconciliations[0]
        .id
        .clone();

    // Unlinked, the statement says so rather than showing nothing.
    let unlinked = bank_preview(&state, statement.clone(), false)
        .await
        .unwrap();
    assert!(unlinked.rows.is_empty());
    assert!(unlinked.notes[0].contains("No bank account is linked"));

    bank_link(&state, "item-1".into(), "plaid-card".into(), card.clone())
        .await
        .unwrap();

    // Two pages, a pending charge, a payment and one after the statement.
    plaid.answer("/accounts/get", 200, accounts(512.34));
    plaid.answer(
        "/transactions/sync",
        200,
        json!({ "added": [kroger("t1", "2026-09-27", "2026-09-26", 48.59)],
                "modified": [], "removed": [], "next_cursor": "c1", "has_more": true }),
    );
    plaid.answer(
        "/transactions/sync",
        200,
        json!({ "added": [
                    kroger("t2", "2026-09-15", "2026-09-14", 12.0),
                    { "transaction_id": "t3", "account_id": "plaid-card", "amount": -300,
                      "date": "2026-09-20", "name": "Payment Thank You", "pending": false },
                    kroger("t4", "2026-10-02", "2026-10-01", 9.99),
                    { "transaction_id": "t5", "account_id": "plaid-card", "amount": 5,
                      "date": "2026-09-28", "name": "SQ *COFFEE", "pending": true },
                    { "transaction_id": "t6", "account_id": "plaid-checking", "amount": 70,
                      "date": "2026-09-28", "name": "AEP OHIO", "pending": false }
                ],
                "modified": [], "removed": [], "next_cursor": "c2", "has_more": false }),
    );
    let preview = bank_preview(&state, statement.clone(), true).await.unwrap();
    assert_eq!(plaid.calls("/transactions/sync")[1]["cursor"], "c1");
    let rows: Vec<_> = preview
        .rows
        .iter()
        .map(|r| (r.bank_ref.as_str(), r.suggested))
        .collect();
    assert_eq!(
        rows,
        [("t2", true), ("t3", false), ("t1", true), ("t4", false)],
        "oldest first; the pending charge and the checking account left out"
    );
    let t1 = &preview.rows[2];
    assert_eq!(t1.transaction.description, "Kroger");
    assert_eq!(t1.bank_text, "KROGER #920 COLUMBUS OH");
    assert_eq!(t1.transaction.date, "2026-09-26", "the day it was made");
    assert_eq!(t1.transaction.category, "Groceries");
    assert_eq!(t1.suggested_bucket.as_deref(), Some(groceries.as_str()));
    assert!(preview.rows[3].after_statement);
    assert!(!preview.rows[1].transaction.charge);

    // Adding the chosen ones keeps the bank's id, labelled as the bank's.
    let lines: Vec<Value> = preview
        .rows
        .iter()
        .filter(|r| r.suggested)
        .map(|r| {
            json!({ "label": r.transaction.description, "amount": r.value,
                    "spentOn": r.transaction.date, "member": "All",
                    "bucketId": r.suggested_bucket.clone().unwrap_or_default(),
                    "bankRef": r.bank_ref, "bankText": r.bank_text })
        })
        .collect();
    let applied = bank_import(&state, statement.clone(), lines.clone())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(applied.entry.via, "plaid");
    assert_eq!(applied.entry.client, "desktop");
    let kept = &document(&state).await.unwrap().reconciliations[0];
    assert_eq!(kept.lines.len(), 3);
    assert_eq!(kept.lines[2].bank_ref, "t1");
    assert_eq!(kept.lines[2].bucket_id, groceries);

    // Shown again: already there. Imported again: nothing happens.
    plaid.answer(
        "/transactions/sync",
        200,
        json!({ "added": [], "modified": [], "removed": [], "next_cursor": "c2",
                "has_more": false }),
    );
    let again = bank_preview(&state, statement.clone(), true).await.unwrap();
    assert_eq!(again.charges, 0);
    assert_eq!(again.duplicates, 2);
    assert!(
        bank_import(&state, statement.clone(), lines)
            .await
            .unwrap()
            .is_none()
    );

    // Balances. The card has an open statement, which sets what it owes,
    // so the bank's figure is not offered for it; checking's is.
    assert!(bank_balances(&state).await.unwrap().is_empty());
    edit(
        &state,
        json!({ "op": "set", "kind": "account",
                "record": { "name": "Checking", "type": "checking", "total": "100" } }),
    )
    .await;
    let checking = document(&state)
        .await
        .unwrap()
        .accounts
        .iter()
        .find(|a| a.name == "Checking")
        .unwrap()
        .id
        .clone();
    bank_link(
        &state,
        "item-1".into(),
        "plaid-checking".into(),
        checking.clone(),
    )
    .await
    .unwrap();
    let proposed = bank_balances(&state).await.unwrap();
    assert_eq!(proposed.len(), 1);
    assert_eq!(proposed[0].to, "2500.25");
    assert_eq!(proposed[0].bank_account, "Total Checking ··0007");
    assert_eq!(proposed[0].available_credit, None);
    assert_eq!(
        bank_apply_balances(&state, vec![checking.clone()])
            .await
            .unwrap(),
        1
    );
    let ledger = document(&state).await.unwrap();
    let account = ledger.accounts.iter().find(|a| a.id == checking).unwrap();
    assert_eq!(account.total.unwrap().to_string(), "2500.25");
    assert!(
        bank_balances(&state).await.unwrap().is_empty(),
        "nothing left to offer"
    );
    let history = state.audit().read().await;
    assert_eq!(history.iter().filter(|e| e.via == "plaid").count(), 2);

    // Disconnecting ends it at Plaid and forgets it here.
    plaid.answer("/item/remove", 200, json!({ "request_id": "r" }));
    let after = bank_disconnect(&state, "item-1".into()).await.unwrap();
    assert!(after.items.is_empty());
    assert_eq!(plaid.calls("/item/remove")[0]["access_token"], "access-1");
    assert!(matches!(state.secrets.get("plaid-item-item-1"), Ok(None)));
}

#[tokio::test]
async fn a_bank_that_wants_a_new_sign_in_says_so_and_keeps_what_it_had() {
    let plaid = FakePlaid::default();
    let url = plaid.start().await;
    let dir = tempfile::tempdir().unwrap();
    let state = machine(dir.path());
    state
        .secrets
        .set(
            KEYS_ID,
            &Secret::PlaidKeys {
                client_id: CLIENT_ID.into(),
                secret: SECRET.into(),
                environment: url,
            },
        )
        .unwrap();
    state
        .secrets
        .set(
            "plaid-item-item-1",
            &Secret::BankToken {
                access_token: "access-1".into(),
            },
        )
        .unwrap();
    banks(&state)
        .change(|f| {
            f.items.push(Item {
                id: "item-1".into(),
                institution: "Chase".into(),
                cursor: "c9".into(),
                ..Default::default()
            })
        })
        .await
        .unwrap();
    plaid.answer(
        "/accounts/get",
        400,
        json!({ "error_type": "ITEM_ERROR", "error_code": "ITEM_LOGIN_REQUIRED",
                "error_message": "the login details of this item have changed",
                "display_message": null }),
    );
    let fetched = bank_fetch(&state, None).await.unwrap();
    assert!(fetched.items[0].needs_sign_in);
    assert!(fetched.items[0].problem.contains("ITEM_LOGIN_REQUIRED"));
    let kept = &banks(&state).read().await.unwrap().items[0];
    assert_eq!(kept.cursor, "c9", "the place it had got to is kept");
    assert!(kept.needs_sign_in);

    // Signing in again goes through Link with the existing connection.
    plaid.answer(
        "/link/token/create",
        200,
        json!({ "link_token": "link-2", "hosted_link_url": "https://secure.plaid.com/hl/x" }),
    );
    plaid.answer(
        "/link/token/get",
        200,
        json!({ "link_sessions": [
            { "finished_at": "2026-10-06T20:00:00Z", "exit": null, "results": {} } ] }),
    );
    let link = plaid::Plaid::new(CLIENT_ID, SECRET, &url_of(&state))
        .unwrap()
        .link_token("install-1", Some("access-1"))
        .await
        .unwrap();
    assert_eq!(link.token, "link-2");
    let asked = &plaid.calls("/link/token/create")[0];
    assert_eq!(asked["access_token"], "access-1");
    assert!(
        asked.get("products").is_none(),
        "update mode adds no products"
    );
    let back = bank_connect_check(&state, "link-2".into(), Some("item-1".into()))
        .await
        .unwrap();
    assert_eq!(back.state, "updated");
    assert!(!banks(&state).read().await.unwrap().items[0].needs_sign_in);
}

fn url_of(state: &AppState) -> String {
    match state.secrets.get(KEYS_ID) {
        Ok(Some(Secret::PlaidKeys { environment, .. })) => environment,
        _ => unreachable!(),
    }
}

#[test]
fn a_first_statement_shows_some_weeks_and_a_later_one_starts_after_the_last() {
    let card = "c".repeat(32);
    let t = |id: &str, posted: &str| plaid::RemoteTransaction {
        id: id.into(),
        account_id: "plaid-card".into(),
        date: posted.into(),
        name: "SHELL".into(),
        amount: Money::from(10),
        ..Default::default()
    };
    let file = store::BankFile {
        items: vec![Item {
            id: "item".into(),
            institution: "Chase".into(),
            fetched_at: "2026-10-06T00:00:00Z".into(),
            accounts: vec![store::Account {
                id: "plaid-card".into(),
                linked: card.clone(),
                ..Default::default()
            }],
            transactions: vec![
                t("old", "2026-07-01"),
                t("overlap", "2026-08-28"),
                t("inside", "2026-09-10"),
            ],
            ..Default::default()
        }],
    };
    let statement = |id: &str, date: &str| Reconciliation {
        id: id.into(),
        card_account_id: card.clone(),
        statement_date: date.into(),
        ..Default::default()
    };
    // A first statement: the 45 days before its date.
    let mut ledger = Ledger {
        reconciliations: vec![statement("now", "2026-09-30")],
        ..Default::default()
    };
    let (rows, _) = bank_rows(&file, &ledger, &ledger.reconciliations[0], "2026-10-06");
    let seen: Vec<_> = rows
        .iter()
        .map(|r| (r.bank_ref.as_str(), r.earlier))
        .collect();
    assert_eq!(seen, [("overlap", false), ("inside", false)]);

    // After an earlier statement dated 2026-08-31: from the day after, with
    // the week before it shown but marked as that statement's.
    ledger
        .reconciliations
        .push(statement("before", "2026-08-31"));
    let (rows, _) = bank_rows(&file, &ledger, &ledger.reconciliations[0], "2026-10-06");
    let seen: Vec<_> = rows
        .iter()
        .map(|r| (r.bank_ref.as_str(), r.earlier))
        .collect();
    assert_eq!(seen, [("overlap", true), ("inside", false)]);
    assert!(!rows[0].suggested && rows[1].suggested);
}

#[test]
fn a_charge_typed_or_from_a_csv_is_matched_by_day_and_amount() {
    let card = "c".repeat(32);
    let file = store::BankFile {
        items: vec![Item {
            id: "item".into(),
            fetched_at: "2026-10-06T00:00:00Z".into(),
            accounts: vec![store::Account {
                id: "a".into(),
                linked: card.clone(),
                ..Default::default()
            }],
            transactions: ["t1", "t2"]
                .iter()
                .map(|id| plaid::RemoteTransaction {
                    id: id.to_string(),
                    account_id: "a".into(),
                    date: "2026-09-27".into(),
                    authorized: "2026-09-26".into(),
                    merchant: "Kroger".into(),
                    name: "KROGER #920".into(),
                    amount: Money::new(rust_decimal::Decimal::new(4859, 2)),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }],
    };
    let ledger = Ledger {
        reconciliations: vec![Reconciliation {
            id: "s".into(),
            card_account_id: card,
            statement_date: "2026-09-30".into(),
            lines: vec![ledger_domain::records::ReconLine {
                label: "KROGER #920 COLUMBUS".into(),
                spent_on: "2026-09-26".into(),
                amount: Money::new(rust_decimal::Decimal::new(4859, 2)),
                ..Default::default()
            }],
            ..Default::default()
        }],
        ..Default::default()
    };
    let (rows, _) = bank_rows(&file, &ledger, &ledger.reconciliations[0], "2026-10-06");
    // One line from the CSV covers one of the two identical charges.
    let flags: Vec<_> = rows.iter().map(|r| (r.duplicate, r.suggested)).collect();
    assert_eq!(flags, [(true, false), (false, true)]);
}

#[test]
fn a_card_is_offered_what_it_owes_and_the_credit_left() {
    let card = "c".repeat(32);
    let file = store::BankFile {
        items: vec![Item {
            institution: "Chase".into(),
            accounts: vec![store::Account {
                id: "a".into(),
                name: "Sapphire".into(),
                linked: card.clone(),
                current: Some(Money::new(rust_decimal::Decimal::new(51234, 2))),
                available: Some(Money::from(8000)),
                ..Default::default()
            }],
            ..Default::default()
        }],
    };
    let mut ledger = Ledger {
        accounts: vec![ledger_domain::records::Account {
            id: card.clone(),
            name: "Sapphire".into(),
            kind: "credit".into(),
            total: Some(Money::from(400)),
            ..Default::default()
        }],
        ..Default::default()
    };
    let offered = proposals(&file, &ledger);
    assert_eq!(offered[0].to, "512.34");
    assert_eq!(offered[0].available_credit.as_deref(), Some("8000.00"));
    assert_eq!(offered[0].bank_account, "Sapphire");

    ledger.reconciliations.push(Reconciliation {
        card_account_id: card,
        status: "open".into(),
        ..Default::default()
    });
    assert!(
        proposals(&file, &ledger).is_empty(),
        "an open statement decides"
    );
}

#[tokio::test]
async fn a_bank_account_links_only_to_a_ledger_account_of_its_kind() {
    let dir = tempfile::tempdir().unwrap();
    let state = machine(dir.path());
    for (name, kind) in [("Sapphire", "credit"), ("Fidelity Savings", "savings")] {
        edit(
            &state,
            json!({ "op": "set", "kind": "account", "record": { "name": name, "type": kind } }),
        )
        .await;
    }
    let ledger = document(&state).await.unwrap();
    let id = |name: &str| {
        ledger
            .accounts
            .iter()
            .find(|a| a.name == name)
            .unwrap()
            .id
            .clone()
    };
    banks(&state)
        .change(|f| {
            f.items.push(Item {
                id: "item-1".into(),
                accounts: vec![store::Account {
                    id: "saving".into(),
                    name: "Plaid Saving".into(),
                    kind: "depository".into(),
                    subtype: "savings".into(),
                    ..Default::default()
                }],
                ..Default::default()
            })
        })
        .await
        .unwrap();

    let refused = bank_link(&state, "item-1".into(), "saving".into(), id("Sapphire")).await;
    let said = refused.err().unwrap().to_string();
    assert!(
        said.contains("Plaid Saving is a savings account at the bank")
            && said.contains("a checking or savings account"),
        "{said}"
    );
    let linked = bank_link(
        &state,
        "item-1".into(),
        "saving".into(),
        id("Fidelity Savings"),
    )
    .await
    .unwrap();
    assert_eq!(linked.items[0].accounts[0].linked_name, "Fidelity Savings");
}

#[test]
fn a_payment_to_the_card_is_never_a_purchase_whatever_its_sign() {
    let card = "c".repeat(32);
    let t = |id: &str, amount: i64, category: &str| plaid::RemoteTransaction {
        id: id.into(),
        account_id: "a".into(),
        date: "2026-09-20".into(),
        name: "AUTOMATIC PAYMENT - THANK".into(),
        category: category.into(),
        amount: Money::from(amount),
        ..Default::default()
    };
    let file = store::BankFile {
        items: vec![Item {
            fetched_at: "2026-10-06T00:00:00Z".into(),
            accounts: vec![store::Account {
                id: "a".into(),
                linked: card.clone(),
                ..Default::default()
            }],
            transactions: vec![
                t("sandbox", 2078, "LOAN_PAYMENTS_OTHER_PAYMENT"),
                t("real", -2078, "LOAN_PAYMENTS_CREDIT_CARD_PAYMENT"),
                t("in", 40, "TRANSFER_IN_ACCOUNT_TRANSFER"),
                t("kfc", 12, "FOOD_AND_DRINK_FAST_FOOD"),
            ],
            ..Default::default()
        }],
    };
    let ledger = Ledger {
        reconciliations: vec![Reconciliation {
            card_account_id: card,
            statement_date: "2026-09-30".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let (rows, _) = bank_rows(&file, &ledger, &ledger.reconciliations[0], "2026-10-06");
    let taken: Vec<_> = rows
        .iter()
        .map(|r| (r.bank_ref.as_str(), r.suggested))
        .collect();
    assert_eq!(
        taken,
        [
            ("in", false),
            ("kfc", true),
            ("real", false),
            ("sandbox", false)
        ]
    );
    let sandbox = rows.iter().find(|r| r.bank_ref == "sandbox").unwrap();
    assert_eq!(
        sandbox.transaction.problem,
        "a payment to the card, not a purchase"
    );
}
