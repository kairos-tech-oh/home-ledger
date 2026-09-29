//! Every derived number: rollups, earner splits, bucket totals, P/L,
//! projections. Pure functions of a [`Ledger`] — nothing here writes.
//!
//! Port target: `core/Model.js` (2,188 lines) from the Omarchy plugin.
//! `tools/check-math.mjs` there is the oracle each ported function is
//! checked against. See `docs/PORT.md`.

mod goals;
pub use goals::{GoalTotals, goal_progress, goal_saved, goal_totals, goals_in_order};

use ledger_domain::records::{Account, Holding};
use ledger_domain::{Ledger, Money};
use rust_decimal::Decimal;

/// What the balance sheet says right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct NetWorth {
    pub assets: Money,
    pub debts: Money,
    pub net: Money,
}

/// What a holding is worth now: a live price if there is one, otherwise what
/// it cost. Kept in full precision — a 4dp price times a fractional quantity
/// is rounded once at the end, not per holding.
fn holding_value(holding: &Holding) -> Decimal {
    let price = holding.price.unwrap_or(holding.avg_cost);
    price.inner() * holding.quantity.inner()
}

/// An account's gross worth: the cash typed on it plus everything it holds.
/// A liability contributes nothing here; it is counted as owed instead.
fn account_gross(account: &Account, holdings: &[Holding]) -> Decimal {
    if account.is_liability() {
        return Decimal::ZERO;
    }
    let cash = account.total.unwrap_or(Money::ZERO).inner();
    let held: Decimal = holdings
        .iter()
        .filter(|h| h.account_id == account.id)
        .map(holding_value)
        .sum();
    cash + held
}

/// What an account owes. For a liability the typed total *replaces* the debt
/// lines rather than adding to them — counting both would double it.
fn gross_debts(account: &Account) -> Decimal {
    let lines: Decimal = account.debts.iter().map(|d| d.balance.inner()).sum();
    if account.is_liability() {
        return match account.total {
            Some(total) => total.inner().abs(),
            None => lines.abs(),
        };
    }
    lines
}

/// Assets less debts, across every account and holding.
pub fn net_worth(ledger: &Ledger) -> NetWorth {
    let mut assets = Decimal::ZERO;
    let mut debts = Decimal::ZERO;

    for account in &ledger.accounts {
        assets += account_gross(account, &ledger.investments);
        debts += gross_debts(account);
    }

    // A holding belonging to no account still belongs to the household.
    assets += ledger
        .investments
        .iter()
        .filter(|h| h.account_id.is_empty())
        .map(holding_value)
        .sum::<Decimal>();

    let assets = Money::new(assets);
    let debts = Money::new(debts);
    NetWorth {
        assets,
        debts,
        net: assets - debts,
    }
}

/// What every income stream comes to in a month.
pub fn monthly_income(ledger: &Ledger) -> Money {
    ledger.income.iter().map(|s| s.monthly_total).sum()
}

/// What the budget plans to spend in a month.
pub fn monthly_budget(ledger: &Ledger) -> Money {
    ledger.budget.iter().map(|b| b.monthly_amount).sum()
}

