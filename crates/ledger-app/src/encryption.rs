//! Turning encryption on and off, and unlocking a machine that has not seen
//! the passphrase yet. The sealing itself is `ledger_store::sealed`.

use crate::commands::{Answer, CommandError};
use crate::state::{AppState, DATA_KEY};
use ledger_config::Secret;
use ledger_store::sealed::{self, Kdf, Key, SealError};
use serde::Serialize;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    /// This machine seals what it writes.
    pub enabled: bool,
    /// This machine holds the key.
    pub unlocked: bool,
    /// Something sealed was found that this machine cannot open yet.
    pub needs_unlock: bool,
    /// Without a keychain the key is not kept, and the passphrase is asked
    /// at every launch.
    pub keychain: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Enabled {
    /// Shown once. Opens the ledger if the passphrase is forgotten.
    pub recovery_code: String,
    /// Copies that could not be reached to seal now; sealed when next written.
    pub skipped: Vec<String>,
    /// False when there is no keychain to keep the key in.
    pub kept: bool,
}

fn refused(e: SealError) -> CommandError {
    CommandError::Message(e.to_string())
}

pub async fn encryption_status(state: &AppState) -> Answer<Status> {
    status(state).await
}

pub async fn status(state: &AppState) -> Answer<Status> {
    // A read is what discovers what to unlock. Only worth doing when this
    // machine knows encryption is on but holds no key; otherwise the read the
    // screen just made has already found any sealed data.
    let config = state.config().await;
    if config.encryption_key_id.is_some()
        && !state.vault.unlocked()
        && state.vault.waiting().is_none()
    {
        let _ = state.live().await.engine.load().await;
    }
    Ok(Status {
        enabled: config.encryption_key_id.is_some(),
        unlocked: state.vault.unlocked(),
        needs_unlock: !state.vault.unlocked()
            && (state.vault.waiting().is_some() || config.encryption_key_id.is_some()),
        keychain: state.keychain_available,
    })
}

/// Keeps the key on this machine. False when there is nowhere to keep it.
fn keep(state: &AppState, key: &Key) -> bool {
    let secret = Secret::DataKey {
        key_id: key.id.clone(),
        key: key.export(),
        envelope: serde_json::to_string(&key.envelope).expect("an envelope serialises"),
    };
    match state.secrets.set(DATA_KEY, &secret) {
        Ok(()) => state.keychain_available,
        Err(e) => {
            tracing::warn!(error = %e, "the data key could not be kept");
            false
        }
    }
}

/// Seals every copy of the ledger, the queued edits, and the local history
/// and snapshots and bank connections, then this machine's shared history
/// and snapshot objects.
async fn reseal(state: &AppState) -> Answer<Vec<String>> {
    let skipped = state.live().await.engine.rewrite_all().await?;
    state
        .audit()
        .reseal()
        .await
        .map_err(|e| CommandError::Message(e.to_string()))?;
    state
        .points()
        .reseal()
        .await
        .map_err(|e| CommandError::Message(e.to_string()))?;
    crate::bank::banks(state)
        .reseal()
        .await
        .map_err(|e| CommandError::Message(e.to_string()))?;
    let (primary, install) = state.primary_and_install().await;
    for folder in ["history", "snapshots"] {
        let Some(slot) = primary.shelf(folder).and_then(|s| s.slot(&install)) else {
            continue;
        };
        if let Ok(Some(found)) = slot.load().await {
            let _ = slot
                .save(&found.body, ledger_store::Expect::Version(found.version))
                .await;
        }
    }
    Ok(skipped)
}

/// Turns encryption on with a new passphrase, and returns the recovery code.
pub async fn encryption_enable(state: &AppState, passphrase: String) -> Answer<Enabled> {
    enable(state, passphrase, Kdf::STANDARD).await
}

