//! `hl bank`: bank connections through Plaid, with the person's own keys.
//! The same work as the app's Bank connections panel, from `ledger_app::bank`.

use crate::out::{Failure, Outcome, Table, maybe_money, money, show};
use crate::session::{Session, find};
use ledger_app::bank::{self as bk, BankStatus};
use serde_json::{Value, json};
use std::io::{BufRead, Write};
use std::time::Duration;

const POLL: Duration = Duration::from_secs(2);
/// Longer than anyone takes to sign in to a bank.
const GIVE_UP: Duration = Duration::from_secs(30 * 60);

fn failed(e: impl std::fmt::Display) -> Failure {
    Failure::from_app(e)
}

/// From the environment variable, or asked for in the terminal. A secret is
/// never an argument, which other users of the machine could see.
fn ask(variable: &str, prompt: &str, hidden: bool) -> Result<String, Failure> {
    if let Ok(v) = std::env::var(variable)
        && !v.trim().is_empty()
    {
        return Ok(v);
    }
    let no_terminal = || {
        Failure::Usage(format!(
            "set {variable}, or run hl in a terminal to be asked"
        ))
    };
    if hidden {
        return rpassword::prompt_password(prompt).map_err(|_| no_terminal());
    }
    eprint!("{prompt}");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|_| no_terminal())?;
    Ok(line.trim().to_string())
}

pub async fn keys(
    s: &Session,
    environment: &str,
    client_id: Option<String>,
    forget: bool,
) -> Outcome {
    if forget {
        bk::bank_forget_keys(&s.state).await.map_err(failed)?;
        return show(s.json, &json!({ "keys": false }), |_| {
            println!("Plaid keys removed")
        });
    }
    let client_id = match client_id {
        Some(id) => id,
        None => ask("PLAID_CLIENT_ID", "Plaid client id: ", false)?,
    };
    let secret = ask(
        "PLAID_SECRET",
        &format!("Plaid {environment} secret: "),
        true,
    )?;
    let status = bk::bank_save_keys(&s.state, client_id, secret, environment.into())
        .await
        .map_err(|e| Failure::Refused(e.to_string()))?;
    show(
        s.json,
        &json!({ "environment": status.environment }),
        |_| {
            println!(
                "Plaid accepted the keys ({environment}); they are in this computer's keychain"
            )
        },
    )
}

pub async fn status(s: &Session) -> Outcome {
    let status = bk::bank_status(&s.state).await.map_err(failed)?;
    show(s.json, &status, |st| {
        match &st.environment {
            Some(env) => println!("Plaid keys: {env}"),
            None => println!("Plaid keys: none; `hl bank keys` adds them"),
        }
        for item in &st.items {
            println!();
            let fetched = if item.fetched_at.is_empty() {
                "never fetched".to_string()
            } else {
                format!("fetched {}", item.fetched_at)
            };
            println!(
                "{}  ({fetched}, {} transactions)",
                item.institution, item.transactions
            );
            if item.needs_sign_in {
                println!(
                    "  the bank wants a new sign-in: hl bank connect --again \"{}\"",
                    item.institution
                );
            } else if !item.problem.is_empty() {
                println!("  {}", item.problem);
            }
            if item.gathering {
                println!("  Plaid is still gathering its history");
            }
            let mut t = Table::new(&["Account", "Kind", "Balance", "Linked to"]).figures(&[2]);
            for a in &item.accounts {
                t.row(vec![
                    if a.mask.is_empty() {
                        a.name.clone()
                    } else {
                        format!("{} ··{}", a.name, a.mask)
                    },
                    if a.subtype.is_empty() {
                        a.kind.clone()
                    } else {
                        a.subtype.clone()
                    },
                    maybe_money(&a.current),
                    if a.linked_name.is_empty() {
                        "—".into()
                    } else {
                        a.linked_name.clone()
                    },
                ]);
            }
            t.print();
        }
    })
}

fn item_named<'a>(status: &'a BankStatus, wanted: &str) -> Result<&'a bk::ItemView, Failure> {
    find(
        &status.items,
        wanted,
        "bank connection",
        |i| &i.id,
        |i| &i.institution,
    )
}

