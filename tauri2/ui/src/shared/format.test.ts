import { describe, expect, it } from "vitest";

import { connectionText, downloadStatusText, libraryNoticeText, noticeText, signInText } from "./copy";
import { relativeTime, sizeLabel, transferEstimate } from "./format";
import { localeIds, translate, type LocaleId, type Translate } from "../i18n/catalogs";
import type { LibraryNoticeCode, NoticeCode } from "../backend/types";

const inLocale =
  (locale: LocaleId): Translate =>
  (key, values) =>
    translate(locale, key, values);
const t = inLocale("en-US");

const noticeCodes: NoticeCode[] = [
  "sign_in_rejected",
  "too_many_attempts",
  "self_test_account_only",
  "unreachable",
  "certificate",
  "incompatible",
  "invalid_response",
  "server",
];

const libraryCodes: LibraryNoticeCode[] = [
  "sign_in_required",
  "sign_in_rejected",
  "forbidden",
  "not_found",
  "unreachable",
  "certificate",
  "server",
  "invalid_response",
  "local",
  "kept_changing",
  "size_mismatch",
  "checksum_mismatch",
  "already_queued",
  "catalog_incomplete",
];

describe("sizeLabel", () => {
  it("uses binary units with one decimal", () => {
    expect(sizeLabel(0)).toBe("0 B");
    expect(sizeLabel(512)).toBe("512 B");
    expect(sizeLabel(8 * 1024 * 1024)).toBe("8.0 MB");
    expect(sizeLabel(10 * 1024 ** 3 + 123)).toBe("10.0 GB");
  });
});

describe("transferEstimate", () => {
  it("reports rate and remaining minutes only while bytes remain", () => {
    expect(transferEstimate(t, 60 * 1024 * 1024, 600 * 1024 * 1024, 60)).toBe("1.0 MB/s · about 10 min remaining");
    expect(transferEstimate(t, 0, 10, 5)).toBeNull();
    expect(transferEstimate(t, 10, 0, 5)).toBeNull();
  });
});

describe("copy", () => {
  it("phrases every notice code in every language, never showing a raw key", () => {
    for (const locale of localeIds) {
      const tr = inLocale(locale);
      for (const code of noticeCodes) {
        const text = noticeText(tr, { code, detail: "RJ108375.zip" });
        expect(text.length, `${locale} ${code}`).toBeGreaterThan(5);
        expect(text).not.toContain("notice.");
        expect(text).not.toMatch(/\{\w+\}/);
      }
      for (const code of libraryCodes) {
        const text = libraryNoticeText(tr, { code, detail: "3" });
        expect(text).not.toContain("libraryNotice.");
        expect(text).not.toMatch(/\{\w+\}/);
      }
    }
  });

  it("keeps sign-in problems short", () => {
    expect(signInText(t, { code: "sign_in_rejected" })).toBe("Wrong username or password.");
    for (const locale of localeIds) {
      const tr = inLocale(locale);
      for (const code of noticeCodes) {
        const text = signInText(tr, { code, detail: "x" });
        expect(text, `${locale} ${code}`).not.toMatch(/\{\w+\}|signIn\.|notice\./);
      }
    }
    expect(signInText(t, { code: "unreachable", detail: "x" })).toBe(t("signIn.unreachable"));
    expect(signInText(t, { code: "server", status: 500, detail: "boom" })).toBe("Sign-in didn't work. Try again in a moment.");
  });

  it("shows the service's own message except for expired sign-ins", () => {
    expect(noticeText(t, { code: "server", status: 503, detail: "DLA downloads are temporarily unavailable." })).toBe(
      "DLA downloads are temporarily unavailable.",
    );
    expect(noticeText(t, { code: "server", status: 401, detail: "rejected" })).toContain("Sign out");
  });
});

describe("connectionText", () => {
  it("is ready only for the self-test or a verified private connection", () => {
    expect(connectionText(t, { state: "connected" }).tone).toBe("ready");
    expect(connectionText(t, { state: "self_test" }).tone).toBe("ready");
    expect(connectionText(t, { state: "connecting" }).tone).toBe("waiting");
    expect(connectionText(t, null).tone).toBe("waiting");
    expect(connectionText(t, { state: "unreachable", detail: "x" }).tone).toBe("offline");
    expect(connectionText(t, { state: "restarting", seconds: 5 }).text).toContain("5s");
    expect(connectionText(inLocale("es-ES"), { state: "connected" }).text).toBe("Conexión privada lista");
  });
});

describe("library copy", () => {
  it("phrases special library notices", () => {
    expect(libraryNoticeText(t, { code: "server", status: 429 })).toContain("Too many connections");
    expect(libraryNoticeText(t, { code: "already_queued", detail: "1" })).toContain("already in your downloads");
    expect(libraryNoticeText(t, { code: "local", detail: "/x: denied" })).toContain("/x: denied");
  });

  it("describes download states", () => {
    const base = {
      id: "a",
      fileName: "RJ1__a.zip",
      set: null,
      folder: "2013-01",
      workId: "RJ1",
      title: "",
      directory: "/tmp",
      size: 10,
      received: 5,
      bytesPerSecond: 0,
      savedPath: null,
      checked: true,
      verified: false,
      existing: false,
      notice: null,
    } as const;
    expect(downloadStatusText(t, { ...base, state: "done", activity: null, verified: true })).toContain("verified");
    expect(downloadStatusText(t, { ...base, state: "running", activity: { kind: "reconnecting", seconds: 4, attempt: 2, of: 5 } })).toContain("4s");
    expect(downloadStatusText(t, { ...base, state: "running", activity: { kind: "verifying" } })).toContain("Verifying");
    expect(downloadStatusText(inLocale("de-DE"), { ...base, state: "paused", activity: null })).toBe("Pausiert");
    expect(downloadStatusText(t, { ...base, state: "done", activity: null, verified: true, existing: true })).toBe(
      "Already in folder · checksum verified",
    );
    expect(downloadStatusText(t, { ...base, state: "running", activity: { kind: "comparing", checked: 64 << 20 } })).toBe(
      "Checking the copy already in the folder (64.0 MB)",
    );
  });
});

describe("relative times", () => {
  it("says how long ago in the chosen language", () => {
    expect(relativeTime(1000 - 300, 1000, "en-US")).toBe("5 minutes ago");
    expect(relativeTime(1000 - 2 * 3600, 1000, "en-US")).toBe("2 hours ago");
    expect(relativeTime(1000 - 10, 1000, "en-US")).toBe("now");
    expect(relativeTime(1000 - 86400, 1000, "es-ES")).toBe("ayer");
  });
});
