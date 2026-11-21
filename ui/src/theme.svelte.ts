// The look, in four parts chosen per device:
//
// - style: the shape of things (Terminal, Finance or Trading), set in CSS;
// - mode: light, dark, or as the device is set;
// - colours: the style's own, or a colour theme kept in the ledger;
// - depth: flat, soft or deep, how far panels stand off the page.
//
// A colour theme is only tokens (see TOKENS) laid over the style as inline
// custom properties, so a theme made for one style is worn by every style and
// a token it leaves out keeps the style's own colour.
import type { ColorTheme, Palette } from "./ledger";

export type Style = "terminal" | "finance" | "trading";
export type Mode = "light" | "dark" | "system";
export type Depth = "flat" | "soft" | "deep";

export const STYLES: [Style, string][] = [
  ["terminal", "Terminal"],
  ["finance", "Finance"],
  ["trading", "Trading"],
];
export const MODES: [Mode, string][] = [
  ["light", "Light"],
  ["dark", "Dark"],
  ["system", "Device"],
];
export const DEPTHS: [Depth, string][] = [
  ["flat", "Flat"],
  ["soft", "Soft"],
  ["deep", "Deep"],
];

/** How a token reaches CSS: straight, or as the `r, g, b` triplet some rules mix with alpha. */
type Kind = "color" | "triplet";
export interface Token {
  key: string;
  label: string;
  group: string;
  css: string;
  kind: Kind;
}

// The editable tokens, in the order the editor lists them. Their names are
// the theme file's format: rename one and every saved theme loses it.
export const TOKENS: Token[] = [
  { key: "ground", label: "Page", group: "Surfaces", css: "--ground", kind: "triplet" },
  { key: "surface", label: "Panel", group: "Surfaces", css: "--surface", kind: "color" },
  { key: "raised", label: "Raised", group: "Surfaces", css: "--raised", kind: "color" },
  { key: "raisedStrong", label: "Raised, hovered", group: "Surfaces", css: "--raised-strong", kind: "color" },
  { key: "menu", label: "Menus and pop-ups", group: "Surfaces", css: "--menu", kind: "color" },
  { key: "hairline", label: "Lines", group: "Surfaces", css: "--hairline", kind: "color" },
  { key: "text", label: "Text", group: "Text", css: "--text", kind: "color" },
  { key: "dim", label: "Secondary", group: "Text", css: "--dim", kind: "color" },
  { key: "faint", label: "Faint", group: "Text", css: "--faint", kind: "color" },
  { key: "accent", label: "Accent", group: "Accent", css: "--accent", kind: "color" },
  { key: "accentStrong", label: "Accent, hovered", group: "Accent", css: "--accent-strong", kind: "color" },
  { key: "onAccent", label: "Text on accent", group: "Accent", css: "--on-accent", kind: "color" },
  { key: "positive", label: "Held, gains", group: "Meaning", css: "--positive", kind: "color" },
  { key: "negative", label: "Owed, losses", group: "Meaning", css: "--negative", kind: "color" },
  { key: "warn", label: "Warnings", group: "Meaning", css: "--warn", kind: "color" },
  { key: "pillBlue", label: "Blue", group: "Pills", css: "--pill-blue", kind: "color" },
  { key: "pillViolet", label: "Violet", group: "Pills", css: "--pill-violet", kind: "color" },
  { key: "pillGreen", label: "Green", group: "Pills", css: "--pill-green", kind: "color" },
  { key: "pillAmber", label: "Amber", group: "Pills", css: "--pill-amber", kind: "color" },
  { key: "pillRed", label: "Red", group: "Pills", css: "--pill-red", kind: "color" },
  { key: "shadow", label: "Shadow", group: "Depth", css: "--shadow-rgb", kind: "triplet" },
  { key: "highlight", label: "Top-edge light", group: "Depth", css: "--edge-rgb", kind: "triplet" },
];