pub async fn enable(state: &AppState, passphrase: String, kdf: Kdf) -> Answer<Enabled> {
    if state.config().await.encryption_key_id.is_some() {
        return Err(CommandError::Message("encryption is already on".into()));
    }
    // Argon2 is deliberately slow; keep it off the async threads.
    let (key, code) = tokio::task::spawn_blocking(move || sealed::create(&passphrase, kdf))
        .await
        .map_err(|e| CommandError::Message(e.to_string()))?
        .map_err(refused)?;
    let kept = keep(state, &key);
    let key_id = key.id.clone();
    state.vault.set(true, Some(key));

    let skipped = match reseal(state).await {
        Ok(skipped) => skipped,
        Err(e) => {
            // Nothing is left half-on: back to plain, and say why.
            state.vault.set(false, None);
            let _ = state.secrets.forget(DATA_KEY);
            return Err(e);
        }
    };
    let mut config = state.config().await;
    config.encryption_key_id = Some(key_id);
    state.reconfigure(config).await?;
    Ok(Enabled {
        recovery_code: code,
        skipped,
        kept,
    })
}

/// Unlocks this machine with the passphrase or the recovery code.
pub async fn encryption_unlock(state: &AppState, secret: String) -> Answer<bool> {
    unlock(state, secret).await
}

pub async fn unlock(state: &AppState, secret: String) -> Answer<bool> {
    if state.vault.waiting().is_none() {
        let _ = state.live().await.engine.load().await;
    }
    let Some(envelope) = state.vault.waiting() else {
        return Err(CommandError::Message(
            "nothing encrypted was found to unlock".into(),
        ));
    };
    let key = tokio::task::spawn_blocking(move || {
        sealed::unlock_with_passphrase(&envelope, &secret)
            .or_else(|_| sealed::unlock_with_recovery_code(&envelope, &secret))
            .map_err(|_| "that is neither the passphrase nor the recovery code".to_string())
    })
    .await
    .map_err(|e| CommandError::Message(e.to_string()))?
    .map_err(CommandError::Message)?;

    let kept = keep(state, &key);
    let key_id = key.id.clone();
    state.vault.set(true, Some(key));
    let mut config = state.config().await;
    if config.encryption_key_id.as_deref() != Some(key_id.as_str()) {
        config.encryption_key_id = Some(key_id);
        state.reconfigure(config).await?;
    }
    // This machine's own files may still be plain from before.
    reseal(state).await?;
    Ok(kept)
}

/// Forgets the key on this machine. The ledger stays encrypted; the next use
/// here asks for the passphrase again.
pub async fn lock(state: &AppState) -> Answer<()> {
    let _ = state.secrets.forget(DATA_KEY);
    state.vault.set(state.vault.required(), None);
    Ok(())
}

/// Turns encryption off, after checking the passphrase, and rewrites
/// everything plain.
pub async fn encryption_disable(state: &AppState, passphrase: String) -> Answer<Vec<String>> {
    disable(state, passphrase).await
}

pub async fn disable(state: &AppState, passphrase: String) -> Answer<Vec<String>> {
    let Some(key_id) = state.config().await.encryption_key_id else {
        return Err(CommandError::Message("encryption is already off".into()));
    };
    let Some(key) = crate::state::kept_key(state.secrets.as_ref(), &key_id) else {
        return Err(CommandError::Message("unlock the ledger first".into()));
    };
    let envelope = key.envelope.clone();
    tokio::task::spawn_blocking(move || sealed::unlock_with_passphrase(&envelope, &passphrase))
        .await
        .map_err(|e| CommandError::Message(e.to_string()))?
        .map_err(refused)?;

    // Still able to open what is sealed, no longer sealing what is written.
    state.vault.set(false, Some(key));
    let skipped = reseal(state).await?;
    state.vault.set(false, None);
    let _ = state.secrets.forget(DATA_KEY);
    let mut config = state.config().await;
    config.encryption_key_id = None;
    state.reconfigure(config).await?;
    Ok(skipped)
}
