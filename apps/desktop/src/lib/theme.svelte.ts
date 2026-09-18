/**
 * Which theme the window paints in.
 *
 * "system" is the default and stamps nothing on <html>, so the palette in
 * app.css resolves through `prefers-color-scheme` — the webview follows the
 * desktop's own light/dark setting. "light"/"dark" stamp `data-theme`, which
 * app.css gives precedence over the media query in both directions.
 *
 * The choice is a window preference, not workspace data: it lives in
 * localStorage and never touches the API files on disk.
 */
export type ThemeChoice = "system" | "light" | "dark";

const STORAGE_KEY = "reqchain.theme";

function isChoice(value: unknown): value is ThemeChoice {
  return value === "system" || value === "light" || value === "dark";
}

/** Reads the stored choice. Any storage failure just means "system". */
function readStored(): ThemeChoice {
  try {
    const raw = localStorage.getItem(STORAGE_KEY);
    return isChoice(raw) ? raw : "system";
  } catch {
    // Private mode, disabled site data, a webview without storage — the app
    // still has to render, so fall back rather than throw at module load.
    return "system";
  }
}

export const theme = $state<{ choice: ThemeChoice }>({ choice: readStored() });

/** Applies `choice` to <html> and remembers it. */
export function setTheme(choice: ThemeChoice): void {
  theme.choice = choice;
  applyTheme(choice);
  try {
    localStorage.setItem(STORAGE_KEY, choice);
  } catch {
    // Not being able to remember the choice must not break applying it.
  }
}

/** Stamps (or clears) `data-theme` on the document element. */
export function applyTheme(choice: ThemeChoice): void {
  const root = document.documentElement;
  if (choice === "system") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", choice);
  }
}

/** The order the titlebar control cycles through. */
const ORDER: ThemeChoice[] = ["system", "light", "dark"];

export function nextTheme(choice: ThemeChoice): ThemeChoice {
  return ORDER[(ORDER.indexOf(choice) + 1) % ORDER.length]!;
}

export function themeLabel(choice: ThemeChoice): string {
  switch (choice) {
    case "light":
      return "Light";
    case "dark":
      return "Dark";
    default:
      return "System";
  }
}