interface Choice {
  style: Style;
  mode: Mode;
  depth: Depth;
  /** A colour theme's id, or empty for the style's own colours. */
  colors: string;
}

const KEY = "home-ledger.look";
/** The theme being worn, kept here too so the first paint wears it before the ledger is read. */
const WORN_KEY = "home-ledger.look-theme";
const OLD_KEY = "home-ledger.theme";
const DEFAULT: Choice = { style: "terminal", mode: "dark", depth: "soft", colors: "" };

const oneOf = <T extends string>(value: unknown, list: [T, string][], fallback: T): T =>
  list.some(([v]) => v === value) ? (value as T) : fallback;

function stored(): Choice {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) {
      const c = JSON.parse(raw);
      return {
        style: oneOf(c.style, STYLES, DEFAULT.style),
        mode: oneOf(c.mode, MODES, DEFAULT.mode),
        depth: oneOf(c.depth, DEPTHS, DEFAULT.depth),
        colors: typeof c.colors === "string" ? c.colors : "",
      };
    }
    // The single choice before the look had parts: "terminal", "finance",
    // or a style and a mode such as "trading-system".
    const old = localStorage.getItem(OLD_KEY);
    if (old) {
      const [style, mode] = old.split("-");
      return {
        ...DEFAULT,
        style: oneOf(style, STYLES, DEFAULT.style),
        mode: oneOf(mode, MODES, style === "finance" ? "light" : "dark"),
      };
    }
  } catch {
    // Unreadable or refused storage: the default look.
  }
  return { ...DEFAULT };
}

function storedTheme(): ColorTheme[] {
  try {
    const t = JSON.parse(localStorage.getItem(WORN_KEY) ?? "null");
    return t && typeof t.id === "string" && t.light && t.dark ? [t] : [];
  } catch {
    return [];
  }
}

export const look = $state({
  ...stored(),
  /** Themes kept in the ledger, as last read; until then, the one this device wore. */
  custom: storedTheme(),
  /** A theme being edited, worn in place of the chosen colours until it closes. */
  preview: null as ColorTheme | null,
  /** Whether the look is dark right now, whichever way the mode says. */
  dark: true,
});

function save() {
  const { style, mode, depth, colors } = look;
  const worn = look.custom.find((t) => t.id === colors);
  try {
    localStorage.setItem(KEY, JSON.stringify({ style, mode, depth, colors }));
    if (worn) localStorage.setItem(WORN_KEY, JSON.stringify(worn));
    else localStorage.removeItem(WORN_KEY);
  } catch {
    // Private windows refuse storage; the choice then lasts the session.
  }
}

const prefersDark =
  typeof window !== "undefined" ? window.matchMedia("(prefers-color-scheme: dark)") : null;

// ------------------------------------------------------------- colours

const HEX = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i;

/** `#rgb`, `#rrggbb` or `#rrggbbaa` as numbers, alpha 0–1. */
export function parseHex(hex: string): [number, number, number, number] | null {
  if (!HEX.test(hex)) return null;
  let h = hex.slice(1);
  if (h.length === 3) h = [...h].map((c) => c + c).join("");
  const n = (i: number) => parseInt(h.slice(i, i + 2), 16);
  return [n(0), n(2), n(4), h.length === 8 ? n(6) / 255 : 1];
}

const two = (n: number) =>
  Math.max(0, Math.min(255, Math.round(n)))
    .toString(16)
    .padStart(2, "0");

export function toHex(r: number, g: number, b: number, a = 1): string {
  return `#${two(r)}${two(g)}${two(b)}${a < 1 ? two(a * 255) : ""}`;
}

/** The six-digit part of a colour, for a colour picker that knows no alpha. */
export function opaque(hex: string): string {
  const c = parseHex(hex);
  return c ? toHex(c[0], c[1], c[2]) : "#000000";
}

const triplet = (hex: string) => {
  const c = parseHex(hex);
  return c ? `${c[0]}, ${c[1]}, ${c[2]}` : null;
};

