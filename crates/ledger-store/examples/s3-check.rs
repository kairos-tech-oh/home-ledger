//! Exercise the S3 store against a real bucket, without changing anything.
//!
//!     cargo run -p ledger-store --example s3-check -- <bucket> <key> <region> [endpoint]
//!
//! Credentials come from a profile in `~/.aws/credentials`, named by
//! `AWS_PROFILE`. Two checks, both safe:
//!
//!   1. a read, which proves the signing, the credentials and the endpoint
//!   2. a write carrying a deliberately stale version, which must be refused
//!
//! The second is the interesting one. It exercises the whole conditional-write
//! path and provably stores nothing, because the precondition fails.

use ledger_store::store::{Expect, Store, Version};
use ledger_store::{Credentials, S3Config, S3Store};

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 3 {
        eprintln!("usage: s3-check <bucket> <key> <region> [endpoint]");
        std::process::exit(2);
    }

    let profile = std::env::var("AWS_PROFILE").unwrap_or_else(|_| "default".into());
    let credentials = match load_profile(&profile) {
        Ok(credentials) => credentials,
        Err(why) => {
            eprintln!("{why}");
            std::process::exit(1);
        }
    };
    println!("using profile {profile}");

    let store = S3Store::new(
        "s3",
        S3Config {
            bucket: args[0].clone(),
            key: args[1].clone(),
            region: args[2].clone(),
            endpoint: args.get(3).cloned(),
        },
        credentials,
    )
    .expect("configured");

    println!("health: {:?}", store.health().await);

    match store.load().await {
        Ok(Some(snapshot)) => {
            println!(
                "read ok: {} bytes, version {}",
                snapshot.body.len(),
                snapshot.version
            );

            // A write that must fail. If this succeeds, conditional writes are
            // not working and two machines could silently overwrite each other.
            let stale = Version("0000000000000000000000000000000f".into());
            match store
                .save(b"{\"selftest\":true}", Expect::Version(stale))
                .await
            {
                Err(ledger_store::StoreError::Conflict) => {
                    println!("conditional write: refused a stale version, as it must")
                }
                Ok(_) => {
                    eprintln!("conditional write: ACCEPTED A STALE VERSION — writes are not safe");
                    std::process::exit(1);
                }
                Err(e) => eprintln!("conditional write: inconclusive ({e})"),
            }
        }
        Ok(None) => println!("read ok: nothing stored at that key yet"),
        Err(e) => {
            eprintln!("read failed: {e}");
            std::process::exit(1);
        }
    }
}

/// Minimal INI reading, enough for an AWS credentials file. Never prints a
/// secret, including on the error paths.
fn load_profile(profile: &str) -> Result<Credentials, String> {
    let home = std::env::var("HOME").map_err(|_| "no HOME set".to_string())?;
    let path = format!("{home}/.aws/credentials");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {path}: {e}"))?;

    let mut current = String::new();
    let mut id = None;
    let mut secret = None;
    let mut token = None;

    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            current = name.to_string();
            continue;
        }
        if current != profile {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let value = value.trim().to_string();
            match key.trim() {
                "aws_access_key_id" => id = Some(value),
                "aws_secret_access_key" => secret = Some(value),
                "aws_session_token" => token = Some(value),
                _ => {}
            }
        }
    }

    match (id, secret) {
        (Some(access_key_id), Some(secret_access_key)) => Ok(Credentials {
            access_key_id,
            secret_access_key,
            session_token: token,
        }),
        _ => Err(format!("profile [{profile}] has no key pair in {path}")),
    }
}