pub async fn connect(s: &Session, again: Option<&str>) -> Outcome {
    let item = match again {
        Some(name) => {
            let status = bk::bank_status(&s.state).await.map_err(failed)?;
            Some(item_named(&status, name)?.id.clone())
        }
        None => None,
    };
    let link = bk::bank_connect(&s.state, item.clone())
        .await
        .map_err(failed)?;
    eprintln!("Plaid's sign-in page should now be open in your browser. If not, open:");
    eprintln!("  {}", link.url);
    eprintln!("Waiting for you to finish there…");
    let started = std::time::Instant::now();
    let done = loop {
        tokio::time::sleep(POLL).await;
        let now = bk::bank_connect_check(&s.state, link.token.clone(), item.clone())
            .await
            .map_err(failed)?;
        if now.state != "waiting" {
            break now;
        }
        if started.elapsed() > GIVE_UP {
            return Err(Failure::Error(
                "the sign-in page was left too long; connect again".into(),
            ));
        }
    };
    match done.state {
        "exited" => Err(Failure::Refused(if done.message.is_empty() {
            "the sign-in was closed before a bank was connected".into()
        } else {
            done.message
        })),
        "updated" => {
            let id = item.unwrap_or_default();
            bk::bank_fetch(&s.state, Some(id)).await.map_err(failed)?;
            show(s.json, &json!({ "state": "updated" }), |_| {
                println!("signed in again")
            })
        }
        _ => {
            let added = done.item.map(|i| i.id).unwrap_or_default();
            bk::bank_fetch(&s.state, Some(added))
                .await
                .map_err(failed)?;
            if !s.json {
                println!(
                    "connected. Link its accounts with `hl bank link <account> <ledger account>`:\n"
                );
            }
            status(s).await
        }
    }
}

/// A bank account by its id, its name, or its last four digits.
pub async fn link(s: &Session, account: &str, ledger_account: &str) -> Outcome {
    let status = bk::bank_status(&s.state).await.map_err(failed)?;
    let wanted = account.trim().trim_start_matches("··");
    let matches: Vec<(&bk::ItemView, &bk::AccountView)> = status
        .items
        .iter()
        .flat_map(|i| i.accounts.iter().map(move |a| (i, a)))
        .filter(|(_, a)| a.id == wanted || a.mask == wanted || a.name.eq_ignore_ascii_case(wanted))
        .collect();
    let (item, bank_account) = match matches.as_slice() {
        [one] => *one,
        [] => {
            return Err(Failure::Usage(format!(
                "no bank account \"{account}\"; see `hl bank`"
            )));
        }
        _ => {
            return Err(Failure::Usage(format!(
                "more than one bank account matches \"{account}\"; use its last four digits or id"
            )));
        }
    };
    let target = if matches!(ledger_account.to_lowercase().as_str(), "none" | "") {
        String::new()
    } else {
        let v = s.view().await?;
        find(
            &v.accounts,
            ledger_account,
            "account",
            |a| &a.id,
            |a| &a.name,
        )?
        .id
        .clone()
    };
    if s.dry_run {
        println!(
            "would link {} to {}",
            bank_account.name,
            if target.is_empty() {
                "nothing"
            } else {
                ledger_account
            }
        );
        return Ok(());
    }
    bk::bank_link(
        &s.state,
        item.id.clone(),
        bank_account.id.clone(),
        target.clone(),
    )
    .await
    .map_err(failed)?;
    show(
        s.json,
        &json!({ "account": bank_account.id, "linked": target }),
        |_| {
            if target.is_empty() {
                println!("{} is no longer used", bank_account.name);
            } else {
                println!(
                    "{} ··{} is now {ledger_account}",
                    bank_account.name, bank_account.mask
                );
            }
        },
    )
}

pub async fn fetch(s: &Session) -> Outcome {
    let got = bk::bank_fetch(&s.state, None).await.map_err(failed)?;
    show(s.json, &got, |got| {
        for i in &got.items {
            if i.problem.is_empty() {
                println!(
                    "{}: {} new, {} changed, {} removed{}",
                    i.institution,
                    i.added,
                    i.changed,
                    i.removed,
                    if i.gathering {
                        " (Plaid is still gathering history)"
                    } else {
                        ""
                    }
                );
            } else {
                println!("{}: {}", i.institution, i.problem);
            }
        }
        if got.items.is_empty() {
            println!("no banks are connected; `hl bank connect` adds one");
        }
    })?;
    match got.items.iter().find(|i| !i.problem.is_empty()) {
        Some(i) => Err(Failure::Error(format!("{}: {}", i.institution, i.problem))),
        None => Ok(()),
    }
}

