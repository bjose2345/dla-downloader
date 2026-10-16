//! Updates for the app itself. After sign-in, and every few hours while
//! signed in, the releases folder is asked for a newer version. Its
//! installer downloads in the background, is checked by SHA-256 and by the
//! DLA update key's signature, and waits for the user to choose "Restart
//! to update". What was found survives a restart, so a verified installer
//! can be used without signing in again.

use std::{
    fs,
    path::PathBuf,
    sync::{Arc, Mutex, MutexGuard},
    thread,
    time::{Duration, Instant},
};

use dla_app_update::{Check, Release, UpdateError};
use dla_library::{DownloadActivity, DownloadError, DownloadOptions, DownloadSink};
use dla_remote::{ArchiveClient, Credentials, Endpoint, PauseSignal};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::system::{unix_seconds, write_private};

pub type AppUpdateNotifier = Arc<dyn Fn(&AppUpdateView) + Send + Sync>;

/// The error a refused upload or download returns; the interface shows the
/// update banner for it.
pub const UPDATE_REQUIRED: &str = "update_required";

const STATE: &str = "update.json";
const DOWNLOADS: &str = "downloads";
/// How often the releases folder is asked while signed in.
const EVERY: u64 = 6 * 3600;
/// A sign-in this soon after a check does not check again.
const SIGN_IN_GAP: u64 = 10 * 60;
/// A manual check again within this time answers from the last one.
const MANUAL_GAP: u64 = 30;
/// After a failure, automatic checks and downloads wait this long.
const RETRY_AFTER: u64 = 30 * 60;
const PROGRESS_EVERY: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AppUpdateProblemCode {
    /// The releases folder could not be reached.
    Offline,
    /// The release list could not be read.
    Invalid,
    /// The download did not match the release list.
    Checksum,
    /// The download was not signed with the DLA update key.
    Signature,
    /// Uploads, downloads or a DAT are still being worked on.
    Busy,
    /// The installer could not be started.
    Install,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppUpdateProblem {
    pub code: AppUpdateProblemCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl AppUpdateProblem {
    fn new(code: AppUpdateProblemCode, detail: Option<String>) -> Self {
        Self { code, detail }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Progress {
    pub received: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateView {
    pub revision: u64,
    /// This build can update itself: it carries the update key, and
    /// installers are published for its platform.
    pub enabled: bool,
    /// This copy can put an update in place: the Windows app, or an AppImage.
    pub installable: bool,
    pub current: String,
    pub signed_in: bool,
    pub checking: bool,
    /// Unix seconds of the last answer from the releases folder.
    pub last_checked: Option<u64>,
    /// A newer version found by the last check.
    pub latest: Option<String>,
    pub notes: Option<String>,
    /// The newer version has no installer for this platform yet.
    pub unavailable: bool,
    /// This version is older than the newest release's minimum: new
    /// uploads and downloads wait until it is updated.
    pub required: bool,
    pub downloading: Option<Progress>,
    /// The newer version is downloaded and verified.
    pub ready: bool,
    pub installing: bool,
    pub problem: Option<AppUpdateProblem>,
}

/// What survives a restart, in `updates/update.json`.
#[derive(Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Saved {
    last_checked: Option<u64>,
    latest: Option<String>,
    required: bool,
    /// The newer version's installer for this platform.
    release: Option<Release>,
    /// That installer, downloaded and verified.
    installer: Option<PathBuf>,
}

struct Inner {
    revision: u64,
    saved: Saved,
    client: Option<Arc<ArchiveClient>>,
    checking: bool,
    downloading: Option<(Progress, PauseSignal)>,
    installing: bool,
    problem: Option<AppUpdateProblem>,
    /// No automatic check or download before this time (unix seconds).
    not_before: u64,
}

pub struct AppUpdater {
    endpoint: Endpoint,
    directory: Option<PathBuf>,
    key: Option<&'static str>,
    target: Option<&'static str>,
    current: String,
    /// The running AppImage on Linux, which an update replaces.
    app_image: Option<PathBuf>,
    notify: AppUpdateNotifier,
    inner: Mutex<Inner>,
}

impl AppUpdater {
    /// `key` is the public update key built into the app, if this build has
    /// one; `current` is this app's version, and `app_image` the running
    /// AppImage on Linux.
    pub fn new(
        endpoint: Endpoint,
        data_dir: Option<PathBuf>,
        current: String,
        key: Option<&'static str>,
        app_image: Option<PathBuf>,
        notify: AppUpdateNotifier,
    ) -> Self {
        let directory = data_dir.map(|directory| directory.join("updates"));
        let mut saved: Saved = directory
            .as_ref()
            .and_then(|directory| fs::read(directory.join(STATE)).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default();
        // Installed since: what was found is this version or older now. Its
        // installer may still be running (it starts this app when it is
        // done), so the download is removed later, by `tidy`.
        if saved
            .latest
            .as_deref()
            .is_some_and(|latest| !dla_app_update::is_newer(latest, &current))
        {
            saved = Saved {
                last_checked: saved.last_checked,
                ..Saved::default()
            };
            if let (Some(directory), Ok(bytes)) = (&directory, serde_json::to_vec_pretty(&saved)) {
                let _ = write_private(&directory.join(STATE), &bytes);
            }
        }
        Self {
            endpoint,
            directory,
            key,
            target: dla_app_update::target(),
            current,
            app_image,
            notify,
            inner: Mutex::new(Inner {
                revision: 0,
                saved,
                client: None,
                checking: false,
                downloading: None,
                installing: false,
                problem: None,
                not_before: 0,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().expect("update lock")
    }

    fn enabled(&self) -> bool {
        self.key.is_some() && self.target.is_some() && self.directory.is_some()
    }

    fn installable(&self) -> bool {
        cfg!(target_os = "windows") || self.app_image.as_deref().is_some_and(|path| path.is_file())
    }

    fn view_of(&self, inner: &Inner) -> AppUpdateView {
        let saved = &inner.saved;
        AppUpdateView {
            revision: inner.revision,
            enabled: self.enabled(),
            installable: self.installable(),
            current: self.current.clone(),
            signed_in: inner.client.is_some(),
            checking: inner.checking,
            last_checked: saved.last_checked,
            latest: saved.latest.clone(),
            notes: saved.release.as_ref().map(|release| release.notes.clone()),
            unavailable: saved.latest.is_some() && saved.release.is_none(),
            required: saved.required,
            downloading: inner.downloading.as_ref().map(|(progress, _)| *progress),
            ready: saved.installer.is_some(),
            installing: inner.installing,
            problem: inner.problem.clone(),
        }
    }

    pub fn view(&self) -> AppUpdateView {
        self.view_of(&self.lock())
    }

    fn changed(&self, mut inner: MutexGuard<'_, Inner>) -> AppUpdateView {
        inner.revision += 1;
        let view = self.view_of(&inner);
        drop(inner);
        (self.notify)(&view);
        view
    }

    fn save(&self, inner: &Inner) {
        if let (Some(directory), Ok(bytes)) =
            (&self.directory, serde_json::to_vec_pretty(&inner.saved))
        {
            let _ = write_private(&directory.join(STATE), &bytes);
        }
    }

    /// New uploads and downloads wait while this version is below the
    /// newest release's minimum.
    pub fn required(&self) -> bool {
        self.lock().saved.required
    }

    /// Called after the upload side verified the account's password.
    pub fn sign_in(self: &Arc<Self>, username: String, password: Zeroizing<String>) {
        if !self.enabled() {
            return;
        }
        let Ok(client) = ArchiveClient::new(&self.endpoint, Credentials { username, password })
        else {
            return;
        };
        let mut inner = self.lock();
        inner.client = Some(Arc::new(client));
        let recent = inner
            .saved
            .last_checked
            .is_some_and(|last| unix_seconds() < last + SIGN_IN_GAP);
        self.changed(inner);
        if recent {
            self.begin_download();
        } else {
            self.begin_check(false);
        }
    }

    pub fn sign_out(&self) {
        let mut inner = self.lock();
        inner.client = None;
        if let Some((_, pause)) = &inner.downloading {
            pause.request();
        }
        self.changed(inner);
    }

    /// Asks the releases folder now, unless it was asked a moment ago.
    pub fn check(self: &Arc<Self>) -> AppUpdateView {
        self.begin_check(true)
    }

    fn begin_check(self: &Arc<Self>, manual: bool) -> AppUpdateView {
        let mut inner = self.lock();
        let recent = inner
            .saved
            .last_checked
            .is_some_and(|last| unix_seconds() < last + MANUAL_GAP);
        let Some(client) = inner.client.clone() else {
            return self.view_of(&inner);
        };
        if !self.enabled() || inner.checking || inner.installing || (manual && recent) {
            return self.view_of(&inner);
        }
        inner.checking = true;
        if manual {
            inner.problem = None;
        }
        let view = self.changed(inner);
        let updater = Arc::clone(self);
        thread::spawn(move || updater.run_check(&client));
        view
    }

    fn run_check(self: &Arc<Self>, client: &ArchiveClient) {
        let Some(target) = self.target else {
            return;
        };
        let result = dla_app_update::check(client, &self.current, target);
        let mut inner = self.lock();
        inner.checking = false;
        let now = unix_seconds();
        match result {
            Ok(found) => {
                inner.saved.last_checked = Some(now);
                inner.problem = None;
                let (latest, required, release) = match found {
                    Check::NoReleases | Check::Current { .. } => (None, false, None),
                    Check::Unavailable { latest, required } => (Some(latest), required, None),
                    Check::Available { release, required } => {
                        (Some(release.version.clone()), required, Some(release))
                    }
                };
                if release != inner.saved.release {
                    // Another release (or none): what was downloaded is stale.
                    if let Some((_, pause)) = &inner.downloading {
                        pause.request();
                    }
                    inner.saved.installer = None;
                }
                inner.saved.latest = latest;
                inner.saved.required = required;
                inner.saved.release = release;
            }
            Err(error) => {
                inner.not_before = now + RETRY_AFTER;
                inner.problem = Some(problem(&error));
            }
        }
        self.save(&inner);
        self.changed(inner);
        self.begin_download();
    }

    /// Downloads the known newer version, unless it is here already.
    fn begin_download(self: &Arc<Self>) {
        let mut inner = self.lock();
        let (Some(client), Some(release)) = (inner.client.clone(), inner.saved.release.clone())
        else {
            return;
        };
        if inner.downloading.is_some() || inner.saved.installer.is_some() || inner.installing {
            return;
        }
        let pause = PauseSignal::default();
        inner.downloading = Some((
            Progress {
                received: 0,
                total: release.size,
            },
            pause.clone(),
        ));
        self.changed(inner);
        let updater = Arc::clone(self);
        thread::spawn(move || updater.run_download(&client, &release, &pause));
    }

    fn run_download(&self, client: &ArchiveClient, release: &Release, pause: &PauseSignal) {
        let (Some(directory), Some(key)) = (&self.directory, self.key) else {
            return;
        };
        let sink = Sink {
            updater: self,
            last: Mutex::new(Instant::now()),
        };
        let result = dla_app_update::download(
            client,
            release,
            &directory.join(DOWNLOADS),
            key,
            &DownloadOptions::default(),
            pause,
            &sink,
        );
        let mut inner = self.lock();
        inner.downloading = None;
        let wanted = inner.saved.release.as_ref() == Some(release);
        match result {
            Ok(path) if wanted => {
                inner.saved.installer = Some(path);
                inner.problem = None;
            }
            Ok(_) | Err(UpdateError::Download(DownloadError::Paused)) => {}
            Err(error) => {
                inner.not_before = unix_seconds() + RETRY_AFTER;
                inner.problem = Some(problem(&error));
            }
        }
        self.save(&inner);
        self.changed(inner);
    }

    /// Removes downloads nothing is waiting for, such as the installer that
    /// just updated this app: Windows keeps it while it still runs, so this
    /// is tried again every minute until it succeeds.
    fn tidy(&self) {
        let pending = {
            let inner = self.lock();
            inner.saved.release.is_some() || inner.downloading.is_some() || inner.installing
        };
        if let (false, Some(directory)) = (pending, &self.directory) {
            let downloads = directory.join(DOWNLOADS);
            if downloads.exists() {
                let _ = fs::remove_dir_all(downloads);
            }
        }
    }

    /// Checks soon after start that a saved installer is still intact, then
    /// asks the releases folder every few hours while signed in.
    pub fn start(self: &Arc<Self>) {
        if !self.enabled() {
            return;
        }
        let updater = Arc::clone(self);
        thread::spawn(move || {
            updater.recheck_installer();
            loop {
                updater.tidy();
                thread::sleep(Duration::from_secs(60));
                let (due, download) = {
                    let inner = updater.lock();
                    let idle = inner.client.is_some()
                        && !inner.checking
                        && inner.downloading.is_none()
                        && !inner.installing
                        && unix_seconds() >= inner.not_before;
                    let last = inner.saved.last_checked.unwrap_or(0);
                    (
                        idle && unix_seconds() >= last + EVERY,
                        idle && inner.saved.release.is_some() && inner.saved.installer.is_none(),
                    )
                };
                if due {
                    updater.begin_check(false);
                } else if download {
                    updater.begin_download();
                }
            }
        });
    }

    fn recheck_installer(&self) {
        let (Some(key), (Some(path), Some(release))) = (self.key, {
            let inner = self.lock();
            (inner.saved.installer.clone(), inner.saved.release.clone())
        }) else {
            return;
        };
        if dla_app_update::is_verified(&path, &release, key) {
            return;
        }
        let mut inner = self.lock();
        inner.saved.installer = None;
        self.save(&inner);
        self.changed(inner);
    }

    /// Hands over the verified installer to run, and marks the update as
    /// under way. Refused while `busy`, or when nothing is ready.
    pub fn begin_install(&self, busy: bool) -> Result<Installer, Box<AppUpdateView>> {
        let mut inner = self.lock();
        let (Some(path), Some(release)) =
            (inner.saved.installer.clone(), inner.saved.release.clone())
        else {
            return Err(Box::new(self.view_of(&inner)));
        };
        if inner.installing || !self.installable() {
            return Err(Box::new(self.view_of(&inner)));
        }
        if busy {
            inner.problem = Some(AppUpdateProblem::new(AppUpdateProblemCode::Busy, None));
            return Err(Box::new(self.changed(inner)));
        }
        // Checked once more right before it runs.
        if !self
            .key
            .is_some_and(|key| dla_app_update::is_verified(&path, &release, key))
        {
            inner.saved.installer = None;
            inner.problem = Some(AppUpdateProblem::new(AppUpdateProblemCode::Signature, None));
            self.save(&inner);
            return Err(Box::new(self.changed(inner)));
        }
        inner.installing = true;
        inner.problem = None;
        self.changed(inner);
        Ok(Installer {
            path,
            app_image: self.app_image.clone(),
        })
    }

    /// The installer could not be started; the app keeps running.
    pub fn install_failed(&self, detail: String) -> AppUpdateView {
        let mut inner = self.lock();
        inner.installing = false;
        inner.problem = Some(AppUpdateProblem::new(
            AppUpdateProblemCode::Install,
            Some(detail),
        ));
        self.changed(inner)
    }
}

/// A verified installer, ready to run.
pub struct Installer {
    pub path: PathBuf,
    /// On Linux, the AppImage to replace with it; Windows runs the installer.
    #[cfg_attr(target_os = "windows", allow(dead_code))]
    pub app_image: Option<PathBuf>,
}

fn problem(error: &UpdateError) -> AppUpdateProblem {
    let code = match error {
        UpdateError::Remote(_) => AppUpdateProblemCode::Offline,
        UpdateError::Manifest(_) => AppUpdateProblemCode::Invalid,
        UpdateError::Signature => AppUpdateProblemCode::Signature,
        UpdateError::Download(
            DownloadError::ChecksumMismatch | DownloadError::SizeMismatch { .. },
        ) => AppUpdateProblemCode::Checksum,
        UpdateError::Download(DownloadError::Remote(_)) => AppUpdateProblemCode::Offline,
        UpdateError::Download(_) | UpdateError::Local(_) => AppUpdateProblemCode::Failed,
    };
    AppUpdateProblem::new(code, Some(error.to_string()))
}

struct Sink<'a> {
    updater: &'a AppUpdater,
    last: Mutex<Instant>,
}

impl DownloadSink for Sink<'_> {
    fn activity(&self, _: DownloadActivity) {}

    fn progress(&self, received: u64, total: u64) {
        {
            let mut last = self.last.lock().expect("progress lock");
            if last.elapsed() < PROGRESS_EVERY && received < total {
                return;
            }
            *last = Instant::now();
        }
        let mut inner = self.updater.lock();
        if let Some((progress, _)) = &mut inner.downloading {
            *progress = Progress { received, total };
        }
        self.updater.changed(inner);
    }

    fn etag(&self, _: Option<&str>) {}
}

#[cfg(test)]
mod tests;
