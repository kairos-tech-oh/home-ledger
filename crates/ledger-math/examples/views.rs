//! This port's figures for the Goals, Planning, Spending and Dashboard
//! screens, in the same shape `tools/oracle-views.mjs` prints the prototype's.
//!
//!     cargo run -p ledger-math --example views -- <ledger.json> <today yyyy-mm-dd>

use ledger_domain::{Ledger, Money};
use ledger_math::calendar::Day;
use ledger_math::{dashboard, planning, spending};
use rust_decimal::prelude::ToPrimitive;

fn f(m: Money) -> String {
    m.to_string()
}

fn p1(v: Option<f64>) -> String {
    v.map(|v| format!("{:.1}", (v * 10.0).round() / 10.0))
        .unwrap_or_else(|| "-".into())
}

fn short(id: &str) -> &str {
    &id[..id.len().min(8)]
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let (Some(path), Some(today)) = (args.first(), args.get(1).and_then(|t| Day::parse(t))) else {
        eprintln!("usage: views <ledger.json> <today yyyy-mm-dd>");
        std::process::exit(2);
    };
    let raw = std::fs::read(path).expect("readable");
    let doc = Ledger::from_bytes(&raw).expect("a ledger");

    println!("goals");
    for g in &doc.goals {
        println!(
            "  {} saved {} progress {}",
            short(&g.id),
            f(ledger_math::goal_saved(&doc, g)),
            p1(ledger_math::goal_progress(&doc, g))
        );
    }

    println!("planning");
    for months in [3, 12, 24] {
        let (y, m, d) = today.ymd();
        let to = Day::from_ymd(y, m + months, d);
        let plan = planning::project(&doc, today, to);
        println!("  {months}m months {}", p1(plan.months.to_f64()));
        for b in plan.buckets {
            if b.current.is_zero() && b.contributions.is_zero() && b.deductions.is_zero() {
                continue;
            }
            println!(
                "    {} now {} in {} out {} ends {} pct {} draws {}",
                short(&b.id),
                f(b.current),
                f(b.contributions),
                f(b.deductions),
                f(b.projected),
                p1(b.percent),
                b.draws.len()
            );
        }
    }

    println!("spending");
    for period in ["1m", "3m", "1y", "all"] {
        for status in ["all", "settled", "open"] {
            let window = spending::range(period, "", "", today);
            // The oracle reads settle times on this machine's clock; so must this.
            let offset = std::env::var("OFFSET_MINUTES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            let r = spending::analyse(&doc, window, spending::Status::parse(status), offset);
            println!(
                "  {period} {status} total {} count {} withdrawn {} buckets {} everyday {} unattributed {} unitemized {} average {} undated {} inferred {} people {} items {} months {}",
                f(r.total),
                r.count,
                f(r.withdrawn),
                f(r.from_buckets),
                f(r.everyday),
                f(r.unattributed),
                f(r.unitemized),
                f(r.average),
                r.undated,
                r.inferred_dates,
                r.people.len(),
                r.items.len(),
                r.months.len()
            );
        }
    }

    println!("dashboard");
    let kinds: Vec<_> = dashboard::default_dashboard(&doc)
        .widgets
        .into_iter()
        .map(|w| w.kind)
        .collect();
    println!("  default {}", kinds.join(","));
    let c = dashboard::credit(&doc);
    println!(
        "  credit available {} owed {} limit {} count {} recorded {}",
        f(c.available),
        f(c.owed),
        f(c.limit),
        c.count,
        c.recorded
    );
    let offset = std::env::var("OFFSET_MINUTES")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let r = dashboard::reconciling(&doc, today, offset);
    println!(
        "  reconciling open {} balance {} year {} buckets {} days {}",
        r.open,
        f(r.open_balance),
        f(r.year_spending),
        f(r.year_buckets),
        r.days_since
            .map(|d| d.to_string())
            .unwrap_or_else(|| "null".into())
    );
    let ret = dashboard::retirement_split(&doc);
    println!(
        "  retirement roth {} traditional {} contributions {}",
        f(ret.roth),
        f(ret.traditional),
        f(ret.roth_contributions)
    );
    let h = dashboard::holdings_summary(&doc);
    println!(
        "  holdings value {} basis {} gain {} count {}",
        f(h.value),
        f(h.basis),
        f(h.gain),
        h.count
    );
    let refs = |kind: &str| {
        dashboard::suggested_refs(&doc, kind)
            .iter()
            .map(|id| short(id).to_string())
            .collect::<Vec<_>>()
            .join(",")
    };
    println!("  suggested buckets {}", refs("buckets"));
    println!("  suggested accounts {}", refs("accounts"));
}
