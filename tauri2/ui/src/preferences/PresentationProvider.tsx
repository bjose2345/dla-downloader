import { MotionConfig } from "motion/react";
import { createContext, useContext, useEffect, useMemo, useState, type ReactNode } from "react";

import { plural, translate, type LocaleId, type Translate } from "../i18n/catalogs";
import {
  applyPreferences,
  readPreferences,
  resolveTheme,
  systemPrefersDark,
  writePreferences,
  type Preferences,
  type ThemeChoice,
} from "./preferences";

interface Presentation extends Preferences {
  resolvedTheme: "light" | "dark";
  setTheme: (theme: ThemeChoice) => void;
  setLocale: (locale: LocaleId) => void;
  setReduceMotion: (reduceMotion: boolean) => void;
  t: Translate;
  count: (base: string, count: number, values?: Record<string, string | number>) => string;
}

const PresentationContext = createContext<Presentation | null>(null);

export function PresentationProvider({ children }: { children: ReactNode }) {
  const [preferences, setPreferences] = useState(readPreferences);
  const [prefersDark, setPrefersDark] = useState(systemPrefersDark);

  useEffect(() => {
    const media = window.matchMedia("(prefers-color-scheme: dark)");
    const follow = (event: MediaQueryListEvent) => setPrefersDark(event.matches);
    media.addEventListener("change", follow);
    return () => media.removeEventListener("change", follow);
  }, []);

  useEffect(() => {
    applyPreferences(preferences, prefersDark);
    writePreferences(preferences);
  }, [preferences, prefersDark]);

  const value = useMemo<Presentation>(
    () => ({
      ...preferences,
      resolvedTheme: resolveTheme(preferences.theme, prefersDark),
      setTheme: (theme) => setPreferences((current) => ({ ...current, theme })),
      setLocale: (locale) => setPreferences((current) => ({ ...current, locale })),
      setReduceMotion: (reduceMotion) => setPreferences((current) => ({ ...current, reduceMotion })),
      t: (key, values) => translate(preferences.locale, key, values),
      count: (base, count, values) => plural(preferences.locale, base, count, values),
    }),
    [preferences, prefersDark],
  );

  return (
    <PresentationContext.Provider value={value}>
      {/* "user" follows the system setting; the switch forces calm motion. */}
      <MotionConfig reducedMotion={preferences.reduceMotion ? "always" : "user"}>{children}</MotionConfig>
    </PresentationContext.Provider>
  );
}

export function usePresentation(): Presentation {
  const context = useContext(PresentationContext);
  if (!context) throw new Error("usePresentation must be used within PresentationProvider");
  return context;
}
