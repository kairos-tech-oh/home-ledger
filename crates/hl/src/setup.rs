//! Setting a machine up and looking after it: status, sync, naming, the
//! encryption key, and the jobs a scheduled task runs.

use crate::out::{Failure, Outcome, Table, show};
use crate::session::Session;
use ledger_config::{Secret, Settings, StoreConfig};
use serde_json::json;

pub async fn status(s: &Session) -> Outcome {
    let setup = ledger_app::storage::setup(&s.state)
        .await
        .map_err(Failure::from_app)?;
    let stores = ledger_app::commands::stores(&s.state)
        .await
        .map_err(Failure::from_app)?;
    let sync = ledger_app::commands::sync_state(&s.state)
        .await
        .map_err(Failure::from_app)?;
    let lock = ledger_app::encryption::status(&s.state)
        .await
        .map_err(Failure::from_app)?;
    let report = json!({
        "version": env!("CARGO_PKG_VERSION"),
        "machine": setup.device,
        "machineIsDefault": setup.device_is_default,
        "install": setup.install,
        "setUp": setup.setup_complete,
        "sync": sync,
        "encryption": lock,
        "stores": stores,
        "problems": setup.problems,
    });
    show(s.json, &report, |_| {
        println!("hl {}", env!("CARGO_PKG_VERSION"));
        println!(
            "machine     {}{}",
            setup.device,
            if setup.device_is_default {
                "  (a default name: `hl name \"…\"` to tell it apart)"
            } else {
                ""
            }
        );
        println!("install     {}", setup.install);
        println!(
            "set up      {}",
            if setup.setup_complete {
                "yes"
            } else {
                "no: run `hl init`"
            }
        );
        println!(
            "sync        {}",
            match &sync {
                ledger_store::SyncState::Synced { .. } => "up to date".to_string(),
                ledger_store::SyncState::Behind { queued, reason } =>
                    format!("{queued} waiting: {reason}"),
                ledger_store::SyncState::Blocked { queued, reason } =>
                    format!("{queued} blocked: {reason}"),
                ledger_store::SyncState::Unconfigured => "no store".to_string(),
            }
        );
        println!(
            "encryption  {}",
            match (lock.enabled, lock.unlocked, lock.needs_unlock) {
                (_, _, true) => "on, locked: run `hl unlock`",
                (true, true, _) => "on",
                _ => "off",
            }
        );
        println!();
        let mut t = Table::new(&["Store", "Kind", "Role", "Health"]);
        for st in &stores {
            t.row(vec![
                st.id.0.clone(),
                format!("{:?}", st.kind).to_lowercase(),
                format!("{:?}", st.role).to_lowercase(),
                match &st.health {
                    ledger_store::Health::Reachable => "reachable".to_string(),
                    ledger_store::Health::Denied(why) => format!("refused: {why}"),
                    ledger_store::Health::Unreachable(why) => format!("unreachable: {why}"),
                },
            ]);
        }
        t.print();
        for p in &setup.problems {
            println!("problem: {p}");
        }
    })
}

pub async fn sync(s: &Session) -> Outcome {
    let state = ledger_app::commands::flush(&s.state)
        .await
        .map_err(Failure::from_app)?;
    s.state.finish_background().await;
    show(s.json, &state, |state| match state {
        ledger_store::SyncState::Synced { .. } => println!("up to date"),
        other => println!("{other:?}"),
    })?;
    match state {
        ledger_store::SyncState::Behind { reason, .. }
        | ledger_store::SyncState::Blocked { reason, .. } => Err(Failure::Queued(reason)),
        _ => Ok(()),
    }
}

pub async fn name(s: &Session, name: &str) -> Outcome {
    let setup = ledger_app::storage::rename_device(&s.state, name.into())
        .await
        .map_err(|e| Failure::Refused(e.to_string()))?;
    show(s.json, &json!({ "machine": setup.device }), |_| {
        println!("this machine is now \"{}\" in the history", setup.device)
    })
}

/// From `LEDGER_PASSPHRASE`, or asked for without echoing. Never an argument,
/// which other users of the machine could see.
fn passphrase(prompt: &str) -> Result<String, Failure> {
    if let Ok(p) = std::env::var("LEDGER_PASSPHRASE")
        && !p.is_empty()
    {
        return Ok(p);
    }
    rpassword::prompt_password(prompt).map_err(|_| {
        Failure::Usage("set LEDGER_PASSPHRASE, or run hl in a terminal to be asked".into())
    })
}

pub async fn unlock(s: &Session) -> Outcome {
    let secret = passphrase("Passphrase or recovery code: ")?;
    let kept = ledger_app::encryption::unlock(&s.state, secret)
        .await
        .map_err(|e| Failure::Refused(e.to_string()))?;
    show(s.json, &json!({ "unlocked": true, "kept": kept }), |_| {
        if kept {
            println!("unlocked, and the key is kept on this machine");
        } else {
            println!("unlocked for now; this machine has no keychain, so it will ask again");
        }
    })
}

pub async fn lock(s: &Session) -> Outcome {
    ledger_app::encryption::lock(&s.state)
        .await
        .map_err(Failure::from_app)?;
    show(s.json, &json!({ "locked": true }), |_| {
        println!("the key is forgotten on this machine; `hl unlock` to use the ledger here again")
    })
}