const chevron = (hex: string) =>
  `url("data:image/svg+xml,%3Csvg xmlns='http://www.w3.org/2000/svg' width='10' height='6' viewBox='0 0 10 6'%3E%3Cpath d='M1 1l4 4 4-4' fill='none' stroke='%23${opaque(hex).slice(1)}' stroke-width='1.5'/%3E%3C/svg%3E")`;

/** Every property a palette sets, its tokens and what follows from them. */
function properties(p: Palette): [string, string][] {
  const out: [string, string][] = [];
  for (const t of TOKENS) {
    const value = p[t.key];
    if (!value || !HEX.test(value)) continue;
    if (t.kind === "triplet") {
      const rgb = triplet(value);
      if (rgb) out.push([t.css, rgb]);
    } else {
      out.push([t.css, value]);
    }
  }
  const mix = (a: string, b: string, pct: number) => `color-mix(in srgb, ${a} ${pct}%, ${b})`;
  if (p.text) {
    const rgb = triplet(p.text);
    if (rgb) out.push(["--ink", rgb]);
  }
  if (p.menu) out.push(["--control", "var(--menu)"]);
  if (p.surface || p.text) out.push(["--hover", mix("var(--text)", "var(--surface)", 5)]);
  if (p.accent) {
    out.push(["--tint-accent", mix("var(--accent)", "transparent", 16)]);
    out.push(["--hover-accent", mix("var(--accent)", "var(--surface)", 8)]);
    out.push(["--hover-accent-edge", mix("var(--accent)", "var(--hairline)", 35)]);
    if (!p.accentStrong) out.push(["--accent-strong", mix("var(--text)", "var(--accent)", 18)]);
  }
  if (p.positive) out.push(["--tint-positive", mix("var(--positive)", "transparent", 16)]);
  if (p.negative) out.push(["--tint-negative", mix("var(--negative)", "transparent", 16)]);
  if (p.warn) {
    out.push(["--warn-tint", mix("var(--warn)", "transparent", 12)]);
    out.push(["--warn-edge", mix("var(--warn)", "transparent", 40)]);
  }
  if (p.dim) out.push(["--chevron", chevron(p.dim)]);
  return out;
}

let worn: string[] = [];

function clearColors(root: HTMLElement) {
  for (const name of worn) root.style.removeProperty(name);
  worn = [];
}

function wearColors(root: HTMLElement, palette: Palette) {
  clearColors(root);
  for (const [name, value] of properties(palette)) {
    root.style.setProperty(name, value);
    worn.push(name);
  }
}

// --------------------------------------------------------------- apply

export function applyTheme() {
  const root = document.documentElement;
  const dark = look.mode === "dark" || (look.mode === "system" && !!prefersDark?.matches);
  look.dark = dark;
  root.dataset.theme = look.style;
  root.dataset.mode = dark ? "dark" : "light";
  root.dataset.depth = look.depth;
  const theme = look.preview ?? look.custom.find((t) => t.id === look.colors) ?? null;
  if (theme) wearColors(root, dark ? theme.dark : theme.light);
  else clearColors(root);
}

// A device that switches between light and dark by itself is followed as it does.
prefersDark?.addEventListener("change", () => {
  if (look.mode === "system") applyTheme();
});

function choose(change: Partial<Choice>) {
  Object.assign(look, change);
  save();
  applyTheme();
}

export const setStyle = (style: Style) => choose({ style });
export const setMode = (mode: Mode) => choose({ mode });
export const setDepth = (depth: Depth) => choose({ depth });
export const setColors = (colors: string) => choose({ colors });
/** The one-click switch: whichever of light and dark is not showing now. */
export const toggleDark = () => choose({ mode: look.dark ? "light" : "dark" });

/** The ledger's themes, after a read or a save. */
export function setCustomThemes(themes: ColorTheme[]) {
  look.custom = themes;
  save();
  applyTheme();
}

