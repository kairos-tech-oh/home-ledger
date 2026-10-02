//! Encryption of everything the app stores: the ledger on every backend, the
//! history and snapshot objects shared through it, and the local files.
//!
//! A sealed file is
//!
//! ```text
//! HLSEAL1\n | header length (u32, big endian) | header JSON | ciphertext
//! ```
//!
//! The ciphertext is XChaCha20-Poly1305 under a random 256-bit data key, with
//! the magic, length and header as associated data, so tampering with any of
//! them, or with the ciphertext, fails to open rather than reading wrong.
//!
//! The header carries the data key twice, wrapped: once under a key stretched
//! from the passphrase with Argon2id, once under one stretched from a recovery
//! code. That is how a second machine, or a forgotten passphrase, opens the
//! same data: nothing but the passphrase or the code is needed. Each machine
//! keeps the unwrapped data key in its OS keychain so it is not asked again.
//!
//! A file that does not start with the magic is plain JSON from before
//! encryption was turned on, and reads as it is. Once on, nothing is written
//! plain again.

use crate::store::{
    Capabilities, Expect, Health, Shelf, Snapshot, Store, StoreError, StoreId, StoreKind, Version,
};
use async_trait::async_trait;
use chacha20poly1305::aead::rand_core::RngCore;
use chacha20poly1305::aead::{Aead, KeyInit, OsRng, Payload};
use chacha20poly1305::{XChaCha20Poly1305, XNonce};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, RwLock};
use zeroize::Zeroizing;

pub const MAGIC: &[u8; 8] = b"HLSEAL1\n";
const KEY_AAD: &[u8] = b"home-ledger data key v1";

/// Argon2id cost. Stored with each wrapped key, so it can be raised later
/// without stranding data sealed under the old figure.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kdf {
    pub memory_kib: u32,
    pub passes: u32,
    pub lanes: u32,
}

impl Kdf {
    /// About half a second on a laptop: slow enough that guessing passphrases
    /// costs real time, quick enough that unlocking does not.
    pub const STANDARD: Kdf = Kdf {
        memory_kib: 64 * 1024,
        passes: 3,
        lanes: 1,
    };
}

/// The data key under one secret.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Wrapped {
    pub salt: String,
    pub nonce: String,
    pub key: String,
}

/// Everything needed to recover the data key from the passphrase or the code.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Envelope {
    pub v: u32,
    /// Names the data key, so a machine knows whether the key it holds opens
    /// a file, without trying.
    pub key_id: String,
    pub kdf: Kdf,
    pub passphrase: Wrapped,
    pub recovery: Wrapped,
}

#[derive(Serialize, Deserialize)]
struct Header {
    v: u32,
    envelope: Envelope,
    nonce: String,
}

/// An unwrapped data key. Its bytes are wiped when it is dropped.
pub struct Key {
    pub id: String,
    bytes: Zeroizing<[u8; 32]>,
    pub envelope: Envelope,
}

impl std::fmt::Debug for Key {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Key")
            .field("id", &self.id)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum SealError {
    #[error("that passphrase does not open this ledger")]
    WrongPassphrase,
    #[error("that recovery code does not open this ledger")]
    WrongRecoveryCode,
    #[error("a passphrase needs at least {0} characters")]
    TooShort(usize),
    #[error("the encrypted data is damaged or has been altered")]
    Damaged,
}

pub const PASSPHRASE_MIN: usize = 10;

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn unhex(text: &str) -> Option<Vec<u8>> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(text.get(i..i + 2)?, 16).ok())
        .collect()
}

fn random<const N: usize>() -> [u8; N] {
    let mut out = [0u8; N];
    OsRng.fill_bytes(&mut out);
    out
}

fn stretch(secret: &[u8], salt: &[u8], kdf: Kdf) -> Zeroizing<[u8; 32]> {
    let params = argon2::Params::new(kdf.memory_kib, kdf.passes, kdf.lanes, Some(32))
        .expect("argon2 parameters are fixed and valid");
    let argon = argon2::Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
    let mut out = Zeroizing::new([0u8; 32]);
    argon
        .hash_password_into(secret, salt, out.as_mut())
        .expect("argon2 accepts a 16-byte salt and 32-byte output");
    out
}

