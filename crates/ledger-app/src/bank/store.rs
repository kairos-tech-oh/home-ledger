//! What this machine knows about its bank connections: which banks, which of
//! their accounts are linked to which ledger accounts, and the transactions
//! fetched so far.
//!
//! Kept beside the history in the data folder, sealed like it when encryption
//! is on, and never in the ledger: a connection is this machine's, and its
//! access token is in this machine's keychain. Changes are made under a lock
//! across processes, so the app and `hl` fetching at once lose nothing.

use super::plaid::{RemoteAccount, RemoteTransaction};
use crate::sealed_file;
use ledger_domain::Money;
use ledger_store::Vault;
use serde::{Deserialize, Serialize};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

/// Transactions kept per connection. Two years of a busy card is well under.
pub const MAX_TRANSACTIONS: usize = 20_000;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BankFile {
    pub items: Vec<Item>,
}

/// One connected bank: a Plaid "Item".
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub institution: String,
    pub connected_at: String,
    /// Where the next fetch carries on from. Empty fetches everything again.
    pub cursor: String,
    pub fetched_at: String,
    /// Plaid is still pulling this connection's history.
    pub gathering: bool,
    /// Why the last fetch failed, said plainly. Empty when it worked.
    pub problem: String,
    /// The bank wants the person to sign in again.
    pub needs_sign_in: bool,
    pub accounts: Vec<Account>,
    pub transactions: Vec<RemoteTransaction>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    pub mask: String,
    pub kind: String,
    pub subtype: String,
    /// The ledger account this is, or empty when it is not used.
    pub linked: String,
    pub current: Option<Money>,
    pub available: Option<Money>,
    pub limit: Option<Money>,
}

impl Item {
    /// Plaid's accounts as they now are, keeping each one's link.
    pub fn take_accounts(&mut self, remote: Vec<RemoteAccount>) {
        let links: Vec<(String, String)> = self
            .accounts
            .iter()
            .map(|a| (a.id.clone(), a.linked.clone()))
            .collect();
        self.accounts = remote
            .into_iter()
            .map(|r| Account {
                linked: links
                    .iter()
                    .find(|(id, _)| *id == r.id)
                    .map(|(_, l)| l.clone())
                    .unwrap_or_default(),
                id: r.id,
                name: r.name,
                mask: r.mask,
                kind: r.kind,
                subtype: r.subtype,
                current: r.current,
                available: r.available,
                limit: r.limit,
            })
            .collect();
    }

    /// Applies one page of changes. Added and modified both replace by id, so
    /// a page seen twice changes nothing the second time.
    pub fn take_changes(
        &mut self,
        added: Vec<RemoteTransaction>,
        modified: Vec<RemoteTransaction>,
        removed: &[String],
    ) -> (usize, usize, usize) {
        let gone = self.transactions.len();
        self.transactions.retain(|t| !removed.contains(&t.id));
        let gone = gone - self.transactions.len();
        let (mut new, mut changed) = (0, 0);
        for t in added.into_iter().chain(modified) {
            match self.transactions.iter_mut().find(|k| k.id == t.id) {
                Some(kept) => {
                    *kept = t;
                    changed += 1;
                }
                None => {
                    self.transactions.push(t);
                    new += 1;
                }
            }
        }
        // Newest first, and the oldest let go past the cap.
        self.transactions
            .sort_by(|a, b| b.date.cmp(&a.date).then_with(|| a.id.cmp(&b.id)));
        self.transactions.truncate(MAX_TRANSACTIONS);
        (new, changed, gone)
    }
}

pub struct Banks {
    path: PathBuf,
    vault: Arc<Vault>,
}

impl Banks {
    pub fn sealed(data_dir: &std::path::Path, vault: Arc<Vault>) -> Self {
        Self {
            path: data_dir.join("bank.json"),
            vault,
        }
    }

    pub async fn read(&self) -> io::Result<BankFile> {
        match sealed_file::read(&self.path, &self.vault).await? {
            Some(bytes) => serde_json::from_slice(&bytes)
                .map_err(|e| io::Error::other(format!("bank.json is not readable: {e}"))),
            None => Ok(BankFile::default()),
        }
    }

    async fn write(&self, file: &BankFile) -> io::Result<()> {
        let bytes = serde_json::to_vec_pretty(file).map_err(io::Error::other)?;
        sealed_file::write(&self.path, &self.vault, &bytes).await
    }

