//! OAuth for a desktop app: PKCE, a loopback redirect, and token refresh.
//!
//! No client secret. A secret shipped inside an installed application is not
//! secret, which is why PKCE exists — the app proves it started the exchange
//! by holding a verifier only it knows, rather than by holding a password
//! anyone can extract from the binary.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::time::{Duration, SystemTime};

#[derive(Debug, thiserror::Error)]
pub enum OAuthError {
    #[error("the sign-in was refused: {0}")]
    Refused(String),
    #[error("the sign-in did not complete: {0}")]
    Incomplete(String),
    #[error("could not reach the sign-in service: {0}")]
    Unreachable(String),
    #[error(
        "this account granted no refresh token, so the app would have to ask \
         you to sign in again every hour"
    )]
    NoRefreshToken,
}

/// base64url without padding, as PKCE and JWTs use it.
fn base64url(bytes: &[u8]) -> String {
    const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        let take = chunk.len() + 1;
        for i in 0..take {
            out.push(ALPHABET[((n >> (18 - 6 * i)) & 0x3f) as usize] as char);
        }
    }
    out
}

/// One sign-in attempt's secret, and the challenge derived from it.
pub struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    pub fn new() -> Self {
        // Two v4 UUIDs is 244 bits of randomness rendered as unreserved
        // characters, comfortably inside PKCE's 43..128 range.
        let verifier = format!(
            "{}{}",
            uuid::Uuid::new_v4().simple(),
            uuid::Uuid::new_v4().simple()
        );
        let challenge = base64url(&Sha256::digest(verifier.as_bytes()));
        Self {
            verifier,
            challenge,
        }
    }
}

impl Default for Pkce {
    fn default() -> Self {
        Self::new()
    }
}

/// What a provider needs to be talked to. Held as data so the same code can
/// serve Google today and something else later.
#[derive(Clone, Debug)]
pub struct Provider {
    pub authorize_url: String,
    pub token_url: String,
    pub scope: String,
}

impl Provider {
    /// Google, asking only for files this app itself creates.
    ///
    /// `drive.file` is deliberate: it cannot see anything else in the Drive,
    /// which is both the right amount of access and the reason this needs no
    /// verification review from Google.
    pub fn google_drive() -> Self {
        Self {
            authorize_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            scope: "https://www.googleapis.com/auth/drive.file".into(),
        }
    }
}

fn encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

/// Where to send the browser.
pub fn authorize_url(
    provider: &Provider,
    client_id: &str,
    redirect_uri: &str,
    pkce: &Pkce,
    state: &str,
) -> String {
    format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope={}\
         &code_challenge={}&code_challenge_method=S256&state={}\
         &access_type=offline&prompt=consent",
        provider.authorize_url,
        encode(client_id),
        encode(redirect_uri),
        encode(&provider.scope),
        encode(&pkce.challenge),
        encode(state),
    )
}

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: Option<String>,
    refresh_token: Option<String>,
    expires_in: Option<u64>,
    error: Option<String>,
    error_description: Option<String>,
}

/// An access token and when it stops working.
#[derive(Clone, Debug)]
pub struct Access {
    pub token: String,
    pub expires_at: SystemTime,
}

impl Access {
    /// Treated as expired a minute early, so a request is not sent with a
    /// token that dies in flight.
    pub fn usable(&self) -> bool {
        SystemTime::now() + Duration::from_secs(60) < self.expires_at
    }
}

async fn post_form(
    client: &reqwest::Client,
    url: &str,
    form: &[(&str, &str)],
) -> Result<TokenResponse, OAuthError> {
    let response = client
        .post(url)
        .form(form)
        .send()
        .await
        .map_err(|e| OAuthError::Unreachable(e.to_string()))?;

    let body = response
        .text()
        .await
        .map_err(|e| OAuthError::Unreachable(e.to_string()))?;

    let parsed: TokenResponse = serde_json::from_str(&body)
        .map_err(|_| OAuthError::Incomplete("the reply was not readable".into()))?;

    if let Some(error) = &parsed.error {
        let detail = parsed.error_description.clone().unwrap_or_default();
        return Err(OAuthError::Refused(if detail.is_empty() {
            error.clone()
        } else {
            format!("{error}: {detail}")
        }));
    }
    Ok(parsed)
}

