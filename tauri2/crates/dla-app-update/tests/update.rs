//! Release checks, signed downloads and the install helpers, against an
//! in-memory releases folder.

mod common;

use std::{
    collections::BTreeMap,
    fs,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

use common::*;
use dla_app_update::{
    Check, MANIFEST, RELEASES, Release, UpdateError, check, download, evaluate, installer_command,
    is_newer, is_verified, verify_signature,
};
use dla_library::{Archive, DownloadActivity, DownloadError, DownloadOptions, DownloadSink};
use dla_remote::{Fetched, RangeChunk, RemoteEntry};
use dla_remote::{PauseSignal, RemoteError, RemoteErrorKind};

/// The `/releases/dla-downloader` folder, in memory.
#[derive(Default)]
struct Releases {
    files: Mutex<BTreeMap<String, Vec<u8>>>,
    ranges: AtomicUsize,
}

impl Releases {
    fn with(files: &[(&str, &[u8])]) -> Self {
        let releases = Self::default();
        for (path, data) in files {
            releases
                .files
                .lock()
                .unwrap()
                .insert((*path).to_owned(), data.to_vec());
        }
        releases
    }

    fn installer(data: &[u8], signature: &str) -> Self {
        let installer = format!("{RELEASES}/{INSTALLER}");
        Self::with(&[
            (
                MANIFEST,
                &manifest("0.4.0", None, INSTALLER, data, signature),
            ),
            (&installer, data),
        ])
    }
}

fn not_found() -> RemoteError {
    RemoteError::new(RemoteErrorKind::Status(404), "Not found")
}

impl Archive for Releases {
    fn list(&self, _: &str) -> Result<Vec<RemoteEntry>, RemoteError> {
        Ok(Vec::new())
    }

    fn fetch(&self, path: &str, limit: u64, _: Option<&str>) -> Result<Fetched, RemoteError> {
        let data = self
            .files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(not_found)?;
        assert!(data.len() as u64 <= limit);
        Ok(Fetched::Content { data, etag: None })
    }

    fn read_range(
        &self,
        path: &str,
        offset: u64,
        length: u64,
        _: Option<&str>,
    ) -> Result<RangeChunk, RemoteError> {
        self.ranges.fetch_add(1, Ordering::SeqCst);
        let data = self
            .files
            .lock()
            .unwrap()
            .get(path)
            .cloned()
            .ok_or_else(not_found)?;
        let total = data.len() as u64;
        if offset >= total {
            return Ok(RangeChunk::Changed);
        }
        let end = (offset + length).min(total);
        Ok(RangeChunk::Data {
            data: data[offset as usize..end as usize].to_vec(),
            total,
            etag: Some("\"v1\"".into()),
        })
    }
}

struct Quiet;

impl DownloadSink for Quiet {
    fn activity(&self, _: DownloadActivity) {}
    fn progress(&self, _: u64, _: u64) {}
    fn etag(&self, _: Option<&str>) {}
}

fn options() -> DownloadOptions {
    DownloadOptions {
        // Several chunks for a 31-byte file.
        chunk_size: 8,
        retry_delays: Vec::new(),
        max_restarts: 1,
    }
}

fn available(check: Check) -> (Release, bool) {
    match check {
        Check::Available { release, required } => (release, required),
        other => panic!("expected an available release, got {other:?}"),
    }
}

#[test]
fn a_newer_version_is_offered_with_its_installer() {
    let json = manifest("0.4.0", None, INSTALLER, PAYLOAD, SIGNATURE);
    let (release, required) = available(evaluate(&json, "0.3.0-alpha.1", TARGET).unwrap());
    assert!(!required);
    assert_eq!(release.version, "0.4.0");
    assert_eq!(release.notes, "Faster downloads.");
    assert_eq!(release.date.as_deref(), Some("2026-10-01T10:00:00Z"));
    assert_eq!(release.remote_path, format!("{RELEASES}/{INSTALLER}"));
    assert_eq!(release.file_name, "DLA Downloader_0.4.0_x64-setup.exe");
    assert_eq!(release.size, PAYLOAD.len() as u64);
    assert_eq!(release.sha256, sha256(PAYLOAD));
}

#[test]
fn versions_compare_by_precedence() {
    let offered = |latest: &str, current: &str| {
        let json = manifest(latest, None, INSTALLER, PAYLOAD, SIGNATURE);
        matches!(
            evaluate(&json, current, TARGET).unwrap(),
            Check::Available { .. }
        )
    };
    assert!(offered("0.3.0-alpha.2", "0.3.0-alpha.1"));
    assert!(offered("0.3.0", "0.3.0-alpha.9"));
    assert!(offered("0.10.0", "0.9.0"));
    assert!(!offered("0.3.0-alpha.1", "0.3.0-alpha.1"));
    assert!(!offered("0.3.0", "0.4.0"));
    // Build metadata never makes a version newer.
    assert!(!offered("0.4.0+windows.2", "0.4.0+windows.1"));
    assert!(is_newer("0.4.0", "0.4.0-alpha.3"));
    assert!(!is_newer("0.4.0", "0.4.0"));
    assert!(!is_newer("0.4.0+2", "0.4.0+1"));
    assert!(!is_newer("next", "0.4.0"));
    let json = manifest("0.3.0", None, INSTALLER, PAYLOAD, SIGNATURE);
    assert_eq!(
        evaluate(&json, "0.3.0", TARGET).unwrap(),
        Check::Current {
            latest: "0.3.0".into()
        }
    );
}

#[test]
fn a_minimum_version_makes_the_update_required() {
    let json = manifest("0.5.0", Some("0.4.0"), INSTALLER, PAYLOAD, SIGNATURE);
    assert!(available(evaluate(&json, "0.3.9", TARGET).unwrap()).1);
    assert!(!available(evaluate(&json, "0.4.0", TARGET).unwrap()).1);
    assert!(!available(evaluate(&json, "0.4.1", TARGET).unwrap()).1);
    // Required even where no installer exists for this platform.
    assert_eq!(
        evaluate(&json, "0.3.9", "linux-x86_64").unwrap(),
        Check::Unavailable {
            latest: "0.5.0".into(),
            required: true
        }
    );
}

#[test]
fn unsafe_or_incomplete_release_lists_are_refused() {
    let good = serde_json::from_slice::<serde_json::Value>(&manifest(
        "0.4.0", None, INSTALLER, PAYLOAD, SIGNATURE,
    ))
    .unwrap();
    let broken = |change: &dyn Fn(&mut serde_json::Value)| {
        let mut document = good.clone();
        change(&mut document);
        evaluate(&serde_json::to_vec(&document).unwrap(), "0.3.0", TARGET)
    };
    let platform = |key: &'static str, value: serde_json::Value| {
        move |document: &mut serde_json::Value| document["platforms"][TARGET][key] = value.clone()
    };
    for (name, result) in [
        (
            "version",
            broken(&|document| document["version"] = "four".into()),
        ),
        (
            "minimum",
            broken(&|document| document["minimum_version"] = "x".into()),
        ),
        ("parent", broken(&platform("file", "../x.exe".into()))),
        ("absolute", broken(&platform("file", "/etc/x.exe".into()))),
        (
            "backslash",
            broken(&platform("file", "0.4.0\\x.exe".into())),
        ),
        ("drive", broken(&platform("file", "C:x.exe".into()))),
        (
            "empty part",
            broken(&platform("file", "0.4.0//x.exe".into())),
        ),
        ("size", broken(&platform("size", 0.into()))),
        ("sha256", broken(&platform("sha256", "abc".into()))),
        (
            "signature",
            broken(&platform("signature", "not base64!".into())),
        ),
    ] {
        assert!(
            matches!(result, Err(UpdateError::Manifest(_))),
            "{name}: {result:?}"
        );
    }
    assert!(matches!(
        evaluate(b"{not json", "0.3.0", TARGET),
        Err(UpdateError::Manifest(_))
    ));
}

