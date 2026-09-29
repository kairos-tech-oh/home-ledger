//! A ledger in an S3 object.
//!
//! Also reaches every S3-compatible service — MinIO, Backblaze B2, Cloudflare
//! R2, Wasabi — because they speak the same API. That is what the endpoint
//! field is for, and it is why this is the first remote store worth building.

use crate::sigv4::{self, Credentials, Request};
use crate::store::*;
use async_trait::async_trait;
use std::sync::Arc;
use std::time::Duration;

/// Refuse a document larger than this rather than stream it. A household
/// ledger is measured in hundreds of kilobytes; anything near this is a bug
/// or a hostile response, and either way is not worth buffering.
const MAX_BYTES: u64 = 32 * 1024 * 1024;

const TIMEOUT: Duration = Duration::from_secs(20);

#[derive(Clone)]
pub struct S3Store {
    id: StoreId,
    client: reqwest::Client,
    credentials: Credentials,
    bucket: String,
    key: String,
    region: String,
    /// Host the request actually goes to. Virtual-hosted for real S3,
    /// path-style for most compatible services.
    host: String,
    base: String,
    path_style: bool,
}

/// How to reach a bucket. Everything here is non-secret and belongs in the
/// config file; the credentials come from the keychain separately.
#[derive(Clone, Debug)]
pub struct S3Config {
    pub bucket: String,
    pub key: String,
    pub region: String,
    /// `None` for Amazon S3. Otherwise the service's endpoint, which also
    /// switches on path-style addressing.
    pub endpoint: Option<String>,
}

impl S3Store {
    pub fn new(
        id: impl Into<String>,
        config: S3Config,
        credentials: Credentials,
    ) -> Result<Self, StoreError> {
        if config.bucket.is_empty() {
            return Err(StoreError::Denied("a bucket name is required".into()));
        }

        let (host, base, path_style) = match &config.endpoint {
            Some(endpoint) => {
                let trimmed = endpoint.trim_end_matches('/');
                let without_scheme = trimmed
                    .strip_prefix("https://")
                    .or_else(|| trimmed.strip_prefix("http://"))
                    .unwrap_or(trimmed);
                (
                    without_scheme.to_string(),
                    format!("https://{without_scheme}"),
                    true,
                )
            }
            None => {
                let host = format!("{}.s3.{}.amazonaws.com", config.bucket, config.region);
                let base = format!("https://{host}");
                (host, base, false)
            }
        };

        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .https_only(config.endpoint.is_none())
            .build()
            .map_err(|e| StoreError::Unreachable(e.to_string()))?;

        Ok(Self {
            id: StoreId(id.into()),
            client,
            credentials,
            bucket: config.bucket,
            key: config.key,
            region: config.region,
            host,
            base,
            path_style,
        })
    }

    /// The object's path within the host, which differs between
    /// virtual-hosted and path-style addressing.
    fn object_path(&self) -> String {
        if self.path_style {
            format!("/{}/{}", self.bucket, self.key.trim_start_matches('/'))
        } else {
            format!("/{}", self.key.trim_start_matches('/'))
        }
    }

    fn bucket_path(&self) -> String {
        if self.path_style {
            format!("/{}", self.bucket)
        } else {
            "/".into()
        }
    }

    async fn send(
        &self,
        method: &str,
        body: Vec<u8>,
        extra: &[(String, String)],
    ) -> Result<(u16, Vec<(String, String)>, Vec<u8>), StoreError> {
        self.send_to(method, &self.object_path(), "", body, extra)
            .await
    }

