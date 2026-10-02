//! How long creating and unlocking a key takes at the real cost.
//!
//!     cargo run --release -p ledger-store --example unlock-time

use ledger_store::sealed::{Kdf, create, unlock_with_passphrase};
use std::time::Instant;

fn main() {
    let started = Instant::now();
    let (key, _) = create("correct horse battery", Kdf::STANDARD).unwrap();
    println!("create (two wraps): {:?}", started.elapsed());
    let started = Instant::now();
    unlock_with_passphrase(&key.envelope, "correct horse battery").unwrap();
    println!("unlock: {:?}", started.elapsed());
}
