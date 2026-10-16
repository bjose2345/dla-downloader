//! Index refresh and download behaviour against an in-memory archive that
//! can change files, drop connections and count requests.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};

use dla_library::{
    Archive, DownloadActivity, DownloadError, DownloadJob, DownloadOptions, DownloadSink,
    IndexStore, KnownFile, KnownFiles, PART_SUFFIX, RefreshOptions, download, existing_copy,
};
use dla_remote::{Fetched, RangeChunk, RemoteEntry};
use dla_remote::{PauseSignal, RemoteError, RemoteErrorKind};
use sha2::{Digest, Sha256};

#[derive(Clone)]
struct File {
    data: Vec<u8>,
    version: u32,
}

#[derive(Default)]
struct State {
    /// Folder name → (modified, files).
    folders: BTreeMap<String, (String, BTreeMap<String, File>)>,
    /// Fail this many of the next requests with a dropped connection.
    drop_next: usize,
    /// Fail this many of the next manifest fetches with a dropped connection.
    fail_fetches: usize,
    /// Change this file after this many more range reads.
    change_after: Option<(String, usize)>,
    /// Pause after this many range reads.
    pause_after: Option<(usize, PauseSignal)>,
}

#[derive(Default)]
struct FakeArchive {
    state: Mutex<State>,
    fetches: AtomicUsize,
    not_modified: AtomicUsize,
    ranges: AtomicUsize,
}

fn dropped() -> RemoteError {
    RemoteError::new(RemoteErrorKind::Connection, "dropped")
}

fn not_found() -> RemoteError {
    RemoteError::new(RemoteErrorKind::Status(404), "Not found")
}

fn etag(file: &File) -> String {
    format!("\"v{}-{}\"", file.version, file.data.len())
}

impl FakeArchive {
    fn put(&self, folder: &str, modified: &str, name: &str, data: &[u8]) {
        let mut state = self.state.lock().unwrap();
        let (stamp, files) = state
            .folders
            .entry(folder.to_owned())
            .or_insert_with(|| (modified.to_owned(), BTreeMap::new()));
        *stamp = modified.to_owned();
        let version = files.get(name).map_or(1, |file| file.version + 1);
        files.insert(
            name.to_owned(),
            File {
                data: data.to_vec(),
                version,
            },
        );
    }

    fn remove_folder(&self, folder: &str) {
        self.state.lock().unwrap().folders.remove(folder);
    }

    fn file(&self, path: &str) -> Option<File> {
        let rest = path.strip_prefix("/dlsite/")?;
        let (folder, name) = rest.split_once('/')?;
        let state = self.state.lock().unwrap();
        state.folders.get(folder)?.1.get(name).cloned()
    }

    fn take_drop(&self) -> bool {
        let mut state = self.state.lock().unwrap();
        if state.drop_next > 0 {
            state.drop_next -= 1;
            return true;
        }
        false
    }
}

impl Archive for FakeArchive {
    fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, RemoteError> {
        if self.take_drop() {
            return Err(dropped());
        }
        assert_eq!(path, "/dlsite");
        let state = self.state.lock().unwrap();
        Ok(state
            .folders
            .iter()
            .map(|(name, (modified, _))| RemoteEntry {
                name: name.clone(),
                path: format!("/dlsite/{name}"),
                is_dir: true,
                size: 0,
                modified: Some(modified.clone()),
                etag: None,
            })
            .collect())
    }

    fn fetch(
        &self,
        path: &str,
        limit: u64,
        if_none_match: Option<&str>,
    ) -> Result<Fetched, RemoteError> {
        if self.take_drop() {
            return Err(dropped());
        }
        self.fetches.fetch_add(1, Ordering::SeqCst);
        {
            let mut state = self.state.lock().unwrap();
            if state.fail_fetches > 0 {
                state.fail_fetches -= 1;
                return Err(dropped());
            }
        }
        let file = self.file(path).ok_or_else(not_found)?;
        if if_none_match == Some(etag(&file).as_str()) {
            self.not_modified.fetch_add(1, Ordering::SeqCst);
            return Ok(Fetched::NotModified);
        }
        if file.data.len() as u64 > limit {
            return Err(dropped());
        }
        Ok(Fetched::Content {
            etag: Some(etag(&file)),
            data: file.data,
        })
    }

