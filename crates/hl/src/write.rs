//! Commands that change the ledger. Each builds the same edit the app makes
//! and hands it to [`Session::edit`], so it is checked by the same rules and
//! recorded in the same history.

use crate::out::{Failure, Outcome};
use crate::read::statement;
use crate::session::{Session, find};
use serde_json::{Value, json};

/// An amount as typed: `1234.56`, `$1,234.56`, `-12`. Only plain decimals:
/// `1e3` or `NaN` would be read by the ledger as nothing at all.
fn amount(text: &str) -> Result<String, Failure> {
    let clean: String = text
        .chars()
        .filter(|c| !matches!(c, '$' | ',' | ' '))
        .collect();
    let digits = clean.strip_prefix('-').unwrap_or(&clean);
    let mut parts = digits.splitn(2, '.');
    let whole = parts.next().unwrap_or("");
    let fraction = parts.next();
    let ok = !whole.is_empty()
        && whole.chars().all(|c| c.is_ascii_digit())
        && fraction.is_none_or(|f| !f.is_empty() && f.chars().all(|c| c.is_ascii_digit()));
    if ok {
        Ok(clean)
    } else {
        Err(Failure::Usage(format!("\"{text}\" is not an amount")))
    }
}

pub async fn account_balance(s: &Session, account: &str, value: &str) -> Outcome {
    let v = s.view().await?;
    let a = find(&v.accounts, account, "account", |a| &a.id, |a| &a.name)?;
    s.edit(json!({ "op": "set", "kind": "account", "id": a.id, "record": { "total": amount(value)? } }))
        .await
}