    async fn send_to(
        &self,
        method: &str,
        path: &str,
        query: &str,
        body: Vec<u8>,
        extra: &[(String, String)],
    ) -> Result<(u16, Vec<(String, String)>, Vec<u8>), StoreError> {
        let (amz_date, date) = timestamps();
        let path = path.to_string();

        let headers = sigv4::sign(
            &Request {
                method,
                host: &self.host,
                path: &path,
                query,
                payload: &body,
                region: &self.region,
                service: "s3",
                amz_date: &amz_date,
                date: &date,
            },
            &self.credentials,
            extra,
        );

        let mut url = format!("{}{}", self.base, sigv4::encode_path(&path));
        if !query.is_empty() {
            url = format!("{url}?{query}");
        }
        let mut request = self.client.request(
            method
                .parse()
                .map_err(|_| StoreError::Denied("bad method".into()))?,
            &url,
        );
        for (name, value) in &headers {
            // Host is set by the client from the URL; sending it twice is an
            // error in reqwest and it is already covered by the signature.
            if name != "host" {
                request = request.header(name, value);
            }
        }
        if !body.is_empty() {
            request = request.body(body);
        }

        let response = request.send().await.map_err(|e| {
            if e.is_timeout() || e.is_connect() {
                StoreError::Unreachable(e.to_string())
            } else {
                StoreError::Denied(e.to_string())
            }
        })?;

        let status = response.status().as_u16();
        let out_headers: Vec<(String, String)> = response
            .headers()
            .iter()
            .map(|(n, v)| (n.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
            .collect();

        if let Some(length) = response.content_length()
            && length > MAX_BYTES
        {
            return Err(StoreError::TooLarge {
                size: length,
                limit: MAX_BYTES,
            });
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| StoreError::Unreachable(e.to_string()))?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(StoreError::TooLarge {
                size: bytes.len() as u64,
                limit: MAX_BYTES,
            });
        }

        Ok((status, out_headers, bytes.to_vec()))
    }

    fn folder_prefix(&self, folder: &str) -> String {
        let key = self.key.trim_start_matches('/');
        match key.rsplit_once('/') {
            Some((dir, _)) => format!("{dir}/{folder}/"),
            None => format!("{folder}/"),
        }
    }

    fn etag_of(headers: &[(String, String)]) -> Version {
        let raw = headers
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case("etag"))
            .map(|(_, value)| value.as_str())
            .unwrap_or("");
        Version(raw.trim_matches('"').to_string())
    }

    /// S3 reports a failure as an XML document. The code inside it is the only
    /// useful part, and it is what distinguishes "wrong key" from "no bucket".
    fn explain(status: u16, body: &[u8]) -> StoreError {
        let text = String::from_utf8_lossy(body);
        let code = text
            .split_once("<Code>")
            .and_then(|(_, rest)| rest.split_once("</Code>"))
            .map(|(code, _)| code.to_string())
            .unwrap_or_default();

        match (status, code.as_str()) {
            (403, _)
            | (_, "AccessDenied")
            | (_, "InvalidAccessKeyId")
            | (_, "SignatureDoesNotMatch") => StoreError::Denied(if code.is_empty() {
                "the credentials were refused".into()
            } else {
                format!("the credentials were refused ({code})")
            }),
            (404, _) | (_, "NoSuchBucket") => {
                StoreError::Denied(format!("no such bucket or object ({code})"))
            }
            (500..=599, _) => StoreError::Unreachable(format!("the service returned {status}")),
            (s, "") => StoreError::Denied(format!("the service returned {s}")),
            (s, c) => StoreError::Denied(format!("the service returned {s} ({c})")),
        }
    }
}

#[async_trait]
impl Store for S3Store {
    fn id(&self) -> &StoreId {
        &self.id
    }

    fn kind(&self) -> StoreKind {
        StoreKind::S3
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            // If-Match and If-None-Match, enforced by the service. Safe for
            // several machines writing at once.
            cas: Cas::Native,
            shared: true,
            max_bytes: MAX_BYTES,
        }
    }

    async fn health(&self) -> Health {
        // A HEAD costs nothing and answers the only question worth asking:
        // can these credentials reach this object. A missing object is fine —
        // that is a first run, not a fault.
        match self.send("HEAD", Vec::new(), &[]).await {
            Ok((200 | 404, _, _)) => Health::Reachable,
            Ok((403, _, body)) => Health::Denied(Self::explain(403, &body).to_string()),
            Ok((status, _, body)) => Health::Unreachable(Self::explain(status, &body).to_string()),
            Err(StoreError::Denied(why)) => Health::Denied(why),
            Err(e) => Health::Unreachable(e.to_string()),
        }
    }

    async fn load(&self) -> Result<Option<Snapshot>, StoreError> {
        let (status, headers, body) = self.send("GET", Vec::new(), &[]).await?;
        match status {
            200 => Ok(Some(Snapshot {
                version: Self::etag_of(&headers),
                body,
            })),
            // Absent is not an error: an empty bucket is a first run.
            404 => Ok(None),
            _ => Err(Self::explain(status, &body)),
        }
    }

    async fn save(&self, body: &[u8], expect: Expect) -> Result<Version, StoreError> {
        if body.len() as u64 > MAX_BYTES {
            return Err(StoreError::TooLarge {
                size: body.len() as u64,
                limit: MAX_BYTES,
            });
        }

        let mut extra = vec![
            ("content-type".into(), "application/json".into()),
            ("x-amz-server-side-encryption".into(), "AES256".into()),
        ];
        match &expect {
            Expect::Version(version) => {
                extra.push(("if-match".into(), format!("\"{}\"", version.0)))
            }
            Expect::Absent => extra.push(("if-none-match".into(), "*".into())),
            Expect::Force => {}
        }

        let (status, headers, response) = self.send("PUT", body.to_vec(), &extra).await?;
        match status {
            200 => Ok(Self::etag_of(&headers)),
            // 409 as well as 412: some S3-compatible services report a failed
            // precondition as a conflict.
            409 | 412 => Err(StoreError::Conflict),
            _ => Err(Self::explain(status, &response)),
        }
    }

    fn shelf(&self, folder: &str) -> Option<Arc<dyn Shelf>> {
        shelf_name_ok(folder).then(|| {
            Arc::new(S3Shelf {
                prefix: self.folder_prefix(folder),
                store: self.clone(),
            }) as Arc<dyn Shelf>
        })
    }
}

