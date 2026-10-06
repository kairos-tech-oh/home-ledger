// Builds `hl` and puts it where Tauri's bundler looks for a program shipped
// beside the app: src-tauri/binaries/hl-<target triple>[.exe]. The bundler
// then installs it next to the app on Windows and in /usr/bin on Linux.
//
//   node tools/release/stage-hl.mjs            # release build, for installers
//   node tools/release/stage-hl.mjs --debug    # debug build, for `tauri dev`
import { execFileSync } from "node:child_process";
import { copyFileSync, mkdirSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const root = join(dirname(fileURLToPath(import.meta.url)), "..", "..");
const debug = process.argv.includes("--debug");

// The triple Tauri names the file by: the host's, unless a target is given.
const target =
  process.env.TAURI_ENV_TARGET_TRIPLE ||
  execFileSync("rustc", ["-vV"], { encoding: "utf8" })
    .split("\n")
    .find((l) => l.startsWith("host:"))
    .slice(5)
    .trim();

const args = ["build", "-p", "hl", ...(debug ? [] : ["--release"])];
if (process.env.TAURI_ENV_TARGET_TRIPLE) args.push("--target", target);
execFileSync("cargo", args, { cwd: root, stdio: "inherit" });

const exe = target.includes("windows") ? ".exe" : "";
const built = join(
  root,
  "target",
  ...(process.env.TAURI_ENV_TARGET_TRIPLE ? [target] : []),
  debug ? "debug" : "release",
  `hl${exe}`,
);
const staged = join(root, "src-tauri", "binaries", `hl-${target}${exe}`);
mkdirSync(dirname(staged), { recursive: true });
copyFileSync(built, staged);
console.log(`staged ${staged}`);
