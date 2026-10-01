//! Settle and undo every statement in a real ledger, in memory, and check each
//! comes back byte-identical; also count cards a read would reset. Writes nothing.

use ledger_domain::Ledger;
use ledger_writer::{Op, Writer};

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: settle-check <ledger.json>");
        std::process::exit(2);
    };
    let raw = std::fs::read(&path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        std::process::exit(1);
    });
    let original = Ledger::from_bytes(&raw).expect("readable ledger");
    let before = original.to_bytes().expect("serialises");
    let writer = Writer::new("settle-check");
    let mut failed = 0;

    for (n, rec) in original.reconciliations.iter().enumerate() {
        let mut ledger = original.clone();
        let (first, second) = if rec.status == "settled" {
            (
                Op::ReconcileUndo { id: rec.id.clone() },
                Op::ReconcileSettle {
                    id: rec.id.clone(),
                    cover: Vec::new(),
                },
            )
        } else {
            (
                Op::ReconcileSettle {
                    id: rec.id.clone(),
                    cover: Vec::new(),
                },
                Op::ReconcileUndo { id: rec.id.clone() },
            )
        };
        let outcome = writer
            .apply(&mut ledger, &first)
            .and_then(|_| writer.apply(&mut ledger, &second));
        let label = format!("statement {n} ({})", rec.status);
        match outcome {
            // The message can carry amounts, so only its kind is printed.
            Err(e) => println!(
                "{label}: refused ({})",
                match e {
                    ledger_writer::WriteError::Refused(_) => "a rule said no",
                    ledger_writer::WriteError::Missing(_) => "something it names is missing",
                    _ => "other",
                }
            ),
            Ok(_) => {
                // A fresh settle stamps a new time; only that may differ.
                let mut after = ledger.clone();
                after.reconciliations[n].settled_at = rec.settled_at.clone();
                if after.to_bytes().expect("serialises") == before {
                    println!("{label}: round trip identical");
                } else {
                    println!("{label}: DIFFERS");
                    failed += 1;
                }
            }
        }
    }
    // What reading would change: cards whose balance an open statement resets.
    let derived = ledger_writer::read(&raw).expect("readable ledger");
    let moved = original
        .accounts
        .iter()
        .zip(&derived.accounts)
        .filter(|(a, b)| a.total != b.total || a.available_credit != b.available_credit)
        .count();
    println!("{moved} card balances would change on read");

    println!(
        "{} statements, {failed} differ",
        original.reconciliations.len()
    );
    std::process::exit(if failed == 0 { 0 } else { 1 });
}