    fn read_range(
        &self,
        path: &str,
        offset: u64,
        length: u64,
        wanted: Option<&str>,
    ) -> Result<RangeChunk, RemoteError> {
        if self.take_drop() {
            return Err(dropped());
        }
        let count = self.ranges.fetch_add(1, Ordering::SeqCst) + 1;
        {
            let mut state = self.state.lock().unwrap();
            if let Some((_, signal)) = state
                .pause_after
                .as_ref()
                .filter(|(after, _)| *after == count)
            {
                signal.request();
            }
            if let Some((name, after)) = state.change_after.clone() {
                if after == 0 {
                    state.change_after = None;
                    drop(state);
                    let file = self.file(path).unwrap();
                    let mut data = file.data.clone();
                    data[0] ^= 0xff;
                    self.put("2013-01", "changed", &name, &data);
                } else {
                    state.change_after = Some((name, after - 1));
                }
            }
        }
        let file = self.file(path).ok_or_else(not_found)?;
        let total = file.data.len() as u64;
        if wanted.is_some_and(|wanted| wanted != etag(&file)) || offset >= total {
            return Ok(RangeChunk::Changed);
        }
        let end = (offset + length).min(total);
        Ok(RangeChunk::Data {
            data: file.data[offset as usize..end as usize].to_vec(),
            total,
            etag: Some(etag(&file)),
        })
    }
}

fn manifest(entries: &[(&str, &str, &[u8])]) -> Vec<u8> {
    let files: serde_json::Map<String, serde_json::Value> = entries
        .iter()
        .map(|(name, work, data)| {
            (
                (*name).to_owned(),
                serde_json::json!({
                    "work_id": work, "title": format!("Title of {work}"), "circle": "Circle",
                    "size": data.len(), "hashes": {"sha256": hex::encode(Sha256::digest(data))}
                }),
            )
        })
        .collect();
    serde_json::to_vec(&serde_json::json!({"manifest_info": {}, "files": files})).unwrap()
}

fn quick() -> RefreshOptions {
    RefreshOptions {
        thorough: false,
        retry_delays: vec![Duration::ZERO; 2],
    }
}

fn refresh(store: &mut IndexStore, archive: &FakeArchive, options: &RefreshOptions) {
    store.refresh(archive, options, &|_| {}, &|| false).unwrap();
}

#[test]
fn refresh_fetches_only_changed_folders_and_survives_restarts() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("cache/index.json");
    let archive = FakeArchive::default();
    archive.put(
        "2013-01",
        "t1",
        "_manifest.json",
        &manifest(&[("RJ1__a.zip", "RJ1", b"a")]),
    );
    archive.put(
        "2013-02",
        "t1",
        "_manifest.json",
        &manifest(&[("RJ2__b.zip", "RJ2", b"b")]),
    );
    archive.put("2014-01", "t1", "RJ3__c.zip", b"c");

    let mut store = IndexStore::open(Some(cache.clone()));
    assert!(store.is_empty());
    let summary = store
        .refresh(&archive, &quick(), &|_| {}, &|| false)
        .unwrap();
    assert_eq!((summary.folders, summary.files, summary.fetched), (3, 2, 2));
    assert_eq!(summary.missing, ["2014-01"]);
    assert!(summary.complete);
    assert_eq!(archive.fetches.load(Ordering::SeqCst), 3);
    // A folder without a manifest is shown as not catalogued, not empty.
    let manifests: Vec<_> = store
        .catalog()
        .folders()
        .into_iter()
        .map(|folder| (folder.name, folder.manifest))
        .collect();
    assert_eq!(
        manifests,
        [
            ("2013-01".to_owned(), true),
            ("2013-02".to_owned(), true),
            ("2014-01".to_owned(), false)
        ]
    );

    // Unchanged folder times: nothing is fetched.
    refresh(&mut store, &archive, &quick());
    assert_eq!(archive.fetches.load(Ordering::SeqCst), 3);

    // One folder changed: only its manifest is fetched, and it is parsed.
    archive.put(
        "2013-02",
        "t2",
        "_manifest.json",
        &manifest(&[("RJ2__b.zip", "RJ2", b"b"), ("RJ4__d.zip", "RJ4", b"d")]),
    );
    archive.remove_folder("2014-01");
    let mut reopened = IndexStore::open(Some(cache.clone()));
    refresh(&mut reopened, &archive, &quick());
    assert_eq!(archive.fetches.load(Ordering::SeqCst), 4);
    let catalog = IndexStore::open(Some(cache)).catalog();
    let folders: Vec<_> = catalog
        .folders()
        .into_iter()
        .map(|f| (f.name, f.files))
        .collect();
    assert_eq!(
        folders,
        [("2013-01".to_owned(), 1), ("2013-02".to_owned(), 2)]
    );
    assert_eq!(
        catalog.search("rj4", 5)[0].path,
        "/dlsite/2013-02/RJ4__d.zip"
    );

    // A thorough refresh revalidates every manifest by ETag.
    refresh(
        &mut reopened,
        &archive,
        &RefreshOptions {
            thorough: true,
            ..quick()
        },
    );
    assert_eq!(archive.not_modified.load(Ordering::SeqCst), 2);
}

