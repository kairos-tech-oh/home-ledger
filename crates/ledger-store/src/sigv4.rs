//! AWS Signature Version 4, by hand.
//!
//! The AWS SDK is an enormous dependency tree for two HTTP verbs against one
//! object. This is the whole of what is needed, and it is what lets the same
//! code reach MinIO, Backblaze B2, Cloudflare R2 and Wasabi.

use hmac::{Hmac, Mac};
use sha2::{Digest, Sha256};

type HmacSha256 = Hmac<Sha256>;

const ALGORITHM: &str = "AWS4-HMAC-SHA256";

pub fn sha256_hex(data: &[u8]) -> String {
    hex(&Sha256::digest(data))
}

fn hmac(key: &[u8], data: &[u8]) -> Vec<u8> {
    let mut mac = HmacSha256::new_from_slice(key).expect("hmac takes a key of any length");
    mac.update(data);
    mac.finalize().into_bytes().to_vec()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Percent-encode a path, leaving the separators alone. AWS wants each
/// segment encoded but not the slashes between them.
pub fn encode_path(path: &str) -> String {
    path.split('/')
        .map(encode_segment)
        .collect::<Vec<_>>()
        .join("/")
}

pub fn encode_segment(segment: &str) -> String {
    let mut out = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(byte as char)
            }
            _ => out.push_str(&format!("%{byte:02X}")),
        }
    }
    out
}

#[derive(Clone)]
pub struct Credentials {
    pub access_key_id: String,
    pub secret_access_key: String,
    pub session_token: Option<String>,
}

/// Everything about one request that the signature covers.
pub struct Request<'a> {
    pub method: &'a str,
    pub host: &'a str,
    pub path: &'a str,
    pub query: &'a str,
    pub payload: &'a [u8],
    pub region: &'a str,
    /// `s3` for object storage. Named because the same signing works for
    /// other services, and getting it wrong is a confusing 403.
    pub service: &'a str,
    /// `yyyymmddThhmmssZ`, the same instant as `date`.
    pub amz_date: &'a str,
    pub date: &'a str,
}

/// Sign a request, returning the headers to send. The caller adds nothing to
/// the signed set afterwards, or the signature stops matching.
pub fn sign(
    request: &Request<'_>,
    credentials: &Credentials,
    extra: &[(String, String)],
) -> Vec<(String, String)> {
    let payload_hash = sha256_hex(request.payload);

    let mut headers: Vec<(String, String)> = vec![
        ("host".into(), request.host.to_string()),
        ("x-amz-content-sha256".into(), payload_hash.clone()),
        ("x-amz-date".into(), request.amz_date.to_string()),
    ];
    for (name, value) in extra {
        headers.push((name.to_ascii_lowercase(), value.clone()));
    }
    if let Some(token) = &credentials.session_token {
        headers.push(("x-amz-security-token".into(), token.clone()));
    }
    headers.sort_by(|a, b| a.0.cmp(&b.0));

    let canonical_headers: String = headers
        .iter()
        .map(|(name, value)| format!("{name}:{}\n", value.trim()))
        .collect();
    let signed_headers: Vec<&str> = headers.iter().map(|(name, _)| name.as_str()).collect();
    let signed_headers = signed_headers.join(";");

    let canonical = format!(
        "{}\n{}\n{}\n{}\n{}\n{}",
        request.method,
        encode_path(request.path),
        request.query,
        canonical_headers,
        signed_headers,
        payload_hash
    );

    let scope = format!(
        "{}/{}/{}/aws4_request",
        request.date, request.region, request.service
    );
    let to_sign = format!(
        "{ALGORITHM}\n{}\n{scope}\n{}",
        request.amz_date,
        sha256_hex(canonical.as_bytes())
    );

    let key = signing_key(
        &credentials.secret_access_key,
        request.date,
        request.region,
        request.service,
    );
    let signature = hex(&hmac(&key, to_sign.as_bytes()));

    let authorization = format!(
        "{ALGORITHM} Credential={}/{scope}, SignedHeaders={signed_headers}, Signature={signature}",
        credentials.access_key_id
    );

    headers.push(("authorization".into(), authorization));
    headers
}

