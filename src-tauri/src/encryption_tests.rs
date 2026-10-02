//! Encryption end to end: two machines sharing one store, one turning it on.

use crate::commands::apply_value;
use crate::encryption::{disable, enable, status, unlock};
use crate::state::AppState;
use ledger_config::{Config, Places, Settings, StoreConfig};
use ledger_store::sealed::{Kdf, is_sealed};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Cheap enough for tests; the app uses [`Kdf::STANDARD`].
const QUICK: Kdf = Kdf {
    memory_kib: 64,
    passes: 1,
    lanes: 1,
};
const PASSPHRASE: &str = "correct horse battery";

fn machine(root: &Path, name: &str, shared: &Path) -> AppState {
    let places = Places {
        config_file: root.join(name).join("config.json"),
        data_dir: root.join(name).join("data"),
    };
    places
        .save_config(&Config {
            stores: vec![StoreConfig {
                id: "shared".into(),
                label: "shared".into(),
                settings: Settings::Local {
                    path: shared.to_path_buf(),
                },
                accept_risk: false,
            }],
            device: name.into(),
            setup_complete: true,
            ..Config::default()
        })
        .unwrap();
    AppState::open(
        places,
        Arc::new(ledger_config::secrets::InMemory::default()),
        true,
        || None,
    )
    .unwrap()
}

fn shared(root: &Path) -> PathBuf {
    let dir = root.join("shared");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join("ledger.json")
}

async fn add_type(state: &AppState, name: &str) {
    apply_value(
        state,
        json!({"op": "type-add", "list": "budget", "name": name}),
    )
    .await
    .unwrap()
    .expect("the edit applied");
}

async fn types(state: &AppState) -> Vec<String> {
    let loaded = state.live().await.engine.load().await.unwrap();
    ledger_writer::read(&loaded.snapshot.unwrap().body)
        .unwrap()
        .budget_types
}

fn bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

fn holds(path: &Path, text: &str) -> bool {
    bytes(path)
        .windows(text.len())
        .any(|w| w == text.as_bytes())
}

#[tokio::test]
async fn once_on_nothing_is_readable_on_disk_and_another_machine_needs_the_passphrase() {
    let root = tempfile::tempdir().unwrap();
    let ledger = shared(root.path());
    let desk = machine(root.path(), "desk", &ledger);
    add_type(&desk, "Vacation fund").await;
    assert!(holds(&ledger, "Vacation fund"), "plain before");

    let on = enable(&desk, PASSPHRASE.into(), QUICK).await.unwrap();
    assert!(!on.recovery_code.is_empty());

    // The shared ledger, and this machine's history, are sealed.
    assert!(is_sealed(&bytes(&ledger)));
    assert!(!holds(&ledger, "Vacation fund"));
    let audit = root.path().join("desk/data/audit.json");
    assert!(is_sealed(&bytes(&audit)), "local history sealed");
    // This machine still reads it, and its edits land sealed.
    add_type(&desk, "Car repairs").await;
    assert!(types(&desk).await.contains(&"Car repairs".to_string()));
    assert!(!holds(&ledger, "Car repairs"));

    // A second machine sees a locked ledger until it is given the passphrase.
    let laptop = machine(root.path(), "laptop", &ledger);
    assert!(laptop.live().await.engine.load().await.is_err());
    assert!(status(&laptop).await.unwrap().needs_unlock);
    assert!(unlock(&laptop, "not the passphrase".into()).await.is_err());
    unlock(&laptop, PASSPHRASE.into()).await.unwrap();
    assert!(types(&laptop).await.contains(&"Vacation fund".to_string()));
    let s = status(&laptop).await.unwrap();
    assert!(s.enabled && s.unlocked && !s.needs_unlock);

    // Its edits are sealed too, and the first machine reads them.
    add_type(&laptop, "Gifts").await;
    assert!(!holds(&ledger, "Gifts"));
    assert!(types(&desk).await.contains(&"Gifts".to_string()));

    // A third, with only the recovery code.
    let spare = machine(root.path(), "spare", &ledger);
    unlock(&spare, on.recovery_code.to_lowercase())
        .await
        .unwrap();
    assert!(types(&spare).await.contains(&"Gifts".to_string()));
}

#[tokio::test]
async fn a_machine_that_kept_its_key_opens_at_the_next_launch_without_asking() {
    let root = tempfile::tempdir().unwrap();
    let ledger = shared(root.path());
    let secrets = Arc::new(ledger_config::secrets::InMemory::default());
    let places = Places {
        config_file: root.path().join("desk/config.json"),
        data_dir: root.path().join("desk/data"),
    };
    places
        .save_config(&Config {
            stores: vec![StoreConfig {
                id: "shared".into(),
                label: "shared".into(),
                settings: Settings::Local {
                    path: ledger.clone(),
                },
                accept_risk: false,
            }],
            device: "desk".into(),
            setup_complete: true,
            ..Config::default()
        })
        .unwrap();
    let first = AppState::open(places.clone(), secrets.clone(), true, || None).unwrap();
    add_type(&first, "Vacation fund").await;
    enable(&first, PASSPHRASE.into(), QUICK).await.unwrap();
    drop(first);

    let again = AppState::open(places, secrets, true, || None).unwrap();
    assert!(again.vault.unlocked(), "the kept key was found");
    assert!(types(&again).await.contains(&"Vacation fund".to_string()));
}

#[tokio::test]
async fn turning_it_off_needs_the_passphrase_and_leaves_everything_plain() {
    let root = tempfile::tempdir().unwrap();
    let ledger = shared(root.path());
    let desk = machine(root.path(), "desk", &ledger);
    add_type(&desk, "Vacation fund").await;
    enable(&desk, PASSPHRASE.into(), QUICK).await.unwrap();

    assert!(disable(&desk, "wrong passphrase!".into()).await.is_err());
    assert!(
        is_sealed(&bytes(&ledger)),
        "a wrong passphrase changed nothing"
    );

    disable(&desk, PASSPHRASE.into()).await.unwrap();
    assert!(holds(&ledger, "Vacation fund"));
    assert!(!is_sealed(&bytes(
        &root.path().join("desk/data/audit.json")
    )));
    let s = status(&desk).await.unwrap();
    assert!(!s.enabled && !s.needs_unlock);
    add_type(&desk, "Plain again").await;
    assert!(holds(&ledger, "Plain again"));
}

#[tokio::test]
async fn a_second_enable_is_refused() {
    let root = tempfile::tempdir().unwrap();
    let ledger = shared(root.path());
    let desk = machine(root.path(), "desk", &ledger);
    add_type(&desk, "x").await;
    enable(&desk, PASSPHRASE.into(), QUICK).await.unwrap();
    assert!(enable(&desk, PASSPHRASE.into(), QUICK).await.is_err());
}
