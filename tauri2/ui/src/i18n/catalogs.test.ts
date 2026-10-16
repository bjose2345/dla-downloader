import { describe, expect, it } from "vitest";

import { localeIds, messageCatalogs, plural, translate } from "./catalogs";

const placeholders = (text: string) => [...text.matchAll(/\{(\w+)\}/g)].map((match) => match[1]).sort();

describe("message catalogs", () => {
  const english = messageCatalogs["en-US"];
  const keys = Object.keys(english).sort();

  it("have exactly the English keys and placeholders in every language", () => {
    for (const locale of localeIds) {
      const catalog = messageCatalogs[locale];
      expect(Object.keys(catalog).sort(), locale).toEqual(keys);
      for (const key of keys) {
        const text = catalog[key as keyof typeof english];
        expect(text.trim().length, `${locale} ${key}`).toBeGreaterThan(0);
        expect(placeholders(text), `${locale} ${key}`).toEqual(placeholders(english[key as keyof typeof english]));
      }
    }
  });

  it("fills placeholders", () => {
    expect(translate("en-US", "connection.restarting", { seconds: 5 })).toBe("Reconnecting in 5s");
    expect(translate("fr-FR", "downloads.sizeOf", { received: "1 MB", total: "2 MB" })).toBe("1 MB sur 2 MB");
  });

  it("picks each language's plural form", () => {
    expect(plural("en-US", "count.files", 1)).toBe("1 file");
    expect(plural("en-US", "count.files", 24154)).toBe("24,154 files");
    expect(plural("ru-RU", "count.files", 1)).toBe("1 файл");
    expect(plural("ru-RU", "count.files", 3)).toBe("3 файла");
    expect(plural("ru-RU", "count.files", 5)).toBe("5 файлов");
    expect(plural("ru-RU", "count.files", 21)).toBe("21 файл");
    expect(plural("ja-JP", "count.months", 284)).toBe("284 か月");
    expect(plural("de-DE", "count.files", 24154)).toBe("24.154 Dateien");
  });
});
