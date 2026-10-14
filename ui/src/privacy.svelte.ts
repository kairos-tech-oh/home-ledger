import { invoke } from "@tauri-apps/api/core";

/**
 * Two ways to show the app without showing the money, kept on this computer
 * only:
 *
 * - Demo mode: every figure comes from a made-up copy of the ledger, and
 *   edits change only that copy. The same seed makes up the same figures.
 * - Blur: the real figures, blurred until the pointer is over them.
 */
type Prefs = { demo: boolean; seed: string; blur: boolean };

const KEY = "home-ledger.privacy";

function stored(): Prefs {
  const fresh = { demo: false, seed: newSeed(), blur: false };
  try {
    const raw = localStorage.getItem(KEY);
    return raw ? { ...fresh, ...JSON.parse(raw) } : fresh;
  } catch {
    return fresh;
  }
}

function keep() {
  try {
    localStorage.setItem(KEY, JSON.stringify(privacy));
  } catch {
    // Not kept past this session; the switches still work.
  }
}

function newSeed(): string {
  return crypto.randomUUID();
}

export const privacy = $state<Prefs>(stored());

/** Whether the backend is showing the made-up copy right now. */
let showing = false;

/** Brings the backend in line with the switch. Safe to call before every load. */
export async function syncDemo(): Promise<void> {
  if (privacy.demo === showing) return;
  await invoke<boolean>("demo_set", { seed: privacy.demo ? privacy.seed : null });
  showing = privacy.demo;
}

export async function setDemo(on: boolean): Promise<void> {
  privacy.demo = on;
  keep();
  await syncDemo();
}

/** New made-up figures, and any edits made in demo mode dropped. */
export async function reshuffle(): Promise<void> {
  privacy.seed = newSeed();
  keep();
  await invoke<boolean>("demo_set", { seed: privacy.seed });
  showing = true;
}

export function setBlur(on: boolean) {
  privacy.blur = on;
  keep();
  blur(on);
}

// ------------------------------------------------------------------ blur

// A dollar figure anywhere in a piece of text: "$1,234.56", "−$80", "$12k".
const AMOUNT = /\$\s?\d/;
let observer: MutationObserver | null = null;

/** The element to blur for a piece of text: a dropdown as a whole, since an
 * option cannot be blurred on its own. */
function holder(text: Node): Element | null {
  const parent = text.parentElement;
  if (!parent) return null;
  return parent.closest("select") ?? parent;
}

function mark(text: Node) {
  const el = holder(text);
  if (!el) return;
  if (AMOUNT.test(text.textContent ?? "")) {
    el.setAttribute("data-amount", "");
  } else if (el.hasAttribute("data-amount") && !AMOUNT.test(el.textContent ?? "")) {
    el.removeAttribute("data-amount");
  }
}

function scan(root: Node) {
  if (root.nodeType === Node.TEXT_NODE) return mark(root);
  const walker = document.createTreeWalker(root, NodeFilter.SHOW_TEXT);
  for (let n = walker.nextNode(); n; n = walker.nextNode()) mark(n);
}

/**
 * Marks every element holding a dollar figure, now and as screens change,
 * and lets the stylesheet blur them. The marks are plain attributes the
 * screens never set, so redrawing a screen does not fight them.
 */
export function blur(on: boolean) {
  const root = document.documentElement;
  if (on) {
    root.setAttribute("data-blur", "");
    if (!observer) {
      observer = new MutationObserver((changes) => {
        for (const c of changes) {
          if (c.type === "characterData") mark(c.target);
          else c.addedNodes.forEach(scan);
        }
      });
      observer.observe(document.body, { subtree: true, childList: true, characterData: true });
    }
    scan(document.body);
  } else {
    root.removeAttribute("data-blur");
    observer?.disconnect();
    observer = null;
  }
}
