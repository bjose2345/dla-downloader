import type { LocaleId, Translate } from "../i18n/catalogs";
import type { CatalogEntry, DownloadState, DownloadView, FolderFile, FolderSummary, RecentFile } from "../backend/types";

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

/**
 * The set a split archive's part belongs to and the part's number:
 * `RJ1__a.part2.rar` is part 2 of `RJ1__a`, `RJ1__a.zip.003` part 3.
 * Null for a file that is not a part.
 */
export function splitPart(name: string): { set: string; part: number } | null {
  const rar = /^(.*)\.part(\d+)\.(?:exe|rar)$/i.exec(name);
  if (rar) return { set: rar[1]!, part: Number(rar[2]) };
  const numbered = /^(.*)\.(?:zip|7z)\.(\d{3})$/i.exec(name);
  return numbered ? { set: numbered[1]!, part: Number(numbered[2]) } : null;
}

/** One row of a file list: a whole file, or every part of a split set. */
export interface PartGroup<T> {
  key: string;
  /** The first part, which names the set. */
  lead: T;
  /** The files, in part order. */
  files: T[];
  size: number;
}

/**
 * Joins the parts of each split set into one row, where its first part
 * comes in the list; other files keep a row of their own.
 */
export function groupParts<T>(
  items: readonly T[],
  folder: (item: T) => string,
  name: (item: T) => string,
  size: (item: T) => number,
): PartGroup<T>[] {
  const groups = new Map<string, T[]>();
  for (const item of items) {
    const part = splitPart(name(item));
    const key = `${folder(item)}/${part ? part.set : name(item)}`;
    groups.set(key, [...(groups.get(key) ?? []), item]);
  }
  return [...groups].map(([key, files]) => {
    const sorted = files.length > 1
      ? [...files].sort((left, right) => (splitPart(name(left))?.part ?? 0) - (splitPart(name(right))?.part ?? 0))
      : files;
    return { key, lead: sorted[0]!, files: sorted, size: files.reduce((sum, file) => sum + size(file), 0) };
  });
}

/** A row of the Downloads panel: one download, or every part of a split set saved to one folder. */
export interface DownloadGroup {
  key: string;
  items: DownloadView[];
  lead: DownloadView;
  /** The part the row talks about: the one running, else the first that needs the user. */
  current: DownloadView;
  /** 1-based place of `current` among the set's parts. */
  part: number;
  state: DownloadState;
  size: number;
  received: number;
  verified: boolean;
  /** Every part was already in the folder, so none was downloaded. */
  existing: boolean;
}

// A set is running while any part runs; a failed part outranks waiting ones.
const GROUP_STATES: readonly DownloadState[] = ["running", "failed", "paused", "queued", "done"];

