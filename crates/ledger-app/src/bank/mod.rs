//! Bank connections through Plaid, with the person's own keys. The design is
//! in docs/BANK-CONNECTIONS.md: the bank is a source for the import and the
//! balances the app already has, and nothing is written without being shown.

pub mod plaid;
pub mod store;

use crate::commands::{Answer, Applied, CommandError, apply_via};
use crate::state::AppState;
use crate::transactions::{ImportPreview, Row, suggest_buckets, summarise};
use ledger_config::Secret;
use ledger_domain::records::Reconciliation;
use ledger_domain::{Ledger, Money};
use ledger_writer::bank_csv::{Mapping, Transaction};
use plaid::{LinkState, LinkToken, Plaid, PlaidError, category_words};
use serde::Serialize;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};
use store::{Banks, Item};

/// Where the keys are filed in the keychain.
pub const KEYS_ID: &str = "plaid-keys";
/// The label bank edits carry in the change history.
pub const VIA: &str = "plaid";
/// Pages of one fetch, at most: 500 transactions each.
const MAX_PAGES: usize = 200;
/// How far before a statement the bank's charges are shown when the card has
/// no earlier statement to start from.
const FIRST_STATEMENT_DAYS: i64 = 45;
/// How far before the start of a statement charges are still shown, marked
/// as most likely the last statement's.
const OVERLAP_DAYS: i64 = 7;

fn token_id(item: &str) -> String {
    format!("plaid-item-{item}")
}

pub fn banks(state: &AppState) -> Banks {
    Banks::sealed(&state.places.data_dir, state.vault.clone())
}

fn message(e: impl std::fmt::Display) -> CommandError {
    CommandError::Message(e.to_string())
}

fn client(state: &AppState) -> Answer<Plaid> {
    match state.secrets.get(KEYS_ID) {
        Ok(Some(Secret::PlaidKeys {
            client_id,
            secret,
            environment,
        })) => Plaid::new(&client_id, &secret, &environment).map_err(message),
        Ok(_) => Err(message(
            "no Plaid keys are saved on this computer; add them under Bank connections",
        )),
        Err(e) => Err(message(e)),
    }
}

fn access_token(state: &AppState, item: &str) -> Result<String, String> {
    match state.secrets.get(&token_id(item)) {
        Ok(Some(Secret::BankToken { access_token })) => Ok(access_token),
        Ok(_) => Err("this computer has no access token for it; connect it again".into()),
        Err(e) => Err(e.to_string()),
    }
}

async fn document(state: &AppState) -> Answer<Ledger> {
    let loaded = state.live().await.engine.load().await?;
    Ok(match &loaded.snapshot {
        Some(s) => ledger_writer::read(&s.body)?,
        None => Ledger::default(),
    })
}

