//! Turning supplied input into a record that is safe to store.
//!
//! Bad input is cleaned, not refused — a name that is too long is cut, an
//! unknown type falls back. Only input that cannot mean anything is refused,
//! which is why every refusal here names what to fix.

use crate::WriteError;
use ledger_domain::Money;
use ledger_domain::records::*;
use ledger_domain::text::*;

fn refuse<T>(message: impl Into<String>) -> Result<T, WriteError> {
    Err(WriteError::Refused(message.into()))
}

/// Amounts arriving from outside have not been through [`Money::new`], so
/// they can carry more precision than money has. Normalise on the way in.
fn cents(amount: Money) -> Money {
    Money::new(amount.inner())
}

fn cents_opt(amount: Option<Money>) -> Option<Money> {
    amount.map(cents)
}

fn keep_or_new(id: &str) -> String {
    let existing = valid_id(id);
    if existing.is_empty() {
        new_id()
    } else {
        existing
    }
}

pub fn account(mut record: Account, keep: &str) -> Result<Account, WriteError> {
    record.name = plain(&record.name, NAME_MAX);
    if record.name.is_empty() {
        return refuse("account needs a name");
    }
    record.id = if keep.is_empty() {
        new_id()
    } else {
        keep.to_string()
    };
    record.kind = one_of(&record.kind, ACCOUNT_TYPES, "checking");
    record.institution = plain(&record.institution, NAME_MAX);
    record.notes = plain(&record.notes, NOTES_MAX);
    record.total = cents_opt(record.total);

    // Only a card or HELOC keeps a credit limit, and only a retirement
    // account keeps a schedule. Changing the type drops them, so no stale
    // figure lingers on a record it no longer describes.
    if !record.takes_credit_limit() {
        record.available_credit = None;
    } else {
        record.available_credit = cents_opt(record.available_credit);
    }
    record.loan_account_id = if record.can_secure_a_loan() {
        valid_id(&record.loan_account_id)
    } else {
        String::new()
    };
    record.retirement = match record.retirement.take() {
        Some(block) if record.is_retirement() => Some(retirement(block, &crate::this_month())?),
        _ => None,
    };

    record.debts.truncate(caps::LINES);
    record
        .debts
        .retain(|d| !plain(&d.name, NAME_MAX).is_empty());
    for debt in record.debts.iter_mut() {
        debt.id = keep_or_new(&debt.id);
        debt.name = plain(&debt.name, NAME_MAX);
        debt.kind = one_of(&debt.kind, DEBT_KINDS, "other");
        debt.balance = cents(debt.balance);
        debt.payment = cents(debt.payment);
        debt.notes = plain(&debt.notes, LINE_NOTES_MAX);
        // An APR outside 0..100 is a typo, not a rate.
        let rate = debt.rate.inner().clamp(0.into(), 100.into());
        debt.rate = Money::new(rate);
    }

    record.contributions_amount = cents_opt(record.contributions_amount);
    record.contribution_allocations.truncate(caps::ALLOCATIONS);
    // One row per bucket: two rows for the same bucket is an edit that went
    // wrong, not two separate allocations.
    let mut seen: Vec<String> = Vec::new();
    record.contribution_allocations.retain(|a| {
        let id = valid_id(&a.bucket_id);
        if id.is_empty() || seen.contains(&id) {
            return false;
        }
        seen.push(id);
        true
    });
    for allocation in record.contribution_allocations.iter_mut() {
        allocation.amount = cents(allocation.amount);
    }

    let spread: Money = record
        .contribution_allocations
        .iter()
        .map(|a| a.amount)
        .sum();
    if !record.contribution_allocations.is_empty() {
        let held = record.contributions_amount.unwrap_or(Money::ZERO);
        if record.contributions_amount.is_none() || spread > held {
            return refuse(format!(
                "cannot allocate {spread} of contributions; the account records {}",
                match record.contributions_amount {
                    Some(amount) => amount.to_string(),
                    None => "none".into(),
                }
            ));
        }
    }

    Ok(record)
}

