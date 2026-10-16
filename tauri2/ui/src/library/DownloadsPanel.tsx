import { ArrowDownToLine, FolderOpen, Pause, Play, RotateCw, X } from "lucide-react";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";

import { downloadStatusText, libraryNoticeText } from "../shared/copy";
import { remainingLabel, sizeLabel } from "../shared/format";
import { gateway } from "../backend/gateway";
import { summaryText } from "./library";
import { usePresentation } from "../preferences/PresentationProvider";
import type { LibraryView } from "../backend/types";

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
            {downloads.map((download) => {
              const remaining = download.size - download.received;
              const rate = download.state === "running" && download.bytesPerSecond > 0 ? download.bytesPerSecond : 0;
              const eta = rate ? remainingLabel(t, remaining, rate) : null;
              const values = { received: sizeLabel(download.received), total: sizeLabel(download.size) };
              return (
                <motion.div
                  className="download-row"
                  data-state={download.state}
                  key={download.id}
                  layout
                  initial={{ opacity: 0, height: 0 }}
                  animate={{ opacity: 1, height: "auto" }}
                  exit={{ opacity: 0, height: 0 }}
                  transition={{ duration: 0.2 }}
                >
                  <div className="download-main">
                    <div className="file-name">
                      {download.workId && <span className="work-code">{download.workId}</span>}
                      {download.title || download.fileName}
                    </div>
                    <div className="file-meta">
                      {download.fileName} → {download.directory}
                    </div>
                    {download.state !== "done" && (
                      <progress
                        max={Math.max(1, download.size)}
                        value={download.received}
                        aria-label={t("downloads.progressLabel", { name: download.fileName })}
                        aria-valuetext={t("downloads.sizeOf", values)}
                      />
                    )}
                    {download.notice && download.state === "failed" && (
                      <div className="download-problem">{libraryNoticeText(t, download.notice)}</div>
                    )}
                  </div>
                  <div className="download-status">
                    <span>{downloadStatusText(t, download)}</span>
                    <span className="muted">
                      {download.state === "done" ? sizeLabel(download.size) : t("downloads.sizeOf", values)}
                      {rate ? ` · ${sizeLabel(rate)}/s` : ""}
                      {eta ? ` · ${eta}` : ""}
                    </span>
                  </div>
                  <div className="download-actions">
                    {(download.state === "running" || download.state === "queued") && (
                      <button type="button" className="secondary compact" onClick={() => run(gateway.pauseDownload(download.id))}>
                        <Pause size={14} aria-hidden="true" /> {t("downloads.pause")}
                      </button>
                    )}
                    {(download.state === "paused" || download.state === "failed") && (
                      <button type="button" className="secondary compact" onClick={() => run(gateway.resumeDownload(download.id))}>
                        {download.state === "failed" ? <RotateCw size={14} aria-hidden="true" /> : <Play size={14} aria-hidden="true" />}{" "}
                        {download.state === "failed" ? t("downloads.retry") : t("downloads.resume")}
                      </button>
                    )}
                    {download.state === "done" && (
                      <button type="button" className="secondary compact" onClick={() => void gateway.showDownload(download.id)}>
                        <FolderOpen size={14} aria-hidden="true" /> {t("downloads.showInFolder")}
                      </button>
                    )}
                    <button
                      type="button"
                      className="icon-button small"
                      aria-label={
                        download.state === "done"
                          ? t("downloads.removeLabel", { name: download.fileName })
                          : t("downloads.cancelLabel", { name: download.fileName })
                      }
                      title={download.state === "done" ? t("downloads.removeTitle") : t("downloads.cancelTitle")}
                      onClick={() => run(gateway.removeDownload(download.id))}
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
