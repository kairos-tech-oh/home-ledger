//! Print the figures this crate derives from a real ledger file.
//!
//! The port's comparison harness: run it against a document the prototype
//! wrote, then check the numbers against what the prototype shows.
//!
//!     cargo run -p ledger-math --example figures -- path/to/ledger.json

use ledger_domain::Ledger;
use std::collections::BTreeSet;

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: figures <ledger.json>");
        std::process::exit(2);
    };

    let raw = std::fs::read(&path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        std::process::exit(1);
    });

    let ledger = match Ledger::from_bytes(&raw) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("PARSE FAILED: {e}");
            std::process::exit(1);
        }
    };

    println!("parsed ok");
    println!("  schema version   {}", ledger.v);
    println!("  accounts         {}", ledger.accounts.len());
    println!("  income streams   {}", ledger.income.len());
    println!("  budget lines     {}", ledger.budget.len());
    println!("  buckets          {}", ledger.buckets.len());
    println!("  holdings         {}", ledger.investments.len());
    println!("  goals            {}", ledger.goals.len());
    println!("  templates        {}", ledger.templates.len());
    println!("  reconciliations  {}", ledger.reconciliations.len());

    let worth = ledger_math::net_worth(&ledger);
    println!("\nfigures");
    println!("  assets           {}", worth.assets);
    println!("  debts            {}", worth.debts);
    println!("  net              {}", worth.net);
    println!(
        "  monthly income   {}",
        ledger_math::monthly_income(&ledger)
    );
    println!(
        "  monthly budget   {}",
        ledger_math::monthly_budget(&ledger)
    );
    println!("  bucket cash      {}", ledger_math::bucket_cash(&ledger));

    let rollup = ledger_math::retirement_rollup(&ledger);
    println!(
        "  retirement       {} monthly {} over {} accounts",
        rollup.total, rollup.monthly, rollup.count
    );
    for rate in [6, 8, 10] {
        let line = &ledger_math::retirement_projection(
            &ledger,
            rust_decimal::Decimal::from(30),
            &[rust_decimal::Decimal::from(rate)],
        )
        .lines[0];
        println!(
            "  project 30y @{rate}%  {} contributed {}",
            line.value, line.contributed
        );
    }

    let earners = ledger_math::owner_shares(&ledger);
    if !earners.is_empty() {
        println!("\nearners");
        for earner in &earners {
            println!(
                // Rounded explicitly: `{:.2}` on a Decimal truncates.
                "  {} {} {}% {}/mo",
                earner.owner,
                earner.monthly,
                earner.percent.round_dp(2),
                earner.paychecks_per_month.round_dp(4)
            );
        }
    }

    // The part most likely to be wrong: a field this build does not model is
    // carried in `unknown`, and anything landing there is a gap in the port.
    if !ledger.unknown.is_empty() {
        let keys: BTreeSet<&String> = ledger.unknown.keys().collect();
        println!("\ntop-level keys not modelled ({}):", keys.len());
        for key in keys {
            println!("  {key}");
        }
    }
    if !ledger.passthrough.is_empty() {
        println!("\npassthrough collections: {}", ledger.passthrough.len());
    }

    // Round-tripping proves nothing was dropped on the way through.
    match ledger.to_bytes() {
        Ok(out) => {
            let before: serde_json::Value = serde_json::from_slice(&raw).unwrap();
            let after: serde_json::Value = serde_json::from_slice(&out).unwrap();
            if before == after {
                println!("\nround trip: identical");
            } else {
                println!("\nround trip: DIFFERS");
                report_differences(&before, &after);
            }
        }
        Err(e) => println!("\nround trip: serialise failed: {e}"),
    }
}

/// Name what changed, by field path and kind only. Never prints a value:
/// this runs against real household finances.
fn report_differences(before: &serde_json::Value, after: &serde_json::Value) {
    let mut seen: BTreeSet<String> = BTreeSet::new();
    walk("", before, after, &mut seen);
    for line in seen.iter().take(40) {
        println!("  {line}");
    }
    if seen.len() > 40 {
        println!("  ... and {} more", seen.len() - 40);
    }
}

/// Collapses array indices to `[]` so one wrong field on 20 accounts reads as
/// one finding rather than twenty.
fn walk(path: &str, a: &serde_json::Value, b: &serde_json::Value, out: &mut BTreeSet<String>) {
    use serde_json::Value;
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            for (k, v) in x {
                let child = if path.is_empty() {
                    k.clone()
                } else {
                    format!("{path}.{k}")
                };
                match y.get(k) {
                    Some(w) => walk(&child, v, w, out),
                    None => {
                        out.insert(format!("LOST     {child}"));
                    }
                }
            }
            for k in y.keys() {
                if !x.contains_key(k) {
                    let child = if path.is_empty() {
                        k.clone()
                    } else {
                        format!("{path}.{k}")
                    };
                    out.insert(format!(
                        "ADDED    {child}  (null/empty written where nothing was)"
                    ));
                }
            }
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                out.insert(format!("LENGTH   {path}[]  {} -> {}", x.len(), y.len()));
            }
            for (v, w) in x.iter().zip(y.iter()) {
                walk(&format!("{path}[]"), v, w, out);
            }
        }
        _ if a != b => {
            let kind = match (a, b) {
                (Value::Number(_), Value::String(_)) => "number became string",
                (Value::String(_), Value::Number(_)) => "string became number",
                (Value::Number(_), Value::Number(_)) => "number changed",
                (Value::Null, _) => "null replaced",
                (_, Value::Null) => "became null",
                _ => "value changed",
            };
            out.insert(format!("CHANGED  {path}  ({kind})"));
        }
        _ => {}
    }
}
