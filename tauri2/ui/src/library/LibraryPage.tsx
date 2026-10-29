import { useQuery } from "@tanstack/react-query";
import { Calendar, CalendarPlus, ChevronDown, ChevronRight, Download, Plus, RefreshCw } from "lucide-react";
import { motion } from "motion/react";
import { memo, useDeferredValue, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import { libraryNoticeText } from "../shared/copy";
import { dateLabel, sizeLabel } from "../shared/format";
import { gateway } from "../backend/gateway";
import {
  addedMonthLabel,
  copyLabel,
  entryCopy,
  fileCopy,
  fileFolder,
  groupFolders,
  groupParts,
  monthLabel,
  narrowResults,
  newSinceOpened,
  recentCopies,
  recentFolderFile,
  sortFiles,
  summarizeCopies,
  yearKey,
  type Copy,
  type CopySummary,
  type PartGroup,
  type SortKey,
  type YearGroup,
} from "./library";
import { usePresentation } from "../preferences/PresentationProvider";
import type { CatalogEntry, FolderFile, LibraryNotice, LibraryView, RequestedFile } from "../backend/types";
import { SearchField } from "../shared/SearchField";
import { Tabs, type TabItem } from "../shared/Tabs";
import { DownloadsPanel } from "./DownloadsPanel";
import { LoadingLabel, TableSkeleton, TreeSkeleton } from "./Loading";
import { RecentPane } from "./RecentPane";
import { useArrivalsSeen, useLatestDays } from "./useArrivalsSeen";

interface Props {
  view: LibraryView;
  run: (command: Promise<LibraryView>) => void;
}

/** Months by release date, months by when files arrived, or the latest arrivals. */
type Order = "release" | "added" | "latest";

type Sort = { key: SortKey; ascending: boolean };

// The viewer's minutes east of UTC: months by date added start at their midnight.
const OFFSET = -new Date().getTimezoneOffset();

const toggled = (key: string) => (current: Set<string>) => {
  const next = new Set(current);
  if (next.has(key)) next.delete(key);
  else next.add(key);
  return next;
};

export function LibraryPage({ view, run }: Props) {
  const { t, count, locale } = usePresentation();
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query.trim());
  const [order, setOrder] = useState<Order>("release");
  const [folder, setFolder] = useState<string | null>(null);
  const [added, setAdded] = useState<string | null>(null);
  const [highlight, setHighlight] = useState<string | null>(null);
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [addedOpen, setAddedOpen] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [sort, setSort] = useState<Sort>({ key: "name", ascending: true });
  // Date added lists a month's arrivals newest first.
  const [addedSort, setAddedSort] = useState<Sort>({ key: "modified", ascending: false });
  const [days, setDays] = useLatestDays();
  const byAdded = order === "added";

  const folders = useQuery({
    queryKey: ["library-folders", view.catalogRevision],
    queryFn: gateway.libraryFolders,
  });
  const years = useMemo(() => groupFolders(folders.data ?? []), [folders.data]);

  // Open the newest year once, when the tree first has content; after that
  // only the user opens and closes years.
  const newest = years[0]?.key;
  const openedNewest = useRef(false);
  useEffect(() => {
    if (openedNewest.current || !newest) return;
    openedNewest.current = true;
    setOpen((current) => (current.size ? current : new Set([newest])));
  }, [newest]);

  const addedMonths = useQuery({
    queryKey: ["library-added-months", view.catalogRevision],
    queryFn: () => gateway.libraryAddedMonths(OFFSET),
    enabled: byAdded,
  });
  const addedYears = useMemo(() => groupFolders(addedMonths.data ?? []), [addedMonths.data]);
  // Date added opens on its newest month.
  const newestAdded = addedYears[0]?.months[0]?.name;
  useEffect(() => {
    if (!byAdded || added !== null || !newestAdded) return;
    setAdded(newestAdded);
    setAddedOpen((current) => new Set(current).add(yearKey(newestAdded)));
  }, [byAdded, added, newestAdded]);

  const listing = useQuery({
    queryKey: ["library-folder", folder, view.catalogRevision],
    queryFn: () => gateway.libraryFolder(folder ?? ""),
    enabled: view.signedIn && order === "release" && folder !== null,
  });
  const addedListing = useQuery({
    queryKey: ["library-added", added, view.catalogRevision],
    queryFn: () => gateway.libraryAdded(added ?? "", OFFSET),
    enabled: view.signedIn && byAdded && added !== null,
  });
  const recent = useQuery({
    queryKey: ["library-recent", days, view.catalogRevision],
    queryFn: () => gateway.libraryRecent(days),
  });
  const arrivals = useArrivalsSeen(recent.data);
  const copies = useMemo(() => recentCopies(recent.data ?? []), [recent.data]);
  const fresh = useMemo(() => newSinceOpened(copies, arrivals.state), [copies, arrivals.state]);
  // While "Latest added" is open, what it lists counts as looked at.
  const { markOpened } = arrivals;
  useEffect(() => {
    if (order === "latest" && recent.data) markOpened();
  }, [order, recent.data, markOpened]);

  const search = useQuery({
    queryKey: ["library-search", deferredQuery, view.catalogRevision],
    queryFn: () => gateway.librarySearch(deferredQuery),
    enabled: deferredQuery.length > 0,
  });

  const searching = query.trim().length > 0;
  const results = useMemo(() => narrowResults(deferredQuery, search.data ?? []), [deferredQuery, search.data]);
  const monthFiles = useMemo(
    () => (byAdded ? (addedListing.data ?? []).map(recentFolderFile) : (listing.data ?? [])),
    [byAdded, addedListing.data, listing.data],
  );
  const activeSort = byAdded ? addedSort : sort;
  const files = useMemo(
    () => sortFiles(monthFiles, activeSort.key, activeSort.ascending),
    [monthFiles, activeSort.key, activeSort.ascending],
  );
  const month = byAdded ? added : folder;
  const monthListing = byAdded ? addedListing : listing;

  const chooseOrder = (next: Order) => {
    setOrder(next);
    setSelected(new Set());
  };

  const chooseFolder = (name: string, file: string | null = null) => {
    setFolder(name);
    setHighlight(file);
    setSelected(new Set());
    setOpen((current) => new Set(current).add(yearKey(name)));
  };

  const chooseAdded = (name: string) => {
    setAdded(name);
    setHighlight(null);
    setSelected(new Set());
    setAddedOpen((current) => new Set(current).add(yearKey(name)));
  };

  const showInMonth = (name: string, file: string) => {
    setQuery("");
    chooseOrder("release");
    chooseFolder(name, file);
  };

  const download = (requested: RequestedFile[]) => {
    if (requested.length) run(gateway.downloadFiles(requested));
  };

  // A split set is ticked and unticked as a whole.
  const toggle = (paths: string[]) =>
    setSelected((current) => {
      const next = new Set(current);
      const all = paths.every((path) => next.has(path));
      for (const path of paths) {
        if (all) next.delete(path);
        else next.add(path);
      }
      return next;
    });

  const sortBy = (key: SortKey) =>
    (byAdded ? setAddedSort : setSort)((current) => ({
      key,
      ascending: current.key === key ? !current.ascending : key === "name" || key === "title",
    }));

  const chosen = files.filter((file) => selected.has(file.path));
  const uncatalogued = files.filter((file) => !file.workId).length;
  const listingError = listing.error as unknown as LibraryNotice | null;
  const orders: TabItem<Order>[] = [
    { id: "release", label: t("library.viewRelease"), icon: Calendar },
    { id: "added", label: t("library.viewAdded"), icon: CalendarPlus },
    {
      id: "latest",
      label: t("library.viewLatest"),
      icon: Plus,
      badge:
        order !== "latest" && fresh > 0
          ? { label: t("library.newBadge", { works: count("count.newWorks", fresh) }), mark: fresh > 99 ? "99+" : fresh.toLocaleString(locale) }
          : undefined,
    },
  ];
  const searchResults = (
    <SearchResults
      results={results}
      loading={search.isFetching}
      signedIn={view.signedIn}
      onShow={(entry) => showInMonth(entry.folder, entry.fileName)}
      onDownload={(entries) => download(entries.map((entry) => ({ path: entry.path, size: entry.size })))}
    />
  );

  return (
    <section className="library">
      <div className="toolbar">
        <SearchField value={query} onChange={setQuery} label={t("library.searchPlaceholder")} />
        <button
          className="secondary"
          type="button"
          disabled={!view.signedIn || view.refreshing !== null}
          onClick={() => run(gateway.libraryRefresh(true))}
          title={t("library.refreshTitle")}
        >
          <RefreshCw size={16} aria-hidden="true" className={view.refreshing ? "spinning" : undefined} />
          {view.refreshing ? t("library.refreshing") : t("library.refresh")}
        </button>
      </div>
      <div className="view-row">
        <Tabs
          className="view-switch"
          pillId="library-order-pill"
          items={orders}
          current={order}
          label={t("library.viewLabel")}
          onChange={chooseOrder}
        />
        <p className="catalog-status muted" role="status" aria-live="polite">
          {view.refreshing
            ? view.refreshing.total
              ? t("library.statusUpdating", { checked: view.refreshing.checked, total: view.refreshing.total })
              : t("library.statusChecking")
            : view.files
              ? t("library.statusCounts", { files: count("count.files", view.files), months: count("count.months", view.folders) }) +
                (view.signedIn ? "" : t("library.statusOffline"))
              : view.signedIn
                ? t("library.statusEmpty")
                : t("library.statusSignIn")}
        </p>
      </div>

      {view.notice && (
        <div className={"notice" + (view.notice.code === "already_queued" ? " info" : "")} role="status">
          {libraryNoticeText(t, view.notice)}
        </div>
      )}

      {order === "latest" ? (
        searching ? (
          <div className="file-pane">{searchResults}</div>
        ) : (
          <RecentPane
            copies={copies}
            days={days}
            onDays={setDays}
            loading={recent.isFetching || view.refreshing !== null}
            signedIn={view.signedIn}
            seen={arrivals.seen}
            onSeen={arrivals.markSeen}
            onShow={(file) => showInMonth(file.folder, file.name)}
            onDownload={download}
          />
        )
      ) : (
        <div className="library-body">
          <MonthTree
            label={byAdded ? t("library.addedMonths") : t("library.months")}
            root={byAdded ? t("library.addedRoot") : "/dlsite"}
            years={byAdded ? addedYears : years}
            loading={byAdded ? addedMonths.isPending : folders.isPending}
            open={byAdded ? addedOpen : open}
            current={searching ? null : month}
            onToggle={(key) => (byAdded ? setAddedOpen : setOpen)(toggled(key))}
            onChoose={(name) => {
              setQuery("");
              if (byAdded) chooseAdded(name);
              else chooseFolder(name);
            }}
          />

          <div className="file-pane">
            {searching ? (
              searchResults
            ) : month === null ? (
              // Date added opens on its newest month as soon as the months load.
              byAdded && (addedMonths.isFetching || addedYears.length > 0) ? (
                <>
                  <div className="pane-heading">
                    <h3>{t("library.viewAdded")}</h3>
                    <span className="muted">
                      <LoadingLabel />
                    </span>
                  </div>
                  <TableSkeleton />
                </>
              ) : byAdded ? (
                <div className="pane-empty">
                  <h3>{t("library.addedEmptyTitle")}</h3>
                  <p className="muted">{t("library.addedEmptyLead")}</p>
                </div>
              ) : (
                <div className="pane-empty">
                  <h3>{t("library.chooseMonth")}</h3>
                  <p className="muted">{t("library.chooseMonthLead")}</p>
                </div>
              )
            ) : (
              <>
                <div className="pane-heading">
                  <h3>{byAdded ? t("library.addedIn", { month: addedMonthLabel(month, locale) }) : `/dlsite/${month}`}</h3>
                  <span className="muted">
                    {monthListing.isFetching ? (
                      <LoadingLabel />
                    ) : (
                      t("library.folderSummary", {
                          files: count("count.files", files.length),
                        size: sizeLabel(files.reduce((sum, file) => sum + file.size, 0)),
                      })
                    )}
                  </span>
                </div>
                {!byAdded && view.signedIn && !listingError && !listing.isFetching && uncatalogued > 0 && (
                  <p className="muted pane-note">
                    {uncatalogued === files.length
                      ? t("library.monthNotCatalogued")
                      : t("library.partlyCatalogued", { files: count("count.files", uncatalogued) })}
                  </p>
                )}
                {!view.signedIn ? (
                  <p className="muted pane-note">{t("library.signInMonth")}</p>
                ) : !byAdded && listingError ? (
                  <div className="notice">{libraryNoticeText(t, listingError)}</div>
                ) : monthListing.data === undefined ? (
                  <TableSkeleton />
                ) : (
                  <FileTable
                    files={files}
                    highlight={highlight}
                    selected={selected}
                    sort={activeSort}
                    dateKey={byAdded ? "library.colAdded" : "library.colModified"}
                    onSort={sortBy}
                    onToggle={toggle}
                    onToggleAll={(all) => setSelected(all ? new Set(files.map((file) => file.path)) : new Set())}
                    onDownload={(parts) => download(parts.map((file) => ({ path: file.path, size: file.size })))}
                    onShowFolder={byAdded ? (file) => showInMonth(fileFolder(file.path), file.name) : undefined}
                  />
                )}
                {monthListing.data !== undefined && (
                  <div className="pane-actions">
                    <span className="muted">
                      {chosen.length
                        ? t("library.selectedSummary", {
                            files: count("count.files", chosen.length),
                            size: sizeLabel(chosen.reduce((sum, file) => sum + file.size, 0)),
                          })
                        : t("library.selectHint")}
                    </span>
                    <motion.button
                      className="primary"
                      type="button"
                      disabled={!chosen.length}
                      whileTap={{ scale: 0.98 }}
                      onClick={() => {
                        download(chosen.map((file) => ({ path: file.path, size: file.size })));
                        setSelected(new Set());
                      }}
                    >
                      <Download size={16} aria-hidden="true" /> {t("library.downloadSelected")}
                    </motion.button>
                  </div>
                )}
              </>
            )}
          </div>
        </div>
      )}

      <DownloadsPanel view={view} run={run} />
    </section>
  );
}

