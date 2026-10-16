import type { Translate } from "../i18n/catalogs";

const UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

export function sizeLabel(bytes: number): string {
  if (bytes <= 0) return "0 B";
  const index = Math.max(0, Math.min(Math.floor(Math.log(bytes) / Math.log(1024)), UNITS.length - 1));
  return (bytes / 1024 ** index).toFixed(index ? 1 : 0) + " " + UNITS[index];
}

export function transferEstimate(t: Translate, sentBytes: number, remainingBytes: number, elapsedSeconds: number): string | null {
  if (sentBytes <= 0 || elapsedSeconds <= 0 || remainingBytes <= 0) return null;
  const rate = sentBytes / elapsedSeconds;
  const minutes = Math.max(1, Math.ceil(remainingBytes / rate / 60));
  return t("library.rate", { rate: sizeLabel(rate), minutes });
}

export function remainingLabel(t: Translate, remainingBytes: number, bytesPerSecond: number): string | null {
  if (bytesPerSecond <= 0 || remainingBytes <= 0) return null;
  const seconds = Math.ceil(remainingBytes / bytesPerSecond);
  if (seconds < 60) return t("time.seconds", { seconds });
  const minutes = Math.ceil(seconds / 60);
  if (minutes < 60) return t("time.minutes", { minutes });
  return t("time.hours", { hours: Math.floor(minutes / 60), minutes: minutes % 60 });
}

/** WebDAV dates are RFC 1123 (`Mon, 29 Sep 2026 10:00:00 GMT`). */
export function dateLabel(value: string | null): string {
  if (!value) return "";
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  const pad = (part: number) => String(part).padStart(2, "0");
  return `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())} ${pad(date.getHours())}:${pad(date.getMinutes())}`;
}

/** "5 minutes ago", "yesterday"… in the interface language. */
export function relativeTime(seconds: number, now: number, locale: string): string {
  const format = new Intl.RelativeTimeFormat(locale, { numeric: "auto" });
  const elapsed = Math.round(seconds - now);
  const steps: [Intl.RelativeTimeFormatUnit, number][] = [
    ["day", 86400],
    ["hour", 3600],
    ["minute", 60],
  ];
  for (const [unit, size] of steps) {
    if (Math.abs(elapsed) >= size) return format.format(Math.round(elapsed / size), unit);
  }
  return format.format(0, "second");
}