#[test]
fn refresh_retries_dropped_connections_and_marks_unreadable_manifests() {
    let archive = FakeArchive::default();
    archive.put(
        "2013-01",
        "t1",
        "_manifest.json",
        &manifest(&[("RJ1__a.zip", "RJ1", b"a")]),
    );
    archive.put(
        "2013-02",
        "t1",
        "_manifest.json",
        b"<html>not a manifest</html>",
    );
    archive.state.lock().unwrap().drop_next = 2;
    let mut store = IndexStore::open(None);
    let summary = store
        .refresh(&archive, &quick(), &|_| {}, &|| false)
        .unwrap();
    assert_eq!(summary.unreadable, ["2013-02"]);
    assert_eq!(summary.files, 1);
}

#[test]
fn a_failed_or_stopped_refresh_keeps_what_it_had_and_rechecks_later() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("index.json");
    let archive = FakeArchive::default();
    archive.put(
        "2013-01",
        "t1",
        "_manifest.json",
        &manifest(&[("RJ1__a.zip", "RJ1", b"a")]),
    );
    let mut store = IndexStore::open(Some(cache.clone()));
    refresh(&mut store, &archive, &quick());

    archive.put(
        "2013-01",
        "t2",
        "_manifest.json",
        &manifest(&[("RJ1__a.zip", "RJ1", b"a")]),
    );
    archive.state.lock().unwrap().fail_fetches = 3;
    let error = store
        .refresh(&archive, &quick(), &|_| {}, &|| false)
        .unwrap_err();
    assert_eq!(error.kind, RemoteErrorKind::Connection);
    // The old entries are still searchable.
    assert_eq!(IndexStore::open(Some(cache.clone())).catalog().len(), 1);

    let summary = store
        .refresh(&archive, &quick(), &|_| {}, &|| true)
        .unwrap();
    assert!(!summary.complete);

    let before = archive.fetches.load(Ordering::SeqCst);
    let summary = store
        .refresh(&archive, &quick(), &|_| {}, &|| false)
        .unwrap();
    assert!(summary.complete);
    assert_eq!(archive.fetches.load(Ordering::SeqCst), before + 1);
}

#[derive(Default)]
struct Recorder {
    activities: Mutex<Vec<DownloadActivity>>,
    etag: Mutex<Option<String>>,
    received: AtomicUsize,
}

impl DownloadSink for Recorder {
    fn activity(&self, activity: DownloadActivity) {
        self.activities.lock().unwrap().push(activity);
    }

    fn progress(&self, received: u64, _total: u64) {
        self.received.store(received as usize, Ordering::SeqCst);
    }

    fn etag(&self, etag: Option<&str>) {
        *self.etag.lock().unwrap() = etag.map(str::to_owned);
    }
}

fn options() -> DownloadOptions {
    DownloadOptions {
        chunk_size: 4,
        retry_delays: vec![Duration::ZERO; 3],
        max_restarts: 2,
    }
}

fn work(archive: &FakeArchive, data: &[u8]) -> (tempfile::TempDir, DownloadJob) {
    archive.put("2013-01", "t1", "RJ108375__作品.zip", data);
    let directory = tempfile::tempdir().unwrap();
    let job = DownloadJob::new(
        "/dlsite/2013-01/RJ108375__作品.zip",
        directory.path(),
        "RJ108375__作品.zip",
        data.len() as u64,
        Some(hex::encode(Sha256::digest(data))),
    );
    (directory, job)
}

#[test]
fn downloads_verify_and_never_replace_existing_files() {
    let archive = FakeArchive::default();
    let (directory, job) = work(&archive, b"0123456789abcdef!");
    let existing = directory.path().join("RJ108375__作品.zip");
    fs::write(&existing, b"keep me").unwrap();
    archive.state.lock().unwrap().drop_next = 2;

    let sink = Recorder::default();
    let done = download(&archive, &job, &options(), &PauseSignal::default(), &sink).unwrap();
    assert!(done.verified);
    assert_eq!(done.path, directory.path().join("RJ108375__作品 (1).zip"));
    assert_eq!(fs::read(&done.path).unwrap(), b"0123456789abcdef!");
    assert_eq!(fs::read(&existing).unwrap(), b"keep me");
    assert!(!job.part_path().exists());
    assert_eq!(archive.ranges.load(Ordering::SeqCst), 5);
    let activities = sink.activities.lock().unwrap();
    assert!(activities.contains(&DownloadActivity::Reconnecting {
        seconds: 1,
        attempt: 1,
        of: 3
    }));
    assert_eq!(activities.last(), Some(&DownloadActivity::Verifying));
}