interface TreeProps {
  label: string;
  root: string;
  years: YearGroup[];
  /** The months are still on their way. */
  loading: boolean;
  open: Set<string>;
  current: string | null;
  onToggle: (year: string) => void;
  onChoose: (month: string) => void;
}

/** Years and their months, by release date or by date added. */
function MonthTree({ label, root, years, loading, open, current, onToggle, onChoose }: TreeProps) {
  const { t, locale } = usePresentation();
  const yearLabel = (key: string) => (key === "undated" ? t("library.undated") : key === "other" ? t("library.other") : key);
  return (
    <nav className="folder-tree" aria-label={label}>
      <div className="tree-root">{root}</div>
      {years.map((year) => {
        const [only] = year.months;
        // The undated folder is one entry, not a year holding one month.
        if (year.key === "undated" && only && year.months.length === 1) {
          const chosen = only.name === current;
          return (
            <div key={year.key} className="tree-year">
              <button
                type="button"
                className={"tree-toggle tree-single" + (chosen ? " current" : "")}
                aria-current={chosen ? "true" : undefined}
                title={only.name}
                onClick={() => onChoose(only.name)}
              >
                {chosen && <motion.span className="tree-pill" layoutId="tree-pill" transition={{ type: "spring", stiffness: 420, damping: 36 }} />}
                <span>{yearLabel(year.key)}</span>
                <span className="tree-count">{only.manifest ? only.files.toLocaleString(locale) : "–"}</span>
              </button>
            </div>
          );
        }
        return (
        <div key={year.key} className="tree-year">
          <button type="button" className="tree-toggle" aria-expanded={open.has(year.key)} onClick={() => onToggle(year.key)}>
            {open.has(year.key) ? <ChevronDown size={15} aria-hidden="true" /> : <ChevronRight size={15} aria-hidden="true" />}
            {yearLabel(year.key)}
            <span className="tree-count">{year.files.toLocaleString(locale)}</span>
          </button>
          {open.has(year.key) &&
            year.months.map((month) => (
              <button
                type="button"
                key={month.name}
                className={"tree-month" + (month.name === current ? " current" : "")}
                aria-current={month.name === current ? "true" : undefined}
                onClick={() => onChoose(month.name)}
              >
                {month.name === current && (
                  <motion.span className="tree-pill" layoutId="tree-pill" transition={{ type: "spring", stiffness: 420, damping: 36 }} />
                )}
                <span>{monthLabel(month.name, locale)}</span>
                {month.manifest ? (
                  <span className="tree-count">{month.files.toLocaleString(locale)}</span>
                ) : (
                  <span className="tree-count" title={t("library.noCatalogTitle")}>
                    –
                  </span>
                )}
              </button>
            ))}
        </div>
        );
      })}
      {!years.length && (loading ? <TreeSkeleton /> : <p className="muted tree-empty">{t("library.noMonths")}</p>)}
    </nav>
  );
}

