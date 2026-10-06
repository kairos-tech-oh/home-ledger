//! The records a ledger holds. Field names match the JSON the existing
//! helper writes, so a document written by either can be read by both.

use crate::money::Money;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const ACCOUNT_TYPES: &[&str] = &[
    "checking",
    "savings",
    "credit",
    "heloc",
    "retirement-roth",
    "retirement-traditional",
    "loan",
    "investment",
    "property",
    "vehicle",
    "other",
];

/// Types whose balance is money owed, not money held.
pub const LIABILITY_TYPES: &[&str] = &["credit", "heloc", "loan"];
/// Types whose money is put away for retirement.
pub const RETIREMENT_TYPES: &[&str] = &["retirement-roth", "retirement-traditional"];
/// Types that record how much more could be borrowed.
pub const CREDIT_TYPES: &[&str] = &["credit", "heloc"];
/// Things owned that a loan can be secured against: a home, a car. Their
/// total is what they are worth, counted as an asset like any other.
pub const SECURED_TYPES: &[&str] = &["property", "vehicle"];
/// Types that can be the loan against one of those.
pub const SECURING_TYPES: &[&str] = &["loan", "heloc"];

pub const DEBT_KINDS: &[&str] = &[
    "credit-card",
    "mortgage",
    "auto-loan",
    "student-loan",
    "personal-loan",
    "medical",
    "other",
];

pub const FREQUENCIES: &[&str] = &[
    "weekly",
    "biweekly",
    "semimonthly",
    "monthly",
    "quarterly",
    "annual",
];

/// How often a planned expense comes round.
pub const INTERVALS: &[&str] = &[
    "weekly",
    "biweekly",
    "monthly",
    "quarterly",
    "every6months",
    "yearly",
];

/// The six a budget ships with. A family adds its own; only these are
/// undeletable, so a budget can never have no type to file an item under.
pub const DEFAULT_BUDGET_TYPES: &[&str] = &[
    "Living",
    "Spending",
    "Savings (Wants)",
    "Savings (Needs)",
    "Giving/Tithe",
    "Investing",
];

pub const DEFAULT_INVESTMENT_TYPES: &[&str] = &[
    "Stock/Security",
    "Bond",
    "Crypto",
    "Cash/Money Market",
    "Option/Derivative",
    "Real Estate",
    "Other",
];

pub const TRADE_KINDS: &[&str] = &["buy", "sell"];
pub const RECON_STATUSES: &[&str] = &["open", "settled"];

pub const ASSET_CLASSES: &[&str] = &[
    "us-large",
    "us-mid",
    "us-small",
    "international",
    "emerging",
    "bonds",
    "real-estate",
    "target-date",
    "cash",
    "crypto",
    "other",
];

pub mod caps {
    pub const ACCOUNTS: usize = 500;
    pub const INCOME: usize = 200;
    pub const BUDGET: usize = 300;
    pub const BUCKETS: usize = 200;
    pub const TYPES: usize = 40;
    /// Assets or debts on one account.
    pub const LINES: usize = 200;
    /// Planned expenses on one budget item.
    pub const EXPENSES: usize = 100;
    pub const INVESTMENTS: usize = 500;
    /// Buy/sell history on one holding.
    pub const TRADES: usize = 500;
    pub const GOALS: usize = 100;
    pub const TEMPLATES: usize = 20;
    /// Roth contribution splits on one account.
    pub const ALLOCATIONS: usize = 50;
    /// Target weights on one retirement account.
    pub const SLEEVES: usize = 30;
    /// Catch-up ceiling for automatic top-ups.
    pub const ACCRUAL_MONTHS: u32 = 120;
    pub const RECONCILIATIONS: usize = 520;
    pub const RECON_LINES: usize = 300;
    pub const AUDIT: usize = 500;
    /// Fields named in one audit entry.
    pub const CHANGES: usize = 12;
    pub const DOCUMENT_BYTES: usize = 4 * 1024 * 1024;
}

