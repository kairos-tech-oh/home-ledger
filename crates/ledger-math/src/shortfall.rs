//! Whether the savings buckets can pay for the charges put against them.
//!
//! A settle draws a bucket's lines from its cash. With more than one statement
//! open, each may fit alone while together they need more than the bucket
//! holds; whichever settles second then comes up short. These say so before
//! either is settled.

use ledger_domain::records::Reconciliation;
use ledger_domain::{Ledger, Money};
use rust_decimal::Decimal;

/// What one statement takes from one bucket, against what the bucket holds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Need {
    pub bucket_id: String,
    pub needs: Money,
    /// Its cash, never below zero: an overdrawn bucket has nothing to give.
    pub holds: Money,
    /// Needs less holds, when that is above zero.
    pub short: Money,
}

/// What one statement takes from each bucket: its lines there, a refund
/// netted against the charges, as the settle draws them. A bucket the
/// refunds outweigh takes nothing.
fn needs(record: &Reconciliation) -> Vec<(String, Money)> {
    let mut out: Vec<(String, Money)> = Vec::new();
    for line in record.lines.iter().filter(|l| !l.bucket_id.is_empty()) {
        match out.iter_mut().find(|(b, _)| *b == line.bucket_id) {
            Some(row) => row.1 += line.amount,
            None => out.push((line.bucket_id.clone(), line.amount)),
        }
    }
    out.retain(|(_, m)| m.inner() > Decimal::ZERO);
    out
}

fn against(ledger: &Ledger, wanted: Vec<(String, Money)>) -> Vec<Need> {
    wanted
        .into_iter()
        .filter_map(|(bucket_id, needs)| {
            let holds = ledger.bucket(&bucket_id)?.current_total.floor_at_zero();
            Some(Need {
                short: (needs - holds).floor_at_zero(),
                bucket_id,
                needs,
                holds,
            })
        })
        .collect()
}

/// The buckets one statement would come up short on if settled now.
pub fn short_on(ledger: &Ledger, record: &Reconciliation) -> Vec<Need> {
    against(ledger, needs(record))
        .into_iter()
        .filter(|n| !n.short.is_zero())
        .collect()
}

/// What every open statement together takes from each bucket. Each is
/// netted alone: a refund on one statement does not pay for another's charges.
pub fn committed(ledger: &Ledger) -> Vec<Need> {
    let mut total: Vec<(String, Money)> = Vec::new();
    for (bucket_id, m) in ledger
        .reconciliations
        .iter()
        .filter(|r| r.status != "settled")
        .flat_map(needs)
    {
        match total.iter_mut().find(|(b, _)| *b == bucket_id) {
            Some(row) => row.1 += m,
            None => total.push((bucket_id, m)),
        }
    }
    against(ledger, total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::records::{Bucket, ReconLine};

    fn bucket(id: &str, cash: i64) -> Bucket {
        Bucket {
            id: id.repeat(32),
            name: id.into(),
            current_total: Money::from(cash),
            ..Default::default()
        }
    }

    fn statement(status: &str, lines: &[(&str, i64)]) -> Reconciliation {
        Reconciliation {
            status: status.into(),
            lines: lines
                .iter()
                .map(|(b, amount)| ReconLine {
                    bucket_id: if b.is_empty() {
                        String::new()
                    } else {
                        b.repeat(32)
                    },
                    amount: Money::from(*amount),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        }
    }

    fn ledger() -> Ledger {
        Ledger {
            buckets: vec![bucket("g", 120), bucket("t", 500)],
            reconciliations: vec![
                statement("open", &[("g", 80), ("g", 20), ("t", 50), ("", 30)]),
                statement("open", &[("g", 70)]),
                statement("settled", &[("g", 999)]),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn each_statement_alone_fits_but_together_they_are_short() {
        let doc = ledger();
        assert!(short_on(&doc, &doc.reconciliations[0]).is_empty());
        assert!(short_on(&doc, &doc.reconciliations[1]).is_empty());
        let all = committed(&doc);
        let g = all.iter().find(|n| n.bucket_id == "g".repeat(32)).unwrap();
        // 100 and 70 across the open ones; the settled one no longer counts.
        assert_eq!(
            (g.needs, g.holds, g.short),
            (Money::from(170), Money::from(120), Money::from(50))
        );
        let t = all.iter().find(|n| n.bucket_id == "t".repeat(32)).unwrap();
        assert!(t.short.is_zero());
    }

    #[test]
    fn a_statement_short_by_itself_says_by_how_much() {
        let mut doc = ledger();
        doc.buckets[0].current_total = Money::from(60);
        let short = short_on(&doc, &doc.reconciliations[0]);
        assert_eq!(short.len(), 1);
        assert_eq!(short[0].short, Money::from(40));
    }

    #[test]
    fn a_refund_lowers_what_its_statement_takes_from_the_bucket() {
        let mut doc = ledger();
        doc.buckets[0].current_total = Money::from(60);
        // 100 of charges on g, short 40, until a 50 refund on g.
        doc.reconciliations[0]
            .lines
            .push(statement("open", &[("g", -50)]).lines.remove(0));
        assert!(short_on(&doc, &doc.reconciliations[0]).is_empty());
        let g = committed(&doc)
            .into_iter()
            .find(|n| n.bucket_id == "g".repeat(32))
            .unwrap();
        assert_eq!(g.needs, Money::from(120), "50 net here and 70 on the other");
    }

    #[test]
    fn a_refund_on_one_statement_does_not_pay_for_another() {
        let mut doc = ledger();
        doc.reconciliations[0]
            .lines
            .push(statement("open", &[("g", -300)]).lines.remove(0));
        let g = committed(&doc)
            .into_iter()
            .find(|n| n.bucket_id == "g".repeat(32))
            .unwrap();
        assert_eq!(g.needs, Money::from(70));
    }

    #[test]
    fn an_overdrawn_bucket_holds_nothing_to_give() {
        let mut doc = ledger();
        doc.buckets[0].current_total = Money::from(-25);
        let short = short_on(&doc, &doc.reconciliations[1]);
        assert_eq!(
            (short[0].holds, short[0].short),
            (Money::ZERO, Money::from(70))
        );
    }
}
