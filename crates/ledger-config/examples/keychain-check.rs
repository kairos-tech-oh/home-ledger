//! Check that this machine has a usable keychain, and that a secret survives
//! a round trip through it.
//!
//!     cargo run -p ledger-config --example keychain-check
//!
//! Writes a throwaway entry and removes it again. Never prints a secret.

use ledger_config::{Keychain, Secret, Secrets};

fn main() {
    let keychain = Keychain::new();

    println!("keychain available: {}", keychain.available());

    let secret = Secret::AccessKey {
        access_key_id: "AKIA-ROUNDTRIP-CHECK".into(),
        secret_access_key: "this is thrown away".into(),
        session_token: None,
    };

    match keychain.set("__roundtrip__", &secret) {
        Ok(()) => println!("write: ok"),
        Err(e) => {
            println!("write failed: {e}");
            std::process::exit(1);
        }
    }

    match keychain.get("__roundtrip__") {
        Ok(Some(found)) if found == secret => println!("read back: matches"),
        Ok(Some(_)) => println!("read back: DIFFERENT from what was stored"),
        Ok(None) => println!("read back: nothing there, which should not happen"),
        Err(e) => println!("read failed: {e}"),
    }

    match keychain.forget("__roundtrip__") {
        Ok(()) => println!("cleaned up: ok"),
        Err(e) => println!("cleanup failed: {e}"),
    }

    match keychain.get("__roundtrip__") {
        Ok(None) => println!("after cleanup: gone, as it should be"),
        Ok(Some(_)) => println!("after cleanup: STILL THERE"),
        Err(e) => println!("after cleanup: {e}"),
    }
}