pub fn income(mut record: IncomeStream, keep: &str) -> Result<IncomeStream, WriteError> {
    record.name = plain(&record.name, NAME_MAX);
    if record.name.is_empty() {
        return refuse("income stream needs a name");
    }
    record.id = if keep.is_empty() {
        new_id()
    } else {
        keep.to_string()
    };
    record.owner = plain(&record.owner, NAME_MAX);
    record.monthly_total = cents(record.monthly_total);
    record.frequency = one_of(&record.frequency, FREQUENCIES, "biweekly");
    record.account_id = valid_id(&record.account_id);
    record.notes = plain(&record.notes, NOTES_MAX);
    Ok(record)
}

pub fn budget(mut record: BudgetItem, keep: &str) -> Result<BudgetItem, WriteError> {
    record.name = plain(&record.name, NAME_MAX);
    if record.name.is_empty() {
        return refuse("budget item needs a name");
    }
    record.id = if keep.is_empty() {
        new_id()
    } else {
        keep.to_string()
    };
    // Free text, not an enum: an item whose type was deleted still has to keep
    // the label it was filed under.
    record.kind = plain(&record.kind, 80);
    if record.kind.is_empty() {
        record.kind = DEFAULT_BUDGET_TYPES[0].to_string();
    }
    record.monthly_amount = cents(record.monthly_amount).abs();
    record.account_id = valid_id(&record.account_id);
    record.bucket_id = valid_id(&record.bucket_id);
    record.notes = plain(&record.notes, NOTES_MAX);

    record.expenses.truncate(caps::EXPENSES);
    record
        .expenses
        .retain(|e| !plain(&e.name, NAME_MAX).is_empty());
    for expense in record.expenses.iter_mut() {
        expense.id = keep_or_new(&expense.id);
        expense.name = plain(&expense.name, NAME_MAX);
        expense.amount = cents(expense.amount).abs();
        expense.account_id = valid_id(&expense.account_id);
        expense.notes = plain(&expense.notes, LINE_NOTES_MAX);
        expense.draw_date = iso_date(&expense.draw_date);
        if expense.recurring {
            expense.interval = one_of(&expense.interval, INTERVALS, "monthly");
            expense.end_date = iso_date(&expense.end_date);
            // An end before the first occurrence describes a schedule with no
            // occurrences at all, so it is dropped rather than stored.
            if !expense.end_date.is_empty()
                && !expense.draw_date.is_empty()
                && expense.end_date <= expense.draw_date
            {
                expense.end_date = String::new();
            }
        } else {
            expense.interval = String::new();
            expense.end_date = String::new();
        }
    }
    Ok(record)
}

pub fn bucket(mut record: Bucket, keep: &str) -> Result<Bucket, WriteError> {
    record.name = plain(&record.name, NAME_MAX);
    if record.name.is_empty() {
        return refuse("savings bucket needs a name");
    }
    record.id = if keep.is_empty() {
        new_id()
    } else {
        keep.to_string()
    };
    record.target_amount = cents_opt(record.target_amount);
    // Cash only, and a bucket can never hold a negative balance.
    record.current_total = cents(record.current_total).floor_at_zero();
    record.linked_account_id = valid_id(&record.linked_account_id);
    record.notes = plain(&record.notes, NOTES_MAX);
    Ok(record)
}

/// A `yyyy-mm` month, or empty.
pub fn month_key(value: &str) -> String {
    let text = plain(value, 7);
    let ok = text.len() == 7
        && text.as_bytes()[4] == b'-'
        && text[..4].bytes().all(|b| b.is_ascii_digit())
        && matches!(text[5..].parse::<u8>(), Ok(1..=12));
    if ok { text } else { String::new() }
}

