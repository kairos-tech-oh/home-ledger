//! The only thing that changes a ledger.
//!
//! Every mutation is an [`Op`]. Applying one validates the input, clamps it to
//! the caps, produces an audit entry, and leaves the document changed — or
//! refuses and changes nothing. There is no other write path.
//!
//! Port target: `helper/ledger.py` from the Omarchy plugin, with
//! `tools/check-helper.py` there as the oracle. See `docs/PORT.md`.

pub mod audit;
pub mod bank_csv;
pub mod clean;
mod dashboard;
mod holdings;
pub mod import;
mod layout;
pub mod reconcile;
mod retirement;
mod templates;
mod types;

pub use types::TypeList;

use ledger_domain::records::{AuditChange, AuditEntry, caps};
use ledger_domain::text::{NAME_MAX, new_id, plain, valid_id};
use ledger_domain::{Ledger, Lineage, Money, Relation};
use ledger_store::{Document, PendingOp};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    /// A rule said no. The message is shown to the person, so it names what to
    /// fix rather than what failed internally.
    #[error("{0}")]
    Refused(String),
    /// The record an op names is not there.
    #[error("{0}")]
    Missing(String),
    #[error("document is not readable: {0}")]
    Corrupt(String),
    /// Valid, but there was nothing to do, so nothing is written or logged.
    #[error("{0}")]
    Unchanged(String),
}

/// Which collection a record op acts on.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Kind {
    Account,
    Income,
    Budget,
    Bucket,
    Holding,
    Goal,
    Template,
}