pub async fn balances(s: &Session, apply: bool) -> Outcome {
    if !s.dry_run {
        bk::bank_fetch(&s.state, None).await.map_err(failed)?;
    }
    let offered = bk::bank_balances(&s.state).await.map_err(failed)?;
    if !apply || s.dry_run {
        return show(s.json, &offered, |offered| {
            let mut t = Table::new(&["Account", "Ledger", "Bank", "Credit left", "From"])
                .figures(&[1, 2, 3]);
            for p in offered {
                t.row(vec![
                    p.account.clone(),
                    maybe_money(&p.from),
                    money(&p.to),
                    maybe_money(&p.available_credit),
                    format!("{} {}", p.institution, p.bank_account),
                ]);
            }
            t.print();
            if !offered.is_empty() {
                println!("\n`hl bank balances --apply` accepts them");
            }
        });
    }
    let ids = offered.iter().map(|p| p.account_id.clone()).collect();
    let n = bk::bank_apply_balances(&s.state, ids)
        .await
        .map_err(|e| Failure::Refused(e.to_string()))?;
    s.state.finish_background().await;
    show(s.json, &json!({ "updated": n }), |_| {
        println!(
            "{n} balance{} updated from the bank",
            if n == 1 { "" } else { "s" }
        )
    })
}

pub async fn disconnect(s: &Session, which: &str) -> Outcome {
    let status = bk::bank_status(&s.state).await.map_err(failed)?;
    let item = item_named(&status, which)?;
    if s.dry_run {
        println!("would disconnect {}", item.institution);
        return Ok(());
    }
    let name = item.institution.clone();
    bk::bank_disconnect(&s.state, item.id.clone())
        .await
        .map_err(failed)?;
    show(s.json, &json!({ "disconnected": name }), |_| {
        println!("{name} is disconnected; charges already added stay on their statements")
    })
}

/// The bank's charges for a statement: fetched, matched against what is
/// there, and the suggested ones added, each to the bucket its merchant went
/// to last unless `--bucket` says otherwise.
pub async fn statement_import(
    s: &Session,
    statement_id: &str,
    member: &str,
    bucket: Option<&str>,
) -> Outcome {
    let v = s.view().await?;
    let bucket_id = match bucket {
        Some(b) => Some(
            find(&v.buckets, b, "bucket", |b| &b.id, |b| &b.name)?
                .id
                .clone(),
        ),
        None => None,
    };
    let preview = bk::bank_preview(&s.state, statement_id.into(), true)
        .await
        .map_err(failed)?;
    for note in &preview.notes {
        eprintln!("hl: {note}");
    }
    let lines: Vec<Value> = preview
        .rows
        .iter()
        .filter(|r| r.suggested)
        .map(|r| {
            json!({
                "label": r.transaction.description,
                "amount": r.value,
                "spentOn": r.transaction.date,
                "member": member,
                "bucketId": bucket_id.clone().or_else(|| r.suggested_bucket.clone()).unwrap_or_default(),
                "notes": r.transaction.category,
                "bankRef": r.bank_ref,
                "bankText": r.bank_text,
            })
        })
        .collect();
    if !s.json {
        let later = preview.rows.iter().filter(|r| r.after_statement).count();
        let earlier = preview.rows.iter().filter(|r| r.earlier).count();
        println!(
            "{} from the bank: {} to add, {} already added, {} payments or credits, {} after the \
             statement date, {} on the last statement's dates",
            preview.rows.len(),
            preview.charges,
            preview.duplicates,
            preview.set_aside,
            later,
            earlier
        );
    }
    if lines.is_empty() {
        println!("nothing to add");
        return Ok(());
    }
    s.edit_via(
        json!({ "op": "reconcile-import", "id": statement_id, "lines": lines }),
        bk::VIA,
    )
    .await
}
