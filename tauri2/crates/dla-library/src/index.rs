use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    thread,
    time::Duration,
};

use dla_remote::{Fetched, RemoteEntry};
use dla_remote::{RemoteError, RemoteErrorKind};
use serde::{Deserialize, Serialize};

use crate::{
    ARCHIVE_ROOT, Archive,
    catalog::{Catalog, CatalogEntry, parse_manifest},
    with_retries,
};

pub const MANIFEST_NAME: &str = "_manifest.json";
// 2: entries carry update and release dates, so older caches are read again.
// 3: entries carry when dlminer added each file (`added_date`).
const CACHE_VERSION: u32 = 3;
const MAX_MANIFEST_BYTES: u64 = 256 << 20;
// The download service lets an account read one file at a time.
const PARALLEL_FETCHES: usize = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManifestState {
    Present,
    Missing,
    Unreadable,
    /// Listed but not fetched yet, because a refresh stopped early.
    Unchecked,
}

#[derive(Clone, Serialize, Deserialize)]
struct CachedFolder {
    modified: Option<String>,
    etag: Option<String>,
    state: ManifestState,
    entries: Vec<CatalogEntry>,
}

#[derive(Default, Serialize, Deserialize)]
struct CacheFile {
    version: u32,
    folders: BTreeMap<String, CachedFolder>,
}

#[derive(Debug, Clone)]
pub struct RefreshOptions {
    /// Revalidates every manifest by ETag instead of trusting unchanged
    /// folder times, which miss manifests rewritten in place.
    pub thorough: bool,
    pub retry_delays: Vec<Duration>,
}

