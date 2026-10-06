//! Changing where the ledger lives.
//!
//! Each of these is a named operation rather than an edit to a config file,
//! because every one of them can move or replace a document. The rules they
//! share: nothing is saved that could not be built, and nothing that could
//! lose data happens without being asked for.

use crate::commands::{Answer, CommandError};
use crate::state::AppState;
use ledger_config::{Secret, StoreConfig};
use ledger_store::{Document, Expect, Relation, oauth};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Setup {
    pub stores: Vec<StoreConfig>,
    pub device: String,
    /// False means first-run setup has not been through, so the app offers it.
    pub setup_complete: bool,
    /// False means credentials cannot be kept on this machine. The UI must
    /// say so rather than letting someone believe a key was saved.
    pub keychain_available: bool,
    /// Everything wrong with the configuration, so a settings screen can show
    /// all of it at once.
    pub problems: Vec<String>,
    /// How opaque the window is, 0.30..1.0.
    pub opacity: f64,
    pub retirement_target_year: Option<i32>,
}

/// What is configured. Never includes a secret: there is no field for one.
pub async fn setup(state: &AppState) -> Answer<Setup> {
    let config = state.config().await;
    Ok(Setup {
        problems: config.problems(),
        stores: config.stores.clone(),
        device: config.device.clone(),
        setup_complete: config.setup_complete,
        keychain_available: state.keychain_available,
        opacity: config.window_opacity(),
        retirement_target_year: config.retirement_target_year,
    })
}

/// Check settings and credentials before committing to them, so a typo in a
/// bucket name is caught while the person is still looking at it.
pub async fn test_store(store: StoreConfig, secret: Option<Secret>) -> Answer<String> {
    // Built against a throwaway secret store, so a failed test leaves nothing
    // behind in the keychain.
    let scratch = ledger_config::secrets::InMemory::default();
    if let Some(secret) = &secret {
        ledger_config::Secrets::set(&scratch, &store.id, secret)?;
    }

    let built = ledger_config::build_store(&store, &scratch)?;
    match built.health().await {
        ledger_store::Health::Reachable => Ok("reachable".into()),
        ledger_store::Health::Denied(why) | ledger_store::Health::Unreachable(why) => {
            Err(CommandError::Message(why))
        }
    }
}

