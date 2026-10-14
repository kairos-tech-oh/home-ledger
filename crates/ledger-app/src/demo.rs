//! Demo mode: the screens show a copy of the ledger with every amount made
//! up, and edits change only that copy.
//!
//! Each record is scaled by its own factor, so an account, its buckets and
//! every total built from them still agree with each other, and none of them
//! is the real figure. Market prices stay real, since they are public;
//! quantities are scaled instead. Nothing here is ever queued, saved, synced
//! or recorded in the history.
//!
//! It is switched on by the desktop app for its own session only. The `hl`
//! command line never turns it on, so a script always sees the real ledger.

use ledger_domain::records::{AuditEntry, Reconciliation};
use ledger_domain::{Ledger, Money};
use ledger_math::snapshots::Point;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromPrimitive;

pub struct Demo {
    seed: u64,
    pub ledger: Ledger,
    /// Edits made while demo mode is on, oldest first.
    pub edits: Vec<AuditEntry>,
    /// Today's made-up figures over the real ones, for drawing the
    /// net-worth history at the same made-up scale.
    ratios: Ratios,
}

#[derive(Default)]
struct Ratios {
    net: Decimal,
    assets: Decimal,
    debts: Decimal,
    savings: Decimal,
    holdings: Decimal,
    basis: Decimal,
    income: Decimal,
    budget: Decimal,
    buckets: Vec<(String, Decimal)>,
}

impl Demo {
    pub fn new(real: &Ledger, seed: &str) -> Self {
        let seed = hash(0, seed);
        let ledger = scramble(real, seed);
        let ratios = ratios(real, &ledger, seed);
        Self {
            seed,
            ledger,
            edits: Vec::new(),
            ratios,
        }
    }

    /// A history entry with its amounts made up, as the ledger's are.
    pub fn entry(&self, mut entry: AuditEntry) -> AuditEntry {
        let f = factor(self.seed, &entry.id);
        entry.amount = entry.amount.map(|m| scale(m, f));
        for change in entry.changes.iter_mut() {
            change.from = scale_text(&change.from, f);
            change.to = scale_text(&change.to, f);
        }
        entry
    }

    /// The net-worth history at today's made-up scale.
    pub fn points(&self, real: &[Point]) -> Vec<Point> {
        let r = &self.ratios;
        real.iter()
            .cloned()
            .map(|mut p| {
                p.net = by(p.net, r.net);
                p.assets = by(p.assets, r.assets);
                p.debts = by(p.debts, r.debts);
                p.savings = by(p.savings, r.savings);
                p.holdings = by(p.holdings, r.holdings);
                p.basis = by(p.basis, r.basis);
                p.income_monthly = by(p.income_monthly, r.income);
                p.budget_monthly = by(p.budget_monthly, r.budget);
                for b in p.buckets.iter_mut() {
                    let ratio = r
                        .buckets
                        .iter()
                        .find(|(id, _)| *id == b.id)
                        .map(|(_, ratio)| *ratio)
                        .unwrap_or_else(|| factor(self.seed, &b.id));
                    b.total = by(b.total, ratio);
                }
                p
            })
            .collect()
    }
}

