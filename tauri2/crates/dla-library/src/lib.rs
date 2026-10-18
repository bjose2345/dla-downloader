//! The Library: a catalog built from the archive's per-month
//! `_manifest.json` files, local search over it, and resumable downloads that
//! are verified against the manifest's SHA-256 before they are kept.

mod arrivals;
mod catalog;
mod download;
mod index;

use std::{thread, time::Duration};

use dla_remote::RemoteError;
use dla_remote::{ArchiveClient, Fetched, RangeChunk, RemoteEntry};

pub use arrivals::{month_of, month_range, now_seconds, server_time_seconds};
pub use catalog::{Catalog, CatalogEntry, FolderSummary, Recent, parse_manifest};
pub use download::{
    DownloadActivity, DownloadError, DownloadJob, DownloadOptions, DownloadSink, Downloaded,
    KnownFile, KnownFiles, PART_SUFFIX, download, existing_copy, local_name,
};
pub use index::{
    IndexProgress, IndexStore, MANIFEST_NAME, ManifestState, RefreshOptions, RefreshSummary,
};

/// The archive root the download service serves.
pub const ARCHIVE_ROOT: &str = "/dlsite";

/// What the Library needs from the download service; implemented by
/// [`ArchiveClient`] and by in-memory fakes in tests.
pub trait Archive: Send + Sync {
    fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, RemoteError>;
    fn fetch(
        &self,
        path: &str,
        limit: u64,
        if_none_match: Option<&str>,
    ) -> Result<Fetched, RemoteError>;
    fn read_range(
        &self,
        path: &str,
        offset: u64,
        length: u64,
        etag: Option<&str>,
    ) -> Result<RangeChunk, RemoteError>;
}

impl Archive for ArchiveClient {
    fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, RemoteError> {
        ArchiveClient::list(self, path)
    }

    fn fetch(
        &self,
        path: &str,
        limit: u64,
        if_none_match: Option<&str>,
    ) -> Result<Fetched, RemoteError> {
        ArchiveClient::fetch(self, path, limit, if_none_match)
    }

    fn read_range(
        &self,
        path: &str,
        offset: u64,
        length: u64,
        etag: Option<&str>,
    ) -> Result<RangeChunk, RemoteError> {
        ArchiveClient::read_range(self, path, offset, length, etag)
    }
}

/// Repeats `operation` after transient failures, waiting the given delays.
/// Returns `Ok(None)` when `stopped` turns true while waiting.
pub(crate) fn with_retries<T>(
    delays: &[Duration],
    stopped: &dyn Fn() -> bool,
    waiting: &dyn Fn(u64, u32, u32),
    mut operation: impl FnMut() -> Result<T, RemoteError>,
) -> Result<Option<T>, RemoteError> {
    let mut attempt = 0;
    loop {
        match operation() {
            Ok(value) => return Ok(Some(value)),
            Err(error) if error.is_transient() && attempt < delays.len() => {
                let delay = delays[attempt];
                attempt += 1;
                waiting(delay.as_secs().max(1), attempt as u32, delays.len() as u32);
                if !sleep_unless(stopped, delay) {
                    return Ok(None);
                }
            }
            Err(error) => return Err(error),
        }
    }
}

/// Sleeps in short steps; false when `stopped` turned true first.
fn sleep_unless(stopped: &dyn Fn() -> bool, duration: Duration) -> bool {
    const STEP: Duration = Duration::from_millis(100);
    let mut left = duration;
    while !left.is_zero() {
        if stopped() {
            return false;
        }
        let step = left.min(STEP);
        thread::sleep(step);
        left -= step;
    }
    !stopped()
}