pub enum BucketAction<'a> {
    Add(&'a str),
    Spend(&'a str),
    Set(&'a str),
    Move { amount: &'a str, to: &'a str },
}

pub async fn bucket(s: &Session, bucket: &str, action: BucketAction<'_>, note: &str) -> Outcome {
    let v = s.view().await?;
    let b = find(&v.buckets, bucket, "bucket", |b| &b.id, |b| &b.name)?;
    let op = match action {
        BucketAction::Add(n) => json!({ "op": "bucket-adjust",
            "adjustments": [{ "id": b.id, "delta": amount(n)? }],
            "label": if note.is_empty() { "Added" } else { note } }),
        BucketAction::Spend(n) => json!({ "op": "bucket-adjust",
            "adjustments": [{ "id": b.id, "delta": format!("-{}", amount(n)?.trim_start_matches('-')) }],
            "label": if note.is_empty() { "Spent" } else { note } }),
        BucketAction::Set(n) => json!({ "op": "bucket-total", "id": b.id, "amount": amount(n)? }),
        BucketAction::Move { amount: n, to } => {
            let target = find(&v.buckets, to, "bucket", |b| &b.id, |b| &b.name)?;
            json!({ "op": "bucket-move", "fromId": b.id, "toId": target.id, "amount": amount(n)? })
        }
    };
    s.edit(op).await
}

pub async fn payday(s: &Session, earner: &str, undo: bool) -> Outcome {
    let v = s.view().await?;
    let day = find(
        &v.paydays,
        earner,
        "earner with a payday",
        |d| &d.owner,
        |d| &d.owner,
    )?;
    let adjustments: Vec<Value> = day
        .adjustments
        .iter()
        .map(|a| json!({ "id": a.id, "delta": if undo { format!("-{}", a.delta) } else { a.delta.clone() } }))
        .collect();
    s.edit(json!({ "op": "bucket-adjust", "adjustments": adjustments,
        "label": format!("{}{}'s payday", if undo { "Undo " } else { "" }, day.owner) }))
        .await
}

pub struct NewStatement<'a> {
    pub card: &'a str,
    pub balance: &'a str,
    pub date: &'a str,
    pub bucket_source: Option<&'a str>,
    pub spend_source: Option<&'a str>,
    pub account_moves: bool,
}

pub async fn statement_new(s: &Session, n: NewStatement<'_>) -> Outcome {
    let (card, balance, date) = (n.card, n.balance, n.date);
    let v = s.view().await?;
    // Named for a card account when there is one, and starting from the
    // sources the last statement for that card used, as the app does.
    let account = v
        .accounts
        .iter()
        .find(|a| (a.kind == "credit" || a.kind == "heloc") && a.name.eq_ignore_ascii_case(card));
    let last = v.reconciliations.iter().find(|r| {
        account.is_some_and(|a| r.card_account_id == a.id) || r.card.eq_ignore_ascii_case(card)
    });
    let mut record = json!({
        "card": account.map(|a| a.name.as_str()).unwrap_or(card),
        "balance": amount(balance)?,
        "statementDate": date,
        "lines": [],
    });
    if let Some(a) = account {
        record["cardAccountId"] = json!(a.id);
    }
    if let Some(last) = last {
        record["bucketSourceId"] = json!(last.bucket_source_id);
        record["spendSourceId"] = json!(last.spend_source_id);
        record["adjustAccounts"] = json!(last.adjust_accounts);
    }
    if let Some(name) = n.bucket_source {
        record["bucketSourceId"] =
            json!(find(&v.accounts, name, "account", |a| &a.id, |a| &a.name)?.id);
    }
    if let Some(name) = n.spend_source {
        record["spendSourceId"] =
            json!(find(&v.accounts, name, "account", |a| &a.id, |a| &a.name)?.id);
    }
    if !n.account_moves {
        record["adjustAccounts"] = json!(false);
    }
    s.edit(json!({ "op": "reconcile-set", "id": "", "record": record }))
        .await
}

pub async fn statement_import(
    s: &Session,
    which: &str,
    file: &str,
    member: &str,
    bucket: Option<&str>,
) -> Outcome {
    let v = s.view().await?;
    let r = statement(&v, which)?;
    let text = std::fs::read_to_string(file)
        .map_err(|e| Failure::Usage(format!("cannot read {file}: {e}")))?;
    let bucket_id = match bucket {
        Some(b) => find(&v.buckets, b, "bucket", |b| &b.id, |b| &b.name)?
            .id
            .clone(),
        None => String::new(),
    };
    let preview =
        ledger_app::transactions::transactions_preview(&s.state, r.id.clone(), text, None)
            .await
            .map_err(Failure::from_app)?;
    for note in &preview.notes {
        eprintln!("hl: {note}");
    }
    let lines: Vec<Value> = preview
        .rows
        .iter()
        .filter(|row| row.suggested)
        .map(|row| {
            json!({
                "label": row.transaction.description,
                "amount": row.value,
                "spentOn": row.transaction.date,
                "member": member,
                "bucketId": bucket_id,
                "notes": row.transaction.category,
            })
        })
        .collect();
    if !s.json {
        println!(
            "{} rows: {} to add, {} already on the statement, {} payments or credits left out",
            preview.rows.len(),
            preview.charges,
            preview.duplicates,
            preview.set_aside
        );
    }
    if lines.is_empty() {
        println!("nothing to add");
        return Ok(());
    }
    s.edit(json!({ "op": "reconcile-import", "id": r.id, "lines": lines }))
        .await
}

/// `--cover everyday`, `--cover negative`, `--cover Overflow` for every short
/// bucket, or `--cover Groceries=Overflow` for one.
pub async fn statement_settle(s: &Session, which: &str, covers: &[String]) -> Outcome {
    let v = s.view().await?;
    let r = statement(&v, which)?;
    let mut cover = Vec::new();
    for short in &r.shortfalls {
        let asked = covers
            .iter()
            .find_map(|c| {
                let (bucket, how) = c.split_once('=')?;
                (bucket.eq_ignore_ascii_case(&short.bucket_name) || bucket == short.bucket_id)
                    .then_some(how)
            })
            .or_else(|| covers.iter().find(|c| !c.contains('=')).map(String::as_str));
        let Some(how) = asked else {
            if short.when_short.is_empty() {
                return Err(Failure::Refused(format!(
                    "{} holds {} of the {} this needs; say where the rest comes from with \
                     --cover <bucket>, --cover everyday or --cover negative",
                    short.bucket_name, short.holds, short.needs
                )));
            }
            continue;
        };
        let (kind, from) = match how.to_ascii_lowercase().as_str() {
            "everyday" => ("everyday", String::new()),
            "negative" => ("negative", String::new()),
            _ => (
                "bucket",
                find(&v.buckets, how, "bucket", |b| &b.id, |b| &b.name)?
                    .id
                    .clone(),
            ),
        };
        cover.push(json!({ "bucketId": short.bucket_id, "how": kind, "fromBucketId": from }));
    }
    s.edit(json!({ "op": "reconcile-settle", "id": r.id, "cover": cover }))
        .await
}

pub async fn statement_undo(s: &Session, which: &str) -> Outcome {
    let v = s.view().await?;
    let r = v
        .reconciliations
        .iter()
        .find(|r| r.id == which)
        .or_else(|| {
            // By card, the most recent settled one.
            v.reconciliations
                .iter()
                .find(|r| r.status == "settled" && r.card.eq_ignore_ascii_case(which))
        })
        .ok_or_else(|| Failure::Usage(format!("no settled statement \"{which}\"")))?;
    s.edit(json!({ "op": "reconcile-undo", "id": r.id })).await
}

pub async fn trade(
    s: &Session,
    side: &str,
    holding: &str,
    quantity: &str,
    price: &str,
    note: &str,
) -> Outcome {
    let v = s.view().await?;
    let h = find(&v.holdings, holding, "holding", |h| &h.id, |h| &h.name)
        .or_else(|_| find(&v.holdings, holding, "holding", |h| &h.id, |h| &h.ticker))?;
    s.edit(
        json!({ "op": "trade", "id": h.id, "side": side, "quantity": amount(quantity)?,
        "price": amount(price)?, "notes": note }),
    )
    .await
}

pub async fn price_set(s: &Session, ticker: &str, price: &str) -> Outcome {
    let v = s.view().await?;
    let priced: Vec<Value> = v
        .holdings
        .iter()
        .filter(|h| h.ticker.eq_ignore_ascii_case(ticker))
        .map(|h| Ok(json!({ "id": h.id, "price": amount(price)? })))
        .collect::<Result<_, Failure>>()?;
    if priced.is_empty() {
        return Err(Failure::Usage(format!(
            "no holding has the ticker \"{ticker}\""
        )));
    }
    s.edit(json!({ "op": "price-holdings", "priced": priced, "stale": [] }))
        .await
}

/// Any edit the app knows, in its own JSON form; a list applies each in turn.
pub async fn apply(s: &Session, source: &str) -> Outcome {
    let text = if source == "-" {
        std::io::read_to_string(std::io::stdin()).map_err(|e| Failure::Usage(e.to_string()))?
    } else {
        std::fs::read_to_string(source)
            .map_err(|e| Failure::Usage(format!("cannot read {source}: {e}")))?
    };
    let value: Value =
        serde_json::from_str(&text).map_err(|e| Failure::Usage(format!("not JSON: {e}")))?;
    match value {
        Value::Array(ops) => {
            for op in ops {
                s.edit(op).await?;
            }
            Ok(())
        }
        op => s.edit(op).await,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn amounts_are_taken_as_typed_or_refused() {
        assert_eq!(amount("$1,234.56").unwrap(), "1234.56");
        assert_eq!(amount("-12").unwrap(), "-12");
        assert!(amount("twelve").is_err());
        assert!(amount("NaN").is_err());
        assert!(amount("1e3").is_err());
        assert!(amount("1.").is_err());
        assert_eq!(amount("0.5").unwrap(), "0.5");
    }
}