/// The ledger with every amount scaled, record by record, and then the
/// figures worked out from others worked out again.
pub fn scramble(real: &Ledger, seed: u64) -> Ledger {
    let mut l = real.clone();
    for a in l.accounts.iter_mut() {
        let f = factor(seed, &a.id);
        a.total = a.total.map(|m| scale(m, f));
        a.available_credit = a.available_credit.map(|m| scale(m, f));
        a.contributions_amount = a.contributions_amount.map(|m| scale(m, f));
        for allocation in a.contribution_allocations.iter_mut() {
            allocation.amount = scale(allocation.amount, f);
        }
        // Rounded one by one, the shares could come to a cent more than
        // the whole.
        let spread: Money = a.contribution_allocations.iter().map(|x| x.amount).sum();
        if let Some(held) = a.contributions_amount
            && spread > held
        {
            a.contributions_amount = Some(spread);
        }
        for debt in a.debts.iter_mut() {
            debt.balance = scale(debt.balance, f);
            debt.payment = scale(debt.payment, f);
        }
        if let Some(r) = a.retirement.as_mut() {
            r.monthly_contribution = r.monthly_contribution.map(|m| scale(m, f));
        }
    }
    for i in l.income.iter_mut() {
        i.monthly_total = scale(i.monthly_total, factor(seed, &i.id));
    }
    for b in l.budget.iter_mut() {
        let f = factor(seed, &b.id);
        b.monthly_amount = scale(b.monthly_amount, f);
        for e in b.expenses.iter_mut() {
            e.amount = scale(e.amount, f);
        }
    }
    for b in l.buckets.iter_mut() {
        let f = factor(seed, &b.id);
        b.current_total = scale(b.current_total, f);
        b.target_amount = b.target_amount.map(|m| scale(m, f));
    }
    for h in l.investments.iter_mut() {
        let f = factor(seed, &h.id);
        h.quantity = scale_quantity(h.quantity, f);
        h.cost_basis = h.cost_basis.map(|m| scale(m, f));
        for t in h.trades.iter_mut() {
            t.quantity = scale_quantity(t.quantity, f);
        }
    }
    for g in l.goals.iter_mut() {
        g.target_amount = g.target_amount.map(|m| scale(m, factor(seed, &g.id)));
    }
    for t in l.templates.iter_mut() {
        let f = factor(seed, &t.id);
        for item in t.items.iter_mut() {
            item.monthly_amount = scale(item.monthly_amount, f);
        }
    }
    for r in l.reconciliations.iter_mut() {
        statement(r, factor(seed, &r.id));
    }
    ledger_writer::reconcile::card_balances(&mut l);
    ledger_writer::bucket_balances(&mut l);
    l
}

fn statement(r: &mut Reconciliation, f: Decimal) {
    let added_up = r.lines.iter().map(|l| l.amount).sum::<Money>() == r.balance;
    for line in r.lines.iter_mut() {
        line.amount = scale(line.amount, f);
    }
    // A statement that added up still does, so it can still be settled.
    r.balance = if added_up {
        r.lines.iter().map(|l| l.amount).sum()
    } else {
        scale(r.balance, f)
    };
    if let Some(applied) = r.applied.as_mut() {
        for m in applied
            .buckets
            .iter_mut()
            .chain(applied.accounts.iter_mut())
        {
            m.amount = scale(m.amount, f);
            m.credit_delta = m.credit_delta.map(|c| scale(c, f));
        }
    }
}

fn ratios(real: &Ledger, fake: &Ledger, seed: u64) -> Ratios {
    let day = ledger_math::calendar::Day::parse("2000-01-01").expect("a real day");
    let was = ledger_math::snapshots::build(real, day, "");
    let now = ledger_math::snapshots::build(fake, day, "");
    let over = |n: Money, d: Money, key: &str| {
        if d.is_zero() {
            factor(seed, key)
        } else {
            n.inner() / d.inner()
        }
    };
    Ratios {
        net: over(now.net, was.net, "net"),
        assets: over(now.assets, was.assets, "assets"),
        debts: over(now.debts, was.debts, "debts"),
        savings: over(now.savings, was.savings, "savings"),
        holdings: over(now.holdings, was.holdings, "holdings"),
        basis: over(now.basis, was.basis, "basis"),
        income: over(now.income_monthly, was.income_monthly, "income"),
        budget: over(now.budget_monthly, was.budget_monthly, "budget"),
        buckets: was
            .buckets
            .iter()
            .filter_map(|b| {
                let made_up = now.buckets.iter().find(|x| x.id == b.id)?;
                Some((b.id.clone(), over(made_up.total, b.total, &b.id)))
            })
            .collect(),
    }
}

/// A record's own scale: from a third to two and a half times, and never
/// close enough to one to leave a figure near the real one.
fn factor(seed: u64, key: &str) -> Decimal {
    let h = hash(seed, key);
    let unit = (h >> 11) as f64 / (1u64 << 53) as f64;
    let (low, high) = (0.35f64, 2.5f64);
    let mut f = (low.ln() + unit * (high / low).ln()).exp();
    if (0.85..1.0).contains(&f) {
        f *= 0.7;
    } else if (1.0..1.15).contains(&f) {
        f *= 1.4;
    }
    Decimal::from_f64(f).unwrap_or(Decimal::TWO).round_dp(4)
}

