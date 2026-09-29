//! A ledger in a Google Drive file.
//!
//! **Drive cannot refuse a stale write.** The v3 API has no `If-Match` on
//! `files.update`, so unlike S3 there is no way to say "only if it is still
//! the version I read". What it does have is a `version` that increases on
//! every change, and revision history.
//!
//! So the guarantee here is different and the store says so: the version is
//! checked before writing and again after, and a change that appeared in
//! between is reported as a conflict. That catches a clobber rather than
//! preventing it, and what was overwritten is still in Drive's own revisions.
//! See [`Cas::CheckedAfterWrite`].

use crate::oauth::{Access, OAuthError, Provider, refresh};
use crate::store::*;
use async_trait::async_trait;
use serde::Deserialize;
use std::time::Duration;
use tokio::sync::Mutex;

const API: &str = "https://www.googleapis.com/drive/v3";
const UPLOAD: &str = "https://www.googleapis.com/upload/drive/v3";
const MAX_BYTES: u64 = 32 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Clone, Debug)]
pub struct DriveConfig {
    /// The file's name in the Drive. Only files this app created are visible
    /// to it, so this cannot collide with anything else.
    pub file_name: String,
    /// From a Google Cloud OAuth client of type "Desktop app". Not a secret:
    /// PKCE is what proves the exchange, which is why none is stored.
    pub client_id: String,
}

pub struct DriveStore {
    id: StoreId,
    client: reqwest::Client,
    config: DriveConfig,
    refresh_token: String,
    /// Cached so every call does not re-sign in, and the file is not looked up
    /// by name on every read.
    session: Mutex<Session>,
}

