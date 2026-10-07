//! Commands that only read: the same figures the app's screens show.

use crate::out::{Failure, Outcome, Table, maybe_money, money, show};
use crate::session::{Session, find};
use ledger_app::views::LedgerView;

pub async fn summary(s: &Session) -> Outcome {
    let o = ledger_app::commands::overview(&s.state)
        .await
        .map_err(Failure::from_app)?;
    show(s.json, &o, |o| {
        println!("Net worth       {}", money(&o.net));
        println!("Assets          {}", money(&o.assets));
        println!("Debts           {}", money(&o.debts));
        println!("Monthly income  {}", money(&o.monthly_income));
        println!("Monthly budget  {}", money(&o.monthly_budget));
        println!("Bucket cash     {}", money(&o.bucket_cash));
        if o.stale {
            println!(
                "\n(from {}: the source of truth could not be reached)",
                o.loaded_from
            );
        }
    })
}

pub async fn accounts(s: &Session, which: Option<&str>) -> Outcome {
    let v = s.view().await?;
    if let Some(wanted) = which {
        let a = find(&v.accounts, wanted, "account", |a| &a.id, |a| &a.name)?;
        return show(s.json, a, |a| {
            println!("{} ({})", a.name, a.kind);
            println!("id        {}", a.id);
            if !a.institution.is_empty() {
                println!("at        {}", a.institution);
            }
            println!("balance   {}", maybe_money(&a.total));
            if a.holding_count > 0 {
                println!(
                    "holdings  {} across {}",
                    money(&a.holdings),
                    a.holding_count
                );
            }
            if let Some(room) = &a.available_credit {
                println!("available {}", money(room));
            }
            if let (Some(owed), Some(equity)) = (&a.owed_against, &a.equity) {
                println!("owed      {} on {}", money(owed), a.loan_name);
                println!("equity    {}", money(equity));
            }
            if !a.secures.is_empty() {
                println!("secures   {}", a.secures);
            }
            println!("net       {}", money(&a.net));
        });
    }
    show(s.json, &v.accounts, |accounts| {
        let mut t = Table::new(&["Account", "Kind", "Balance", "Net"]).figures(&[2, 3]);
        for a in accounts {
            t.row(vec![
                a.name.clone(),
                a.kind.clone(),
                maybe_money(&a.total),
                money(&a.net),
            ]);
        }
        t.print();
    })
}

pub async fn buckets(s: &Session, which: Option<&str>) -> Outcome {
    let v = s.view().await?;
    if let Some(wanted) = which {
        let b = find(&v.buckets, wanted, "bucket", |b| &b.id, |b| &b.name)?;
        return show(s.json, b, |b| {
            println!("{}{}", b.name, if b.locked { " (locked)" } else { "" });
            println!("id        {}", b.id);
            println!(
                "kept in   {}",
                if b.account_name.is_empty() {
                    "no account"
                } else {
                    &b.account_name
                }
            );
            println!("total     {}", money(&b.total));
            println!("cash      {}", money(&b.cash));
            if b.invested != "0.00" {
                println!("invested  {}", money(&b.invested));
            }
            if let Some(target) = &b.target {
                println!(
                    "target    {} ({:.1}%)",
                    money(target),
                    b.progress.unwrap_or(0.0)
                );
            }
            if !b.funded_by.is_empty() {
                println!(
                    "funded    {} a month by {}",
                    money(&b.funded_monthly),
                    b.funded_by.join(", ")
                );
            }
            if b.committed != "0.00" {
                print!("committed {} on open statements", money(&b.committed));
                match &b.committed_short {
                    Some(short) => println!(", {} short", money(short)),
                    None => println!(),
                }
            }
        });
    }
    show(s.json, &v.buckets, |buckets| {
        let mut t = Table::new(&["Bucket", "Kept in", "Total", "Target", "Monthly", "Short"])
            .figures(&[2, 3, 4, 5]);
        for b in buckets {
            t.row(vec![
                b.name.clone(),
                if b.account_name.is_empty() {
                    "—".into()
                } else {
                    b.account_name.clone()
                },
                money(&b.total),
                maybe_money(&b.target),
                money(&b.funded_monthly),
                b.committed_short.as_deref().map(money).unwrap_or_default(),
            ]);
        }
        t.print();
    })
}

pub async fn budget(s: &Session) -> Outcome {
    let v = s.view().await?;
    show(s.json, &v.budget, |lines| {
        let mut t = Table::new(&["Line", "Type", "Monthly", "Bucket", "Paid from"]).figures(&[2]);
        for b in lines {
            t.row(vec![
                b.name.clone(),
                b.kind.clone(),
                money(&b.monthly_amount),
                b.bucket_name.clone(),
                b.account_name.clone(),
            ]);
        }
        t.print();
    })
}

