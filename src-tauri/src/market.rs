//! Price history from Yahoo's chart endpoint: free, no key, unofficial.
//! A failure here is reported, never fatal — the ledger's figures stand alone.

use ledger_app::AppState;
use ledger_app::{Answer, CommandError};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tauri::State;

const HOST: &str = "https://query1.finance.yahoo.com/v8/finance/chart";
const MAX_BODY: usize = 512 * 1024;
const MAX_POINTS: usize = 2_000;
const TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

/// Range name to Yahoo's range, its interval, and how long a copy stays fresh.
const RANGES: [(&str, &str, &str, u64); 4] = [
    ("5d", "5d", "30m", 10 * 60),
    ("1mo", "1mo", "1d", 30 * 60),
    ("1y", "1y", "1d", 30 * 60),
    ("5y", "5y", "1wk", 12 * 3600),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Chart {
    /// Epoch second, then close, high and low. Numbers, because the chart is
    /// drawn from them; every figure shown as text is formatted below.
    pub points: Vec<[f64; 4]>,
    pub price: Option<f64>,
    pub day_high: Option<f64>,
    pub day_low: Option<f64>,
    pub year_high: Option<f64>,
    pub year_low: Option<f64>,
    pub currency: String,
    pub exchange: String,
    pub name: String,
    pub instrument: String,
    pub fetched_at: u64,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Cached {
    ticker: String,
    charts: std::collections::BTreeMap<String, Chart>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Detail {
    pub ticker: String,
    pub range: String,
    pub chart: Option<Chart>,
    /// Said plainly rather than thrown: a chart that will not load is a gap in
    /// the page, not a failed command.
    pub error: String,
    pub from_cache: bool,
}

/// Uppercase letters, digits and the separators real symbols use. Refused
/// rather than truncated when over-long: a cut symbol is a different company.
fn clean_ticker(raw: &str) -> Option<String> {
    let text = raw.trim().to_uppercase();
    if text.is_empty() || text.len() > 16 {
        return None;
    }
    let mut chars = text.chars();
    let first = chars.next()?;
    if !first.is_ascii_alphanumeric() {
        return None;
    }
    chars
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | ':' | '-'))
        .then_some(text)
}

/// Yahoo writes share classes with a dash where the ledger uses a dot.
fn yahoo_symbol(ticker: &str) -> String {
    ticker.replace('.', "-")
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn number(value: &Value) -> Option<f64> {
    let found = value.as_f64()?;
    found.is_finite().then_some(found)
}

fn text(value: Option<&Value>, limit: usize) -> String {
    value
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .filter(|c| !c.is_control())
        .take(limit)
        .collect()
}

fn parse(body: &str) -> Result<Chart, String> {
    let root: Value = serde_json::from_str(body).map_err(|_| "chart was not readable JSON")?;
    let chart = &root["chart"];
    let Some(result) = chart["result"].get(0) else {
        let said = text(chart["error"].get("description"), 120);
        return Err(if said.is_empty() {
            "no chart for that symbol".into()
        } else {
            said
        });
    };

    let meta = &result["meta"];
    let stamps = result["timestamp"].as_array().cloned().unwrap_or_default();
    let quote = &result["indicators"]["quote"][0];
    let empty = Vec::new();
    let closes = quote["close"].as_array().unwrap_or(&empty);
    let highs = quote["high"].as_array().unwrap_or(&empty);
    let lows = quote["low"].as_array().unwrap_or(&empty);

    let mut points = Vec::new();
    for (i, stamp) in stamps.iter().enumerate() {
        let (Some(at), Some(close)) = (stamp.as_i64(), closes.get(i).and_then(number)) else {
            continue;
        };
        let high = highs.get(i).and_then(number).unwrap_or(close);
        let low = lows.get(i).and_then(number).unwrap_or(close);
        points.push([at as f64, close, high, low]);
    }
    if points.len() > MAX_POINTS {
        points.drain(..points.len() - MAX_POINTS);
    }
    if points.is_empty() {
        return Err("the chart came back with no prices".into());
    }

    Ok(Chart {
        points,
        price: number(&meta["regularMarketPrice"]),
        day_high: number(&meta["regularMarketDayHigh"]),
        day_low: number(&meta["regularMarketDayLow"]),
        year_high: number(&meta["fiftyTwoWeekHigh"]),
        year_low: number(&meta["fiftyTwoWeekLow"]),
        currency: text(meta.get("currency"), 8),
        exchange: text(
            meta.get("fullExchangeName").or(meta.get("exchangeName")),
            40,
        ),
        name: text(meta.get("longName").or(meta.get("shortName")), 120),
        instrument: text(meta.get("instrumentType"), 20),
        fetched_at: now(),
    })
}

async fn fetch(symbol: &str, range: &str, interval: &str) -> Result<Chart, String> {
    let url = format!(
        "{HOST}/{}?range={range}&interval={interval}",
        urlencode(&yahoo_symbol(symbol))
    );
    let response = reqwest::Client::builder()
        .timeout(TIMEOUT)
        .build()
        .map_err(|e| e.to_string())?
        .get(url)
        .header("User-Agent", "Mozilla/5.0")
        .send()
        .await
        .map_err(|e| format!("could not reach Yahoo: {e}"))?;

    match response.status().as_u16() {
        200 => {}
        404 => return Err("no chart for that symbol".into()),
        429 => return Err("rate limited by Yahoo; try again shortly".into()),
        code => return Err(format!("Yahoo returned {code}")),
    }

    let body = response
        .bytes()
        .await
        .map_err(|e| format!("the chart did not finish downloading: {e}"))?;
    if body.len() > MAX_BODY {
        return Err("the chart was larger than this will read".into());
    }
    parse(&String::from_utf8_lossy(&body))
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

fn cache_path(data_dir: &Path, ticker: &str) -> PathBuf {
    data_dir
        .join("detail")
        .join(format!("{}.json", ticker.replace(':', "_")))
}

async fn read_cache(path: &Path, ticker: &str) -> Cached {
    let Ok(raw) = tokio::fs::read(path).await else {
        return Cached::default();
    };
    match serde_json::from_slice::<Cached>(&raw) {
        Ok(held) if held.ticker == ticker => held,
        _ => Cached::default(),
    }
}

async fn write_cache(path: &Path, held: &Cached) {
    if let Some(dir) = path.parent() {
        let _ = tokio::fs::create_dir_all(dir).await;
    }
    if let Ok(body) = serde_json::to_vec(held) {
        let _ = tokio::fs::write(path, body).await;
    }
}

/// One ticker's price history. A stale copy is served when the fetch fails, so
/// losing the network shows yesterday's chart rather than an empty panel.
#[tauri::command]
pub async fn holding_detail(
    state: State<'_, AppState>,
    ticker: String,
    range: String,
    force: bool,
) -> Answer<Detail> {
    let Some(symbol) = clean_ticker(&ticker) else {
        return Err(CommandError::Message(
            "that is not a symbol anything can be looked up by".into(),
        ));
    };
    let (name, yahoo_range, interval, ttl) = RANGES
        .iter()
        .find(|(name, ..)| *name == range)
        .copied()
        .unwrap_or(RANGES[2]);

    let path = cache_path(&state.places.data_dir, &symbol);
    let mut held = read_cache(&path, &symbol).await;
    let fresh = held
        .charts
        .get(name)
        .is_some_and(|c| now().saturating_sub(c.fetched_at) < ttl);

    if fresh && !force {
        return Ok(Detail {
            ticker: symbol,
            range: name.into(),
            chart: held.charts.get(name).cloned(),
            error: String::new(),
            from_cache: true,
        });
    }

    match fetch(&symbol, yahoo_range, interval).await {
        Ok(chart) => {
            held.ticker = symbol.clone();
            held.charts.insert(name.into(), chart.clone());
            write_cache(&path, &held).await;
            Ok(Detail {
                ticker: symbol,
                range: name.into(),
                chart: Some(chart),
                error: String::new(),
                from_cache: false,
            })
        }
        Err(why) => Ok(Detail {
            ticker: symbol,
            range: name.into(),
            chart: held.charts.get(name).cloned(),
            error: why,
            from_cache: true,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_symbol_that_is_not_one_is_refused_rather_than_looked_up() {
        assert_eq!(clean_ticker("voo").as_deref(), Some("VOO"));
        assert_eq!(clean_ticker(" brk.b ").as_deref(), Some("BRK.B"));
        assert_eq!(clean_ticker("").as_deref(), None);
        assert_eq!(clean_ticker("Large Cap Growth").as_deref(), None);
        assert_eq!(clean_ticker(".hidden").as_deref(), None);
        assert_eq!(clean_ticker("ABCDEFGHIJKLMNOPQ").as_deref(), None);
    }

    #[test]
    fn a_cache_name_cannot_climb_out_of_its_directory() {
        assert!(clean_ticker("../../etc/passwd").is_none());
        let path = cache_path(Path::new("/data"), "EXCH:AAA");
        assert_eq!(path, Path::new("/data/detail/EXCH_AAA.json"));
    }

    #[test]
    fn a_share_class_is_written_the_way_yahoo_writes_it() {
        assert_eq!(yahoo_symbol("BRK.B"), "BRK-B");
        assert_eq!(urlencode("BRK-B"), "BRK-B");
        assert_eq!(urlencode("EXCH:AAA"), "EXCH%3AAAA");
    }

    #[test]
    fn a_chart_with_gaps_keeps_the_days_that_have_a_price() {
        let body = r#"{"chart":{"result":[{"meta":{"regularMarketPrice":10.5,
            "currency":"USD","longName":"Example"},
            "timestamp":[100,200,300],
            "indicators":{"quote":[{"close":[1.0,null,3.0],
            "high":[1.5,null,3.5],"low":[0.5,null,2.5]}]}}]}}"#;

        let chart = parse(body).expect("parses");
        assert_eq!(chart.points.len(), 2);
        assert_eq!(chart.points[0], [100.0, 1.0, 1.5, 0.5]);
        assert_eq!(chart.points[1], [300.0, 3.0, 3.5, 2.5]);
        assert_eq!(chart.price, Some(10.5));
        assert_eq!(chart.name, "Example");
    }

    #[test]
    fn a_refusal_from_yahoo_is_read_as_a_reason_not_a_chart() {
        let body = r#"{"chart":{"result":null,
            "error":{"code":"Not Found","description":"No data found, symbol may be delisted"}}}"#;
        assert_eq!(
            parse(body).unwrap_err(),
            "No data found, symbol may be delisted"
        );
    }
}