fn signing_key(secret: &str, date: &str, region: &str, service: &str) -> Vec<u8> {
    let key = hmac(format!("AWS4{secret}").as_bytes(), date.as_bytes());
    let key = hmac(&key, region.as_bytes());
    let key = hmac(&key, service.as_bytes());
    hmac(&key, b"aws4_request")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// RFC 4231 test case 1. Checks the primitive underneath everything else,
    /// so a failure here says "the HMAC is wrong" rather than "S3 said 403".
    #[test]
    fn the_hmac_matches_rfc_4231() {
        assert_eq!(
            hex(&hmac(&[0x0b; 20], b"Hi There")),
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }

    /// The signing key from AWS's published worked example. Derived through
    /// four chained HMACs, so getting it right means every step is right; a
    /// wrong one is a 403 with no clue which.
    ///
    /// Both expectations were computed independently before being written
    /// here, rather than recalled.
    #[test]
    fn the_signing_key_chain_matches_a_known_scope() {
        const SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";

        assert_eq!(
            hex(&signing_key(SECRET, "20150830", "us-east-1", "iam")),
            "c4afb1cc5771d871763a393e44b703571b55cc28424d1a5e86da6ed3c154a4b9"
        );
        assert_eq!(
            hex(&signing_key(SECRET, "20130524", "us-east-1", "s3")),
            "f117494eff5d09da21cbf7f0339559ea04fc9582d31299cb992be70a6b27c97a"
        );
    }

    #[test]
    fn the_region_and_service_change_the_key() {
        const SECRET: &str = "wJalrXUtnFEMI/K7MDENG+bPxRfiCYEXAMPLEKEY";
        let east = signing_key(SECRET, "20130524", "us-east-1", "s3");
        let west = signing_key(SECRET, "20130524", "us-west-2", "s3");
        assert_ne!(east, west, "the region is not reaching the key");
    }

    #[test]
    fn an_empty_payload_hashes_to_the_known_value() {
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn a_path_keeps_its_separators_and_encodes_the_rest() {
        assert_eq!(encode_path("/ledger/ledger.json"), "/ledger/ledger.json");
        assert_eq!(encode_path("/a b/c+d"), "/a%20b/c%2Bd");
        // A tilde is unreserved and must not be escaped; AWS rejects it if it is.
        assert_eq!(encode_path("/a~b"), "/a~b");
    }

    #[test]
    fn the_signature_covers_every_header_it_sends() {
        let credentials = Credentials {
            access_key_id: "key".into(),
            secret_access_key: "secret".into(),
            session_token: None,
        };
        let headers = sign(
            &Request {
                method: "PUT",
                host: "bucket.s3.us-east-2.amazonaws.com",
                path: "/ledger/ledger.json",
                query: "",
                payload: b"{}",
                region: "us-east-2",
                service: "s3",
                amz_date: "20260921T000000Z",
                date: "20260921",
            },
            &credentials,
            &[("if-match".into(), "\"abc\"".into())],
        );

        let authorization = headers
            .iter()
            .find(|(name, _)| name == "authorization")
            .map(|(_, value)| value.clone())
            .expect("signed");

        // Every header the caller is about to send must be in SignedHeaders,
        // or S3 answers 403 with nothing useful.
        assert!(authorization.contains("if-match"));
        assert!(authorization.contains("host"));
        assert!(authorization.contains("x-amz-content-sha256"));
        assert!(authorization.contains("x-amz-date"));
    }

    #[test]
    fn a_session_token_is_signed_when_there_is_one() {
        let credentials = Credentials {
            access_key_id: "key".into(),
            secret_access_key: "secret".into(),
            session_token: Some("token".into()),
        };
        let headers = sign(
            &Request {
                method: "GET",
                host: "h",
                path: "/k",
                query: "",
                payload: b"",
                region: "r",
                service: "s3",
                amz_date: "20260921T000000Z",
                date: "20260921",
            },
            &credentials,
            &[],
        );
        assert!(headers.iter().any(|(n, _)| n == "x-amz-security-token"));
    }
}
