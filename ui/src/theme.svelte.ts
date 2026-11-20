// The look, chosen per device: the terminal style the Omarchy plugin shares, or Finance.
export type Theme = "terminal" | "finance";

const KEY = "home-ledger.theme";

function stored(): Theme {
  try {
    return localStorage.getItem(KEY) === "finance" ? "finance" : "terminal";
  } catch {
    return "terminal";
  }
}

export const look = $state({ theme: stored() });

export function applyTheme() {
  document.documentElement.dataset.theme = look.theme;
}

export function setTheme(theme: Theme) {
  look.theme = theme;
  try {
    localStorage.setItem(KEY, theme);
  } catch {
    // Private windows refuse storage; the choice then lasts the session.
  }
  applyTheme();
}
