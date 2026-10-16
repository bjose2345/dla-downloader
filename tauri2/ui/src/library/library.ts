import type { LocaleId, Translate } from "../i18n/catalogs";
import type { CatalogEntry, DownloadView, FolderFile, FolderSummary } from "../backend/types";

export interface YearGroup {
  /** A year, "undated" or "other"; the page translates the last two. */
  key: string;
  months: FolderSummary[];
  files: number;
}

const MONTH = /^(\d{4})-(\d{2})$/;
// dlminer files works without a known release date under 0001-01.
const UNDATED = "0001";

/** Groups `YYYY-MM` folders by year, newest first; anything else is "Other". */
export function groupFolders(folders: FolderSummary[]): YearGroup[] {
  const groups = new Map<string, FolderSummary[]>();
  for (const folder of folders) {
    const key = MONTH.exec(folder.name)?.[1] ?? "other";
    groups.set(key, [...(groups.get(key) ?? []), folder]);
  }
  const rank = (key: string) => (key === "other" ? -2 : key === UNDATED ? -1 : Number(key));
  return [...groups.entries()]
    .sort(([left], [right]) => rank(right) - rank(left))
    .map(([key, months]) => ({
      key: key === UNDATED ? "undated" : key,
      months: [...months].sort((left, right) => right.name.localeCompare(left.name)),
      files: months.reduce((sum, month) => sum + month.files, 0),
    }));
}

/** `2013-02` → `02 · February`, with the month named in the chosen language. */
export function monthLabel(name: string, locale: LocaleId = "en-US"): string {
  const match = MONTH.exec(name);
  if (!match || match[1] === UNDATED) return name;
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, 1));
  return `${match[2]} · ${date.toLocaleString(locale, { month: "long", timeZone: "UTC" })}`;
}

/** The tree's folder key for a month, matching `groupFolders`. */
export function yearKey(name: string): string {
  const year = MONTH.exec(name)?.[1];
  return year === undefined ? "other" : year === UNDATED ? "undated" : year;
}

const WORK_CODE = /^\s*[a-z]{2}\d{6,8}\s*$/i;

export function isWorkCode(query: string): boolean {
  return WORK_CODE.test(query);
}

/** A typed work code narrows the results to that work when it exists. */
export function narrowResults(query: string, results: CatalogEntry[]): CatalogEntry[] {
  if (!isWorkCode(query)) return results;
  const code = query.trim().toUpperCase();
  const exact = results.filter((entry) => entry.workId.toUpperCase() === code);
  return exact.length ? exact : results;
}

/** One copy of a work in the archive, from a month listing or a search. */
export interface Copy {
  path: string;
  /** The stored name, `<code>__<id>.<ext>`; parts of a split set share the id. */
  name: string;
  workId: string | null;
  version: string | null;
  updated: string | null;
  released: string | null;
}

/** The date that orders a work's copies: its update, else the release. */
export function copyDate(copy: Copy): string {
  return copy.updated || copy.released || "";
}

/** Which copy a file belongs to: the parts of one split set share it. */
function copyKey(copy: Copy): string {
  const id = /__([^.]+)\./.exec(copy.name)?.[1];
  return id ?? copy.name.replace(/\.part\d+\.(exe|rar)$/i, "");
}

/** What a list of copies says about each work's versions. */
export interface CopySummary {
  /** Works shown with more than one copy. */
  several: Set<string>;
  /**
   * The paths of each such work's newest copy. A work whose newest date is
   * shared by two copies gets none, because which is newer is unknown.
   */
  latest: Set<string>;
}

export function summarizeCopies(copies: Copy[]): CopySummary {
  const works = new Map<string, Copy[]>();
  for (const copy of copies) {
    if (copy.workId) works.set(copy.workId, [...(works.get(copy.workId) ?? []), copy]);
  }
  const several = new Set<string>();
  const latest = new Set<string>();
  for (const [workId, files] of works) {
    if (new Set(files.map(copyKey)).size < 2) continue;
    several.add(workId);
    const newest = files.reduce((date, copy) => (copyDate(copy) > date ? copyDate(copy) : date), "");
    const top = new Set(files.filter((copy) => newest && copyDate(copy) === newest).map(copyKey));
    if (top.size !== 1) continue;
    for (const copy of files) if (top.has(copyKey(copy))) latest.add(copy.path);
  }
  return { several, latest };
}

/** "v1.19 · Updated 2026-10-03", "Released 2026-04-30", or null. */
export function copyLabel(t: Translate, copy: Copy): string | null {
  const parts: string[] = [];
  if (copy.version) parts.push(/^v/i.test(copy.version) ? copy.version : `v${copy.version}`);
  if (copy.updated) parts.push(t("library.updatedOn", { date: copy.updated }));
  else if (copy.released) parts.push(t("library.releasedOn", { date: copy.released }));
  return parts.length ? parts.join(" · ") : null;
}

export type SortKey = "name" | "size" | "modified" | "title";

export function sortFiles(files: FolderFile[], key: SortKey, ascending: boolean): FolderFile[] {
  // By name, a work's copies stay together, newest first.
  const byName = (left: FolderFile, right: FolderFile) =>
    (left.workId || left.name).localeCompare(right.workId || right.name) ||
    copyDate(fileCopy(right)).localeCompare(copyDate(fileCopy(left))) ||
    left.name.localeCompare(right.name);
  const value = (file: FolderFile): string | number => {
    switch (key) {
      case "size":
        return file.size;
      case "modified":
        return file.modified ? Date.parse(file.modified) || 0 : 0;
      case "title":
        return file.title ?? "";
      default:
        return file.name;
    }
  };
  const sorted = [...files].sort((left, right) => {
    if (key === "name") return byName(left, right);
    const a = value(left);
    const b = value(right);
    const order = typeof a === "number" && typeof b === "number" ? a - b : String(a).localeCompare(String(b));
    return order || left.name.localeCompare(right.name);
  });
  return ascending ? sorted : sorted.reverse();
}

export function fileCopy(file: FolderFile): Copy {
  return { path: file.path, name: file.name, workId: file.workId, version: file.version, updated: file.updated, released: file.released };
}

export function entryCopy(entry: CatalogEntry): Copy {
  return {
    path: entry.path,
    name: entry.fileName,
    workId: entry.workId || null,
    version: entry.version || null,
    updated: entry.updated || null,
    released: entry.released || null,
  };
}

/** One line for the floating downloads bar, or null when there is nothing new to say. */
export function summaryText(
  t: Translate,
  count: (base: string, count: number) => string,
  downloads: DownloadView[],
  newlySaved: boolean,
  /** A copy already in the folder was recognised instead of downloaded. */
  newlyFound = false,
): string | null {
  const running = downloads.find((download) => download.state === "running");
  const waiting = downloads.filter((download) => download.state === "queued").length;
  const stopped = downloads.filter((download) => download.state === "failed").length;
  const parts: string[] = [];
  if (running) {
    const percent = running.size ? Math.floor((running.received / running.size) * 100) : 0;
    parts.push(t("summary.downloading", { name: running.workId || running.fileName, percent }));
  }
  if (waiting) parts.push(count("count.waiting", waiting));
  if (stopped) parts.push(count("count.stopped", stopped));
  if (!parts.length && newlySaved) parts.push(t("summary.saved"));
  else if (!parts.length && newlyFound) parts.push(t("summary.alreadyInFolder"));
  return parts.length ? parts.join(" · ") : null;
}
