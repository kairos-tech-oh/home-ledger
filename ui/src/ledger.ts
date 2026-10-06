// The ledger side of the app. Every amount arrives as a string, already
// rounded by Rust, so nothing here does arithmetic on money.
import { invoke } from "@tauri-apps/api/core";

export interface AccountView {
  id: string;
  name: string;
  kind: string;
  institution: string;
  notes: string;
  total: string | null;
  availableCredit: string | null;
  liability: boolean;
  holdings: string;
  holdingCount: number;
  debts: string;
  debtCount: number;
  net: string;
  loanAccountId: string;
  loanName: string;
  owedAgainst: string | null;
  equity: string | null;
  secures: string;
}

export interface BucketView {
  id: string;
  name: string;
  notes: string;
  cash: string;
  invested: string;
  contributions: string;
  total: string;
  target: string | null;
  progress: number | null;
  locked: boolean;
  fundedMonthly: string;
  fundedBy: string[];
  committed: string;
  committedShort: string | null;
  whenShort: "" | "bucket" | "everyday" | "negative";
  coverBucketId: string;
  coverBucketName: string;
}

export interface IncomeView {
  id: string;
  name: string;
  owner: string;
  monthlyTotal: string;
  frequency: string;
  accountId: string;
  accountName: string;
  notes: string;
  perPaycheck: string;
}

export interface SplitView {
  owner: string;
  amount: string;
}

export interface BudgetView {
  id: string;
  name: string;
  kind: string;
  monthlyAmount: string;
  accountId: string;
  accountName: string;
  bucketId: string;
  bucketName: string;
  notes: string;
  draws: number;
  drawsMonthly: string;
  split: SplitView[];
}

export interface HoldingView {
  id: string;
  name: string;
  ticker: string;
  kind: string;
  quantity: string;
  costBasis: string | null;
  avgCost: string;
  price: string | null;
  priceAt: string;
  priceStale: boolean;
  fixedPrice: boolean;
  value: string;
  gain: string | null;
  gainPercent: number | null;
  accountId: string;
  accountName: string;
  bucketId: string;
  bucketName: string;
  purchaseDate: string;
  assetClass: string;
  notes: string;
  trades: TradeView[];
}

export interface SleeveView {
  id: string;
  name: string;
  percent: string;
  assetClass: string;
}

export interface RetirementView {
  id: string;
  name: string;
  kind: string;
  institution: string;
  value: string;
  monthly: string;
  fromBudget: string;
  contributions: string;
  contribution: string | null;
  autoContribute: boolean;
  accruedThrough: string;
  sleeves: SleeveView[];
  holdings: number;
}

export interface ReconLineView {
  id: string;
  label: string;
  member: string;
  spentOn: string;
  amount: string;
  bucketId: string;
  bucketName: string;
  notes: string;
}

export interface ReconciliationView {
  id: string;
  card: string;
  cardAccountId: string;
  cardAccountName: string;
  bucketSourceId: string;
  spendSourceId: string;
  adjustAccounts: boolean;
  statementDate: string;
  balance: string;
  status: string;
  settledAt: string;
  notes: string;
  lines: ReconLineView[];
  linesTotal: string;
  unaccounted: string;
  shortfalls: ShortView[];
}

export interface ShortView {
  bucketId: string;
  bucketName: string;
  needs: string;
  holds: string;
  short: string;
  whenShort: "" | "bucket" | "everyday" | "negative";
  coverBucketId: string;
}

/** How one short bucket is covered when a statement is settled. */
export interface Cover {
  bucketId: string;
  how: "bucket" | "everyday" | "negative";
  fromBucketId: string;
}

export interface EarnerView {
  owner: string;
  monthly: string;
  percent: number;
  paychecksPerMonth: number;
  streams: number;
}

export interface GoalView {
  id: string;
  name: string;
  notes: string;
  bucketId: string;
  bucketName: string;
  target: string | null;
  saved: string;
  progress: number | null;
  remaining: string | null;
}

export interface GoalTotalsView {
  saved: string;
  target: string;
  withTarget: number;
  percent: number | null;
  remaining: string;
}