interface TableProps {
  files: FolderFile[];
  highlight: string | null;
  selected: Set<string>;
  sort: Sort;
  /** The date column: a file's time in its folder, or when it arrived. */
  dateKey: "library.colModified" | "library.colAdded";
  onSort: (key: SortKey) => void;
  onToggle: (paths: string[]) => void;
  onToggleAll: (all: boolean) => void;
  /** A whole file, or every part of a split set. */
  onDownload: (files: FolderFile[]) => void;
  /** Adds the Released column, a link to each file's month folder. */
  onShowFolder?: (file: FolderFile) => void;
}

function FileTable({ files, highlight, selected, sort, dateKey, onSort, onToggle, onToggleAll, onDownload, onShowFolder }: TableProps) {
  const { t } = usePresentation();
  const copies = useMemo(() => summarizeCopies(files.map(fileCopy)), [files]);
  const groups = useMemo(() => groupParts(files, (file) => fileFolder(file.path), (file) => file.name, (file) => file.size), [files]);
  // A part of a split set is shown by its set's row, named after the first part.
  const shown = highlight ? groups.find((group) => group.files.some((file) => file.name === highlight))?.lead.name ?? highlight : null;
  useEffect(() => {
    if (shown) document.getElementById("file-" + shown)?.scrollIntoView({ block: "center" });
  }, [shown, groups]);
  // Rows get handlers that never change, so ticking one file in a month of
  // thousands redraws only that row.
  const latest = useRef({ onToggle, onDownload, onShowFolder });
  useLayoutEffect(() => {
    latest.current = { onToggle, onDownload, onShowFolder };
  });
  const actions = useMemo<RowActions>(
    () => ({
      toggle: (group) => latest.current.onToggle(group.files.map((file) => file.path)),
      download: (group) => latest.current.onDownload(group.files),
      showFolder: (group) => latest.current.onShowFolder?.(group.lead),
    }),
    [],
  );

  const header = (key: SortKey, label: string) => (
    <th aria-sort={sort.key === key ? (sort.ascending ? "ascending" : "descending") : undefined}>
      <button type="button" className="sort" onClick={() => onSort(key)}>
        {label}
        {sort.key === key && <span aria-hidden="true">{sort.ascending ? " ▲" : " ▼"}</span>}
      </button>
    </th>
  );

  if (!files.length) return <p className="muted pane-note">{t("library.emptyMonth")}</p>;
  const all = files.every((file) => selected.has(file.path));
  return (
    <div className="table-scroll">
      <table className="file-table">
        <thead>
          <tr>
            <th className="check">
              <input type="checkbox" aria-label={t("library.selectAll")} checked={all} onChange={(event) => onToggleAll(event.target.checked)} />
            </th>
            {header("name", t("library.colName"))}
            {header("title", t("library.colTitle"))}
            {onShowFolder && <th>{t("library.colReleased")}</th>}
            {header("size", t("library.colSize"))}
            {header("modified", t(dateKey))}
            <th className="actions">
              <span className="sr-only">{t("library.actions")}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {groups.map((group) => (
            <FileRow
              key={group.key}
              group={group}
              highlighted={group.lead.name === shown}
              selected={group.files.every((file) => selected.has(file.path))}
              copies={copies}
              actions={actions}
              showFolder={onShowFolder !== undefined}
            />
          ))}
        </tbody>
      </table>
    </div>
  );
}