#[test]
fn nothing_published_is_not_an_error() {
    let releases = Releases::default();
    assert_eq!(
        check(&releases, "0.3.0", TARGET).unwrap(),
        Check::NoReleases
    );
    let releases = Releases::installer(PAYLOAD, SIGNATURE);
    assert!(matches!(
        check(&releases, "0.3.0", TARGET).unwrap(),
        Check::Available { .. }
    ));
}

#[test]
fn signatures_must_come_from_the_update_key() {
    let folder = tempfile::tempdir().unwrap();
    let file = folder.path().join("installer.exe");
    fs::write(&file, PAYLOAD).unwrap();
    verify_signature(&file, SIGNATURE, TEST_KEY).unwrap();
    assert!(matches!(
        verify_signature(&file, OTHER_SIGNATURE, TEST_KEY),
        Err(UpdateError::Signature)
    ));
    fs::write(&file, b"tampered payload\n").unwrap();
    assert!(matches!(
        verify_signature(&file, SIGNATURE, TEST_KEY),
        Err(UpdateError::Signature)
    ));
    assert!(matches!(
        verify_signature(&file, SIGNATURE, "not a key"),
        Err(UpdateError::Local(_))
    ));
}

#[test]
fn downloads_are_verified_reused_and_replace_older_versions() {
    let releases = Releases::installer(PAYLOAD, SIGNATURE);
    let (release, _) = available(check(&releases, "0.3.0", TARGET).unwrap());
    let updates = tempfile::tempdir().unwrap();
    fs::create_dir_all(updates.path().join("0.3.5")).unwrap();
    fs::write(updates.path().join("0.3.5/old-setup.exe"), b"old").unwrap();
    fs::write(updates.path().join("stray.tmp"), b"x").unwrap();

    let pause = PauseSignal::default();
    let path = download(
        &releases,
        &release,
        updates.path(),
        TEST_KEY,
        &options(),
        &pause,
        &Quiet,
    )
    .unwrap();
    assert_eq!(path, updates.path().join("0.4.0").join(&release.file_name));
    assert_eq!(fs::read(&path).unwrap(), PAYLOAD);
    let left: Vec<String> = fs::read_dir(updates.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(left, ["0.4.0"]);
    let reads = releases.ranges.load(Ordering::SeqCst);
    assert!(
        reads >= 4,
        "{reads} range reads for 31 bytes in 8-byte chunks"
    );

    assert!(is_verified(&path, &release, TEST_KEY));
    assert!(!is_verified(&path, &release, "not a key"));

    // Downloaded before: checked again and reused without a request.
    let again = download(
        &releases,
        &release,
        updates.path(),
        TEST_KEY,
        &options(),
        &pause,
        &Quiet,
    )
    .unwrap();
    assert_eq!(again, path);
    assert_eq!(releases.ranges.load(Ordering::SeqCst), reads);

    // A changed file in its place is fetched again.
    fs::write(&path, b"damaged").unwrap();
    assert!(!is_verified(&path, &release, TEST_KEY));
    download(
        &releases,
        &release,
        updates.path(),
        TEST_KEY,
        &options(),
        &pause,
        &Quiet,
    )
    .unwrap();
    assert_eq!(fs::read(&path).unwrap(), PAYLOAD);
}

#[test]
fn a_tampered_installer_is_never_kept() {
    // The release list and the file agree, so the SHA-256 passes; only the
    // signature, made for the original bytes, can tell.
    let tampered = b"DLA Collab update test payloaD\n";
    let releases = Releases::installer(tampered, SIGNATURE);
    let (release, _) = available(check(&releases, "0.3.0", TARGET).unwrap());
    let updates = tempfile::tempdir().unwrap();
    let pause = PauseSignal::default();
    let result = download(
        &releases,
        &release,
        updates.path(),
        TEST_KEY,
        &options(),
        &pause,
        &Quiet,
    );
    assert!(matches!(result, Err(UpdateError::Signature)), "{result:?}");
    let folder = updates.path().join("0.4.0");
    assert_eq!(fs::read_dir(folder).unwrap().count(), 0);

    // Bytes that differ from the release list fail on the SHA-256 first.
    let releases = Releases::installer(PAYLOAD, SIGNATURE);
    let (release, _) = available(check(&releases, "0.3.0", TARGET).unwrap());
    let installer = format!("{RELEASES}/{INSTALLER}");
    releases
        .files
        .lock()
        .unwrap()
        .insert(installer, tampered.to_vec());
    let result = download(
        &releases,
        &release,
        updates.path(),
        TEST_KEY,
        &options(),
        &pause,
        &Quiet,
    );
    assert!(
        matches!(
            result,
            Err(UpdateError::Download(DownloadError::ChecksumMismatch))
        ),
        "{result:?}"
    );
}

#[test]
fn the_windows_installer_runs_like_tauris_updater_runs_it() {
    let command = installer_command(std::path::Path::new("setup.exe"), &["--self-test"]);
    let args: Vec<_> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    assert_eq!(args, ["/P", "/UPDATE", "/R", "/ARGS", "--self-test"]);
}

#[cfg(unix)]
#[test]
fn an_app_image_is_replaced_in_one_step() {
    use std::os::unix::fs::PermissionsExt;

    use dla_app_update::replace_app_image;

    let folder = tempfile::tempdir().unwrap();
    let current = folder.path().join("DLA_Downloader.AppImage");
    fs::write(&current, b"old").unwrap();
    let new = folder.path().join("downloaded");
    fs::write(&new, b"new").unwrap();
    replace_app_image(&current, &new).unwrap();
    assert_eq!(fs::read(&current).unwrap(), b"new");
    assert_eq!(
        fs::metadata(&current).unwrap().permissions().mode() & 0o777,
        0o755
    );
    assert_eq!(fs::read_dir(folder.path()).unwrap().count(), 2);
}