#[derive(Default)]
struct Session {
    access: Option<Access>,
    file_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FileMeta {
    id: String,
    /// Increases on every change. Drive sends it as a string.
    #[serde(default)]
    version: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FileList {
    #[serde(default)]
    files: Vec<FileMeta>,
}

impl DriveStore {
    pub fn new(
        id: impl Into<String>,
        config: DriveConfig,
        refresh_token: String,
    ) -> Result<Self, StoreError> {
        if config.client_id.trim().is_empty() {
            return Err(StoreError::Denied(
                "a Google OAuth client id is required".into(),
            ));
        }
        if config.file_name.trim().is_empty() {
            return Err(StoreError::Denied("a file name is required".into()));
        }

        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .https_only(true)
            .build()
            .map_err(|e| StoreError::Unreachable(e.to_string()))?;

        Ok(Self {
            id: StoreId(id.into()),
            client,
            config,
            refresh_token,
            session: Mutex::new(Session::default()),
        })
    }

    /// A usable access token, refreshed if the one held has run out.
    async fn token(&self) -> Result<String, StoreError> {
        let mut session = self.session.lock().await;
        if let Some(access) = &session.access
            && access.usable()
        {
            return Ok(access.token.clone());
        }

        let fresh = refresh(
            &self.client,
            &Provider::google_drive(),
            &self.config.client_id,
            &self.refresh_token,
        )
        .await
        .map_err(oauth_error)?;

        let token = fresh.token.clone();
        session.access = Some(fresh);
        Ok(token)
    }

    /// The file's id, looked up by name the first time and remembered after.
    async fn file_id(&self) -> Result<Option<String>, StoreError> {
        if let Some(id) = &self.session.lock().await.file_id {
            return Ok(Some(id.clone()));
        }

        let token = self.token().await?;
        let query = format!(
            "name = '{}' and trashed = false",
            escape(&self.config.file_name)
        );
        let response = self
            .client
            .get(format!("{API}/files"))
            .bearer_auth(&token)
            .query(&[
                ("q", query.as_str()),
                ("spaces", "drive"),
                ("fields", "files(id,version)"),
                ("pageSize", "1"),
            ])
            .send()
            .await
            .map_err(transport)?;

        let status = response.status().as_u16();
        let body = response.text().await.map_err(transport)?;
        if status != 200 {
            return Err(explain(status, &body));
        }

        let list: FileList =
            serde_json::from_str(&body).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        let found = list.files.into_iter().next().map(|f| f.id);
        if let Some(id) = &found {
            self.session.lock().await.file_id = Some(id.clone());
        }
        Ok(found)
    }

    /// The file's current version, which stands in for an ETag.
    async fn version_of(&self, file_id: &str) -> Result<Version, StoreError> {
        let token = self.token().await?;
        let response = self
            .client
            .get(format!("{API}/files/{file_id}"))
            .bearer_auth(&token)
            .query(&[("fields", "id,version")])
            .send()
            .await
            .map_err(transport)?;

        let status = response.status().as_u16();
        let body = response.text().await.map_err(transport)?;
        if status != 200 {
            return Err(explain(status, &body));
        }
        let meta: FileMeta =
            serde_json::from_str(&body).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        Ok(Version(meta.version.unwrap_or_default()))
    }

    async fn create(&self, body: &[u8]) -> Result<Version, StoreError> {
        let token = self.token().await?;
        let metadata = serde_json::json!({ "name": self.config.file_name });

        let form = reqwest::multipart::Form::new()
            .part(
                "metadata",
                reqwest::multipart::Part::text(metadata.to_string())
                    .mime_str("application/json")
                    .map_err(|e| StoreError::Denied(e.to_string()))?,
            )
            .part(
                "file",
                reqwest::multipart::Part::bytes(body.to_vec())
                    .mime_str("application/json")
                    .map_err(|e| StoreError::Denied(e.to_string()))?,
            );

        let response = self
            .client
            .post(format!("{UPLOAD}/files"))
            .bearer_auth(&token)
            .query(&[("uploadType", "multipart"), ("fields", "id,version")])
            .multipart(form)
            .send()
            .await
            .map_err(transport)?;

        let status = response.status().as_u16();
        let text = response.text().await.map_err(transport)?;
        if !(200..300).contains(&status) {
            return Err(explain(status, &text));
        }

        let meta: FileMeta =
            serde_json::from_str(&text).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        self.session.lock().await.file_id = Some(meta.id);
        Ok(Version(meta.version.unwrap_or_default()))
    }

    async fn overwrite(&self, file_id: &str, body: &[u8]) -> Result<Version, StoreError> {
        let token = self.token().await?;
        let response = self
            .client
            .patch(format!("{UPLOAD}/files/{file_id}"))
            .bearer_auth(&token)
            .query(&[("uploadType", "media"), ("fields", "id,version")])
            .header("content-type", "application/json")
            .body(body.to_vec())
            .send()
            .await
            .map_err(transport)?;

        let status = response.status().as_u16();
        let text = response.text().await.map_err(transport)?;
        if !(200..300).contains(&status) {
            return Err(explain(status, &text));
        }
        let meta: FileMeta =
            serde_json::from_str(&text).map_err(|e| StoreError::Corrupt(e.to_string()))?;
        Ok(Version(meta.version.unwrap_or_default()))
    }
}

/// Drive's query language takes single-quoted strings, so an apostrophe in a
/// file name has to be escaped or it ends the string.
fn escape(name: &str) -> String {
    name.replace('\\', "\\\\").replace('\'', "\\'")
}

fn transport(error: reqwest::Error) -> StoreError {
    if error.is_timeout() || error.is_connect() {
        StoreError::Unreachable(error.to_string())
    } else {
        StoreError::Denied(error.to_string())
    }
}

fn oauth_error(error: OAuthError) -> StoreError {
    match error {
        OAuthError::Unreachable(why) => StoreError::Unreachable(why),
        other => StoreError::Denied(other.to_string()),
    }
}

/// Drive reports a failure as JSON with a message worth showing.
fn explain(status: u16, body: &str) -> StoreError {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|v| v["error"]["message"].as_str().map(str::to_string))
        .unwrap_or_default();

    match status {
        401 | 403 => StoreError::Denied(if message.is_empty() {
            "Google refused these credentials".into()
        } else {
            message
        }),
        404 => StoreError::Denied("that file is not there".into()),
        429 => StoreError::Unreachable("Google is rate limiting this account".into()),
        500..=599 => StoreError::Unreachable(format!("Google returned {status}")),
        _ if message.is_empty() => StoreError::Denied(format!("Google returned {status}")),
        _ => StoreError::Denied(format!("{message} ({status})")),
    }
}

#[async_trait]
impl Store for DriveStore {
    fn id(&self) -> &StoreId {
        &self.id
    }

    fn kind(&self) -> StoreKind {
        StoreKind::GoogleDrive
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // Not Native: Drive will not refuse a stale write. See the module
            // comment for what is done instead.
            cas: Cas::CheckedAfterWrite,
            shared: true,
            max_bytes: MAX_BYTES,
        }
    }

    async fn health(&self) -> Health {
        // Refreshing the token answers the only question worth asking without
        // downloading anything: are these credentials still good.
        match self.token().await {
            Ok(_) => Health::Reachable,
            Err(StoreError::Denied(why)) => Health::Denied(why),
            Err(e) => Health::Unreachable(e.to_string()),
        }
    }

    async fn load(&self) -> Result<Option<Snapshot>, StoreError> {
        let Some(file_id) = self.file_id().await? else {
            return Ok(None);
        };
        let token = self.token().await?;

        let response = self
            .client
            .get(format!("{API}/files/{file_id}"))
            .bearer_auth(&token)
            .query(&[("alt", "media")])
            .send()
            .await
            .map_err(transport)?;

        let status = response.status().as_u16();
        if status == 404 {
            self.session.lock().await.file_id = None;
            return Ok(None);
        }
        let body = response.bytes().await.map_err(transport)?;
        if status != 200 {
            return Err(explain(status, &String::from_utf8_lossy(&body)));
        }
        if body.len() as u64 > MAX_BYTES {
            return Err(StoreError::TooLarge {
                size: body.len() as u64,
                limit: MAX_BYTES,
            });
        }

        Ok(Some(Snapshot {
            version: self.version_of(&file_id).await?,
            body: body.to_vec(),
        }))
    }