pub async fn income(s: &Session) -> Outcome {
    let v = s.view().await?;
    show(s.json, &v.income, |streams| {
        let mut t = Table::new(&["Income", "Owner", "Monthly", "Per paycheck", "How often"])
            .figures(&[2, 3]);
        for i in streams {
            t.row(vec![
                i.name.clone(),
                i.owner.clone(),
                money(&i.monthly_total),
                money(&i.per_paycheck),
                i.frequency.clone(),
            ]);
        }
        t.print();
    })
}

pub async fn goals(s: &Session) -> Outcome {
    let v = s.view().await?;
    show(s.json, &v.goals, |goals| {
        let mut t =
            Table::new(&["Goal", "Saved", "Target", "Progress", "Bucket"]).figures(&[1, 2, 3]);
        for g in goals {
            t.row(vec![
                g.name.clone(),
                money(&g.saved),
                maybe_money(&g.target),
                g.progress
                    .map(|p| format!("{p:.1}%"))
                    .unwrap_or_else(|| "—".into()),
                g.bucket_name.clone(),
            ]);
        }
        t.print();
    })
}

pub async fn holdings(s: &Session) -> Outcome {
    let v = s.view().await?;
    show(s.json, &v.holdings, |holdings| {
        let mut t = Table::new(&[
            "Holding", "Ticker", "Quantity", "Price", "Value", "Gain", "Account",
        ])
        .figures(&[2, 3, 4, 5]);
        for h in holdings {
            t.row(vec![
                h.name.clone(),
                h.ticker.clone(),
                h.quantity.clone(),
                h.price.clone().unwrap_or_else(|| "—".into()),
                money(&h.value),
                maybe_money(&h.gain),
                h.account_name.clone(),
            ]);
        }
        t.print();
    })
}

pub async fn retirement(s: &Session) -> Outcome {
    let v = s.view().await?;
    show(s.json, &v.retirement, |accounts| {
        let mut t = Table::new(&["Account", "Value", "Monthly", "From budget"]).figures(&[1, 2, 3]);
        for r in accounts {
            t.row(vec![
                r.name.clone(),
                money(&r.value),
                money(&r.monthly),
                money(&r.from_budget),
            ]);
        }
        t.print();
    })
}

/// A statement by id, or by card name when that card has one open statement.
pub fn statement<'a>(
    v: &'a LedgerView,
    wanted: &str,
) -> Result<&'a ledger_app::views::ReconciliationView, Failure> {
    if let Some(r) = v.reconciliations.iter().find(|r| r.id == wanted) {
        return Ok(r);
    }
    let open: Vec<_> = v
        .reconciliations
        .iter()
        .filter(|r| r.status != "settled")
        .collect();
    find(
        &open,
        wanted,
        "open statement for the card",
        |r| &r.id,
        |r| &r.card,
    )
    .copied()
}

pub async fn statements(s: &Session, which: Option<&str>) -> Outcome {
    let v = s.view().await?;
    if let Some(wanted) = which {
        let r = statement(&v, wanted)?;
        return show(s.json, r, |r| {
            println!(
                "{} · {} · {}",
                r.card,
                if r.statement_date.is_empty() {
                    "undated"
                } else {
                    &r.statement_date
                },
                r.status
            );
            println!("id          {}", r.id);
            println!("balance     {}", money(&r.balance));
            println!(
                "charges     {} across {}",
                money(&r.lines_total),
                r.lines.len()
            );
            println!("unaccounted {}", money(&r.unaccounted));
            for short in &r.shortfalls {
                println!(
                    "short       {} holds {} of {}",
                    short.bucket_name,
                    money(&short.holds),
                    money(&short.needs)
                );
            }
            let mut t = Table::new(&["Charge", "On", "By", "Amount", "Bucket"]).figures(&[3]);
            for l in &r.lines {
                t.row(vec![
                    l.label.clone(),
                    l.spent_on.clone(),
                    l.member.clone(),
                    money(&l.amount),
                    if l.bucket_name.is_empty() {
                        "everyday".into()
                    } else {
                        l.bucket_name.clone()
                    },
                ]);
            }
            println!();
            t.print();
        });
    }
    show(s.json, &v.reconciliations, |recs| {
        let mut t = Table::new(&["Card", "Date", "Status", "Balance", "Unaccounted", "Id"])
            .figures(&[3, 4]);
        for r in recs {
            t.row(vec![
                r.card.clone(),
                r.statement_date.clone(),
                r.status.clone(),
                money(&r.balance),
                money(&r.unaccounted),
                r.id.clone(),
            ]);
        }
        t.print();
    })
}

