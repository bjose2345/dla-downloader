use std::{
    cell::Cell,
    fs::{self, File, OpenOptions},
    io::{self, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{Duration, UNIX_EPOCH},
};

use dla_remote::{Fetched, RangeChunk};
use dla_remote::{PauseSignal, RemoteError, RemoteErrorKind};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

use crate::{Archive, with_retries};

/// Appended to the final name while a download is incomplete.
pub const PART_SUFFIX: &str = ".dla-part";
const CHECK_BUFFER: usize = 1 << 20;
const CHECK_REPORT: u64 = 64 << 20;
const MAX_NAME_BYTES: usize = 200;
// `name`, then `name (1)` to `name (9)`, which earlier downloads may have left.
const COPIES_CHECKED: u32 = 10;
const RESERVED_NAMES: [&str; 22] = [
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

/// One archive file to save locally. Everything needed to resume after a
/// restart is here, so the caller can persist it as it is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadJob {
    /// Archive path, such as `/dlsite/2013-01/RJ108375__x.zip`.
    pub remote_path: String,
    pub directory: PathBuf,
    /// The local file name, already safe on Linux and Windows.
    pub file_name: String,
    /// Size from the catalog or listing.
    pub size: u64,
    /// Lowercase SHA-256 from the manifest, when the archive lists one.
    pub sha256: Option<String>,
    /// The archive copy's ETag once the first chunk arrived, so a resumed
    /// download never mixes two versions of a file.
    pub etag: Option<String>,
}

impl DownloadJob {
    pub fn new(
        remote_path: impl Into<String>,
        directory: impl Into<PathBuf>,
        remote_name: &str,
        size: u64,
        sha256: Option<String>,
    ) -> Self {
        Self {
            remote_path: remote_path.into(),
            directory: directory.into(),
            file_name: local_name(remote_name),
            size,
            sha256,
            etag: None,
        }
    }

    pub fn part_path(&self) -> PathBuf {
        self.directory
            .join(format!("{}{PART_SUFFIX}", self.file_name))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum DownloadActivity {
    /// Re-reading the part saved earlier before continuing it.
    Checking {
        checked: u64,
    },
    Receiving,
    Reconnecting {
        seconds: u64,
        attempt: u32,
        of: u32,
    },
    /// The archive copy changed; starting again from the beginning.
    Restarting,
    Verifying,
    /// Reading a file already in the folder to see whether it is this one.
    Comparing {
        checked: u64,
    },
}

pub trait DownloadSink {
    fn activity(&self, activity: DownloadActivity);
    fn progress(&self, received: u64, total: u64);
    /// The archive copy's version was learned or forgotten; persist it with
    /// the job so a later resume keeps using it.
    fn etag(&self, etag: Option<&str>);
}

#[derive(Debug, Clone)]
pub struct DownloadOptions {
    pub chunk_size: u64,
    pub retry_delays: Vec<Duration>,
    /// How often the archive copy may change mid-download before giving up.
    pub max_restarts: u32,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            chunk_size: 8 << 20,
            retry_delays: [1, 2, 4, 8, 15].map(Duration::from_secs).to_vec(),
            max_restarts: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Downloaded {
    pub path: PathBuf,
    pub sha256: String,
    /// True when the SHA-256 matched the manifest; false when the archive
    /// lists none for this file.
    pub verified: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum DownloadError {
    #[error("download paused; the received part is kept")]
    Paused,
    #[error(transparent)]
    Remote(RemoteError),
    #[error("{0}")]
    Local(String),
    #[error("the archive copy kept changing during the download")]
    KeptChanging,
    #[error("the archive copy is {actual} bytes, but the catalog lists {expected}")]
    SizeMismatch { expected: u64, actual: u64 },
    #[error("the downloaded file does not match the catalog checksum and was discarded")]
    ChecksumMismatch,
}

/// Downloads `job` in ranged chunks into its part file, continuing whatever
/// an earlier run saved, then checks the SHA-256 and moves the file into
/// place without ever replacing an existing file.
pub fn download(
    archive: &dyn Archive,
    job: &DownloadJob,
    options: &DownloadOptions,
    pause: &PauseSignal,
    sink: &dyn DownloadSink,
) -> Result<Downloaded, DownloadError> {
    if !job.directory.is_dir() {
        return Err(DownloadError::Local(format!(
            "The folder {} is not available.",
            job.directory.display()
        )));
    }
    let part_path = job.part_path();
    let mut etag = job.etag.clone();
    let mut part = Part::open(&part_path, etag.is_some(), job.size, pause, sink)?;
    let mut total = (etag.is_some() && part.offset == job.size).then_some(job.size);
    if part.offset > 0 {
        sink.progress(part.offset, job.size);
    }
    let mut restarts = 0;
    let reconnecting = Cell::new(false);
    sink.activity(DownloadActivity::Receiving);
    while total.is_none_or(|total| part.offset < total) {
        if pause.is_requested() {
            return Err(DownloadError::Paused);
        }
        let waiting = |seconds, attempt, of| {
            reconnecting.set(true);
            sink.activity(DownloadActivity::Reconnecting {
                seconds,
                attempt,
                of,
            });
        };
        let chunk = with_retries(
            &options.retry_delays,
            &|| pause.is_requested(),
            &waiting,
            || {
                archive.read_range(
                    &job.remote_path,
                    part.offset,
                    options.chunk_size,
                    etag.as_deref(),
                )
            },
        )
        .map_err(DownloadError::Remote)?
        .ok_or(DownloadError::Paused)?;
        if reconnecting.replace(false) {
            sink.activity(DownloadActivity::Receiving);
        }
        let changed = match chunk {
            RangeChunk::Data {
                data,
                total: size,
                etag: current,
            } => {
                if etag.is_some() && current.is_some() && current != etag {
                    true
                } else {
                    if etag.is_none() {
                        etag = current;
                        sink.etag(etag.as_deref());
                    }
                    if job.sha256.is_some() && job.size > 0 && size != job.size {
                        return Err(DownloadError::SizeMismatch {
                            expected: job.size,
                            actual: size,
                        });
                    }
                    if data.is_empty() {
                        return Err(DownloadError::Remote(invalid_response()));
                    }
                    part.append(&data)?;
                    total = Some(size);
                    sink.progress(part.offset, size);
                    false
                }
            }
            // An empty file has no byte range to ask for.
            RangeChunk::Changed
                if part.offset == 0 && etag.is_none() && is_empty(archive, &job.remote_path) =>
            {
                total = Some(0);
                false
            }
            RangeChunk::Changed => true,
        };
        if changed {
            restarts += 1;
            if restarts > options.max_restarts {
                return Err(DownloadError::KeptChanging);
            }
            sink.activity(DownloadActivity::Restarting);
            etag = None;
            sink.etag(None);
            part.reset()?;
            total = None;
            sink.progress(0, job.size);
        }
    }

    sink.activity(DownloadActivity::Verifying);
    let sha256 = part.finish()?;
    if job
        .sha256
        .as_ref()
        .is_some_and(|expected| *expected != sha256)
    {
        let _ = fs::remove_file(&part_path);
        return Err(DownloadError::ChecksumMismatch);
    }
    let path = place(&part_path, &job.directory, &job.file_name)?;
    Ok(Downloaded {
        path,
        sha256,
        verified: job.sha256.is_some(),
    })
}

/// What a check learned about a file in a download folder: its SHA-256, for
/// as long as it keeps this size and modification time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownFile {
    pub size: u64,
    /// Nanoseconds since the Unix epoch.
    pub modified: u64,
    pub sha256: String,
}

impl KnownFile {
    /// The fingerprint of a file just saved with this SHA-256.
    pub fn of(path: &Path, sha256: &str) -> Option<Self> {
        let metadata = fs::metadata(path).ok()?;
        Some(Self {
            size: metadata.len(),
            modified: modified(&metadata)?,
            sha256: sha256.to_owned(),
        })
    }
}

/// Where checks remember what they learned, so a file is read again only
/// after it changed.
pub trait KnownFiles {
    fn get(&self, path: &Path) -> Option<KnownFile>;
    fn put(&self, path: &Path, file: KnownFile);
}

/// Looks in `job`'s folder for a copy of the file it would download: under
/// its own name, or `name (1)`, `name (2)`… left by earlier downloads, with
/// the catalog's size and SHA-256. A file whose fingerprint is remembered is
/// not read again. Only a file the catalog has a SHA-256 for can be
/// recognised. Returns the copy's path, or `None` when it has to be
/// downloaded.
pub fn existing_copy(
    job: &DownloadJob,
    known: &dyn KnownFiles,
    pause: &PauseSignal,
    sink: &dyn DownloadSink,
) -> Result<Option<PathBuf>, DownloadError> {
    let Some(expected) = job.sha256.as_deref() else {
        return Ok(None);
    };
    for number in 0..COPIES_CHECKED {
        let path = job.directory.join(numbered(&job.file_name, number));
        let Ok(metadata) = fs::symlink_metadata(&path) else {
            continue;
        };
        if !metadata.is_file() || metadata.len() != job.size {
            continue;
        }
        let stamp = modified(&metadata);
        let remembered = known
            .get(&path)
            .filter(|file| file.size == job.size && Some(file.modified) == stamp);
        let sha256 = match remembered {
            Some(file) => file.sha256,
            None => {
                let Some(sha256) = hash_file(&path, pause, sink)? else {
                    continue;
                };
                if let Some(modified) = stamp {
                    known.put(
                        &path,
                        KnownFile {
                            size: job.size,
                            modified,
                            sha256: sha256.clone(),
                        },
                    );
                }
                sha256
            }
        };
        if sha256 == expected {
            return Ok(Some(path));
        }
    }
    Ok(None)
}

fn modified(metadata: &fs::Metadata) -> Option<u64> {
    let since = metadata.modified().ok()?.duration_since(UNIX_EPOCH).ok()?;
    u64::try_from(since.as_nanos()).ok()
}

/// The SHA-256 of a file in the folder, or `None` when it cannot be read.
fn hash_file(
    path: &Path,
    pause: &PauseSignal,
    sink: &dyn DownloadSink,
) -> Result<Option<String>, DownloadError> {
    let Ok(mut file) = File::open(path) else {
        return Ok(None);
    };
    sink.activity(DownloadActivity::Comparing { checked: 0 });
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; CHECK_BUFFER];
    let (mut checked, mut reported) = (0, 0);
    loop {
        if pause.is_requested() {
            return Err(DownloadError::Paused);
        }
        let Ok(read) = file.read(&mut buffer) else {
            return Ok(None);
        };
        if read == 0 {
            return Ok(Some(hex::encode(hasher.finalize())));
        }
        hasher.update(&buffer[..read]);
        checked += read as u64;
        if checked - reported >= CHECK_REPORT {
            reported = checked;
            sink.activity(DownloadActivity::Comparing { checked });
        }
    }
}

fn is_empty(archive: &dyn Archive, path: &str) -> bool {
    matches!(archive.fetch(path, 0, None), Ok(Fetched::Content { data, .. }) if data.is_empty())
}

fn invalid_response() -> RemoteError {
    RemoteError::new(
        RemoteErrorKind::InvalidResponse,
        "The DLA server returned an invalid response.",
    )
}

/// The partial file and the running SHA-256 of everything in it.
struct Part {
    file: File,
    path: PathBuf,
    hasher: Sha256,
    offset: u64,
}

impl Part {
    /// Continues a trusted part by hashing what it holds; anything else
    /// starts from an empty file.
    fn open(
        path: &Path,
        trusted: bool,
        expected: u64,
        pause: &PauseSignal,
        sink: &dyn DownloadSink,
    ) -> Result<Self, DownloadError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .map_err(|error| local(path, &error))?;
        let mut part = Self {
            file,
            path: path.to_owned(),
            hasher: Sha256::new(),
            offset: 0,
        };
        let length = part
            .file
            .metadata()
            .map_err(|error| local(path, &error))?
            .len();
        if !trusted || length > expected {
            part.reset()?;
            return Ok(part);
        }
        if length > 0 {
            sink.activity(DownloadActivity::Checking { checked: 0 });
        }
        let mut buffer = vec![0; CHECK_BUFFER];
        let mut reported = 0;
        loop {
            if pause.is_requested() {
                return Err(DownloadError::Paused);
            }
            let read = part
                .file
                .read(&mut buffer)
                .map_err(|error| local(path, &error))?;
            if read == 0 {
                break;
            }
            part.hasher.update(&buffer[..read]);
            part.offset += read as u64;
            if part.offset - reported >= CHECK_REPORT {
                reported = part.offset;
                sink.activity(DownloadActivity::Checking {
                    checked: part.offset,
                });
            }
        }
        Ok(part)
    }

    fn append(&mut self, data: &[u8]) -> Result<(), DownloadError> {
        self.file
            .write_all(data)
            .map_err(|error| local(&self.path, &error))?;
        self.hasher.update(data);
        self.offset += data.len() as u64;
        Ok(())
    }

    fn reset(&mut self) -> Result<(), DownloadError> {
        self.file
            .set_len(0)
            .and_then(|()| self.file.seek(SeekFrom::Start(0)).map(|_| ()))
            .map_err(|error| local(&self.path, &error))?;
        self.hasher = Sha256::new();
        self.offset = 0;
        Ok(())
    }

    fn finish(self) -> Result<String, DownloadError> {
        self.file
            .sync_all()
            .map_err(|error| local(&self.path, &error))?;
        Ok(hex::encode(self.hasher.finalize()))
    }
}

fn local(path: &Path, error: &io::Error) -> DownloadError {
    DownloadError::Local(format!("{}: {error}", path.display()))
}

/// Moves the finished part to `name`, or to `name (1)`, `name (2)`… when
/// that exists. A hard link claims the name atomically; file systems
/// without hard links (FAT, exFAT, some shares) fall back to a rename.
fn place(part: &Path, directory: &Path, name: &str) -> Result<PathBuf, DownloadError> {
    for number in 0..1000 {
        let candidate = directory.join(numbered(name, number));
        match fs::hard_link(part, &candidate) {
            Ok(()) => {
                let _ = fs::remove_file(part);
                return Ok(candidate);
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(_) if fs::symlink_metadata(&candidate).is_ok() => {}
            Err(_) => {
                fs::rename(part, &candidate).map_err(|error| local(&candidate, &error))?;
                return Ok(candidate);
            }
        }
    }
    Err(DownloadError::Local(format!(
        "{}: too many files with this name already exist.",
        directory.join(name).display()
    )))
}

/// `work.zip` → `work (2).zip`; names without an extension get the number
/// at the end.
fn numbered(name: &str, number: u32) -> String {
    if number == 0 {
        return name.to_owned();
    }
    match name.rfind('.').filter(|&dot| dot > 0) {
        Some(dot) => format!("{} ({number}){}", &name[..dot], &name[dot..]),
        None => format!("{name} ({number})"),
    }
}

/// Makes an archive file name safe to create on Linux and Windows: no
/// separators, reserved characters or device names, and short enough to
/// leave room for the part suffix and a number.
pub fn local_name(remote: &str) -> String {
    let replaced: String = remote
        .chars()
        .map(|character| match character {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            character if character.is_control() => '_',
            character => character,
        })
        .collect();
    let mut name = replaced
        .trim_start()
        .trim_end_matches(['.', ' '])
        .to_owned();
    if name.is_empty() {
        name = "download".to_owned();
    }
    let device = name
        .split('.')
        .next()
        .unwrap_or_default()
        .trim_end()
        .to_ascii_uppercase();
    if RESERVED_NAMES.contains(&device.as_str()) {
        name.insert(0, '_');
    }
    if name.len() > MAX_NAME_BYTES {
        let extension = name
            .rfind('.')
            .filter(|&dot| dot > 0 && name.len() - dot <= 16)
            .map(|dot| name[dot..].to_owned())
            .unwrap_or_default();
        let mut end = MAX_NAME_BYTES - extension.len();
        while !name.is_char_boundary(end) {
            end -= 1;
        }
        name = format!("{}{extension}", name[..end].trim_end_matches(['.', ' ']));
    }
    name
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_names_are_safe_on_both_systems() {
        assert_eq!(local_name("RJ108375__作品.zip"), "RJ108375__作品.zip");
        assert_eq!(local_name("a/b\\c:d?.zip"), "a_b_c_d_.zip");
        assert_eq!(local_name("con.txt"), "_con.txt");
        assert_eq!(local_name("  name. . "), "name");
        assert_eq!(local_name(".."), "download");
        assert_eq!(local_name("tab\there"), "tab_here");
        let long = format!("{}.zip", "作".repeat(100));
        let safe = local_name(&long);
        assert!(safe.len() <= MAX_NAME_BYTES && safe.ends_with(".zip"));
    }

    #[test]
    fn numbered_names_keep_the_extension() {
        assert_eq!(numbered("work.zip", 0), "work.zip");
        assert_eq!(numbered("work.zip", 2), "work (2).zip");
        assert_eq!(numbered("README", 1), "README (1)");
        assert_eq!(numbered(".hidden", 1), ".hidden (1)");
    }
}
