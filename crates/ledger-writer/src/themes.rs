//! Saving and removing colour themes, which are kept in the ledger.

use crate::{WriteError, Writer};
use ledger_domain::Ledger;
use ledger_domain::records::AuditEntry;
use ledger_domain::text::new_id;
use ledger_domain::themes::{MAX_THEMES, THEMES_KEY, Theme, clean_theme};
use serde_json::Value;

fn store(ledger: &mut Ledger, themes: &[Theme]) -> Result<(), WriteError> {
    if themes.is_empty() {
        ledger.unknown.remove(THEMES_KEY);
    } else {
        let value = serde_json::to_value(themes).map_err(|e| WriteError::Refused(e.to_string()))?;
        ledger.unknown.insert(THEMES_KEY.into(), value);
    }
    Ok(())
}

impl Writer {
    /// Adds a theme, or replaces the one with the same id. A theme sent
    /// without an id, or with one no stored theme has, is added as new.
    pub(crate) fn theme_set(
        &self,
        ledger: &mut Ledger,
        raw: &Value,
    ) -> Result<AuditEntry, WriteError> {
        let Some(theme) = clean_theme(raw, new_id) else {
            return Err(WriteError::Refused("a theme needs a name".into()));
        };
        let mut themes = ledger.themes();
        let action = match themes.iter().position(|t| t.id == theme.id) {
            Some(at) => {
                if themes[at] == theme {
                    return Err(WriteError::Unchanged("that theme is already saved".into()));
                }
                themes[at] = theme.clone();
                "Update"
            }
            None => {
                if themes.len() >= MAX_THEMES {
                    return Err(WriteError::Refused(format!(
                        "a ledger keeps at most {MAX_THEMES} themes"
                    )));
                }
                themes.push(theme.clone());
                "Create"
            }
        };
        store(ledger, &themes)?;
        Ok(self.finish(
            AuditEntry {
                action: action.into(),
                subject: "theme".into(),
                name: theme.name,
                ..Default::default()
            },
            "theme-set",
        ))
    }

    pub(crate) fn theme_delete(
        &self,
        ledger: &mut Ledger,
        id: &str,
    ) -> Result<AuditEntry, WriteError> {
        let mut themes = ledger.themes();
        let Some(at) = themes.iter().position(|t| t.id == id) else {
            return Err(WriteError::Missing("that theme is not there".into()));
        };
        let gone = themes.remove(at);
        store(ledger, &themes)?;
        Ok(self.finish(
            AuditEntry {
                action: "Delete".into(),
                subject: "theme".into(),
                name: gone.name,
                ..Default::default()
            },
            "theme-delete",
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::{Op, WriteError, Writer};
    use ledger_domain::Ledger;
    use serde_json::json;

    fn apply(
        ledger: &mut Ledger,
        op: serde_json::Value,
    ) -> Result<ledger_domain::records::AuditEntry, WriteError> {
        let op: Op = serde_json::from_value(op).unwrap();
        Writer::new("test").apply(ledger, &op)
    }

    #[test]
    fn a_theme_is_added_updated_and_removed() {
        let mut ledger = Ledger::default();
        let made = apply(
            &mut ledger,
            json!({ "op": "theme-set", "theme": { "name": "Dusk", "dark": { "accent": "#ff8800" } } }),
        )
        .unwrap();
        assert_eq!(
            (made.action.as_str(), made.name.as_str()),
            ("Create", "Dusk")
        );
        let id = ledger.themes()[0].id.clone();
        assert_eq!(id.len(), 32);

        let again = json!({ "op": "theme-set", "theme": { "id": id, "name": "Dusk", "dark": { "accent": "#ff8800" } } });
        assert!(matches!(
            apply(&mut ledger, again),
            Err(WriteError::Unchanged(_))
        ));

        let changed = apply(
            &mut ledger,
            json!({ "op": "theme-set", "theme": { "id": id, "name": "Dusk", "dark": { "accent": "#00ff88" } } }),
        )
        .unwrap();
        assert_eq!(changed.action, "Update");
        assert_eq!(ledger.themes().len(), 1);
        assert_eq!(ledger.themes()[0].dark["accent"], "#00ff88");

        apply(&mut ledger, json!({ "op": "theme-delete", "id": id })).unwrap();
        assert!(ledger.themes().is_empty());
        // The last one gone leaves no key behind.
        assert!(!ledger.unknown.contains_key("themes"));
    }

    #[test]
    fn a_nameless_theme_is_refused_and_changes_nothing() {
        let mut ledger = Ledger::default();
        let refused = apply(
            &mut ledger,
            json!({ "op": "theme-set", "theme": { "name": "" } }),
        );
        assert!(matches!(refused, Err(WriteError::Refused(_))));
        assert!(!ledger.unknown.contains_key("themes"));
        let missing = apply(
            &mut ledger,
            json!({ "op": "theme-delete", "id": "0123456789abcdef0123456789abcdef" }),
        );
        assert!(matches!(missing, Err(WriteError::Missing(_))));
    }
}
