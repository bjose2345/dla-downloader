import type { MessageKey, Translate } from "../i18n/catalogs";
import { sizeLabel } from "./format";
import type { DownloadView, LibraryNotice, Notice, RouteView } from "../backend/types";

export function noticeText(t: Translate, notice: Notice): string {
  if (notice.code === "server") {
    if (notice.status === 401) return t("notice.server401");
    return notice.detail ?? t("notice.server");
  }
  return t(`notice.${notice.code}` as MessageKey, { detail: notice.detail ?? "" });
}

/** Sign-in problems in words that fit the Welcome screen. */
export function signInText(t: Translate, notice: Notice): string {
  switch (notice.code) {
    case "sign_in_rejected":
    case "too_many_attempts":
    case "self_test_account_only":
    case "incompatible":
      return noticeText(t, notice);
    case "unreachable":
      return t("signIn.unreachable");
    case "certificate":
      return t("signIn.certificate");
    default:
      return t("signIn.failed");
  }
}

export type Tone = "ready" | "offline" | "waiting";

export function connectionText(t: Translate, view: RouteView | null): { text: string; tone: Tone } {
  switch (view?.state) {
    case "self_test":
      return { text: t("connection.selfTest"), tone: "ready" };
    case "connected":
      return { text: t("connection.connected"), tone: "ready" };
    case "unreachable":
      return { text: t("connection.unreachable"), tone: "offline" };
    case "restarting":
      return { text: t("connection.restarting", { seconds: view.seconds }), tone: "offline" };
    case "unavailable":
      return { text: t("connection.unavailable"), tone: "offline" };
    default:
      return { text: t("connection.connecting"), tone: "waiting" };
  }
}

export function libraryNoticeText(t: Translate, notice: LibraryNotice): string {
  const detail = notice.detail ?? "";
  if (notice.code === "server") {
    return notice.status === 429 ? t("libraryNotice.server429") : detail || t("libraryNotice.server");
  }
  if (notice.code === "already_queued") {
    return detail === "1" ? t("libraryNotice.already_queued_one") : t("libraryNotice.already_queued", { count: detail });
  }
  return t(`libraryNotice.${notice.code}` as MessageKey, { detail });
}

export function downloadStatusText(t: Translate, download: DownloadView): string {
  switch (download.state) {
    case "done":
      if (download.existing) return t("downloadState.alreadyInFolder");
      return t(download.verified ? "downloadState.doneVerified" : "downloadState.doneUnverified");
    case "running":
      switch (download.activity?.kind) {
        case "checking":
          return t("downloadState.checking", { size: sizeLabel(download.activity.checked) });
        case "comparing":
          return t("downloadState.comparing", { size: sizeLabel(download.activity.checked) });
        case "reconnecting":
          return t("downloadState.reconnecting", {
            seconds: download.activity.seconds,
            attempt: download.activity.attempt,
            of: download.activity.of,
          });
        case "restarting":
          return t("downloadState.restarting");
        case "verifying":
          return t("downloadState.verifying");
        default:
          return t("downloadState.downloading");
      }
    default:
      return t(`downloadState.${download.state}` as MessageKey);
  }
}