// ------------------------------------------------------------------ views

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BankStatus {
    /// "sandbox" or "production" when keys are saved.
    pub environment: Option<String>,
    /// False when no keychain could be reached, so nothing saved survives.
    pub keychain: bool,
    pub items: Vec<ItemView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ItemView {
    pub id: String,
    pub institution: String,
    pub connected_at: String,
    pub fetched_at: String,
    pub gathering: bool,
    pub problem: String,
    pub needs_sign_in: bool,
    pub transactions: usize,
    pub accounts: Vec<AccountView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub id: String,
    pub name: String,
    pub mask: String,
    pub kind: String,
    pub subtype: String,
    pub linked: String,
    pub linked_name: String,
    pub current: Option<String>,
    pub available: Option<String>,
}

fn item_view(item: &Item, ledger: &Ledger) -> ItemView {
    ItemView {
        id: item.id.clone(),
        institution: item.institution.clone(),
        connected_at: item.connected_at.clone(),
        fetched_at: item.fetched_at.clone(),
        gathering: item.gathering,
        problem: item.problem.clone(),
        needs_sign_in: item.needs_sign_in,
        transactions: item.transactions.len(),
        accounts: item
            .accounts
            .iter()
            .map(|a| AccountView {
                id: a.id.clone(),
                name: a.name.clone(),
                mask: a.mask.clone(),
                kind: a.kind.clone(),
                subtype: a.subtype.clone(),
                linked: a.linked.clone(),
                linked_name: ledger
                    .accounts
                    .iter()
                    .find(|l| l.id == a.linked)
                    .map(|l| l.name.clone())
                    .unwrap_or_default(),
                current: a.current.map(|m| m.to_string()),
                available: a.available.map(|m| m.to_string()),
            })
            .collect(),
    }
}

pub async fn bank_status(state: &AppState) -> Answer<BankStatus> {
    let environment = match state.secrets.get(KEYS_ID) {
        Ok(Some(Secret::PlaidKeys { environment, .. })) => Some(environment),
        _ => None,
    };
    let file = banks(state).read().await.map_err(message)?;
    // The names of linked accounts are a nicety; a store that cannot be
    // reached must not hide the connections themselves.
    let ledger = document(state).await.unwrap_or_default();
    Ok(BankStatus {
        environment,
        keychain: state.keychain_available,
        items: file.items.iter().map(|i| item_view(i, &ledger)).collect(),
    })
}

// ------------------------------------------------------------------- keys

fn clean_key(raw: &str, what: &str) -> Answer<String> {
    let key = raw.trim();
    let usable = (8..=100).contains(&key.len()) && key.chars().all(|c| c.is_ascii_alphanumeric());
    if usable {
        Ok(key.to_string())
    } else {
        Err(message(format!("that does not look like a Plaid {what}")))
    }
}

/// Saves the person's Plaid keys, once Plaid has accepted them.
pub async fn bank_save_keys(
    state: &AppState,
    client_id: String,
    secret: String,
    environment: String,
) -> Answer<BankStatus> {
    let client_id = clean_key(&client_id, "client id")?;
    let secret = clean_key(&secret, "secret")?;
    let environment = environment.trim().to_lowercase();
    Plaid::new(&client_id, &secret, &environment)
        .map_err(message)?
        .check_keys()
        .await
        .map_err(message)?;
    state.secrets.set(
        KEYS_ID,
        &Secret::PlaidKeys {
            client_id,
            secret,
            environment,
        },
    )?;
    bank_status(state).await
}

pub async fn bank_forget_keys(state: &AppState) -> Answer<BankStatus> {
    state.secrets.forget(KEYS_ID)?;
    bank_status(state).await
}

// -------------------------------------------------------------- connecting

/// Opens a page in the person's own browser. Only Plaid's https pages are
/// ever passed here.
pub fn open_in_browser(url: &str) -> bool {
    if !url.starts_with("https://") {
        return false;
    }
    #[cfg(windows)]
    let launched = std::process::Command::new("rundll32")
        .args(["url.dll,FileProtocolHandler", url])
        .spawn();
    #[cfg(target_os = "macos")]
    let launched = std::process::Command::new("open").arg(url).spawn();
    #[cfg(all(unix, not(target_os = "macos")))]
    let launched = std::process::Command::new("xdg-open").arg(url).spawn();
    launched.is_ok()
}

/// Starts connecting a bank, or signing an existing connection in again,
/// and opens Plaid's page for it in the browser.
pub async fn bank_connect(state: &AppState, item: Option<String>) -> Answer<LinkToken> {
    let plaid = client(state)?;
    let reconnect = match &item {
        Some(id) => Some(access_token(state, id).map_err(message)?),
        None => None,
    };
    let (_, install) = state.primary_and_install().await;
    let user = if install.is_empty() {
        "home-ledger".to_string()
    } else {
        install
    };
    let link = plaid
        .link_token(&user, reconnect.as_deref())
        .await
        .map_err(message)?;
    open_in_browser(&link.url);
    Ok(link)
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connecting {
    /// "waiting", "exited", "connected" or "updated".
    pub state: &'static str,
    pub message: String,
    pub item: Option<ItemView>,
}

/// Where a connection begun with [`bank_connect`] has got to. Called again
/// until it is no longer waiting.
pub async fn bank_connect_check(
    state: &AppState,
    token: String,
    item: Option<String>,
) -> Answer<Connecting> {
    let plaid = client(state)?;
    let waiting = |state, message: String| Connecting {
        state,
        message,
        item: None,
    };
    match plaid.link_state(&token).await.map_err(message)? {
        LinkState::Waiting => Ok(waiting("waiting", String::new())),
        LinkState::Exited(said) => Ok(waiting("exited", said)),
        LinkState::Updated => {
            let Some(id) = item else {
                return Ok(waiting("exited", "no bank was added".into()));
            };
            banks(state)
                .change(|f| {
                    if let Some(i) = f.items.iter_mut().find(|i| i.id == id) {
                        i.needs_sign_in = false;
                        i.problem.clear();
                    }
                })
                .await
                .map_err(message)?;
            Ok(waiting("updated", String::new()))
        }
        LinkState::Added {
            public_token,
            institution,
        } => {
            let (access, id) = plaid.exchange(&public_token).await.map_err(message)?;
            state.secrets.set(
                &token_id(&id),
                &Secret::BankToken {
                    access_token: access.clone(),
                },
            )?;
            let accounts = plaid.accounts(&access).await.map_err(message)?;
            let added = banks(state)
                .change(|f| {
                    let at = f.items.iter().position(|i| i.id == id);
                    let item = match at {
                        Some(i) => &mut f.items[i],
                        None => {
                            f.items.push(Item {
                                id: id.clone(),
                                institution: if institution.is_empty() {
                                    "Bank".into()
                                } else {
                                    institution
                                },
                                connected_at: ledger_writer::now_iso(),
                                ..Default::default()
                            });
                            f.items.last_mut().unwrap()
                        }
                    };
                    item.take_accounts(accounts);
                    item.clone()
                })
                .await
                .map_err(message)?;
            let ledger = document(state).await.unwrap_or_default();
            Ok(Connecting {
                state: "connected",
                message: String::new(),
                item: Some(item_view(&added, &ledger)),
            })
        }
    }
}

/// The ledger account types a bank account of Plaid's `type` can be. A
/// savings account feeding a credit card would put deposits on a statement
/// and a bank balance on a debt, so a mismatch is refused, not guessed at.
/// "other" in the ledger takes anything; a home or car is valued by the
/// person, never by a bank, so nothing links to one.
pub fn fits(plaid_kind: &str) -> &'static [&'static str] {
    match plaid_kind {
        "depository" => &["checking", "savings", "other"],
        "credit" => &["credit", "heloc", "other"],
        "loan" => &["loan", "heloc", "other"],
        "investment" => &[
            "investment",
            "retirement-roth",
            "retirement-traditional",
            "other",
        ],
        _ => &[
            "checking",
            "savings",
            "credit",
            "heloc",
            "loan",
            "investment",
            "retirement-roth",
            "retirement-traditional",
            "other",
        ],
    }
}

/// Links a bank's account to a ledger account, or unlinks it with an empty
/// id. A ledger account is linked to one bank account at most.
pub async fn bank_link(
    state: &AppState,
    item: String,
    account: String,
    ledger_account: String,
) -> Answer<BankStatus> {
    if !ledger_account.is_empty() {
        let ledger = document(state).await?;
        let Some(target) = ledger.accounts.iter().find(|a| a.id == ledger_account) else {
            return Err(message("no such account in the ledger"));
        };
        let file = banks(state).read().await.map_err(message)?;
        let bank_account = file
            .items
            .iter()
            .filter(|i| i.id == item)
            .flat_map(|i| i.accounts.iter())
            .find(|a| a.id == account);
        if let Some(b) = bank_account {
            let allowed = fits(&b.kind);
            if !allowed.contains(&target.kind.as_str()) {
                let what = if b.subtype.is_empty() {
                    &b.kind
                } else {
                    &b.subtype
                };
                let named: Vec<&str> = allowed.iter().filter(|k| **k != "other").copied().collect();
                let choices = match named.split_last() {
                    Some((last, [])) => last.to_string(),
                    Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
                    None => "other".to_string(),
                };
                return Err(message(format!(
                    "{} is a {what} account at the bank, and {} is a {} account in the ledger. \
                     It can be linked to a {choices} account.",
                    b.name, target.name, target.kind,
                )));
            }
        }
    }
    let found = banks(state)
        .change(|f| {
            let mut found = false;
            for i in &mut f.items {
                for a in &mut i.accounts {
                    if i.id == item && a.id == account {
                        a.linked = ledger_account.clone();
                        found = true;
                    } else if !ledger_account.is_empty() && a.linked == ledger_account {
                        a.linked.clear();
                    }
                }
            }
            found
        })
        .await
        .map_err(message)?;
    if !found {
        return Err(message("no such bank account"));
    }
    bank_status(state).await
}

pub async fn bank_disconnect(state: &AppState, item: String) -> Answer<BankStatus> {
    if let (Ok(plaid), Ok(access)) = (client(state), access_token(state, &item)) {
        match plaid.remove(&access).await {
            Ok(()) => {}
            // Already gone at Plaid: nothing left to end there.
            Err(e) if e.code == "ITEM_NOT_FOUND" || e.code == "INVALID_ACCESS_TOKEN" => {}
            Err(e) => return Err(message(e)),
        }
    }
    state.secrets.forget(&token_id(&item))?;
    banks(state)
        .change(|f| f.items.retain(|i| i.id != item))
        .await
        .map_err(message)?;
    bank_status(state).await
}

// ----------------------------------------------------------------- fetching

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Fetched {
    pub items: Vec<FetchedItem>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchedItem {
    pub id: String,
    pub institution: String,
    pub added: usize,
    pub changed: usize,
    pub removed: usize,
    pub gathering: bool,
    pub problem: String,
    pub needs_sign_in: bool,
}

/// Everything one connection has changed since its cursor, all pages, or the
/// reason it could not be had. Starts the walk again if Plaid says the data
/// moved underneath it, as its documentation asks.
async fn pull(plaid: &Plaid, access: &str, from: &str) -> Result<Pulled, PlaidError> {
    let mut attempts = 0;
    'again: loop {
        attempts += 1;
        let mut pulled = Pulled {
            cursor: from.to_string(),
            history_ready: true,
            ..Default::default()
        };
        for _ in 0..MAX_PAGES {
            let page = match plaid.sync(access, &pulled.cursor).await {
                Ok(page) => page,
                Err(e)
                    if e.code == "TRANSACTIONS_SYNC_MUTATION_DURING_PAGINATION" && attempts < 3 =>
                {
                    continue 'again;
                }
                Err(e) => return Err(e),
            };
            pulled.added.extend(page.added);
            pulled.modified.extend(page.modified);
            pulled.removed.extend(page.removed);
            pulled.history_ready &= page.history_ready;
            if !page.next_cursor.is_empty() {
                pulled.cursor = page.next_cursor;
            }
            if !page.has_more {
                break;
            }
        }
        return Ok(pulled);
    }
}

#[derive(Default)]
struct Pulled {
    added: Vec<plaid::RemoteTransaction>,
    modified: Vec<plaid::RemoteTransaction>,
    removed: Vec<String>,
    cursor: String,
    history_ready: bool,
}

/// Fetches new transactions and balances for every connection, or one.
pub async fn bank_fetch(state: &AppState, only: Option<String>) -> Answer<Fetched> {
    let plaid = client(state)?;
    let file = banks(state).read().await.map_err(message)?;
    let mut report = Vec::new();
    for item in file
        .items
        .iter()
        .filter(|i| only.as_ref().is_none_or(|o| *o == i.id))
    {
        let outcome = match access_token(state, &item.id) {
            Err(e) => Err(PlaidError {
                code: String::new(),
                message: e,
            }),
            Ok(access) => match plaid.accounts(&access).await {
                Err(e) => Err(e),
                Ok(accounts) => pull(&plaid, &access, &item.cursor)
                    .await
                    .map(|p| (accounts, p)),
            },
        };
        let id = item.id.clone();
        let line = banks(state)
            .change(|f| {
                let kept = f.items.iter_mut().find(|i| i.id == id)?;
                let mut line = FetchedItem {
                    id: kept.id.clone(),
                    institution: kept.institution.clone(),
                    added: 0,
                    changed: 0,
                    removed: 0,
                    gathering: kept.gathering,
                    problem: String::new(),
                    needs_sign_in: false,
                };
                match outcome {
                    Ok((accounts, pulled)) => {
                        kept.take_accounts(accounts);
                        (line.added, line.changed, line.removed) =
                            kept.take_changes(pulled.added, pulled.modified, &pulled.removed);
                        kept.cursor = pulled.cursor;
                        kept.gathering = !pulled.history_ready;
                        kept.fetched_at = ledger_writer::now_iso();
                        kept.problem.clear();
                        kept.needs_sign_in = false;
                        line.gathering = kept.gathering;
                    }
                    Err(e) => {
                        kept.problem = e.to_string();
                        kept.needs_sign_in = e.needs_sign_in();
                        line.problem = kept.problem.clone();
                        line.needs_sign_in = kept.needs_sign_in;
                    }
                }
                Some(line)
            })
            .await
            .map_err(message)?;
        report.extend(line);
    }
    Ok(Fetched { items: report })
}

// ---------------------------------------------------------------- importing

/// Plaid categories that are never a purchase, whatever the sign.
const PAYMENT_CATEGORIES: &[&str] = &["LOAN_PAYMENTS", "TRANSFER_IN"];

fn days_before(iso: &str, days: i64) -> String {
    let Some(day) = ledger_math::calendar::Day::parse(iso) else {
        return String::new();
    };
    day.plus_days(-days).iso()
}

/// The fetched charges for one statement's card, set against what the
/// ledger already has, as the CSV import shows a file.
pub fn bank_rows(
    banks: &store::BankFile,
    ledger: &Ledger,
    record: &Reconciliation,
    today: &str,
) -> (Vec<Row>, Vec<String>) {
    let mut notes = Vec::new();
    let card = record.card_account_id.as_str();
    let linked: Vec<(&Item, HashSet<&str>)> = banks
        .items
        .iter()
        .map(|i| {
            let ids = i
                .accounts
                .iter()
                .filter(|a| !card.is_empty() && a.linked == card)
                .map(|a| a.id.as_str())
                .collect::<HashSet<_>>();
            (i, ids)
        })
        .filter(|(_, ids)| !ids.is_empty())
        .collect();
    if linked.is_empty() {
        notes.push(format!(
            "No bank account is linked to {}. Link one under Bank connections.",
            if record.card.is_empty() {
                "this card"
            } else {
                &record.card
            }
        ));
        return (Vec::new(), notes);
    }
    for (item, _) in &linked {
        if item.fetched_at.is_empty() {
            notes.push(format!(
                "Nothing has been fetched from {} yet.",
                item.institution
            ));
        } else if item.gathering {
            notes.push(format!(
                "Plaid is still gathering {}'s history; fetch again in a few minutes.",
                item.institution
            ));
        }
        if !item.problem.is_empty() {
            notes.push(format!("{}: {}", item.institution, item.problem));
        }
    }

    // Where this statement starts: the day after the card's last statement
    // before it, or some weeks before its own date for a first one.
    let end = if record.statement_date.is_empty() {
        today
    } else {
        record.statement_date.as_str()
    };
    let previous = ledger
        .reconciliations
        .iter()
        .filter(|r| r.id != record.id && r.card_account_id == record.card_account_id)
        .map(|r| r.statement_date.as_str())
        .filter(|d| !d.is_empty() && *d < end)
        .max();
    let start = match previous {
        Some(d) => d.to_string(),
        None => days_before(end, FIRST_STATEMENT_DAYS),
    };
    let shown_from = days_before(&start, OVERLAP_DAYS);

    // Already in the ledger: by the bank's id anywhere, or, for charges on
    // this statement that came from a CSV or were typed, by day and amount.
    let taken: HashSet<&str> = ledger
        .reconciliations
        .iter()
        .flat_map(|r| r.lines.iter())
        .map(|l| l.bank_ref.as_str())
        .filter(|r| !r.is_empty())
        .collect();
    let mut unlinked: HashMap<(String, String), usize> = HashMap::new();
    for line in record.lines.iter().filter(|l| l.bank_ref.is_empty()) {
        *unlinked
            .entry((line.spent_on.clone(), line.amount.to_string()))
            .or_default() += 1;
    }

    let mut found: Vec<&plaid::RemoteTransaction> = linked
        .iter()
        .flat_map(|(item, ids)| {
            item.transactions
                .iter()
                .filter(move |t| ids.contains(t.account_id.as_str()))
        })
        .filter(|t| !t.pending && t.date.as_str() > shown_from.as_str())
        .collect();
    found.sort_by(|a, b| a.date.cmp(&b.date).then_with(|| a.id.cmp(&b.id)));

    let rows = found
        .into_iter()
        .enumerate()
        .map(|(i, t)| {
            // Money out of a card is a purchase, unless Plaid says it is a
            // payment or a transfer in: Plaid's Sandbox, and some banks,
            // sign a card payment as money out.
            let payment = PAYMENT_CATEGORIES.iter().any(|c| t.category.starts_with(c));
            let charge = t.amount > Money::ZERO && !payment;
            let amount = Money::new(t.amount.inner().abs());
            let made = if t.authorized.is_empty() {
                &t.date
            } else {
                &t.authorized
            };
            let value = amount.to_string();
            let duplicate = taken.contains(t.id.as_str())
                || [made, &t.date].into_iter().any(|d| {
                    match unlinked.get_mut(&(d.clone(), value.clone())) {
                        Some(n) if *n > 0 => {
                            *n -= 1;
                            true
                        }
                        _ => false,
                    }
                });
            let earlier = t.date.as_str() <= start.as_str();
            let after_statement = !record.statement_date.is_empty()
                && t.date.as_str() > record.statement_date.as_str();
            let (description, bank_text) = if t.merchant.is_empty() {
                (t.name.clone(), String::new())
            } else if t.merchant.eq_ignore_ascii_case(&t.name) {
                (t.merchant.clone(), String::new())
            } else {
                (t.merchant.clone(), t.name.clone())
            };
            let problem = if charge {
                String::new()
            } else if payment {
                "a payment to the card, not a purchase".to_string()
            } else {
                "a payment, refund or credit, not a purchase".to_string()
            };
            Row {
                suggested: charge && !duplicate && !earlier && !after_statement,
                value,
                duplicate,
                after_statement,
                earlier,
                bank_ref: t.id.clone(),
                bank_text,
                suggested_bucket: None,
                transaction: Transaction {
                    line: i + 1,
                    date: made.clone(),
                    description,
                    category: category_words(&t.category),
                    kind: String::new(),
                    amount,
                    charge,
                    problem,
                },
            }
        })
        .collect();
    (rows, notes)
}

/// The bank's charges for one statement, ready to choose from. Fetches
/// first when asked, so what is shown is the bank's latest.
pub async fn bank_preview(state: &AppState, id: String, fetch: bool) -> Answer<ImportPreview> {
    let mut notes = Vec::new();
    if fetch {
        for item in bank_fetch(state, None).await?.items {
            if !item.problem.is_empty() {
                notes.push(format!("{}: {}", item.institution, item.problem));
            }
        }
    }
    let ledger = document(state).await?;
    let Some(record) = ledger.reconciliations.iter().find(|r| r.id == id) else {
        return Err(message("no such reconciliation"));
    };
    let file = banks(state).read().await.map_err(message)?;
    let today = time::OffsetDateTime::now_utc().date().to_string();
    let (mut rows, more) = bank_rows(&file, &ledger, record, &today);
    // A fetch's own complaint is already in `notes`; said once is enough.
    for note in more {
        if !notes.contains(&note) {
            notes.push(note);
        }
    }
    suggest_buckets(&mut rows, &ledger);
    Ok(summarise(rows, Vec::new(), Mapping::default(), true, notes))
}

/// Adds chosen bank charges to a statement, labelled as the bank's.
pub async fn bank_import(
    state: &AppState,
    id: String,
    lines: Vec<Value>,
) -> Answer<Option<Applied>> {
    apply_via(
        state,
        json!({ "op": "reconcile-import", "id": id, "lines": lines }),
        VIA,
    )
    .await
}

// ----------------------------------------------------------------- balances

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Proposal {
    pub account_id: String,
    pub account: String,
    pub institution: String,
    pub bank_account: String,
    pub from: Option<String>,
    pub to: String,
    /// For a card or credit line, what is left to spend, as the bank says.
    pub available_credit: Option<String>,
}

/// Each linked account's balance at the bank, where it differs from the
/// ledger's.
pub fn proposals(file: &store::BankFile, ledger: &Ledger) -> Vec<Proposal> {
    let mut out = Vec::new();
    for item in &file.items {
        for bank in &item.accounts {
            let Some(account) = ledger.accounts.iter().find(|a| a.id == bank.linked) else {
                continue;
            };
            let Some(current) = bank.current else {
                continue;
            };
            // A card with an open statement owes what the statement says;
            // the ledger sets it so on every read. The bank's figure also
            // counts charges since, and belongs to the next statement.
            let open_statement = ledger
                .reconciliations
                .iter()
                .any(|r| r.status == "open" && r.card_account_id == account.id);
            if open_statement {
                continue;
            }
            // What is owed is stored as a positive amount owed.
            let to = if account.is_liability() {
                Money::new(current.inner().abs())
            } else {
                current
            };
            let available_credit = if account.takes_credit_limit() {
                bank.available
            } else {
                None
            };
            let same_credit =
                available_credit.is_none() || available_credit == account.available_credit;
            if account.total == Some(to) && same_credit {
                continue;
            }
            out.push(Proposal {
                account_id: account.id.clone(),
                account: account.name.clone(),
                institution: item.institution.clone(),
                bank_account: if bank.mask.is_empty() {
                    bank.name.clone()
                } else {
                    format!("{} ··{}", bank.name, bank.mask)
                },
                from: account.total.map(|m| m.to_string()),
                to: to.to_string(),
                available_credit: available_credit.map(|m| m.to_string()),
            });
        }
    }
    out
}

pub async fn bank_balances(state: &AppState) -> Answer<Vec<Proposal>> {
    let file = banks(state).read().await.map_err(message)?;
    let ledger = document(state).await?;
    Ok(proposals(&file, &ledger))
}

/// Accepts the bank's balance for each account named, one edit each.
pub async fn bank_apply_balances(state: &AppState, accounts: Vec<String>) -> Answer<usize> {
    let mut done = 0;
    for p in bank_balances(state).await? {
        if !accounts.contains(&p.account_id) {
            continue;
        }
        let mut record = json!({ "total": p.to });
        if let Some(credit) = &p.available_credit {
            record["availableCredit"] = json!(credit);
        }
        let op = json!({ "op": "set", "kind": "account", "id": p.account_id, "record": record });
        if apply_via(state, op, VIA).await?.is_some() {
            done += 1;
        }
    }
    Ok(done)
}

#[cfg(test)]
mod tests;
