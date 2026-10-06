//! What the UI may ask for. Amounts cross as strings so no figure passes
//! through a JavaScript number on the way to being displayed.

use crate::state::AppState;
use ledger_domain::Ledger;
use ledger_domain::records::AuditEntry;
use ledger_store::{PendingOp, StoreStatus, SyncState};
use ledger_writer::import::ImportReport;
use ledger_writer::{Op, WriteError, Writer};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum CommandError {
    #[error("{0}")]
    Message(String),
}

impl Serialize for CommandError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

// Written out per source rather than as a blanket impl, which would collide
// with the standard library's `From<T> for T`.
macro_rules! from_error {
    ($($ty:ty),* $(,)?) => {
        $(impl From<$ty> for CommandError {
            fn from(e: $ty) -> Self {
                CommandError::Message(e.to_string())
            }
        })*
    };
}

from_error!(
    ledger_config::SetupError,
    ledger_config::SecretError,
    ledger_writer::WriteError,
    ledger_store::EngineError,
    ledger_store::StoreError,
    ledger_store::OutboxError,
    serde_json::Error,
);

pub type Answer<T> = Result<T, CommandError>;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub assets: String,
    pub debts: String,
    pub net: String,
    pub monthly_income: String,
    pub monthly_budget: String,
    pub bucket_cash: String,
    pub accounts: usize,
    /// True when this came from a mirror because the primary was unreachable.
    pub stale: bool,
    pub loaded_from: String,
}

pub async fn overview(state: &AppState) -> Answer<Overview> {
    let loaded = state.live().await.engine.load().await?;
    let ledger = match &loaded.snapshot {
        Some(s) => ledger_writer::read(&s.body)?,
        None => Ledger::default(),
    };

    let worth = ledger_math::net_worth(&ledger);
    Ok(Overview {
        assets: worth.assets.to_string(),
        debts: worth.debts.to_string(),
        net: worth.net.to_string(),
        monthly_income: ledger_math::monthly_income(&ledger).to_string(),
        monthly_budget: ledger_math::monthly_budget(&ledger).to_string(),
        bucket_cash: ledger_math::bucket_cash(&ledger).to_string(),
        accounts: ledger.accounts.len(),
        stale: loaded.stale,
        loaded_from: loaded.from.to_string(),
    })
}

/// Make an edit. Queues it locally, then tries to land it on the primary —
/// so an unreachable store slows the sync down, never the edit.
pub async fn apply(state: &AppState, op: Value) -> Answer<Option<Applied>> {
    apply_value(state, op).await
}

