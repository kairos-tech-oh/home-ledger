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
}

export interface EarnerView {
  owner: string;
  monthly: string;
  percent: number;
  paychecksPerMonth: number;
  streams: number;
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
  | { op: "reconcile-settle"; id: string }
  | { op: "reconcile-undo"; id: string }
  | { op: "template-activate"; id: string; keepCurrent: boolean }
  | { op: "type-add"; list: "budget" | "investment"; name: string }
  | { op: "type-delete"; list: "budget" | "investment"; name: string };

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
  detail: (ticker: string, range: string, force = false) =>
    invoke<Detail>("holding_detail", { ticker, range, force }),
};

export const accountKinds = [
  { value: "checking", label: "Checking" },
  { value: "savings", label: "Savings" },
  { value: "investment", label: "Investment" },
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