/// Cash held across every savings bucket.
pub fn bucket_cash(ledger: &Ledger) -> Money {
    ledger.buckets.iter().map(|b| b.current_total).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ledger_domain::records::{Bucket, BudgetItem, Debt, IncomeStream};
    use rust_decimal_macros::dec;

    fn account(kind: &str, total: i64) -> Account {
        Account {
            id: ledger_domain::new_id(),
            name: format!("{kind} account"),
            kind: kind.into(),
            total: Some(Money::from(total)),
            ..Default::default()
        }
    }

    #[test]
    fn a_liability_lands_on_the_debt_side() {
        let mut ledger = Ledger::default();
        ledger.accounts.push(account("checking", 1_000));
        ledger.accounts.push(account("credit", 250));

        let worth = net_worth(&ledger);
        assert_eq!(worth.assets, Money::from(1_000));
        assert_eq!(worth.debts, Money::from(250));
        assert_eq!(worth.net, Money::from(750));
    }

    #[test]
    fn a_liability_typed_negative_still_counts_as_owed() {
        // The rule that catches people out: a card balance of -250 is still
        // 250 owed, not 250 held.
        let mut ledger = Ledger::default();
        ledger.accounts.push(account("credit", -250));
        assert_eq!(net_worth(&ledger).debts, Money::from(250));
    }

    #[test]
    fn debt_lines_add_to_what_is_owed() {
        let mut ledger = Ledger::default();
        let mut acct = account("checking", 5_000);
        acct.debts.push(Debt {
            id: ledger_domain::new_id(),
            name: "Car".into(),
            balance: Money::from(12_000),
            ..Default::default()
        });
        ledger.accounts.push(acct);

        let worth = net_worth(&ledger);
        assert_eq!(worth.assets, Money::from(5_000));
        assert_eq!(worth.debts, Money::from(12_000));
        assert_eq!(worth.net, Money::from(-7_000));
    }

    fn holding(account_id: &str, qty: i64, price: i64) -> Holding {
        Holding {
            id: ledger_domain::new_id(),
            name: "A fund".into(),
            account_id: account_id.into(),
            quantity: Money::from(qty),
            price: Some(Money::from(price)),
            ..Default::default()
        }
    }

    #[test]
    fn holdings_count_toward_the_account_that_holds_them() {
        let mut ledger = Ledger::default();
        let acct = account("investment", 1_000);
        let id = acct.id.clone();
        ledger.accounts.push(acct);
        ledger.investments.push(holding(&id, 10, 250));

        // Cash plus 10 x 250.
        assert_eq!(net_worth(&ledger).assets, Money::from(3_500));
    }

    #[test]
    fn a_holding_with_no_account_still_belongs_to_the_household() {
        let mut ledger = Ledger::default();
        ledger.investments.push(holding("", 4, 100));
        assert_eq!(net_worth(&ledger).assets, Money::from(400));
    }

    #[test]
    fn a_holding_with_no_price_falls_back_to_what_it_cost() {
        let mut ledger = Ledger::default();
        let mut h = holding("", 3, 0);
        h.price = None;
        h.avg_cost = Money::from(50);
        ledger.investments.push(h);
        assert_eq!(net_worth(&ledger).assets, Money::from(150));
    }

    #[test]
    fn a_liability_total_replaces_its_debt_lines_rather_than_adding_to_them() {
        // The bug this guards was six figures out on real data: counting the card's
        // typed balance AND its debt lines doubled everything owed.
        let mut ledger = Ledger::default();
        let mut card = account("credit", 250);
        card.debts.push(Debt {
            id: ledger_domain::new_id(),
            name: "same balance, itemised".into(),
            balance: Money::from(250),
            ..Default::default()
        });
        ledger.accounts.push(card);

        assert_eq!(net_worth(&ledger).debts, Money::from(250));
    }

    #[test]
    fn a_liability_with_no_typed_total_falls_back_to_its_debt_lines() {
        let mut ledger = Ledger::default();
        let mut card = account("credit", 0);
        card.total = None;
        card.debts.push(Debt {
            id: ledger_domain::new_id(),
            name: "Card".into(),
            balance: Money::from(75),
            ..Default::default()
        });
        ledger.accounts.push(card);

        assert_eq!(net_worth(&ledger).debts, Money::from(75));
    }

    #[test]
    fn a_liability_never_adds_to_assets() {
        let mut ledger = Ledger::default();
        ledger.accounts.push(account("credit", 250));
        assert_eq!(net_worth(&ledger).assets, Money::ZERO);
    }

    fn stream(owner: &str, monthly: i64, frequency: &str) -> IncomeStream {
        IncomeStream {
            id: ledger_domain::new_id(),
            name: format!("{owner} job"),
            owner: owner.into(),
            monthly_total: Money::from(monthly),
            frequency: frequency.into(),
            ..Default::default()
        }
    }

    fn earners(streams: Vec<IncomeStream>) -> Vec<Earner> {
        owner_shares(&Ledger {
            income: streams,
            ..Ledger::default()
        })
    }

    #[test]
    fn earners_come_from_whoever_the_streams_name() {
        // Not two hardcoded people: one earner or three needs no change.
        let shares = earners(vec![
            stream("Sam", 3_000, "biweekly"),
            stream("Alex", 1_000, "monthly"),
            stream("Sam", 1_000, "biweekly"),
        ]);

        assert_eq!(shares.len(), 2);
        assert_eq!(shares[0].owner, "Sam");
        assert_eq!(shares[0].monthly, Money::from(4_000));
        assert_eq!(shares[0].count, 2);
        assert_eq!(shares[1].owner, "Alex");
    }

    #[test]
    fn a_stream_with_no_owner_is_still_counted() {
        let shares = earners(vec![stream("", 2_000, "monthly")]);
        assert_eq!(shares[0].owner, "Unassigned");
        assert_eq!(shares[0].monthly, Money::from(2_000));
    }

    #[test]
    fn paychecks_come_from_the_real_cadence_not_a_flat_two() {
        // The app this replaces divided by 2 for everyone, so somebody paid
        // weekly and somebody paid monthly got the same per-paycheck figure.
        let shares = earners(vec![
            stream("Weekly", 5_200, "weekly"),
            stream("Monthly", 5_200, "monthly"),
        ]);
        let weekly = shares.iter().find(|e| e.owner == "Weekly").unwrap();
        let monthly = shares.iter().find(|e| e.owner == "Monthly").unwrap();

        assert_eq!(weekly.paychecks_per_month, dec!(52) / dec!(12));
        assert_eq!(monthly.paychecks_per_month, Decimal::ONE);
        assert_ne!(
            per_paycheck(Money::from(600), weekly),
            per_paycheck(Money::from(600), monthly)
        );
    }

    #[test]
    fn each_cadence_pays_the_number_of_times_it_says() {
        assert_eq!(periods_per_year("weekly"), 52);
        assert_eq!(periods_per_year("biweekly"), 26);
        assert_eq!(periods_per_year("semimonthly"), 24);
        assert_eq!(periods_per_year("monthly"), 12);
        assert_eq!(periods_per_year("quarterly"), 4);
        assert_eq!(periods_per_year("annual"), 1);
        // Anything unrecognised falls back to the common one.
        assert_eq!(periods_per_year("nonsense"), 26);
    }

    // ------------------------------------------------------- splitting

    fn total_of(shares: &[Share]) -> Money {
        shares.iter().map(|s| s.amount).sum()
    }

    #[test]
    fn a_split_is_proportional_to_what_each_brings_in() {
        let shares = split(
            Money::from(1_000),
            &earners(vec![
                stream("Sam", 3_000, "monthly"),
                stream("Alex", 1_000, "monthly"),
            ]),
        );
        assert_eq!(shares[0].amount, Money::from(750));
        assert_eq!(shares[1].amount, Money::from(250));
    }

    #[test]
    fn three_equal_earners_splitting_ten_pounds_lose_nothing() {
        // The bug this exists to prevent: 3.33 each rounds away a penny, and
        // the household's books stop adding up.
        let shares = split(
            Money::from(10),
            &earners(vec![
                stream("A", 1_000, "monthly"),
                stream("B", 1_000, "monthly"),
                stream("C", 1_000, "monthly"),
            ]),
        );
        assert_eq!(total_of(&shares), Money::from(10));
        // One of them carries the extra penny rather than it vanishing.
        let amounts: Vec<String> = shares.iter().map(|s| s.amount.to_string()).collect();
        assert!(amounts.contains(&"3.34".to_string()), "{amounts:?}");
    }

    #[test]
    fn the_shares_always_add_back_up_to_the_amount() {
        // Swept rather than spot-checked: rounding errors hide in particular
        // combinations, not in the obvious ones.
        let people = earners(vec![
            stream("A", 3_333, "monthly"),
            stream("B", 2_111, "biweekly"),
            stream("C", 1_777, "weekly"),
        ]);
        for cents in 1..500u64 {
            let amount = Money::new(Decimal::from(cents) / dec!(100));
            let shares = split(amount, &people);
            assert_eq!(
                total_of(&shares),
                amount,
                "{amount} did not split back to itself: {shares:?}"
            );
        }
    }

    #[test]
    fn a_negative_amount_splits_and_still_sums() {
        let people = earners(vec![
            stream("A", 2_000, "monthly"),
            stream("B", 1_000, "monthly"),
        ]);
        let shares = split(Money::from(-10), &people);
        assert_eq!(total_of(&shares), Money::from(-10));
        assert!(shares.iter().all(|s| s.amount.is_negative()));
    }

    #[test]
    fn nobody_earning_anything_splits_to_nothing_rather_than_dividing_by_zero() {
        let people = earners(vec![stream("A", 0, "monthly")]);
        let shares = split(Money::from(100), &people);
        assert_eq!(shares.len(), 1);
        assert_eq!(shares[0].amount, Money::ZERO);
    }

    #[test]
    fn splitting_between_nobody_gives_nothing() {
        assert!(split(Money::from(100), &[]).is_empty());
    }

    #[test]
    fn the_same_split_comes_out_the_same_way_every_time() {
        // Whoever carries the spare penny must not wander between runs.
        let people = earners(vec![
            stream("A", 1_000, "monthly"),
            stream("B", 1_000, "monthly"),
            stream("C", 1_000, "monthly"),
        ]);
        let first = split(Money::new(dec!(10.01)), &people);
        for _ in 0..20 {
            assert_eq!(split(Money::new(dec!(10.01)), &people), first);
        }
    }

    #[test]
    fn a_bucket_is_cash_plus_what_is_invested_in_its_name() {
        let mut ledger = Ledger::default();
        ledger.buckets.push(Bucket {
            id: "emergency".into(),
            name: "Emergency".into(),
            current_total: Money::from(500),
            ..Default::default()
        });
        let mut held = holding("", 10, 50);
        held.bucket_id = "emergency".into();
        ledger.investments.push(held);
        // A holding assigned nowhere belongs to no bucket.
        ledger.investments.push(holding("", 10, 50));

        let worth = bucket_worth(&ledger, "emergency");
        assert_eq!(worth.cash, Money::from(500));
        assert_eq!(worth.invested, Money::from(500));
        assert_eq!(worth.total, Money::from(1_000));
    }

    #[test]
    fn roth_contributions_are_counted_by_what_was_put_in() {
        // Only contributions come out of a Roth without penalty, so a bucket
        // is credited with the basis rather than the value of the holdings.
        let mut ledger = Ledger::default();
        ledger.buckets.push(Bucket {
            id: "retirement".into(),
            name: "Retirement".into(),
            ..Default::default()
        });
        ledger.accounts.push(Account {
            id: ledger_domain::new_id(),
            name: "Roth".into(),
            kind: "retirement-roth".into(),
            contributions_amount: Some(Money::from(7_000)),
            contribution_allocations: vec![ledger_domain::records::Allocation {
                bucket_id: "retirement".into(),
                amount: Money::from(7_000),
            }],
            ..Default::default()
        });

        assert_eq!(
            bucket_worth(&ledger, "retirement").contributions,
            Money::from(7_000)
        );
    }

    #[test]
    fn an_allocation_on_a_traditional_account_is_not_counted() {
        // Money in a Traditional cannot be withdrawn the way contributions can.
        let mut ledger = Ledger::default();
        ledger.buckets.push(Bucket {
            id: "retirement".into(),
            ..Default::default()
        });
        ledger.accounts.push(Account {
            id: ledger_domain::new_id(),
            name: "401k".into(),
            kind: "retirement-traditional".into(),
            contribution_allocations: vec![ledger_domain::records::Allocation {
                bucket_id: "retirement".into(),
                amount: Money::from(7_000),
            }],
            ..Default::default()
        });

        assert_eq!(
            bucket_worth(&ledger, "retirement").contributions,
            Money::ZERO
        );
    }

    #[test]
    fn every_budget_line_pointing_at_a_bucket_funds_it() {
        // The app this replaces summed all of them, not just the first.
        let mut ledger = Ledger::default();
        ledger.budget.push(BudgetItem {
            name: "Savings".into(),
            bucket_id: "emergency".into(),
            monthly_amount: Money::from(200),
            ..Default::default()
        });
        ledger.budget.push(BudgetItem {
            name: "Extra".into(),
            bucket_id: "emergency".into(),
            monthly_amount: Money::from(50),
            ..Default::default()
        });
        ledger.budget.push(BudgetItem {
            name: "Rent".into(),
            monthly_amount: Money::from(1_200),
            ..Default::default()
        });

        let funding = bucket_funding(&ledger, "emergency");
        assert_eq!(funding.monthly, Money::from(250));
        assert_eq!(funding.names, vec!["Savings", "Extra"]);
    }

    fn draw(
        amount: i64,
        recurring: bool,
        interval: &str,
    ) -> ledger_domain::records::PlannedExpense {
        ledger_domain::records::PlannedExpense {
            id: ledger_domain::new_id(),
            name: "A bill".into(),
            amount: Money::from(amount),
            recurring,
            interval: interval.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_premium_every_six_months_reads_as_a_monthly_figure() {
        // The example from the app this replaces: 1,590 twice a year is 265
        // a month, which is what makes it comparable to the budget line.
        assert_eq!(
            expense_monthly(&draw(1_590, true, "every6months")),
            Money::from(265)
        );
    }

    #[test]
    fn each_interval_spreads_over_the_year_it_actually_repeats_in() {
        assert_eq!(expense_monthly(&draw(120, true, "yearly")), Money::from(10));
        assert_eq!(
            expense_monthly(&draw(30, true, "quarterly")),
            Money::from(10)
        );
        assert_eq!(expense_monthly(&draw(10, true, "monthly")), Money::from(10));
        // Every two weeks is 26 a year, not 24: that is the whole reason the
        // cadence is stored rather than assumed.
        assert_eq!(
            expense_monthly(&draw(120, true, "biweekly")),
            Money::new(dec!(260))
        );
    }

    #[test]
    fn a_one_off_draw_has_no_monthly_figure() {
        // Averaging a one-off over the year would inflate every month.
        assert_eq!(expense_monthly(&draw(1_200, false, "")), Money::ZERO);
    }

    #[test]
    fn the_draws_under_a_line_add_up() {
        let item = BudgetItem {
            name: "Insurance".into(),
            expenses: vec![draw(1_590, true, "every6months"), draw(120, true, "yearly")],
            ..Default::default()
        };
        assert_eq!(planned_monthly(&item), Money::from(275));
    }

    #[test]
    fn a_gain_is_what_it_is_worth_less_what_it_cost() {
        let mut held = holding("", 10, 120);
        held.cost_basis = Some(Money::from(1_000));
        assert_eq!(holding_gain(&held), Some(Money::from(200)));
    }

    #[test]
    fn an_unrecorded_cost_shows_no_gain_rather_than_all_of_it() {
        // A basis of zero is what an unrecorded one looks like; treating it as
        // real turns the whole value into a gain.
        let mut held = holding("", 10, 120);
        held.cost_basis = Some(Money::ZERO);
        assert_eq!(holding_gain(&held), None);

        held.cost_basis = None;
        assert_eq!(holding_gain(&held), None);
    }

    #[test]
    fn a_holding_bought_at_its_current_price_does_not_read_as_minus_zero() {
        // Under a cent is rounding in a four-decimal price, not a loss.
        let mut held = holding("", 3, 100);
        held.cost_basis = Some(Money::new(dec!(300.004)));
        assert_eq!(holding_gain(&held), Some(Money::ZERO));
    }

    #[test]
    fn holdings_add_up_to_a_value_a_basis_and_a_gain() {
        let mut a = holding("", 10, 120);
        a.cost_basis = Some(Money::from(1_000));
        let mut b = holding("", 5, 40);
        b.cost_basis = Some(Money::from(300));

        let total = holdings_total(&[a, b]);
        assert_eq!(total.value, Money::from(1_400));
        assert_eq!(total.basis, Money::from(1_300));
        assert_eq!(total.gain, Money::from(100));
        assert_eq!(total.count, 2);
    }

    #[test]
    fn a_retirement_account_counts_cash_holdings_and_what_the_budget_sends() {
        let mut ledger = Ledger::default();
        let mut acct = account("retirement-roth", 500);
        acct.contributions_amount = Some(Money::from(7_000));
        acct.retirement = Some(ledger_domain::records::Retirement {
            monthly_contribution: Some(Money::from(200)),
            ..Default::default()
        });
        let id = acct.id.clone();
        ledger.accounts.push(acct);
        ledger.investments.push(holding(&id, 10, 50));
        ledger.budget.push(BudgetItem {
            name: "Roth".into(),
            account_id: id.clone(),
            monthly_amount: Money::from(300),
            ..Default::default()
        });

        let standing = retirement_standing(&ledger, &id);
        assert_eq!(standing.value, Money::from(1_000));
        assert_eq!(standing.monthly, Money::from(200));
        assert_eq!(standing.from_budget, Money::from(300));
        assert_eq!(standing.contributions, Money::from(7_000));
    }

    #[test]
    fn open_reconciliations_are_counted_apart_from_settled_ones() {
        let mut ledger = Ledger::default();
        ledger
            .reconciliations
            .push(ledger_domain::records::Reconciliation {
                card: "Visa".into(),
                balance: Money::from(250),
                status: "open".into(),
                ..Default::default()
            });
        ledger
            .reconciliations
            .push(ledger_domain::records::Reconciliation {
                card: "Amex".into(),
                balance: Money::from(900),
                status: "settled".into(),
                ..Default::default()
            });

        let rollup = recon_rollup(&ledger);
        assert_eq!(rollup.open_count, 1);
        assert_eq!(rollup.settled_count, 1);
        // Only what is still owed, not what has already been paid.
        assert_eq!(rollup.open_balance, Money::from(250));
    }

    #[test]
    fn monthly_totals_add_up() {
        let mut ledger = Ledger::default();
        ledger.income.push(IncomeStream {
            monthly_total: Money::new(dec!(4_250.50)),
            ..Default::default()
        });
        ledger.income.push(IncomeStream {
            monthly_total: Money::new(dec!(1_000.25)),
            ..Default::default()
        });
        ledger.budget.push(BudgetItem {
            monthly_amount: Money::new(dec!(2_000.10)),
            ..Default::default()
        });
        ledger.buckets.push(Bucket {
            current_total: Money::new(dec!(999.99)),
            ..Default::default()
        });

        assert_eq!(monthly_income(&ledger), Money::new(dec!(5_250.75)));
        assert_eq!(monthly_budget(&ledger), Money::new(dec!(2_000.10)));
        assert_eq!(bucket_cash(&ledger), Money::new(dec!(999.99)));
    }

    #[test]
    fn a_balance_compounds_monthly_and_takes_its_contribution_at_month_end() {
        let points = project_balance(Money::from(1_000), Money::ZERO, dec!(12), Decimal::ONE);

        assert_eq!(points.len(), 2);
        assert_eq!(points[0].month, 0);
        assert_eq!(points[1].month, 12);
        assert_eq!(points[1].value, Money::new(dec!(1_126.83)));
        assert_eq!(points[1].contributed, Money::ZERO);
    }

    #[test]
    fn the_projection_matches_the_plugin_to_the_dollar() {
        let cases = [
            (
                dec!(233654),
                dec!(1039),
                dec!(19.25),
                [
                    dec!(864629.92),
                    dec!(1189358.64),
                    dec!(1651764.87),
                    dec!(2312236.92),
                ],
            ),
            (
                dec!(0),
                dec!(500),
                dec!(10),
                [
                    dec!(73624.90),
                    dec!(81939.67),
                    dec!(91473.02),
                    dec!(102422.49),
                ],
            ),
            (
                dec!(100000),
                dec!(0),
                dec!(30.5),
                [
                    dec!(338032.27),
                    dec!(620552.60),
                    dec!(1138051.45),
                    dec!(2085016.43),
                ],
            ),
            (
                dec!(85454.37),
                dec!(583.12),
                dec!(39.75),
                [
                    dec!(1098581.20),
                    dec!(2064755.80),
                    dec!(4027102.12),
                    dec!(8071417.98),
                ],
            ),
        ];
        for (start, monthly, years, expected) in cases {
            for (rate, want) in PROJECTION_RATES.iter().zip(expected) {
                let points = project_balance(
                    Money::new(start),
                    Money::new(monthly),
                    Decimal::from(*rate),
                    years,
                );
                let got = points.last().unwrap().value.inner();
                assert_eq!(
                    got.round(),
                    want.round(),
                    "{start} {monthly} {years} @{rate}%"
                );
                assert!((got - want).abs() < dec!(0.05), "{got} vs {want}");
            }
        }
    }

    #[test]
    fn every_month_is_on_the_same_curve_as_the_yearly_points() {
        let every = project_monthly(Money::from(5_000), Money::from(250), dec!(8), dec!(2.5));
        let yearly = project_balance(Money::from(5_000), Money::from(250), dec!(8), dec!(2.5));
        assert_eq!(every.len(), 31);
        for point in yearly {
            assert_eq!(every[point.month as usize], point);
        }
    }

    #[test]
    fn a_horizon_that_is_not_whole_years_keeps_its_own_last_point() {
        let points = project_balance(Money::from(5_000), Money::from(250), dec!(8), dec!(2.5));

        let months: Vec<u32> = points.iter().map(|p| p.month).collect();
        assert_eq!(months, vec![0, 12, 24, 30]);
        assert_eq!(points[3].value, Money::new(dec!(14_375.18)));
        assert_eq!(points[3].contributed, Money::from(7_500));
    }

    #[test]
    fn growth_is_what_the_rate_added_rather_than_what_was_paid_in() {
        let mut ledger = Ledger::default();
        ledger.accounts.push(Account {
            id: "r1".into(),
            kind: "retirement-roth".into(),
            total: Some(Money::from(10_000)),
            retirement: Some(ledger_domain::records::Retirement {
                monthly_contribution: Some(Money::from(100)),
                ..Default::default()
            }),
            ..Default::default()
        });

        let projection = retirement_projection(&ledger, Decimal::from(10), &[dec!(6)]);
        let line = &projection.lines[0];

        assert_eq!(projection.start, Money::from(10_000));
        assert_eq!(projection.monthly, Money::from(100));
        assert_eq!(line.value, Money::new(dec!(34_581.90)));
        assert_eq!(line.contributed, Money::from(12_000));
        assert_eq!(line.growth, Money::new(dec!(12_581.90)));
    }

    #[test]
    fn a_target_year_already_gone_projects_nothing_rather_than_backwards() {
        assert_eq!(years_until(2020, 2026, 8), Decimal::ZERO);
        assert_eq!(years_until(2056, 2026, 0), Decimal::from(30));
        assert_eq!(years_until(2027, 2026, 9), dec!(0.25));
        assert_eq!(
            years_until(9999, 2026, 0),
            Decimal::from(MAX_PROJECTION_YEARS)
        );
    }

    #[test]
    fn a_budget_line_reaches_retirement_through_a_linked_bucket() {
        let mut ledger = Ledger::default();
        ledger.accounts.push(Account {
            id: "r1".into(),
            name: "Empower 401k".into(),
            kind: "retirement-roth".into(),
            ..Default::default()
        });
        ledger.buckets.push(Bucket {
            id: "b1".into(),
            name: "Retirement".into(),
            linked_account_id: "r1".into(),
            ..Default::default()
        });
        ledger.budget.push(BudgetItem {
            name: "Through the bucket".into(),
            bucket_id: "b1".into(),
            monthly_amount: Money::from(300),
            ..Default::default()
        });
        ledger.budget.push(BudgetItem {
            name: "Straight at it".into(),
            account_id: "r1".into(),
            monthly_amount: Money::from(200),
            ..Default::default()
        });
        ledger.budget.push(BudgetItem {
            name: "Nothing to do with it".into(),
            monthly_amount: Money::from(900),
            ..Default::default()
        });

        assert_eq!(
            retirement_standing(&ledger, "r1").from_budget,
            Money::from(500)
        );
    }
}

// ================================================ earners and paychecks

use rust_decimal::prelude::*;

/// How many times a year each cadence pays.
pub fn periods_per_year(frequency: &str) -> u32 {
    match frequency {
        "weekly" => 52,
        "semimonthly" => 24,
        "monthly" => 12,
        "quarterly" => 4,
        "annual" => 1,
        // Every two weeks, and the fallback: it is the most common cadence
        // and the one the record defaults to.
        _ => 26,
    }
}

/// One person who brings money in, and what share of the total they bring.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Earner {
    pub owner: String,
    pub monthly: Money,
    /// How many income streams this person has.
    pub count: usize,
    /// Share of all monthly income, 0..100.
    pub percent: Decimal,
    /// What turns a monthly share into a per-paycheck figure. Derived from
    /// the real cadence rather than assuming two paydays a month.
    pub paychecks_per_month: Decimal,
}

/// Who earns what.
///
/// Earners come from whatever owners the income streams carry, so a household
/// with one earner or three needs no change. A stream with no owner is filed
/// under "Unassigned" rather than dropped, because its money is still real.
pub fn owner_shares(ledger: &Ledger) -> Vec<Earner> {
    let mut order: Vec<String> = Vec::new();
    let mut monthly: Vec<Money> = Vec::new();
    let mut periods: Vec<u32> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();

    for stream in &ledger.income {
        let owner = if stream.owner.trim().is_empty() {
            "Unassigned".to_string()
        } else {
            stream.owner.clone()
        };
        let at = match order.iter().position(|o| o == &owner) {
            Some(index) => index,
            None => {
                order.push(owner);
                monthly.push(Money::ZERO);
                periods.push(0);
                counts.push(0);
                order.len() - 1
            }
        };
        monthly[at] += stream.monthly_total;
        periods[at] += periods_per_year(&stream.frequency);
        counts[at] += 1;
    }

    let total: Money = monthly.iter().copied().sum();
    let mut out: Vec<Earner> = order
        .into_iter()
        .enumerate()
        .map(|(i, owner)| Earner {
            owner,
            monthly: monthly[i],
            count: counts[i],
            percent: if total.is_zero() {
                Decimal::ZERO
            } else {
                monthly[i].inner() / total.inner() * Decimal::from(100)
            },
            paychecks_per_month: Decimal::from(periods[i]) / Decimal::from(12),
        })
        .collect();

    // Largest earner first, and by name where two are equal so the order does
    // not wander between runs.
    out.sort_by(|a, b| {
        b.monthly
            .cmp(&a.monthly)
            .then_with(|| a.owner.cmp(&b.owner))
    });
    out
}

/// One earner's part of an amount.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Share {
    pub owner: String,
    pub amount: Money,
}

