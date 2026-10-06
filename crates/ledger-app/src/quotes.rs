//! Share prices from Finnhub, written back into the ledger as one op.
//! The key is the user's own and lives in the keychain, never in a file.

use crate::commands::{Answer, CommandError};
use crate::state::AppState;
use ledger_config::Secret;
use ledger_writer::{Op, Priced};
use rust_decimal::Decimal;
use serde::Serialize;
use std::str::FromStr;

/// The id the key is filed under. Not a store, but the keychain is keyed the
/// same way and one safe place is better than two.
pub const KEY_ID: &str = "finnhub-api-key";

const HOST: &str = "https://finnhub.io/api/v1";
const PER_REQUEST: std::time::Duration = std::time::Duration::from_secs(8);
/// The free tier is rate limited, so calls are spaced rather than fired at once.
const GAP: std::time::Duration = std::time::Duration::from_millis(200);
const MAX_SYMBOLS: usize = 200;
const WHOLE_SWEEP: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refreshed {
    pub attempted: usize,
    pub priced: usize,
    pub failed: usize,
    /// Distinct reasons, so one dead symbol does not print fifty times.
    pub errors: Vec<String>,
}

/// A Finnhub key is alphanumeric. Checked before it is sent, so a pasted line
/// of shell cannot travel as a header.
fn clean_key(raw: &str) -> Option<String> {
    let key = raw.trim().to_string();
    let usable = (8..=128).contains(&key.len())
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-'));
    usable.then_some(key)
}

pub async fn save_api_key(state: &AppState, key: String) -> Answer<bool> {
    let Some(key) = clean_key(&key) else {
        return Err(CommandError::Message(
            "that does not look like a Finnhub key".into(),
        ));
    };
    state.secrets.set(KEY_ID, &Secret::ApiKey { key })?;
    Ok(true)
}

pub async fn forget_api_key(state: &AppState) -> Answer<bool> {
    state.secrets.forget(KEY_ID)?;
    Ok(false)
}

/// Whether a key is stored. Never the key itself.
pub async fn has_api_key(state: &AppState) -> Answer<bool> {
    Ok(matches!(
        state.secrets.get(KEY_ID),
        Ok(Some(Secret::ApiKey { .. }))
    ))
}

async fn quote(client: &reqwest::Client, symbol: &str, key: &str) -> Result<Decimal, String> {
    let response = client
        .get(format!("{HOST}/quote?symbol={}", urlencode(symbol)))
        .header("X-Finnhub-Token", key)
        .timeout(PER_REQUEST)
        .send()
        .await
        .map_err(|e| format!("could not reach Finnhub: {e}"))?;

    match response.status().as_u16() {
        200 => {}
        401 | 403 => return Err("the API key was rejected".into()),
        429 => return Err("rate limited by Finnhub".into()),
        code => return Err(format!("Finnhub returned {code}")),
    }

    let body: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "the response was not readable JSON".to_string())?;
    let current = body["c"].as_f64();
    let previous = body["pc"].as_f64();
    let Some(current) = current else {
        return Err("no price in the response".into());
    };
    // Finnhub answers an unknown symbol with zeroes rather than an error, so
    // a flat zero here means "never heard of it", not "worth nothing".
    if current == 0.0 && previous.unwrap_or(0.0) == 0.0 {
        return Err("no data for that symbol".into());
    }
    Decimal::from_str(&current.to_string()).map_err(|_| "the price was not a number".into())
}

fn urlencode(raw: &str) -> String {
    raw.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '-' | '.' | '_' | '~') {
                c.to_string()
            } else {
                format!("%{:02X}", c as u32)
            }
        })
        .collect()
}

/// Price every holding that has a symbol and is not priced by hand.
pub async fn refresh_prices(state: &AppState) -> Answer<Refreshed> {
    let Ok(Some(Secret::ApiKey { key })) = state.secrets.get(KEY_ID) else {
        return Err(CommandError::Message(
            "no Finnhub key is stored; add one in Storage".into(),
        ));
    };

    let loaded = state.live().await.engine.load().await?;
    let doc = match &loaded.snapshot {
        Some(s) => ledger_domain::Ledger::from_bytes(&s.body)?,
        None => ledger_domain::Ledger::default(),
    };

    let targets: Vec<(String, String)> = doc
        .investments
        .iter()
        .filter(|h| !h.ticker.is_empty() && !h.fixed_price)
        .take(MAX_SYMBOLS)
        .map(|h| (h.id.clone(), h.ticker.clone()))
        .collect();

    if targets.is_empty() {
        return Ok(Refreshed {
            attempted: 0,
            priced: 0,
            failed: 0,
            errors: Vec::new(),
        });
    }

    let client = reqwest::Client::builder()
        .build()
        .map_err(|e| CommandError::Message(e.to_string()))?;
    let deadline = std::time::Instant::now() + WHOLE_SWEEP;
    let mut priced = Vec::new();
    let mut stale = Vec::new();
    let mut errors: Vec<String> = Vec::new();

    for (index, (id, symbol)) in targets.iter().enumerate() {
        if std::time::Instant::now() >= deadline {
            stale.push(id.clone());
            push_reason(&mut errors, "ran out of time");
            continue;
        }
        match quote(&client, symbol, &key).await {
            Ok(price) => priced.push(Priced {
                id: id.clone(),
                price: ledger_domain::Money::price(price),
            }),
            Err(why) => {
                stale.push(id.clone());
                push_reason(&mut errors, &why);
            }
        }
        if index + 1 < targets.len() {
            tokio::time::sleep(GAP).await;
        }
    }

    let found = priced.len();
    let missed = stale.len();
    // One op for the sweep, through the ordinary edit path: it re-reads the
    // primary first, so an edit made while this was on the network survives.
    crate::commands::apply_value(
        state,
        serde_json::to_value(Op::PriceHoldings { priced, stale })?,
    )
    .await?;

    Ok(Refreshed {
        attempted: targets.len(),
        priced: found,
        failed: missed,
        errors,
    })
}

fn push_reason(errors: &mut Vec<String>, why: &str) {
    if errors.len() < 5 && !errors.iter().any(|e| e == why) {
        errors.push(why.to_string());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_that_is_not_one_is_refused_before_it_becomes_a_header() {
        assert_eq!(
            clean_key("  abc123def456  ").as_deref(),
            Some("abc123def456")
        );
        assert_eq!(clean_key("with space in it").as_deref(), None);
        assert_eq!(clean_key("short").as_deref(), None);
        assert_eq!(clean_key("has\nnewline1234").as_deref(), None);
        assert_eq!(clean_key(&"x".repeat(129)).as_deref(), None);
    }

    #[test]
    fn one_reason_is_reported_once_however_many_symbols_hit_it() {
        let mut errors = Vec::new();
        for _ in 0..40 {
            push_reason(&mut errors, "rate limited by Finnhub");
        }
        push_reason(&mut errors, "no data for that symbol");
        assert_eq!(errors.len(), 2);
    }
}
