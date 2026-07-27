import { commands, type Theme, type UiShell, type UiTheme } from "@/bindings";

/**
 * Appearance handling: light/dark mode and color palette.
 *
 * Two orthogonal settings drive the CSS tokens (see `styles/theme.css`):
 *  - `theme` (light/dark mode): `system` removes the override so the
 *    `prefers-color-scheme` media query governs; `light`/`dark` set
 *    `data-theme` on the document root, whose higher-specificity CSS
 *    selectors win over the media query.
 *  - `ui_theme` (palette): `abrax` is the brand palette and leaves the root
 *    untouched; other palettes set `data-ui-theme`, which re-points the
 *    color tokens wholesale. Imperial is dark by design, so while it is
 *    active the effective mode is forced to `dark` — the stored `theme`
 *    is preserved and governs again when the palette returns to abrax.
 *
 * Both choices are persisted in `AppSettings` (source of truth) and mirrored
 * to localStorage so they can be applied synchronously on boot, before React
 * mounts, avoiding a flash of the wrong palette.
 */

const THEME_STORAGE_KEY = "abrax.theme";
/** Pre-rebrand storage key, read once as a fallback so nobody gets reset. */
const LEGACY_THEME_STORAGE_KEY = "handy.theme";
const UI_THEME_STORAGE_KEY = "abrax.palette";
/**
 * Window shell (`classic`/`retro`) — the SHAPE axis, orthogonal to
 * mode and palette. Mirrored to localStorage so the boot can set `data-shell`
 * synchronously and paint the transparent backdrop without a flash of the
 * classic layout. The window chrome (frameless/transparent) is decided
 * backend-side at build time; this only drives the CSS/React shell.
 */
const SHELL_STORAGE_KEY = "abrax.shell";

export const THEME_OPTIONS: Theme[] = ["system", "light", "dark"];
export const UI_THEME_OPTIONS: UiTheme[] = ["abrax", "imperial", "escuderia"];
export const UI_SHELL_OPTIONS: UiShell[] = [
  "classic",
  "retro",
  "quiet",
  "bancada",
];

const isTheme = (value: unknown): value is Theme =>
  value === "system" || value === "light" || value === "dark";

const isUiTheme = (value: unknown): value is UiTheme =>
  value === "abrax" || value === "imperial" || value === "escuderia";

const isUiShell = (value: unknown): value is UiShell =>
  value === "classic" ||
  value === "retro" ||
  value === "quiet" ||
  value === "bancada";

/**
 * Apply a mode + palette pair to a document root. Pure DOM: no persistence.
 * Exported so the overlay window (which reads its own copy of the settings)
 * can render with the same palette rules as the main window.
 */
export const applyAppearanceToRoot = (
  theme: Theme,
  uiTheme: UiTheme,
  root: HTMLElement = document.documentElement,
): void => {
  if (uiTheme === "abrax") {
    delete root.dataset.uiTheme;
  } else {
    root.dataset.uiTheme = uiTheme;
  }
  // Dark by design → forzar modo oscuro mientras estén activos:
  //  - Imperial y Escudería (paletas oscuras por diseño).
  //  - Quiet y Bancada (shells de identidad "siempre oscuro": sus componentes
  //    embebidos —historial/ajustes— usan los tokens base, y sin esto seguirían
  //    el tema ambiente (p. ej. claro), pintando tarjetas claras sobre su negro).
  const shell = getStoredShell();
  const forceDark =
    uiTheme === "imperial" ||
    uiTheme === "escuderia" ||
    shell === "quiet" ||
    shell === "bancada";
  const effective: Theme = forceDark ? "dark" : theme;
  if (effective === "system") {
    delete root.dataset.theme;
  } else {
    root.dataset.theme = effective;
  }
};

