//! Colour themes people make for themselves. They are kept in the ledger, so a
//! theme made on one machine is offered on every other; which one is worn is
//! still chosen per device.
//!
//! A theme is only colours: a light set and a dark set, each a map from a
//! token name to a hex colour. It is laid over whichever style is chosen, and
//! a token it leaves out keeps that style's own colour, so a theme can change
//! just the accent. Kept in the document's `unknown` map and read through
//! [`clean_themes`], which never refuses: a bad entry is dropped, a token name
//! this build does not know is kept so a newer client's theme survives.

use crate::document::Ledger;
use crate::text::{plain, valid_id};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// The key the themes are stored under.
pub const THEMES_KEY: &str = "themes";
pub const MAX_THEMES: usize = 50;
pub const MAX_TOKENS: usize = 64;
pub const THEME_NAME_MAX: usize = 40;
const TOKEN_NAME_MAX: usize = 32;

/// Token name to colour, for one mode.
pub type Palette = BTreeMap<String, String>;

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Theme {
    pub id: String,
    pub name: String,
    pub light: Palette,
    pub dark: Palette,
}

/// `#rgb`, `#rrggbb` or `#rrggbbaa`, lowercased; anything else is nothing.
/// Only a hex colour is allowed, because a value is written into a style
/// property and must not be able to carry anything else.
pub fn clean_color(value: &str) -> Option<String> {
    let value = value.trim();
    let hex = value.strip_prefix('#')?;
    let ok = matches!(hex.len(), 3 | 6 | 8) && hex.chars().all(|c| c.is_ascii_hexdigit());
    ok.then(|| value.to_ascii_lowercase())
}

/// A camelCase word: a lowercase letter, then letters and digits.
fn clean_token(name: &str) -> Option<String> {
    let ok = !name.is_empty()
        && name.len() <= TOKEN_NAME_MAX
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name.chars().all(|c| c.is_ascii_alphanumeric());
    ok.then(|| name.to_string())
}

pub fn clean_palette(raw: Option<&Value>) -> Palette {
    let mut out = Palette::new();
    let Some(map) = raw.and_then(Value::as_object) else {
        return out;
    };
    for (name, value) in map {
        if out.len() >= MAX_TOKENS {
            break;
        }
        if let (Some(name), Some(color)) = (clean_token(name), value.as_str().and_then(clean_color))
        {
            out.insert(name, color);
        }
    }
    out
}

/// One theme, or nothing when it has no name. An id that is not a real one is
/// replaced through `make_id`, so a theme pasted from a file gets its own.
pub fn clean_theme(raw: &Value, make_id: impl FnOnce() -> String) -> Option<Theme> {
    let obj = raw.as_object()?;
    let name = plain(
        obj.get("name").and_then(Value::as_str).unwrap_or(""),
        THEME_NAME_MAX,
    );
    if name.is_empty() {
        return None;
    }
    let id = valid_id(obj.get("id").and_then(Value::as_str).unwrap_or(""));
    Some(Theme {
        id: if id.is_empty() { make_id() } else { id },
        name,
        light: clean_palette(obj.get("light")),
        dark: clean_palette(obj.get("dark")),
    })
}

/// Every stored theme that reads, once per id, in the order stored.
pub fn clean_themes(raw: Option<&Value>) -> Vec<Theme> {
    let mut out: Vec<Theme> = Vec::new();
    for item in raw.and_then(Value::as_array).into_iter().flatten() {
        if out.len() >= MAX_THEMES {
            break;
        }
        // A stored theme without a real id cannot be edited or removed, so it
        // is dropped rather than given one that changes on every read.
        if let Some(theme) = clean_theme(item, String::new)
            && !theme.id.is_empty()
            && !out.iter().any(|t| t.id == theme.id)
        {
            out.push(theme);
        }
    }
    out
}

impl Ledger {
    pub fn themes(&self) -> Vec<Theme> {
        clean_themes(self.unknown.get(THEMES_KEY))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const ID: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn only_a_hex_colour_is_a_colour() {
        assert_eq!(clean_color("#ABC"), Some("#abc".into()));
        assert_eq!(clean_color(" #112233 "), Some("#112233".into()));
        assert_eq!(clean_color("#11223344"), Some("#11223344".into()));
        for bad in [
            "red",
            "#12",
            "#12345",
            "#gggggg",
            "112233",
            "#112233;}",
            "url(x)",
        ] {
            assert_eq!(clean_color(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_theme_keeps_good_tokens_and_drops_the_rest() {
        let theme = clean_theme(
            &json!({ "id": ID, "name": "  Sea\nside ", "light": {
                "accent": "#0077FF", "pillBlue": "#123", "Bad": "#fff",
                "has-dash": "#fff", "text": "black", "futureToken": "#abcdef"
            }, "dark": "nonsense" }),
            || unreachable!(),
        )
        .unwrap();
        assert_eq!(theme.name, "Sea side");
        assert_eq!(
            theme.light.keys().collect::<Vec<_>>(),
            ["accent", "futureToken", "pillBlue"]
        );
        assert_eq!(theme.light["accent"], "#0077ff");
        assert!(theme.dark.is_empty());
    }

    #[test]
    fn a_theme_without_a_name_is_nothing_and_a_bad_id_is_replaced() {
        assert!(clean_theme(&json!({ "name": "  " }), || "x".into()).is_none());
        let theme = clean_theme(&json!({ "id": "../x", "name": "A" }), || "made".into()).unwrap();
        assert_eq!(theme.id, "made");
    }

    #[test]
    fn stored_themes_are_read_once_each_and_never_refused() {
        let mut ledger = Ledger::default();
        assert!(ledger.themes().is_empty());
        ledger.unknown.insert(
            THEMES_KEY.into(),
            json!([
                { "id": ID, "name": "One" },
                { "id": ID, "name": "Again" },
                { "id": "bad", "name": "No id" },
                7,
            ]),
        );
        let themes = ledger.themes();
        assert_eq!(themes.len(), 1);
        assert_eq!(themes[0].name, "One");
        ledger
            .unknown
            .insert(THEMES_KEY.into(), json!("not a list"));
        assert!(ledger.themes().is_empty());
    }
}
