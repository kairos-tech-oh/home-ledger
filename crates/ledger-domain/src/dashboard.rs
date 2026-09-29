//! The landing page's layout: an ordered list of widgets. It is stored in the
//! ledger, so every machine shows the same dashboard.
//!
//! Kept in the document's `unknown` map rather than as a typed field, so a
//! read and a write give back exactly what was stored. It is read through
//! [`clean_dashboard`], ported from the helper's `clean_dashboard`: that never
//! refuses, because one bad widget must not make the ledger unreadable.
//! Bad entries are dropped instead, and a widget kind this build does not know
//! is kept, so a newer client's widget survives a write from an older one.

use crate::document::Ledger;
use crate::text::{plain, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const MAX_WIDGETS: usize = 40;
pub const MAX_WIDGET_REFS: usize = 50;
const TITLE_MAX: usize = 60;

pub const SPENDING_PERIODS: [&str; 6] = ["1m", "3m", "6m", "1y", "all", "custom"];
pub const SPENDING_STATUSES: [&str; 3] = ["all", "settled", "open"];
pub const SPENDING_STATS: [&str; 9] = [
    "total",
    "count",
    "open",
    "settled",
    "withdrawn",
    "fromBuckets",
    "everyday",
    "average",
    "unattributed",
];
pub const SPENDING_BREAKDOWNS: [&str; 6] = [
    "people",
    "items",
    "months",
    "cards",
    "bucketSpending",
    "withdrawals",
];
pub const SPENDING_LIMITS: [u32; 3] = [3, 5, 10];

/// What a spending widget shows and over which dates.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpendingOptions {
    pub period: String,
    pub status: String,
    /// `yyyy-mm-dd` for a custom period, empty otherwise.
    pub from: String,
    pub to: String,
    /// In the order the plugin lists them, not the order chosen.
    pub stats: Vec<String>,
    pub breakdowns: Vec<String>,
    /// Rows shown per breakdown.
    pub limit: u32,
}