fn wrap(data_key: &[u8; 32], secret: &[u8], kdf: Kdf) -> Wrapped {
    let salt: [u8; 16] = random();
    let nonce: [u8; 24] = random();
    let kek = stretch(secret, &salt, kdf);
    let cipher = XChaCha20Poly1305::new(kek.as_ref().into());
    let key = cipher
        .encrypt(
            XNonce::from_slice(&nonce),
            Payload {
                msg: data_key,
                aad: KEY_AAD,
            },
        )
        .expect("encrypting 32 bytes cannot fail");
    Wrapped {
        salt: hex(&salt),
        nonce: hex(&nonce),
        key: hex(&key),
    }
}

fn unwrap(w: &Wrapped, secret: &[u8], kdf: Kdf) -> Option<Zeroizing<[u8; 32]>> {
    let kek = stretch(secret, &unhex(&w.salt)?, kdf);
    let cipher = XChaCha20Poly1305::new(kek.as_ref().into());
    let nonce = unhex(&w.nonce)?;
    if nonce.len() != 24 {
        return None;
    }
    let plain = Zeroizing::new(
        cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &unhex(&w.key)?,
                    aad: KEY_AAD,
                },
            )
            .ok()?,
    );
    let mut out = Zeroizing::new([0u8; 32]);
    if plain.len() != 32 {
        return None;
    }
    out.copy_from_slice(&plain);
    Some(out)
}

// Crockford's base32: no I, L, O or U, so a code copied by hand is not misread.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

/// A recovery code, as shown: 32 characters in groups of four.
fn recovery_code() -> String {
    let bytes: [u8; 20] = random();
    let mut bits: u64 = 0;
    let mut have = 0;
    let mut chars = String::new();
    for b in bytes {
        bits = (bits << 8) | u64::from(b);
        have += 8;
        while have >= 5 {
            have -= 5;
            chars.push(ALPHABET[((bits >> have) & 31) as usize] as char);
        }
    }
    chars
        .as_bytes()
        .chunks(4)
        .map(|c| std::str::from_utf8(c).unwrap())
        .collect::<Vec<_>>()
        .join("-")
}

/// A recovery code as typed: any case, with or without dashes or spaces, and
/// the letters people mistake for digits read as the digits.
pub fn normalise_recovery_code(typed: &str) -> String {
    typed
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| match c.to_ascii_uppercase() {
            'O' => '0',
            'I' | 'L' => '1',
            other => other,
        })
        .collect()
}

/// A new data key, wrapped under the passphrase and a fresh recovery code.
/// The code is returned once, to be shown once.
pub fn create(passphrase: &str, kdf: Kdf) -> Result<(Key, String), SealError> {
    if passphrase.chars().count() < PASSPHRASE_MIN {
        return Err(SealError::TooShort(PASSPHRASE_MIN));
    }
    let bytes = Zeroizing::new(random::<32>());
    let code = recovery_code();
    let envelope = Envelope {
        v: 1,
        key_id: hex(&random::<8>()),
        kdf,
        passphrase: wrap(&bytes, passphrase.as_bytes(), kdf),
        recovery: wrap(&bytes, normalise_recovery_code(&code).as_bytes(), kdf),
    };
    Ok((
        Key {
            id: envelope.key_id.clone(),
            bytes,
            envelope,
        },
        code,
    ))
}

pub fn unlock_with_passphrase(envelope: &Envelope, passphrase: &str) -> Result<Key, SealError> {
    let bytes = unwrap(&envelope.passphrase, passphrase.as_bytes(), envelope.kdf)
        .ok_or(SealError::WrongPassphrase)?;
    Ok(Key {
        id: envelope.key_id.clone(),
        bytes,
        envelope: envelope.clone(),
    })
}