impl Default for RefreshOptions {
    fn default() -> Self {
        Self {
            thorough: false,
            retry_delays: [1, 2, 4].map(Duration::from_secs).to_vec(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct IndexProgress {
    pub checked: usize,
    pub total: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RefreshSummary {
    pub folders: usize,
    pub files: usize,
    /// Manifests downloaded because they were new or changed.
    pub fetched: usize,
    pub missing: Vec<String>,
    pub unreadable: Vec<String>,
    /// False when the refresh was stopped before every folder was checked.
    pub complete: bool,
}

/// The on-disk catalog cache: every folder's manifest entries plus what is
/// needed to tell cheaply whether the folder changed since.
pub struct IndexStore {
    path: Option<PathBuf>,
    cache: CacheFile,
}

impl IndexStore {
    /// Loads the cache, treating a missing, damaged or older file as empty.
    pub fn open(path: Option<PathBuf>) -> Self {
        let cache = path
            .as_deref()
            .and_then(|path| fs::read(path).ok())
            .and_then(|data| serde_json::from_slice::<CacheFile>(&data).ok())
            .filter(|cache| cache.version == CACHE_VERSION)
            .unwrap_or_default();
        Self { path, cache }
    }

    pub fn is_empty(&self) -> bool {
        self.cache.folders.is_empty()
    }

    pub fn catalog(&self) -> Catalog {
        Catalog::new(
            self.cache.folders.keys().cloned(),
            self.cache
                .folders
                .values()
                .flat_map(|folder| folder.entries.iter().cloned())
                .collect(),
        )
        .without_manifest(
            self.cache
                .folders
                .iter()
                .filter(|(_, folder)| {
                    matches!(
                        folder.state,
                        ManifestState::Missing | ManifestState::Unreadable
                    )
                })
                .map(|(name, _)| name.clone()),
        )
    }

    /// Brings the cache up to date with the archive. Whatever was checked
    /// is kept even when the refresh fails or stops part way.
    pub fn refresh(
        &mut self,
        archive: &dyn Archive,
        options: &RefreshOptions,
        progress: &(dyn Fn(IndexProgress) + Sync),
        stop: &(dyn Fn() -> bool + Sync),
    ) -> Result<RefreshSummary, RemoteError> {
        let quiet = |_, _, _| {};
        let Some(listing) = with_retries(&options.retry_delays, stop, &quiet, || {
            archive.list(ARCHIVE_ROOT)
        })?
        else {
            return Ok(self.summary(0, false));
        };
        let mut next = BTreeMap::new();
        let mut stale = Vec::new();
        for folder in listing
            .into_iter()
            .filter(|entry| entry.is_dir && !entry.name.starts_with('.'))
        {
            let cached = self.cache.folders.remove(&folder.name);
            match cached {
                Some(cached)
                    if !options.thorough
                        && folder.modified.is_some()
                        && cached.modified == folder.modified
                        && cached.state != ManifestState::Unchecked =>
                {
                    next.insert(folder.name, cached);
                }
                cached => stale.push((folder, cached)),
            }
        }

        let total = stale.len();
        let queue = Mutex::new(stale.into_iter());
        let checked_folders = Mutex::new(Vec::new());
        let failure: Mutex<Option<RemoteError>> = Mutex::new(None);
        let checked = AtomicUsize::new(0);
        let downloaded = AtomicUsize::new(0);
        let halted = || stop() || failure.lock().expect("refresh lock").is_some();
        progress(IndexProgress { checked: 0, total });
        thread::scope(|scope| {
            for _ in 0..PARALLEL_FETCHES.min(total) {
                scope.spawn(|| {
                    while !halted() {
                        let Some((folder, cached)) = queue.lock().expect("refresh lock").next()
                        else {
                            return;
                        };
                        let result =
                            fetch_folder(archive, &folder, cached.as_ref(), options, &halted);
                        let checked_folder = match result {
                            Ok(Some((checked_folder, fetched))) => {
                                downloaded.fetch_add(usize::from(fetched), Ordering::SeqCst);
                                checked_folder
                            }
                            Ok(None) => unchecked(cached),
                            Err(error) => {
                                failure.lock().expect("refresh lock").get_or_insert(error);
                                unchecked(cached)
                            }
                        };
                        checked_folders
                            .lock()
                            .expect("refresh lock")
                            .push((folder.name, checked_folder));
                        progress(IndexProgress {
                            checked: checked.fetch_add(1, Ordering::SeqCst) + 1,
                            total,
                        });
                    }
                });
            }
        });

        next.extend(checked_folders.into_inner().expect("refresh lock"));
        // Folders never reached keep their old entries and are checked again
        // next time.
        for (folder, cached) in queue.into_inner().expect("refresh lock") {
            next.insert(folder.name, unchecked(cached));
        }
        let complete = next
            .values()
            .all(|folder| folder.state != ManifestState::Unchecked);
        self.cache = CacheFile {
            version: CACHE_VERSION,
            folders: next,
        };
        // Best effort: without the cache the next start fetches everything.
        let _ = self.save();
        match failure.into_inner().expect("refresh lock") {
            Some(error) => Err(error),
            None => Ok(self.summary(downloaded.into_inner(), complete)),
        }
    }

    fn summary(&self, fetched: usize, complete: bool) -> RefreshSummary {
        let named = |state| {
            self.cache
                .folders
                .iter()
                .filter(|(_, folder)| folder.state == state)
                .map(|(name, _)| name.clone())
                .collect()
        };
        RefreshSummary {
            folders: self.cache.folders.len(),
            files: self
                .cache
                .folders
                .values()
                .map(|folder| folder.entries.len())
                .sum(),
            fetched,
            missing: named(ManifestState::Missing),
            unreadable: named(ManifestState::Unreadable),
            complete,
        }
    }

    fn save(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        write_replacing(path, &serde_json::to_vec(&self.cache)?)
    }
}

fn unchecked(cached: Option<CachedFolder>) -> CachedFolder {
    let entries = cached.map(|cached| cached.entries).unwrap_or_default();
    CachedFolder {
        modified: None,
        etag: None,
        state: ManifestState::Unchecked,
        entries,
    }
}

/// Fetches one folder's manifest, reusing the cached entries when the
/// server confirms it is unchanged, and tells whether a manifest was
/// downloaded. `Ok(None)` means the refresh stopped.
fn fetch_folder(
    archive: &dyn Archive,
    folder: &RemoteEntry,
    cached: Option<&CachedFolder>,
    options: &RefreshOptions,
    stopped: &dyn Fn() -> bool,
) -> Result<Option<(CachedFolder, bool)>, RemoteError> {
    let path = format!("{ARCHIVE_ROOT}/{}/{MANIFEST_NAME}", folder.name);
    let known = cached.filter(|cached| cached.state == ManifestState::Present);
    let etag = known.and_then(|cached| cached.etag.as_deref());
    let quiet = |_, _, _| {};
    let fetched = match with_retries(&options.retry_delays, stopped, &quiet, || {
        archive.fetch(&path, MAX_MANIFEST_BYTES, etag)
    }) {
        Ok(Some(fetched)) => fetched,
        Ok(None) => return Ok(None),
        Err(error) if error.kind == RemoteErrorKind::Status(404) => {
            let missing = CachedFolder {
                modified: folder.modified.clone(),
                etag: None,
                state: ManifestState::Missing,
                entries: Vec::new(),
            };
            return Ok(Some((missing, false)));
        }
        Err(error) => return Err(error),
    };
    let checked = match fetched {
        Fetched::NotModified => {
            let Some(known) = known else {
                return Err(RemoteError::new(
                    RemoteErrorKind::InvalidResponse,
                    "The DLA server returned an invalid response.",
                ));
            };
            return Ok(Some((
                CachedFolder {
                    modified: folder.modified.clone(),
                    ..known.clone()
                },
                false,
            )));
        }
        Fetched::Content { data, etag } => match parse_manifest(&folder.name, &data) {
            Some(entries) => CachedFolder {
                modified: folder.modified.clone(),
                etag,
                state: ManifestState::Present,
                entries,
            },
            None => CachedFolder {
                modified: folder.modified.clone(),
                etag: None,
                state: ManifestState::Unreadable,
                entries: Vec::new(),
            },
        },
    };
    Ok(Some((checked, true)))
}

fn write_replacing(path: &Path, data: &[u8]) -> io::Result<()> {
    use std::io::Write;
    let temporary = path.with_extension("json.new");
    let mut file = fs::File::create(&temporary)?;
    file.write_all(data)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)
}