/// Divide an amount between earners in proportion to what each brings in.
///
/// **The shares always add back up to the amount.** The prototype computed
/// each share as `amount * percent / 100` and rounded them independently,
/// which loses or invents a cent: three equal earners splitting £10 get
/// £3.33 each and sixpence goes missing from the household's books.
///
/// So the remainder is handed out rather than rounded away — a cent each to
/// whoever was cut by most, which is the largest-remainder method.
pub fn split(amount: Money, earners: &[Earner]) -> Vec<Share> {
    if earners.is_empty() {
        return Vec::new();
    }
    let total: Money = earners.iter().map(|e| e.monthly).sum();
    if total.is_zero() || amount.is_zero() {
        return earners
            .iter()
            .map(|e| Share {
                owner: e.owner.clone(),
                amount: Money::ZERO,
            })
            .collect();
    }

    // Worked in whole cents on the absolute value, so the arithmetic is
    // integer and the sign is put back at the end.
    let negative = amount.is_negative();
    let hundred = Decimal::from(100);
    let cents = (amount.abs().inner() * hundred)
        .round()
        .to_i128()
        .unwrap_or(0);

    let mut exact: Vec<Decimal> = Vec::with_capacity(earners.len());
    for earner in earners {
        exact.push(Decimal::from(cents) * earner.monthly.inner() / total.inner());
    }

    let mut whole: Vec<i128> = exact
        .iter()
        .map(|e| e.floor().to_i128().unwrap_or(0))
        .collect();
    let mut left = cents - whole.iter().sum::<i128>();

    // Biggest shortfall first, ties by position so the result is the same
    // every time it is worked out.
    let mut by_remainder: Vec<usize> = (0..earners.len()).collect();
    by_remainder.sort_by(|&a, &b| {
        let ra = exact[a] - exact[a].floor();
        let rb = exact[b] - exact[b].floor();
        rb.partial_cmp(&ra).unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut at = 0;
    while left > 0 && !by_remainder.is_empty() {
        whole[by_remainder[at % by_remainder.len()]] += 1;
        left -= 1;
        at += 1;
    }

    earners
        .iter()
        .zip(whole)
        .map(|(earner, c)| {
            let value = Decimal::from(c) / hundred;
            Share {
                owner: earner.owner.clone(),
                amount: Money::new(if negative { -value } else { value }),
            }
        })
        .collect()
}

/// What one earner puts in per paycheck towards a monthly amount.
pub fn per_paycheck(monthly_share: Money, earner: &Earner) -> Money {
    if earner.paychecks_per_month.is_zero() {
        return Money::ZERO;
    }
    Money::new(monthly_share.inner() / earner.paychecks_per_month)
}

// ==================================================== savings buckets

/// What a bucket is worth, broken into where the money actually is.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BucketWorth {
    /// Money sitting in the bucket.
    pub cash: Money,
    /// Holdings assigned to this bucket, at what they are worth now.
    pub invested: Money,
    /// Roth contributions counted against this bucket.
    ///
    /// Only what was *put in* to a Roth comes out again without penalty, so a
    /// Roth is allocated by its contribution basis rather than by the value of
    /// what it holds.
    pub contributions: Money,
    pub total: Money,
}

pub fn bucket_worth(ledger: &Ledger, bucket_id: &str) -> BucketWorth {
    let cash = ledger
        .bucket(bucket_id)
        .map(|b| b.current_total)
        .unwrap_or(Money::ZERO);

    let invested: Decimal = ledger
        .investments
        .iter()
        .filter(|h| h.bucket_id == bucket_id)
        .map(holding_value)
        .sum();

    let contributions: Money = ledger
        .accounts
        .iter()
        .filter(|a| a.kind == "retirement-roth")
        .flat_map(|a| &a.contribution_allocations)
        .filter(|allocation| allocation.bucket_id == bucket_id)
        .map(|allocation| allocation.amount)
        .sum();

    let invested = Money::new(invested);
    BucketWorth {
        cash,
        invested,
        contributions,
        total: cash + invested + contributions,
    }
}

/// Every budget line feeding a bucket, and what they come to each month.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BucketFunding {
    pub monthly: Money,
    pub names: Vec<String>,
}

