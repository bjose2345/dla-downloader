import { ChevronDown, Download } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { memo, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";

import type { RecentFile, RequestedFile } from "../backend/types";
import { usePresentation } from "../preferences/PresentationProvider";
import { sizeLabel } from "../shared/format";
import { LoadingLabel, TableSkeleton } from "./Loading";
import { LATEST_DAYS, arrivalDayLabel, arrivalDays, copyLabel, isNewCopy, recentCopy, type RecentCopy } from "./library";

// Hovering counts as seeing a work only after a short pause, so moving the
// pointer down the list does not clear every mark on the way. The pause
// starts with a real pointer movement: rows that scroll under a resting
// pointer get mouseover but no mousemove, and are not marked.
const HOVER_SEEN_MS = 700;

const files = (copies: RecentCopy[]): RecentFile[] => copies.flatMap((copy) => copy.files);
const requested = (copies: RecentCopy[]): RequestedFile[] => files(copies).map((file) => ({ path: file.path, size: file.size }));
const total = (copies: RecentCopy[]) => sizeLabel(copies.reduce((sum, copy) => sum + copy.size, 0));

interface Props {
  copies: RecentCopy[];
  /** How many days back the list goes. */
  days: number;
  onDays: (days: number) => void;
  loading: boolean;
  signedIn: boolean;
  seen: Set<string>;
  onSeen: (paths: string[]) => void;
  onShow: (file: RecentFile) => void;
  onDownload: (files: RequestedFile[]) => void;
}

/** "Latest added": what reached the archive in the last days, newest first. */
export function RecentPane({ copies, days, onDays, loading, signedIn, seen, onSeen, onShow, onDownload }: Props) {
  const { t, count, locale } = usePresentation();
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const byDay = useMemo(() => arrivalDays(copies, new Date()), [copies]);

  // A refresh can drop copies from the list; their selection goes with them.
  useEffect(() => {
    const listed = new Set(copies.map((copy) => copy.key));
    setSelected((current) => {
      const kept = new Set([...current].filter((key) => listed.has(key)));
      return kept.size === current.size ? current : kept;
    });
  }, [copies]);

  // Downloading a copy counts as having seen it.
  const download = (chosen: RecentCopy[]) => {
    onSeen(files(chosen).map((file) => file.path));
    onDownload(requested(chosen));
  };
  const toggle = (key: string) =>
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });

  // Rows get handlers that never change, so marking one row seen redraws
  // only that row, not the whole list.
  const latest = useRef({ toggle, onSeen, onShow, download });
  useLayoutEffect(() => {
    latest.current = { toggle, onSeen, onShow, download };
  });
  const actions = useMemo<RowActions>(
    () => ({
      toggle: (copy) => latest.current.toggle(copy.key),
      seen: (copy) => latest.current.onSeen(copy.files.map((file) => file.path)),
      show: (copy) => latest.current.onShow(copy.lead),
      download: (copy) => latest.current.download([copy]),
    }),
    [],
  );

  const chosen = copies.filter((copy) => selected.has(copy.key));
  const unseen = copies.filter((copy) => isNewCopy(copy, seen));
  const all = copies.length > 0 && copies.every((copy) => selected.has(copy.key));
  const waiting = loading && !copies.length;

  return (
    <div className="file-pane recent-pane">
      <div className="pane-heading">
        <h3>{t("library.recentTitle")}</h3>
        <span className="muted">
          {waiting ? <LoadingLabel /> : t("library.folderSummary", { files: count("count.works", copies.length), size: total(copies) })}
        </span>
        <div className="recent-controls">
          <label className="recent-days">
            <span>{t("library.latestShow")}</span>
            <span className="select-field">
              <select value={days} onChange={(event) => onDays(Number(event.target.value))}>
                {LATEST_DAYS.map((value) => (
                  <option key={value} value={value}>
                    {count("count.lastDays", value)}
                  </option>
                ))}
              </select>
              <ChevronDown size={15} aria-hidden="true" />
            </span>
          </label>
          {unseen.length > 0 && (
            <button type="button" className="text-button" onClick={() => onSeen(files(unseen).map((file) => file.path))}>
              {t("library.markAllSeen")}
            </button>
          )}
        </div>
      </div>

      {waiting ? (
        <TableSkeleton />
      ) : !copies.length ? (
        <div className="pane-empty">
          <h3>{t("library.recentEmptyTitle")}</h3>
          <p className="muted">{t("library.recentEmptyLead")}</p>
        </div>
      ) : (
        <div className="table-scroll">
          <table className="file-table recent-table">
            <thead>
              <tr>
                <th className="check">
                  <input
                    type="checkbox"
                    aria-label={t("library.selectAll")}
                    checked={all}
                    onChange={(event) => setSelected(event.target.checked ? new Set(copies.map((copy) => copy.key)) : new Set())}
                  />
                </th>
                <th>{t("library.colWork")}</th>
                <th>{t("library.colType")}</th>
                <th>{t("library.colReleased")}</th>
                <th className="size">{t("library.colSize")}</th>
                <th>{t("library.colArrived")}</th>
                <th className="actions">
                  <span className="sr-only">{t("library.actions")}</span>
                </th>
              </tr>
            </thead>
            {byDay.map((day) => (
              <tbody key={day.key}>
                <tr className="day-row">
                  <td colSpan={7}>
                    {arrivalDayLabel(t, day, locale)}
                    <span className="muted">
                      {t("library.folderSummary", { files: count("count.works", day.copies.length), size: total(day.copies) })}
                    </span>
                  </td>
                </tr>
                {day.copies.map((copy) => (
                  <RecentRow
                    key={copy.key}
                    copy={copy}
                    fresh={isNewCopy(copy, seen)}
                    selected={selected.has(copy.key)}
                    signedIn={signedIn}
                    actions={actions}
                  />
                ))}
              </tbody>
            ))}
          </table>
        </div>
      )}

      {!waiting && (
        <div className="pane-actions">
          <span className="muted">
            {chosen.length
              ? t("library.selectedSummary", { files: count("count.works", chosen.length), size: total(chosen) })
              : t("library.recentHint")}
          </span>
          <div className="pane-buttons">
            <button type="button" className="secondary" disabled={!signedIn || !copies.length} onClick={() => download(copies)}>
              <Download size={16} aria-hidden="true" /> {t("library.downloadAll", { count: copies.length.toLocaleString(locale) })}
            </button>
            <motion.button
              className="primary"
              type="button"
              disabled={!signedIn || !chosen.length}
              whileTap={{ scale: 0.98 }}
              onClick={() => {
                download(chosen);
                setSelected(new Set());
              }}
            >
              <Download size={16} aria-hidden="true" /> {t("library.downloadSelected")}
            </motion.button>
          </div>
        </div>
      )}
    </div>
  );
}

