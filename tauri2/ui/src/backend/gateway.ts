import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import type {
  AccountOutcome,
  AppInfo,
  AppUpdateView,
  CatalogEntry,
  ConnectionView,
  FolderFile,
  FolderSummary,
  LibraryView,
  RecentFile,
  RequestedFile,
  SessionView,
} from "./types";

export const gateway = {
  appInfo: () => invoke<AppInfo>("app_info"),
  connectionStatus: () => invoke<ConnectionView>("connection_status"),
  session: () => invoke<SessionView>("session"),
  signIn: (username: string, password: string) => invoke<SessionView>("sign_in", { username, password }),
  signOut: () => invoke<SessionView>("sign_out"),
  library: () => invoke<LibraryView>("library"),
  libraryFolders: () => invoke<FolderSummary[]>("library_folders"),
  librarySearch: (query: string) => invoke<CatalogEntry[]>("library_search", { query }),
  libraryFolder: (name: string) => invoke<FolderFile[]>("library_folder", { name }),
  libraryRecent: (days: number) => invoke<RecentFile[]>("library_recent", { days }),
  libraryAddedMonths: (offset: number) => invoke<FolderSummary[]>("library_added_months", { offset }),
  libraryAdded: (month: string, offset: number) => invoke<RecentFile[]>("library_added", { month, offset }),
  libraryRefresh: (thorough: boolean) => invoke<LibraryView>("library_refresh", { thorough }),
  downloadFiles: (files: RequestedFile[]) => invoke<LibraryView>("download_files", { files }),
  pauseDownload: (id: string) => invoke<LibraryView>("pause_download", { id }),
  resumeDownload: (id: string) => invoke<LibraryView>("resume_download", { id }),
  removeDownload: (id: string) => invoke<LibraryView>("remove_download", { id }),
  clearDownloads: () => invoke<LibraryView>("clear_downloads"),
  showDownload: (id: string) => invoke<void>("show_download", { id }),
  chooseDownloadFolder: () => invoke<LibraryView>("choose_download_folder"),
  clearDownloadFolder: () => invoke<LibraryView>("clear_download_folder"),
  setAskEveryTime: (ask: boolean) => invoke<LibraryView>("set_ask_every_time", { ask }),
  setRememberAccount: (remember: boolean) => invoke<AppInfo>("set_remember_account", { remember }),
  /** The interface zoom in percent; setting it returns the level used. */
  windowZoom: () => invoke<number>("window_zoom"),
  setWindowZoom: (percent: number) => invoke<number>("set_window_zoom", { percent }),
  openPasswordPage: () => invoke<void>("open_password_page"),
  changePassword: (current: string, newPassword: string) => invoke<AccountOutcome>("change_password", { current, newPassword }),
  accountEmail: () => invoke<AccountOutcome>("account_email"),
  setAccountEmail: (current: string, email: string) => invoke<AccountOutcome>("set_account_email", { current, email }),
  sendResetCode: (username: string) => invoke<AccountOutcome>("send_reset_code", { username }),
  resetPassword: (username: string, code: string, newPassword: string) =>
    invoke<AccountOutcome>("reset_password", { username, code, newPassword }),
  appUpdate: () => invoke<AppUpdateView>("app_update"),
  appUpdateCheck: () => invoke<AppUpdateView>("app_update_check"),
  /** Ends the app when the update starts; the new version opens by itself. */
  appUpdateInstall: () => invoke<AppUpdateView>("app_update_install"),
  onAppUpdate: (handler: (view: AppUpdateView) => void): Promise<UnlistenFn> =>
    listen<AppUpdateView>("app-update://changed", (event) => handler(event.payload)),
  onLibrary: (handler: (view: LibraryView) => void): Promise<UnlistenFn> =>
    listen<LibraryView>("library://changed", (event) => handler(event.payload)),
  onSession: (handler: (view: SessionView) => void): Promise<UnlistenFn> =>
    listen<SessionView>("session://changed", (event) => handler(event.payload)),
  onConnection: (handler: (view: ConnectionView) => void): Promise<UnlistenFn> =>
    listen<ConnectionView>("connection://changed", (event) => handler(event.payload)),
};