impl Kind {
    /// What the audit log calls one of these.
    fn label(self) -> &'static str {
        match self {
            Kind::Account => "account",
            Kind::Income => "income stream",
            Kind::Budget => "budget line",
            Kind::Bucket => "savings bucket",
            Kind::Holding => "holding",
            Kind::Goal => "goal",
            Kind::Template => "budget template",
        }
    }

    fn limit(self) -> usize {
        match self {
            Kind::Account => caps::ACCOUNTS,
            Kind::Income => caps::INCOME,
            Kind::Budget => caps::BUDGET,
            Kind::Bucket => caps::BUCKETS,
            Kind::Holding => caps::INVESTMENTS,
            Kind::Goal => caps::GOALS,
            Kind::Template => caps::TEMPLATES,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Adjustment {
    pub id: String,
    pub delta: Money,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Side {
    Buy,
    Sell,
}

/// An edit, named the way the audit log names it. Fields are camelCase, as
/// the interface sends them.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "kebab-case", rename_all_fields = "camelCase")]
pub enum Op {
    /// Create or replace one record. **Merges**: a field the caller leaves out
    /// keeps what is stored, so saving an account from one editor cannot wipe
    /// what another editor set. To clear a field, send it explicitly.
    Set {
        kind: Kind,
        #[serde(default)]
        id: String,
        record: Value,
    },
    Delete {
        kind: Kind,
        id: String,
    },
    /// One atomic list of bucket balance deltas: a contribution, everyone's at
    /// once, the undo of either, or an expense split across buckets.
    BucketAdjust {
        adjustments: Vec<Adjustment>,
        #[serde(default)]
        label: String,
    },
    /// Set a bucket's balance by hand, to match a real account.
    BucketTotal {
        id: String,
        amount: Money,
    },
    BucketMove {
        from_id: String,
        to_id: String,
        amount: Money,
    },
    /// What a quote refresh found. One op for the whole sweep, so a slow
    /// network costs one write rather than one per holding.
    PriceHoldings {
        #[serde(default)]
        priced: Vec<Priced>,
        /// Looked up and not answered. The last price is kept and flagged,
        /// because a stale figure is worth more than no figure.
        #[serde(default)]
        stale: Vec<String>,
    },
    /// Buy or sell shares of a holding, re-deriving its average cost.
    Trade {
        id: String,
        side: Side,
        quantity: Money,
        price: Money,
        #[serde(default)]
        notes: String,
    },
    /// An employer account's schedule. Merges like `Set`, one level down.
    RetirementSet {
        id: String,
        record: Value,
    },
    /// Add the months gone by since each automatic top-up last ran. An empty
    /// id means every account.
    RetirementAccrue {
        #[serde(default)]
        id: String,
    },
    /// Start or edit an open statement. Replaces rather than merges: a
    /// statement is edited whole, lines included.
    ReconcileSet {
        #[serde(default)]
        id: String,
        record: Value,
    },
    ReconcileDelete {
        id: String,
    },
    /// Append charges read from a bank export to an open statement.
    ReconcileImport {
        id: String,
        lines: Vec<Value>,
    },
    /// Pay the statement: draw from buckets and accounts, pay down the card.
    ReconcileSettle {
        id: String,
        /// How to cover a bucket that holds less than this statement takes
        /// from it. A bucket not named here uses its own default, and with
        /// neither the settle is refused before anything moves.
        #[serde(default)]
        cover: Vec<reconcile::Cover>,
    },
    /// Return exactly what the settle moved, and reopen the statement.
    ReconcileUndo {
        id: String,
    },
    /// Replace the live budget with a template's lines.
    TemplateActivate {
        id: String,
        #[serde(default = "yes")]
        keep_current: bool,
    },
    TypeAdd {
        list: TypeList,
        name: String,
    },
    TypeDelete {
        list: TypeList,
        name: String,
    },
    /// Replace the dashboard layout whole.
    DashboardSet {
        dashboard: Value,
    },
    /// The order of the Accounts page's sections; empty for the default.
    AccountOrderSet {
        #[serde(default)]
        order: Vec<String>,
    },
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Priced {
    pub id: String,
    pub price: Money,
}

/// Applies ops to a document. Holds no state, so it can be shared freely.
#[derive(Default)]
pub struct Writer {
    pub device: String,
}

impl Writer {
    pub fn new(device: impl Into<String>) -> Self {
        Self {
            device: device.into(),
        }
    }

    /// Apply one op. Returns the audit entry describing what changed.
    ///
    /// Either the document is changed and an entry returned, or an error is
    /// returned and the document is untouched — never half of each.
    pub fn apply(&self, ledger: &mut Ledger, op: &Op) -> Result<AuditEntry, WriteError> {
        let entry = match op {
            Op::Set { kind, id, record } => self.set(ledger, *kind, id, record)?,
            Op::Delete { kind, id } => self.delete(ledger, *kind, id)?,
            Op::BucketAdjust { adjustments, label } => self.adjust(ledger, adjustments, label)?,
            Op::BucketTotal { id, amount } => self.bucket_total(ledger, id, *amount)?,
            Op::BucketMove {
                from_id,
                to_id,
                amount,
            } => self.mv(ledger, from_id, to_id, *amount)?,
            Op::PriceHoldings { priced, stale } => self.price(ledger, priced, stale)?,
            Op::Trade {
                id,
                side,
                quantity,
                price,
                notes,
            } => self.trade(ledger, id, *side, *quantity, *price, notes)?,
            Op::RetirementSet { id, record } => self.retirement_set(ledger, id, record)?,
            Op::RetirementAccrue { id } => self.accrue(ledger, id, &this_month())?,
            Op::ReconcileSet { id, record } => self.reconcile_set(ledger, id, record)?,
            Op::ReconcileDelete { id } => self.reconcile_delete(ledger, id)?,
            Op::ReconcileImport { id, lines } => self.reconcile_import(ledger, id, lines)?,
            Op::ReconcileSettle { id, cover } => self.settle(ledger, id, cover)?,
            Op::ReconcileUndo { id } => self.undo(ledger, id)?,
            Op::TemplateActivate { id, keep_current } => {
                self.activate(ledger, id, *keep_current)?
            }
            Op::TypeAdd { list, name } => self.type_add(ledger, *list, name)?,
            Op::TypeDelete { list, name } => self.type_delete(ledger, *list, name)?,
            Op::DashboardSet { dashboard } => self.dashboard_set(ledger, dashboard)?,
            Op::AccountOrderSet { order } => self.account_order_set(ledger, order)?,
        };
        Ok(entry)
    }

    /// Finish an audit entry: give it an identity, a time and an author.
    fn finish(&self, mut entry: AuditEntry, op: &str) -> AuditEntry {
        entry.id = new_id();
        entry.op = op.into();
        entry.actor = self.device.clone();
        entry.at = now_iso();
        entry
    }

    // --------------------------------------------------------------- set

    fn set(
        &self,
        ledger: &mut Ledger,
        kind: Kind,
        id: &str,
        supplied: &Value,
    ) -> Result<AuditEntry, WriteError> {
        if !supplied.is_object() {
            return Err(WriteError::Refused(format!(
                "{} is not an object",
                kind.label()
            )));
        }
        let wanted = valid_id(id);
        let names = audit::name_index(ledger);

        // Read the stored record as JSON, merge what was supplied over it, and
        // clean the result. Merging here rather than on the typed record is
        // what preserves "a field left out keeps its stored value".
        let existing: Option<Value> = match kind {
            Kind::Account => find_json(&ledger.accounts, &wanted, |a| &a.id),
            Kind::Income => find_json(&ledger.income, &wanted, |a| &a.id),
            Kind::Budget => find_json(&ledger.budget, &wanted, |a| &a.id),
            Kind::Bucket => find_json(&ledger.buckets, &wanted, |a| &a.id),
            Kind::Holding => find_json(&ledger.investments, &wanted, |a| &a.id),
            Kind::Goal => find_json(&ledger.goals, &wanted, |a| &a.id),
            Kind::Template => find_json(&ledger.templates, &wanted, |a| &a.id),
        };

        let merged = match &existing {
            Some(before) => merge(before, supplied),
            None => supplied.clone(),
        };
        let keep = existing.as_ref().map(|_| wanted.as_str()).unwrap_or("");

        let creating = existing.is_none();
        if creating {
            let count = match kind {
                Kind::Account => ledger.accounts.len(),
                Kind::Income => ledger.income.len(),
                Kind::Budget => ledger.budget.len(),
                Kind::Bucket => ledger.buckets.len(),
                Kind::Holding => ledger.investments.len(),
                Kind::Goal => ledger.goals.len(),
                Kind::Template => ledger.templates.len(),
            };
            if count >= kind.limit() {
                return Err(WriteError::Refused(format!(
                    "no room for another {}",
                    kind.label()
                )));
            }
        }

        let (after, name) = match kind {
            Kind::Account => {
                let record = clean::account(from_json(&merged)?, keep)?;
                check_secured_loan(ledger, &record)?;
                let name = record.name.clone();
                (store(&mut ledger.accounts, record, |a| &a.id)?, name)
            }
            Kind::Income => {
                let record = clean::income(from_json(&merged)?, keep)?;
                let name = record.name.clone();
                (store(&mut ledger.income, record, |a| &a.id)?, name)
            }
            Kind::Budget => {
                let record = clean::budget(from_json(&merged)?, keep)?;
                let name = record.name.clone();
                (store(&mut ledger.budget, record, |a| &a.id)?, name)
            }
            Kind::Bucket => {
                let record = clean::bucket(from_json(&merged)?, keep)?;
                if !record.cover_bucket_id.is_empty()
                    && (record.cover_bucket_id == record.id
                        || ledger.bucket(&record.cover_bucket_id).is_none())
                {
                    return Err(WriteError::Refused(
                        "choose another bucket to cover this one when it runs short".into(),
                    ));
                }
                let name = record.name.clone();
                (store(&mut ledger.buckets, record, |a| &a.id)?, name)
            }
            Kind::Holding => {
                let record = clean::holding(from_json(&merged)?, keep)?;
                let name = record.name.clone();
                (store(&mut ledger.investments, record, |a| &a.id)?, name)
            }
            Kind::Goal => {
                let record = clean::goal(from_json(&merged)?, keep)?;
                let name = record.name.clone();
                (store(&mut ledger.goals, record, |a| &a.id)?, name)
            }
            Kind::Template => {
                let record = clean::template(from_json(&merged)?, keep)?;
                let name = record.name.clone();
                (store(&mut ledger.templates, record, |a| &a.id)?, name)
            }
        };

        let changes = audit::diff_fields(existing.as_ref(), &after, &names);
        Ok(self.finish(
            AuditEntry {
                action: if creating { "Create" } else { "Update" }.into(),
                subject: kind.label().into(),
                name,
                changes,
                ..Default::default()
            },
            if creating { "create" } else { "update" },
        ))
    }

    // ------------------------------------------------------------ delete

    fn delete(&self, ledger: &mut Ledger, kind: Kind, id: &str) -> Result<AuditEntry, WriteError> {
        let wanted = valid_id(id);
        let gone = match kind {
            Kind::Account => take(&mut ledger.accounts, &wanted, |a| &a.id).map(|a| a.name),
            Kind::Income => take(&mut ledger.income, &wanted, |a| &a.id).map(|a| a.name),
            Kind::Budget => take(&mut ledger.budget, &wanted, |a| &a.id).map(|a| a.name),
            Kind::Bucket => take(&mut ledger.buckets, &wanted, |a| &a.id).map(|a| a.name),
            Kind::Holding => take(&mut ledger.investments, &wanted, |a| &a.id).map(|a| a.name),
            Kind::Goal => take(&mut ledger.goals, &wanted, |a| &a.id).map(|a| a.name),
            Kind::Template => take(&mut ledger.templates, &wanted, |a| &a.id).map(|a| a.name),
        };
        let Some(name) = gone else {
            return Err(WriteError::Missing(format!("no such {}", kind.label())));
        };

        unlink(ledger, kind, &wanted);

        Ok(self.finish(
            AuditEntry {
                action: "Delete".into(),
                subject: kind.label().into(),
                name,
                ..Default::default()
            },
            "delete",
        ))
    }

    // ------------------------------------------------------------ buckets

    fn adjust(
        &self,
        ledger: &mut Ledger,
        adjustments: &[Adjustment],
        label: &str,
    ) -> Result<AuditEntry, WriteError> {
        if adjustments.is_empty() || adjustments.len() > caps::BUCKETS {
            return Err(WriteError::Refused(
                "adjustments must be a non-empty list".into(),
            ));
        }

        // Resolve every target before moving anything: the whole list applies
        // or none of it does.
        let mut targets = Vec::with_capacity(adjustments.len());
        let mut seen: Vec<String> = Vec::new();
        for row in adjustments {
            let id = valid_id(&row.id);
            let Some(index) = ledger.buckets.iter().position(|b| b.id == id) else {
                return Err(WriteError::Missing("no such bucket".into()));
            };
            if seen.contains(&id) {
                return Err(WriteError::Refused(
                    "the same bucket twice in one adjustment".into(),
                ));
            }
            seen.push(id);
            targets.push((index, row.delta));
        }

        let mut changes = Vec::new();
        let mut net = Money::ZERO;
        for (index, delta) in targets {
            let bucket = &mut ledger.buckets[index];
            let before = bucket.current_total;
            // Never below zero, and never lower than a bucket already overdrawn
            // by a settle: money added fills it back up, and nothing can be
            // spent from it until it is above zero again.
            let floor = if before.is_negative() {
                before
            } else {
                Money::ZERO
            };
            let after = (before + delta).max(floor);
            bucket.current_total = after;
            net += after - before;
            if changes.len() < caps::CHANGES {
                changes.push(AuditChange {
                    field: bucket.name.clone(),
                    from: before.to_string(),
                    to: after.to_string(),
                });
            }
        }

        let named = plain(label, NAME_MAX);
        let count = adjustments.len();
        Ok(self.finish(
            AuditEntry {
                action: if net.is_negative() { "Remove" } else { "Add" }.into(),
                subject: "savings".into(),
                name: if named.is_empty() {
                    format!("{count} bucket{}", if count == 1 { "" } else { "s" })
                } else {
                    named
                },
                amount: Some(net),
                changes,
                ..Default::default()
            },
            "bucket-adjust",
        ))
    }

    /// Prices are written where they are found and flagged where they are not.
    /// A holding priced by hand is never touched, whatever was sent for it.
    fn price(
        &self,
        ledger: &mut Ledger,
        priced: &[Priced],
        stale: &[String],
    ) -> Result<AuditEntry, WriteError> {
        let mut changes = Vec::new();
        let mut done = 0usize;
        let mut flagged = 0usize;

        for found in priced {
            let wanted = valid_id(&found.id);
            let Some(holding) = ledger
                .investments
                .iter_mut()
                .find(|h| h.id == wanted && !h.fixed_price)
            else {
                continue;
            };
            let was = Money::new(
                holding.price.unwrap_or(holding.avg_cost).inner() * holding.quantity.inner(),
            );
            holding.price = Some(Money::price(found.price.inner()));
            holding.price_at = now_iso();
            holding.price_stale = false;
            let now =
                Money::new(holding.price.unwrap_or_default().inner() * holding.quantity.inner());
            done += 1;
            if was != now && changes.len() < caps::CHANGES {
                changes.push(AuditChange {
                    field: holding.name.clone(),
                    from: was.to_string(),
                    to: now.to_string(),
                });
            }
        }

        for id in stale {
            let wanted = valid_id(id);
            if let Some(holding) = ledger
                .investments
                .iter_mut()
                .find(|h| h.id == wanted && !h.fixed_price)
            {
                holding.price_stale = true;
                flagged += 1;
            }
        }

        if done == 0 && flagged == 0 {
            return Err(WriteError::Refused("no holding to price".into()));
        }

        Ok(self.finish(
            AuditEntry {
                action: "Quote".into(),
                subject: "prices".into(),
                name: format!("{done} priced, {flagged} left stale"),
                changes,
                ..Default::default()
            },
            "price-holdings",
        ))
    }

    fn bucket_total(
        &self,
        ledger: &mut Ledger,
        id: &str,
        amount: Money,
    ) -> Result<AuditEntry, WriteError> {
        let wanted = valid_id(id);
        let Some(bucket) = ledger.buckets.iter_mut().find(|b| b.id == wanted) else {
            return Err(WriteError::Missing("no such bucket".into()));
        };
        let was = bucket.current_total;
        bucket.current_total = Money::new(amount.inner()).floor_at_zero();
        let now = bucket.current_total;

        Ok(self.finish(
            AuditEntry {
                action: "Manual".into(),
                subject: "savings bucket".into(),
                name: bucket.name.clone(),
                amount: Some(now - was),
                changes: vec![AuditChange {
                    field: "balance".into(),
                    from: was.to_string(),
                    to: now.to_string(),
                }],
                ..Default::default()
            },
            "bucket-total",
        ))
    }

    fn mv(
        &self,
        ledger: &mut Ledger,
        from_id: &str,
        to_id: &str,
        amount: Money,
    ) -> Result<AuditEntry, WriteError> {
        let amount = Money::new(amount.inner());
        if amount <= Money::ZERO {
            return Err(WriteError::Refused(
                "the amount to move must be positive".into(),
            ));
        }
        let from = valid_id(from_id);
        let to = valid_id(to_id);
        let (Some(source), Some(target)) = (
            ledger.buckets.iter().position(|b| b.id == from),
            ledger.buckets.iter().position(|b| b.id == to),
        ) else {
            return Err(WriteError::Missing("no such bucket".into()));
        };
        if source == target {
            return Err(WriteError::Refused("that is the same bucket".into()));
        }
        if ledger.buckets[source].locked {
            return Err(WriteError::Refused(format!(
                "{} is locked, so funds cannot be moved out of it",
                ledger.buckets[source].name
            )));
        }
        if ledger.buckets[source].current_total < amount {
            return Err(WriteError::Refused(format!(
                "{} does not hold that much",
                ledger.buckets[source].name
            )));
        }

        let from_before = ledger.buckets[source].current_total;
        let to_before = ledger.buckets[target].current_total;
        ledger.buckets[source].current_total = from_before - amount;
        ledger.buckets[target].current_total = to_before + amount;

        let name = format!(
            "{} → {}",
            ledger.buckets[source].name, ledger.buckets[target].name
        );
        let changes = vec![
            AuditChange {
                field: ledger.buckets[source].name.clone(),
                from: from_before.to_string(),
                to: ledger.buckets[source].current_total.to_string(),
            },
            AuditChange {
                field: ledger.buckets[target].name.clone(),
                from: to_before.to_string(),
                to: ledger.buckets[target].current_total.to_string(),
            },
        ];

        Ok(self.finish(
            AuditEntry {
                action: "Move".into(),
                subject: "savings".into(),
                name,
                amount: Some(amount),
                changes,
                ..Default::default()
            },
            "bucket-move",
        ))
    }
}

// ------------------------------------------------------------------ helpers

/// Parse a document the way every screen and replay should see it: with card
/// balances brought in line with their open statements.
pub fn read(bytes: &[u8]) -> Result<Ledger, serde_json::Error> {
    let mut ledger = Ledger::from_bytes(bytes)?;
    reconcile::card_balances(&mut ledger);
    Ok(ledger)
}

fn yes() -> bool {
    true
}

/// The current `yyyy-mm`, in UTC: `time` will not read a local offset from a
/// threaded process. A top-up can land a few hours early or late, never twice.
pub fn this_month() -> String {
    let now = time::OffsetDateTime::now_utc();
    format!("{:04}-{:02}", now.year(), u8::from(now.month()))
}

/// UTC, to the second. Audit entries are ordered and read by people; a
/// sub-second timestamp would add noise and no information.
pub fn now_iso() -> String {
    const SHAPE: &[time::format_description::FormatItem<'_>] =
        time::macros::format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]Z");
    time::OffsetDateTime::now_utc()
        .format(SHAPE)
        .unwrap_or_default()
}

fn find_json<T: Serialize>(rows: &[T], id: &str, key: impl Fn(&T) -> &String) -> Option<Value> {
    if id.is_empty() {
        return None;
    }
    rows.iter()
        .find(|row| key(row) == id)
        .and_then(|row| serde_json::to_value(row).ok())
}

fn from_json<T: for<'de> Deserialize<'de>>(value: &Value) -> Result<T, WriteError> {
    serde_json::from_value(value.clone()).map_err(|e| WriteError::Refused(e.to_string()))
}

/// Shallow merge, matching the prototype: a key the caller supplied replaces
/// the stored one outright, including a list.
fn merge(before: &Value, supplied: &Value) -> Value {
    let (Some(base), Some(over)) = (before.as_object(), supplied.as_object()) else {
        return supplied.clone();
    };
    let mut out = base.clone();
    for (key, value) in over {
        out.insert(key.clone(), value.clone());
    }
    Value::Object(out)
}

/// Put the record where it belongs, replacing one with the same id. Returns
/// the stored record as JSON, for the audit diff.
fn store<T: Serialize>(
    rows: &mut Vec<T>,
    record: T,
    key: impl Fn(&T) -> &String,
) -> Result<Value, WriteError> {
    let as_json = serde_json::to_value(&record).map_err(|e| WriteError::Corrupt(e.to_string()))?;
    match rows.iter().position(|row| key(row) == key(&record)) {
        Some(index) => rows[index] = record,
        None => rows.push(record),
    }
    Ok(as_json)
}

fn take<T>(rows: &mut Vec<T>, id: &str, key: impl Fn(&T) -> &String) -> Option<T> {
    if id.is_empty() {
        return None;
    }
    rows.iter()
        .position(|row| key(row) == id)
        .map(|index| rows.remove(index))
}

/// A property or vehicle may name one loan or HELOC that exists, and that no
/// other property or vehicle already names; otherwise the equity it shows
/// would count one loan twice.
fn check_secured_loan(ledger: &Ledger, record: &ledger_domain::Account) -> Result<(), WriteError> {
    let wanted = &record.loan_account_id;
    if wanted.is_empty() {
        return Ok(());
    }
    match ledger.account(wanted) {
        Some(loan) if loan.can_be_secured() && loan.id != record.id => {}
        _ => {
            return Err(WriteError::Refused(
                "choose a loan or HELOC account for this to secure".into(),
            ));
        }
    }
    if let Some(other) = ledger
        .accounts
        .iter()
        .find(|a| a.id != record.id && &a.loan_account_id == wanted)
    {
        return Err(WriteError::Refused(format!(
            "that loan is already secured against {}",
            other.name
        )));
    }
    Ok(())
}

/// Clear references to a record that has just been deleted, so nothing points
/// at an id that is no longer there.
fn unlink(ledger: &mut Ledger, kind: Kind, id: &str) {
    // The dashboard layout is not modelled here, but its widgets name
    // accounts, buckets and goals, and must not point at one that is gone.
    if matches!(kind, Kind::Account | Kind::Bucket | Kind::Goal)
        && let Some(widgets) = ledger
            .unknown
            .get_mut("dashboard")
            .and_then(|d| d.get_mut("widgets"))
            .and_then(Value::as_array_mut)
    {
        for widget in widgets.iter_mut() {
            if let Some(refs) = widget.get_mut("refs").and_then(Value::as_array_mut) {
                refs.retain(|r| r.as_str() != Some(id));
            }
        }
    }
    match kind {
        Kind::Account => {
            for account in ledger.accounts.iter_mut() {
                if account.loan_account_id == id {
                    account.loan_account_id.clear();
                }
            }
            for stream in ledger.income.iter_mut() {
                if stream.account_id == id {
                    stream.account_id.clear();
                }
            }
            for item in ledger.budget.iter_mut() {
                if item.account_id == id {
                    item.account_id.clear();
                }
                for expense in item.expenses.iter_mut() {
                    if expense.account_id == id {
                        expense.account_id.clear();
                    }
                }
            }
            for bucket in ledger.buckets.iter_mut() {
                if bucket.linked_account_id == id {
                    bucket.linked_account_id.clear();
                }
            }
            for holding in ledger.investments.iter_mut() {
                if holding.account_id == id {
                    holding.account_id.clear();
                }
            }
        }
        Kind::Bucket => {
            for item in ledger.budget.iter_mut() {
                if item.bucket_id == id {
                    item.bucket_id.clear();
                }
            }
            for holding in ledger.investments.iter_mut() {
                if holding.bucket_id == id {
                    holding.bucket_id.clear();
                }
            }
            for goal in ledger.goals.iter_mut() {
                if goal.bucket_id == id {
                    goal.bucket_id.clear();
                }
            }
            // A bucket that was covered by this one goes back to asking.
            for bucket in ledger.buckets.iter_mut() {
                if bucket.cover_bucket_id == id {
                    bucket.cover_bucket_id.clear();
                    bucket.when_short.clear();
                }
            }
            for account in ledger.accounts.iter_mut() {
                account
                    .contribution_allocations
                    .retain(|a| a.bucket_id != id);
            }
        }
        Kind::Budget | Kind::Income | Kind::Holding | Kind::Goal | Kind::Template => {}
    }
}

/// What the store engine needs from something that understands the document:
/// replay a queued outbox onto it, stamp its lineage, and place two copies
/// against each other.
impl Document for Writer {
    fn replay(&self, document: Option<&[u8]>, ops: &[PendingOp]) -> Result<Vec<u8>, String> {
        let mut ledger = match document {
            Some(bytes) => read(bytes).map_err(|e| e.to_string())?,
            None => Ledger::default(),
        };
        for pending in ops {
            let op: Op = serde_json::from_value(pending.op.clone()).map_err(|e| e.to_string())?;
            self.apply(&mut ledger, &op).map_err(|e| e.to_string())?;
        }
        ledger.to_bytes().map_err(|e| e.to_string())
    }

    fn stamp(&self, document: &[u8], writer: &str) -> Result<Vec<u8>, String> {
        let mut ledger = Ledger::from_bytes(document).map_err(|e| e.to_string())?;
        // A document with no lineage is starting one here rather than being
        // given a history it never had.
        ledger.lineage = Some(match &ledger.lineage {
            Some(previous) => previous.next(writer),
            None => Lineage::start(writer),
        });
        ledger.updated_at = now_iso();
        ledger.updated_by = self.device.clone();
        ledger.to_bytes().map_err(|e| e.to_string())
    }

    fn relation(&self, ours: &[u8], theirs: &[u8]) -> Relation {
        let ours = Ledger::from_bytes(ours).ok();
        let theirs = Ledger::from_bytes(theirs).ok();
        ledger_domain::lineage::compare(
            ours.as_ref().and_then(|l| l.lineage.as_ref()),
            theirs.as_ref().and_then(|l| l.lineage.as_ref()),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn writer() -> Writer {
        Writer::new("test")
    }

    fn make(ledger: &mut Ledger, kind: Kind, record: Value) -> String {
        writer()
            .apply(
                ledger,
                &Op::Set {
                    kind,
                    id: String::new(),
                    record,
                },
            )
            .expect("created");
        match kind {
            Kind::Account => ledger.accounts.last().unwrap().id.clone(),
            Kind::Income => ledger.income.last().unwrap().id.clone(),
            Kind::Budget => ledger.budget.last().unwrap().id.clone(),
            Kind::Bucket => ledger.buckets.last().unwrap().id.clone(),
            Kind::Holding => ledger.investments.last().unwrap().id.clone(),
            Kind::Goal => ledger.goals.last().unwrap().id.clone(),
            Kind::Template => ledger.templates.last().unwrap().id.clone(),
        }
    }

    fn bucket_with(ledger: &mut Ledger, name: &str, total: i64) -> String {
        make(
            ledger,
            Kind::Bucket,
            json!({ "name": name, "currentTotal": total }),
        )
    }

    // ------------------------------------------------------------- set

    #[test]
    fn creating_a_record_gives_it_an_id_and_logs_a_create() {
        let mut ledger = Ledger::default();
        let entry = writer()
            .apply(
                &mut ledger,
                &Op::Set {
                    kind: Kind::Account,
                    id: String::new(),
                    record: json!({ "name": "Checking", "type": "checking", "total": 100 }),
                },
            )
            .expect("created");

        assert_eq!(ledger.accounts.len(), 1);
        assert_eq!(entry.action, "Create");
        assert_eq!(entry.name, "Checking");
        assert_eq!(valid_id(&ledger.accounts[0].id).len(), 32);
    }

    #[test]
    fn a_field_left_out_of_an_update_keeps_what_was_stored() {
        // The rule that matters most: saving an account from one editor must
        // not erase what another editor set on it.
        let mut ledger = Ledger::default();
        let id = make(
            &mut ledger,
            Kind::Account,
            json!({
                "name": "Roth", "type": "retirement-roth",
                "total": 5000, "institution": "Vanguard", "contributionsAmount": 1200
            }),
        );

        writer()
            .apply(
                &mut ledger,
                &Op::Set {
                    kind: Kind::Account,
                    id: id.clone(),
                    record: json!({ "total": 6000 }),
                },
            )
            .expect("updated");

        let account = ledger.account(&id).unwrap();
        assert_eq!(account.total, Some(Money::from(6000)));
        assert_eq!(account.institution, "Vanguard", "institution was wiped");
        assert_eq!(
            account.contributions_amount,
            Some(Money::from(1200)),
            "contributions were wiped"
        );
        assert_eq!(account.name, "Roth");
    }

    #[test]
    fn a_field_can_be_cleared_by_sending_it_explicitly() {
        let mut ledger = Ledger::default();
        let id = make(
            &mut ledger,
            Kind::Account,
            json!({
                "name": "Checking", "type": "checking", "institution": "Chase"
            }),
        );

        writer()
            .apply(
                &mut ledger,
                &Op::Set {
                    kind: Kind::Account,
                    id: id.clone(),
                    record: json!({ "institution": "" }),
                },
            )
            .expect("updated");

        assert_eq!(ledger.account(&id).unwrap().institution, "");
    }

    #[test]
    fn an_update_keeps_the_id_rather_than_adding_a_second_record() {
        let mut ledger = Ledger::default();
        let id = make(&mut ledger, Kind::Bucket, json!({ "name": "Emergency" }));
        writer()
            .apply(
                &mut ledger,
                &Op::Set {
                    kind: Kind::Bucket,
                    id: id.clone(),
                    record: json!({ "name": "Emergency fund" }),
                },
            )
            .expect("updated");

        assert_eq!(ledger.buckets.len(), 1);
        assert_eq!(ledger.buckets[0].id, id);
        assert_eq!(ledger.buckets[0].name, "Emergency fund");
    }

    #[test]
    fn a_record_with_no_name_is_refused_and_nothing_is_stored() {
        let mut ledger = Ledger::default();
        let refused = writer().apply(
            &mut ledger,
            &Op::Set {
                kind: Kind::Account,
                id: String::new(),
                record: json!({ "type": "checking", "total": 100 }),
            },
        );

        assert!(matches!(refused, Err(WriteError::Refused(_))));
        assert!(
            ledger.accounts.is_empty(),
            "a refused write left something behind"
        );
    }

    #[test]
    fn a_credit_limit_is_dropped_when_the_type_is_not_a_card() {
        let mut ledger = Ledger::default();
        let id = make(
            &mut ledger,
            Kind::Account,
            json!({
                "name": "Card", "type": "credit", "availableCredit": 4000
            }),
        );
        assert!(ledger.account(&id).unwrap().available_credit.is_some());

        writer()
            .apply(
                &mut ledger,
                &Op::Set {
                    kind: Kind::Account,
                    id: id.clone(),
                    record: json!({ "type": "checking" }),
                },
            )
            .expect("updated");

        assert!(
            ledger.account(&id).unwrap().available_credit.is_none(),
            "a stale credit limit survived a type change"
        );
    }

    #[test]
    fn allocating_more_contributions_than_are_recorded_is_refused() {
        let mut ledger = Ledger::default();
        let bucket = bucket_with(&mut ledger, "Emergency", 0);
        let refused = writer().apply(
            &mut ledger,
            &Op::Set {
                kind: Kind::Account,
                id: String::new(),
                record: json!({
                    "name": "Roth", "type": "retirement-roth", "contributionsAmount": 100,
                    "contributionAllocations": [ { "bucketId": bucket, "amount": 500 } ]
                }),
            },
        );
        assert!(matches!(refused, Err(WriteError::Refused(_))));
    }

    // ---------------------------------------------------------- delete

    #[test]
    fn deleting_an_account_clears_what_pointed_at_it() {
        let mut ledger = Ledger::default();
        let account = make(&mut ledger, Kind::Account, json!({ "name": "Checking" }));
        make(
            &mut ledger,
            Kind::Income,
            json!({ "name": "Salary", "accountId": account }),
        );
        make(
            &mut ledger,
            Kind::Budget,
            json!({ "name": "Rent", "accountId": account }),
        );

        writer()
            .apply(
                &mut ledger,
                &Op::Delete {
                    kind: Kind::Account,
                    id: account,
                },
            )
            .expect("deleted");

        assert!(ledger.accounts.is_empty());
        assert_eq!(
            ledger.income[0].account_id, "",
            "income still points at a deleted account"
        );
        assert_eq!(
            ledger.budget[0].account_id, "",
            "budget still points at a deleted account"
        );
    }

    #[test]
    fn deleting_something_that_is_not_there_is_reported_not_ignored() {
        let mut ledger = Ledger::default();
        let missing = writer().apply(
            &mut ledger,
            &Op::Delete {
                kind: Kind::Bucket,
                id: new_id(),
            },
        );
        assert!(matches!(missing, Err(WriteError::Missing(_))));
    }

    // --------------------------------------------------------- buckets

    #[test]
    fn an_adjustment_moves_every_balance_and_reports_the_net() {
        let mut ledger = Ledger::default();
        let a = bucket_with(&mut ledger, "Emergency", 100);
        let b = bucket_with(&mut ledger, "Car", 50);

        let entry = writer()
            .apply(
                &mut ledger,
                &Op::BucketAdjust {
                    adjustments: vec![
                        Adjustment {
                            id: a,
                            delta: Money::from(25),
                        },
                        Adjustment {
                            id: b,
                            delta: Money::from(-10),
                        },
                    ],
                    label: "payday".into(),
                },
            )
            .expect("adjusted");

        assert_eq!(ledger.buckets[0].current_total, Money::from(125));
        assert_eq!(ledger.buckets[1].current_total, Money::from(40));
        assert_eq!(entry.amount, Some(Money::from(15)));
        assert_eq!(entry.action, "Add");
        assert_eq!(entry.name, "payday");
    }

    #[test]
    fn an_adjustment_naming_a_missing_bucket_moves_nothing() {
        // Resolved up front on purpose: a partial application would leave one
        // bucket credited and the other not.
        let mut ledger = Ledger::default();
        let a = bucket_with(&mut ledger, "Emergency", 100);

        let refused = writer().apply(
            &mut ledger,
            &Op::BucketAdjust {
                adjustments: vec![
                    Adjustment {
                        id: a,
                        delta: Money::from(25),
                    },
                    Adjustment {
                        id: new_id(),
                        delta: Money::from(25),
                    },
                ],
                label: String::new(),
            },
        );

        assert!(matches!(refused, Err(WriteError::Missing(_))));
        assert_eq!(
            ledger.buckets[0].current_total,
            Money::from(100),
            "a balance moved anyway"
        );
    }

    #[test]
    fn the_same_bucket_twice_in_one_adjustment_is_refused() {
        let mut ledger = Ledger::default();
        let a = bucket_with(&mut ledger, "Emergency", 100);
        let refused = writer().apply(
            &mut ledger,
            &Op::BucketAdjust {
                adjustments: vec![
                    Adjustment {
                        id: a.clone(),
                        delta: Money::from(25),
                    },
                    Adjustment {
                        id: a,
                        delta: Money::from(25),
                    },
                ],
                label: String::new(),
            },
        );
        assert!(matches!(refused, Err(WriteError::Refused(_))));
    }

    #[test]
    fn a_bucket_cannot_be_driven_negative() {
        let mut ledger = Ledger::default();
        let a = bucket_with(&mut ledger, "Emergency", 30);
        writer()
            .apply(
                &mut ledger,
                &Op::BucketAdjust {
                    adjustments: vec![Adjustment {
                        id: a,
                        delta: Money::from(-100),
                    }],
                    label: String::new(),
                },
            )
            .expect("adjusted");
        assert_eq!(ledger.buckets[0].current_total, Money::ZERO);
    }

    #[test]
    fn moving_funds_leaves_the_total_unchanged() {
        let mut ledger = Ledger::default();
        let a = bucket_with(&mut ledger, "Emergency", 100);
        let b = bucket_with(&mut ledger, "Car", 50);

        writer()
            .apply(
                &mut ledger,
                &Op::BucketMove {
                    from_id: a,
                    to_id: b,
                    amount: Money::from(30),
                },
            )
            .expect("moved");

        assert_eq!(ledger.buckets[0].current_total, Money::from(70));
        assert_eq!(ledger.buckets[1].current_total, Money::from(80));
    }

    #[test]
    fn funds_cannot_be_moved_out_of_a_locked_bucket() {
        let mut ledger = Ledger::default();
        let a = make(
            &mut ledger,
            Kind::Bucket,
            json!({ "name": "Retirement", "currentTotal": 500, "locked": true }),
        );
        let b = bucket_with(&mut ledger, "Car", 0);

        let refused = writer().apply(
            &mut ledger,
            &Op::BucketMove {
                from_id: a,
                to_id: b,
                amount: Money::from(100),
            },
        );

        assert!(matches!(refused, Err(WriteError::Refused(_))));
        assert_eq!(ledger.buckets[0].current_total, Money::from(500));
        assert_eq!(ledger.buckets[1].current_total, Money::ZERO);
    }

    #[test]
    fn a_bucket_cannot_pay_out_more_than_it_holds() {
        let mut ledger = Ledger::default();
        let a = bucket_with(&mut ledger, "Emergency", 20);
        let b = bucket_with(&mut ledger, "Car", 0);
        let refused = writer().apply(
            &mut ledger,
            &Op::BucketMove {
                from_id: a,
                to_id: b,
                amount: Money::from(100),
            },
        );
        assert!(matches!(refused, Err(WriteError::Refused(_))));
        assert_eq!(ledger.buckets[0].current_total, Money::from(20));
    }

    #[test]
    fn ops_read_the_field_names_the_interface_sends() {
        // Fields were snake_case, so the UI's `fromId` never parsed and every
        // bucket move was refused before it reached the writer.
        let from = new_id();
        let op: Op = serde_json::from_value(json!({
            "op": "bucket-move", "fromId": from, "toId": new_id(), "amount": "5"
        }))
        .expect("the interface's bucket move parses");
        assert!(matches!(op, Op::BucketMove { from_id, .. } if from_id == from));

        let op: Op = serde_json::from_value(json!({
            "op": "trade", "id": new_id(), "side": "sell", "quantity": 1, "price": 2
        }))
        .expect("a trade parses");
        assert!(matches!(
            op,
            Op::Trade {
                side: Side::Sell,
                ..
            }
        ));
    }

    #[test]
    fn moving_to_the_same_bucket_is_refused() {
        let mut ledger = Ledger::default();
        let a = bucket_with(&mut ledger, "Emergency", 100);
        let refused = writer().apply(
            &mut ledger,
            &Op::BucketMove {
                from_id: a.clone(),
                to_id: a,
                amount: Money::from(10),
            },
        );
        assert!(matches!(refused, Err(WriteError::Refused(_))));
    }

    #[test]
    fn setting_a_balance_by_hand_records_the_difference() {
        let mut ledger = Ledger::default();
        let a = bucket_with(&mut ledger, "Emergency", 100);
        let entry = writer()
            .apply(
                &mut ledger,
                &Op::BucketTotal {
                    id: a,
                    amount: Money::from(175),
                },
            )
            .expect("set");

        assert_eq!(ledger.buckets[0].current_total, Money::from(175));
        assert_eq!(entry.amount, Some(Money::from(75)));
        assert_eq!(entry.action, "Manual");
    }

    // ----------------------------------------------------------- audit

    #[test]
    fn every_entry_carries_the_device_and_an_id() {
        let mut ledger = Ledger::default();
        let entry = writer()
            .apply(
                &mut ledger,
                &Op::Set {
                    kind: Kind::Bucket,
                    id: String::new(),
                    record: json!({ "name": "Emergency" }),
                },
            )
            .expect("created");
        assert_eq!(entry.actor, "test");
        assert_eq!(valid_id(&entry.id).len(), 32);
    }

    #[test]
    fn every_entry_is_timestamped_to_the_second_in_utc() {
        let mut ledger = Ledger::default();
        let entry = writer()
            .apply(
                &mut ledger,
                &Op::Set {
                    kind: Kind::Bucket,
                    id: String::new(),
                    record: json!({ "name": "Emergency" }),
                },
            )
            .expect("created");
        assert_eq!(
            entry.at.len(),
            20,
            "unexpected timestamp shape: {}",
            entry.at
        );
        assert!(entry.at.ends_with('Z'));
    }

    #[test]
    fn a_queued_op_replays_onto_a_document_someone_else_changed() {
        // What the outbox does after the primary was unreachable: the edit is
        // applied to whatever is there now, not to the copy it was made against.
        let mut theirs = Ledger::default();
        make(&mut theirs, Kind::Bucket, json!({ "name": "Theirs" }));
        let document = theirs.to_bytes().unwrap();

        let queued = PendingOp {
            id: new_id(),
            at: now_iso(),
            device: "laptop".into(),
            op: serde_json::to_value(Op::Set {
                kind: Kind::Bucket,
                id: String::new(),
                record: json!({ "name": "Ours" }),
            })
            .unwrap(),
        };

        let merged = writer()
            .replay(Some(&document), &[queued])
            .expect("replayed");
        let after = Ledger::from_bytes(&merged).unwrap();

        let names: Vec<&str> = after.buckets.iter().map(|b| b.name.as_str()).collect();
        assert_eq!(names, vec!["Theirs", "Ours"], "a replay lost one side");
    }

    #[test]
    fn a_replay_that_breaks_a_rule_fails_rather_than_writing_half_of_it() {
        let mut ledger = Ledger::default();
        let locked = make(
            &mut ledger,
            Kind::Bucket,
            json!({ "name": "Retirement", "currentTotal": 100, "locked": true }),
        );
        let other = bucket_with(&mut ledger, "Car", 0);
        let document = ledger.to_bytes().unwrap();

        let queued = PendingOp {
            id: new_id(),
            at: now_iso(),
            device: "laptop".into(),
            op: serde_json::to_value(Op::BucketMove {
                from_id: locked,
                to_id: other,
                amount: Money::from(50),
            })
            .unwrap(),
        };

        assert!(writer().replay(Some(&document), &[queued]).is_err());
    }

    // --------------------------------------------------------- lineage

    #[test]
    fn a_document_with_no_lineage_starts_one_rather_than_inventing_a_history() {
        let plain = Ledger::default().to_bytes().unwrap();
        let stamped = writer().stamp(&plain, "s3").expect("stamped");
        let doc = Ledger::from_bytes(&stamped).unwrap();

        let lineage = doc.lineage.expect("a lineage was started");
        assert_eq!(lineage.generation, 1);
        assert_eq!(lineage.writer, "s3");
        assert!(lineage.history.is_empty(), "claimed a history it never had");
    }

    #[test]
    fn stamping_again_advances_rather_than_restarting() {
        let w = writer();
        let first = w
            .stamp(&Ledger::default().to_bytes().unwrap(), "s3")
            .unwrap();
        let second = w.stamp(&first, "s3").unwrap();

        let a = Ledger::from_bytes(&first).unwrap().lineage.unwrap();
        let b = Ledger::from_bytes(&second).unwrap().lineage.unwrap();
        assert_eq!(b.generation, 2);
        assert_eq!(b.history[0], a.revision);
    }

    #[test]
    fn a_backup_left_behind_is_recognised_as_safe_to_fast_forward() {
        let w = writer();
        let backup = w
            .stamp(&Ledger::default().to_bytes().unwrap(), "s3")
            .unwrap();
        let truth = w.stamp(&backup, "s3").unwrap();

        assert_eq!(w.relation(&truth, &backup), Relation::Ahead);
        assert_eq!(w.relation(&backup, &truth), Relation::Behind);
    }

    #[test]
    fn two_stores_that_both_moved_on_are_reported_as_forked() {
        // Promotion must never resolve this by itself.
        let w = writer();
        let shared = w
            .stamp(&Ledger::default().to_bytes().unwrap(), "s3")
            .unwrap();
        let ours = w.stamp(&shared, "s3").unwrap();
        let theirs = w.stamp(&shared, "drive").unwrap();

        assert_eq!(w.relation(&ours, &theirs), Relation::Forked);
    }

    #[test]
    fn a_document_from_the_app_this_replaces_cannot_be_placed() {
        let w = writer();
        let stamped = w
            .stamp(&Ledger::default().to_bytes().unwrap(), "s3")
            .unwrap();
        let unstamped = Ledger::default().to_bytes().unwrap();

        assert_eq!(w.relation(&stamped, &unstamped), Relation::TooFarApart);
    }

    #[test]
    fn stamping_leaves_every_record_alone() {
        let mut ledger = Ledger::default();
        make(
            &mut ledger,
            Kind::Bucket,
            json!({ "name": "Emergency", "currentTotal": 100 }),
        );
        let before = ledger.to_bytes().unwrap();

        let after = writer().stamp(&before, "s3").expect("stamped");
        let doc = Ledger::from_bytes(&after).unwrap();

        assert_eq!(doc.buckets.len(), 1);
        assert_eq!(doc.buckets[0].name, "Emergency");
        assert_eq!(doc.buckets[0].current_total, Money::from(100));
    }

    #[test]
    fn an_update_names_a_referenced_bucket_rather_than_its_id() {
        let mut ledger = Ledger::default();
        let bucket = bucket_with(&mut ledger, "Emergency fund", 0);
        let item = make(&mut ledger, Kind::Budget, json!({ "name": "Savings" }));

        let entry = writer()
            .apply(
                &mut ledger,
                &Op::Set {
                    kind: Kind::Budget,
                    id: item,
                    record: json!({ "bucketId": bucket }),
                },
            )
            .expect("updated");

        let change = entry
            .changes
            .iter()
            .find(|c| c.field == "bucketId")
            .expect("named");
        assert_eq!(change.from, "none");
        assert_eq!(change.to, "Emergency fund");
    }

    fn priceable(ledger: &mut Ledger, ticker: &str, fixed: bool) -> String {
        let id = new_id();
        ledger.investments.push(ledger_domain::records::Holding {
            id: id.clone(),
            name: format!("{ticker} holding"),
            ticker: ticker.into(),
            quantity: Money::from(10),
            avg_cost: Money::from(100),
            fixed_price: fixed,
            ..Default::default()
        });
        id
    }

    #[test]
    fn a_holding_priced_by_hand_is_never_overwritten_by_a_quote() {
        let mut ledger = Ledger::default();
        let live = priceable(&mut ledger, "VOO", false);
        let fixed = priceable(&mut ledger, "GOLD", true);

        writer()
            .apply(
                &mut ledger,
                &Op::PriceHoldings {
                    priced: vec![
                        Priced {
                            id: live.clone(),
                            price: Money::from(200),
                        },
                        Priced {
                            id: fixed.clone(),
                            price: Money::from(999),
                        },
                    ],
                    stale: vec![],
                },
            )
            .expect("applies");

        let moved = ledger.investments.iter().find(|h| h.id == live).unwrap();
        let untouched = ledger.investments.iter().find(|h| h.id == fixed).unwrap();
        assert_eq!(moved.price, Some(Money::from(200)));
        assert!(!moved.price_stale);
        assert!(!moved.price_at.is_empty());
        assert_eq!(untouched.price, None);
    }

    #[test]
    fn a_symbol_that_did_not_answer_keeps_its_last_price_and_is_flagged() {
        let mut ledger = Ledger::default();
        let id = priceable(&mut ledger, "VOO", false);
        ledger.investments[0].price = Some(Money::from(150));

        writer()
            .apply(
                &mut ledger,
                &Op::PriceHoldings {
                    priced: vec![],
                    stale: vec![id],
                },
            )
            .expect("applies");

        let held = &ledger.investments[0];
        assert_eq!(held.price, Some(Money::from(150)));
        assert!(held.price_stale);
    }

    #[test]
    fn a_sweep_that_touched_nothing_is_refused_rather_than_logged() {
        let mut ledger = Ledger::default();
        priceable(&mut ledger, "VOO", false);
        assert!(
            writer()
                .apply(
                    &mut ledger,
                    &Op::PriceHoldings {
                        priced: vec![Priced {
                            id: "nosuchholding".into(),
                            price: Money::from(1),
                        }],
                        stale: vec![],
                    },
                )
                .is_err()
        );
    }
}

#[cfg(test)]
mod secured_tests {
    use crate::{Kind, Op, Writer};
    use ledger_domain::Ledger;
    use serde_json::{Value, json};

    fn set(ledger: &mut Ledger, id: &str, record: Value) -> Result<String, crate::WriteError> {
        Writer::new("test").apply(
            ledger,
            &Op::Set {
                kind: Kind::Account,
                id: id.into(),
                record,
            },
        )?;
        Ok(if id.is_empty() {
            ledger.accounts.last().unwrap().id.clone()
        } else {
            id.to_string()
        })
    }

    #[test]
    fn a_home_names_the_mortgage_against_it() {
        let mut ledger = Ledger::default();
        let mortgage = set(
            &mut ledger,
            "",
            json!({ "name": "Mortgage", "type": "loan", "total": 195494 }),
        )
        .unwrap();
        let condo = set(
            &mut ledger,
            "",
            json!({ "name": "Condo", "type": "property", "total": 260000, "loanAccountId": mortgage }),
        )
        .unwrap();
        assert_eq!(ledger.account(&condo).unwrap().loan_account_id, mortgage);
        // Value is an asset and the loan a debt: net is the equity.
        let worth = ledger_math::net_worth(&ledger);
        assert_eq!(worth.net, ledger_domain::Money::from(260000 - 195494));
    }

    #[test]
    fn only_a_loan_or_heloc_can_be_named_and_only_once() {
        let mut ledger = Ledger::default();
        let checking = set(
            &mut ledger,
            "",
            json!({ "name": "Checking", "type": "checking" }),
        )
        .unwrap();
        assert!(
            set(
                &mut ledger,
                "",
                json!({ "name": "Condo", "type": "property", "loanAccountId": checking })
            )
            .is_err()
        );
        assert!(
            set(
                &mut ledger,
                "",
                json!({ "name": "Condo", "type": "property", "loanAccountId": "a".repeat(32) })
            )
            .is_err()
        );

        let loan = set(
            &mut ledger,
            "",
            json!({ "name": "Auto Loan", "type": "loan" }),
        )
        .unwrap();
        set(
            &mut ledger,
            "",
            json!({ "name": "Car", "type": "vehicle", "loanAccountId": loan }),
        )
        .unwrap();
        let err = set(
            &mut ledger,
            "",
            json!({ "name": "Second car", "type": "vehicle", "loanAccountId": loan }),
        );
        assert!(err.is_err_and(|e| e.to_string().contains("Car")));
    }

    #[test]
    fn only_a_property_or_vehicle_keeps_a_link() {
        let mut ledger = Ledger::default();
        let loan = set(&mut ledger, "", json!({ "name": "Loan", "type": "loan" })).unwrap();
        let car = set(
            &mut ledger,
            "",
            json!({ "name": "Car", "type": "vehicle", "loanAccountId": loan }),
        )
        .unwrap();
        set(&mut ledger, &car, json!({ "type": "other" })).unwrap();
        assert!(ledger.account(&car).unwrap().loan_account_id.is_empty());
    }

    #[test]
    fn deleting_the_loan_lets_go_of_the_link() {
        let mut ledger = Ledger::default();
        let loan = set(&mut ledger, "", json!({ "name": "Loan", "type": "loan" })).unwrap();
        let car = set(
            &mut ledger,
            "",
            json!({ "name": "Car", "type": "vehicle", "loanAccountId": loan }),
        )
        .unwrap();
        Writer::new("test")
            .apply(
                &mut ledger,
                &Op::Delete {
                    kind: Kind::Account,
                    id: loan,
                },
            )
            .unwrap();
        assert!(ledger.account(&car).unwrap().loan_account_id.is_empty());
    }

    #[test]
    fn an_account_without_a_link_writes_no_link() {
        let mut ledger = Ledger::default();
        set(
            &mut ledger,
            "",
            json!({ "name": "Checking", "type": "checking" }),
        )
        .unwrap();
        let out = String::from_utf8(ledger.to_bytes().unwrap()).unwrap();
        assert!(!out.contains("loanAccountId"));
    }
}