/// Add a store, or replace the settings of one already configured.
///
/// A new store joins as a backup. Making it the source of truth is
/// [`promote_store`], which is a separate decision because it moves data.
pub async fn save_store(
    state: &AppState,
    store: StoreConfig,
    secret: Option<Secret>,
) -> Answer<Setup> {
    if let Some(problem) = store.settings.problem() {
        return Err(CommandError::Message(problem));
    }

    // The secret goes in first: building the store needs it, and a store that
    // cannot be built must not reach the configuration.
    if let Some(secret) = &secret {
        state.secrets.set(&store.id, secret)?;
    }

    let mut config = state.config().await;
    match config.stores.iter_mut().find(|s| s.id == store.id) {
        Some(existing) => *existing = store,
        None => config.stores.push(store),
    }

    state.reconfigure(config).await?;
    setup(state).await
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PromoteReport {
    /// What the candidate was relative to the current source of truth.
    pub was: String,
    /// Whether the candidate had to be brought up to date first.
    pub fast_forwarded: bool,
    pub setup: Setup,
}

/// Make a backup the source of truth.
///
/// Checked rather than hoped. If the current source of truth can still be
/// read, the candidate is brought up to date before the roles swap, so
/// promotion never loses what the old one held. If the two have genuinely
/// diverged, or the old one cannot be read at all, this refuses unless the
/// caller says to go ahead anyway.
pub async fn promote_store(state: &AppState, id: String, force: bool) -> Answer<PromoteReport> {
    let config = state.config().await;

    let Some(candidate) = config.find(&id).cloned() else {
        return Err(CommandError::Message(format!("no store called {id}")));
    };
    let Some(current) = config.source_of_truth().cloned() else {
        return Err(CommandError::Message("nothing is configured".into()));
    };
    if current.id == candidate.id {
        return Err(CommandError::Message(format!(
            "{} is already the source of truth",
            candidate.label
        )));
    }

    let truth = ledger_config::build_store(&current, state.secrets.as_ref())?;
    let target = ledger_config::build_store(&candidate, state.secrets.as_ref())?;

    // Read both before deciding. A promotion is the one place two copies are
    // compared, and getting it wrong silently discards one of them.
    let held = truth.load().await;
    let theirs = target.load().await?;

    // How the source of truth looks from the promotion's point of view.
    let standing = match &held {
        Err(_) => Standing::Unreadable,
        Ok(None) => Standing::Empty,
        Ok(Some(ours)) => {
            let live = state.live().await;
            Standing::Related(match &theirs {
                Some(t) => live.writer.relation(&ours.body, &t.body),
                // Nothing on the candidate yet, so it is simply behind.
                None => Relation::Ahead,
            })
        }
    };

    let (was, fast_forwarded) = match decide(standing, force) {
        Outcome::Blocked(why) => {
            let detail = match &held {
                Err(e) => e.to_string(),
                _ => String::new(),
            };
            return Err(CommandError::Message(why.explain(
                &current.label,
                &candidate.label,
                &detail,
            )));
        }
        Outcome::Proceed { was, fast_forward } => {
            if fast_forward && let Ok(Some(ours)) = &held {
                let expect = match &theirs {
                    Some(t) => Expect::Version(t.version.clone()),
                    None => Expect::Absent,
                };
                target.save(&ours.body, expect).await?;
            }
            (was.to_string(), fast_forward)
        }
    };

    let mut config = state.config().await;
    config
        .promote(&id)
        .map_err(|e| CommandError::Message(e.to_string()))?;
    state.reconfigure(config).await?;

    Ok(PromoteReport {
        was,
        fast_forwarded,
        setup: setup(state).await?,
    })
}

/// Forget a store. The data is left where it is — deleting from an account of
/// yours is not this app's decision to make.
pub async fn remove_store(state: &AppState, id: String) -> Answer<Setup> {
    let mut config = state.config().await;
    let gone = config
        .remove(&id)
        .map_err(|e| CommandError::Message(e.to_string()))?;

    state.reconfigure(config).await?;

    // Only once the configuration is safely replaced, so a failure above does
    // not leave a working store with no credentials.
    if gone.settings.needs_secret()
        && let Err(e) = state.secrets.forget(&gone.id)
    {
        tracing::warn!(store = %gone.label, error = %e, "credentials not removed");
    }

    setup(state).await
}

/// Rename this machine, as it appears in the audit log.
pub async fn rename_device(state: &AppState, name: String) -> Answer<Setup> {
    let Some(name) = crate::state::machine_name(&name) else {
        return Err(CommandError::Message("this machine needs a name".into()));
    };
    let mut config = state.config().await;
    config.device = name;
    state.reconfigure(config).await?;
    setup(state).await
}

pub async fn set_retirement_target_year(state: &AppState, year: Option<i32>) -> Answer<Setup> {
    let this_year = crate::clock::year_and_month().0;
    let kept = crate::state::target_year(year, this_year);
    if year.is_some() && kept.is_none() {
        return Err(CommandError::Message(format!(
            "a target year runs from {this_year} to {}",
            this_year + 70
        )));
    }
    state.set_retirement_target_year(kept).await?;
    setup(state).await
}

/// How see-through the window is. Clamped rather than refused, because an
/// unreadable window is worse than an ignored setting.
pub async fn set_opacity(state: &AppState, value: f64) -> Answer<Setup> {
    let mut config = state.config().await;
    config.opacity = value;
    // Read back through the clamp, so what is stored is what will be used.
    config.opacity = config.window_opacity();
    state.reconfigure(config).await?;
    setup(state).await
}

/// Mark first-run setup as done, so the app stops offering it.
pub async fn finish_setup(state: &AppState) -> Answer<Setup> {
    let mut config = state.config().await;
    config.setup_complete = true;
    state.reconfigure(config).await?;
    setup(state).await
}

// --------------------------------------------------- the promotion decision

/// How the current source of truth stands relative to the candidate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Standing {
    /// It cannot be read at all, so nothing can be compared.
    Unreadable,
    /// It has never held anything.
    Empty,
    Related(Relation),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Blocked {
    /// Promoting blind: it is not known what the old store held.
    TruthUnreadable,
    /// Both copies moved on, so one of them would be discarded.
    Diverged,
}

impl Blocked {
    fn explain(self, current: &str, candidate: &str, detail: &str) -> String {
        match self {
            Blocked::TruthUnreadable => format!(
                "{current} cannot be read, so it is not known whether {candidate} is \
                 up to date ({detail}); promote again to go ahead anyway"
            ),
            Blocked::Diverged => format!(
                "{current} and {candidate} have both changed independently, so promoting \
                 would discard one of them; promote again to go ahead anyway"
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    Proceed {
        was: &'static str,
        /// Whether the candidate must be brought up to date before it takes
        /// over, so promotion does not lose what the old store held.
        fast_forward: bool,
    },
    Blocked(Blocked),
}

/// Whether this promotion can go ahead, and what it has to do first.
///
/// Separated from the I/O because it is the part that can lose data, and a
/// decision table is far easier to be sure of than a chain of matches buried
/// in a command.
pub fn decide(standing: Standing, force: bool) -> Outcome {
    match standing {
        Standing::Unreadable if !force => Outcome::Blocked(Blocked::TruthUnreadable),
        Standing::Unreadable => Outcome::Proceed {
            was: "unknown",
            fast_forward: false,
        },

        // Nothing was ever written, so there is nothing to carry over.
        Standing::Empty => Outcome::Proceed {
            was: "empty",
            fast_forward: false,
        },

        Standing::Related(Relation::Same) => Outcome::Proceed {
            was: "identical",
            fast_forward: false,
        },

        // The ordinary case: a backup is behind the store it mirrors, so it is
        // brought up to date and then takes over having lost nothing.
        Standing::Related(Relation::Ahead) | Standing::Related(Relation::Behind) => {
            Outcome::Proceed {
                was: "behind",
                fast_forward: true,
            }
        }

        Standing::Related(Relation::Forked) if !force => Outcome::Blocked(Blocked::Diverged),
        Standing::Related(Relation::TooFarApart) if !force => Outcome::Blocked(Blocked::Diverged),
        Standing::Related(Relation::Forked) => Outcome::Proceed {
            was: "forked",
            fast_forward: false,
        },
        Standing::Related(Relation::TooFarApart) => Outcome::Proceed {
            was: "too far apart",
            fast_forward: false,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_backup_that_is_behind_is_brought_up_to_date_first() {
        // The ordinary case, and the one that must never lose the newer data.
        assert_eq!(
            decide(Standing::Related(Relation::Ahead), false),
            Outcome::Proceed {
                was: "behind",
                fast_forward: true
            }
        );
    }

    #[test]
    fn an_identical_copy_is_just_a_role_swap() {
        assert_eq!(
            decide(Standing::Related(Relation::Same), false),
            Outcome::Proceed {
                was: "identical",
                fast_forward: false
            }
        );
    }

    #[test]
    fn nothing_is_copied_when_the_old_store_never_held_anything() {
        assert_eq!(
            decide(Standing::Empty, false),
            Outcome::Proceed {
                was: "empty",
                fast_forward: false
            }
        );
    }

    #[test]
    fn two_copies_that_diverged_are_refused() {
        // Promoting either one discards the other, which is a decision for a
        // person and not for this code.
        assert_eq!(
            decide(Standing::Related(Relation::Forked), false),
            Outcome::Blocked(Blocked::Diverged)
        );
        assert_eq!(
            decide(Standing::Related(Relation::TooFarApart), false),
            Outcome::Blocked(Blocked::Diverged)
        );
    }

    #[test]
    fn promoting_blind_is_refused_until_it_is_asked_for_twice() {
        // The old store is gone, so whether the candidate is current is
        // unknowable. Often exactly what someone wants — but knowingly.
        assert_eq!(
            decide(Standing::Unreadable, false),
            Outcome::Blocked(Blocked::TruthUnreadable)
        );
        assert_eq!(
            decide(Standing::Unreadable, true),
            Outcome::Proceed {
                was: "unknown",
                fast_forward: false
            }
        );
    }

    #[test]
    fn forcing_never_turns_into_a_silent_overwrite_of_newer_data() {
        // Forcing past a fork proceeds without copying: the candidate keeps
        // what it has rather than being overwritten by the other side.
        assert_eq!(
            decide(Standing::Related(Relation::Forked), true),
            Outcome::Proceed {
                was: "forked",
                fast_forward: false
            }
        );
    }

    #[test]
    fn a_fast_forward_is_only_ever_proposed_when_the_copies_are_related() {
        for standing in [Standing::Unreadable, Standing::Empty] {
            for force in [false, true] {
                if let Outcome::Proceed { fast_forward, .. } = decide(standing, force) {
                    assert!(!fast_forward, "{standing:?} should not copy anything");
                }
            }
        }
    }

    #[test]
    fn every_refusal_names_both_stores_and_says_how_to_go_ahead() {
        for blocked in [Blocked::TruthUnreadable, Blocked::Diverged] {
            let message = blocked.explain("My bucket", "The NAS", "timed out");
            assert!(message.contains("My bucket"), "{message}");
            assert!(message.contains("The NAS"), "{message}");
            assert!(message.contains("promote again"), "{message}");
        }
    }
}

// ------------------------------------------------------------ signing in

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Connected {
    pub label: String,
    pub setup: Setup,
}

/// Connect a Google Drive by signing in.
///
/// Opens the browser, waits on a loopback port for the code to come back,
/// trades it for a refresh token, and keeps that in the keychain. The store
/// joins as a backup; making it the source of truth is a separate decision.
pub async fn connect_drive(
    state: &AppState,
    client_id: String,
    file_name: String,
    label: String,
) -> Answer<Connected> {
    let client_id = client_id.trim().to_string();
    if client_id.is_empty() {
        return Err(CommandError::Message(
            "a Google OAuth client id is required; create one of type \
             \"Desktop app\" in your own Google Cloud project"
                .into(),
        ));
    }
    let file_name = {
        let trimmed = file_name.trim();
        if trimmed.is_empty() {
            "home-ledger.json".to_string()
        } else {
            trimmed.to_string()
        }
    };

    if !state.keychain_available {
        tracing::warn!("no keychain: this sign-in will not survive the app closing");
    }

    let provider = oauth::Provider::google_drive();
    let pkce = oauth::Pkce::new();
    let csrf = oauth::new_state();

    let redirect = oauth::bind_loopback()
        .await
        .map_err(|e| CommandError::Message(e.to_string()))?;

    // Kept before the listener is consumed: the exchange has to present the
    // same redirect the authorize step used, or Google refuses it.
    let redirect_uri = redirect.uri.clone();

    let url = oauth::authorize_url(&provider, &client_id, &redirect_uri, &pkce, &csrf);
    oauth::open_in_browser(&url).map_err(|e| CommandError::Message(e.to_string()))?;

    let code = redirect
        .wait_for_code(&csrf)
        .await
        .map_err(|e| CommandError::Message(e.to_string()))?;

    let client = reqwest::Client::new();
    let (_access, refresh_token) =
        oauth::exchange_code(&client, &provider, &client_id, &redirect_uri, &code, &pkce)
            .await
            .map_err(|e| CommandError::Message(e.to_string()))?;

    let store = StoreConfig {
        id: ledger_domain::new_id(),
        label: {
            let trimmed = label.trim();
            if trimmed.is_empty() {
                "Google Drive".to_string()
            } else {
                trimmed.to_string()
            }
        },
        settings: ledger_config::Settings::GoogleDrive {
            file_name,
            client_id,
        },
        accept_risk: false,
    };

    // The token goes in before the store, since a store that cannot be built
    // must not reach the configuration.
    state
        .secrets
        .set(&store.id, &Secret::OAuth { refresh_token })?;

    let label = store.label.clone();
    let mut config = state.config().await;
    config.stores.push(store);
    state.reconfigure(config).await?;

    Ok(Connected {
        label,
        setup: setup(state).await?,
    })
}