impl Default for SpendingOptions {
    fn default() -> Self {
        clean_spending_options(None)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Widget {
    pub id: String,
    pub kind: String,
    /// Empty means the kind's own name.
    pub title: String,
    /// 1 is half a row, 2 is the whole row.
    pub span: u8,
    /// The accounts, buckets or goals it was pointed at, in order.
    pub refs: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub options: Option<SpendingOptions>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dashboard {
    pub v: u32,
    pub widgets: Vec<Widget>,
}

fn valid_date(text: &str) -> bool {
    let b = text.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let (Ok(y), Ok(m), Ok(d)) = (
        text[0..4].parse::<u32>(),
        text[5..7].parse::<u32>(),
        text[8..10].parse::<u32>(),
    ) else {
        return false;
    };
    let leap = (y % 4 == 0 && y % 100 != 0) || y % 400 == 0;
    let last = match m {
        2 if leap => 29,
        2 => 28,
        4 | 6 | 9 | 11 => 30,
        1..=12 => 31,
        _ => return false,
    };
    (1..=last).contains(&d)
}

fn choice(value: Option<&Value>, allowed: &[&str], fallback: &str) -> String {
    match value.and_then(Value::as_str) {
        Some(v) if allowed.contains(&v) => v.to_string(),
        _ => fallback.to_string(),
    }
}

/// The allowed keys that were asked for, in the allowed order. Not a list at
/// all means the defaults; an empty list means none.
fn keys(value: Option<&Value>, allowed: &[&str], fallback: &[&str]) -> Vec<String> {
    let Some(list) = value.and_then(Value::as_array) else {
        return fallback.iter().map(|s| s.to_string()).collect();
    };
    allowed
        .iter()
        .filter(|key| list.iter().any(|v| v.as_str() == Some(key)))
        .map(|s| s.to_string())
        .collect()
}

pub fn clean_spending_options(raw: Option<&Value>) -> SpendingOptions {
    let o = raw.and_then(Value::as_object);
    let get = |key: &str| o.and_then(|o| o.get(key));
    let date = |key: &str| match get(key).and_then(Value::as_str) {
        Some(d) if valid_date(d) => d.to_string(),
        _ => String::new(),
    };
    // Only a whole number, as the helper's `type(limit) is int` has it.
    let limit = get("limit")
        .and_then(Value::as_u64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|n| SPENDING_LIMITS.contains(n))
        .unwrap_or(5);
    SpendingOptions {
        period: choice(get("period"), &SPENDING_PERIODS, "1m"),
        status: choice(get("status"), &SPENDING_STATUSES, "all"),
        from: date("from"),
        to: date("to"),
        stats: keys(
            get("stats"),
            &SPENDING_STATS,
            &["total", "fromBuckets", "everyday", "unattributed"],
        ),
        breakdowns: keys(
            get("breakdowns"),
            &SPENDING_BREAKDOWNS,
            &["people", "items"],
        ),
        limit,
    }
}

fn kind_ok(kind: &str) -> bool {
    let mut chars = kind.chars();
    kind.len() <= 24
        && chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c == '-')
}

/// One widget, or None when it is not one. A widget with no usable id gets
/// the one `fallback_id` gives it.
pub fn clean_widget(raw: &Value, fallback_id: impl FnOnce() -> String) -> Option<Widget> {
    let o = raw.as_object()?;
    let kind = o.get("kind").and_then(Value::as_str).unwrap_or("");
    if !kind_ok(kind) {
        return None;
    }
    let mut refs: Vec<String> = Vec::new();
    for item in o
        .get("refs")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_WIDGET_REFS)
    {
        let id = valid_id(item.as_str().unwrap_or(""));
        if !id.is_empty() && !refs.contains(&id) {
            refs.push(id);
        }
    }
    let id = valid_id(o.get("id").and_then(Value::as_str).unwrap_or(""));
    Some(Widget {
        id: if id.is_empty() { fallback_id() } else { id },
        kind: kind.to_string(),
        title: plain(
            o.get("title").and_then(Value::as_str).unwrap_or(""),
            TITLE_MAX,
        ),
        span: if o.get("span").and_then(Value::as_u64) == Some(2) {
            2
        } else {
            1
        },
        refs,
        options: (kind == "spending").then(|| clean_spending_options(o.get("options"))),
    })
}

/// The whole layout, or None when what is stored is not one at all.
///
/// `fallback_id` names a widget stored without a usable id. The writer passes
/// a fresh id; a read passes one fixed by position, so the same stored layout
/// reads back with the same ids every time and an edit aimed at a widget
/// finds it.
pub fn clean_dashboard(
    raw: &Value,
    mut fallback_id: impl FnMut(usize) -> String,
) -> Option<Dashboard> {
    let o = raw.as_object()?;
    let mut widgets: Vec<Widget> = Vec::new();
    for (index, item) in o
        .get("widgets")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_WIDGETS)
        .enumerate()
    {
        if let Some(widget) = clean_widget(item, || fallback_id(index))
            && !widgets.iter().any(|w| w.id == widget.id)
        {
            widgets.push(widget);
        }
    }
    Some(Dashboard { v: 1, widgets })
}

/// A fixed 32-hex id for the widget at a position. Not random, for the reason
/// given on [`clean_dashboard`].
pub fn positional_id(index: usize) -> String {
    format!("{:032x}", index + 1)
}

