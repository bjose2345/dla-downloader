import { describe, expect, it } from "vitest";

import { appUpdateStatus } from "./AppUpdate";
import { translate, type Translate } from "../i18n/catalogs";
import type { AppUpdateView } from "../backend/types";

const t: Translate = (key, values) => translate("en-US", key, values);
const now = 10_000;

const idle: AppUpdateView = {
  revision: 1,
  enabled: true,
  installable: true,
  current: "0.3.0",
  signedIn: true,
  checking: false,
  lastChecked: null,
  latest: null,
  notes: null,
  unavailable: false,
  required: false,
  downloading: null,
  ready: false,
  installing: false,
  problem: null,
};

const status = (change: Partial<AppUpdateView>) => appUpdateStatus(t, { ...idle, ...change }, now, "en-US");

describe("the app update line", () => {
  it("asks for a sign-in before the first check", () => {
    expect(status({ signedIn: false })).toEqual({ tone: "idle", text: "Sign in to check for a new version." });
  });

  it("says up to date and when it last looked", () => {
    expect(status({ lastChecked: now - 300 })).toEqual({ tone: "ok", text: "DLA Downloader is up to date · checked 5 minutes ago" });
    expect(status({ checking: true, lastChecked: now - 300 }).text).toBe("Checking for a new version…");
  });

  it("follows a new version from found to installing", () => {
    expect(status({ latest: "0.4.0" }).text).toBe("Version 0.4.0 is available.");
    expect(status({ latest: "0.4.0", signedIn: false }).text).toBe("Version 0.4.0 is available. Sign in to download it.");
    expect(status({ latest: "0.4.0", downloading: { received: 3, total: 4 } }).text).toBe("Downloading version 0.4.0… 75%");
    expect(status({ latest: "0.4.0", ready: true })).toEqual({ tone: "new", text: "Version 0.4.0 is ready to install." });
    expect(status({ latest: "0.4.0", ready: true, installing: true }).text).toBe("Starting the update…");
    expect(status({ latest: "0.4.0", unavailable: true }).text).toBe("Version 0.4.0 is out, but not for this system yet.");
  });

  it("puts problems first", () => {
    const busy = status({ latest: "0.4.0", ready: true, problem: { code: "busy" } });
    expect(busy).toEqual({ tone: "problem", text: "Finish or pause your downloads first, then restart." });
    expect(status({ problem: { code: "invalid", detail: "bad version" } }).text).toBe("The release list couldn't be read: bad version");
  });

  it("speaks the chosen language", () => {
    const ja: Translate = (key, values) => translate("ja-JP", key, values);
    expect(appUpdateStatus(ja, { ...idle, latest: "0.4.0", ready: true }, now, "ja-JP").text).toBe("バージョン 0.4.0 をインストールする準備ができました。");
  });
});
