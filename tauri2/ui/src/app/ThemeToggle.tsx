import { usePresentation } from "../preferences/PresentationProvider";

/** A quick light/dark switch; Settings also offers "follow the system". */
export function ThemeToggle() {
  const { resolvedTheme, setTheme, t } = usePresentation();
  const dark = resolvedTheme === "dark";
  const label = dark ? t("theme.toLight") : t("theme.toDark");
  return (
    <button
      className="theme-toggle"
      type="button"
      aria-label={label}
      aria-pressed={dark}
      title={label}
      onClick={() => setTheme(dark ? "light" : "dark")}
    >
      <svg className="sun" viewBox="0 0 24 24" aria-hidden="true">
        <circle cx="12" cy="12" r="4" />
        <path d="M12 2v2m0 16v2M2 12h2m16 0h2M5 5l1.5 1.5m11 11L19 19M5 19l1.5-1.5m11-11L19 5" />
      </svg>
      <svg className="moon" viewBox="0 0 24 24" aria-hidden="true">
        <path d="M20.5 14a8.5 8.5 0 0 1-10.5-10.5A8.5 8.5 0 1 0 20.5 14Z" />
      </svg>
    </button>
  );
}