export interface DrawView {
  name: string;
  amount: string;
  occurrences: number;
  impact: string;
  schedule: string;
}

export interface BucketPlanView {
  id: string;
  name: string;
  current: string;
  projected: string;
  change: string;
  velocity: string | null;
  contributions: string;
  deductions: string;
  target: string | null;
  percent: number | null;
  monthsToGoal: number | null;
  draws: DrawView[];
}

export interface PlanningView {
  from: string;
  to: string;
  months: number;
  buckets: BucketPlanView[];
  current: string;
  contributions: string;
  deductions: string;
  projected: string;
  change: string;
}

export interface TemplateView {
  id: string;
  name: string;
  notes: string;
  savedAt: string;
  lines: number;
  monthly: string;
  againstNow: string;
}

export interface GroupView {
  key: string;
  name: string;
  amount: string;
  count: number;
  share: number;
}

export interface ChargeView {
  date: string;
  inferred: boolean;
  name: string;
  member: string;
  bucket: string;
  card: string;
  status: "open" | "settled";
  amount: string;
  reconciliationId: string;
}

export interface SpendingView {
  valid: boolean;
  start: string;
  end: string;
  total: string;
  count: number;
  open: string;
  settled: string;
  fromBuckets: string;
  everyday: string;
  withdrawn: string;
  unattributed: string;
  average: string;
  undated: number;
  inferredDates: number;
  unitemized: string;
  missingSettlementHistory: number;
  undatedWithdrawals: number;
  people: GroupView[];
  items: GroupView[];
  months: GroupView[];
  cards: GroupView[];
  bucketSpending: GroupView[];
  withdrawals: GroupView[];
  transactions: ChargeView[];
  members: string[];
  familyMembers: string[];
}

export const spendingPeriods = [
  { value: "1m", label: "1 month" },
  { value: "3m", label: "3 months" },
  { value: "6m", label: "6 months" },
  { value: "1y", label: "1 year" },
  { value: "all", label: "All time" },
  { value: "custom", label: "Custom range" },
] as const;

export const spendingStatuses = [
  { value: "all", label: "Open and settled" },
  { value: "settled", label: "Settled only" },
  { value: "open", label: "Open only" },
] as const;