#[test]
fn a_paused_download_resumes_from_its_part_file() {
    let archive = FakeArchive::default();
    let (_directory, mut job) = work(&archive, b"0123456789abcdef!");
    let pause = PauseSignal::default();
    archive.state.lock().unwrap().pause_after = Some((2, pause.clone()));
    let sink = Recorder::default();
    assert_eq!(
        download(&archive, &job, &options(), &pause, &sink),
        Err(DownloadError::Paused)
    );
    assert_eq!(fs::read(job.part_path()).unwrap(), b"01234567");

    // The saved ETag lets the next run continue instead of starting over.
    job.etag = sink.etag.lock().unwrap().clone();
    assert!(job.etag.is_some());
    let before = archive.ranges.load(Ordering::SeqCst);
    let sink = Recorder::default();
    let done = download(&archive, &job, &options(), &PauseSignal::default(), &sink).unwrap();
    assert_eq!(fs::read(done.path).unwrap(), b"0123456789abcdef!");
    assert_eq!(archive.ranges.load(Ordering::SeqCst) - before, 3);
    assert!(
        sink.activities
            .lock()
            .unwrap()
            .contains(&DownloadActivity::Checking { checked: 0 })
    );
}

#[test]
fn a_part_without_a_recorded_version_is_not_trusted() {
    let archive = FakeArchive::default();
    let (_directory, job) = work(&archive, b"0123456789");
    fs::write(job.part_path(), b"XXXX").unwrap();
    let done = download(
        &archive,
        &job,
        &options(),
        &PauseSignal::default(),
        &Recorder::default(),
    )
    .unwrap();
    assert_eq!(fs::read(done.path).unwrap(), b"0123456789");
}

#[test]
fn a_changed_archive_copy_restarts_then_fails_verification() {
    let archive = FakeArchive::default();
    let (directory, job) = work(&archive, b"0123456789abcdef");
    archive.state.lock().unwrap().change_after = Some(("RJ108375__作品.zip".into(), 1));
    let sink = Recorder::default();
    // The manifest still describes the old bytes, so the new copy is refused.
    assert_eq!(
        download(&archive, &job, &options(), &PauseSignal::default(), &sink),
        Err(DownloadError::ChecksumMismatch)
    );
    assert!(
        sink.activities
            .lock()
            .unwrap()
            .contains(&DownloadActivity::Restarting)
    );
    assert!(!job.part_path().exists());
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 0);
}

#[test]
fn files_without_a_catalog_hash_are_saved_unverified() {
    let archive = FakeArchive::default();
    let (_directory, mut job) = work(&archive, b"abc");
    job.sha256 = None;
    let done = download(
        &archive,
        &job,
        &options(),
        &PauseSignal::default(),
        &Recorder::default(),
    )
    .unwrap();
    assert!(!done.verified);
    assert_eq!(done.sha256, hex::encode(Sha256::digest(b"abc")));
}

#[test]
fn empty_files_and_wrong_sizes() {
    let archive = FakeArchive::default();
    let (_directory, job) = work(&archive, b"");
    let done = download(
        &archive,
        &job,
        &options(),
        &PauseSignal::default(),
        &Recorder::default(),
    )
    .unwrap();
    assert_eq!(fs::read(done.path).unwrap(), b"");

    let (_directory, mut job) = work(&archive, b"0123456789");
    job.size = 11;
    assert_eq!(
        download(
            &archive,
            &job,
            &options(),
            &PauseSignal::default(),
            &Recorder::default()
        ),
        Err(DownloadError::SizeMismatch {
            expected: 11,
            actual: 10
        })
    );
    assert!(job.part_path().to_string_lossy().ends_with(PART_SUFFIX));
}

#[derive(Default)]
struct Known(Mutex<BTreeMap<PathBuf, KnownFile>>);

impl KnownFiles for Known {
    fn get(&self, path: &Path) -> Option<KnownFile> {
        self.0.lock().unwrap().get(path).cloned()
    }

    fn put(&self, path: &Path, file: KnownFile) {
        self.0.lock().unwrap().insert(path.to_owned(), file);
    }
}

fn compared(sink: &Recorder) -> bool {
    sink.activities
        .lock()
        .unwrap()
        .iter()
        .any(|activity| matches!(activity, DownloadActivity::Comparing { .. }))
}

