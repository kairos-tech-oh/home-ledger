//! What the screens are given.
//!
//! Deliberately not the stored records. Every amount crosses as a string, so
//! nothing reaches a JavaScript number on its way to being displayed, and
//! every derived figure is worked out here rather than in the interface.

use crate::commands::Answer;
use crate::state::AppState;
use ledger_domain::{Ledger, Money};
use rust_decimal::Decimal;
use serde::Serialize;
use tauri::State;

fn amount(value: Money) -> String {
    value.to_string()
}

fn maybe(value: Option<Money>) -> Option<String> {
    value.map(amount)
}

/// Every place kept, for share counts and prices, which carry four. A form
/// filled from a two-place figure would save the rounded one back.
fn exact(value: Money) -> String {
    value.inner().normalize().to_string()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub institution: String,
    pub notes: String,
    /// The cash figure typed on the account. Holdings are priced separately.
    pub total: Option<String>,
    pub available_credit: Option<String>,
    /// Whether this is money owed rather than money held.
    pub liability: bool,
    /// What the holdings assigned to this account are worth now.
    pub holdings: String,
    pub holding_count: usize,
    /// Debt lines hanging off the account.
    pub debts: String,
    pub debt_count: usize,
    /// Cash plus holdings, less what is owed.
    pub net: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BucketView {
    pub id: String,
    pub name: String,
    pub notes: String,
    pub cash: String,
    pub invested: String,
    pub contributions: String,
    pub total: String,
    pub target: Option<String>,
    /// How far along, 0..100, or None when nothing is being aimed at.
    pub progress: Option<f64>,
    pub locked: bool,
    /// What the budget puts in each month, and which lines do it.
    pub funded_monthly: String,
    pub funded_by: Vec<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomeView {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub monthly_total: String,
    pub frequency: String,
    pub account_id: String,
    /// Resolved here so the screen shows a name rather than an id.
    pub account_name: String,
    pub notes: String,
    /// The monthly total divided by how often this stream actually pays.
    pub per_paycheck: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SplitView {
    pub owner: String,
    pub amount: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub monthly_amount: String,
    pub account_id: String,
    pub account_name: String,
    pub bucket_id: String,
    pub bucket_name: String,
    pub notes: String,
    /// Planned draws sitting under this line.
    pub draws: usize,
    /// What those draws come to each month, so a premium every six months is
    /// comparable to the line it sits under.
    pub draws_monthly: String,
    /// Each earner's share, in proportion to what they bring in. These always
    /// add back up to the line's amount.
    pub split: Vec<SplitView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EarnerView {
    pub owner: String,
    pub monthly: String,
    pub percent: f64,
    pub paychecks_per_month: f64,
    pub streams: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HoldingView {
    pub id: String,
    pub name: String,
    pub ticker: String,
    pub kind: String,
    pub quantity: String,
    pub cost_basis: Option<String>,
    pub avg_cost: String,
    pub price: Option<String>,
    pub price_at: String,
    pub price_stale: bool,
    pub fixed_price: bool,
    pub value: String,
    /// None when the cost is unknown, rather than pretending it is all gain.
    pub gain: Option<String>,
    pub gain_percent: Option<f64>,
    pub account_id: String,
    pub account_name: String,
    pub bucket_id: String,
    pub bucket_name: String,
    pub purchase_date: String,
    pub asset_class: String,
    pub notes: String,
    pub trades: Vec<TradeView>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TradeView {
    pub kind: String,
    pub quantity: String,
    pub price: String,
    pub total: String,
    pub at: String,
    pub notes: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SleeveView {
    pub id: String,
    pub name: String,
    pub percent: String,
    pub asset_class: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RetirementView {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub institution: String,
    pub value: String,
    /// Typed on the account, separate from anything the budget sends.
    pub monthly: String,
    pub from_budget: String,
    pub contributions: String,
    /// The schedule's own figure, as typed, for the editor.
    pub contribution: Option<String>,
    pub auto_contribute: bool,
    /// `yyyy-mm` of the last month topped up, empty when that is off.
    pub accrued_through: String,
    pub sleeves: Vec<SleeveView>,
    pub holdings: usize,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconLineView {
    pub id: String,
    pub label: String,
    pub member: String,
    pub spent_on: String,
    pub amount: String,
    pub bucket_id: String,
    pub bucket_name: String,
    pub notes: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReconciliationView {
    pub id: String,
    pub card: String,
    pub card_account_id: String,
    pub card_account_name: String,
    pub bucket_source_id: String,
    pub spend_source_id: String,
    pub adjust_accounts: bool,
    pub statement_date: String,
    pub balance: String,
    pub status: String,
    pub settled_at: String,
    pub notes: String,
    pub lines: Vec<ReconLineView>,
    /// What the lines come to, which is what makes a balance reconcile or not.
    pub lines_total: String,
    /// Balance less the lines. Zero means it is ready to settle.
    pub unaccounted: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalView {
    pub id: String,
    pub name: String,
    pub notes: String,
    pub bucket_id: String,
    /// Empty when the goal is linked to nothing, or to a bucket since deleted.
    pub bucket_name: String,
    pub target: Option<String>,
    /// What the bucket behind it holds.
    pub saved: String,
    /// 0..=100, or None when there is no target.
    pub progress: Option<f64>,
    /// Target less saved, never below zero. None without a target.
    pub remaining: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GoalTotalsView {
    pub saved: String,
    pub target: String,
    pub with_target: usize,
    pub percent: Option<f64>,
    /// Still to find across the goals with a target.
    pub remaining: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TemplateView {
    pub id: String,
    pub name: String,
    pub notes: String,
    pub saved_at: String,
    pub lines: usize,
    /// What its lines come to in a month.
    pub monthly: String,
    /// Its monthly total less the live budget's: what putting it back would
    /// change by.
    pub against_now: String,
}

#[derive(Serialize)]
pub struct FixedTypes {
    pub budget: &'static [&'static str],
    pub investment: &'static [&'static str],
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LedgerView {
    pub accounts: Vec<AccountView>,
    pub buckets: Vec<BucketView>,
    pub income: Vec<IncomeView>,
    pub budget: Vec<BudgetView>,
    pub holdings: Vec<HoldingView>,
    pub retirement: Vec<RetirementView>,
    pub reconciliations: Vec<ReconciliationView>,
    pub earners: Vec<EarnerView>,
    /// Nearest to done first, goals without a target last.
    pub goals: Vec<GoalView>,
    pub goal_totals: GoalTotalsView,
    /// Family names to offer on a charge: this machine's list, income owners
    /// and names already used.
    pub members: Vec<String>,
    /// Saved budgets, as stored: oldest first.
    pub templates: Vec<TemplateView>,
    /// Every type the budget uses, in the order they are filed under.
    pub budget_types: Vec<String>,
    pub investment_types: Vec<String>,
    /// The built-in types, which cannot be removed.
    pub fixed_types: FixedTypes,
    /// True when this came from a backup because the source of truth could
    /// not be reached, so the screens can say it may be behind.
    pub stale: bool,
    pub loaded_from: String,
}

/// Everything the account and savings screens need, in one read.
#[tauri::command]
pub async fn ledger(state: State<'_, AppState>) -> Answer<LedgerView> {
    let loaded = state.live().await.engine.load().await?;
    let doc = match &loaded.snapshot {
        Some(s) => ledger_writer::read(&s.body)?,
        None => Ledger::default(),
    };

    Ok(LedgerView {
        accounts: accounts_of(&doc),
        buckets: buckets_of(&doc),
        income: income_of(&doc),
        budget: budget_of(&doc),
        holdings: holdings_of(&doc),
        retirement: retirement_of(&doc),
        reconciliations: reconciliations_of(&doc),
        earners: earners_of(&doc),
        goals: goals_of(&doc),
        goal_totals: goal_totals_of(&doc),
        templates: templates_of(&doc),
        members: ledger_math::spending::members(&doc, &state.family_members().await),
        budget_types: doc.budget_types.clone(),
        investment_types: doc.investment_types.clone(),
        fixed_types: FixedTypes {
            budget: ledger_domain::records::DEFAULT_BUDGET_TYPES,
            investment: ledger_domain::records::DEFAULT_INVESTMENT_TYPES,
        },
        stale: loaded.stale,
        loaded_from: loaded.from.to_string(),
    })
}

fn accounts_of(doc: &Ledger) -> Vec<AccountView> {
    doc.accounts
        .iter()
        .map(|account| {
            let held: Vec<_> = doc
                .investments
                .iter()
                .filter(|h| h.account_id == account.id)
                .collect();
            let holdings: Money = held
                .iter()
                .map(|h| {
                    let price = h.price.unwrap_or(h.avg_cost);
                    Money::new(price.inner() * h.quantity.inner())
                })
                .sum();
            let debts: Money = account.debts.iter().map(|d| d.balance.abs()).sum();
            let liability = account.is_liability();

            // Mirrors the balance sheet: a liability's typed total is what is
            // owed and replaces its debt lines rather than adding to them.
            let net = if liability {
                let owed = match account.total {
                    Some(total) => total.abs(),
                    None => debts,
                };
                Money::ZERO - owed
            } else {
                account.total.unwrap_or(Money::ZERO) + holdings - debts
            };

            AccountView {
                id: account.id.clone(),
                name: account.name.clone(),
                kind: account.kind.clone(),
                institution: account.institution.clone(),
                notes: account.notes.clone(),
                total: maybe(account.total),
                available_credit: maybe(account.available_credit),
                liability,
                holdings: amount(holdings),
                holding_count: held.len(),
                debts: amount(debts),
                debt_count: account.debts.len(),
                net: amount(net),
            }
        })
        .collect()
}

fn buckets_of(doc: &Ledger) -> Vec<BucketView> {
    doc.buckets
        .iter()
        .map(|bucket| {
            let worth = ledger_math::bucket_worth(doc, &bucket.id);
            let funding = ledger_math::bucket_funding(doc, &bucket.id);

            // Only where something is actually being aimed at, and never
            // dividing by a target of zero.
            let progress = bucket
                .target_amount
                .filter(|t| !t.is_zero())
                .map(|t| (worth.total.to_f64() / t.to_f64() * 100.0).clamp(0.0, 999.0));

            BucketView {
                id: bucket.id.clone(),
                name: bucket.name.clone(),
                notes: bucket.notes.clone(),
                cash: amount(worth.cash),
                invested: amount(worth.invested),
                contributions: amount(worth.contributions),
                total: amount(worth.total),
                target: maybe(bucket.target_amount),
                progress,
                locked: bucket.locked,
                funded_monthly: amount(funding.monthly),
                funded_by: funding.names,
            }
        })
        .collect()
}

fn income_of(doc: &Ledger) -> Vec<IncomeView> {
    doc.income
        .iter()
        .map(|stream| {
            // Per paycheck for this stream alone, from its own cadence.
            let per_year = ledger_math::periods_per_year(&stream.frequency);
            let per_paycheck = Money::new(
                stream.monthly_total.inner() * rust_decimal::Decimal::from(12)
                    / rust_decimal::Decimal::from(per_year.max(1)),
            );
            IncomeView {
                id: stream.id.clone(),
                name: stream.name.clone(),
                owner: stream.owner.clone(),
                monthly_total: amount(stream.monthly_total),
                frequency: stream.frequency.clone(),
                account_id: stream.account_id.clone(),
                account_name: name_of_account(doc, &stream.account_id),
                notes: stream.notes.clone(),
                per_paycheck: amount(per_paycheck),
            }
        })
        .collect()
}

fn budget_of(doc: &Ledger) -> Vec<BudgetView> {
    let earners = ledger_math::owner_shares(doc);

    doc.budget
        .iter()
        .map(|item| BudgetView {
            id: item.id.clone(),
            name: item.name.clone(),
            kind: item.kind.clone(),
            monthly_amount: amount(item.monthly_amount),
            account_id: item.account_id.clone(),
            account_name: name_of_account(doc, &item.account_id),
            bucket_id: item.bucket_id.clone(),
            bucket_name: doc
                .bucket(&item.bucket_id)
                .map(|b| b.name.clone())
                .unwrap_or_default(),
            notes: item.notes.clone(),
            draws: item.expenses.len(),
            draws_monthly: amount(ledger_math::planned_monthly(item)),
            split: ledger_math::split(item.monthly_amount, &earners)
                .into_iter()
                .map(|share| SplitView {
                    owner: share.owner,
                    amount: amount(share.amount),
                })
                .collect(),
        })
        .collect()
}

fn holdings_of(doc: &Ledger) -> Vec<HoldingView> {
    doc.investments
        .iter()
        .map(|holding| {
            let value = ledger_math::holding_worth(holding);
            let gain = ledger_math::holding_gain(holding);
            let gain_percent = gain.and_then(|g| {
                let basis = holding.cost_basis?;
                if basis.is_zero() {
                    return None;
                }
                Some(g.to_f64() / basis.to_f64() * 100.0)
            });

            HoldingView {
                id: holding.id.clone(),
                name: holding.name.clone(),
                ticker: holding.ticker.clone(),
                kind: holding.kind.clone(),
                quantity: exact(holding.quantity),
                cost_basis: maybe(holding.cost_basis),
                avg_cost: exact(holding.avg_cost),
                price: holding.price.map(exact),
                price_at: holding.price_at.clone(),
                price_stale: holding.price_stale,
                fixed_price: holding.fixed_price,
                value: amount(value),
                gain: maybe(gain),
                gain_percent,
                account_id: holding.account_id.clone(),
                account_name: name_of_account(doc, &holding.account_id),
                bucket_id: holding.bucket_id.clone(),
                bucket_name: doc
                    .bucket(&holding.bucket_id)
                    .map(|b| b.name.clone())
                    .unwrap_or_default(),
                purchase_date: holding.purchase_date.clone(),
                asset_class: holding.asset_class.clone(),
                notes: holding.notes.clone(),
                trades: holding
                    .trades
                    .iter()
                    .map(|trade| TradeView {
                        kind: trade.kind.clone(),
                        quantity: exact(trade.quantity),
                        price: exact(trade.price),
                        total: amount(Money::new(trade.quantity.inner() * trade.price.inner())),
                        at: trade.at.clone(),
                        notes: trade.notes.clone(),
                    })
                    .collect(),
            }
        })
        .collect()
}

fn retirement_of(doc: &Ledger) -> Vec<RetirementView> {
    doc.accounts
        .iter()
        .filter(|a| a.is_retirement())
        .map(|account| {
            let standing = ledger_math::retirement_standing(doc, &account.id);
            let block = account.retirement.as_ref();

            RetirementView {
                id: account.id.clone(),
                name: account.name.clone(),
                kind: account.kind.clone(),
                institution: account.institution.clone(),
                value: amount(standing.value),
                monthly: amount(standing.monthly),
                from_budget: amount(standing.from_budget),
                contributions: amount(standing.contributions),
                contribution: block.and_then(|b| b.monthly_contribution).map(amount),
                auto_contribute: block.is_some_and(|b| b.auto_contribute),
                accrued_through: block.map(|b| b.accrued_through.clone()).unwrap_or_default(),
                sleeves: block
                    .map(|b| {
                        b.sleeves
                            .iter()
                            .map(|s| SleeveView {
                                id: s.id.clone(),
                                name: s.name.clone(),
                                percent: amount(s.percent),
                                asset_class: s.asset_class.clone(),
                            })
                            .collect()
                    })
                    .unwrap_or_default(),
                holdings: doc
                    .investments
                    .iter()
                    .filter(|h| h.account_id == account.id)
                    .count(),
            }
        })
        .collect()
}

fn reconciliations_of(doc: &Ledger) -> Vec<ReconciliationView> {
    doc.reconciliations
        .iter()
        .map(|record| {
            let lines_total = ledger_math::recon_lines_total(record);
            ReconciliationView {
                id: record.id.clone(),
                card: record.card.clone(),
                card_account_id: record.card_account_id.clone(),
                bucket_source_id: record.bucket_source_id.clone(),
                spend_source_id: record.spend_source_id.clone(),
                adjust_accounts: record.adjust_accounts,
                card_account_name: name_of_account(doc, &record.card_account_id),
                statement_date: record.statement_date.clone(),
                balance: amount(record.balance),
                status: record.status.clone(),
                settled_at: record.settled_at.clone(),
                notes: record.notes.clone(),
                lines: record
                    .lines
                    .iter()
                    .map(|line| ReconLineView {
                        id: line.id.clone(),
                        label: line.label.clone(),
                        member: line.member.clone(),
                        spent_on: line.spent_on.clone(),
                        amount: amount(line.amount),
                        bucket_id: line.bucket_id.clone(),
                        notes: line.notes.clone(),
                        bucket_name: doc
                            .bucket(&line.bucket_id)
                            .map(|b| b.name.clone())
                            .unwrap_or_default(),
                    })
                    .collect(),
                lines_total: amount(lines_total),
                unaccounted: amount(record.balance - lines_total),
            }
        })
        .collect()
}

fn templates_of(doc: &Ledger) -> Vec<TemplateView> {
    let live = ledger_math::monthly_budget(doc);
    doc.templates
        .iter()
        .map(|template| {
            let monthly: Money = template.items.iter().map(|i| i.monthly_amount).sum();
            TemplateView {
                id: template.id.clone(),
                name: template.name.clone(),
                notes: template.notes.clone(),
                saved_at: template.saved_at.clone(),
                lines: template.items.len(),
                monthly: amount(monthly),
                against_now: amount(monthly - live),
            }
        })
        .collect()
}

fn goals_of(doc: &Ledger) -> Vec<GoalView> {
    ledger_math::goals_in_order(doc)
        .into_iter()
        .map(|goal| {
            let saved = ledger_math::goal_saved(doc, goal);
            let target = goal
                .target_amount
                .filter(|t| !t.is_zero() && !t.is_negative());
            GoalView {
                id: goal.id.clone(),
                name: goal.name.clone(),
                notes: goal.notes.clone(),
                bucket_id: goal.bucket_id.clone(),
                bucket_name: doc
                    .bucket(&goal.bucket_id)
                    .map(|b| b.name.clone())
                    .unwrap_or_default(),
                target: maybe(goal.target_amount),
                saved: amount(saved),
                progress: ledger_math::goal_progress(doc, goal),
                remaining: target.map(|t| amount((t - saved).floor_at_zero())),
            }
        })
        .collect()
}

fn goal_totals_of(doc: &Ledger) -> GoalTotalsView {
    let totals = ledger_math::goal_totals(doc);
    GoalTotalsView {
        saved: amount(totals.saved),
        target: amount(totals.target),
        with_target: totals.with_target,
        percent: totals.percent,
        remaining: amount((totals.target - totals.saved).floor_at_zero()),
    }
}

fn name_of_account(doc: &Ledger, id: &str) -> String {
    doc.account(id).map(|a| a.name.clone()).unwrap_or_default()
}

fn earners_of(doc: &Ledger) -> Vec<EarnerView> {
    use rust_decimal::prelude::ToPrimitive;
    ledger_math::owner_shares(doc)
        .into_iter()
        .map(|earner| EarnerView {
            owner: earner.owner,
            monthly: amount(earner.monthly),
            percent: earner.percent.to_f64().unwrap_or(0.0),
            paychecks_per_month: earner.paychecks_per_month.to_f64().unwrap_or(0.0),
            streams: earner.count,
        })
        .collect()
}

/// The audit log, newest first.
#[tauri::command]
pub async fn history(state: State<'_, AppState>) -> Answer<HistoryView> {
    Ok(history_of(&state).await)
}

pub async fn history_of(state: &AppState) -> HistoryView {
    let local = crate::audit::Audit::new(&state.places.data_dir)
        .read()
        .await;
    let (primary, install) = {
        let live = state.live().await;
        (live.engine.primary().clone(), live.config.install.clone())
    };
    let gathered = crate::audit::gather(primary.as_ref(), &install, &local).await;

    HistoryView {
        problems: gathered.problems,
        shared: gathered.shared,
        entries: gathered
            .entries
            .into_iter()
            .rev()
            .take(200)
            .map(|entry| HistoryEntry {
                id: entry.id,
                at: entry.at,
                action: entry.action,
                subject: entry.subject,
                name: entry.name,
                actor: entry.actor,
                amount: maybe(entry.amount),
                changes: entry
                    .changes
                    .into_iter()
                    .map(|c| format!("{}: {} → {}", c.field, c.from, c.to))
                    .collect(),
            })
            .collect(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryView {
    pub entries: Vec<HistoryEntry>,
    pub problems: Vec<String>,
    pub shared: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionPointView {
    pub month: u32,
    pub value: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionMonthView {
    pub value: String,
    pub contributed: String,
    pub growth: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionLineView {
    pub rate: String,
    pub points: Vec<ProjectionPointView>,
    pub months: Vec<ProjectionMonthView>,
    pub value: String,
    pub contributed: String,
    pub growth: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectionView {
    pub years: String,
    pub months: u32,
    pub target_year: Option<i32>,
    pub start_month: i64,
    pub start: String,
    pub monthly: String,
    pub lines: Vec<ProjectionLineView>,
}

#[tauri::command]
pub async fn projection(state: State<'_, AppState>) -> Answer<ProjectionView> {
    let loaded = state.live().await.engine.load().await?;
    let doc = match &loaded.snapshot {
        Some(s) => ledger_writer::read(&s.body)?,
        None => Ledger::default(),
    };
    let target_year = state.retirement_target_year().await;
    let (this_year, months_elapsed) = crate::clock::year_and_month();
    Ok(projection_view(
        &doc,
        target_year,
        this_year,
        months_elapsed,
    ))
}

pub fn projection_view(
    doc: &Ledger,
    target_year: Option<i32>,
    this_year: i32,
    months_elapsed: u32,
) -> ProjectionView {
    let years = match target_year {
        Some(year) => ledger_math::years_until(year, this_year, months_elapsed),
        None => Decimal::ZERO,
    };
    let rates: Vec<Decimal> = ledger_math::PROJECTION_RATES
        .iter()
        .map(|&r| Decimal::from(r))
        .collect();
    let projected = ledger_math::retirement_projection(doc, years, &rates);
    let start = projected.start;
    let start_month = target_year
        .map(|year| ((Decimal::from(year) - years) * Decimal::from(12)).round())
        .and_then(|m| i64::try_from(m).ok())
        .unwrap_or(i64::from(this_year) * 12 + i64::from(months_elapsed));

    ProjectionView {
        years: years.round_dp(2).to_string(),
        months: projected
            .lines
            .first()
            .and_then(|l| l.points.last())
            .map(|p| p.month)
            .unwrap_or(0),
        target_year,
        start_month,
        start: amount(start),
        monthly: amount(projected.monthly),
        lines: projected
            .lines
            .into_iter()
            .map(|line| ProjectionLineView {
                rate: line.rate.normalize().to_string(),
                value: amount(line.value),
                contributed: amount(line.contributed),
                growth: amount(line.growth),
                points: line
                    .points
                    .into_iter()
                    .map(|p| ProjectionPointView {
                        month: p.month,
                        value: amount(p.value),
                    })
                    .collect(),
                months: ledger_math::project_monthly(start, projected.monthly, line.rate, years)
                    .into_iter()
                    .map(|p| ProjectionMonthView {
                        value: amount(p.value),
                        contributed: amount(p.contributed),
                        growth: amount(p.value - start - p.contributed),
                    })
                    .collect(),
            })
            .collect(),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoryEntry {
    pub id: String,
    pub at: String,
    pub action: String,
    pub subject: String,
    pub name: String,
    pub actor: String,
    pub amount: Option<String>,
    pub changes: Vec<String>,
}

#[cfg(test)]
mod projection_tests {
    use super::*;

    #[test]
    fn the_projection_uses_the_plugins_rates_and_calendar() {
        let view = projection_view(&Ledger::default(), Some(2046), 2026, 8);
        let rates: Vec<&str> = view.lines.iter().map(|l| l.rate.as_str()).collect();
        assert_eq!(rates, vec!["4", "6", "8", "10"]);
        assert_eq!(view.months, 232);
        assert_eq!(view.start_month, 2026 * 12 + 8);
        assert!(view.lines.iter().all(|l| l.months.len() == 233));
    }

    #[test]
    fn no_target_year_projects_nothing() {
        let view = projection_view(&Ledger::default(), None, 2026, 8);
        assert_eq!(view.years, "0");
        assert_eq!(view.months, 0);
        assert_eq!(view.target_year, None);
    }
}