struct S3Shelf {
    prefix: String,
    store: S3Store,
}

#[async_trait]
impl Shelf for S3Shelf {
    async fn names(&self) -> Result<Vec<String>, StoreError> {
        let query = format!("list-type=2&prefix={}", sigv4::encode_segment(&self.prefix));
        let path = self.store.bucket_path();
        let (status, _, body) = self
            .store
            .send_to("GET", &path, &query, Vec::new(), &[])
            .await?;
        if status != 200 {
            return Err(S3Store::explain(status, &body));
        }
        Ok(listed_names(&String::from_utf8_lossy(&body), &self.prefix))
    }

    fn slot(&self, name: &str) -> Option<Arc<dyn Store>> {
        shelf_name_ok(name).then(|| {
            let mut store = self.store.clone();
            store.id = StoreId(format!("{}/{name}", self.store.id));
            store.key = format!("{}{name}.json", self.prefix);
            Arc::new(store) as Arc<dyn Store>
        })
    }
}

fn listed_names(xml: &str, prefix: &str) -> Vec<String> {
    let mut names: Vec<String> = xml
        .split("<Key>")
        .skip(1)
        .filter_map(|rest| rest.split_once("</Key>").map(|(key, _)| key))
        .filter_map(|key| key.strip_prefix(prefix)?.strip_suffix(".json"))
        .filter(|name| shelf_name_ok(name))
        .map(str::to_string)
        .collect();
    names.sort();
    names
}

/// `(yyyymmddThhmmssZ, yyyymmdd)` for right now, in UTC.
fn timestamps() -> (String, String) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let (year, month, day, hour, minute, second) = civil_from_unix(now as i64);
    (
        format!("{year:04}{month:02}{day:02}T{hour:02}{minute:02}{second:02}Z"),
        format!("{year:04}{month:02}{day:02}"),
    )
}