/** Today on this machine's calendar, as `yyyy-mm-dd`. */
export function localToday(): string {
  const d = new Date();
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}`;
}

export interface SpendingOptions {
  period: string;
  status: string;
  from: string;
  to: string;
  stats: string[];
  breakdowns: string[];
  limit: number;
}

/** A dashboard widget as stored. */
export interface Widget {
  id: string;
  kind: string;
  title: string;
  span: 1 | 2;
  refs: string[];
  options?: SpendingOptions;
}

export interface WidgetRow {
  id: string;
  name: string;
  note: string;
  kind: string;
  available: string | null;
  value: string;
  target: string | null;
  percent: number | null;
  owes: boolean;
}

export interface WidgetStat {
  key: string;
  label: string;
  money: boolean;
  value: string;
}

export interface WidgetBreakdown {
  key: string;
  label: string;
  frequency: boolean;
  rows: { name: string; amount: string; count: number; share: number }[];
  more: number;
}

export type Figures =
  | { type: "networth"; net: string; assets: string; debts: string; change: string | null; history: number[] }
  | { type: "rows"; rows: WidgetRow[]; total: string | null; missing: number }
  | {
      type: "retirement";
      total: string;
      roth: string;
      traditional: string;
      rothShare: number;
      rothContributions: string;
    }
  | {
      type: "reconciliation";
      daysSince: number | null;
      open: number;
      openBalance: string;
      year: number;
      yearSpending: string;
      yearBuckets: string;
    }
  | { type: "cashflow"; income: string; budgeted: string; left: string; percent: number | null }
  | {
      type: "credit";
      available: string;
      limit: string;
      count: number;
      recorded: number;
      utilisation: number | null;
    }
  | { type: "holdings"; value: string; basis: string; gain: string; percent: number | null; count: number }
  | {
      type: "spending";
      valid: boolean;
      period: string;
      count: number;
      stats: WidgetStat[];
      breakdowns: WidgetBreakdown[];
    }
  | { type: "unknown" };

export interface WidgetView extends Widget {
  label: string;
  page: string;
  figures: Figures;
}

export interface WidgetKind {
  kind: string;
  label: string;
  picks: "" | "buckets" | "accounts" | "goals";
  span: 1 | 2;
  description: string;
}

export interface Choice {
  id: string;
  name: string;
  value: string;
  target: string | null;
  percent: number | null;
  kind: string;
  owes: boolean;
}

export interface DashboardView {
  widgets: WidgetView[];
  isDefault: boolean;
  kinds: WidgetKind[];
  choices: { buckets: Choice[]; accounts: Choice[]; goals: Choice[] };
  suggested: { buckets: string[]; accounts: string[]; goals: string[] };
  defaultLayout: Widget[];
  historyDays: number;
}

export interface SnapshotImport {
  path: string;
  points: number;
  new: number;
  first: string;
  last: string;
  overlap: number;
}

export const spendingStatOptions = [
  { value: "total", label: "Itemized spending" },
  { value: "count", label: "Charges" },
  { value: "open", label: "Open charges" },
  { value: "settled", label: "Settled charges" },
  { value: "withdrawn", label: "Buckets withdrawn" },
  { value: "fromBuckets", label: "Bucket-funded" },
  { value: "everyday", label: "Everyday spending" },
  { value: "average", label: "Average charge" },
  { value: "unattributed", label: "Unattributed" },
] as const;

export const spendingBreakdownOptions = [
  { value: "people", label: "Who spent the most" },
  { value: "items", label: "Most frequent items" },
  { value: "months", label: "Monthly spending" },
  { value: "cards", label: "Spending by card" },
  { value: "bucketSpending", label: "Charges assigned to buckets" },
  { value: "withdrawals", label: "Actual bucket withdrawals" },
] as const;

export const spendingLimits = [3, 5, 10] as const;

/** A widget id in the shape the writer accepts: 32 lowercase hex. */
export function widgetId(): string {
  const bytes = new Uint8Array(16);
  crypto.getRandomValues(bytes);
  return Array.from(bytes, (b) => b.toString(16).padStart(2, "0")).join("");
}

export interface BankMapping {
  date: number | null;
  description: number | null;
  amount: number | null;
  debit: number | null;
  credit: number | null;
  category: number | null;
  kind: number | null;
  chargesNegative: boolean;
  dayFirst: boolean;
}

export interface ImportRow {
  line: number;
  date: string;
  description: string;
  category: string;
  kind: string;
  amount: string;
  charge: boolean;
  problem: string;
  value: string;
  duplicate: boolean;
  afterStatement: boolean;
  suggested: boolean;
}

export interface ImportPreview {
  headers: string[];
  mapping: BankMapping;
  hadHeaders: boolean;
  notes: string[];
  rows: ImportRow[];
  charges: number;
  duplicates: number;
  setAside: number;
}

export interface PaydayView {
  owner: string;
  total: string;
  adjustments: { id: string; delta: string }[];
}

export interface LedgerView {
  accounts: AccountView[];
  buckets: BucketView[];
  income: IncomeView[];
  budget: BudgetView[];
  holdings: HoldingView[];
  retirement: RetirementView[];
  reconciliations: ReconciliationView[];
  earners: EarnerView[];
  goals: GoalView[];
  goalTotals: GoalTotalsView;
  templates: TemplateView[];
  accountOrder: string[];
  paydays: PaydayView[];
  members: string[];
  budgetTypes: string[];
  investmentTypes: string[];
  fixedTypes: { budget: string[]; investment: string[] };
  stale: boolean;
  loadedFrom: string;
}

export interface HistoryView {
  entries: HistoryEntry[];
  problems: string[];
  shared: boolean;
}

export interface HistoryEntry {
  id: string;
  at: string;
  action: string;
  subject: string;
  name: string;
  actor: string;
  client: string;
  install: string;
  version: string;
  via: string;
  amount: string | null;
  changes: string[];
}

/** An edit, shaped the way the writer expects it. */
export type Op =
  | { op: "set"; kind: RecordKind; id: string; record: Record<string, unknown> }
  | { op: "delete"; kind: RecordKind; id: string }
  | { op: "bucket-adjust"; adjustments: { id: string; delta: string }[]; label: string }
  | { op: "bucket-total"; id: string; amount: string }
  | { op: "bucket-move"; fromId: string; toId: string; amount: string }
  | {
      op: "trade";
      id: string;
      side: "buy" | "sell";
      quantity: string;
      price: string;
      notes: string;
    }
  | { op: "retirement-set"; id: string; record: Record<string, unknown> }
  | { op: "retirement-accrue"; id: string }
  | { op: "reconcile-set"; id: string; record: Record<string, unknown> }
  | { op: "reconcile-delete"; id: string }
  | { op: "reconcile-settle"; id: string; cover?: Cover[] }
  | { op: "reconcile-undo"; id: string }
  | { op: "reconcile-import"; id: string; lines: Record<string, unknown>[] }
  | { op: "template-activate"; id: string; keepCurrent: boolean }
  | { op: "type-add"; list: "budget" | "investment"; name: string }
  | { op: "type-delete"; list: "budget" | "investment"; name: string }
  | { op: "dashboard-set"; dashboard: { v: 1; widgets: Widget[] } }
  | { op: "account-order-set"; order: string[] };

export type RecordKind =
  | "account"
  | "income"
  | "budget"
  | "bucket"
  | "holding"
  | "goal"
  | "template";

export interface Applied {
  entry: HistoryEntry;
  queued: number;
  sync: unknown;
}

export interface Series {
  label: string;
  colour: string;
  points: [number, number][];
  fill?: boolean;
}

export interface TradeView {
  kind: string;
  quantity: string;
  price: string;
  total: string;
  at: string;
  notes: string;
}

export interface ProjectionPointView {
  month: number;
  value: string;
}

export interface ProjectionMonthView {
  value: string;
  contributed: string;
  growth: string;
}

export interface ProjectionLineView {
  rate: string;
  points: ProjectionPointView[];
  months: ProjectionMonthView[];
  value: string;
  contributed: string;
  growth: string;
}

export interface ProjectionView {
  years: string;
  months: number;
  targetYear: number | null;
  startMonth: number;
  start: string;
  monthly: string;
  lines: ProjectionLineView[];
}

export interface Chart {
  points: [number, number, number, number][];
  price: number | null;
  dayHigh: number | null;
  dayLow: number | null;
  yearHigh: number | null;
  yearLow: number | null;
  currency: string;
  exchange: string;
  name: string;
  instrument: string;
  fetchedAt: number;
}

export interface Detail {
  ticker: string;
  range: string;
  chart: Chart | null;
  error: string;
  fromCache: boolean;
}

export const chartRanges = [
  { value: "5d", label: "5D" },
  { value: "1mo", label: "1M" },
  { value: "1y", label: "1Y" },
  { value: "5y", label: "5Y" },
] as const;

export interface Refreshed {
  attempted: number;
  priced: number;
  failed: number;
  errors: string[];
}

export const quotes = {
  refresh: () => invoke<Refreshed>("refresh_prices"),
  hasKey: () => invoke<boolean>("has_api_key"),
  saveKey: (key: string) => invoke<boolean>("save_api_key", { key }),
  forgetKey: () => invoke<boolean>("forget_api_key"),
};

export const ledger = {
  read: () => invoke<LedgerView>("ledger"),
  history: () => invoke<HistoryView>("history"),
  /** Null when the edit was valid but had nothing to do. */
  apply: (op: Op) => invoke<Applied | null>("apply", { op }),
  projection: () => invoke<ProjectionView>("projection"),
  /** Both dates as `yyyy-mm-dd`; `from` is today on this machine's calendar. */
  planning: (from: string, to: string) => invoke<PlanningView>("planning", { from, to }),
  /** Periods end today on this machine, and settlements land on its days. */
  spending: (period: string, from: string, to: string, status: string) =>
    invoke<SpendingView>("spending", {
      period,
      from,
      to,
      status,
      today: localToday(),
      offsetMinutes: -new Date().getTimezoneOffset(),
    }),
  /** A bank export read against one statement; `mapping` overrides the guess. */
  transactionsPreview: (id: string, text: string, mapping: BankMapping | null) =>
    invoke<ImportPreview>("transactions_preview", { id, text, mapping }),
  setFamilyMembers: (names: string[]) => invoke<string[]>("set_family_members", { names }),
  dashboard: () =>
    invoke<DashboardView>("dashboard", {
      today: localToday(),
      offsetMinutes: -new Date().getTimezoneOffset(),
    }),
  /** True when today's point was taken; false when there already was one. */
  takeSnapshot: () => invoke<boolean>("take_snapshot", { today: localToday() }),
  pluginSnapshotsPath: () => invoke<string | null>("plugin_snapshots_path"),
  snapshotImportPreview: (path: string) => invoke<SnapshotImport>("snapshot_import_preview", { path }),
  snapshotImport: (path: string) => invoke<number>("snapshot_import", { path }),
  detail: (ticker: string, range: string, force = false) =>
    invoke<Detail>("holding_detail", { ticker, range, force }),
};

export const accountKinds = [
  { value: "checking", label: "Checking" },
  { value: "savings", label: "Savings" },
  { value: "investment", label: "Investment" },
  { value: "property", label: "Property" },
  { value: "vehicle", label: "Vehicle" },
  { value: "retirement-roth", label: "Retirement (Roth)" },
  { value: "retirement-traditional", label: "Retirement (Traditional)" },
  { value: "credit", label: "Credit card" },
  { value: "heloc", label: "HELOC" },
  { value: "loan", label: "Loan" },
  { value: "other", label: "Other" },
] as const;

export const frequencies = [
  { value: "weekly", label: "Weekly" },
  { value: "biweekly", label: "Every 2 weeks" },
  { value: "semimonthly", label: "Twice a month" },
  { value: "monthly", label: "Monthly" },
  { value: "quarterly", label: "Quarterly" },
  { value: "annual", label: "Annually" },
] as const;

export const assetClasses = [
  { value: "", label: "Work it out from the name" },
  { value: "us-large", label: "US large cap" },
  { value: "us-mid", label: "US mid cap" },
  { value: "us-small", label: "US small cap" },
  { value: "international", label: "International" },
  { value: "emerging", label: "Emerging markets" },
  { value: "bonds", label: "Bonds" },
  { value: "real-estate", label: "Real estate" },
  { value: "target-date", label: "Target date" },
  { value: "cash", label: "Cash" },
  { value: "crypto", label: "Crypto" },
  { value: "other", label: "Other" },
] as const;

export function frequencyLabel(value: string): string {
  return frequencies.find((f) => f.value === value)?.label ?? value;
}

export function kindLabel(kind: string): string {
  return accountKinds.find((k) => k.value === kind)?.label ?? kind;
}

/** Amounts arrive already rounded; this only adds the currency and grouping. */
export function money(value: string | null): string {
  if (value === null) return "—";
  const n = Number(value);
  if (!Number.isFinite(n)) return value;
  return n.toLocaleString(undefined, {
    style: "currency",
    currency: "USD",
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  });
}

export function wholeMoney(value: string): string {
  const n = Number(value);
  if (!Number.isFinite(n)) return value;
  const whole = Math.abs(n).toFixed(0).replace(/\B(?=(\d{3})+(?!\d))/g, ",");
  return (n < 0 ? "−$" : "$") + whole;
}

/** A quantity, trimmed of the trailing zeros money keeps. */
export function units(value: string): string {
  const n = Number(value);
  if (!Number.isFinite(n)) return value;
  return n.toLocaleString(undefined, { maximumFractionDigits: 4 });
}

export function when(iso: string): string {
  if (!iso) return "";
  const at = new Date(iso);
  return Number.isNaN(at.getTime()) ? iso : at.toLocaleString();
}
