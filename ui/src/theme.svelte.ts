// The look, chosen per device: the terminal style the Omarchy plugin shares, or Finance or
// Trading, each light, dark, or following the device's own setting.
type Mode = "light" | "dark" | "system";
export type Theme = "terminal" | `finance-${Mode}` | `trading-${Mode}`;

const KEY = "home-ledger.theme";
const CHOICES: Theme[] = [
  "terminal",
  "finance-light",
  "finance-dark",
  "finance-system",
  "trading-dark",
  "trading-light",
  "trading-system",
];

function stored(): Theme {
  try {
    const value = localStorage.getItem(KEY);
    // "finance" was the choice before Finance had a dark mode.
    if (value === "finance") return "finance-light";
    return CHOICES.includes(value as Theme) ? (value as Theme) : "terminal";
  } catch {
    return "terminal";
  }
}

export const look = $state({ theme: stored() });

const prefersDark =
  typeof window !== "undefined" ? window.matchMedia("(prefers-color-scheme: dark)") : null;

export function applyTheme() {
  const root = document.documentElement;
  if (look.theme === "terminal") {
    root.dataset.theme = "terminal";
    delete root.dataset.mode;
    return;
  }
  const [family, mode] = look.theme.split("-");
  root.dataset.theme = family;
  const dark = mode === "dark" || (mode === "system" && !!prefersDark?.matches);
  root.dataset.mode = dark ? "dark" : "light";
}

// A device that switches between light and dark by itself is followed as it does.
prefersDark?.addEventListener("change", () => {
  if (look.theme.endsWith("-system")) applyTheme();
});

export function setTheme(theme: Theme) {
  look.theme = theme;
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // Private windows refuse storage; the choice then lasts the session.
  }
  applyTheme();
}
