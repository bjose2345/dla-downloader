//! Updates for DLA Downloader itself.
//!
//! Releases are published in the read-only `/releases/dla-downloader` folder,
//! which the app reads through the download service with the same tunnel and
//! sign-in as the Library. `latest.json` names the newest version
//! and, for each platform, an installer with its size, SHA-256 and the
//! minisign signature `cargo tauri build` made with the DLA update key.
//!
//! An installer arrives through the Library's download, which checks the
//! SHA-256, and is kept only when its signature matches the public key built
//! into the app: write access to the folder alone can never ship code.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    process::Command,
    thread,
    time::Duration,
};

use base64::{Engine, engine::general_purpose::STANDARD};
use dla_library::{
    Archive, DownloadError, DownloadJob, DownloadOptions, DownloadSink, download as download_file,
    local_name,
};
use dla_remote::Fetched;
use dla_remote::{PauseSignal, RemoteError, RemoteErrorKind};
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

/// The app's folder in the archive.
pub const RELEASES: &str = "/releases/dla-downloader";
/// Written last when a release is published, so it never names a missing file.
pub const MANIFEST: &str = "/releases/dla-downloader/latest.json";
const MANIFEST_LIMIT: u64 = 256 << 10;

/// How long a check keeps asking while the service is busy or out of reach
/// for a moment, about a minute in all. The download service serves one file
/// per account at a time, and right after sign-in the catalog refresh is
/// fetching month manifests, a full one for up to about half a minute.
pub const CHECK_RETRY_DELAYS: [Duration; 6] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
    Duration::from_secs(8),
    Duration::from_secs(15),
    Duration::from_secs(30),
];
const MAX_INSTALLER: u64 = 2 << 30;
const MAX_NOTES: usize = 4000;
const READ: usize = 1 << 20;

/// This build's platform as `latest.json` names it, the same as Tauri's
/// updater; `None` where no installer is published.
pub fn target() -> Option<&'static str> {
    if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
        Some("windows-x86_64")
    } else if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
        Some("linux-x86_64")
    } else {
        None
    }
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
    #[serde(default)]
    notes: String,
    #[serde(default)]
    pub_date: Option<String>,
    #[serde(default)]
    minimum_version: Option<String>,
    #[serde(default)]
    platforms: BTreeMap<String, Platform>,
}

#[derive(Deserialize)]
struct Platform {
    /// Relative to [`RELEASES`], such as `0.1.0/DLA Downloader_0.1.0_x64-setup.exe`.
    file: String,
    size: u64,
    sha256: String,
    /// The `.sig` file `cargo tauri build` wrote beside the installer.
    signature: String,
}

/// A newer version's installer for this platform.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Release {
    pub version: String,
    pub notes: String,
    pub date: Option<String>,
    /// The installer below the archive root.
    pub remote_path: String,
    /// The local file name, already safe on Linux and Windows.
    pub file_name: String,
    pub size: u64,
    pub sha256: String,
    pub signature: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Check {
    /// Nothing has been published yet.
    NoReleases,
    /// This version is the newest one.
    Current { latest: String },
    /// A newer version with an installer for this platform. It is
    /// `required` when this version is older than the release's minimum.
    Available { release: Release, required: bool },
    /// A newer version exists, but not for this platform.
    Unavailable { latest: String, required: bool },
}

#[derive(Debug, Error)]
pub enum UpdateError {
    #[error(transparent)]
    Remote(RemoteError),
    #[error("the release list is invalid: {0}")]
    Manifest(String),
    #[error(transparent)]
    Download(DownloadError),
    #[error("the installer is not signed with the DLA update key")]
    Signature,
    #[error("{0}")]
    Local(String),
}

/// Reads `latest.json` from the releases folder and compares it with
/// `current`, this app's version, asking again while the service is busy
/// (see [`CHECK_RETRY_DELAYS`]). Blocks while it waits.
pub fn check(archive: &dyn Archive, current: &str, target: &str) -> Result<Check, UpdateError> {
    check_with_retries(archive, current, target, &CHECK_RETRY_DELAYS)
}

/// [`check`], waiting `delays` between attempts that failed for a moment.
pub fn check_with_retries(
    archive: &dyn Archive,
    current: &str,
    target: &str,
    delays: &[Duration],
) -> Result<Check, UpdateError> {
    let mut waits = delays.iter();
    let fetched = loop {
        match archive.fetch(MANIFEST, MANIFEST_LIMIT, None) {
            Err(error) if error.is_transient() => match waits.next() {
                Some(delay) => thread::sleep(*delay),
                None => break Err(error),
            },
            fetched => break fetched,
        }
    };
    match fetched {
        Ok(Fetched::Content { data, .. }) => evaluate(&data, current, target),
        Ok(Fetched::NotModified) => Err(invalid("an answer to a question not asked")),
        // The folder and the file are only there once something is published.
        Err(error) if matches!(error.kind, RemoteErrorKind::Status(404)) => Ok(Check::NoReleases),
        Err(error) => Err(UpdateError::Remote(error)),
    }
}