    /// Read, change and write back under the lock. The change sees the file
    /// as it is now, not as it was when some slow fetch began.
    pub async fn change<T>(&self, f: impl FnOnce(&mut BankFile) -> T) -> io::Result<T> {
        let _held = self.exclusive().await?;
        let mut file = self.read().await?;
        let out = f(&mut file);
        self.write(&file).await?;
        Ok(out)
    }

    /// Writes the file back as it is, sealed or not to match encryption now.
    pub async fn reseal(&self) -> io::Result<()> {
        if tokio::fs::metadata(&self.path).await.is_err() {
            return Ok(());
        }
        self.change(|_| ()).await
    }

    async fn exclusive(&self) -> io::Result<std::fs::File> {
        let mut name = self.path.as_os_str().to_owned();
        name.push(".lock");
        let path = PathBuf::from(name);
        if let Some(dir) = self.path.parent() {
            tokio::fs::create_dir_all(dir).await?;
        }
        tokio::task::spawn_blocking(move || {
            let file = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(&path)?;
            file.lock()?;
            Ok(file)
        })
        .await
        .map_err(io::Error::other)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(id: &str, date: &str, amount: i64) -> RemoteTransaction {
        RemoteTransaction {
            id: id.into(),
            account_id: "a".into(),
            date: date.into(),
            amount: Money::from(amount),
            ..Default::default()
        }
    }

    #[test]
    fn a_page_seen_twice_changes_nothing_the_second_time() {
        let mut item = Item::default();
        let page = vec![t("1", "2026-09-01", 5), t("2", "2026-09-03", 7)];
        assert_eq!(item.take_changes(page.clone(), vec![], &[]), (2, 0, 0));
        assert_eq!(item.take_changes(page, vec![], &[]), (0, 2, 0));
        assert_eq!(item.transactions.len(), 2);
        assert_eq!(item.transactions[0].id, "2", "newest first");

        let (_, changed, gone) =
            item.take_changes(vec![], vec![t("1", "2026-09-01", 6)], &["2".into()]);
        assert_eq!((changed, gone), (1, 1));
        assert_eq!(item.transactions[0].amount, Money::from(6));
    }

    #[test]
    fn refreshed_accounts_keep_their_links() {
        let mut item = Item::default();
        let remote = |name: &str| RemoteAccount {
            id: "a1".into(),
            name: name.into(),
            mask: "4421".into(),
            kind: "credit".into(),
            subtype: String::new(),
            current: None,
            available: None,
            limit: None,
            currency: String::new(),
        };
        item.take_accounts(vec![remote("Sapphire")]);
        item.accounts[0].linked = "f".repeat(32);
        item.take_accounts(vec![remote("Sapphire Preferred")]);
        assert_eq!(item.accounts[0].name, "Sapphire Preferred");
        assert_eq!(item.accounts[0].linked, "f".repeat(32));
    }

    #[tokio::test]
    async fn changes_are_kept_and_sealed_when_encryption_is_on() {
        let dir = tempfile::tempdir().unwrap();
        let vault = Vault::new();
        let banks = Banks::sealed(dir.path(), vault.clone());
        assert!(banks.read().await.unwrap().items.is_empty(), "no file yet");
        banks
            .change(|f| {
                f.items.push(Item {
                    id: "item-1".into(),
                    institution: "Chase".into(),
                    ..Default::default()
                })
            })
            .await
            .unwrap();
        assert_eq!(banks.read().await.unwrap().items[0].institution, "Chase");
        let raw = std::fs::read(dir.path().join("bank.json")).unwrap();
        assert!(
            String::from_utf8_lossy(&raw).contains("Chase"),
            "plain while off"
        );

        let quick = ledger_store::sealed::Kdf {
            memory_kib: 64,
            passes: 1,
            lanes: 1,
        };
        let (key, _) = ledger_store::sealed::create("a long passphrase", quick).unwrap();
        vault.set(true, Some(key));
        banks.reseal().await.unwrap();
        let raw = std::fs::read(dir.path().join("bank.json")).unwrap();
        assert!(ledger_store::sealed::is_sealed(&raw));
        assert!(!String::from_utf8_lossy(&raw).contains("Chase"));
        assert_eq!(banks.read().await.unwrap().items[0].institution, "Chase");
    }
}