/** Wear a theme being edited, or stop: null goes back to the chosen colours. */
export function previewTheme(theme: ColorTheme | null) {
  look.preview = theme;
  applyTheme();
}

/** Show the preview in light or dark while editing, without storing the choice. */
export function previewMode(dark: boolean) {
  const root = document.documentElement;
  look.dark = dark;
  root.dataset.mode = dark ? "dark" : "light";
  if (look.preview) wearColors(root, dark ? look.preview.dark : look.preview.light);
}

// ---------------------------------------------------- the style's own

/** A colour the browser computed, as hex; alpha is kept. */
function resolve(probe: HTMLElement, value: string): string | null {
  const v = value.trim();
  if (!v) return null;
  probe.style.color = "";
  probe.style.color = /^\d/.test(v) ? `rgb(${v})` : v;
  const c = getComputedStyle(probe).color;
  let m = c.match(/^rgba?\(([\d.]+),\s*([\d.]+),\s*([\d.]+)(?:,\s*([\d.]+))?\)$/);
  if (m) return toHex(+m[1], +m[2], +m[3], m[4] === undefined ? 1 : +m[4]);
  m = c.match(/^color\(srgb ([\d.]+) ([\d.]+) ([\d.]+)(?: \/ ([\d.]+))?\)$/);
  if (m) return toHex(+m[1] * 255, +m[2] * 255, +m[3] * 255, m[4] === undefined ? 1 : +m[4]);
  return null;
}

/**
 * The current style's own colours in one mode, as a palette: where a new
 * theme starts, and what an editor shows for a token the theme leaves out.
 * Read from the stylesheets themselves, so they never drift from the CSS.
 */
export function styleColors(dark: boolean): Palette {
  const root = document.documentElement;
  const keptMode = root.dataset.mode;
  const kept = worn.map((name) => [name, root.style.getPropertyValue(name)] as const);
  clearColors(root);
  root.dataset.mode = dark ? "dark" : "light";
  const probe = document.createElement("span");
  probe.style.display = "none";
  document.body.appendChild(probe);
  const css = getComputedStyle(root);
  const out: Palette = {};
  for (const t of TOKENS) {
    const hex = resolve(probe, css.getPropertyValue(t.css));
    if (hex) out[t.key] = hex;
  }
  probe.remove();
  root.dataset.mode = keptMode;
  for (const [name, value] of kept) {
    root.style.setProperty(name, value);
    worn.push(name);
  }
  return out;
}

// ---------------------------------------------------------------- files

/** A theme as a file to share: no id, so whoever opens it gets their own copy. */
export function themeFile(theme: ColorTheme): string {
  return JSON.stringify(
    { homeLedgerTheme: 1, name: theme.name, light: theme.light, dark: theme.dark },
    null,
    2,
  );
}

/** A shared theme file, checked the way the ledger will check it. Throws a sentence. */
export function readThemeFile(text: string): ColorTheme {
  let raw: unknown;
  try {
    raw = JSON.parse(text);
  } catch {
    throw new Error("That is not a theme file: it is not JSON.");
  }
  if (!raw || typeof raw !== "object") throw new Error("That is not a theme file.");
  const r = raw as Record<string, unknown>;
  const palette = (value: unknown): Palette => {
    const out: Palette = {};
    if (value && typeof value === "object") {
      for (const [k, v] of Object.entries(value as Record<string, unknown>)) {
        if (/^[a-z][a-zA-Z0-9]{0,31}$/.test(k) && typeof v === "string" && HEX.test(v.trim())) {
          out[k] = v.trim().toLowerCase();
        }
      }
    }
    return out;
  };
  const name = typeof r.name === "string" ? r.name.trim().slice(0, 40) : "";
  const light = palette(r.light);
  const dark = palette(r.dark);
  if (!name) throw new Error("That theme file has no name.");
  if (!Object.keys(light).length && !Object.keys(dark).length) {
    throw new Error("That theme file has no colours in it.");
  }
  return { id: "", name, light, dark };
}