pub fn bucket_funding(ledger: &Ledger, bucket_id: &str) -> BucketFunding {
    let mut funding = BucketFunding::default();
    for item in &ledger.budget {
        if item.bucket_id == bucket_id {
            funding.monthly += item.monthly_amount;
            funding.names.push(item.name.clone());
        }
    }
    funding
}

// ================================================== planned draws

/// How many times a year a planned draw comes round.
pub fn intervals_per_year(interval: &str) -> u32 {
    match interval {
        "weekly" => 52,
        "biweekly" => 26,
        "quarterly" => 4,
        "every6months" => 2,
        "yearly" => 1,
        _ => 12,
    }
}

/// What a planned draw costs in a month.
///
/// Only a repeating one has a monthly figure: a one-off is a date, not a rate,
/// and averaging it over the year would quietly inflate every month's budget.
pub fn expense_monthly(expense: &ledger_domain::records::PlannedExpense) -> Money {
    if !expense.recurring {
        return Money::ZERO;
    }
    let per_year = Decimal::from(intervals_per_year(&expense.interval));
    Money::new(expense.amount.inner() * per_year / Decimal::from(12))
}

/// What all the draws under one budget line come to in a month.
pub fn planned_monthly(item: &ledger_domain::records::BudgetItem) -> Money {
    item.expenses.iter().map(expense_monthly).sum()
}