/// An employer account's schedule. `now` is the current `yyyy-mm`, where a
/// newly switched-on top-up starts its clock.
pub fn retirement(mut block: Retirement, now: &str) -> Result<Retirement, WriteError> {
    block.sleeves.truncate(caps::SLEEVES);
    let mut seen: Vec<String> = Vec::new();
    let mut sleeves = Vec::with_capacity(block.sleeves.len());
    for mut sleeve in block.sleeves {
        sleeve.name = plain(&sleeve.name, NAME_MAX);
        sleeve.percent = cents(sleeve.percent).min(Money::from(100));
        if sleeve.name.is_empty() || sleeve.percent <= Money::ZERO {
            continue;
        }
        sleeve.id = keep_or_new(&sleeve.id);
        if seen.contains(&sleeve.id) {
            continue;
        }
        seen.push(sleeve.id.clone());
        sleeve.asset_class = one_of(&sleeve.asset_class, ASSET_CLASSES, "");
        sleeves.push(sleeve);
    }
    // Under 100 is a partial picture; over 100 would make the mix chart lie.
    let spread: Money = sleeves.iter().map(|s| s.percent).sum();
    if spread > Money::from(100) {
        return refuse(format!(
            "target weights come to {spread}%; they cannot exceed 100%"
        ));
    }
    block.sleeves = sleeves;

    block.monthly_contribution = cents_opt(block.monthly_contribution);
    if block.monthly_contribution.is_some_and(|m| m.is_negative()) {
        return refuse("a monthly contribution cannot be negative");
    }
    block.accrued_through = month_key(&block.accrued_through);
    // Switched on with no start month would accrue from nothing, so the clock
    // starts now; switched off clears it, so no dead months are caught up later.
    let contributes = block.monthly_contribution.is_some_and(|m| !m.is_zero());
    if block.auto_contribute && contributes && block.accrued_through.is_empty() {
        block.accrued_through = now.to_string();
    }
    if !block.auto_contribute {
        block.accrued_through.clear();
    }
    Ok(block)
}

fn trade(mut record: Trade) -> Option<Trade> {
    record.quantity = Money::price(record.quantity.inner()).abs();
    if record.quantity <= Money::ZERO {
        return None;
    }
    record.id = keep_or_new(&record.id);
    record.kind = one_of(&record.kind, TRADE_KINDS, "buy");
    record.price = Money::price(record.price.inner()).abs();
    record.at = plain(&record.at, 32);
    if record.at.is_empty() {
        record.at = crate::now_iso();
    }
    record.notes = plain(&record.notes, LINE_NOTES_MAX);
    Some(record)
}

pub fn holding(mut record: Holding, keep: &str) -> Result<Holding, WriteError> {
    record.name = plain(&record.name, NAME_MAX);
    if record.name.is_empty() {
        return refuse("holding needs a name");
    }
    record.id = if keep.is_empty() {
        new_id()
    } else {
        keep.to_string()
    };
    record.ticker = ticker(&record.ticker);
    record.kind = plain(&record.kind, 80);
    if record.kind.is_empty() {
        record.kind = DEFAULT_INVESTMENT_TYPES[0].to_string();
    }
    record.account_id = valid_id(&record.account_id);
    record.bucket_id = valid_id(&record.bucket_id);
    // Quantities and prices keep four places; two loses money on a big position.
    record.quantity = Money::price(record.quantity.inner()).floor_at_zero();
    record.cost_basis = cents_opt(record.cost_basis);
    record.avg_cost = average(record.cost_basis, record.quantity);
    record.price = record.price.map(|p| Money::price(p.inner()));
    record.price_at = plain(&record.price_at, 32);
    record.asset_class = one_of(&record.asset_class, ASSET_CLASSES, "");
    record.sector = plain(&record.sector, 60);
    record.sector_at = plain(&record.sector_at, 32);
    record.purchase_date = iso_date(&record.purchase_date);
    record.notes = plain(&record.notes, NOTES_MAX);
    record.trades.truncate(caps::TRADES);
    record.trades = std::mem::take(&mut record.trades)
        .into_iter()
        .filter_map(trade)
        .collect();
    Ok(record)
}

/// Average cost is derived from basis over quantity, never taken on trust, or
/// every gain figure built on it is wrong.
pub fn average(basis: Option<Money>, quantity: Money) -> Money {
    match basis {
        Some(basis) if quantity > Money::ZERO => Money::price(basis.inner() / quantity.inner()),
        _ => Money::ZERO,
    }
}

pub(crate) fn recon_line(mut record: ReconLine) -> Option<ReconLine> {
    record.amount = cents(record.amount);
    if record.amount <= Money::ZERO {
        return None;
    }
    record.id = keep_or_new(&record.id);
    record.label = plain(&record.label, NAME_MAX);
    record.member = plain(&record.member, NAME_MAX);
    record.spent_on = iso_date(&record.spent_on);
    record.bucket_id = valid_id(&record.bucket_id);
    record.notes = plain(&record.notes, LINE_NOTES_MAX);
    Some(record)
}

