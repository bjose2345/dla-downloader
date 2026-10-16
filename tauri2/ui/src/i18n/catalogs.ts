import deDE from "./locales/de-DE/common.json";
import enUS from "./locales/en-US/common.json";
import esES from "./locales/es-ES/common.json";
import frFR from "./locales/fr-FR/common.json";
import itIT from "./locales/it-IT/common.json";
import jaJP from "./locales/ja-JP/common.json";
import ptPT from "./locales/pt-PT/common.json";
import ruRU from "./locales/ru-RU/common.json";

// The same languages, order and catalog layout as dla-launcher.
export const localeIds = ["en-US", "ja-JP", "es-ES", "de-DE", "fr-FR", "it-IT", "pt-PT", "ru-RU"] as const;

export type LocaleId = (typeof localeIds)[number];
export type MessageKey = keyof typeof enUS;
export type Translate = (key: MessageKey, values?: Record<string, string | number>) => string;

type MessageCatalog = Record<MessageKey, string>;

export const messageCatalogs: Record<LocaleId, MessageCatalog> = {
  "en-US": enUS,
  "ja-JP": jaJP,
  "es-ES": esES,
  "de-DE": deDE,
  "fr-FR": frFR,
  "it-IT": itIT,
  "pt-PT": ptPT,
  "ru-RU": ruRU,
};

export function translate(locale: LocaleId, key: MessageKey, values: Record<string, string | number> = {}): string {
  let message = messageCatalogs[locale][key] ?? messageCatalogs["en-US"][key] ?? key;
  for (const [name, value] of Object.entries(values)) {
    message = message.replaceAll(`{${name}}`, String(value));
  }
  return message;
}

/**
 * Counted phrases such as "3 files". Each has one/few/many/other variants in
 * every catalog, chosen with the language's own plural rules (Russian needs
 * all four; Japanese uses only "other").
 */
export function plural(
  locale: LocaleId,
  base: string,
  count: number,
  values: Record<string, string | number> = {},
): string {
  const category = new Intl.PluralRules(locale).select(count);
  const variant = (["one", "few", "many"].includes(category) ? category : "other") as "one" | "few" | "many" | "other";
  return translate(locale, `${base}.${variant}` as MessageKey, { count: count.toLocaleString(locale), ...values });
}