// ===================================================== holdings

/// What one holding is worth now.
pub fn holding_worth(holding: &Holding) -> Money {
    Money::new(holding_value(holding))
}

/// Gain or loss against what it cost, or `None` when the cost is unknown.
///
/// A basis of zero is what an unrecorded one looks like once it has been
/// through the app, and treating that as a real cost turns the whole value
/// into a gain — which is worse than saying nothing.
pub fn holding_gain(holding: &Holding) -> Option<Money> {
    let basis = holding.cost_basis?;
    if basis.is_zero() {
        return None;
    }
    let gain = Money::new(holding_value(holding)) - basis;
    // Under a cent is rounding in a four-decimal price, not a loss. Without
    // this a holding bought at exactly its current price reads as "−$0".
    if gain.abs() < Money::new(rust_decimal::Decimal::new(1, 2)) {
        return Some(Money::ZERO);
    }
    Some(gain)
}

/// What every holding is worth, and what they cost.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HoldingsTotal {
    pub value: Money,
    pub basis: Money,
    pub gain: Money,
    pub count: usize,
}

pub fn holdings_total(holdings: &[Holding]) -> HoldingsTotal {
    let value: Decimal = holdings.iter().map(holding_value).sum();
    let basis: Money = holdings.iter().filter_map(|h| h.cost_basis).sum();
    let value = Money::new(value);
    HoldingsTotal {
        value,
        basis,
        gain: value - basis,
        count: holdings.len(),
    }
}