interface RowActions {
  toggle: (copy: RecentCopy) => void;
  seen: (copy: RecentCopy) => void;
  show: (copy: RecentCopy) => void;
  download: (copy: RecentCopy) => void;
}

interface RowProps {
  copy: RecentCopy;
  /** Not seen yet: the row carries a New mark. */
  fresh: boolean;
  selected: boolean;
  signedIn: boolean;
  actions: RowActions;
}

const RecentRow = memo(function RecentRow({ copy, fresh, selected, signedIn, actions }: RowProps) {
  const { t, count, locale } = usePresentation();
  const timer = useRef<number | undefined>(undefined);
  useEffect(() => () => window.clearTimeout(timer.current), []);
  const onSeen = () => actions.seen(copy);
  const onDownload = () => actions.download(copy);
  const stop = () => {
    window.clearTimeout(timer.current);
    timer.current = undefined;
  };
  const seen = () => {
    stop();
    if (fresh) onSeen();
  };
  const file = copy.lead;
  // Only an update says which version it is; beside "Added" a DLsite update
  // date would read as a contradiction.
  const version = file.update && (file.version || file.updated) ? copyLabel(t, recentCopy(file)) : null;
  const time = new Date(copy.arrived).toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
  const checked = copy.files.every((part) => part.checked);

  return (
    <tr
      className={(selected ? "selected" : "") + (fresh ? " unseen" : "")}
      onMouseMove={() => {
        if (fresh && timer.current === undefined) timer.current = window.setTimeout(onSeen, HOVER_SEEN_MS);
      }}
      onMouseLeave={stop}
      onClick={seen}
      onFocus={seen}
      onDoubleClick={() => signedIn && onDownload()}
    >
      <td className="check">
        <input
          type="checkbox"
          aria-label={t("library.select", { name: file.title || file.name })}
          checked={selected}
          onChange={() => actions.toggle(copy)}
        />
      </td>
      <td className="work" title={copy.files.map((part) => part.name).join("\n")}>
        <span className="work-title">
          <span>{file.title || (file.workId ? <span className="muted">{t("library.untitled")}</span> : file.name)}</span>
          <AnimatePresence initial={false}>
            {fresh && (
              <motion.span
                className="new-mark"
                title={t("library.newMarkTitle")}
                initial={{ opacity: 0, scale: 0.6 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.6 }}
                transition={{ duration: 0.35 }}
              >
                {t("library.newMark")}
              </motion.span>
            )}
          </AnimatePresence>
        </span>
        <span className="work-sub">
          {file.workId ? <span className="work-code">{file.workId}</span> : <span>{t("library.notInCatalog")}</span>}
          {file.circle && <span>{file.circle}</span>}
          {copy.files.length > 1 && <span>{count("count.parts", copy.files.length)}</span>}
        </span>
      </td>
      <td className="type">
        {file.workId && (
          <span
            className={"type-badge " + (file.update ? "updated" : "added")}
            title={file.update ? t("library.typeUpdatedTitle") : t("library.typeAddedTitle")}
          >
            {file.update ? t("library.typeUpdated") : t("library.typeAdded")}
          </span>
        )}
        {version && <span className="copy-line">{version}</span>}
      </td>
      <td className="month">
        <button type="button" className="link" title={t("library.showInMonth", { month: file.folder })} onClick={() => actions.show(copy)}>
          {file.folder}
        </button>
      </td>
      <td className="size">{sizeLabel(copy.size)}</td>
      <td className="modified">{time}</td>
      <td className="actions">
        <button
          type="button"
          className="secondary compact"
          disabled={!signedIn}
          onClick={onDownload}
          title={checked ? t("library.checksumTitle") : t("library.noChecksumTitle")}
        >
          {t("library.download")}
        </button>
      </td>
    </tr>
  );
});
