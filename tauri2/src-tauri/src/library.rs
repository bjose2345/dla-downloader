use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, MutexGuard, RwLock,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Instant,
};

use dla_library::{
    ARCHIVE_ROOT, Catalog, CatalogEntry, DownloadActivity, DownloadError, DownloadJob,
    DownloadOptions, DownloadSink, FolderSummary, IndexProgress, IndexStore, KnownFile, KnownFiles,
    MANIFEST_NAME, Recent, RefreshOptions, RefreshSummary, download, existing_copy, now_seconds,
};
use dla_remote::{ArchiveClient, Credentials, Endpoint, PauseSignal, RemoteError, RemoteErrorKind};
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

use crate::system::write_private;

const SEARCH_LIMIT: usize = 200;
// Beyond this many remembered files, those no longer on disk are forgotten.
const MAX_KNOWN_FILES: usize = 20_000;

pub type LibraryNotifier = Arc<dyn Fn(&LibraryView) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LibraryNoticeCode {
    SignInRequired,
    SignInRejected,
    Forbidden,
    NotFound,
    Unreachable,
    Certificate,
    Server,
    InvalidResponse,
    Local,
    KeptChanging,
    SizeMismatch,
    ChecksumMismatch,
    AlreadyQueued,
    CatalogIncomplete,
}

/// A stable code for the interface to phrase, plus untranslated detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryNotice {
    pub code: LibraryNoticeCode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
}

impl LibraryNotice {
    fn new(code: LibraryNoticeCode) -> Self {
        Self {
            code,
            detail: None,
            status: None,
        }
    }

    fn with_detail(code: LibraryNoticeCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: Some(detail.into()),
            status: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DownloadState {
    Queued,
    Running,
    Paused,
    Failed,
    Done,
}

/// A download as remembered across restarts. Only archive paths and folders
/// the user chose are stored, never credentials.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedItem {
    id: String,
    job: DownloadJob,
    folder: String,
    work_id: String,
    title: String,
    state: DownloadState,
    #[serde(default)]
    saved_path: Option<PathBuf>,
    #[serde(default)]
    verified: bool,
    /// A verified copy was already in the folder, so nothing was downloaded.
    #[serde(default)]
    existing: bool,
    #[serde(default)]
    notice: Option<LibraryNotice>,
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SavedDownloads {
    #[serde(default)]
    last_directory: Option<PathBuf>,
    /// The user's chosen download folder, used without asking when
    /// `ask_every_time` is off.
    #[serde(default)]
    default_directory: Option<PathBuf>,
    #[serde(default = "ask_by_default")]
    ask_every_time: bool,
    #[serde(default)]
    items: Vec<SavedItem>,
}

impl Default for SavedDownloads {
    fn default() -> Self {
        Self {
            last_directory: None,
            default_directory: None,
            ask_every_time: true,
            items: Vec::new(),
        }
    }
}

fn ask_by_default() -> bool {
    true
}

/// Where downloads go, as the Settings panel shows it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadSettings {
    pub directory: Option<String>,
    pub ask_every_time: bool,
}

#[derive(Default)]
struct Rate {
    last: Option<(Instant, u64)>,
    bytes_per_second: f64,
}

impl Rate {
    fn update(&mut self, received: u64) {
        let now = Instant::now();
        match self.last {
            Some((then, before)) if received >= before => {
                let elapsed = now.duration_since(then).as_secs_f64();
                if elapsed >= 0.25 {
                    let current = (received - before) as f64 / elapsed;
                    self.bytes_per_second = if self.bytes_per_second == 0.0 {
                        current
                    } else {
                        0.7 * self.bytes_per_second + 0.3 * current
                    };
                    self.last = Some((now, received));
                }
            }
            _ => self.last = Some((now, received)),
        }
    }
}

struct Item {
    saved: SavedItem,
    received: u64,
    activity: Option<DownloadActivity>,
    rate: Rate,
}

impl Item {
    fn new(saved: SavedItem) -> Self {
        let received = match saved.state {
            DownloadState::Done => saved.job.size,
            _ => fs::metadata(saved.job.part_path())
                .map(|metadata| metadata.len().min(saved.job.size))
                .unwrap_or(0),
        };
        Self {
            saved,
            received,
            activity: None,
            rate: Rate::default(),
        }
    }