export function downloadGroups(downloads: readonly DownloadView[]): DownloadGroup[] {
  const groups = new Map<string, DownloadView[]>();
  for (const download of downloads) {
    const key = download.set ? `${download.set}|${download.directory}` : download.id;
    groups.set(key, [...(groups.get(key) ?? []), download]);
  }
  return [...groups].map(([key, items]) => {
    const state = GROUP_STATES.find((candidate) => items.some((item) => item.state === candidate)) ?? "done";
    const current = items.find((item) => item.state === state) ?? items[0]!;
    return {
      key,
      items,
      lead: items[0]!,
      current,
      part: items.indexOf(current) + 1,
      state,
      size: items.reduce((sum, item) => sum + item.size, 0),
      received: items.reduce((sum, item) => sum + (item.state === "done" ? item.size : item.received), 0),
      verified: items.every((item) => item.verified),
      existing: items.every((item) => item.existing),
    };
  });
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

/** When a recent file reached the archive, in ms; 0 when the server's time is unreadable. */
export function arrivedAt(file: RecentFile): number {
  return Date.parse(file.arrived) || 0;
}

/** One row of "Latest added": a copy of a work, with every part of a split set. */
export interface RecentCopy {
  /** The month folder and the copy's id. */
  key: string;
  /** The first part, which names the work for the whole set. */
  lead: RecentFile;
  /** The copy's files, in part order. */
  files: RecentFile[];
  /** When its newest part arrived, in ms. */
  arrived: number;
  size: number;
}

/** Joins the parts of each split set into one row, newest first. */
export function recentCopies(files: RecentFile[]): RecentCopy[] {
  const copies = new Map<string, RecentFile[]>();
  for (const file of files) {
    // The work code too, so two works can never share a row.
    const key = `${file.folder}/${file.name.split("__")[0]}/${copyKey(recentCopy(file))}`;
    copies.set(key, [...(copies.get(key) ?? []), file]);
  }
  return [...copies.entries()]
    .flatMap(([key, parts]) => {
      const sorted = [...parts].sort((left, right) => left.name.localeCompare(right.name, undefined, { numeric: true }));
      const lead = sorted[0];
      return lead
        ? [{ key, lead, files: sorted, arrived: Math.max(...parts.map(arrivedAt)), size: parts.reduce((sum, file) => sum + file.size, 0) }]
        : [];
    })
    .sort((left, right) => right.arrived - left.arrived || left.key.localeCompare(right.key));
}

/** The copies that arrived on one local calendar day. */
export interface ArrivalDay {
  /** `YYYY-MM-DD` in local time. */
  key: string;
  /** Local midnight of that day. */
  date: Date;
  /** 0 for today, 1 for yesterday… */
  daysAgo: number;
  copies: RecentCopy[];
}

/** Groups copies, already newest first, by the local day they arrived. */
export function arrivalDays(copies: RecentCopy[], now: Date): ArrivalDay[] {
  const midnight = (time: Date) => new Date(time.getFullYear(), time.getMonth(), time.getDate());
  const pad = (part: number) => String(part).padStart(2, "0");
  const today = midnight(now).getTime();
  const days: ArrivalDay[] = [];
  for (const copy of copies) {
    const date = midnight(new Date(copy.arrived));
    const key = `${date.getFullYear()}-${pad(date.getMonth() + 1)}-${pad(date.getDate())}`;
    let day = days.at(-1);
    if (day?.key !== key) {
      // Rounded, so a day with a daylight-saving change still counts as one.
      day = { key, date, daysAgo: Math.round((today - date.getTime()) / 86_400_000), copies: [] };
      days.push(day);
    }
    day.copies.push(copy);
  }
  return days;
}

/** "Today · Sun 18 Oct", "Yesterday · Sat 17 Oct", "Fri 16 Oct". */
export function arrivalDayLabel(t: Translate, day: ArrivalDay, locale: LocaleId): string {
  const date = day.date.toLocaleDateString(locale, { weekday: "short", day: "numeric", month: "short" });
  if (day.daysAgo === 0) return t("library.recentToday", { date });
  if (day.daysAgo === 1) return t("library.recentYesterday", { date });
  return date;
}

/** What the user has seen of "Latest added", kept between sessions. */
export interface ArrivalsSeen {
  /** When "Latest added" was last open, ms since 1970; null before the first time. */
  openedAt: number | null;
  /** Recent files whose New mark was cleared. */
  seen: string[];
}

/** A copy is new until every one of its parts was seen. */
export function isNewCopy(copy: RecentCopy, seen: Set<string>): boolean {
  return copy.files.some((file) => !seen.has(file.path));
}

/** The switch's badge: unseen copies that arrived since "Latest added" was last open. */
export function newSinceOpened(copies: RecentCopy[], state: ArrivalsSeen): number {
  const seen = new Set(state.seen);
  return copies.filter((copy) => isNewCopy(copy, seen) && (state.openedAt === null || copy.arrived > state.openedAt)).length;
}

export function recentCopy(file: RecentFile): Copy {
  return { path: file.path, name: file.name, workId: file.workId, version: file.version, updated: file.updated, released: file.released };
}

/** How far back "Latest added" can look, in days. */
export const LATEST_DAYS = [1, 3, 7, 14, 30] as const;

/** The `YYYY-MM` folder an archive path is in. */
export function fileFolder(path: string): string {
  return path.split("/").at(-2) ?? "";
}

/** A file of "Date added" as a month listing row, dated by its arrival. */
export function recentFolderFile(file: RecentFile): FolderFile {
  return {
    name: file.name,
    path: file.path,
    size: file.size,
    modified: file.arrived,
    workId: file.workId,
    title: file.title,
    circle: file.circle,
    version: file.version,
    updated: file.updated,
    released: file.released,
    checked: file.checked,
  };
}

/** `2026-10` → `October 2026`, in the chosen language. */
export function addedMonthLabel(name: string, locale: LocaleId = "en-US"): string {
  const match = MONTH.exec(name);
  if (!match) return name;
  const date = new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, 1));
  return date.toLocaleString(locale, { month: "long", year: "numeric", timeZone: "UTC" });
}