/// Trade the code the browser came back with for tokens.
pub async fn exchange_code(
    client: &reqwest::Client,
    provider: &Provider,
    client_id: &str,
    redirect_uri: &str,
    code: &str,
    pkce: &Pkce,
) -> Result<(Access, String), OAuthError> {
    let parsed = post_form(
        client,
        &provider.token_url,
        &[
            ("client_id", client_id),
            ("code", code),
            ("code_verifier", &pkce.verifier),
            ("redirect_uri", redirect_uri),
            ("grant_type", "authorization_code"),
        ],
    )
    .await?;

    let token = parsed
        .access_token
        .ok_or_else(|| OAuthError::Incomplete("no access token came back".into()))?;
    // Without this the app can only work for an hour and then has to
    // interrupt somebody, so it is a failure rather than a degraded mode.
    let refresh = parsed.refresh_token.ok_or(OAuthError::NoRefreshToken)?;

    Ok((
        Access {
            token,
            expires_at: expires_at(parsed.expires_in),
        },
        refresh,
    ))
}

/// Get a fresh access token from a stored refresh token.
pub async fn refresh(
    client: &reqwest::Client,
    provider: &Provider,
    client_id: &str,
    refresh_token: &str,
) -> Result<Access, OAuthError> {
    let parsed = post_form(
        client,
        &provider.token_url,
        &[
            ("client_id", client_id),
            ("refresh_token", refresh_token),
            ("grant_type", "refresh_token"),
        ],
    )
    .await?;

    Ok(Access {
        token: parsed
            .access_token
            .ok_or_else(|| OAuthError::Incomplete("no access token came back".into()))?,
        expires_at: expires_at(parsed.expires_in),
    })
}