pub async fn snapshot(s: &Session) -> Outcome {
    let (today, _) = Session::today();
    let took = ledger_app::snapshots::take_snapshot(&s.state, today)
        .await
        .map_err(Failure::from_app)?;
    s.state.finish_background().await;
    show(s.json, &json!({ "taken": took }), |_| {
        println!(
            "{}",
            if took {
                "today's point taken"
            } else {
                "today already has a point"
            }
        )
    })
}

pub async fn prices_refresh(s: &Session) -> Outcome {
    if s.dry_run {
        return Err(Failure::Usage(
            "a price refresh asks the quote service; it has no dry run".into(),
        ));
    }
    let r = ledger_app::quotes::refresh_prices(&s.state)
        .await
        .map_err(Failure::from_app)?;
    s.state.finish_background().await;
    show(s.json, &r, |r| {
        println!(
            "{} of {} priced, {} failed",
            r.priced, r.attempted, r.failed
        );
        for e in &r.errors {
            println!("  {e}");
        }
    })
}

pub async fn import(s: &Session, file: &str, replace: bool) -> Outcome {
    let preview = ledger_app::commands::import_preview(&s.state, file.into())
        .await
        .map_err(Failure::from_app)?;
    if s.dry_run || !replace {
        return show(s.json, &preview, |p| {
            println!("{}", serde_json::to_string_pretty(p).unwrap());
            if !replace {
                println!("\nnothing written; add --replace to make this the ledger");
            }
        });
    }
    let report = ledger_app::commands::import_apply(&s.state, file.into(), true)
        .await
        .map_err(|e| Failure::Refused(e.to_string()))?;
    s.state.finish_background().await;
    show(s.json, &report, |r| {
        println!("{}", serde_json::to_string_pretty(r).unwrap())
    })
}

pub struct Init {
    pub name: String,
    pub local: Option<String>,
    pub s3_bucket: Option<String>,
    pub s3_key: String,
    pub s3_region: Option<String>,
    pub s3_endpoint: Option<String>,
    pub aws_profile: Option<String>,
}

/// Sets up a machine that has no Home Ledger app: names it and points it at
/// the ledger. On a machine with the app, `hl` already uses the app's setup.
pub async fn init(s: &Session, args: Init) -> Outcome {
    if s.state.config().await.setup_complete {
        return Err(Failure::Usage(
            "this machine is already set up; `hl status` shows how, and the app's Settings change it".into(),
        ));
    }
    ledger_app::storage::rename_device(&s.state, args.name.clone())
        .await
        .map_err(|e| Failure::Refused(e.to_string()))?;

    let (settings, secret, label) = match (&args.local, &args.s3_bucket) {
        (Some(path), None) => {
            // Named on purpose, so made if missing: unlike a share that may
            // not be mounted, a folder asked for here is meant to exist.
            if let Some(dir) = std::path::Path::new(path)
                .parent()
                .filter(|d| !d.as_os_str().is_empty())
            {
                std::fs::create_dir_all(dir)
                    .map_err(|e| Failure::Usage(format!("cannot make {}: {e}", dir.display())))?;
            }
            (Settings::Local { path: path.into() }, None, "Ledger file")
        }
        (None, Some(bucket)) => {
            let region = args
                .s3_region
                .clone()
                .ok_or_else(|| Failure::Usage("an S3 store needs --s3-region".into()))?;
            // A profile signs from ~/.aws/credentials; otherwise the standard
            // AWS variables are kept in the keychain.
            let secret = match &args.aws_profile {
                Some(_) => None,
                None => {
                    let id = std::env::var("AWS_ACCESS_KEY_ID").map_err(|_| {
                        Failure::Usage("give --aws-profile, or set AWS_ACCESS_KEY_ID and AWS_SECRET_ACCESS_KEY".into())
                    })?;
                    let key = std::env::var("AWS_SECRET_ACCESS_KEY")
                        .map_err(|_| Failure::Usage("AWS_SECRET_ACCESS_KEY is not set".into()))?;
                    Some(Secret::AccessKey {
                        access_key_id: id,
                        secret_access_key: key,
                        session_token: std::env::var("AWS_SESSION_TOKEN").ok(),
                    })
                }
            };
            (
                Settings::S3 {
                    bucket: bucket.clone(),
                    key: args.s3_key.clone(),
                    region,
                    endpoint: args.s3_endpoint.clone(),
                    aws_profile: args.aws_profile.clone(),
                },
                secret,
                "S3",
            )
        }
        _ => {
            return Err(Failure::Usage(
                "choose where the ledger is: --local <path>, or --s3-bucket with --s3-region (Google Drive is set up in the app)".into(),
            ));
        }
    };

    let store = StoreConfig {
        id: uuid::Uuid::new_v4().simple().to_string(),
        label: label.into(),
        settings,
        accept_risk: false,
    };
    let id = store.id.clone();
    ledger_app::storage::save_store(&s.state, store, secret)
        .await
        .map_err(|e| Failure::Refused(e.to_string()))?;
    // Either the machine ends up set up, or the store just added goes again:
    // a half-finished init must not leave a store behind.
    let finished = async {
        ledger_app::storage::promote_store(&s.state, id.clone(), false).await?;
        ledger_app::storage::finish_setup(&s.state).await
    }
    .await;
    if let Err(e) = finished {
        let _ = ledger_app::storage::remove_store(&s.state, id).await;
        return Err(Failure::Refused(e.to_string()));
    }
    show(
        s.json,
        &json!({ "machine": args.name, "setUp": true }),
        |_| {
            println!(
                "set up as \"{}\"; `hl status` to check the store",
                args.name
            )
        },
    )
}
