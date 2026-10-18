export type NoticeCode =
  | "sign_in_rejected"
  | "too_many_attempts"
  | "self_test_account_only"
  | "unreachable"
  | "certificate"
  | "incompatible"
  | "invalid_response"
  | "server";

export interface Notice {
  code: NoticeCode;
  detail?: string;
  status?: number;
}

/** Why an account change did not happen; `code` names a *.problem.* message. */
export interface AccountProblem {
  code: string;
  detail?: string;
}

/** The result of a password, email or recovery request. */
export interface AccountOutcome {
  changed: boolean;
  /** The email address, for the email requests. */
  email?: string;
  problem: AccountProblem | null;
}

export interface SessionView {
  revision: number;
  account: string | null;
  notice: Notice | null;
}

export interface AppInfo {
  version: string;
  selfTest: boolean;
  service: string;
  lastAccount: string | null;
  rememberAccount: boolean;
}

export type RouteView =
  | { state: "self_test" }
  | { state: "starting" }
  | { state: "connecting" }
  | { state: "connected" }
  | { state: "unreachable"; detail: string }
  | { state: "restarting"; seconds: number }
  | { state: "unavailable"; detail: string };

/** The private route to the download service. */
export interface ConnectionView {
  archive: RouteView;
}

export interface IndexProgress {
  checked: number;
  total: number;
}

export interface RefreshSummary {
  folders: number;
  files: number;
  fetched: number;
  missing: string[];
  unreadable: string[];
  complete: boolean;
}

export type LibraryNoticeCode =
  | "sign_in_required"
  | "sign_in_rejected"
  | "forbidden"
  | "not_found"
  | "unreachable"
  | "certificate"
  | "server"
  | "invalid_response"
  | "local"
  | "kept_changing"
  | "size_mismatch"
  | "checksum_mismatch"
  | "already_queued"
  | "catalog_incomplete";

export interface LibraryNotice {
  code: LibraryNoticeCode;
  detail?: string;
  status?: number;
}

export type DownloadState = "queued" | "running" | "paused" | "failed" | "done";

export type DownloadActivity =
  | { kind: "checking"; checked: number }
  | { kind: "receiving" }
  | { kind: "reconnecting"; seconds: number; attempt: number; of: number }
  | { kind: "restarting" }
  | { kind: "verifying" }
  /** Reading a file already in the folder to see whether it is this one. */
  | { kind: "comparing"; checked: number };

export interface DownloadView {
  id: string;
  fileName: string;
  folder: string;
  workId: string;
  title: string;
  directory: string;
  size: number;
  received: number;
  state: DownloadState;
  activity: DownloadActivity | null;
  bytesPerSecond: number;
  savedPath: string | null;
  checked: boolean;
  verified: boolean;
  /** A verified copy was already in the folder, so nothing was downloaded. */
  existing: boolean;
  notice: LibraryNotice | null;
}

export interface LibraryView {
  revision: number;
  catalogRevision: number;
  signedIn: boolean;
  folders: number;
  files: number;
  refreshing: IndexProgress | null;
  summary: RefreshSummary | null;
  notice: LibraryNotice | null;
  downloads: DownloadView[];
  settings: DownloadSettings;
}

export interface DownloadSettings {
  directory: string | null;
  askEveryTime: boolean;
}

export interface FolderSummary {
  name: string;
  files: number;
  /** False while the month has no readable manifest: its files are not catalogued yet. */
  manifest: boolean;
}

export interface FolderFile {
  name: string;
  path: string;
  size: number;
  modified: string | null;
  workId: string | null;
  title: string | null;
  circle: string | null;
  /** From the catalog, as in `CatalogEntry`. */
  version: string | null;
  updated: string | null;
  released: string | null;
  checked: boolean;
}

export interface CatalogEntry {
  folder: string;
  fileName: string;
  path: string;
  workId: string;
  title: string;
  circle: string;
  version: string;
  /** dlminer's update date for this copy, `YYYY-MM-DD`, or empty. */
  updated: string;
  /** The work's release date, `YYYY-MM-DD`, or empty. */
  released: string;
  size: number;
  sha256: string | null;
}

/** A file with when it reached the archive ("Date added" and "Latest added"). */
export interface RecentFile {
  /** The month folder, which follows the work's release date. */
  folder: string;
  name: string;
  path: string;
  size: number;
  /** When dlminer added the file to the archive (RFC 3339). */
  arrived: string;
  workId: string | null;
  title: string | null;
  circle: string | null;
  version: string | null;
  updated: string | null;
  released: string | null;
  checked: boolean;
  /** A newer copy of a work the archive already had. */
  update: boolean;
}

export interface RequestedFile {
  path: string;
  size: number;
}

export type AppUpdateProblemCode = "offline" | "invalid" | "checksum" | "signature" | "busy" | "install" | "failed";

/** Updates for the app itself, from the releases folder. */
export interface AppUpdateView {
  revision: number;
  /** This build can update itself (it carries the update key). */
  enabled: boolean;
  /** This copy can put an update in place: the Windows app, or an AppImage. */
  installable: boolean;
  current: string;
  signedIn: boolean;
  checking: boolean;
  /** Unix seconds of the last answer from the releases folder. */
  lastChecked: number | null;
  /** A newer version found by the last check. */
  latest: string | null;
  notes: string | null;
  /** The newer version has no installer for this platform yet. */
  unavailable: boolean;
  /** New downloads wait until this version is updated. */
  required: boolean;
  downloading: { received: number; total: number } | null;
  /** The newer version is downloaded and verified. */
  ready: boolean;
  installing: boolean;
  problem: { code: AppUpdateProblemCode; detail?: string } | null;
}

/** What a refused download returns while an update is required. */
export const UPDATE_REQUIRED = "update_required";
