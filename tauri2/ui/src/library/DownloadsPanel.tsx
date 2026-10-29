import { ArrowDownToLine, FolderOpen, Pause, Play, RotateCw, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";

import { downloadStatusText, libraryNoticeText } from "../shared/copy";
import { remainingLabel, sizeLabel } from "../shared/format";
import { gateway } from "../backend/gateway";
import { downloadGroups, summaryText } from "./library";
import { usePresentation } from "../preferences/PresentationProvider";
import type { DownloadState, LibraryView } from "../backend/types";

interface Props {
  view: LibraryView;
  run: (command: Promise<LibraryView>) => void;
}

export function DownloadsPanel({ view, run }: Props) {
  const { t, count } = usePresentation();
  const downloads = view.downloads;
  const finished = downloads.filter((download) => download.state === "done").length;
  const saved = downloads.filter((download) => download.state === "done" && !download.existing).length;
  const found = finished - saved;
  const waiting = downloads.some((download) => download.state === "queued");

  // The panel sits below the file list, so a small bar reports activity
  // while it is scrolled out of view.
  const panel = useRef<HTMLElement>(null);
  const [inView, setInView] = useState(true);
  const [seen, setSeen] = useState({ saved, found });
  const hasDownloads = downloads.length > 0;
  useEffect(() => {
    const element = panel.current;
    if (!element) return;
    const observer = new IntersectionObserver(([entry]) => setInView(Boolean(entry?.isIntersecting)), { threshold: 0.15 });
    observer.observe(element);
    return () => observer.disconnect();
  }, [hasDownloads]);
  useEffect(() => {
    if (inView) setSeen({ saved, found });
  }, [inView, saved, found]);

  const summary = hasDownloads ? summaryText(t, count, downloads, saved > seen.saved, found > seen.found) : null;

  return (
    <>
      <AnimatePresence>
        {summary && !inView && (
          <motion.div
            className="downloads-bar"
            role="status"
            initial={{ opacity: 0, y: 24, x: "-50%" }}
            animate={{ opacity: 1, y: 0, x: "-50%" }}
            exit={{ opacity: 0, y: 24, x: "-50%" }}
            transition={{ type: "spring", stiffness: 380, damping: 30 }}
          >
            <ArrowDownToLine size={16} aria-hidden="true" />
            <span>{summary}</span>
            <button
              type="button"
              className="secondary compact"
              onClick={() => panel.current?.scrollIntoView({ behavior: "smooth", block: "start" })}
            >
              {t("downloads.show")}
            </button>
          </motion.div>
        )}
      </AnimatePresence>
      {hasDownloads && (
        <section className="downloads" aria-label={t("downloads.title")} ref={panel}>
          <div className="pane-heading">
            <h3>{t("downloads.title")}</h3>
            <span className="muted">
              {!view.signedIn && waiting ? t("downloads.signInToContinue") : ""}
              {t("downloads.lead")}
            </span>
            {finished > 0 && (
              <button type="button" className="text-button" onClick={() => run(gateway.clearDownloads())}>
                {t("downloads.clearFinished")}
              </button>
            )}
          </div>
          <AnimatePresence initial={false}>
            {downloadGroups(downloads).map((group) => {
              const { current, items, lead } = group;
              const parts = items.length;
              const remaining = group.size - group.received;
              const rate = group.state === "running" && current.bytesPerSecond > 0 ? current.bytesPerSecond : 0;
              const eta = rate ? remainingLabel(t, remaining, rate) : null;
              const values = { received: sizeLabel(group.received), total: sizeLabel(group.size) };
              // A set found whole in the folder says so; one found in part was still saved there.
              const status = downloadStatusText(t, { ...current, state: group.state, verified: group.verified, existing: group.existing });
              const problem = items.find((item) => item.state === "failed" && item.notice)?.notice ?? null;
              const name = parts > 1 ? lead.set?.split("/").pop() ?? lead.fileName : lead.fileName;
              // A set's buttons act on each of its parts in turn.
              const each = (states: DownloadState[], command: (id: string) => Promise<LibraryView>) =>
                run(items
                  .filter((item) => states.includes(item.state))
                  .reduce<Promise<LibraryView>>((previous, item) => previous.then(() => command(item.id)), Promise.resolve(view)));
              return (
                <motion.div
                  className="download-row"
                  data-state={group.state}
                  key={group.key}
                  layout
                  initial={{ opacity: 0, height: 0 }}
                  animate={{ opacity: 1, height: "auto" }}
                  exit={{ opacity: 0, height: 0 }}
                  transition={{ duration: 0.2 }}
                >
                  <div className="download-main">
                    <div className="file-name">
                      {lead.workId && <span className="work-code">{lead.workId}</span>}
                      {lead.title || name}
                    </div>
                    <div className="file-meta" title={parts > 1 ? items.map((item) => item.fileName).join("\n") : undefined}>
                      {name}
                      {parts > 1 ? ` · ${count("count.parts", parts)}` : ""} → {lead.directory}
                    </div>
                    {group.state !== "done" && (
                      <progress
                        max={Math.max(1, group.size)}
                        value={group.received}
                        aria-label={t("downloads.progressLabel", { name })}
                        aria-valuetext={t("downloads.sizeOf", values)}
                      />
                    )}
                    {problem && group.state === "failed" && (
                      <div className="download-problem">{libraryNoticeText(t, problem)}</div>
                    )}
                  </div>
                  <div className="download-status">
                    <span>
                      {status}
                      {parts > 1 && group.state !== "done" ? ` · ${t("downloads.partOf", { part: group.part, total: parts })}` : ""}
                    </span>
                    <span className="muted">
                      {group.state === "done" ? sizeLabel(group.size) : t("downloads.sizeOf", values)}
                      {rate ? ` · ${sizeLabel(rate)}/s` : ""}
                      {eta ? ` · ${eta}` : ""}
                    </span>
                  </div>
                  <div className="download-actions">
                    {(group.state === "running" || group.state === "queued") && (
                      <button type="button" className="secondary compact" onClick={() => each(["running", "queued"], gateway.pauseDownload)}>
                        <Pause size={14} aria-hidden="true" /> {t("downloads.pause")}
                      </button>
                    )}
                    {(group.state === "paused" || group.state === "failed") && (
                      <button type="button" className="secondary compact" onClick={() => each(["paused", "failed"], gateway.resumeDownload)}>
                        {group.state === "failed" ? <RotateCw size={14} aria-hidden="true" /> : <Play size={14} aria-hidden="true" />}{" "}
                        {group.state === "failed" ? t("downloads.retry") : t("downloads.resume")}
                      </button>
                    )}
                    {group.state === "done" && (
                      <button type="button" className="secondary compact" onClick={() => void gateway.showDownload(lead.id)}>
                        <FolderOpen size={14} aria-hidden="true" /> {t("downloads.showInFolder")}
                      </button>
                    )}
                    <button
                      type="button"
                      className="icon-button small"
                      aria-label={
                        group.state === "done"
                          ? t("downloads.removeLabel", { name })
                          : t("downloads.cancelLabel", { name })
                      }
                      title={group.state === "done" ? t("downloads.removeTitle") : t("downloads.cancelTitle")}
                      onClick={() => each(["queued", "running", "paused", "failed", "done"], gateway.removeDownload)}
                    >
                      <X size={15} aria-hidden="true" />
                    </button>
                  </div>
                </motion.div>
              );
            })}
          </AnimatePresence>
        </section>
      )}
    </>
  );
}
