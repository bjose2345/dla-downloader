import { CircleAlert, CircleCheck, CloudDownload, RefreshCw, RotateCw } from "lucide-react";

import { relativeTime } from "../shared/format";
import { usePresentation } from "../preferences/PresentationProvider";
import type { MessageKey, Translate } from "../i18n/catalogs";
import type { AppUpdateView } from "../backend/types";

export type UpdateTone = "ok" | "new" | "problem" | "idle";

/** One line saying where the app's own update stands. */
export function appUpdateStatus(t: Translate, update: AppUpdateView, now: number, locale: string): { tone: UpdateTone; text: string } {
  const version = update.latest ?? "";
  if (update.installing) return { tone: "new", text: t("appUpdate.installing") };
  if (update.checking) return { tone: "idle", text: t("appUpdate.checking") };
  if (update.problem) {
    return { tone: "problem", text: t(`appUpdate.problem.${update.problem.code}` as MessageKey, { detail: update.problem.detail ?? "" }) };
  }
  if (update.ready) return { tone: "new", text: t("appUpdate.ready", { version }) };
  if (update.downloading) {
    const { received, total } = update.downloading;
    const percent = total > 0 ? Math.floor((received / total) * 100) : 0;
    return { tone: "new", text: t("appUpdate.downloading", { version, percent }) };
  }
  if (update.latest && update.unavailable) return { tone: "new", text: t("appUpdate.unavailable", { version }) };
  if (update.latest) {
    const available = t("appUpdate.available", { version });
    return { tone: "new", text: update.signedIn ? available : `${available} ${t("appUpdate.signInToDownload")}` };
  }
  if (update.lastChecked) {
    const when = relativeTime(update.lastChecked, now, locale);
    return { tone: "ok", text: `${t("appUpdate.upToDate")} · ${t("update.checked", { when })}` };
  }
  return { tone: "idle", text: t("appUpdate.signIn") };
}

interface Props {
  update: AppUpdateView;
  onCheck: () => void;
  onInstall: () => void;
}

/** Settings › About: the app's own update, its notes and "Restart to update". */
export function AppUpdateSection({ update, onCheck, onInstall }: Props) {
  const { t, locale } = usePresentation();
  const { tone, text } = appUpdateStatus(t, update, Date.now() / 1000, locale);
  const canInstall = update.ready && update.installable;
  return (
    <section className="app-update">
      <h4>{t("appUpdate.title")}</h4>
      <div className={"update-status " + tone} role="status">
        <StatusIcon tone={tone} spinning={update.checking} />
        <span>{text}</span>
      </div>
      {update.downloading && <progress max={update.downloading.total || 1} value={update.downloading.received} />}
      {update.latest && update.notes && (
        <p className="app-update-notes">
          <strong>{t("appUpdate.whatsNew")}</strong>
          {update.notes}
        </p>
      )}
      {update.ready && !update.installable && <p className="app-update-manual">{t("appUpdate.manual")}</p>}
      <div className="update-actions">
        {canInstall && (
          <button type="button" className="primary compact" disabled={update.installing} onClick={onInstall}>
            <RotateCw size={15} aria-hidden="true" />
            {t("appUpdate.restart")}
          </button>
        )}
        <button type="button" className="secondary compact" disabled={!update.signedIn || update.checking || update.installing} onClick={onCheck}>
          <RefreshCw size={15} aria-hidden="true" />
          {t("update.checkNow")}
        </button>
      </div>
    </section>
  );
}

/**
 * Above every screen while this version is below the newest release's
 * minimum: new uploads and downloads wait until it is updated.
 */
export function UpdateRequiredBanner({ update, onInstall }: { update: AppUpdateView; onInstall: () => void }) {
  const { t, locale } = usePresentation();
  const canInstall = update.ready && update.installable;
  const { text } = appUpdateStatus(t, update, Date.now() / 1000, locale);
  return (
    <div className="update-banner" role="alert">
      <div>
        <strong>{t("appUpdate.requiredTitle")}</strong>
        <span>{t("appUpdate.required")}</span>
        <span className="update-banner-status">{update.ready && !update.installable ? t("appUpdate.manual") : text}</span>
      </div>
      {canInstall && (
        <button type="button" className="primary compact" disabled={update.installing} onClick={onInstall}>
          <RotateCw size={15} aria-hidden="true" />
          {t("appUpdate.restart")}
        </button>
      )}
    </div>
  );
}

function StatusIcon({ tone, spinning }: { tone: UpdateTone; spinning: boolean }) {
  if (spinning) return <RefreshCw size={15} className="spinning" aria-hidden="true" />;
  if (tone === "problem") return <CircleAlert size={15} aria-hidden="true" />;
  if (tone === "new") return <CloudDownload size={15} aria-hidden="true" />;
  return <CircleCheck size={15} aria-hidden="true" />;
}
