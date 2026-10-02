// Encryption of everything stored. The sealing and every key live in Rust;
// this only asks and reports.
import { invoke } from "@tauri-apps/api/core";

export interface EncryptionStatus {
  enabled: boolean;
  unlocked: boolean;
  needsUnlock: boolean;
  keychain: boolean;
}

export interface Enabled {
  recoveryCode: string;
  skipped: string[];
  kept: boolean;
}

export const PASSPHRASE_MIN = 10;

export const encryption = {
  status: () => invoke<EncryptionStatus>("encryption_status"),
  enable: (passphrase: string) => invoke<Enabled>("encryption_enable", { passphrase }),
  /** The passphrase or the recovery code; true when the key was kept on this machine. */
  unlock: (secret: string) => invoke<boolean>("encryption_unlock", { secret }),
  disable: (passphrase: string) => invoke<string[]>("encryption_disable", { passphrase }),
};
