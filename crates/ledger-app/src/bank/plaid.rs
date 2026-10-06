//! The few calls to Plaid that bank connections need, and nothing else.
//!
//! The keys are the person's own (docs/BANK-CONNECTIONS.md). Every request
//! carries them in the body, as Plaid's API expects, over TLS to Plaid only.

use ledger_domain::Money;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::str::FromStr;
use std::time::Duration;

pub const SANDBOX: &str = "https://sandbox.plaid.com";
pub const PRODUCTION: &str = "https://production.plaid.com";

const PER_REQUEST: Duration = Duration::from_secs(30);
/// Plaid's largest page of transactions.
const PAGE: u32 = 500;
/// How far back a new connection asks for: Plaid's most.
const HISTORY_DAYS: u32 = 730;

/// What Plaid said went wrong, in its own words where it gave any.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlaidError {
    /// Plaid's `error_code`, such as `ITEM_LOGIN_REQUIRED`. Empty when the
    /// failure was reaching Plaid at all.
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for PlaidError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.code.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "Plaid: {} ({})", self.message, self.code)
        }
    }
}

impl PlaidError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            code: String::new(),
            message: message.into(),
        }
    }

    /// The bank wants the person to sign in again, through Link.
    pub fn needs_sign_in(&self) -> bool {
        matches!(
            self.code.as_str(),
            "ITEM_LOGIN_REQUIRED" | "PENDING_EXPIRATION" | "PENDING_DISCONNECT"
        )
    }
}

pub fn base_for(environment: &str) -> Result<&'static str, String> {
    match environment {
        "sandbox" => Ok(SANDBOX),
        "production" => Ok(PRODUCTION),
        other => Err(format!(
            "\"{other}\" is not a Plaid environment; use sandbox or production"
        )),
    }
}

pub struct Plaid {
    http: reqwest::Client,
    base: String,
    client_id: String,
    secret: String,
}

/// A Link session waiting for the person in their browser.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkToken {
    pub token: String,
    pub url: String,
}

