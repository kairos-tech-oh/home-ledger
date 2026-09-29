// The Rust side owns every number. This file only describes what comes back.
import { invoke } from "@tauri-apps/api/core";

export type Cas = "native" | "local-lock" | "best-effort" | "exclusive";
export type StoreKind = "local" | "s3" | "google-drive" | "nas";
export type Role = "primary" | "mirror";

export type Health =
  | { state: "reachable" }
  | { state: "denied"; detail: string }
  | { state: "unreachable"; detail: string };

export interface StoreStatus {
  id: string;
  kind: StoreKind;
  role: Role;
  health: Health;
  capabilities: { cas: Cas; shared: boolean; maxBytes: number };
}

export type SyncState =
  | { state: "synced"; version: string }
  | { state: "behind"; queued: number; reason: string }
  | { state: "blocked"; queued: number; reason: string }
  | { state: "unconfigured" };

export interface Overview {
  assets: string;
  debts: string;
  net: string;
  monthlyIncome: string;
  monthlyBudget: string;
  bucketCash: string;
  accounts: number;
  stale: boolean;
  loadedFrom: string;
}

export const api = {
  overview: () => invoke<Overview>("overview"),
  stores: () => invoke<StoreStatus[]>("stores"),
  syncState: () => invoke<SyncState>("sync_state"),
  flush: () => invoke<SyncState>("flush"),
};

export function money(value: string): string {
  const n = Number(value);
  if (!Number.isFinite(n)) return value;
  return n.toLocaleString(undefined, {
    style: "currency",
    currency: "USD",
    maximumFractionDigits: 2,
  });
}