pub fn unlock_with_recovery_code(envelope: &Envelope, code: &str) -> Result<Key, SealError> {
    let bytes = unwrap(
        &envelope.recovery,
        normalise_recovery_code(code).as_bytes(),
        envelope.kdf,
    )
    .ok_or(SealError::WrongRecoveryCode)?;
    Ok(Key {
        id: envelope.key_id.clone(),
        bytes,
        envelope: envelope.clone(),
    })
}

impl Key {
    /// For the keychain, so this machine is not asked again.
    pub fn export(&self) -> String {
        hex(self.bytes.as_ref())
    }

    /// The key this machine kept. Nothing here proves it is the right one; a
    /// wrong key simply fails to open what it is used on, as damaged data does.
    pub fn import(envelope: &Envelope, exported: &str) -> Option<Key> {
        let raw = Zeroizing::new(unhex(exported)?);
        if raw.len() != 32 {
            return None;
        }
        let mut bytes = Zeroizing::new([0u8; 32]);
        bytes.copy_from_slice(&raw);
        Some(Key {
            id: envelope.key_id.clone(),
            bytes,
            envelope: envelope.clone(),
        })
    }

    pub fn seal(&self, plain: &[u8]) -> Vec<u8> {
        let nonce: [u8; 24] = random();
        let header = serde_json::to_vec(&Header {
            v: 1,
            envelope: self.envelope.clone(),
            nonce: hex(&nonce),
        })
        .expect("a header always serialises");
        let mut out = Vec::with_capacity(MAGIC.len() + 4 + header.len() + plain.len() + 16);
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&(header.len() as u32).to_be_bytes());
        out.extend_from_slice(&header);
        let cipher = XChaCha20Poly1305::new(self.bytes.as_ref().into());
        let sealed = cipher
            .encrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: plain,
                    aad: &out,
                },
            )
            .expect("encryption of an in-memory buffer cannot fail");
        out.extend_from_slice(&sealed);
        out
    }

    pub fn open(&self, data: &[u8]) -> Result<Vec<u8>, SealError> {
        let (header, aad_len) = parse(data).ok_or(SealError::Damaged)?;
        if header.envelope.key_id != self.id {
            return Err(SealError::Damaged);
        }
        let nonce = unhex(&header.nonce)
            .filter(|n| n.len() == 24)
            .ok_or(SealError::Damaged)?;
        let cipher = XChaCha20Poly1305::new(self.bytes.as_ref().into());
        cipher
            .decrypt(
                XNonce::from_slice(&nonce),
                Payload {
                    msg: &data[aad_len..],
                    aad: &data[..aad_len],
                },
            )
            .map_err(|_| SealError::Damaged)
    }
}

pub fn is_sealed(data: &[u8]) -> bool {
    data.starts_with(MAGIC)
}

fn parse(data: &[u8]) -> Option<(Header, usize)> {
    if !is_sealed(data) || data.len() < MAGIC.len() + 4 {
        return None;
    }
    let len_at = MAGIC.len();
    let len = u32::from_be_bytes(data[len_at..len_at + 4].try_into().ok()?) as usize;
    let end = len_at + 4 + len;
    let header: Header = serde_json::from_slice(data.get(len_at + 4..end)?).ok()?;
    Some((header, end))
}

/// The envelope a sealed file carries, for unlocking it.
pub fn envelope_of(data: &[u8]) -> Option<Envelope> {
    parse(data).map(|(h, _)| h.envelope)
}

/// What this machine holds: whether new data must be sealed, and the key to
/// do it with once unlocked. Shared by every store, the outbox and the local
/// files, so one unlock opens them all.
#[derive(Default)]
pub struct Vault {
    state: RwLock<State>,
}

#[derive(Default)]
struct State {
    /// Encryption is on: nothing may be written plain.
    required: bool,
    key: Option<Arc<Key>>,
    /// The envelope of the last sealed file met without a key, so the
    /// passphrase prompt has something to unlock.
    waiting: Option<Envelope>,
}

impl Vault {
    pub fn new() -> Arc<Vault> {
        Arc::new(Vault::default())
    }