    async fn save(&self, body: &[u8], expect: Expect) -> Result<Version, StoreError> {
        if body.len() as u64 > MAX_BYTES {
            return Err(StoreError::TooLarge {
                size: body.len() as u64,
                limit: MAX_BYTES,
            });
        }

        let existing = self.file_id().await?;

        match (&expect, &existing) {
            (Expect::Absent, Some(_)) => return Err(StoreError::Conflict),
            (Expect::Version(_), None) => return Err(StoreError::Conflict),
            // Checked before writing, since catching it here costs nothing and
            // avoids overwriting anything at all.
            (Expect::Version(wanted), Some(file_id))
                if &self.version_of(file_id).await? != wanted =>
            {
                return Err(StoreError::Conflict);
            }
            _ => {}
        }

        let Some(file_id) = existing else {
            return self.create(body).await;
        };

        let written = self.overwrite(&file_id, body).await?;

        // Checked again afterwards. Drive cannot refuse a stale write, so this
        // is where an interleaved change is caught: the engine re-reads and
        // replays, and Drive's revision history still holds what was
        // overwritten.
        if let Expect::Version(wanted) = &expect
            && !advanced_once(wanted, &written)
        {
            return Err(StoreError::Conflict);
        }

        Ok(written)
    }
}

/// Whether a version moved on by exactly this write and no other.
///
/// Drive's version is a decimal counter as a string. A jump of more than one
/// means somebody else wrote in between.
fn advanced_once(before: &Version, after: &Version) -> bool {
    match (before.0.parse::<u64>(), after.0.parse::<u64>()) {
        (Ok(a), Ok(b)) => b <= a + 1,
        // Unparseable on either side: nothing can be concluded, so this does
        // not claim a conflict it cannot demonstrate.
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> DriveStore {
        DriveStore::new(
            "drive",
            DriveConfig {
                file_name: "ledger.json".into(),
                client_id: "id.apps.googleusercontent.com".into(),
            },
            "refresh-token".into(),
        )
        .unwrap()
    }

    #[test]
    fn a_client_id_and_a_file_name_are_both_required() {
        for (name, client) in [("", "id"), ("ledger.json", "")] {
            assert!(
                DriveStore::new(
                    "drive",
                    DriveConfig {
                        file_name: name.into(),
                        client_id: client.into()
                    },
                    "t".into()
                )
                .is_err()
            );
        }
    }

    #[test]
    fn it_does_not_claim_a_guarantee_drive_cannot_keep() {
        // The API has no If-Match on files.update, so calling this Native
        // would be a promise the store cannot honour.
        assert_eq!(store().capabilities().cas, Cas::CheckedAfterWrite);
        assert_ne!(store().capabilities().cas, Cas::Native);
        assert!(store().capabilities().shared);
    }

    #[test]
    fn an_apostrophe_in_a_file_name_cannot_break_out_of_the_query() {
        assert_eq!(escape("my'ledger.json"), "my\\'ledger.json");
        assert_eq!(escape("back\\slash"), "back\\\\slash");
    }

    #[test]
    fn a_version_that_moved_by_one_is_this_write_and_nobody_else() {
        assert!(advanced_once(&Version("7".into()), &Version("8".into())));
    }

    #[test]
    fn a_version_that_jumped_means_someone_wrote_in_between() {
        assert!(!advanced_once(&Version("7".into()), &Version("9".into())));
    }

    #[test]
    fn an_unreadable_version_does_not_invent_a_conflict() {
        // Reporting a conflict that cannot be demonstrated would block syncing
        // with no way for anyone to resolve it.
        assert!(advanced_once(&Version("".into()), &Version("8".into())));
        assert!(advanced_once(
            &Version("7".into()),
            &Version("later".into())
        ));
    }

    #[test]
    fn a_refused_token_is_denied_rather_than_looking_like_an_outage() {
        let denied = explain(401, r#"{"error":{"message":"Invalid Credentials"}}"#);
        assert!(matches!(denied, StoreError::Denied(_)));
        assert!(!denied.is_transient(), "a bad token must not retry forever");
        assert!(denied.to_string().contains("Invalid Credentials"));
    }

    #[test]
    fn rate_limiting_and_outages_are_transient_so_edits_wait() {
        assert!(explain(429, "{}").is_transient());
        assert!(explain(503, "{}").is_transient());
    }
}