/// Where a Link session has got to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkState {
    Waiting,
    /// Closed without connecting, with Plaid's reason when it gave one.
    Exited(String),
    /// A new bank connected: its public token, to exchange, and its name.
    Added {
        public_token: String,
        institution: String,
    },
    /// An existing connection signed in again; nothing to exchange.
    Updated,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteAccount {
    pub id: String,
    pub name: String,
    pub mask: String,
    /// Plaid's `type`: depository, credit, loan, investment, other.
    pub kind: String,
    pub subtype: String,
    pub current: Option<Money>,
    pub available: Option<Money>,
    pub limit: Option<Money>,
    pub currency: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct RemoteTransaction {
    pub id: String,
    pub account_id: String,
    /// The day it posted, `yyyy-mm-dd`.
    pub date: String,
    /// The day it was made, when the bank says; the date a card's own export
    /// shows.
    pub authorized: String,
    /// The bank's description.
    pub name: String,
    /// Plaid's clean merchant name ("Kroger"), when it recognised one.
    pub merchant: String,
    /// Plaid's detailed category, such as `FOOD_AND_DRINK_GROCERIES`.
    pub category: String,
    /// Positive when money left the account: on a card, a purchase.
    pub amount: Money,
    pub pending: bool,
}

impl Default for RemoteTransaction {
    fn default() -> Self {
        Self {
            id: String::new(),
            account_id: String::new(),
            date: String::new(),
            authorized: String::new(),
            name: String::new(),
            merchant: String::new(),
            category: String::new(),
            amount: Money::ZERO,
            pending: false,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SyncPage {
    pub added: Vec<RemoteTransaction>,
    pub modified: Vec<RemoteTransaction>,
    pub removed: Vec<String>,
    pub next_cursor: String,
    pub has_more: bool,
    /// False while Plaid is still pulling a new connection's history.
    pub history_ready: bool,
}

impl Plaid {
    pub fn new(client_id: &str, secret: &str, environment: &str) -> Result<Self, String> {
        // Tests stand in for Plaid on this machine. Nothing else can point the
        // keys anywhere but Plaid: no setting, no environment variable.
        #[cfg(test)]
        if environment.starts_with("http://127.0.0.1:") {
            return Ok(Self::at(environment, client_id, secret));
        }
        Ok(Self::at(base_for(environment)?, client_id, secret))
    }

    pub fn at(base: impl Into<String>, client_id: &str, secret: &str) -> Self {
        Self {
            http: reqwest::Client::new(),
            base: base.into().trim_end_matches('/').to_string(),
            client_id: client_id.to_string(),
            secret: secret.to_string(),
        }
    }

    async fn call(&self, path: &str, mut body: Value) -> Result<Value, PlaidError> {
        body["client_id"] = json!(self.client_id);
        body["secret"] = json!(self.secret);
        let response = self
            .http
            .post(format!("{}{path}", self.base))
            .json(&body)
            .timeout(PER_REQUEST)
            .send()
            .await
            .map_err(|e| PlaidError::new(format!("could not reach Plaid: {e}")))?;
        let ok = response.status().is_success();
        let code = response.status().as_u16();
        let answer: Value = response.json().await.map_err(|_| {
            PlaidError::new(format!("Plaid returned {code} and no readable answer"))
        })?;
        if ok {
            return Ok(answer);
        }
        let text = |k: &str| answer[k].as_str().unwrap_or_default().to_string();
        let message = match (text("display_message"), text("error_message")) {
            (shown, _) if !shown.is_empty() => shown,
            (_, said) if !said.is_empty() => said,
            _ => format!("Plaid returned {code}"),
        };
        Err(PlaidError {
            code: text("error_code"),
            message,
        })
    }

    /// Checks the keys with the cheapest call that needs them.
    pub async fn check_keys(&self) -> Result<(), PlaidError> {
        self.call(
            "/institutions/get",
            json!({ "count": 1, "offset": 0, "country_codes": ["US"] }),
        )
        .await
        .map(|_| ())
    }

    /// Starts a Hosted Link session. With an access token it signs an existing
    /// connection in again rather than adding a new one.
    pub async fn link_token(
        &self,
        user: &str,
        reconnect: Option<&str>,
    ) -> Result<LinkToken, PlaidError> {
        let mut body = json!({
            "client_name": "Home Ledger",
            "language": "en",
            "country_codes": ["US"],
            "user": { "client_user_id": user },
            "hosted_link": {},
        });
        match reconnect {
            Some(access) => body["access_token"] = json!(access),
            None => {
                body["products"] = json!(["transactions"]);
                body["transactions"] = json!({ "days_requested": HISTORY_DAYS });
            }
        }
        let answer = self.call("/link/token/create", body).await?;
        let token = answer["link_token"].as_str().unwrap_or_default();
        let url = answer["hosted_link_url"].as_str().unwrap_or_default();
        if token.is_empty() || url.is_empty() {
            return Err(PlaidError::new("Plaid did not return a sign-in page"));
        }
        Ok(LinkToken {
            token: token.to_string(),
            url: url.to_string(),
        })
    }

    pub async fn link_state(&self, token: &str) -> Result<LinkState, PlaidError> {
        let answer = self
            .call("/link/token/get", json!({ "link_token": token }))
            .await?;
        Ok(link_state(&answer))
    }

    /// Returns the access token and Plaid's id for the connection.
    pub async fn exchange(&self, public_token: &str) -> Result<(String, String), PlaidError> {
        let answer = self
            .call(
                "/item/public_token/exchange",
                json!({ "public_token": public_token }),
            )
            .await?;
        let access = answer["access_token"].as_str().unwrap_or_default();
        let item = answer["item_id"].as_str().unwrap_or_default();
        if access.is_empty() || item.is_empty() {
            return Err(PlaidError::new("Plaid did not return an access token"));
        }
        Ok((access.to_string(), item.to_string()))
    }

    /// Every account at the bank, with Plaid's latest balances.
    pub async fn accounts(&self, access: &str) -> Result<Vec<RemoteAccount>, PlaidError> {
        let answer = self
            .call("/accounts/get", json!({ "access_token": access }))
            .await?;
        Ok(accounts(&answer))
    }

    /// One page of what changed since `cursor`; an empty cursor starts over.
    pub async fn sync(&self, access: &str, cursor: &str) -> Result<SyncPage, PlaidError> {
        let mut body = json!({ "access_token": access, "count": PAGE });
        if !cursor.is_empty() {
            body["cursor"] = json!(cursor);
        }
        let answer = self.call("/transactions/sync", body).await?;
        Ok(sync_page(&answer))
    }

    /// Ends the connection at Plaid, so the access token stops working.
    pub async fn remove(&self, access: &str) -> Result<(), PlaidError> {
        self.call("/item/remove", json!({ "access_token": access }))
            .await
            .map(|_| ())
    }
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

/// A JSON number as money, through its decimal text so 48.59 stays 48.59.
fn money(v: &Value) -> Option<Money> {
    match v {
        Value::Number(n) => Decimal::from_str(&n.to_string())
            .or_else(|_| Decimal::from_scientific(&n.to_string()))
            .ok()
            .map(Money::new),
        _ => None,
    }
}

fn link_state(answer: &Value) -> LinkState {
    let sessions = answer["link_sessions"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    // A token can carry more than one session if the page was reopened; the
    // one that added something wins, then any that finished.
    for session in &sessions {
        let results = session["results"]["item_add_results"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        if let Some(added) = results.first() {
            let public_token = text(&added["public_token"]);
            if !public_token.is_empty() {
                return LinkState::Added {
                    public_token,
                    institution: text(&added["institution"]["name"]),
                };
            }
        }
    }
    for session in &sessions {
        if session["finished_at"].is_string() {
            let exit = &session["exit"];
            if exit.is_object() {
                let said = text(&exit["error"]["display_message"]);
                let said = if said.is_empty() {
                    text(&exit["error"]["error_message"])
                } else {
                    said
                };
                return LinkState::Exited(said);
            }
            return LinkState::Updated;
        }
    }
    LinkState::Waiting
}

fn accounts(answer: &Value) -> Vec<RemoteAccount> {
    answer["accounts"]
        .as_array()
        .map(|list| {
            list.iter()
                .map(|a| RemoteAccount {
                    id: text(&a["account_id"]),
                    name: text(&a["name"]),
                    mask: text(&a["mask"]),
                    kind: text(&a["type"]),
                    subtype: text(&a["subtype"]),
                    current: money(&a["balances"]["current"]),
                    available: money(&a["balances"]["available"]),
                    limit: money(&a["balances"]["limit"]),
                    currency: text(&a["balances"]["iso_currency_code"]),
                })
                .filter(|a| !a.id.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn transaction(t: &Value) -> Option<RemoteTransaction> {
    let id = text(&t["transaction_id"]);
    if id.is_empty() {
        return None;
    }
    Some(RemoteTransaction {
        id,
        account_id: text(&t["account_id"]),
        date: text(&t["date"]),
        authorized: text(&t["authorized_date"]),
        name: text(&t["name"]),
        merchant: text(&t["merchant_name"]),
        category: text(&t["personal_finance_category"]["detailed"]),
        amount: money(&t["amount"])?,
        pending: t["pending"].as_bool().unwrap_or(false),
    })
}

fn sync_page(answer: &Value) -> SyncPage {
    let list = |k: &str| {
        answer[k]
            .as_array()
            .map(|l| l.iter().filter_map(transaction).collect())
            .unwrap_or_default()
    };
    SyncPage {
        added: list("added"),
        modified: list("modified"),
        removed: answer["removed"]
            .as_array()
            .map(|l| {
                l.iter()
                    .map(|r| text(&r["transaction_id"]))
                    .filter(|id| !id.is_empty())
                    .collect()
            })
            .unwrap_or_default(),
        next_cursor: text(&answer["next_cursor"]),
        has_more: answer["has_more"].as_bool().unwrap_or(false),
        // Absent on older answers, which only ever came once history was in.
        history_ready: !matches!(
            answer["transactions_update_status"].as_str(),
            Some("NOT_READY")
        ),
    }
}

/// `FOOD_AND_DRINK_GROCERIES` as a person would say it: "Groceries".
pub fn category_words(detailed: &str) -> String {
    const PRIMARIES: &[&str] = &[
        "BANK_FEES",
        "ENTERTAINMENT",
        "FOOD_AND_DRINK",
        "GENERAL_MERCHANDISE",
        "GENERAL_SERVICES",
        "GOVERNMENT_AND_NON_PROFIT",
        "HOME_IMPROVEMENT",
        "INCOME",
        "LOAN_PAYMENTS",
        "MEDICAL",
        "PERSONAL_CARE",
        "RENT_AND_UTILITIES",
        "TRANSFER_IN",
        "TRANSFER_OUT",
        "TRANSPORTATION",
        "TRAVEL",
    ];
    let rest = PRIMARIES
        .iter()
        .find_map(|p| detailed.strip_prefix(p).and_then(|r| r.strip_prefix('_')))
        .unwrap_or(detailed);
    let words = rest.replace('_', " ").to_lowercase();
    let mut chars = words.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal_macros::dec;

    #[test]
    fn a_finished_hosted_link_gives_its_public_token() {
        let waiting = json!({ "link_sessions": [] });
        assert_eq!(link_state(&waiting), LinkState::Waiting);

        let added = json!({ "link_sessions": [
            { "finished_at": "2026-10-06T20:00:00Z", "exit": null, "results": {
                "item_add_results": [{ "public_token": "public-sandbox-1",
                    "institution": { "name": "Chase", "institution_id": "ins_56" } }] } }
        ]});
        assert_eq!(
            link_state(&added),
            LinkState::Added {
                public_token: "public-sandbox-1".into(),
                institution: "Chase".into()
            }
        );

        let gave_up = json!({ "link_sessions": [
            { "finished_at": "2026-10-06T20:00:00Z",
              "exit": { "error": null, "status": "requires_credentials" } }
        ]});
        assert_eq!(link_state(&gave_up), LinkState::Exited(String::new()));

        let signed_in_again = json!({ "link_sessions": [
            { "finished_at": "2026-10-06T20:00:00Z", "exit": null, "results": {} }
        ]});
        assert_eq!(link_state(&signed_in_again), LinkState::Updated);
    }

    #[test]
    fn transactions_keep_their_cents_and_their_sign() {
        let page = sync_page(&json!({
            "added": [
                { "transaction_id": "t1", "account_id": "a1", "amount": 48.59,
                  "date": "2026-09-27", "authorized_date": "2026-09-26",
                  "name": "KROGER #920", "merchant_name": "Kroger", "pending": false,
                  "personal_finance_category": { "primary": "FOOD_AND_DRINK",
                      "detailed": "FOOD_AND_DRINK_GROCERIES" } },
                { "transaction_id": "t2", "account_id": "a1", "amount": -500,
                  "date": "2026-09-28", "authorized_date": null, "name": "Payment Thank You",
                  "merchant_name": null, "pending": false },
                { "account_id": "a1", "amount": 1 }
            ],
            "modified": [],
            "removed": [{ "transaction_id": "t0", "account_id": "a1" }],
            "next_cursor": "c2",
            "has_more": true,
            "transactions_update_status": "INITIAL_UPDATE_COMPLETE"
        }));
        assert_eq!(page.added.len(), 2, "one without an id is dropped");
        let kroger = &page.added[0];
        assert_eq!(kroger.amount, Money::new(dec!(48.59)));
        assert_eq!(kroger.merchant, "Kroger");
        assert_eq!(kroger.authorized, "2026-09-26");
        assert_eq!(page.added[1].amount, Money::new(dec!(-500)));
        assert!(page.added[1].merchant.is_empty());
        assert_eq!(page.removed, vec!["t0".to_string()]);
        assert!(page.has_more && page.history_ready);
        assert!(!sync_page(&json!({ "transactions_update_status": "NOT_READY" })).history_ready);
    }

    #[test]
    fn balances_are_read_as_given() {
        let list = accounts(&json!({ "accounts": [
            { "account_id": "a1", "name": "Sapphire", "mask": "4421", "type": "credit",
              "subtype": "credit card",
              "balances": { "current": 1234.5, "available": 8765.5, "limit": 10000,
                            "iso_currency_code": "USD" } },
            { "account_id": "a2", "name": "Checking", "type": "depository",
              "balances": { "current": 10.01, "available": null } }
        ]}));
        assert_eq!(list[0].current, Some(Money::new(dec!(1234.5))));
        assert_eq!(list[0].limit, Some(Money::new(dec!(10000))));
        assert_eq!(list[1].available, None);
        assert_eq!(list[1].mask, "");
    }

    #[test]
    fn categories_read_as_words() {
        assert_eq!(category_words("FOOD_AND_DRINK_GROCERIES"), "Groceries");
        assert_eq!(
            category_words("FOOD_AND_DRINK_BEER_WINE_AND_LIQUOR"),
            "Beer wine and liquor"
        );
        assert_eq!(category_words("TRAVEL_FLIGHTS"), "Flights");
        assert_eq!(category_words(""), "");
    }

    #[test]
    fn only_the_two_environments_are_known() {
        assert_eq!(base_for("sandbox"), Ok(SANDBOX));
        assert_eq!(base_for("production"), Ok(PRODUCTION));
        assert!(base_for("development").is_err());
    }
}