    pub fn set(&self, required: bool, key: Option<Key>) {
        let mut s = self.state.write().unwrap();
        s.required = required;
        s.key = key.map(Arc::new);
        if s.key.is_some() {
            s.waiting = None;
        }
    }

    pub fn required(&self) -> bool {
        self.state.read().unwrap().required
    }

    pub fn key_id(&self) -> Option<String> {
        self.state
            .read()
            .unwrap()
            .key
            .as_ref()
            .map(|k| k.id.clone())
    }

    pub fn unlocked(&self) -> bool {
        self.state.read().unwrap().key.is_some()
    }

    /// An envelope a locked read ran into, waiting for the passphrase.
    pub fn waiting(&self) -> Option<Envelope> {
        self.state.read().unwrap().waiting.clone()
    }

    /// Plain when encryption is off; sealed when it is on; refused when it is
    /// on and this machine has not been unlocked, so nothing leaks plain.
    pub fn seal(&self, plain: &[u8]) -> Result<Vec<u8>, StoreError> {
        let s = self.state.read().unwrap();
        match (&s.key, s.required) {
            (Some(key), true) => Ok(key.seal(plain)),
            (None, true) => Err(StoreError::Locked),
            (_, false) => Ok(plain.to_vec()),
        }
    }

    /// Plain data as it is; sealed data opened with the key, or refused and
    /// remembered for unlocking when there is none.
    pub fn open(&self, data: &[u8]) -> Result<Vec<u8>, StoreError> {
        if !is_sealed(data) {
            return Ok(data.to_vec());
        }
        let key = self.state.read().unwrap().key.clone();
        match key {
            Some(key) if envelope_of(data).is_some_and(|e| e.key_id == key.id) => key
                .open(data)
                .map_err(|e| StoreError::Corrupt(e.to_string())),
            _ => {
                if let Some(envelope) = envelope_of(data) {
                    self.state.write().unwrap().waiting = Some(envelope);
                }
                Err(StoreError::Locked)
            }
        }
    }
}

/// A store whose contents are sealed. Everything else (health, capabilities,
/// versions and conditional writes) is the inner store's own.
pub struct Sealed {
    inner: Arc<dyn Store>,
    vault: Arc<Vault>,
}

impl Sealed {
    pub fn wrap(inner: Arc<dyn Store>, vault: Arc<Vault>) -> Arc<dyn Store> {
        Arc::new(Sealed { inner, vault })
    }
}

#[async_trait]
impl Store for Sealed {
    fn id(&self) -> &StoreId {
        self.inner.id()
    }
    fn kind(&self) -> StoreKind {
        self.inner.kind()
    }
    fn capabilities(&self) -> Capabilities {
        self.inner.capabilities()
    }
    async fn health(&self) -> Health {
        self.inner.health().await
    }
    async fn load(&self) -> Result<Option<Snapshot>, StoreError> {
        match self.inner.load().await? {
            Some(s) => Ok(Some(Snapshot {
                body: self.vault.open(&s.body)?,
                version: s.version,
            })),
            None => Ok(None),
        }
    }
    async fn save(&self, body: &[u8], expect: Expect) -> Result<Version, StoreError> {
        let sealed = self.vault.seal(body)?;
        self.inner.save(&sealed, expect).await
    }
    fn shelf(&self, folder: &str) -> Option<Arc<dyn Shelf>> {
        let inner = self.inner.shelf(folder)?;
        Some(Arc::new(SealedShelf {
            inner,
            vault: self.vault.clone(),
        }))
    }
}

struct SealedShelf {
    inner: Arc<dyn Shelf>,
    vault: Arc<Vault>,
}