// =================================================== retirement

/// What a retirement account holds and puts away.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RetirementStanding {
    /// Cash typed on the account plus what its holdings are worth.
    pub value: Money,
    /// Typed on the account, separate from anything the budget sends.
    pub monthly: Money,
    /// What budget lines aimed at this account add each month.
    pub from_budget: Money,
    pub contributions: Money,
}

/// Named directly, through a bucket linked to the account, or named for the
/// account inside a "Retirement" bucket. All three from the prototype.
fn retirement_budget_lines<'a>(
    ledger: &'a Ledger,
    account: &'a Account,
) -> impl Iterator<Item = &'a ledger_domain::records::BudgetItem> {
    let account_name = account.name.to_lowercase();
    ledger.budget.iter().filter(move |item| {
        if item.account_id == account.id {
            return true;
        }
        let Some(bucket) = ledger.bucket(&item.bucket_id) else {
            return false;
        };
        bucket.linked_account_id == account.id
            || (bucket.name.trim().eq_ignore_ascii_case("retirement")
                && account_name.starts_with(&item.name.to_lowercase()))
    })
}

pub fn retirement_standing(ledger: &Ledger, account_id: &str) -> RetirementStanding {
    let Some(account) = ledger.account(account_id) else {
        return RetirementStanding::default();
    };

    let held: Decimal = ledger
        .investments
        .iter()
        .filter(|h| h.account_id == account_id)
        .map(holding_value)
        .sum();

    let from_budget: Money = retirement_budget_lines(ledger, account)
        .map(|b| b.monthly_amount)
        .sum();

    RetirementStanding {
        value: Money::new(account.total.unwrap_or(Money::ZERO).inner() + held),
        monthly: account
            .retirement
            .as_ref()
            .and_then(|r| r.monthly_contribution)
            .unwrap_or(Money::ZERO),
        from_budget,
        contributions: account.contributions_amount.unwrap_or(Money::ZERO),
    }
}