pub async fn spending(s: &Session, period: &str, status: &str, from: &str, to: &str) -> Outcome {
    let (today, offset) = Session::today();
    let r = ledger_app::spending::spending(
        &s.state,
        period.into(),
        from.into(),
        to.into(),
        status.into(),
        today,
        offset,
    )
    .await
    .map_err(Failure::from_app)?;
    show(s.json, &r, |r| {
        if !r.valid {
            println!("choose dates with --from on or before --to");
            return;
        }
        let span = if r.start.is_empty() {
            "all time".to_string()
        } else {
            format!("{} to {}", r.start, r.end)
        };
        println!("{span}, {} charges\n", r.count);
        for (label, value) in [
            ("Itemized spending", &r.total),
            ("Open", &r.open),
            ("Settled", &r.settled),
            ("From buckets", &r.from_buckets),
            ("Everyday", &r.everyday),
            ("Buckets withdrawn", &r.withdrawn),
            ("Unattributed", &r.unattributed),
            ("Average charge", &r.average),
        ] {
            println!("{label:<18} {:>14}", money(value));
        }
        for (title, rows) in [
            ("Who", &r.people),
            ("Items", &r.items),
            ("Months", &r.months),
            ("Cards", &r.cards),
        ] {
            println!();
            let mut t = Table::new(&[title, "Count", "Amount"]).figures(&[1, 2]);
            for g in rows.iter().take(10) {
                t.row(vec![g.name.clone(), g.count.to_string(), money(&g.amount)]);
            }
            t.print();
        }
    })
}

/// `1y`, `6m`, `90d` from today, or a date.
pub fn date_ahead(spec: &str) -> Result<String, Failure> {
    if chrono::NaiveDate::parse_from_str(spec, "%Y-%m-%d").is_ok() {
        return Ok(spec.to_string());
    }
    let (number, unit) = spec.split_at(spec.len().saturating_sub(1));
    let n: u32 = number.parse().map_err(|_| {
        Failure::Usage(format!(
            "\"{spec}\" is neither a date nor a span like 6m or 1y"
        ))
    })?;
    let today = chrono::Local::now().date_naive();
    let then = match unit {
        "d" => today.checked_add_days(chrono::Days::new(n.into())),
        "m" => today.checked_add_months(chrono::Months::new(n)),
        "y" => today.checked_add_months(chrono::Months::new(n * 12)),
        _ => None,
    }
    .ok_or_else(|| {
        Failure::Usage(format!(
            "\"{spec}\" is neither a date nor a span like 6m or 1y"
        ))
    })?;
    Ok(then.format("%Y-%m-%d").to_string())
}

pub async fn plan(s: &Session, to: &str) -> Outcome {
    let (today, _) = Session::today();
    let p = ledger_app::planning::planning(&s.state, today, date_ahead(to)?)
        .await
        .map_err(Failure::from_app)?;
    show(s.json, &p, |p| {
        println!("{} to {} ({} months)\n", p.from, p.to, p.months);
        let mut t = Table::new(&["Bucket", "Now", "In", "Out", "Ends", "Of target"])
            .figures(&[1, 2, 3, 4, 5]);
        for b in &p.buckets {
            t.row(vec![
                b.name.clone(),
                money(&b.current),
                money(&b.contributions),
                money(&b.deductions),
                money(&b.projected),
                b.percent
                    .map(|x| format!("{x:.1}%"))
                    .unwrap_or_else(|| "—".into()),
            ]);
        }
        t.print();
        println!(
            "\nTotal: {} now, {} at {}",
            money(&p.current),
            money(&p.projected),
            p.to
        );
    })
}

pub async fn history(
    s: &Session,
    since: Option<&str>,
    by: Option<&str>,
    client: Option<&str>,
) -> Outcome {
    let h = ledger_app::views::history(&s.state)
        .await
        .map_err(Failure::from_app)?;
    let floor = match since {
        None => None,
        Some(spec) => Some(date_behind(spec)?),
    };
    let entries: Vec<_> = h
        .entries
        .into_iter()
        .filter(|e| floor.as_deref().is_none_or(|f| e.at.as_str() >= f))
        .filter(|e| by.is_none_or(|b| e.actor.eq_ignore_ascii_case(b) || e.install == b))
        .filter(|e| client.is_none_or(|c| e.client.eq_ignore_ascii_case(c)))
        .collect();
    for problem in &h.problems {
        eprintln!("hl: {problem}");
    }
    show(s.json, &entries, |entries| {
        let mut t = Table::new(&["When", "Who", "What", "Amount"]).figures(&[3]);
        for e in entries {
            let who = [e.actor.as_str(), e.client.as_str(), e.via.as_str()]
                .into_iter()
                .filter(|p| !p.is_empty())
                .collect::<Vec<_>>()
                .join(" · ");
            t.row(vec![
                e.at.replace('T', " ").trim_end_matches('Z').to_string(),
                who,
                format!("{} {} {}", e.action, e.subject, e.name),
                e.amount.as_deref().map(money).unwrap_or_default(),
            ]);
        }
        t.print();
    })
}

