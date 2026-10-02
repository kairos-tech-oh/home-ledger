//! A local file sealed like the stores are, for the history and the
//! snapshots kept on this machine.
//!
//! Reading is strict on purpose. A history file that is merely damaged reads
//! as empty and is started again, as before; one that is sealed while this
//! machine is locked is refused, because reading it as empty and then
//! appending would overwrite everything it held with one entry.

use ledger_store::{StoreError, Vault};
use std::io;
use std::path::Path;

fn locked() -> io::Error {
    io::Error::other("this file is encrypted; enter the passphrase to unlock it")
}

/// The file's plain contents, `None` when there is no file yet.
pub async fn read(path: &Path, vault: &Vault) -> io::Result<Option<Vec<u8>>> {
    match tokio::fs::read(path).await {
        Ok(bytes) => match vault.open(&bytes) {
            Ok(plain) => Ok(Some(plain)),
            Err(StoreError::Locked) => Err(locked()),
            Err(e) => Err(io::Error::other(e.to_string())),
        },
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}

/// Written whole through a temporary file, sealed when encryption is on.
pub async fn write(path: &Path, vault: &Vault, plain: &[u8]) -> io::Result<()> {
    let body = vault.seal(plain).map_err(|_| locked())?;
    if let Some(dir) = path.parent() {
        tokio::fs::create_dir_all(dir).await?;
    }
    let temp = path.with_extension(format!("tmp-{}", uuid::Uuid::new_v4().simple()));
    tokio::fs::write(&temp, &body).await?;
    tokio::fs::rename(&temp, path).await
}