// A holding is a collection of its own ([`Holding`]), not a line nested on an
// account: it can belong to an account, to a savings bucket, to both, or to
// neither, which nesting cannot express.

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Debt {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub balance: Money,
    /// An APR, held between 0 and 100.
    pub rate: Money,
    pub payment: Money,
    pub notes: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Allocation {
    pub bucket_id: String,
    pub amount: Money,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Sleeve {
    pub id: String,
    pub name: String,
    pub percent: Money,
    pub asset_class: String,
}

/// A retirement schedule. Kept only for a retirement account type and dropped
/// when the type changes, so no stale schedule lingers.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Retirement {
    pub monthly_contribution: Option<Money>,
    #[serde(default)]
    pub auto_contribute: bool,
    /// `yyyy-mm` of the last month added; empty when top-ups are off.
    pub accrued_through: String,
    #[serde(default)]
    pub sleeves: Vec<Sleeve>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Account {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub institution: String,
    pub total: Option<Money>,
    /// Only a credit card or HELOC keeps this. Negative means over limit.
    pub available_credit: Option<Money>,
    pub notes: String,
    pub contributions_amount: Option<Money>,
    #[serde(default)]
    pub contribution_allocations: Vec<Allocation>,
    pub retirement: Option<Retirement>,
    #[serde(default)]
    pub debts: Vec<Debt>,
    /// On a property or vehicle, the loan account secured against it, so the
    /// two read together as equity. Absent rather than empty when unset, so a
    /// document written before this field reads and writes back unchanged.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub loan_account_id: String,
}

impl Account {
    pub fn can_secure_a_loan(&self) -> bool {
        SECURED_TYPES.contains(&self.kind.as_str())
    }
    pub fn can_be_secured(&self) -> bool {
        SECURING_TYPES.contains(&self.kind.as_str())
    }
    pub fn is_liability(&self) -> bool {
        LIABILITY_TYPES.contains(&self.kind.as_str())
    }
    pub fn is_retirement(&self) -> bool {
        RETIREMENT_TYPES.contains(&self.kind.as_str())
    }
    pub fn takes_credit_limit(&self) -> bool {
        CREDIT_TYPES.contains(&self.kind.as_str())
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct IncomeStream {
    pub id: String,
    pub name: String,
    pub owner: String,
    pub monthly_total: Money,
    pub frequency: String,
    pub account_id: String,
    pub notes: String,
}

/// One planned draw against a budget item: a bill, a premium, a subscription.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct PlannedExpense {
    pub id: String,
    pub name: String,
    pub amount: Money,
    pub account_id: String,
    #[serde(default)]
    pub recurring: bool,
    pub draw_date: String,
    pub interval: String,
    pub end_date: String,
    pub notes: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct BudgetItem {
    pub id: String,
    pub name: String,
    /// Free text, not an enum: an item whose type was deleted still has to
    /// keep the label it was filed under.
    #[serde(rename = "type")]
    pub kind: String,
    pub monthly_amount: Money,
    pub account_id: String,
    pub bucket_id: String,
    pub notes: String,
    #[serde(default)]
    pub expenses: Vec<PlannedExpense>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Bucket {
    pub id: String,
    pub name: String,
    pub target_amount: Option<Money>,
    /// Cash only. Below zero only when a statement was settled with "let it go
    /// negative": the bucket then owes that much until it is refilled.
    pub current_total: Money,
    pub linked_account_id: String,
    /// Says "do not move funds out" without depending on the bucket's name.
    #[serde(default)]
    pub locked: bool,
    pub notes: String,
    /// What settling does when a statement needs more than this bucket holds:
    /// empty to ask, or one of [`WHEN_SHORT`]. Absent rather than empty when
    /// unset, so a document written before this field reads back unchanged.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub when_short: String,
    /// With `when_short` of "bucket", the bucket the rest comes from.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub cover_bucket_id: String,
}

/// How a shortfall is covered: from another bucket, from everyday spending,
/// or by letting the bucket go below zero.
pub const WHEN_SHORT: &[&str] = &["bucket", "everyday", "negative"];

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Trade {
    pub id: String,
    pub kind: String,
    pub quantity: Money,
    pub price: Money,
    pub at: String,
    pub notes: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Holding {
    pub id: String,
    pub name: String,
    pub ticker: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub quantity: Money,
    /// None rather than zero: an unknown basis must not read as a full gain.
    pub cost_basis: Option<Money>,
    /// Derived from basis over quantity on every trade, never taken on trust.
    pub avg_cost: Money,
    pub account_id: String,
    pub bucket_id: String,
    pub purchase_date: String,
    pub notes: String,
    pub price: Option<Money>,
    pub price_at: String,
    pub price_stale: bool,
    /// Such a holding is never sent to the quote API.
    pub fixed_price: bool,
    pub trades: Vec<Trade>,
    /// Set by hand when deriving the class from the name gets it wrong.
    pub asset_class: String,
    /// Industry, cached from a quote refresh.
    pub sector: String,
    pub sector_at: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Goal {
    pub id: String,
    pub name: String,
    pub bucket_id: String,
    pub target_amount: Option<Money>,
    pub notes: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct TemplateItem {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub monthly_amount: Money,
    pub bucket_id: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Template {
    pub id: String,
    pub name: String,
    pub notes: String,
    pub saved_at: String,
    pub items: Vec<TemplateItem>,
}

/// One charge on a card statement being reconciled.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ReconLine {
    pub id: String,
    pub label: String,
    /// Which family member made the charge.
    pub member: String,
    pub spent_on: String,
    pub amount: Money,
    /// Empty means everyday spending, paid out of checking.
    pub bucket_id: String,
    pub notes: String,
    /// The bank connection's id for the transaction this charge came from,
    /// so fetching it again never adds it twice. Absent for a typed charge.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bank_ref: String,
    /// The bank's own description ("KROGER #920 COLUMBUS OH"), kept when the
    /// label is the cleaner merchant name ("Kroger").
    #[serde(skip_serializing_if = "String::is_empty")]
    pub bank_text: String,
}

/// What a settle actually moved, kept so an undo can return it exactly.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AppliedMove {
    pub id: String,
    pub amount: Money,
    /// Only on the card's own move; absent elsewhere, as the prototype writes it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credit_delta: Option<Money>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Applied {
    #[serde(default)]
    pub buckets: Vec<AppliedMove>,
    #[serde(default)]
    pub accounts: Vec<AppliedMove>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Reconciliation {
    pub id: String,
    pub card: String,
    /// Kept so later edits stay unambiguous if the account is renamed.
    pub card_account_id: String,
    pub statement_date: String,
    pub balance: Money,
    /// Which bucket the bucket-backed lines are drawn from.
    pub bucket_source_id: String,
    /// Which account everyday spending is drawn from.
    pub spend_source_id: String,
    /// Whether settling moves the account balances too. Absent means yes.
    #[serde(default = "yes")]
    pub adjust_accounts: bool,
    /// "open" or "settled".
    pub status: String,
    pub settled_at: String,
    pub notes: String,
    pub lines: Vec<ReconLine>,
    pub applied: Option<Applied>,
}

fn yes() -> bool {
    true
}

// Written out rather than derived so a reconciliation built in code agrees
// with one read from a document: absent `adjustAccounts` means yes.
impl Default for Reconciliation {
    fn default() -> Self {
        Self {
            id: String::new(),
            card: String::new(),
            card_account_id: String::new(),
            statement_date: String::new(),
            balance: Money::ZERO,
            bucket_source_id: String::new(),
            spend_source_id: String::new(),
            adjust_accounts: true,
            status: String::new(),
            settled_at: String::new(),
            notes: String::new(),
            lines: Vec::new(),
            applied: None,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AuditChange {
    pub field: String,
    pub from: String,
    pub to: String,
}

/// A bounded diff: changed fields only, references rendered as the name they
/// point at, written inside the same transaction as the change itself.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AuditEntry {
    pub id: String,
    pub at: String,
    pub action: String,
    pub subject: String,
    pub name: String,
    pub op: String,
    /// The install's own name for itself: "Laptop", "Office PC", "Basement Pi".
    pub actor: String,
    /// Which program made the change: "desktop", "cli" or "mobile". Empty on
    /// entries written before this was recorded, or imported from the plugin.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub client: String,
    /// The install that made it: a permanent id, unchanged by a rename, so two
    /// machines given the same name are still told apart.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub install: String,
    /// The version of the app that made it.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub version: String,
    /// A label a script gave itself with `--via`, such as "nightly-import".
    #[serde(skip_serializing_if = "String::is_empty")]
    pub via: String,
    pub amount: Option<Money>,
    #[serde(default)]
    pub changes: Vec<AuditChange>,
}

/// A dated point of computed totals, for the history chart.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Snapshot {
    pub date: String,
    pub taken_at: String,
    pub net: Money,
    pub assets: Money,
    pub debts: Money,
    pub savings: Money,
    pub holdings: Money,
    pub basis: Money,
    pub monthly_income: Money,
    pub monthly_budget: Money,
    #[serde(default)]
    pub buckets: BTreeMap<String, Money>,
}

/// Collections this build does not model, carried through untouched so a
/// client that does not know about them cannot delete them.
pub type Passthrough = BTreeMap<String, Value>;
