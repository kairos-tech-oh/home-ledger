// The storage side of the app. Types mirror what Rust sends; nothing here
// decides anything, because Rust owns every rule.
import { invoke } from "@tauri-apps/api/core";

export type StoreKind = "local" | "s3" | "nas" | "google-drive";

export type Settings =
  | { kind: "local"; path: string }
  | { kind: "nas"; path: string }
  | {
      kind: "s3";
      bucket: string;
      key: string;
      region: string;
      endpoint?: string;
      aws_profile?: string;
    }
  | { kind: "google-drive"; file_name: string; client_id: string };

export interface StoreConfig {
  id: string;
  label: string;
  settings: Settings;
  acceptRisk: boolean;
}

/** Only ever sent to Rust. Never comes back, and never reaches the config file. */
export type Secret =
  | { kind: "access-key"; access_key_id: string; secret_access_key: string }
  | { kind: "oauth"; refresh_token: string };

export interface Setup {
  stores: StoreConfig[];
  device: string;
  setupComplete: boolean;
  keychainAvailable: boolean;
  problems: string[];
  opacity: number;
  retirementTargetYear: number | null;
  deviceIsDefault: boolean;
  install: string;
}

export interface PromoteReport {
  was: string;
  fastForwarded: boolean;
  setup: Setup;
}

export const storage = {
  setup: () => invoke<Setup>("setup"),
  test: (store: StoreConfig, secret?: Secret) =>
    invoke<string>("test_store", { store, secret: secret ?? null }),
  save: (store: StoreConfig, secret?: Secret) =>
    invoke<Setup>("save_store", { store, secret: secret ?? null }),
  promote: (id: string, force = false) =>
    invoke<PromoteReport>("promote_store", { id, force }),
  remove: (id: string) => invoke<Setup>("remove_store", { id }),
  renameDevice: (name: string) => invoke<Setup>("rename_device", { name }),
  finishSetup: () => invoke<Setup>("finish_setup"),
  setOpacity: (value: number) => invoke<Setup>("set_opacity", { value }),
  setRetirementTargetYear: (year: number | null) =>
    invoke<Setup>("set_retirement_target_year", { year }),
  connectDrive: (clientId: string, fileName: string, label: string) =>
    invoke<{ label: string; setup: Setup }>("connect_drive", {
      clientId,
      fileName,
      label,
    }),
};

/** What a kind is called on screen, and what it can promise. */
export const kinds: Record<
  StoreKind,
  { name: string; blurb: string; safeAsSourceOfTruth: boolean }
> = {
  local: {
    name: "This computer",
    blurb: "A file on this machine. Nothing leaves it, and there is no version history.",
    safeAsSourceOfTruth: true,
  },
  s3: {
    name: "S3 or compatible",
    blurb:
      "Amazon S3, or anything that speaks its API — MinIO, Backblaze B2, Cloudflare R2, Wasabi.",
    safeAsSourceOfTruth: true,
  },
  "google-drive": {
    name: "Google Drive",
    blurb:
      "A file in your Drive. The app can only see files it creates there. Drive cannot refuse a stale write, so a clash is caught just after rather than prevented — and the old version stays in Drive's history.",
    safeAsSourceOfTruth: true,
  },
  nas: {
    name: "NAS or network share",
    blurb:
      "A file on a mounted share. Network filesystems cannot lock reliably, so this is a good backup and a poor source of truth.",
    safeAsSourceOfTruth: false,
  },
};

export function describe(settings: Settings): string {
  switch (settings.kind) {
    case "local":
    case "nas":
      return settings.path;
    case "s3":
      return `${settings.bucket}/${settings.key}${
        settings.endpoint ? ` at ${settings.endpoint}` : ""
      }${settings.aws_profile ? ` as [${settings.aws_profile}]` : ""}`;
    case "google-drive":
      return settings.file_name;
  }
}

/** Kinds set up by filling in a form. Drive is not one: it signs in instead. */
export type ManualKind = Exclude<StoreKind, "google-drive">;

export const manualKinds = ["local", "s3", "nas"] as const satisfies readonly ManualKind[];

/** Ids are stable and never re-used, so a removed store cannot inherit an old key. */
export function newStoreId(): string {
  return crypto.randomUUID().replace(/-/g, "");
}