fn expires_at(seconds: Option<u64>) -> SystemTime {
    SystemTime::now() + Duration::from_secs(seconds.unwrap_or(3600))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_matches_rfc_4648_vectors() {
        // Computed from the RFC's test strings, standard alphabet aside from
        // the two substituted characters and no padding.
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(b"foob"), "Zm9vYg");
        assert_eq!(base64url(b"fooba"), "Zm9vYmE");
        assert_eq!(base64url(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn base64url_never_emits_padding_or_the_url_unsafe_characters() {
        // Bytes chosen to produce + and / under the standard alphabet.
        let encoded = base64url(&[0xfb, 0xff, 0xfe]);
        assert!(!encoded.contains('+'), "{encoded}");
        assert!(!encoded.contains('/'), "{encoded}");
        assert!(!encoded.contains('='), "{encoded}");
    }

    #[test]
    fn a_challenge_is_the_hash_of_the_verifier_not_the_verifier() {
        // Sending the verifier would defeat the point of PKCE entirely.
        let pkce = Pkce::new();
        assert_ne!(pkce.challenge, pkce.verifier);
        assert_eq!(
            pkce.challenge,
            base64url(&Sha256::digest(pkce.verifier.as_bytes()))
        );
    }

    #[test]
    fn the_verifier_is_long_enough_and_uses_only_unreserved_characters() {
        let pkce = Pkce::new();
        assert!(
            (43..=128).contains(&pkce.verifier.len()),
            "{}",
            pkce.verifier.len()
        );
        assert!(pkce.verifier.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn two_attempts_never_share_a_verifier() {
        assert_ne!(Pkce::new().verifier, Pkce::new().verifier);
    }

    #[test]
    fn the_authorize_url_asks_for_offline_access_and_carries_the_challenge() {
        let pkce = Pkce::new();
        let url = authorize_url(
            &Provider::google_drive(),
            "client-123.apps.googleusercontent.com",
            "http://127.0.0.1:41234",
            &pkce,
            "state-abc",
        );

        // Without offline access there is no refresh token, and the app can
        // only work until the first hour is up.
        assert!(url.contains("access_type=offline"), "{url}");
        assert!(url.contains("code_challenge_method=S256"), "{url}");
        assert!(url.contains(&pkce.challenge), "{url}");
        assert!(
            !url.contains(&pkce.verifier),
            "the verifier must not leave this machine"
        );
        assert!(url.contains("state-abc"), "{url}");
        assert!(url.contains("drive.file"), "{url}");
    }

    #[test]
    fn the_scope_asks_only_for_files_this_app_creates() {
        let scope = Provider::google_drive().scope;
        assert!(scope.ends_with("drive.file"));
        // Anything broader can read the whole Drive and needs Google's review.
        assert!(!scope.contains("drive.readonly"));
        assert!(!scope.ends_with("auth/drive"));
    }

    #[test]
    fn a_token_about_to_expire_is_treated_as_expired() {
        // Sent a second before expiry, a token can die in flight.
        let nearly = Access {
            token: "t".into(),
            expires_at: SystemTime::now() + Duration::from_secs(30),
        };
        assert!(!nearly.usable());

        let fresh = Access {
            token: "t".into(),
            expires_at: SystemTime::now() + Duration::from_secs(3600),
        };
        assert!(fresh.usable());
    }

    #[test]
    fn a_redirect_uri_is_escaped_rather_than_pasted_in() {
        let url = authorize_url(
            &Provider::google_drive(),
            "id",
            "http://127.0.0.1:41234",
            &Pkce::new(),
            "s",
        );
        assert!(url.contains("http%3A%2F%2F127.0.0.1%3A41234"), "{url}");
    }
}

// -------------------------------------------------- the loopback redirect

use std::collections::HashMap;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

/// How long to wait for somebody to finish signing in before giving up.
const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(300);

/// Bytes read from the browser's request before it is refused. A redirect is
/// a couple of hundred bytes; anything larger is not one.
const MAX_REQUEST: usize = 8 * 1024;

/// A port on the loopback interface, waiting for the browser to come back.
pub struct Redirect {
    listener: TcpListener,
    pub uri: String,
}

/// Listen on a port the operating system picks, on the loopback interface
/// only. Google allows any port on 127.0.0.1 for a desktop client, which is
/// what makes this work without registering a fixed one.
pub async fn bind_loopback() -> Result<Redirect, OAuthError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .map_err(|e| OAuthError::Incomplete(format!("could not open a local port: {e}")))?;
    let port = listener
        .local_addr()
        .map_err(|e| OAuthError::Incomplete(e.to_string()))?
        .port();
    Ok(Redirect {
        listener,
        uri: format!("http://127.0.0.1:{port}"),
    })
}

/// Pull the query parameters out of an HTTP request line.
///
/// Only the first line is needed and only three values are read from it, so
/// this does not pretend to be an HTTP server.
fn params_of(request: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let Some(line) = request.lines().next() else {
        return out;
    };
    let Some(target) = line.split_whitespace().nth(1) else {
        return out;
    };
    let Some((_, query)) = target.split_once('?') else {
        return out;
    };
    for pair in query.split('&') {
        if let Some((key, value)) = pair.split_once('=') {
            out.insert(key.to_string(), decode(value));
        }
    }
    out
}

fn decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hex = std::str::from_utf8(&bytes[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(byte) => {
                        out.push(byte);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(bytes[i]);
                        i += 1;
                    }
                }
            }
            byte => {
                out.push(byte);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).to_string()
}

fn page(title: &str, message: &str) -> String {
    let body = format!(
        "<!doctype html><meta charset=utf-8><title>{title}</title>\
         <body style=\"font-family:system-ui;padding:3rem;max-width:32rem\">\
         <h1 style=\"font-size:1.2rem\">{title}</h1><p>{message}</p>"
    );
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

impl Redirect {
    /// Wait for the browser to arrive with a code.
    ///
    /// The `state` is checked before the code is accepted: without that, any
    /// page the person happens to visit could aim a code of its own at this
    /// port and have the app adopt somebody else's account.
    pub async fn wait_for_code(self, state: &str) -> Result<String, OAuthError> {
        let accept = tokio::time::timeout(SIGN_IN_TIMEOUT, self.listener.accept());
        let (mut socket, _) = accept
            .await
            .map_err(|_| OAuthError::Incomplete("the sign-in was not finished in time".into()))?
            .map_err(|e| OAuthError::Incomplete(e.to_string()))?;

        let mut buffer = vec![0u8; MAX_REQUEST];
        let read = socket
            .read(&mut buffer)
            .await
            .map_err(|e| OAuthError::Incomplete(e.to_string()))?;
        let request = String::from_utf8_lossy(&buffer[..read]).to_string();

        let params = params_of(&request);
        let outcome = check(&params, state);

        let reply = match &outcome {
            Ok(_) => page(
                "Signed in",
                "You can close this tab and go back to Home Ledger.",
            ),
            Err(e) => page("That did not work", &e.to_string()),
        };
        let _ = socket.write_all(reply.as_bytes()).await;
        let _ = socket.shutdown().await;

        outcome
    }
}

/// What came back, once. Separated so it can be tested without a socket.
fn check(params: &HashMap<String, String>, expected_state: &str) -> Result<String, OAuthError> {
    if let Some(error) = params.get("error") {
        return Err(OAuthError::Refused(error.clone()));
    }
    match params.get("state") {
        Some(state) if state == expected_state => {}
        // Never a code from a request that cannot prove this app started it.
        _ => {
            return Err(OAuthError::Refused(
                "that sign-in did not come from this app".into(),
            ));
        }
    }
    params
        .get("code")
        .filter(|code| !code.is_empty())
        .cloned()
        .ok_or_else(|| OAuthError::Incomplete("no code came back".into()))
}

/// Hand a URL to the desktop's browser.
///
/// Spawned directly rather than through a shell, so nothing in the URL can be
/// read as a command.
pub fn open_in_browser(url: &str) -> Result<(), OAuthError> {
    let mut command = if cfg!(target_os = "windows") {
        // Avoids `cmd /c start`, whose quoting rules treat the first quoted
        // argument as a window title and mangle URLs containing `&`.
        let mut c = std::process::Command::new("rundll32.exe");
        c.arg("url.dll,FileProtocolHandler").arg(url);
        c
    } else if cfg!(target_os = "macos") {
        let mut c = std::process::Command::new("open");
        c.arg(url);
        c
    } else {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(url);
        c
    };

    command
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| OAuthError::Incomplete(format!("could not open a browser: {e}")))
}

/// A one-off value tying a sign-in to this attempt.
pub fn new_state() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod redirect_tests {
    use super::*;

    fn params(query: &str) -> HashMap<String, String> {
        params_of(&format!(
            "GET /?{query} HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n"
        ))
    }

    #[test]
    fn a_code_is_read_out_of_the_request_line() {
        let found = params("code=abc123&state=xyz");
        assert_eq!(found.get("code").map(String::as_str), Some("abc123"));
        assert_eq!(found.get("state").map(String::as_str), Some("xyz"));
    }

    #[test]
    fn percent_escapes_are_undone() {
        let found = params("code=a%2Fb%2Bc&state=s");
        assert_eq!(found.get("code").map(String::as_str), Some("a/b+c"));
    }

    #[test]
    fn a_request_with_no_query_yields_nothing_rather_than_panicking() {
        assert!(params_of("GET / HTTP/1.1").is_empty());
        assert!(params_of("").is_empty());
        assert!(params_of("nonsense").is_empty());
    }

    #[test]
    fn a_code_is_accepted_only_with_the_matching_state() {
        assert_eq!(
            check(&params("code=abc&state=right"), "right").unwrap(),
            "abc"
        );
    }

    #[test]
    fn a_code_carrying_the_wrong_state_is_refused() {
        // Otherwise any page someone visits could aim a code at this port and
        // have the app quietly adopt a different account.
        assert!(check(&params("code=abc&state=wrong"), "right").is_err());
        assert!(check(&params("code=abc"), "right").is_err());
    }

    #[test]
    fn an_error_from_the_provider_is_reported_rather_than_read_as_a_code() {
        let refused = check(&params("error=access_denied&state=right"), "right");
        assert!(matches!(refused, Err(OAuthError::Refused(_))));
    }

    #[test]
    fn a_reply_with_no_code_is_incomplete() {
        assert!(check(&params("state=right"), "right").is_err());
        assert!(check(&params("code=&state=right"), "right").is_err());
    }

    #[tokio::test]
    async fn the_listener_binds_loopback_only() {
        let redirect = bind_loopback().await.expect("bound");
        // Binding anything else would expose the sign-in to the network.
        assert!(
            redirect.uri.starts_with("http://127.0.0.1:"),
            "{}",
            redirect.uri
        );
        assert!(!redirect.uri.contains("0.0.0.0"));
    }

    #[tokio::test]
    async fn two_sign_ins_get_different_ports_and_different_states() {
        let a = bind_loopback().await.unwrap();
        let b = bind_loopback().await.unwrap();
        assert_ne!(a.uri, b.uri);
        assert_ne!(new_state(), new_state());
    }
}