/// `7d`, `2m` before now, or a date, as the start of an ISO timestamp.
fn date_behind(spec: &str) -> Result<String, Failure> {
    if chrono::NaiveDate::parse_from_str(spec, "%Y-%m-%d").is_ok() {
        return Ok(spec.to_string());
    }
    let (number, unit) = spec.split_at(spec.len().saturating_sub(1));
    let n: i64 = number
        .parse()
        .map_err(|_| Failure::Usage(format!("\"{spec}\" is neither a date nor a span like 7d")))?;
    let days = match unit {
        "d" => n,
        "w" => n * 7,
        "m" => n * 31,
        _ => {
            return Err(Failure::Usage(format!(
                "\"{spec}\" is neither a date nor a span like 7d"
            )));
        }
    };
    let then = chrono::Utc::now() - chrono::Duration::days(days);
    Ok(then.format("%Y-%m-%dT%H:%M:%SZ").to_string())
}

pub async fn export(s: &Session, format: &str, collection: Option<&str>) -> Outcome {
    let loaded = s
        .state
        .live()
        .await
        .engine
        .load()
        .await
        .map_err(Failure::from_app)?;
    let body = loaded
        .snapshot
        .map(|s| s.body)
        .unwrap_or_else(|| b"{}".to_vec());
    let doc: serde_json::Value =
        serde_json::from_slice(&body).map_err(|e| Failure::Error(e.to_string()))?;
    let picked = match collection {
        None => doc,
        Some(name) => doc
            .get(name)
            .cloned()
            .ok_or_else(|| Failure::Usage(format!("the ledger has no \"{name}\"")))?,
    };
    match format {
        "json" => println!("{}", serde_json::to_string_pretty(&picked).unwrap()),
        "csv" => print!("{}", csv(&picked)?),
        other => {
            return Err(Failure::Usage(format!(
                "export as json or csv, not \"{other}\""
            )));
        }
    }
    Ok(())
}

/// One row per record, a column for each field any record has that holds a
/// plain value. Nested lists are left out; export them by name.
pub fn csv(value: &serde_json::Value) -> Result<String, Failure> {
    let rows = value.as_array().ok_or_else(|| {
        Failure::Usage(
            "CSV needs a list: name one, such as `hl export --format csv accounts`".into(),
        )
    })?;
    let mut columns: Vec<String> = Vec::new();
    for row in rows {
        for (k, v) in row.as_object().into_iter().flatten() {
            if !v.is_array() && !v.is_object() && !columns.contains(k) {
                columns.push(k.clone());
            }
        }
    }
    let cell = |v: Option<&serde_json::Value>| -> String {
        let text = match v {
            None | Some(serde_json::Value::Null) => String::new(),
            Some(serde_json::Value::String(s)) => s.clone(),
            Some(other) => other.to_string(),
        };
        if text.contains([',', '"', '\n']) {
            format!("\"{}\"", text.replace('"', "\"\""))
        } else {
            text
        }
    };
    let mut out = columns.join(",") + "\n";
    for row in rows {
        let line: Vec<String> = columns.iter().map(|c| cell(row.get(c))).collect();
        out.push_str(&line.join(","));
        out.push('\n');
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn a_collection_becomes_one_row_per_record_with_plain_fields_only() {
        let out = csv(&json!([
            { "name": "Checking", "total": 473.97, "debts": [] },
            { "name": "Joint, Main", "notes": "say \"hi\"" }
        ]))
        .unwrap();
        assert_eq!(
            out,
            "name,total,notes\nChecking,473.97,\n\"Joint, Main\",,\"say \"\"hi\"\"\"\n"
        );
        assert!(csv(&json!({ "not": "a list" })).is_err());
    }

    #[test]
    fn spans_read_forwards_and_backwards() {
        assert_eq!(date_ahead("2027-01-31").unwrap(), "2027-01-31");
        assert!(date_ahead("6m").is_ok() && date_ahead("1y").is_ok() && date_ahead("90d").is_ok());
        assert!(date_ahead("soon").is_err());
        assert!(date_behind("7d").unwrap().ends_with('Z'));
        assert!(date_behind("never").is_err());
    }
}
