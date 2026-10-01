//! A payday: what one earner puts into each savings bucket from one paycheck.
//!
//! Each bucket is funded by the budget lines pointed at it. That monthly
//! amount is shared between the earners in proportion to what each brings in,
//! and each earner's share is spread over the paychecks they actually receive.
//!
//! Port of `bucketContributions`, `contributionDeltas` and `contributionTotal`
//! in `core/Model.js`, and rounded the way it rounds: once, at the paycheck.
//! Sharing the monthly amount to the cent first (as [`crate::split`] does for a
//! budget line) and then dividing by paychecks rounds twice, and came out a
//! cent high on a third of the real ledger's buckets.

use crate::{Earner, bucket_funding, owner_shares};
use ledger_domain::{Ledger, Money};
use rust_decimal::Decimal;

/// One earner's paycheck into one bucket.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contribution {
    pub bucket_id: String,
    pub amount: Money,
}

/// Everything one earner adds on payday.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Payday {
    pub owner: String,
    pub total: Money,
    /// Only buckets this earner actually funds, in the ledger's bucket order.
    pub contributions: Vec<Contribution>,
}

fn into_bucket(ledger: &Ledger, bucket_id: &str, earners: &[Earner]) -> Vec<(String, Money)> {
    let monthly = bucket_funding(ledger, bucket_id).monthly.inner();
    earners
        .iter()
        .map(|e| {
            let amount = if e.paychecks_per_month.is_zero() {
                Decimal::ZERO
            } else {
                monthly * e.percent / Decimal::from(100) / e.paychecks_per_month
            };
            (e.owner.clone(), Money::new(amount))
        })
        .collect()
}

/// Every earner's payday, largest earner first, leaving out anyone who funds
/// nothing.
pub fn paydays(ledger: &Ledger) -> Vec<Payday> {
    let earners = owner_shares(ledger);
    let mut out: Vec<Payday> = earners
        .iter()
        .map(|e| Payday {
            owner: e.owner.clone(),
            total: Money::ZERO,
            contributions: Vec::new(),
        })
        .collect();
    for bucket in &ledger.buckets {
        for (owner, amount) in into_bucket(ledger, &bucket.id, &earners) {
            if amount.inner() <= Decimal::ZERO {
                continue;
            }
            if let Some(day) = out.iter_mut().find(|d| d.owner == owner) {
                day.total += amount;
                day.contributions.push(Contribution {
                    bucket_id: bucket.id.clone(),
                    amount,
                });
            }
        }
    }
    out.retain(|d| !d.contributions.is_empty());
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::records::{Bucket, BudgetItem, IncomeStream};

    fn ledger() -> Ledger {
        let mut doc = Ledger::default();
        // Chris earns two thirds and is paid every two weeks; Yuki a third,
        // twice a month.
        doc.income.push(IncomeStream {
            owner: "Chris".into(),
            monthly_total: Money::from(6_000),
            frequency: "biweekly".into(),
            ..Default::default()
        });
        doc.income.push(IncomeStream {
            owner: "Yuki".into(),
            monthly_total: Money::from(3_000),
            frequency: "semimonthly".into(),
            ..Default::default()
        });
        for (id, name) in [("g", "Groceries"), ("t", "Travel"), ("x", "Unfunded")] {
            doc.buckets.push(Bucket {
                id: id.repeat(32),
                name: name.into(),
                ..Default::default()
            });
        }
        doc.budget.push(BudgetItem {
            name: "Groceries".into(),
            monthly_amount: Money::from(900),
            bucket_id: "g".repeat(32),
            ..Default::default()
        });
        doc.budget.push(BudgetItem {
            name: "Travel".into(),
            monthly_amount: Money::from(300),
            bucket_id: "t".repeat(32),
            ..Default::default()
        });
        doc
    }

    #[test]
    fn each_earner_pays_their_share_of_each_bucket_per_paycheck() {
        let days = paydays(&ledger());
        let owners: Vec<_> = days.iter().map(|d| d.owner.as_str()).collect();
        assert_eq!(owners, ["Chris", "Yuki"], "largest earner first");

        // Chris: 600 of groceries and 200 of travel a month, over 26/12
        // paychecks a month.
        let chris = &days[0];
        assert_eq!(
            chris.contributions.len(),
            2,
            "an unfunded bucket gets nothing"
        );
        assert_eq!(
            chris.contributions[0].amount,
            Money::new(Decimal::new(27692, 2))
        );
        assert_eq!(
            chris.contributions[1].amount,
            Money::new(Decimal::new(9231, 2))
        );
        assert_eq!(chris.total, Money::new(Decimal::new(36923, 2)));

        // Yuki: 300 and 100 a month over two paychecks.
        let yuki = &days[1];
        assert_eq!(yuki.contributions[0].amount, Money::from(150));
        assert_eq!(yuki.contributions[1].amount, Money::from(50));
        assert_eq!(yuki.total, Money::from(200));
    }

    #[test]
    fn no_income_or_no_funded_buckets_is_no_payday() {
        let mut doc = ledger();
        doc.budget.clear();
        assert!(paydays(&doc).is_empty());
        let mut doc = ledger();
        doc.income.clear();
        assert!(paydays(&doc).is_empty());
    }
}