    fn view(&self) -> DownloadView {
        let saved = &self.saved;
        DownloadView {
            id: saved.id.clone(),
            file_name: saved.job.file_name.clone(),
            folder: saved.folder.clone(),
            work_id: saved.work_id.clone(),
            title: saved.title.clone(),
            directory: saved.job.directory.display().to_string(),
            size: saved.job.size,
            received: self.received,
            state: saved.state,
            activity: self.activity.clone(),
            bytes_per_second: self.rate.bytes_per_second as u64,
            saved_path: saved
                .saved_path
                .as_ref()
                .map(|path| path.display().to_string()),
            checked: saved.job.sha256.is_some(),
            verified: saved.verified,
            existing: saved.existing,
            notice: saved.notice.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadView {
    pub id: String,
    pub file_name: String,
    pub folder: String,
    pub work_id: String,
    pub title: String,
    pub directory: String,
    pub size: u64,
    pub received: u64,
    pub state: DownloadState,
    pub activity: Option<DownloadActivity>,
    pub bytes_per_second: u64,
    pub saved_path: Option<String>,
    /// The catalog lists a SHA-256 the download will be checked against.
    pub checked: bool,
    pub verified: bool,
    /// A verified copy was already in the folder, so nothing was downloaded.
    pub existing: bool,
    pub notice: Option<LibraryNotice>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryView {
    /// Increases with every change so the interface can ignore stale views.
    pub revision: u64,
    /// Increases when the catalog changed, so folders and searches reload.
    pub catalog_revision: u64,
    pub signed_in: bool,
    pub folders: usize,
    pub files: usize,
    pub refreshing: Option<IndexProgress>,
    pub summary: Option<RefreshSummary>,
    pub notice: Option<LibraryNotice>,
    pub downloads: Vec<DownloadView>,
    pub settings: DownloadSettings,
}

/// One entry of a live folder listing, with what the catalog knows about it.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FolderFile {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified: Option<String>,
    pub work_id: Option<String>,
    pub title: Option<String>,
    pub circle: Option<String>,
    /// The catalog's version and dates for this copy (see `CatalogEntry`).
    pub version: Option<String>,
    pub updated: Option<String>,
    pub released: Option<String>,
    pub checked: bool,
}

/// A file with when it reached the archive, for "Date added" and
/// "Latest added".
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentFile {
    /// The month folder, which follows the work's release date.
    pub folder: String,
    pub name: String,
    pub path: String,
    pub size: u64,
    /// When dlminer added the file to the archive (RFC 3339).
    pub arrived: String,
    pub work_id: Option<String>,
    pub title: Option<String>,
    pub circle: Option<String>,
    pub version: Option<String>,
    pub updated: Option<String>,
    pub released: Option<String>,
    pub checked: bool,
    /// A newer copy of a work the archive already had.
    pub update: bool,
}

/// A time zone offset in minutes, within the ones in use, as seconds.
fn offset_seconds(minutes: i32) -> i64 {
    i64::from(minutes.clamp(-14 * 60, 14 * 60)) * 60
}

fn recent_file(recent: Recent<'_>) -> RecentFile {
    let entry = recent.entry;
    RecentFile {
        folder: entry.folder.clone(),
        name: entry.file_name.clone(),
        path: entry.path.clone(),
        size: entry.size,
        arrived: entry.added.clone(),
        work_id: Some(entry.work_id.clone()),
        title: Some(entry.title.clone()),
        circle: Some(entry.circle.clone()),
        version: Some(entry.version.clone()),
        updated: Some(entry.updated.clone()),
        released: Some(entry.released.clone()),
        checked: entry.sha256.is_some(),
        update: recent.update,
    }
}

/// An archive file the interface asks to download.
#[derive(Debug, Clone, Deserialize)]
pub struct RequestedFile {
    pub path: String,
    pub size: u64,
}

struct Running {
    id: String,
    pause: PauseSignal,
    /// A pause the user asked for; otherwise (sign-out, exit) the download
    /// continues by itself next time.
    by_user: bool,
}

struct Inner {
    revision: u64,
    catalog_revision: u64,
    client: Option<Arc<ArchiveClient>>,
    refreshing: Option<(Option<IndexProgress>, Arc<AtomicBool>)>,
    summary: Option<RefreshSummary>,
    notice: Option<LibraryNotice>,
    last_directory: Option<PathBuf>,
    default_directory: Option<PathBuf>,
    ask_every_time: bool,
    items: Vec<Item>,
    running: Option<Running>,
}

impl Inner {
    fn item(&mut self, id: &str) -> Option<&mut Item> {
        self.items.iter_mut().find(|item| item.saved.id == id)
    }

    fn document(&self) -> SavedDownloads {
        SavedDownloads {
            last_directory: self.last_directory.clone(),
            default_directory: self.default_directory.clone(),
            ask_every_time: self.ask_every_time,
            items: self.items.iter().map(|item| item.saved.clone()).collect(),
        }
    }
}

/// Browsing, searching and downloading the archive for the signed-in account.
pub struct Library {
    endpoint: Endpoint,
    notify: LibraryNotifier,
    downloads_path: Option<PathBuf>,
    options: DownloadOptions,
    index: Mutex<IndexStore>,
    catalog: RwLock<Arc<Catalog>>,
    known: KnownStore,
    inner: Mutex<Inner>,
}

impl Library {
    /// Loads the cached catalog and the remembered downloads. Downloads that
    /// were running when the app closed continue after the next sign-in.
    pub fn new(endpoint: Endpoint, data_dir: Option<PathBuf>, notify: LibraryNotifier) -> Self {
        let kind = if endpoint.is_self_test() {
            "self-test"
        } else {
            "private"
        };
        let index = IndexStore::open(
            data_dir
                .as_ref()
                .map(|directory| directory.join(format!("library/index-{kind}.json"))),
        );
        let downloads_path = data_dir
            .as_ref()
            .map(|directory| directory.join(format!("downloads-{kind}.json")));
        let saved: SavedDownloads = downloads_path
            .as_deref()
            .and_then(|path| fs::read(path).ok())
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        let items = saved
            .items
            .into_iter()
            .map(|mut item| {
                if item.state == DownloadState::Running {
                    item.state = DownloadState::Queued;
                }
                Item::new(item)
            })
            .collect();
        Self {
            endpoint,
            notify,
            downloads_path,
            options: DownloadOptions::default(),
            known: KnownStore::open(
                data_dir
                    .as_ref()
                    .map(|directory| directory.join(format!("known-files-{kind}.json"))),
            ),
            catalog: RwLock::new(Arc::new(index.catalog())),
            index: Mutex::new(index),
            inner: Mutex::new(Inner {
                revision: 0,
                catalog_revision: 0,
                client: None,
                refreshing: None,
                summary: None,
                notice: None,
                last_directory: saved.last_directory,
                default_directory: saved.default_directory,
                ask_every_time: saved.ask_every_time,
                items,
                running: None,
            }),
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().expect("library lock")
    }

    fn catalog(&self) -> Arc<Catalog> {
        Arc::clone(&self.catalog.read().expect("catalog lock"))
    }

    fn view_of(&self, inner: &Inner) -> LibraryView {
        let catalog = self.catalog();
        LibraryView {
            revision: inner.revision,
            catalog_revision: inner.catalog_revision,
            signed_in: inner.client.is_some(),
            folders: catalog.folders().len(),
            files: catalog.len(),
            refreshing: inner.refreshing.as_ref().map(|(progress, _)| {
                progress.unwrap_or(IndexProgress {
                    checked: 0,
                    total: 0,
                })
            }),
            summary: inner.summary.clone(),
            notice: inner.notice.clone(),
            downloads: inner.items.iter().map(Item::view).collect(),
            settings: DownloadSettings {
                directory: inner
                    .default_directory
                    .as_ref()
                    .map(|path| path.display().to_string()),
                ask_every_time: inner.ask_every_time,
            },
        }
    }

    pub fn view(&self) -> LibraryView {
        self.view_of(&self.lock())
    }

    fn changed(&self, mut inner: MutexGuard<'_, Inner>) -> LibraryView {
        inner.revision += 1;
        let view = self.view_of(&inner);
        drop(inner);
        (self.notify)(&view);
        view
    }

    fn persist(&self, inner: &Inner) {
        let Some(path) = &self.downloads_path else {
            return;
        };
        // Best effort: a lost queue only means choosing the files again.
        if let Ok(data) = serde_json::to_vec_pretty(&inner.document()) {
            let _ = write_private(path, &data);
        }
    }

    /// `Ok` with the folder to use without asking, or `Err` with where the
    /// folder picker should start.
    pub fn destination(&self) -> Result<PathBuf, Option<PathBuf>> {
        let inner = self.lock();
        match &inner.default_directory {
            Some(directory) if !inner.ask_every_time && directory.is_dir() => Ok(directory.clone()),
            default => Err(default
                .clone()
                .filter(|directory| directory.is_dir())
                .or_else(|| inner.last_directory.clone())
                .filter(|directory| directory.is_dir())),
        }
    }

    /// Choosing a folder means "save here": asking stops until it is switched
    /// on again. Without a folder the picker is the only way to save.
    pub fn set_default_directory(&self, directory: Option<PathBuf>) -> LibraryView {
        let mut inner = self.lock();
        inner.ask_every_time = directory.is_none();
        inner.default_directory = directory;
        self.persist(&inner);
        self.changed(inner)
    }

    /// Saving without asking needs a default folder.
    pub fn set_ask_every_time(&self, ask: bool) -> LibraryView {
        let mut inner = self.lock();
        inner.ask_every_time = ask || inner.default_directory.is_none();
        self.persist(&inner);
        self.changed(inner)
    }

    pub fn folders(&self) -> Vec<FolderSummary> {
        self.catalog().folders()
    }

    pub fn search(&self, query: &str) -> Vec<CatalogEntry> {
        self.catalog()
            .search(query, SEARCH_LIMIT)
            .into_iter()
            .cloned()
            .collect()
    }

    /// What was added to the archive in the last `days` days (1 to 31),
    /// newest first, as the latest catalog refresh saw it.
    pub fn recent(&self, days: u32) -> Vec<RecentFile> {
        let since = now_seconds() - i64::from(days.clamp(1, 31)) * 86_400;
        self.catalog()
            .arrived_since(since)
            .into_iter()
            .map(recent_file)
            .collect()
    }

    /// How many files were added in each month; `offset` is the viewer's
    /// minutes east of UTC, so months start at their midnight.
    pub fn added_months(&self, offset: i32) -> Vec<FolderSummary> {
        self.catalog().added_months(offset_seconds(offset))
    }

    /// The files added in a `YYYY-MM` month, newest first.
    pub fn added(&self, month: &str, offset: i32) -> Vec<RecentFile> {
        self.catalog()
            .added_in(month, offset_seconds(offset))
            .into_iter()
            .map(recent_file)
            .collect()
    }

    /// Called after the session verified the account's password.
    pub fn sign_in(self: &Arc<Self>, username: String, password: Zeroizing<String>) -> LibraryView {
        let client = match ArchiveClient::new(&self.endpoint, Credentials { username, password }) {
            Ok(client) => Arc::new(client),
            Err(error) => {
                let mut inner = self.lock();
                inner.notice = Some(LibraryNotice::with_detail(
                    LibraryNoticeCode::Certificate,
                    error.to_string(),
                ));
                return self.changed(inner);
            }
        };
        let mut inner = self.lock();
        inner.client = Some(client);
        inner.notice = None;
        self.changed(inner);
        self.refresh(false);
        self.pump();
        self.view()
    }

    pub fn sign_out(&self) -> LibraryView {
        let mut inner = self.lock();
        inner.client = None;
        inner.notice = None;
        if let Some((_, stop)) = &inner.refreshing {
            stop.store(true, Ordering::SeqCst);
        }
        if let Some(running) = &inner.running {
            running.pause.request();
        }
        self.changed(inner)
    }

    /// Stops background work when the app closes; the queue is already saved.
    pub fn shutdown(&self) {
        self.sign_out();
    }

    /// Updates the catalog in the background. A quick refresh fetches only
    /// folders whose time changed; a thorough one revalidates every manifest.
    pub fn refresh(self: &Arc<Self>, thorough: bool) -> LibraryView {
        let mut inner = self.lock();
        let Some(client) = inner.client.clone() else {
            inner.notice = Some(LibraryNotice::new(LibraryNoticeCode::SignInRequired));
            return self.changed(inner);
        };
        if inner.refreshing.is_some() {
            return self.view_of(&inner);
        }
        let stop = Arc::new(AtomicBool::new(false));
        inner.refreshing = Some((None, Arc::clone(&stop)));
        inner.notice = None;
        let view = self.changed(inner);
        let library = Arc::clone(self);
        thread::spawn(move || library.run_refresh(&client, thorough, &stop));
        view
    }

    fn run_refresh(&self, client: &ArchiveClient, thorough: bool, stop: &AtomicBool) {
        let options = RefreshOptions {
            thorough,
            ..RefreshOptions::default()
        };
        let progress = |progress: IndexProgress| {
            let mut inner = self.lock();
            if let Some((current, _)) = &mut inner.refreshing {
                *current = Some(progress);
            }
            self.changed(inner);
        };
        let (result, catalog) = {
            let mut index = self.index.lock().expect("index lock");
            let result =
                index.refresh(client, &options, &progress, &|| stop.load(Ordering::SeqCst));
            (result, index.catalog())
        };
        *self.catalog.write().expect("catalog lock") = Arc::new(catalog);
        let mut inner = self.lock();
        inner.refreshing = None;
        inner.catalog_revision += 1;
        match result {
            Ok(summary) => {
                if !summary.unreadable.is_empty() {
                    inner.notice = Some(LibraryNotice::with_detail(
                        LibraryNoticeCode::CatalogIncomplete,
                        summary.unreadable.join(", "),
                    ));
                }
                inner.summary = Some(summary);
            }
            Err(error) => inner.notice = Some(remote_notice(error)),
        }
        self.changed(inner);
    }

    /// Lists one archive folder live, joined with its catalog entries.
    /// Blocks on the network; call it outside the async runtime.
    pub fn folder(&self, name: &str) -> Result<Vec<FolderFile>, LibraryNotice> {
        let client = self
            .lock()
            .client
            .clone()
            .ok_or(LibraryNotice::new(LibraryNoticeCode::SignInRequired))?;
        if name.is_empty() || name.contains(['/', '\\']) {
            return Err(LibraryNotice::new(LibraryNoticeCode::NotFound));
        }
        let listing = client
            .list(&format!("{ARCHIVE_ROOT}/{name}"))
            .map_err(remote_notice)?;
        let catalog = self.catalog();
        let mut files: Vec<FolderFile> = listing
            .into_iter()
            .filter(|entry| !entry.is_dir && entry.name != MANIFEST_NAME)
            .map(|entry| {
                let known = catalog.entry(name, &entry.name);
                FolderFile {
                    work_id: known.map(|known| known.work_id.clone()),
                    title: known.map(|known| known.title.clone()),
                    circle: known.map(|known| known.circle.clone()),
                    version: known.map(|known| known.version.clone()),
                    updated: known.map(|known| known.updated.clone()),
                    released: known.map(|known| known.released.clone()),
                    checked: known.is_some_and(|known| known.sha256.is_some()),
                    name: entry.name,
                    path: entry.path,
                    size: entry.size,
                    modified: entry.modified,
                }
            })
            .collect();
        files.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(files)
    }

    /// Queues archive files for `directory`, which the user chose.
    pub fn queue(self: &Arc<Self>, directory: PathBuf, files: Vec<RequestedFile>) -> LibraryView {
        let catalog = self.catalog();
        let mut inner = self.lock();
        inner.last_directory = Some(directory.clone());
        let mut duplicates = 0;
        for file in files {
            let Some((folder, name)) = archive_file(&file.path) else {
                continue;
            };
            let known = catalog.entry(folder, name);
            let (size, sha256) = match known {
                Some(known) if known.sha256.is_some() => (known.size, known.sha256.clone()),
                _ => (file.size, None),
            };
            let job = DownloadJob::new(&file.path, &directory, name, size, sha256);
            let queued = inner.items.iter().any(|item| {
                item.saved.state != DownloadState::Done
                    && (item.saved.job.part_path() == job.part_path()
                        || (item.saved.job.remote_path == job.remote_path
                            && item.saved.job.directory == job.directory))
            });
            if queued {
                duplicates += 1;
                continue;
            }
            let item = SavedItem {
                id: new_download_id(),
                folder: folder.to_owned(),
                work_id: known.map(|known| known.work_id.clone()).unwrap_or_default(),
                title: known.map(|known| known.title.clone()).unwrap_or_default(),
                job,
                state: DownloadState::Queued,
                saved_path: None,
                verified: false,
                existing: false,
                notice: None,
            };
            inner.items.push(Item::new(item));
        }
        inner.notice = (duplicates > 0).then(|| {
            LibraryNotice::with_detail(LibraryNoticeCode::AlreadyQueued, duplicates.to_string())
        });
        self.persist(&inner);
        self.changed(inner);
        self.pump();
        self.view()
    }

    pub fn pause(&self, id: &str) -> LibraryView {
        let mut inner = self.lock();
        if let Some(running) = inner.running.as_mut().filter(|running| running.id == id) {
            running.by_user = true;
            running.pause.request();
        } else if let Some(item) = inner
            .item(id)
            .filter(|item| item.saved.state == DownloadState::Queued)
        {
            item.saved.state = DownloadState::Paused;
        }
        self.persist(&inner);
        self.changed(inner)
    }

    pub fn resume(self: &Arc<Self>, id: &str) -> LibraryView {
        let mut inner = self.lock();
        if let Some(item) = inner.item(id).filter(|item| {
            matches!(
                item.saved.state,
                DownloadState::Paused | DownloadState::Failed
            )
        }) {
            item.saved.state = DownloadState::Queued;
            item.saved.notice = None;
        }
        self.persist(&inner);
        self.changed(inner);
        self.pump();
        self.view()
    }

    /// Forgets a download. An unfinished one also loses its part file; a
    /// finished file stays where it was saved.
    pub fn remove(&self, id: &str) -> LibraryView {
        let mut inner = self.lock();
        let Some(index) = inner.items.iter().position(|item| item.saved.id == id) else {
            return self.view_of(&inner);
        };
        let item = inner.items.remove(index);
        match inner.running.as_mut().filter(|running| running.id == id) {
            // The worker deletes the part once it has stopped writing it.
            Some(running) => {
                running.by_user = true;
                running.pause.request();
            }
            None if item.saved.state != DownloadState::Done => {
                let _ = fs::remove_file(item.saved.job.part_path());
            }
            None => {}
        }
        self.persist(&inner);
        self.changed(inner)
    }

    pub fn clear_finished(&self) -> LibraryView {
        let mut inner = self.lock();
        inner
            .items
            .retain(|item| item.saved.state != DownloadState::Done);
        self.persist(&inner);
        self.changed(inner)
    }

    pub fn saved_path(&self, id: &str) -> Option<PathBuf> {
        let mut inner = self.lock();
        let item = inner.item(id)?;
        Some(
            item.saved
                .saved_path
                .clone()
                .unwrap_or_else(|| item.saved.job.directory.clone()),
        )
    }

    /// Starts the next queued download when signed in and none is running.
    fn pump(self: &Arc<Self>) {
        let mut inner = self.lock();
        if inner.running.is_some() {
            return;
        }
        let Some(client) = inner.client.clone() else {
            return;
        };
        let Some(item) = inner
            .items
            .iter_mut()
            .find(|item| item.saved.state == DownloadState::Queued)
        else {
            return;
        };
        item.saved.state = DownloadState::Running;
        item.saved.notice = None;
        item.activity = None;
        item.rate = Rate::default();
        let id = item.saved.id.clone();
        let job = item.saved.job.clone();
        let pause = PauseSignal::default();
        inner.running = Some(Running {
            id: id.clone(),
            pause: pause.clone(),
            by_user: false,
        });
        self.persist(&inner);
        self.changed(inner);
        let library = Arc::clone(self);
        thread::spawn(move || library.run_download(&client, &id, &job, &pause));
    }

    fn run_download(
        self: &Arc<Self>,
        client: &ArchiveClient,
        id: &str,
        job: &DownloadJob,
        pause: &PauseSignal,
    ) {
        let sink = ItemSink { library: self, id };
        let result = match existing_copy(job, &self.known, pause, &sink) {
            Ok(Some(path)) => {
                let _ = fs::remove_file(job.part_path());
                Ok(Finished::Existing(path))
            }
            Ok(None) => download(client, job, &self.options, pause, &sink).map(|done| {
                if let Some(file) = KnownFile::of(&done.path, &done.sha256) {
                    self.known.put(&done.path, file);
                }
                Finished::Downloaded(done)
            }),
            Err(error) => Err(error),
        };
        let mut inner = self.lock();
        let by_user = inner.running.take().is_some_and(|running| running.by_user);
        match inner.item(id) {
            None => {
                // Removed while running.
                if result.is_err() {
                    let _ = fs::remove_file(job.part_path());
                }
            }
            Some(item) => {
                item.activity = None;
                item.rate = Rate::default();
                match result {
                    Ok(finished) => {
                        item.received = item.saved.job.size;
                        item.saved.state = DownloadState::Done;
                        item.saved.notice = None;
                        match finished {
                            Finished::Downloaded(done) => {
                                item.saved.saved_path = Some(done.path);
                                item.saved.verified = done.verified;
                                item.saved.existing = false;
                            }
                            Finished::Existing(path) => {
                                item.saved.saved_path = Some(path);
                                item.saved.verified = true;
                                item.saved.existing = true;
                            }
                        }
                    }
                    Err(DownloadError::Paused) => {
                        item.saved.state = if by_user {
                            DownloadState::Paused
                        } else {
                            DownloadState::Queued
                        };
                    }
                    Err(error) => {
                        if matches!(
                            error,
                            DownloadError::ChecksumMismatch | DownloadError::KeptChanging
                        ) {
                            item.saved.job.etag = None;
                            item.received = 0;
                        }
                        item.saved.state = DownloadState::Failed;
                        item.saved.notice = Some(download_notice(error));
                    }
                }
            }
        }
        self.persist(&inner);
        self.changed(inner);
        self.pump();
    }
}

enum Finished {
    Downloaded(dla_library::Downloaded),
    /// A verified copy was already in the folder.
    Existing(PathBuf),
}

/// The files checks have read in download folders, remembered across
/// restarts so each is read again only after it changed.
struct KnownStore {
    path: Option<PathBuf>,
    files: Mutex<BTreeMap<PathBuf, KnownFile>>,
}

#[derive(Serialize, Deserialize)]
struct StoredKnownFile {
    path: PathBuf,
    #[serde(flatten)]
    file: KnownFile,
}

impl KnownStore {
    fn open(path: Option<PathBuf>) -> Self {
        let stored: Vec<StoredKnownFile> = path
            .as_deref()
            .and_then(|path| fs::read(path).ok())
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default();
        Self {
            path,
            files: Mutex::new(
                stored
                    .into_iter()
                    .map(|stored| (stored.path, stored.file))
                    .collect(),
            ),
        }
    }
}

impl KnownFiles for KnownStore {
    fn get(&self, path: &Path) -> Option<KnownFile> {
        self.files
            .lock()
            .expect("known files lock")
            .get(path)
            .cloned()
    }

    fn put(&self, path: &Path, file: KnownFile) {
        let mut files = self.files.lock().expect("known files lock");
        files.insert(path.to_owned(), file);
        if files.len() > MAX_KNOWN_FILES {
            files.retain(|path, _| path.is_file());
        }
        let Some(store) = &self.path else {
            return;
        };
        let stored: Vec<StoredKnownFile> = files
            .iter()
            .map(|(path, file)| StoredKnownFile {
                path: path.clone(),
                file: file.clone(),
            })
            .collect();
        // Best effort: without it, files are only read once more.
        if let Ok(data) = serde_json::to_vec(&stored) {
            let _ = write_private(store, &data);
        }
    }
}

struct ItemSink<'a> {
    library: &'a Library,
    id: &'a str,
}

impl DownloadSink for ItemSink<'_> {
    fn activity(&self, activity: DownloadActivity) {
        let mut inner = self.library.lock();
        if let Some(item) = inner.item(self.id) {
            item.activity = Some(activity);
        }
        self.library.changed(inner);
    }

    fn progress(&self, received: u64, _total: u64) {
        let mut inner = self.library.lock();
        if let Some(item) = inner.item(self.id) {
            item.received = received;
            item.rate.update(received);
        }
        self.library.changed(inner);
    }

    fn etag(&self, etag: Option<&str>) {
        let mut inner = self.library.lock();
        if let Some(item) = inner.item(self.id) {
            item.saved.job.etag = etag.map(str::to_owned);
        }
        self.library.persist(&inner);
    }
}

/// `/dlsite/<folder>/<name>` → (folder, name); anything else is refused.
fn archive_file(path: &str) -> Option<(&str, &str)> {
    let rest = path.strip_prefix(ARCHIVE_ROOT)?.strip_prefix('/')?;
    let (folder, name) = rest.split_once('/')?;
    let valid = |segment: &str| {
        !segment.is_empty()
            && segment != "."
            && segment != ".."
            && !segment.contains(['/', '\\'])
            && !segment.chars().any(char::is_control)
    };
    (valid(folder) && valid(name) && name != MANIFEST_NAME).then_some((folder, name))
}

fn remote_notice(error: RemoteError) -> LibraryNotice {
    match error.kind {
        RemoteErrorKind::Status(401) => LibraryNotice::new(LibraryNoticeCode::SignInRejected),
        RemoteErrorKind::Status(403) => LibraryNotice::new(LibraryNoticeCode::Forbidden),
        RemoteErrorKind::Status(404) => LibraryNotice::new(LibraryNoticeCode::NotFound),
        RemoteErrorKind::Status(status) => LibraryNotice {
            code: LibraryNoticeCode::Server,
            detail: Some(error.message),
            status: Some(status),
        },
        RemoteErrorKind::Connection => {
            LibraryNotice::with_detail(LibraryNoticeCode::Unreachable, error.message)
        }
        RemoteErrorKind::Certificate => {
            LibraryNotice::with_detail(LibraryNoticeCode::Certificate, error.message)
        }
        RemoteErrorKind::InvalidResponse
        | RemoteErrorKind::Incompatible
        | RemoteErrorKind::Paused => LibraryNotice::new(LibraryNoticeCode::InvalidResponse),
    }
}

fn download_notice(error: DownloadError) -> LibraryNotice {
    match error {
        DownloadError::Remote(error) => remote_notice(error),
        DownloadError::Local(detail) => {
            LibraryNotice::with_detail(LibraryNoticeCode::Local, detail)
        }
        DownloadError::KeptChanging => LibraryNotice::new(LibraryNoticeCode::KeptChanging),
        DownloadError::SizeMismatch { expected, actual } => LibraryNotice::with_detail(
            LibraryNoticeCode::SizeMismatch,
            format!("{actual} / {expected}"),
        ),
        DownloadError::ChecksumMismatch => LibraryNotice::new(LibraryNoticeCode::ChecksumMismatch),
        // Pauses are a state, not a problem; they never reach here.
        DownloadError::Paused => LibraryNotice::new(LibraryNoticeCode::InvalidResponse),
    }
}

/// A random identifier for a queued download.
fn new_download_id() -> String {
    let mut bytes = [0; 24];
    getrandom::fill(&mut bytes).expect("operating system random source");
    hex::encode(bytes)
}

#[cfg(test)]
mod tests {
    use std::{path::Path, time::Duration};

    use sha2::{Digest, Sha256};

    use super::*;
    use crate::go_services::{GoServices, PASSWORD};

    fn open_library(directory: &Path) -> Arc<Library> {
        Arc::new(Library::new(
            Endpoint::self_test("http://127.0.0.1:9/v1").unwrap(),
            Some(directory.to_owned()),
            Arc::new(|_| {}),
        ))
    }

    fn requested(path: &str) -> RequestedFile {
        RequestedFile {
            path: path.to_owned(),
            size: 10,
        }
    }

    #[test]
    fn accepts_only_archive_files() {
        assert_eq!(
            archive_file("/dlsite/2013-01/RJ1__a.zip"),
            Some(("2013-01", "RJ1__a.zip"))
        );
        for path in [
            "/dlsite/2013-01",
            "/dlsite/2013-01/a/b.zip",
            "/dlsite/../x",
            "/upload/2013-01/a.zip",
            "/dlsite/2013-01/_manifest.json",
            "/dlsite/2013-01/..",
        ] {
            assert_eq!(archive_file(path), None, "{path}");
        }
    }

    #[test]
    fn queued_downloads_are_remembered_and_never_duplicated() {
        let state = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let library = open_library(state.path());
        let view = library.queue(
            target.path().to_owned(),
            vec![
                requested("/dlsite/2013-01/RJ1__a.zip"),
                requested("/dlsite/2013-01/RJ1__a.zip"),
                requested("/etc/passwd"),
            ],
        );
        assert_eq!(view.downloads.len(), 1);
        assert_eq!(view.downloads[0].state, DownloadState::Queued);
        assert_eq!(view.notice.unwrap().code, LibraryNoticeCode::AlreadyQueued);
        assert!(!view.signed_in);

        let id = view.downloads[0].id.clone();
        assert_eq!(library.pause(&id).downloads[0].state, DownloadState::Paused);
        let reopened = open_library(state.path());
        let restored = reopened.view();
        assert_eq!(restored.downloads[0].state, DownloadState::Paused);
        assert_eq!(reopened.destination(), Err(Some(target.path().to_owned())));
        assert_eq!(
            reopened.resume(&id).downloads[0].state,
            DownloadState::Queued
        );

        fs::write(target.path().join("RJ1__a.zip.dla-part"), b"12345").unwrap();
        let reopened = open_library(state.path());
        assert_eq!(reopened.view().downloads[0].received, 5);
        assert!(reopened.remove(&id).downloads.is_empty());
        assert!(!target.path().join("RJ1__a.zip.dla-part").exists());
        assert!(open_library(state.path()).view().downloads.is_empty());
    }

    #[test]
    fn choosing_a_default_folder_saves_there_without_asking() {
        let state = tempfile::tempdir().unwrap();
        let target = tempfile::tempdir().unwrap();
        let library = open_library(state.path());
        assert_eq!(library.destination(), Err(None));
        assert!(library.set_ask_every_time(false).settings.ask_every_time);

        let view = library.set_default_directory(Some(target.path().to_owned()));
        assert!(!view.settings.ask_every_time);
        assert_eq!(library.destination(), Ok(target.path().to_owned()));
        // Asking can be switched back on; the folder is then where it starts.
        assert!(library.set_ask_every_time(true).settings.ask_every_time);
        assert_eq!(library.destination(), Err(Some(target.path().to_owned())));
        assert!(!library.set_ask_every_time(false).settings.ask_every_time);
        assert_eq!(library.destination(), Ok(target.path().to_owned()));

        let reopened = open_library(state.path());
        assert_eq!(reopened.destination(), Ok(target.path().to_owned()));
        let view = reopened.set_default_directory(None);
        assert_eq!(
            view.settings,
            DownloadSettings {
                directory: None,
                ask_every_time: true
            }
        );
    }

    #[test]
    fn browsing_requires_sign_in() {
        let state = tempfile::tempdir().unwrap();
        let library = open_library(state.path());
        assert_eq!(
            library.folder("2013-01").unwrap_err().code,
            LibraryNoticeCode::SignInRequired
        );
        assert_eq!(
            library.refresh(true).notice.unwrap().code,
            LibraryNoticeCode::SignInRequired
        );
        assert!(library.search("RJ1").is_empty());
    }

    fn wait_for(library: &Library, done: impl Fn(&LibraryView) -> bool) -> LibraryView {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            let view = library.view();
            if done(&view) {
                return view;
            }
            assert!(Instant::now() < deadline, "gave up waiting: {view:?}");
            thread::sleep(Duration::from_millis(50));
        }
    }

    #[test]
    fn a_verified_copy_already_in_the_folder_is_not_downloaded_again() {
        let archive = tempfile::tempdir().unwrap();
        let month = archive.path().join("2026-10");
        fs::create_dir_all(&month).unwrap();
        let (have, want): (&[u8], &[u8]) = (b"already in the folder", b"only in the archive");
        fs::write(month.join("RJ01000001.zip"), have).unwrap();
        fs::write(month.join("RJ01000002.zip"), want).unwrap();
        let entry = |code: &str, data: &[u8]| {
            serde_json::json!({
                "work_id": code,
                "title": code,
                "size": data.len(),
                "hashes": { "sha256": hex::encode(Sha256::digest(data)) },
            })
        };
        let manifest = serde_json::json!({ "files": {
            "RJ01000001.zip": entry("RJ01000001", have),
            "RJ01000002.zip": entry("RJ01000002", want),
        }});
        fs::write(month.join("_manifest.json"), manifest.to_string()).unwrap();
        let Some(go) = GoServices::start(Some(archive.path()), None) else {
            return;
        };
        let data = tempfile::tempdir().unwrap();
        let library = Arc::new(Library::new(
            go.endpoint(),
            Some(data.path().to_owned()),
            Arc::new(|_| {}),
        ));
        library.sign_in("demo-a".into(), Zeroizing::new(PASSWORD.into()));
        wait_for(&library, |view| {
            view.files == 2 && view.refreshing.is_none()
        });

        let folder = tempfile::tempdir().unwrap();
        let copy = folder.path().join("RJ01000001.zip");
        fs::write(&copy, have).unwrap();
        let before = fs::metadata(&copy).unwrap().modified().unwrap();
        let both = || {
            vec![
                RequestedFile {
                    path: "/dlsite/2026-10/RJ01000001.zip".into(),
                    size: have.len() as u64,
                },
                RequestedFile {
                    path: "/dlsite/2026-10/RJ01000002.zip".into(),
                    size: want.len() as u64,
                },
            ]
        };
        library.queue(folder.path().to_owned(), both());
        let finished = |view: &LibraryView| {
            !view.downloads.is_empty()
                && view
                    .downloads
                    .iter()
                    .all(|download| download.state == DownloadState::Done)
        };
        let view = wait_for(&library, finished);
        let by_name = |view: &LibraryView, name: &str| {
            view.downloads
                .iter()
                .find(|download| download.file_name == name)
                .cloned()
                .unwrap()
        };
        let found = by_name(&view, "RJ01000001.zip");
        assert!(found.existing && found.verified, "{found:?}");
        assert_eq!(found.saved_path, Some(copy.display().to_string()));
        let fetched = by_name(&view, "RJ01000002.zip");
        assert!(!fetched.existing && fetched.verified, "{fetched:?}");
        let mut names: Vec<String> = fs::read_dir(folder.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        assert_eq!(names, ["RJ01000001.zip", "RJ01000002.zip"]);
        assert_eq!(fs::metadata(&copy).unwrap().modified().unwrap(), before);

        // Asked again later, both are already there, remembered across a restart.
        library.clear_finished();
        drop(library);
        let library = Arc::new(Library::new(
            go.endpoint(),
            Some(data.path().to_owned()),
            Arc::new(|_| {}),
        ));
        library.sign_in("demo-a".into(), Zeroizing::new(PASSWORD.into()));
        library.queue(folder.path().to_owned(), both());
        let view = wait_for(&library, finished);
        assert!(view.downloads.iter().all(|download| download.existing));
        assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 2);
    }
}