/// FNV-1a, finished with a mix so neighbouring ids land far apart.
fn hash(seed: u64, key: &str) -> u64 {
    let mut h = 0xcbf2_9ce4_8422_2325u64 ^ seed;
    for b in key.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h ^= h >> 33;
    h = h.wrapping_mul(0xff51_afd7_ed55_8ccd);
    h ^= h >> 33;
    h
}

fn scale(m: Money, f: Decimal) -> Money {
    Money::new(m.inner() * f)
}

fn scale_quantity(m: Money, f: Decimal) -> Money {
    Money::price(m.inner() * f)
}

fn by(m: Money, ratio: Decimal) -> Money {
    Money::new(m.inner() * ratio)
}

/// A figure written into a history line, scaled; anything else as it was.
fn scale_text(text: &str, f: Decimal) -> String {
    match text.trim().parse::<Decimal>() {
        Ok(n) if text.contains('.') => Money::new(n * f).to_string(),
        _ => text.to_string(),
    }
}

/// Whether a factor turned a figure into itself: only zero should.
#[cfg(test)]
fn moved(real: Money, made_up: Money) -> bool {
    real.is_zero() || real != made_up
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::records::{Account, Bucket, ReconLine};
    use rust_decimal::prelude::ToPrimitive;

    fn ledger() -> Ledger {
        let mut l = Ledger::default();
        l.accounts.push(Account {
            id: "a".repeat(32),
            name: "Fidelity Savings".into(),
            kind: "savings".into(),
            total: Some(Money::from(3000)),
            ..Default::default()
        });
        l.accounts.push(Account {
            id: "c".repeat(32),
            name: "Chase Joint".into(),
            kind: "checking".into(),
            total: Some(Money::from(1255)),
            ..Default::default()
        });
        for (id, cash) in [("1", 1000), ("2", 2000)] {
            l.buckets.push(Bucket {
                id: id.repeat(32),
                name: format!("Bucket {id}"),
                current_total: Money::from(cash),
                linked_account_id: "a".repeat(32),
                ..Default::default()
            });
        }
        l.reconciliations.push(Reconciliation {
            id: "r".repeat(32),
            balance: Money::from(30),
            lines: vec![
                ReconLine {
                    amount: Money::new(Decimal::new(1001, 2)),
                    ..Default::default()
                },
                ReconLine {
                    amount: Money::new(Decimal::new(1999, 2)),
                    ..Default::default()
                },
            ],
            status: "open".into(),
            ..Default::default()
        });
        l
    }

    #[test]
    fn no_amount_is_left_as_it_was_and_the_totals_still_agree() {
        let real = ledger();
        let demo = Demo::new(&real, "seed");
        let fake = &demo.ledger;
        for (r, f) in real.accounts.iter().zip(&fake.accounts) {
            assert!(moved(r.total.unwrap(), f.total.unwrap()), "{}", r.name);
        }
        for (r, f) in real.buckets.iter().zip(&fake.buckets) {
            assert!(moved(r.current_total, f.current_total), "{}", r.name);
        }
        // The account still holds exactly what its buckets hold.
        let sum: Money = fake.buckets.iter().map(|b| b.current_total).sum();
        assert_eq!(fake.accounts[0].total, Some(sum));
        // A statement that added up still does.
        let s = &fake.reconciliations[0];
        assert_eq!(s.lines.iter().map(|l| l.amount).sum::<Money>(), s.balance);
        assert_ne!(s.balance, Money::from(30));
    }

    #[test]
    fn the_same_seed_makes_up_the_same_figures_and_another_does_not() {
        let real = ledger();
        let a = Demo::new(&real, "one").ledger;
        let b = Demo::new(&real, "one").ledger;
        let c = Demo::new(&real, "two").ledger;
        assert_eq!(a.accounts[1].total, b.accounts[1].total);
        assert_ne!(a.accounts[1].total, c.accounts[1].total);
    }

    #[test]
    fn every_factor_stays_in_range_and_away_from_one() {
        for i in 0..5000 {
            let f = factor(7, &format!("id{i}")).to_f64().unwrap();
            assert!((0.24..=2.5).contains(&f), "{f}");
            assert!(!(0.85..1.15).contains(&f), "{f}");
        }
    }

    #[test]
    fn the_history_is_drawn_at_todays_made_up_scale() {
        let real = ledger();
        let demo = Demo::new(&real, "seed");
        let day = ledger_math::calendar::Day::parse("2026-10-14").unwrap();
        let today = ledger_math::snapshots::build(&real, day, "");
        let drawn = demo.points(std::slice::from_ref(&today));
        let made_up = ledger_math::snapshots::build(&demo.ledger, day, "");
        assert_eq!(drawn[0].net, made_up.net);
        assert_eq!(drawn[0].savings, made_up.savings);
    }

    fn machine(root: &std::path::Path) -> crate::AppState {
        use ledger_config::{Config, Places, Settings, StoreConfig};
        let places = Places {
            config_file: root.join("config.json"),
            data_dir: root.join("data"),
        };
        places
            .save_config(&Config {
                stores: vec![StoreConfig {
                    id: "local".into(),
                    label: "local".into(),
                    settings: Settings::Local {
                        path: root.join("ledger.json"),
                    },
                    accept_risk: false,
                }],
                device: "Office PC".into(),
                setup_complete: true,
                ..Config::default()
            })
            .unwrap();
        crate::AppState::open(
            places,
            std::sync::Arc::new(ledger_config::secrets::InMemory::default()),
            true,
            || None,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn demo_edits_change_only_the_made_up_copy() {
        use crate::commands::{apply_value, demo_set, ledger_of, shown};
        use serde_json::json;
        let dir = tempfile::tempdir().unwrap();
        let state = machine(dir.path());
        apply_value(
            &state,
            json!({ "op": "set", "kind": "bucket",
                    "record": { "name": "Emergency", "currentTotal": 500 } }),
        )
        .await
        .unwrap();
        let id = ledger_of(&state).await.buckets[0].id.clone();
        let history = state.audit().read().await.len();

        assert!(demo_set(&state, Some("seed".into())).await.unwrap());
        let made_up = shown(&state).await.unwrap().ledger.buckets[0].current_total;
        assert_ne!(made_up, Money::from(500));

        apply_value(
            &state,
            json!({ "op": "bucket-adjust", "adjustments": [{ "id": id, "delta": 100 }] }),
        )
        .await
        .unwrap();
        let shown_now = shown(&state).await.unwrap();
        assert_eq!(
            shown_now.ledger.buckets[0].current_total,
            made_up + Money::from(100)
        );
        assert_eq!(shown_now.from, "demo");

        // The real ledger, its history and its outbox never saw it.
        assert_eq!(
            ledger_of(&state).await.buckets[0].current_total,
            Money::from(500)
        );
        assert_eq!(state.audit().read().await.len(), history);
        let seen = crate::views::history(&state).await.unwrap();
        assert_eq!(seen.entries.len(), history + 1, "the demo edit is listed");
        assert!(state.not_in_demo("fetch from the bank").is_err());

        // Off again: the real figures, and the demo edit gone.
        assert!(!demo_set(&state, None).await.unwrap());
        assert_eq!(
            shown(&state).await.unwrap().ledger.buckets[0].current_total,
            Money::from(500)
        );
        assert!(state.not_in_demo("fetch from the bank").is_ok());
    }

    #[test]
    fn a_history_line_has_its_figures_made_up_and_its_words_kept() {
        let demo = Demo::new(&ledger(), "seed");
        let entry = demo.entry(AuditEntry {
            id: "e".repeat(32),
            amount: Some(Money::from(100)),
            changes: vec![ledger_domain::records::AuditChange {
                field: "Emergency".into(),
                from: "6245.14".into(),
                to: "Fidelity Savings".into(),
            }],
            ..Default::default()
        });
        assert_ne!(entry.amount, Some(Money::from(100)));
        assert_ne!(entry.changes[0].from, "6245.14");
        assert_eq!(entry.changes[0].to, "Fidelity Savings");
        assert_eq!(entry.changes[0].field, "Emergency");
    }
}
