// Keeping the app current. The check, the download and the signature check
// all happen in Rust; this only asks and reports.
import { invoke } from "@tauri-apps/api/core";

export interface UpdateInfo {
  version: string;
  current: string;
  notes: string;
  date: string;
}

export const updates = {
  /** A newer release, or null when this is the latest. */
  check: () => invoke<UpdateInfo | null>("update_check"),
  /** Installs what the last check offered, then the app restarts. */
  install: () => invoke<void>("update_install"),
  version: () => invoke<string>("app_version"),
};