/// Days-from-civil, in reverse. Avoids a date dependency for the one place
/// this crate needs a calendar.
fn civil_from_unix(secs: i64) -> (i64, u32, u32, u32, u32, u32) {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);

    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = if m <= 2 { y + 1 } else { y };

    (
        year,
        m,
        d,
        (rem / 3_600) as u32,
        ((rem % 3_600) / 60) as u32,
        (rem % 60) as u32,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn credentials() -> Credentials {
        Credentials {
            access_key_id: "key".into(),
            secret_access_key: "secret".into(),
            session_token: None,
        }
    }

    fn config(endpoint: Option<&str>) -> S3Config {
        S3Config {
            bucket: "my-bucket".into(),
            key: "ledger/ledger.json".into(),
            region: "us-east-2".into(),
            endpoint: endpoint.map(str::to_string),
        }
    }

    #[test]
    fn amazon_uses_a_virtual_hosted_url() {
        let store = S3Store::new("s3", config(None), credentials()).unwrap();
        assert_eq!(store.host, "my-bucket.s3.us-east-2.amazonaws.com");
        assert_eq!(store.object_path(), "/ledger/ledger.json");
    }

    #[test]
    fn a_custom_endpoint_uses_path_style_so_the_bucket_is_in_the_path() {
        // What makes MinIO, R2, B2 and Wasabi work without any more code.
        let store = S3Store::new(
            "minio",
            config(Some("https://minio.home:9000")),
            credentials(),
        )
        .unwrap();
        assert_eq!(store.host, "minio.home:9000");
        assert_eq!(store.object_path(), "/my-bucket/ledger/ledger.json");
    }

    #[test]
    fn an_endpoint_without_a_scheme_still_works() {
        let store = S3Store::new("minio", config(Some("minio.home:9000")), credentials()).unwrap();
        assert_eq!(store.host, "minio.home:9000");
    }

    #[test]
    fn a_bucket_is_required() {
        let mut broken = config(None);
        broken.bucket = String::new();
        assert!(S3Store::new("s3", broken, credentials()).is_err());
    }

    #[test]
    fn it_declares_a_real_compare_and_swap() {
        let store = S3Store::new("s3", config(None), credentials()).unwrap();
        assert_eq!(store.capabilities().cas, Cas::Native);
        assert!(store.capabilities().shared);
    }

    #[test]
    fn an_etag_is_read_without_its_quotes() {
        let headers = vec![("ETag".to_string(), "\"abc123\"".to_string())];
        assert_eq!(S3Store::etag_of(&headers).0, "abc123");
    }

    #[test]
    fn a_refused_key_is_reported_as_denied_not_as_a_network_problem() {
        // The distinction decides whether the app retries forever or tells
        // the person their key is wrong.
        let body = b"<Error><Code>InvalidAccessKeyId</Code></Error>";
        let error = S3Store::explain(403, body);
        assert!(matches!(error, StoreError::Denied(_)));
        assert!(!error.is_transient(), "a bad key must not look retryable");
        assert!(error.to_string().contains("InvalidAccessKeyId"));
    }

    #[test]
    fn a_service_outage_is_transient_so_the_outbox_holds() {
        let error = S3Store::explain(503, b"<Error><Code>SlowDown</Code></Error>");
        assert!(
            error.is_transient(),
            "a 5xx should let edits wait and retry"
        );
    }

    #[test]
    fn the_clock_conversion_matches_known_instants() {
        assert_eq!(civil_from_unix(0), (1970, 1, 1, 0, 0, 0));
        // 2026-09-21T18:30:45Z
        assert_eq!(civil_from_unix(1_790_015_445), (2026, 9, 21, 18, 30, 45));
        // A leap day, which is where naive date maths goes wrong.
        assert_eq!(civil_from_unix(1_709_164_800), (2024, 2, 29, 0, 0, 0));
    }

    #[test]
    fn the_two_timestamps_describe_the_same_day() {
        let (amz, date) = timestamps();
        assert_eq!(&amz[..8], date.as_str());
        assert_eq!(amz.len(), 16);
        assert!(amz.ends_with('Z'));
    }

    #[test]
    fn a_shelf_sits_beside_the_ledger_key() {
        let store = S3Store::new("s3", config(None), credentials()).unwrap();
        assert_eq!(store.folder_prefix("history"), "ledger/history/");
        let mut flat = config(None);
        flat.key = "ledger.json".into();
        let store = S3Store::new("s3", flat, credentials()).unwrap();
        assert_eq!(store.folder_prefix("history"), "history/");
    }

    #[test]
    fn a_listing_keeps_only_well_formed_names_under_the_prefix() {
        let xml = "<ListBucketResult>\
            <Contents><Key>ledger/history/abc-123.json</Key></Contents>\
            <Contents><Key>ledger/history/../ledger.json</Key></Contents>\
            <Contents><Key>ledger/history/sub/x.json</Key></Contents>\
            <Contents><Key>ledger/ledger.json</Key></Contents>\
            <Contents><Key>ledger/history/notes.txt</Key></Contents>\
            </ListBucketResult>";
        assert_eq!(listed_names(xml, "ledger/history/"), vec!["abc-123"]);
    }

    #[test]
    fn a_slot_is_its_own_key_and_refuses_a_path() {
        let store = S3Store::new("s3", config(None), credentials()).unwrap();
        let shelf = S3Shelf {
            prefix: store.folder_prefix("history"),
            store,
        };
        assert!(shelf.slot("../ledger").is_none());
        assert!(shelf.slot("a/b").is_none());
        assert!(shelf.slot("abc123").is_some());
    }
}
