//! Everything Home Ledger does, with no window attached: the state an install
//! holds, every screen's figures, every edit, storage setup, encryption,
//! history and snapshot sharing.
//!
//! The desktop app wraps these as Tauri commands, the `hl` command line calls
//! them directly, and the phone app will do the same as the desktop. Keeping
//! them here, rather than in any one of those, is what makes the three agree.

pub mod audit;
pub mod clock;
pub mod commands;
pub mod dashboard;
pub mod encryption;
#[cfg(test)]
mod encryption_tests;
#[cfg(test)]
mod history_sync_tests;
pub mod planning;
pub mod plugin_history;
pub mod quotes;
pub mod sealed_file;
pub mod snapshots;
pub mod spending;
pub mod state;
pub mod storage;
pub mod transactions;
pub mod views;

pub use commands::{Answer, CommandError};
pub use state::{AppState, Client};
