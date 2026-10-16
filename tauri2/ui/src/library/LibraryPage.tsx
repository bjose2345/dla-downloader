import { useQuery } from "@tanstack/react-query";
import { ChevronDown, ChevronRight, Download, RefreshCw } from "lucide-react";
import { motion } from "motion/react";
import { useDeferredValue, useEffect, useMemo, useRef, useState } from "react";

import { libraryNoticeText } from "../shared/copy";
import { dateLabel, sizeLabel } from "../shared/format";
import { gateway } from "../backend/gateway";
import {
  copyLabel,
  entryCopy,
  fileCopy,
  groupFolders,
  monthLabel,
  narrowResults,
  sortFiles,
  summarizeCopies,
  yearKey,
  type Copy,
  type CopySummary,
  type SortKey,
} from "./library";
import { usePresentation } from "../preferences/PresentationProvider";
import type { CatalogEntry, FolderFile, LibraryNotice, LibraryView, RequestedFile } from "../backend/types";
import { SearchField } from "../shared/SearchField";
import { DownloadsPanel } from "./DownloadsPanel";

interface Props {
  view: LibraryView;
  run: (command: Promise<LibraryView>) => void;
}

export function LibraryPage({ view, run }: Props) {
  const { t, count, locale } = usePresentation();
  const [query, setQuery] = useState("");
  const deferredQuery = useDeferredValue(query.trim());
  const [folder, setFolder] = useState<string | null>(null);
  const [highlight, setHighlight] = useState<string | null>(null);
  const [open, setOpen] = useState<Set<string>>(new Set());
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [sort, setSort] = useState<{ key: SortKey; ascending: boolean }>({ key: "name", ascending: true });

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

  const listing = useQuery({
    queryKey: ["library-folder", folder, view.catalogRevision],
    queryFn: () => gateway.libraryFolder(folder ?? ""),
    enabled: view.signedIn && folder !== null,
  });
  const search = useQuery({
    queryKey: ["library-search", deferredQuery, view.catalogRevision],
    queryFn: () => gateway.librarySearch(deferredQuery),
    enabled: deferredQuery.length > 0,
  });

  const searching = query.trim().length > 0;
  const results = useMemo(() => narrowResults(deferredQuery, search.data ?? []), [deferredQuery, search.data]);
  const files = useMemo(() => sortFiles(listing.data ?? [], sort.key, sort.ascending), [listing.data, sort.key, sort.ascending]);

  const chooseFolder = (name: string, file: string | null = null) => {
    setFolder(name);
    setHighlight(file);
    setSelected(new Set());
    setOpen((current) => new Set(current).add(yearKey(name)));
  };

  const download = (requested: RequestedFile[]) => {
    if (requested.length) run(gateway.downloadFiles(requested));
  };

  const toggle = (path: string) =>
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(path)) next.delete(path);
      else next.add(path);
      return next;
    });

  const sortBy = (key: SortKey) =>
    setSort((current) => ({ key, ascending: current.key === key ? !current.ascending : key === "name" || key === "title" }));

  const chosen = files.filter((file) => selected.has(file.path));
  const uncatalogued = files.filter((file) => !file.workId).length;
  const listingError = listing.error as unknown as LibraryNotice | null;
  const yearLabel = (key: string) => (key === "undated" ? t("library.undated") : key === "other" ? t("library.other") : key);

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

      {view.notice && (
        <div className={"notice" + (view.notice.code === "already_queued" ? " info" : "")} role="status">
          {libraryNoticeText(t, view.notice)}
        </div>
      )}

      <div className="library-body">
        <nav className="folder-tree" aria-label={t("library.months")}>
          <div className="tree-root">/dlsite</div>
          {years.map((year) => (
            <div key={year.key} className="tree-year">
              <button
                type="button"
                className="tree-toggle"
                aria-expanded={open.has(year.key)}
                onClick={() =>
                  setOpen((current) => {
                    const next = new Set(current);
                    if (next.has(year.key)) next.delete(year.key);
                    else next.add(year.key);
                    return next;
                  })
                }
              >
                {open.has(year.key) ? <ChevronDown size={15} aria-hidden="true" /> : <ChevronRight size={15} aria-hidden="true" />}
                {yearLabel(year.key)}
                <span className="tree-count">{year.files.toLocaleString(locale)}</span>
              </button>
              {open.has(year.key) &&
                year.months.map((month) => (
                  <button
                    type="button"
                    key={month.name}
                    className={"tree-month" + (month.name === folder && !searching ? " current" : "")}
                    aria-current={month.name === folder && !searching ? "true" : undefined}
                    onClick={() => {
                      setQuery("");
                      chooseFolder(month.name);
                    }}
                  >
                    {month.name === folder && !searching && (
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
          ))}
          {!years.length && <p className="muted tree-empty">{t("library.noMonths")}</p>}
        </nav>

        <div className="file-pane">
          {searching ? (
            <SearchResults
              results={results}
              loading={search.isFetching}
              signedIn={view.signedIn}
              onShow={(entry) => {
                setQuery("");
                chooseFolder(entry.folder, entry.fileName);
              }}
              onDownload={(entry) => download([{ path: entry.path, size: entry.size }])}
            />
          ) : folder === null ? (
            <div className="pane-empty">
              <h3>{t("library.chooseMonth")}</h3>
              <p className="muted">{t("library.chooseMonthLead")}</p>
            </div>
          ) : (
            <>
              <div className="pane-heading">
                <h3>/dlsite/{folder}</h3>
                <span className="muted">
                  {listing.isFetching
                    ? t("library.loading")
                    : t("library.folderSummary", {
                        files: count("count.files", files.length),
                        size: sizeLabel(files.reduce((sum, file) => sum + file.size, 0)),
                      })}
                </span>
              </div>
              {view.signedIn && !listingError && !listing.isFetching && uncatalogued > 0 && (
                <p className="muted pane-note">
                  {uncatalogued === files.length
                    ? t("library.monthNotCatalogued")
                    : t("library.partlyCatalogued", { files: count("count.files", uncatalogued) })}
                </p>
              )}
              {!view.signedIn ? (
                <p className="muted pane-note">{t("library.signInMonth")}</p>
              ) : listingError ? (
                <div className="notice">{libraryNoticeText(t, listingError)}</div>
              ) : (
                <FileTable
                  files={files}
                  highlight={highlight}
                  selected={selected}
                  sort={sort}
                  onSort={sortBy}
                  onToggle={toggle}
                  onToggleAll={(all) => setSelected(all ? new Set(files.map((file) => file.path)) : new Set())}
                  onDownload={(file) => download([{ path: file.path, size: file.size }])}
                />
              )}
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
            </>
          )}
        </div>
      </div>

      <DownloadsPanel view={view} run={run} />
    </section>
  );
}

interface TableProps {
  files: FolderFile[];
  highlight: string | null;
  selected: Set<string>;
  sort: { key: SortKey; ascending: boolean };
  onSort: (key: SortKey) => void;
  onToggle: (path: string) => void;
  onToggleAll: (all: boolean) => void;
  onDownload: (file: FolderFile) => void;
}

function FileTable({ files, highlight, selected, sort, onSort, onToggle, onToggleAll, onDownload }: TableProps) {
  const { t } = usePresentation();
  const copies = useMemo(() => summarizeCopies(files.map(fileCopy)), [files]);
  useEffect(() => {
    if (highlight) document.getElementById("file-" + highlight)?.scrollIntoView({ block: "center" });
  }, [highlight, files]);

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
            {header("size", t("library.colSize"))}
            {header("modified", t("library.colModified"))}
            <th className="actions">
              <span className="sr-only">{t("library.actions")}</span>
            </th>
          </tr>
        </thead>
        <tbody>
          {files.map((file) => (
            <tr
              key={file.path}
              id={"file-" + file.name}
              className={(file.name === highlight ? "highlight " : "") + (selected.has(file.path) ? "selected" : "")}
              onDoubleClick={() => onDownload(file)}
            >
              <td className="check">
                <input
                  type="checkbox"
                  aria-label={t("library.select", { name: file.name })}
                  checked={selected.has(file.path)}
                  onChange={() => onToggle(file.path)}
                />
              </td>
              <td className="name">
                <span className="file-name">{file.name}</span>
                {file.workId && <span className="work-code">{file.workId}</span>}
                <CopyLine copy={fileCopy(file)} copies={copies} />
              </td>
              <td className="title">
                <span>{file.title || <span className="muted">{file.workId ? t("library.untitled") : t("library.notInCatalog")}</span>}</span>
                {file.circle && <span className="circle">{file.circle}</span>}
              </td>
              <td className="size">{sizeLabel(file.size)}</td>
              <td className="modified">{dateLabel(file.modified)}</td>
              <td className="actions">
                <button
                  type="button"
                  className="secondary compact"
                  onClick={() => onDownload(file)}
                  title={file.checked ? t("library.checksumTitle") : t("library.noChecksumTitle")}
                >
                  {t("library.download")}
                </button>
              </td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

interface ResultsProps {
  results: CatalogEntry[];
  loading: boolean;
  signedIn: boolean;
  onShow: (entry: CatalogEntry) => void;
  onDownload: (entry: CatalogEntry) => void;
}

function SearchResults({ results, loading, signedIn, onShow, onDownload }: ResultsProps) {
  const { t, count } = usePresentation();
  const copies = useMemo(() => summarizeCopies(results.map(entryCopy)), [results]);
  return (
    <>
      <div className="pane-heading">
        <h3>{t("library.searchResults")}</h3>
        <span className="muted">
          {loading ? t("library.searching") : results.length >= 200 ? t("library.first200") : count("count.matches", results.length)}
        </span>
      </div>
      {!results.length && !loading ? (
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
              {results.map((entry) => (
                <tr key={entry.path} onDoubleClick={() => signedIn && onDownload(entry)}>
                  <td className="name">
                    <span className="work-code">{entry.workId || "—"}</span>
                    <span className="file-name">{entry.fileName}</span>
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
                  <td className="size">{sizeLabel(entry.size)}</td>
                  <td className="actions">
                    <button type="button" className="secondary compact" disabled={!signedIn} onClick={() => onDownload(entry)}>
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