/// What an edit would do, without doing it: applied to a copy of the ledger
/// and never queued, written or recorded. None when it would change nothing.
pub async fn dry_run(state: &AppState, op: Value) -> Answer<Option<AuditEntry>> {
    let parsed: Op = serde_json::from_value(op)
        .map_err(|e| CommandError::Message(format!("that is not an edit this app knows: {e}")))?;
    let live = state.live().await;
    let loaded = live.engine.load().await?;
    let mut ledger = match &loaded.snapshot {
        Some(s) => ledger_writer::read(&s.body)?,
        None => Ledger::default(),
    };
    match live.writer.apply(&mut ledger, &parsed) {
        Ok(entry) => Ok(Some(entry)),
        Err(WriteError::Unchanged(_)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// The same edit path, callable from inside the app rather than from the UI.
/// None means the edit was valid but had nothing to do, so nothing was queued.
pub async fn apply_value(state: &AppState, op: Value) -> Answer<Option<Applied>> {
    apply_as(state, op, "").await
}

/// The same edit, labelled in the history with what brought it in, such as
/// "plaid", after any label the program already gave itself.
pub async fn apply_via(state: &AppState, op: Value, via: &str) -> Answer<Option<Applied>> {
    apply_as(state, op, via).await
}

async fn apply_as(state: &AppState, op: Value, via: &str) -> Answer<Option<Applied>> {
    // Parsed here rather than in the outbox so a malformed op is refused
    // before it is queued, not on every later flush.
    let parsed: Op = serde_json::from_value(op.clone())
        .map_err(|e| CommandError::Message(format!("that is not an edit this app knows: {e}")))?;

    // Held across the whole edit, so a reconfiguration cannot swap the store
    // out between checking the op and queueing it.
    let live = state.live().await;

    // Checked against the current document for the same reason the op is
    // parsed early: the person is told "that bucket is locked" now, not after
    // a sync they cannot see.
    let loaded = live.engine.load().await?;
    let mut ledger = match &loaded.snapshot {
        Some(s) => ledger_writer::read(&s.body)?,
        None => Ledger::default(),
    };
    let relabelled;
    let writer = if via.is_empty() {
        live.writer.as_ref()
    } else {
        let mut by = live.writer.by.clone();
        by.via = if by.via.is_empty() {
            via.to_string()
        } else {
            format!("{} · {via}", by.via)
        };
        relabelled = Writer::attributed(by);
        &relabelled
    };
    let entry = match writer.apply(&mut ledger, &parsed) {
        Ok(entry) => entry,
        Err(WriteError::Unchanged(_)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };

    let queued = live
        .engine
        .enqueue(PendingOp {
            id: entry.id.clone(),
            at: entry.at.clone(),
            device: live.writer.device.clone(),
            op,
        })
        .await?;

    let sync = live.engine.flush().await?;

    // Written after the edit is queued, so a refused edit records nothing.
    // A failure here is logged rather than raised: losing a line of history
    // is not a reason to tell someone their edit did not happen.
    if let Err(e) = state.audit().append(entry.clone()).await {
        tracing::warn!(error = %e, "the change was made but not recorded in history");
    }
    if !matches!(sync, SyncState::Behind { .. }) {
        state.spawn(crate::audit::share(
            live.engine.primary().clone(),
            live.config.install.clone(),
            state.places.data_dir.clone(),
            state.vault.clone(),
        ));
    }

    Ok(Some(Applied {
        entry,
        queued,
        sync,
    }))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Applied {
    pub entry: AuditEntry,
    pub queued: usize,
    pub sync: SyncState,
}

/// What importing this file would do. Reads and validates, writes nothing.
pub async fn import_preview(state: &AppState, path: String) -> Answer<ImportPreview> {
    let raw = tokio::fs::read(&path)
        .await
        .map_err(|e| CommandError::Message(format!("cannot read that file: {e}")))?;
    let imported = ledger_writer::import::read(&raw)?;

    // Whether anything would be lost tells the UI whether to warn.
    let loaded = state.live().await.engine.load().await?;
    let holding = match &loaded.snapshot {
        Some(s) => Ledger::from_bytes(&s.body).map(count_of).unwrap_or(0),
        None => 0,
    };

    Ok(ImportPreview {
        report: imported.report,
        replaces: holding,
    })
}

/// Replace the ledger with the contents of this file.
///
/// Deliberately not an incremental edit: it does not queue in the outbox, so
/// importing into a store that cannot be reached fails loudly instead of
/// landing later, when nobody is expecting it.
pub async fn import_apply(state: &AppState, path: String, replace: bool) -> Answer<ImportReport> {
    let raw = tokio::fs::read(&path)
        .await
        .map_err(|e| CommandError::Message(format!("cannot read that file: {e}")))?;
    let imported = ledger_writer::import::read(&raw)?;

    let loaded = state.live().await.engine.load().await?;
    let holding = match &loaded.snapshot {
        Some(s) => Ledger::from_bytes(&s.body).map(count_of).unwrap_or(0),
        None => 0,
    };
    if holding > 0 && !replace {
        return Err(CommandError::Message(format!(
            "this ledger already holds {holding} records; \
             importing would replace all of them"
        )));
    }

    state
        .live()
        .await
        .engine
        .adopt(&imported.ledger.to_bytes()?)
        .await?;
    Ok(imported.report)
}

fn count_of(ledger: Ledger) -> usize {
    ledger.accounts.len()
        + ledger.income.len()
        + ledger.budget.len()
        + ledger.buckets.len()
        + ledger.investments.len()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportPreview {
    pub report: ImportReport,
    /// How many records would be replaced. Zero means nothing is at risk.
    pub replaces: usize,
}

pub async fn stores(state: &AppState) -> Answer<Vec<StoreStatus>> {
    Ok(state.live().await.engine.status().await)
}

pub async fn sync_state(state: &AppState) -> Answer<SyncState> {
    sync(state).await
}

pub async fn flush(state: &AppState) -> Answer<SyncState> {
    sync(state).await
}

pub async fn sync(state: &AppState) -> Answer<SyncState> {
    let result = state.live().await.engine.flush().await?;
    if !matches!(result, SyncState::Behind { .. }) {
        state.share_history().await;
    }
    Ok(result)
}