/// Compares a `latest.json` with `current`, this app's version. Build
/// metadata (`+…`) never makes a version newer.
pub fn evaluate(json: &[u8], current: &str, target: &str) -> Result<Check, UpdateError> {
    let manifest: Manifest =
        serde_json::from_slice(json).map_err(|error| invalid(error.to_string()))?;
    let latest = version(&manifest.version)?;
    let minimum = manifest
        .minimum_version
        .as_deref()
        .map(version)
        .transpose()?;
    let current = Version::parse(current).map_err(|_| {
        UpdateError::Local(format!(
            "this app's version {current} is not a version number"
        ))
    })?;
    let name = manifest.version.trim().to_owned();
    if latest.cmp_precedence(&current).is_le() {
        return Ok(Check::Current { latest: name });
    }
    let required = minimum.is_some_and(|minimum| current.cmp_precedence(&minimum).is_lt());
    match manifest.platforms.get(target) {
        Some(platform) => Ok(Check::Available {
            release: release(&manifest, platform)?,
            required,
        }),
        None => Ok(Check::Unavailable {
            latest: name,
            required,
        }),
    }
}

/// Whether `candidate` is a newer version than `current`; false when either
/// is not a version number.
pub fn is_newer(candidate: &str, current: &str) -> bool {
    match (
        Version::parse(candidate.trim()),
        Version::parse(current.trim()),
    ) {
        (Ok(candidate), Ok(current)) => candidate.cmp_precedence(&current).is_gt(),
        _ => false,
    }
}

fn version(text: &str) -> Result<Version, UpdateError> {
    Version::parse(text.trim()).map_err(|_| invalid(format!("{text} is not a version number")))
}

fn release(manifest: &Manifest, platform: &Platform) -> Result<Release, UpdateError> {
    let parts: Vec<&str> = platform.file.split('/').collect();
    let unsafe_part = |part: &&str| {
        part.is_empty() || *part == "." || *part == ".." || part.contains(['\\', '\0', ':'])
    };
    if parts.iter().any(unsafe_part) {
        return Err(invalid(format!(
            "{} is not a file in the releases folder",
            platform.file
        )));
    }
    if platform.size == 0 || platform.size > MAX_INSTALLER {
        return Err(invalid(format!("an installer of {} bytes", platform.size)));
    }
    let sha256 = platform.sha256.trim().to_ascii_lowercase();
    if sha256.len() != 64 || !sha256.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(invalid("an installer without its SHA-256"));
    }
    signature_of(&platform.signature)?;
    Ok(Release {
        version: manifest.version.trim().to_owned(),
        notes: manifest.notes.trim().chars().take(MAX_NOTES).collect(),
        date: manifest.pub_date.clone(),
        remote_path: format!("{RELEASES}/{}", platform.file),
        file_name: local_name(parts[parts.len() - 1]),
        size: platform.size,
        sha256,
        signature: platform.signature.trim().to_owned(),
    })
}

/// Downloads `release` into `directory/<version>/`, removing whatever
/// earlier versions left in `directory`, and returns the installer once its
/// SHA-256 and signature are right. An installer downloaded before is
/// checked again and reused. A part file does not survive a restart: the
/// installer is small enough to fetch again.
pub fn download(
    archive: &dyn Archive,
    release: &Release,
    directory: &Path,
    public_key: &str,
    options: &DownloadOptions,
    pause: &PauseSignal,
    sink: &dyn DownloadSink,
) -> Result<PathBuf, UpdateError> {
    let key = public_key_of(public_key)?;
    let folder = directory.join(&release.version);
    fs::create_dir_all(&folder).map_err(|error| local(&folder, &error))?;
    for entry in fs::read_dir(directory)
        .map_err(|error| local(directory, &error))?
        .flatten()
    {
        let path = entry.path();
        if path != folder {
            let removed = if path.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            removed.map_err(|error| local(&path, &error))?;
        }
    }

    let installer = folder.join(&release.file_name);
    if installer.is_file() {
        if matches_release(&installer, release)?
            && verify_with(&installer, &release.signature, &key).is_ok()
        {
            return Ok(installer);
        }
        fs::remove_file(&installer).map_err(|error| local(&installer, &error))?;
    }
    let job = DownloadJob::new(
        &release.remote_path,
        &folder,
        &release.file_name,
        release.size,
        Some(release.sha256.clone()),
    );
    let downloaded =
        download_file(archive, &job, options, pause, sink).map_err(UpdateError::Download)?;
    if let Err(error) = verify_with(&downloaded.path, &release.signature, &key) {
        let _ = fs::remove_file(&downloaded.path);
        return Err(error);
    }
    Ok(downloaded.path)
}