const persist = (key: string, value: string): void => {
  try {
    localStorage.setItem(key, value);
  } catch {
    // localStorage may be unavailable (e.g. private mode); the setting still
    // persists in AppSettings, so this only costs a one-frame flash on boot.
  }
};

/** Apply a light/dark mode and remember it for the next launch. */
export const applyTheme = (theme: Theme): void => {
  persist(THEME_STORAGE_KEY, theme);
  applyAppearanceToRoot(theme, getStoredUiTheme());
};

/** Apply a palette and remember it for the next launch. */
export const applyUiTheme = (uiTheme: UiTheme): void => {
  persist(UI_THEME_STORAGE_KEY, uiTheme);
  applyAppearanceToRoot(getStoredTheme(), uiTheme);
};

/**
 * Apply the window shell to a document root. Pure DOM: `classic` clears the
 * attribute (the base, decorated layout) and any other shell sets `data-shell`,
 * which the shell CSS keys off to go transparent and mount its own chrome.
 */
const applyShellToRoot = (
  shell: UiShell,
  root: HTMLElement = document.documentElement,
): void => {
  if (shell === "classic") {
    delete root.dataset.shell;
  } else {
    root.dataset.shell = shell;
  }
};

/** Apply a shell and remember it for the next launch. */
export const applyShell = (shell: UiShell): void => {
  persist(SHELL_STORAGE_KEY, shell);
  applyShellToRoot(shell);
};

/** Read the last-applied mode for synchronous boot-time application. */
const getStoredTheme = (): Theme => {
  try {
    const stored = localStorage.getItem(THEME_STORAGE_KEY);
    if (isTheme(stored)) return stored;
    // One-time migration from the pre-rebrand key: read it, adopt it under
    // the new key, and never look back (the old key is left behind, inert).
    const legacy = localStorage.getItem(LEGACY_THEME_STORAGE_KEY);
    if (isTheme(legacy)) {
      persist(THEME_STORAGE_KEY, legacy);
      return legacy;
    }
  } catch {
    // ignore
  }
  return "system";
};

/** Read the last-applied palette for synchronous boot-time application. */
const getStoredUiTheme = (): UiTheme => {
  try {
    const stored = localStorage.getItem(UI_THEME_STORAGE_KEY);
    if (isUiTheme(stored)) return stored;
  } catch {
    // ignore
  }
  return "abrax";
};

/** Read the last-applied shell for synchronous boot-time application. */
const getStoredShell = (): UiShell => {
  try {
    const stored = localStorage.getItem(SHELL_STORAGE_KEY);
    if (isUiShell(stored)) return stored;
  } catch {
    // ignore
  }
  // Debe coincidir con `default_ui_shell()` de Rust: solo se usa en el PRIMER
  // arranque (localStorage vacío), antes de que lleguen los settings. Si
  // divergen, la primera ventana se pinta con el marco del shell equivocado y
  // se corrige al sincronizar — un parpadeo evitable.
  return "quiet";
};

/** Apply the persisted mode + palette + shell from the last launch, synchronously. */
export const applyStoredAppearance = (): void => {
  applyAppearanceToRoot(getStoredTheme(), getStoredUiTheme());
  applyShellToRoot(getStoredShell());
};

/** Apply the persisted appearance from AppSettings (the source of truth). */
export const syncThemeFromSettings = async (): Promise<void> => {
  try {
    const result = await commands.getAppSettings();
    if (result.status === "ok") {
      const theme = result.data.theme ?? "system";
      const uiTheme = result.data.ui_theme ?? "abrax";
      const uiShell = result.data.ui_shell ?? "quiet";
      persist(THEME_STORAGE_KEY, theme);
      persist(UI_THEME_STORAGE_KEY, uiTheme);
      persist(SHELL_STORAGE_KEY, uiShell);
      applyAppearanceToRoot(theme, uiTheme);
      applyShellToRoot(uiShell);
    }
  } catch (e) {
    console.warn("Failed to sync theme from settings:", e);
  }
};