interface RowActions {
  toggle: (group: PartGroup<FolderFile>) => void;
  download: (group: PartGroup<FolderFile>) => void;
  showFolder: (group: PartGroup<FolderFile>) => void;
}

interface RowProps {
  group: PartGroup<FolderFile>;
  highlighted: boolean;
  selected: boolean;
  copies: CopySummary;
  actions: RowActions;
  /** Adds the Released column. */
  showFolder: boolean;
}

const FileRow = memo(function FileRow({ group, highlighted, selected, copies, actions, showFolder }: RowProps) {
  const { t, count } = usePresentation();
  const file = group.lead;
  const parts = group.files.length;
  const modified = parts > 1
    ? group.files.reduce<string | null>((latest, part) => (part.modified && (!latest || part.modified > latest) ? part.modified : latest), null)
    : file.modified;
  return (
    <tr
      id={"file-" + file.name}
      className={(highlighted ? "highlight " : "") + (selected ? "selected" : "")}
      onDoubleClick={() => actions.download(group)}
    >
      <td className="check">
        <input type="checkbox" aria-label={t("library.select", { name: file.name })} checked={selected} onChange={() => actions.toggle(group)} />
      </td>
      <td className="name">
        <span className="file-name" title={parts > 1 ? group.files.map((part) => part.name).join("\n") : undefined}>{file.name}</span>
        {file.workId && <span className="work-code">{file.workId}</span>}
        {parts > 1 && <span className="parts-count">{count("count.parts", parts)}</span>}
        <CopyLine copy={fileCopy(file)} copies={copies} />
      </td>
      <td className="title">
        <span>{file.title || <span className="muted">{file.workId ? t("library.untitled") : t("library.notInCatalog")}</span>}</span>
        {file.circle && <span className="circle">{file.circle}</span>}
      </td>
      {showFolder && (
        <td className="month">
          <button type="button" className="link" title={t("library.showInMonth", { month: fileFolder(file.path) })} onClick={() => actions.showFolder(group)}>
            {fileFolder(file.path)}
          </button>
        </td>
      )}
      <td className="size">{sizeLabel(group.size)}</td>
      <td className="modified">{dateLabel(modified)}</td>
      <td className="actions">
        <button
          type="button"
          className="secondary compact"
          onClick={() => actions.download(group)}
          title={file.checked ? t("library.checksumTitle") : t("library.noChecksumTitle")}
        >
          {t("library.download")}
        </button>
      </td>
    </tr>
  );
});