/// Whether the installer at `path` is still `release`'s, byte for byte and
/// signed with `public_key`; checked again before it is run.
pub fn is_verified(path: &Path, release: &Release, public_key: &str) -> bool {
    public_key_of(public_key).is_ok_and(|key| {
        path.is_file()
            && matches_release(path, release).unwrap_or(false)
            && verify_with(path, &release.signature, &key).is_ok()
    })
}

fn matches_release(path: &Path, release: &Release) -> Result<bool, UpdateError> {
    let size = fs::metadata(path)
        .map_err(|error| local(path, &error))?
        .len();
    Ok(size == release.size && sha256_of(path)? == release.sha256)
}

/// Checks that `path` is what `signature` (a Tauri `.sig`, base64) signed
/// with the key `public_key` (a Tauri `.key.pub`, base64).
pub fn verify_signature(path: &Path, signature: &str, public_key: &str) -> Result<(), UpdateError> {
    verify_with(path, signature, &public_key_of(public_key)?)
}

fn verify_with(path: &Path, signature: &str, key: &PublicKey) -> Result<(), UpdateError> {
    let signature = signature_of(signature)?;
    // Tauri signs a BLAKE2b hash of the file, so it is read in pieces.
    let mut verifier = key
        .verify_stream(&signature)
        .map_err(|_| UpdateError::Signature)?;
    let mut file = File::open(path).map_err(|error| local(path, &error))?;
    let mut buffer = vec![0; READ];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| local(path, &error))?;
        if count == 0 {
            break;
        }
        verifier.update(&buffer[..count]);
    }
    verifier.finalize().map_err(|_| UpdateError::Signature)
}

fn public_key_of(encoded: &str) -> Result<PublicKey, UpdateError> {
    decode_text(encoded)
        .and_then(|text| PublicKey::decode(&text).ok())
        .ok_or_else(|| UpdateError::Local("the update key built into this app is invalid".into()))
}

fn signature_of(encoded: &str) -> Result<Signature, UpdateError> {
    decode_text(encoded)
        .and_then(|text| Signature::decode(&text).ok())
        .ok_or_else(|| invalid("an installer signature is not readable"))
}

fn decode_text(encoded: &str) -> Option<String> {
    String::from_utf8(STANDARD.decode(encoded.trim()).ok()?).ok()
}

fn sha256_of(path: &Path) -> Result<String, UpdateError> {
    let mut file = File::open(path).map_err(|error| local(path, &error))?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0; READ];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|error| local(path, &error))?;
        if count == 0 {
            return Ok(hex::encode(hasher.finalize()));
        }
        hasher.update(&buffer[..count]);
    }
}

/// Runs the Windows installer `cargo tauri build` makes (NSIS) the way
/// Tauri's updater does: `/P` shows only its progress, `/UPDATE` keeps the
/// existing installation's choices, and `/R` starts the app again with
/// `args` (after `/ARGS`). Only plain flags such as `--self-test` belong in
/// `args`, because NSIS reads the command line itself.
pub fn installer_command(installer: &Path, args: &[&str]) -> Command {
    let mut command = Command::new(installer);
    command.args(["/P", "/UPDATE", "/R", "/ARGS"]).args(args);
    command
}

/// Puts `new` in place of the AppImage at `current`. It is copied beside it
/// first, so the swap is one rename on the same file system, and a failure
/// leaves the old AppImage as it was.
pub fn replace_app_image(current: &Path, new: &Path) -> io::Result<()> {
    let name = current
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "the AppImage has no name"))?;
    let staged = current.with_file_name(format!(".{}.update", name.to_string_lossy()));
    let _ = fs::remove_file(&staged);
    let result = (|| {
        fs::copy(new, &staged)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&staged, fs::Permissions::from_mode(0o755))?;
        }
        File::open(&staged)?.sync_all()?;
        fs::rename(&staged, current)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&staged);
    }
    result
}

fn invalid(detail: impl Into<String>) -> UpdateError {
    UpdateError::Manifest(detail.into())
}

fn local(path: &Path, error: &io::Error) -> UpdateError {
    UpdateError::Local(format!("{}: {error}", path.display()))
}