/// What every retirement account together holds and receives each month.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RetirementRollup {
    pub total: Money,
    pub monthly: Money,
    pub count: usize,
}

pub fn retirement_rollup(ledger: &Ledger) -> RetirementRollup {
    let mut out = RetirementRollup::default();
    for account in ledger.accounts.iter().filter(|a| a.is_retirement()) {
        let standing = retirement_standing(ledger, &account.id);
        out.total += standing.value;
        out.monthly += standing.monthly + standing.from_budget;
        out.count += 1;
    }
    out
}

// ==================================================== projection

/// Past this a projection is answering a question nobody asked, and a mistyped
/// target year cannot run the compounding away with it.
pub const MAX_PROJECTION_YEARS: i64 = 70;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProjectionPoint {
    /// Months from today, not years: the last point of an odd horizon lands
    /// between two years and rounding it would move the figure.
    pub month: u32,
    pub value: Money,
    pub contributed: Money,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionLine {
    pub rate: Decimal,
    pub points: Vec<ProjectionPoint>,
    pub value: Money,
    pub contributed: Money,
    pub growth: Money,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Projection {
    pub years: Decimal,
    pub start: Money,
    pub monthly: Money,
    pub lines: Vec<ProjectionLine>,
}

/// Fractional, so a target three months out does not read as a whole year.
pub fn years_until(target_year: i32, this_year: i32, months_elapsed: u32) -> Decimal {
    let whole = Decimal::from(target_year) - Decimal::from(this_year);
    let part = Decimal::from(months_elapsed) / Decimal::from(12);
    (whole - part).clamp(Decimal::ZERO, Decimal::from(MAX_PROJECTION_YEARS))
}

fn months_in(years: Decimal) -> u32 {
    let months = (years * Decimal::from(12))
        .round()
        .clamp(Decimal::ZERO, Decimal::from(MAX_PROJECTION_YEARS * 12));
    u32::try_from(months).unwrap_or(0)
}

pub const PROJECTION_RATES: [u32; 4] = [4, 6, 8, 10];

/// Compound one balance forward, adding the monthly contribution at the end of
/// each month. A point per year, plus the horizon itself when it is not one.
pub fn project_balance(
    start: Money,
    monthly: Money,
    annual_rate: Decimal,
    years: Decimal,
) -> Vec<ProjectionPoint> {
    let every = project_monthly(start, monthly, annual_rate, years);
    let last = every.last().map(|p| p.month).unwrap_or(0);
    every
        .into_iter()
        .filter(|p| p.month.is_multiple_of(12) || p.month == last)
        .collect()
}

pub fn project_monthly(
    start: Money,
    monthly: Money,
    annual_rate: Decimal,
    years: Decimal,
) -> Vec<ProjectionPoint> {
    let months = months_in(years);
    let step = Decimal::ONE + annual_rate / Decimal::from(1200);
    let mut balance = start.inner();
    let mut contributed = Decimal::ZERO;
    let mut points = vec![ProjectionPoint {
        month: 0,
        value: start,
        contributed: Money::ZERO,
    }];

    for month in 1..=months {
        // Held at eight places rather than at cents: rounding a running balance
        // to the penny every month compounds the rounding too.
        balance = (balance * step + monthly.inner()).round_dp(8);
        contributed += monthly.inner();
        points.push(ProjectionPoint {
            month,
            value: Money::new(balance),
            contributed: Money::new(contributed),
        });
    }
    points
}

/// The retirement picture projected to a horizon, once per rate asked for.
pub fn retirement_projection(ledger: &Ledger, years: Decimal, rates: &[Decimal]) -> Projection {
    let rollup = retirement_rollup(ledger);
    let lines = rates
        .iter()
        .map(|&rate| {
            let points = project_balance(rollup.total, rollup.monthly, rate, years);
            let last = points.last().copied().unwrap_or(ProjectionPoint {
                month: 0,
                value: rollup.total,
                contributed: Money::ZERO,
            });
            ProjectionLine {
                rate,
                value: last.value,
                contributed: last.contributed,
                growth: last.value - rollup.total - last.contributed,
                points,
            }
        })
        .collect();

    Projection {
        years,
        start: rollup.total,
        monthly: rollup.monthly,
        lines,
    }
}

// =============================================== reconciliation

/// Where a set of reconciliations stands.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ReconRollup {
    pub open_balance: Money,
    pub open_count: usize,
    pub settled_count: usize,
}

pub fn recon_rollup(ledger: &Ledger) -> ReconRollup {
    let mut out = ReconRollup::default();
    for record in &ledger.reconciliations {
        if record.status == "settled" {
            out.settled_count += 1;
        } else {
            out.open_count += 1;
            out.open_balance += record.balance;
        }
    }
    out
}

/// What the lines on one reconciliation come to.
pub fn recon_lines_total(record: &ledger_domain::records::Reconciliation) -> Money {
    record.lines.iter().map(|l| l.amount).sum()
}