#[async_trait]
impl Shelf for SealedShelf {
    async fn names(&self) -> Result<Vec<String>, StoreError> {
        self.inner.names().await
    }
    fn slot(&self, name: &str) -> Option<Arc<dyn Store>> {
        Some(Sealed::wrap(self.inner.slot(name)?, self.vault.clone()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Cheap enough for tests; the real figure is [`Kdf::STANDARD`].
    const QUICK: Kdf = Kdf {
        memory_kib: 64,
        passes: 1,
        lanes: 1,
    };

    fn fresh() -> (Key, String) {
        create("correct horse battery", QUICK).unwrap()
    }

    #[test]
    fn sealed_data_opens_with_its_key_and_hides_what_it_holds() {
        let (key, _) = fresh();
        let plain = br#"{"accounts":[{"name":"Checking","total":473.97}]}"#;
        let sealed = key.seal(plain);
        assert!(is_sealed(&sealed));
        assert!(!sealed.windows(8).any(|w| w == b"Checking"));
        assert_eq!(key.open(&sealed).unwrap(), plain);
        // The same data sealed twice is never the same bytes.
        assert_ne!(key.seal(plain), sealed);
    }

    #[test]
    fn any_change_to_a_sealed_file_is_caught() {
        let (key, _) = fresh();
        let sealed = key.seal(b"{\"a\":1}");
        for at in [3, 12, sealed.len() / 2, sealed.len() - 1] {
            let mut bent = sealed.clone();
            bent[at] ^= 1;
            assert!(key.open(&bent).is_err(), "byte {at}");
        }
    }

    #[test]
    fn the_passphrase_or_the_recovery_code_opens_it_on_another_machine() {
        let (key, code) = fresh();
        let sealed = key.seal(b"ledger");
        let envelope = envelope_of(&sealed).unwrap();

        let by_pass = unlock_with_passphrase(&envelope, "correct horse battery").unwrap();
        assert_eq!(by_pass.open(&sealed).unwrap(), b"ledger");

        // Typed carelessly: lower case, no dashes, O for 0.
        let typed = code.replace('-', " ").to_lowercase().replace('0', "o");
        let by_code = unlock_with_recovery_code(&envelope, &typed).unwrap();
        assert_eq!(by_code.open(&sealed).unwrap(), b"ledger");

        assert_eq!(
            unlock_with_passphrase(&envelope, "wrong horse battery").unwrap_err(),
            SealError::WrongPassphrase
        );
        assert_eq!(
            unlock_with_recovery_code(&envelope, "AAAA-BBBB").unwrap_err(),
            SealError::WrongRecoveryCode
        );
    }

    #[test]
    fn a_kept_key_round_trips_through_the_keychain_form() {
        let (key, _) = fresh();
        let again = Key::import(&key.envelope, &key.export()).unwrap();
        let sealed = key.seal(b"x");
        assert_eq!(again.open(&sealed).unwrap(), b"x");
        assert!(Key::import(&key.envelope, "nothex").is_none());
    }

    #[test]
    fn a_short_passphrase_is_refused() {
        assert_eq!(
            create("short", QUICK).unwrap_err(),
            SealError::TooShort(PASSPHRASE_MIN)
        );
    }

    #[test]
    fn recovery_codes_read_cleanly() {
        let (_, code) = fresh();
        assert_eq!(code.len(), 32 + 7, "{code}");
        assert!(
            code.chars()
                .all(|c| c == '-' || ALPHABET.contains(&(c as u8)))
        );
    }

    #[test]
    fn the_vault_never_writes_plain_once_encryption_is_on() {
        let vault = Vault::new();
        assert_eq!(vault.seal(b"x").unwrap(), b"x", "off: plain");
        vault.set(true, None);
        assert!(
            matches!(vault.seal(b"x"), Err(StoreError::Locked)),
            "on and locked: refused"
        );
        let (key, _) = fresh();
        vault.set(true, Some(key));
        let sealed = vault.seal(b"x").unwrap();
        assert!(is_sealed(&sealed));
        assert_eq!(vault.open(&sealed).unwrap(), b"x");
        assert_eq!(
            vault.open(b"{\"plain\":true}").unwrap(),
            b"{\"plain\":true}"
        );
    }

    #[test]
    fn a_locked_read_remembers_what_to_unlock() {
        let (key, _) = fresh();
        let sealed = key.seal(b"x");
        let vault = Vault::new();
        assert!(matches!(vault.open(&sealed), Err(StoreError::Locked)));
        assert_eq!(vault.waiting().unwrap().key_id, key.id);
    }
}