fn applied_moves(rows: Vec<AppliedMove>, credit: bool) -> Vec<AppliedMove> {
    rows.into_iter()
        .take(caps::BUCKETS)
        .filter_map(|mut row| {
            row.id = valid_id(&row.id);
            row.amount = cents(row.amount);
            row.credit_delta = if credit {
                cents_opt(row.credit_delta)
            } else {
                None
            };
            (!row.id.is_empty() && row.amount > Money::ZERO).then_some(row)
        })
        .collect()
}

/// `stored` is true only for a record read from a document. From an edit the
/// settle state is ignored, so a client cannot claim money moved that did not.
pub fn reconciliation(
    mut record: Reconciliation,
    keep: &str,
    stored: bool,
) -> Result<Reconciliation, WriteError> {
    record.card = plain(&record.card, NAME_MAX);
    if record.card.is_empty() {
        return refuse("reconciliation needs a card name");
    }
    record.id = if keep.is_empty() {
        new_id()
    } else {
        keep.to_string()
    };
    record.card_account_id = valid_id(&record.card_account_id);
    record.statement_date = iso_date(&record.statement_date);
    record.balance = cents(record.balance);
    record.bucket_source_id = valid_id(&record.bucket_source_id);
    record.spend_source_id = valid_id(&record.spend_source_id);
    record.notes = plain(&record.notes, NOTES_MAX);
    record.lines.truncate(caps::RECON_LINES);
    record.lines = std::mem::take(&mut record.lines)
        .into_iter()
        .filter_map(recon_line)
        .collect();

    record.status = if stored {
        one_of(&record.status, RECON_STATUSES, "open")
    } else {
        "open".into()
    };
    if record.status == "settled" {
        record.settled_at = plain(&record.settled_at, 32);
        record.applied = record.applied.take().map(|a| Applied {
            buckets: applied_moves(a.buckets, false),
            accounts: applied_moves(a.accounts, true),
        });
    } else {
        record.settled_at.clear();
        record.applied = None;
    }
    Ok(record)
}

pub fn goal(mut record: Goal, keep: &str) -> Result<Goal, WriteError> {
    record.name = plain(&record.name, NAME_MAX);
    if record.name.is_empty() {
        return refuse("goal needs a name");
    }
    record.id = if keep.is_empty() {
        new_id()
    } else {
        keep.to_string()
    };
    record.target_amount = cents_opt(record.target_amount);
    record.bucket_id = valid_id(&record.bucket_id);
    record.notes = plain(&record.notes, NOTES_MAX);
    Ok(record)
}

fn template_item(mut record: TemplateItem) -> Option<TemplateItem> {
    record.name = plain(&record.name, NAME_MAX);
    if record.name.is_empty() {
        return None;
    }
    record.id = keep_or_new(&record.id);
    record.kind = plain(&record.kind, 80);
    if record.kind.is_empty() {
        record.kind = DEFAULT_BUDGET_TYPES[0].to_string();
    }
    record.monthly_amount = cents(record.monthly_amount).abs();
    record.bucket_id = valid_id(&record.bucket_id);
    Some(record)
}

/// A saved budget to put back later: names and amounts, not links to live
/// records, so it survives the budget being rewritten.
pub fn template(mut record: Template, keep: &str) -> Result<Template, WriteError> {
    record.name = plain(&record.name, NAME_MAX);
    if record.name.is_empty() {
        return refuse("template needs a name");
    }
    record.id = if keep.is_empty() {
        new_id()
    } else {
        keep.to_string()
    };
    record.notes = plain(&record.notes, NOTES_MAX);
    record.saved_at = plain(&record.saved_at, 32);
    if record.saved_at.is_empty() {
        record.saved_at = crate::now_iso();
    }
    record.items.truncate(caps::BUDGET);
    record.items = std::mem::take(&mut record.items)
        .into_iter()
        .filter_map(template_item)
        .collect();
    Ok(record)
}