#[test]
fn a_verified_copy_in_the_folder_is_found_and_remembered() {
    let archive = FakeArchive::default();
    let (directory, job) = work(&archive, b"0123456789abcdef!");
    let copy = directory.path().join("RJ108375__作品.zip");
    fs::write(&copy, b"0123456789abcdef!").unwrap();
    let known = Known::default();

    let sink = Recorder::default();
    let found = existing_copy(&job, &known, &PauseSignal::default(), &sink).unwrap();
    assert_eq!(found, Some(copy.clone()));
    assert!(compared(&sink));
    assert_eq!(
        known.get(&copy).unwrap().sha256,
        job.sha256.clone().unwrap()
    );
    assert_eq!(archive.ranges.load(Ordering::SeqCst), 0);

    // Remembered: found again without reading the file.
    let sink = Recorder::default();
    let found = existing_copy(&job, &known, &PauseSignal::default(), &sink).unwrap();
    assert_eq!(found, Some(copy));
    assert!(!compared(&sink));
}

#[test]
fn a_different_file_of_the_same_size_is_not_taken_for_the_download() {
    let archive = FakeArchive::default();
    let (directory, job) = work(&archive, b"0123456789abcdef!");
    let other = directory.path().join("RJ108375__作品.zip");
    fs::write(&other, b"0123456789ABCDEF!").unwrap();
    let known = Known::default();
    let pause = PauseSignal::default();

    assert_eq!(
        existing_copy(&job, &known, &pause, &Recorder::default()),
        Ok(None)
    );
    // Its fingerprint is remembered, so the next check does not read it.
    let sink = Recorder::default();
    assert_eq!(existing_copy(&job, &known, &pause, &sink), Ok(None));
    assert!(!compared(&sink));

    // The download goes beside it; that copy is found next time.
    let done = download(&archive, &job, &options(), &pause, &Recorder::default()).unwrap();
    assert_eq!(done.path, directory.path().join("RJ108375__作品 (1).zip"));
    known.put(&done.path, KnownFile::of(&done.path, &done.sha256).unwrap());
    let sink = Recorder::default();
    assert_eq!(
        existing_copy(&job, &known, &pause, &sink),
        Ok(Some(done.path))
    );
    assert!(!compared(&sink));
    assert_eq!(fs::read(&other).unwrap(), b"0123456789ABCDEF!");
}

#[test]
fn a_changed_copy_is_read_again() {
    let archive = FakeArchive::default();
    let (directory, job) = work(&archive, b"0123456789abcdef!");
    let copy = directory.path().join("RJ108375__作品.zip");
    fs::write(&copy, b"0123456789abcdef!").unwrap();
    let known = Known::default();
    let pause = PauseSignal::default();
    assert!(
        existing_copy(&job, &known, &pause, &Recorder::default())
            .unwrap()
            .is_some()
    );

    // Same size, new content, a later modification time.
    let mut stale = known.get(&copy).unwrap();
    stale.modified -= 1;
    known.put(&copy, stale);
    fs::write(&copy, b"0123456789ABCDEF!").unwrap();
    let sink = Recorder::default();
    assert_eq!(existing_copy(&job, &known, &pause, &sink), Ok(None));
    assert!(compared(&sink));
}

#[test]
fn only_files_with_a_catalog_hash_and_the_right_size_are_recognised() {
    let archive = FakeArchive::default();
    let (directory, mut job) = work(&archive, b"0123456789abcdef!");
    fs::write(
        directory.path().join("RJ108375__作品.zip"),
        b"0123456789abcdef",
    )
    .unwrap();
    let known = Known::default();
    let pause = PauseSignal::default();
    let sink = Recorder::default();
    assert_eq!(existing_copy(&job, &known, &pause, &sink), Ok(None));
    assert!(!compared(&sink), "a wrong size is never read");

    fs::write(
        directory.path().join("RJ108375__作品.zip"),
        b"0123456789abcdef!",
    )
    .unwrap();
    job.sha256 = None;
    assert_eq!(existing_copy(&job, &known, &pause, &sink), Ok(None));
    assert!(
        !compared(&sink),
        "without a catalog hash nothing is compared"
    );
}

#[test]
fn checking_a_copy_can_be_paused() {
    let archive = FakeArchive::default();
    let (directory, job) = work(&archive, b"0123456789abcdef!");
    fs::write(
        directory.path().join("RJ108375__作品.zip"),
        b"0123456789abcdef!",
    )
    .unwrap();
    let pause = PauseSignal::default();
    pause.request();
    assert_eq!(
        existing_copy(&job, &Known::default(), &pause, &Recorder::default()),
        Err(DownloadError::Paused)
    );
}