impl Ledger {
    /// The stored layout, cleaned. None when nothing has been saved yet, which
    /// is when the default applies.
    pub fn dashboard(&self) -> Option<Dashboard> {
        self.unknown
            .get("dashboard")
            .and_then(|raw| clean_dashboard(raw, positional_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

    fn fixed(_: usize) -> String {
        "f".repeat(32)
    }

    #[test]
    fn spending_options_default_when_absent() {
        // check-spending.mjs: spendingWidgetOptions(null).
        let o = clean_spending_options(None);
        assert_eq!(o.period, "1m");
        assert_eq!(o.status, "all");
        assert_eq!((o.from.as_str(), o.to.as_str()), ("", ""));
        assert_eq!(
            o.stats,
            ["total", "fromBuckets", "everyday", "unattributed"]
        );
        assert_eq!(o.breakdowns, ["people", "items"]);
        assert_eq!(o.limit, 5);
    }

    #[test]
    fn spending_options_keep_only_what_is_allowed_in_the_allowed_order() {
        // check-spending.mjs: the "bogus" case, answer for answer.
        let o = clean_spending_options(Some(&json!({
            "period": "bogus", "status": "open", "from": "2026-02-30", "to": "2026-09-01",
            "stats": ["average", "nope", "total", "average"], "breakdowns": [], "limit": 7
        })));
        assert_eq!(o.period, "1m");
        assert_eq!(o.status, "open");
        assert_eq!(o.from, "");
        assert_eq!(o.to, "2026-09-01");
        assert_eq!(o.stats, ["total", "average"]);
        assert!(o.breakdowns.is_empty());
        assert_eq!(o.limit, 5);
    }

    #[test]
    fn a_limit_must_be_a_whole_number_on_the_list() {
        for (raw, want) in [
            (json!(10), 10),
            (json!(3), 3),
            (json!(3.5), 5),
            (json!("10"), 5),
        ] {
            let o = clean_spending_options(Some(&json!({ "limit": raw })));
            assert_eq!(o.limit, want);
        }
    }

    #[test]
    fn a_widget_is_cleaned_and_a_bad_one_dropped() {
        let d = clean_dashboard(
            &json!({ "v": 7, "widgets": [
                { "id": A, "kind": "buckets", "title": "  Mine\n", "span": 2,
                  "refs": [B, B, "nope", A] },
                { "id": B, "kind": "Not A Kind" },
                "not even an object",
                { "id": A, "kind": "goals" },
                { "kind": "spending", "options": { "period": "all" } },
            ]}),
            fixed,
        )
        .unwrap();
        assert_eq!(d.v, 1);
        assert_eq!(d.widgets.len(), 2, "{d:?}");
        let first = &d.widgets[0];
        assert_eq!((first.title.as_str(), first.span), ("Mine", 2));
        assert_eq!(first.refs, [B, A]);
        assert_eq!(first.options, None);
        let spending = &d.widgets[1];
        assert_eq!(spending.id, fixed(0));
        assert_eq!(spending.options.as_ref().unwrap().period, "all");
    }

    #[test]
    fn a_kind_this_build_does_not_know_is_kept() {
        let d = clean_dashboard(
            &json!({ "widgets": [{ "id": A, "kind": "from-the-future" }] }),
            fixed,
        )
        .unwrap();
        assert_eq!(d.widgets[0].kind, "from-the-future");
    }

    #[test]
    fn caps_hold() {
        let widgets: Vec<Value> = (0..60)
            .map(|i| json!({ "id": format!("{:032x}", i + 1), "kind": "networth" }))
            .collect();
        let d = clean_dashboard(&json!({ "widgets": widgets }), fixed).unwrap();
        assert_eq!(d.widgets.len(), MAX_WIDGETS);
    }

    #[test]
    fn not_an_object_is_no_dashboard() {
        assert_eq!(clean_dashboard(&json!([1, 2]), fixed), None);
        assert_eq!(clean_dashboard(&json!("x"), fixed), None);
    }

    #[test]
    fn a_read_gives_the_same_ids_every_time() {
        let mut ledger = Ledger::default();
        ledger.unknown.insert(
            "dashboard".into(),
            json!({ "widgets": [{ "kind": "networth" }] }),
        );
        assert_eq!(ledger.dashboard(), ledger.dashboard());
        assert_eq!(ledger.dashboard().unwrap().widgets[0].id, positional_id(0));
    }
}
