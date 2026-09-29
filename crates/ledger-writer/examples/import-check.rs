//! Dry-run an import against a real ledger file and report what it would do.
//!
//!     cargo run -p ledger-writer --example import-check -- path/to/ledger.json
//!
//! Writes nothing. Counts, the rows it could not clean, and the figures the
//! result comes to — so an import can be checked before it replaces anything.

fn main() {
    let Some(path) = std::env::args().nth(1) else {
        eprintln!("usage: import-check <ledger.json>");
        std::process::exit(2);
    };

    let raw = std::fs::read(&path).unwrap_or_else(|e| {
        eprintln!("cannot read {path}: {e}");
        std::process::exit(1);
    });

    let imported = match ledger_writer::import::read(&raw) {
        Ok(imported) => imported,
        Err(e) => {
            eprintln!("IMPORT REFUSED: {e}");
            std::process::exit(1);
        }
    };

    let report = &imported.report;
    println!("would import {} records", report.records());
    println!(
        "  accounts {}  income {}  budget {}  buckets {}",
        report.accounts, report.income, report.budget, report.buckets
    );
    println!(
        "  holdings {}  goals {}  templates {}  reconciliations {}",
        report.holdings, report.goals, report.templates, report.reconciliations
    );

    if !report.carried.is_empty() {
        println!("  carried through untouched: {}", report.carried.join(", "));
    }

    if report.skipped.is_empty() {
        println!("  nothing skipped");
    } else {
        println!("  skipped {}:", report.skipped.len());
        for row in &report.skipped {
            println!("    {row}");
        }
    }

    let worth = ledger_math::net_worth(&imported.ledger);
    println!("\nthe result comes to");
    println!("  assets  {}", worth.assets);
    println!("  debts   {}", worth.debts);
    println!("  net     {}", worth.net);
}