interface ResultsProps {
  results: CatalogEntry[];
  loading: boolean;
  signedIn: boolean;
  onShow: (entry: CatalogEntry) => void;
  /** A whole file, or every part of a split set. */
  onDownload: (entries: CatalogEntry[]) => void;
}

function SearchResults({ results, loading, signedIn, onShow, onDownload }: ResultsProps) {
  const { t, count } = usePresentation();
  const copies = useMemo(() => summarizeCopies(results.map(entryCopy)), [results]);
  const groups = useMemo(() => groupParts(results, (entry) => entry.folder, (entry) => entry.fileName, (entry) => entry.size), [results]);
  return (
    <>
      <div className="pane-heading">
        <h3>{t("library.searchResults")}</h3>
        <span className="muted">
          {loading ? (
            <LoadingLabel text={t("library.searching")} />
          ) : results.length >= 200 ? (
            t("library.first200")
          ) : (
            count("count.matches", groups.length)
          )}
        </span>
      </div>
      {loading && !results.length ? (
        <TableSkeleton />
      ) : !results.length ? (
        <p className="muted pane-note">{t("library.noMatches")}</p>
      ) : (
        <div className="table-scroll">
          <table className="file-table">
            <thead>
              <tr>
                <th>{t("library.colWork")}</th>
                <th>{t("library.colTitle")}</th>
                <th>{t("library.colMonth")}</th>
                <th>{t("library.colSize")}</th>
                <th className="actions">
                  <span className="sr-only">{t("library.actions")}</span>
                </th>
              </tr>
            </thead>
            <tbody>
              {groups.map(({ key, lead: entry, files, size }) => (
                <tr key={key} onDoubleClick={() => signedIn && onDownload(files)}>
                  <td className="name">
                    <span className="work-code">{entry.workId || "—"}</span>
                    <span className="file-name" title={files.length > 1 ? files.map((part) => part.fileName).join("\n") : undefined}>{entry.fileName}</span>
                    {files.length > 1 && <span className="parts-count">{count("count.parts", files.length)}</span>}
                    <CopyLine copy={entryCopy(entry)} copies={copies} />
                  </td>
                  <td className="title">
                    <span>{entry.title || <span className="muted">{t("library.untitled")}</span>}</span>
                    {entry.circle && <span className="circle">{entry.circle}</span>}
                  </td>
                  <td className="month">
                    <button type="button" className="link" onClick={() => onShow(entry)}>
                      {entry.folder}
                    </button>
                  </td>
                  <td className="size">{sizeLabel(size)}</td>
                  <td className="actions">
                    <button type="button" className="secondary compact" disabled={!signedIn} onClick={() => onDownload(files)}>
                      {t("library.download")}
                    </button>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </>
  );
}

/** A copy's version and date, and whether it is its work's newest copy. */
function CopyLine({ copy, copies }: { copy: Copy; copies: CopySummary }) {
  const { t } = usePresentation();
  const several = copy.workId != null && copies.several.has(copy.workId);
  const label = copy.version || copy.updated || several ? copyLabel(t, copy) : null;
  const latest = copies.latest.has(copy.path);
  if (!label && !latest) return null;
  return (
    <span className="copy-line">
      {label}
      {latest && (
        <span className="latest-badge" title={t("library.latestTitle")}>
          {t("library.latest")}
        </span>
      )}
    </span>
  );
}
