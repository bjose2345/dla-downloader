import { localeIds, type LocaleId } from "../i18n/catalogs";

export const themeChoices = ["light", "dark", "system"] as const;
export type ThemeChoice = (typeof themeChoices)[number];

export interface LocaleDefinition {
  id: LocaleId;
  label: string;
}

export const locales: readonly LocaleDefinition[] = [
  { id: "en-US", label: "English" },
  { id: "ja-JP", label: "日本語" },
  { id: "es-ES", label: "Español" },
  { id: "de-DE", label: "Deutsch" },
  { id: "fr-FR", label: "Français" },
  { id: "it-IT", label: "Italiano" },
  { id: "pt-PT", label: "Português" },
  { id: "ru-RU", label: "Русский" },
];

/** Interface-only choices, kept in this window's storage on this computer. */
export interface Preferences {
  theme: ThemeChoice;
  locale: LocaleId;
  reduceMotion: boolean;
}

const STORAGE_KEY = "dla-downloader:presentation:v1";

/** The system language when it is one of ours, otherwise English. */
export function systemLocale(languages: readonly string[] = typeof navigator === "undefined" ? [] : navigator.languages): LocaleId {
  for (const language of languages) {
    const exact = localeIds.find((id) => id.toLowerCase() === language.toLowerCase());
    if (exact) return exact;
    const primary = language.split("-")[0]?.toLowerCase();
    const sameLanguage = localeIds.find((id) => id.split("-")[0] === primary);
    if (sameLanguage) return sameLanguage;
  }
  return "en-US";
}

export function defaultPreferences(): Preferences {
  return { theme: "system", locale: systemLocale(), reduceMotion: false };
}

export function readPreferences(storage: Pick<Storage, "getItem"> | null = safeStorage()): Preferences {
  const defaults = defaultPreferences();
  try {
    const stored = JSON.parse(storage?.getItem(STORAGE_KEY) ?? "{}") as Record<string, unknown>;
    return {
      theme: themeChoices.find((theme) => theme === stored.theme) ?? defaults.theme,
      locale: localeIds.find((locale) => locale === stored.locale) ?? defaults.locale,
      reduceMotion: typeof stored.reduceMotion === "boolean" ? stored.reduceMotion : defaults.reduceMotion,
    };
  } catch {
    return defaults;
  }
}

export function writePreferences(preferences: Preferences): void {
  try {
    safeStorage()?.setItem(STORAGE_KEY, JSON.stringify(preferences));
  } catch {
    // Still applied for this session.
  }
}

export function systemPrefersDark(): boolean {
  return typeof window !== "undefined" && window.matchMedia("(prefers-color-scheme: dark)").matches;
}

export function resolveTheme(choice: ThemeChoice, prefersDark: boolean): "light" | "dark" {
  return choice === "system" ? (prefersDark ? "dark" : "light") : choice;
}

export function applyPreferences(preferences: Preferences, prefersDark = systemPrefersDark()): void {
  document.documentElement.dataset.theme = resolveTheme(preferences.theme, prefersDark);
  document.documentElement.lang = preferences.locale;
}

function safeStorage(): Storage | null {
  try {
    return typeof window === "undefined" ? null : window.localStorage;
  } catch {
    return null;
  }
}
