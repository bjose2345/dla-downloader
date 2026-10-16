import { describe, expect, it } from "vitest";

import { remainingLabel, dateLabel } from "../shared/format";
import { plural, translate, type Translate } from "../i18n/catalogs";
import { copyLabel, entryCopy, fileCopy, groupFolders, isWorkCode, monthLabel, narrowResults, sortFiles, summarizeCopies, summaryText, yearKey } from "./library";
import type { CatalogEntry, DownloadView, FolderFile } from "../backend/types";

const t: Translate = (key, values) => translate("en-US", key, values);
const count = (base: string, value: number) => plural("en-US", base, value);

describe("summaryText", () => {
  const download = (state: DownloadView["state"], received = 0): DownloadView => ({
    id: state + received,
    fileName: "RJ1__a.zip",
    folder: "2013-01",
    workId: "RJ1",
    title: "",
    directory: "C:\\Downloads",
    size: 200,
    received,
    state,
    activity: null,
    bytesPerSecond: 0,
    savedPath: null,
    checked: true,
    verified: false,
    existing: false,
    notice: null,
  });

  it("reports activity, and a new save only once there is nothing else", () => {
    expect(summaryText(t, count, [download("running", 50), download("queued"), download("queued")], false)).toBe(
      "Downloading RJ1 · 25% · 2 waiting",
    );
    expect(summaryText(t, count, [download("failed")], false)).toBe("1 stopped");
    expect(summaryText(t, count, [download("done")], true)).toBe("Download saved");
    expect(summaryText(t, count, [download("done")], false, true)).toBe("Already in your folder");
    expect(summaryText(t, count, [download("done"), download("done")], true, true)).toBe("Download saved");
    expect(summaryText(t, count, [download("done"), download("paused")], false)).toBeNull();
  });
});

describe("groupFolders", () => {
  it("groups months by year, newest first, with undated and other folders last", () => {
    const years = groupFolders([
      { name: "2013-01", files: 2, manifest: true },
      { name: "2024-05", files: 1, manifest: true },
      { name: "0001-01", files: 4, manifest: true },
      { name: "2013-02", files: 3, manifest: true },
      { name: "misc", files: 0, manifest: true },
    ]);
    expect(years.map((year) => year.key)).toEqual(["2024", "2013", "undated", "other"]);
    expect(years[1]?.months.map((month) => month.name)).toEqual(["2013-02", "2013-01"]);
    expect(years[1]?.files).toBe(5);
    expect(monthLabel("2013-02")).toBe("02 · February");
    expect(monthLabel("0001-01")).toBe("0001-01");
    expect(monthLabel("2013-02", "es-ES")).toBe("02 · febrero");
    expect([yearKey("2013-02"), yearKey("0001-01"), yearKey("misc")]).toEqual(["2013", "undated", "other"]);
  });
});

function entry(workId: string): CatalogEntry {
  return { folder: "2013-01", fileName: `${workId}__x.zip`, path: `/dlsite/2013-01/${workId}__x.zip`, workId, title: "", circle: "", version: "", updated: "", released: "", size: 1, sha256: null };
}

describe("search helpers", () => {
  it("narrows a typed work code to that work", () => {
    expect(isWorkCode(" rj108375 ")).toBe(true);
    expect(isWorkCode("summer rain")).toBe(false);
    const results = [entry("RJ108375"), entry("RJ1083750")];
    expect(narrowResults("rj108375", results).map((hit) => hit.workId)).toEqual(["RJ108375"]);
    expect(narrowResults("RJ10837", results)).toHaveLength(2);
    expect(narrowResults("RJ999999", results)).toHaveLength(2);
  });
});

describe("sortFiles", () => {
  const file = (name: string, size: number, modified: string | null): FolderFile => ({ name, path: "/" + name, size, modified, workId: null, title: null, circle: null, version: null, updated: null, released: null, checked: false });
  it("sorts by any column and keeps names as the tie-breaker", () => {
    const files = [file("b", 5, "Mon, 01 Jan 2024 00:00:00 GMT"), file("a", 5, null), file("c", 1, "Tue, 02 Jan 2024 00:00:00 GMT")];
    expect(sortFiles(files, "name", true).map((f) => f.name)).toEqual(["a", "b", "c"]);
    expect(sortFiles(files, "size", false).map((f) => f.name)).toEqual(["b", "a", "c"]);
    expect(sortFiles(files, "modified", false).map((f) => f.name)).toEqual(["c", "b", "a"]);
  });

  it("keeps a work's copies together by name, newest first", () => {
    const copy = (name: string, workId: string, updated: string | null): FolderFile => ({
      ...file(name, 1, null),
      workId,
      updated,
      released: "2026-04-30",
    });
    const files = [copy("RJ2__a.zip", "RJ2", null), copy("RJ1__z.zip", "RJ1", null), copy("RJ1__b.zip", "RJ1", "2026-10-03"), copy("RJ1__c.zip", "RJ1", "2026-05-08")];
    expect(sortFiles(files, "name", true).map((f) => f.name)).toEqual(["RJ1__b.zip", "RJ1__c.zip", "RJ1__z.zip", "RJ2__a.zip"]);
  });
});

describe("a work's copies", () => {
  const copy = (name: string, version: string, updated: string, workId = "RJ01549336") =>
    entryCopy({ ...entry(workId), fileName: name, path: `/dlsite/2026-04/${name}`, version, updated, released: "2026-04-30" });

  it("marks the newest copy, all parts of a split set, and only when a work has several", () => {
    const copies = [
      copy("RJ01549336__a.zip", "", ""),
      copy("RJ01549336__b.zip", "1.044", "2026-05-08"),
      copy("RJ01549336__c.part1.exe", "1.19", "2026-10-03"),
      copy("RJ01549336__c.part2.rar", "1.19", "2026-10-03"),
      copy("RJ00000001__d.zip", "", "", "RJ00000001"),
    ];
    const summary = summarizeCopies(copies);
    expect([...summary.several]).toEqual(["RJ01549336"]);
    expect([...summary.latest].sort()).toEqual(["/dlsite/2026-04/RJ01549336__c.part1.exe", "/dlsite/2026-04/RJ01549336__c.part2.rar"]);
  });

  it("marks none when two copies share the newest date", () => {
    const summary = summarizeCopies([copy("RJ01549336__a.zip", "", ""), copy("RJ01549336__b.zip", "", "")]);
    expect(summary.several.size).toBe(1);
    expect(summary.latest.size).toBe(0);
  });

  it("labels a copy with its version and date", () => {
    expect(copyLabel(t, copy("x", "1.19", "2026-10-03"))).toBe("v1.19 · Updated 2026-10-03");
    expect(copyLabel(t, copy("x", "v2", "2026-10-03"))).toBe("v2 · Updated 2026-10-03");
    expect(copyLabel(t, copy("x", "", ""))).toBe("Released 2026-04-30");
    expect(copyLabel(t, fileCopy({ name: "x", path: "/x", size: 1, modified: null, workId: null, title: null, circle: null, version: null, updated: null, released: null, checked: false }))).toBeNull();
  });
});

describe("download formatting", () => {
  it("estimates the time left", () => {
    expect(remainingLabel(t, 0, 10)).toBeNull();
    expect(remainingLabel(t, 30, 1)).toBe("30s left");
    expect(remainingLabel(t, 7200, 1)).toBe("2 h 0 min left");
    expect(dateLabel("not a date")).toBe("not a date");
    expect(dateLabel(null)).toBe("");
  });
});
