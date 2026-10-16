//! The update cycle against the receiver's download service and the Go mock
//! SFTPGo behind it, which serves a read-only /releases folder like SFTPGo
//! (see `go_services`). The key and signatures are the
//! test fixtures of `dla-app-update`, made for tests only.

use std::path::Path;

use serde_json::json;
use sha2::{Digest, Sha256};

use super::*;
use crate::go_services::GoServices;

const TEST_KEY: &str = include_str!("../../../crates/dla-app-update/tests/fixtures/test.key.pub");
const PAYLOAD: &[u8] = include_bytes!("../../../crates/dla-app-update/tests/fixtures/payload.bin");
const SIGNATURE: &str =
    include_str!("../../../crates/dla-app-update/tests/fixtures/payload.bin.sig");
const OTHER_SIGNATURE: &str =
    include_str!("../../../crates/dla-app-update/tests/fixtures/other.bin.sig");

/// Version 0.4.0 (minimum 0.3.5) for this platform, signed with
/// `signature`, published in the mock SFTPGo's /releases.
struct Services {
    go: GoServices,
    _releases: tempfile::TempDir,
}

impl Services {
    fn start(signature: &str) -> Option<Self> {
        let releases = tempfile::tempdir().unwrap();
        let app = releases.path().join("dla-downloader");
        fs::create_dir_all(app.join("0.4.0")).unwrap();
        fs::write(app.join("0.4.0/DLA Downloader setup.bin"), PAYLOAD).unwrap();
        let manifest = json!({
            "version": "0.4.0",
            "notes": "Faster downloads.",
            "minimum_version": "0.3.5",
            "platforms": {
                dla_app_update::target().unwrap(): {
                    "file": "0.4.0/DLA Downloader setup.bin",
                    "size": PAYLOAD.len(),
                    "sha256": hex::encode(Sha256::digest(PAYLOAD)),
                    "signature": signature,
                }
            }
        });
        fs::write(app.join("latest.json"), manifest.to_string()).unwrap();
        Some(Self {
            go: GoServices::start(None, Some(releases.path()))?,
            _releases: releases,
        })
    }

    fn endpoint(&self) -> Endpoint {
        self.go.endpoint()
    }
}

fn updater(
    endpoint: Endpoint,
    data: &Path,
    current: &str,
    app_image: Option<&Path>,
) -> Arc<AppUpdater> {
    Arc::new(AppUpdater::new(
        endpoint,
        Some(data.to_owned()),
        current.to_owned(),
        Some(TEST_KEY),
        app_image.map(Path::to_owned),
        Arc::new(|_| {}),
    ))
}

fn sign_in(updater: &Arc<AppUpdater>) {
    updater.sign_in("demo-a".into(), Zeroizing::new("local-demo".into()));
}

fn wait_for(updater: &AppUpdater, done: impl Fn(&AppUpdateView) -> bool) -> AppUpdateView {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        let view = updater.view();
        if done(&view) {
            return view;
        }
        assert!(Instant::now() < deadline, "gave up waiting: {view:?}");
        thread::sleep(Duration::from_millis(50));
    }
}

#[test]
fn a_release_is_found_and_downloaded_after_sign_in_and_kept_across_restarts() {
    let Some(mock) = Services::start(SIGNATURE) else {
        return;
    };
    let data = tempfile::tempdir().unwrap();
    let app_image = data.path().join("DLA_Downloader.AppImage");
    fs::write(&app_image, b"old").unwrap();
    let updater = updater(mock.endpoint(), data.path(), "0.3.0", Some(&app_image));
    let view = updater.view();
    assert!(view.enabled && view.installable && !view.signed_in);
    assert_eq!(view.latest, None);

    sign_in(&updater);
    let view = wait_for(&updater, |view| view.ready);
    assert_eq!(view.latest.as_deref(), Some("0.4.0"));
    assert_eq!(view.notes.as_deref(), Some("Faster downloads."));
    assert!(view.required && updater.required());
    assert!(view.last_checked.is_some() && view.problem.is_none());

    // The next start knows it without signing in.
    let reopened = self::updater(mock.endpoint(), data.path(), "0.3.0", Some(&app_image));
    let view = reopened.view();
    assert!(view.ready && view.required && !view.signed_in);

    // Busy work keeps the app running; otherwise the installer is handed over.
    let refused = reopened.begin_install(true).err().unwrap();
    assert_eq!(refused.problem.unwrap().code, AppUpdateProblemCode::Busy);
    let installer = reopened.begin_install(false).ok().unwrap();
    assert!(reopened.view().installing);
    assert_eq!(installer.app_image.as_deref(), Some(app_image.as_path()));
    dla_app_update::replace_app_image(&app_image, &installer.path).unwrap();
    assert_eq!(fs::read(&app_image).unwrap(), PAYLOAD);
    assert!(reopened.begin_install(false).is_err(), "only once");
}

#[test]
fn an_installer_signed_by_another_key_is_reported_and_never_ready() {
    let Some(mock) = Services::start(OTHER_SIGNATURE) else {
        return;
    };
    let data = tempfile::tempdir().unwrap();
    let updater = updater(mock.endpoint(), data.path(), "0.3.0", None);
    sign_in(&updater);
    let view = wait_for(&updater, |view| view.problem.is_some());
    assert_eq!(view.problem.unwrap().code, AppUpdateProblemCode::Signature);
    assert!(!view.ready && view.downloading.is_none());
    assert_eq!(view.latest.as_deref(), Some("0.4.0"));
    // Nothing to hand over, even when idle.
    assert!(updater.begin_install(false).is_err());
}

#[test]
fn a_current_version_has_nothing_to_offer() {
    let Some(mock) = Services::start(SIGNATURE) else {
        return;
    };
    let data = tempfile::tempdir().unwrap();
    let updater = updater(mock.endpoint(), data.path(), "0.4.0", None);
    sign_in(&updater);
    let view = wait_for(&updater, |view| {
        view.last_checked.is_some() && !view.checking
    });
    assert_eq!(view.latest, None);
    assert!(!view.ready && !view.required && view.problem.is_none());
    assert!(!data.path().join("updates/downloads").exists());
}

#[test]
fn an_installed_update_is_forgotten_on_the_next_start() {
    let data = tempfile::tempdir().unwrap();
    let updates = data.path().join("updates");
    fs::create_dir_all(updates.join("downloads/0.4.0")).unwrap();
    fs::write(updates.join("downloads/0.4.0/setup.exe"), b"x").unwrap();
    fs::write(
        updates.join(STATE),
        json!({
            "lastChecked": 100,
            "latest": "0.4.0",
            "required": true,
            "installer": updates.join("downloads/0.4.0/setup.exe"),
        })
        .to_string(),
    )
    .unwrap();
    let endpoint = Endpoint::self_test("http://127.0.0.1:9/v1").unwrap();
    let updater = updater(endpoint, data.path(), "0.4.0", None);
    let view = updater.view();
    assert_eq!(view.latest, None);
    assert!(!view.ready && !view.required);
    assert_eq!(view.last_checked, Some(100));
    // Forgotten on disk too, not only in memory.
    let saved: serde_json::Value =
        serde_json::from_slice(&fs::read(updates.join(STATE)).unwrap()).unwrap();
    assert_eq!(saved["latest"], serde_json::Value::Null);
    assert_eq!(saved["installer"], serde_json::Value::Null);
    // The installer that started this version may still run, so its
    // download goes a little later.
    assert!(updates.join("downloads").exists());
    updater.tidy();
    assert!(!updates.join("downloads").exists());
}

#[test]
fn downloads_waiting_to_be_installed_are_kept() {
    let Some(mock) = Services::start(SIGNATURE) else {
        return;
    };
    let data = tempfile::tempdir().unwrap();
    let updater = updater(mock.endpoint(), data.path(), "0.3.0", None);
    sign_in(&updater);
    wait_for(&updater, |view| view.ready);
    updater.tidy();
    assert!(updater.view().ready);
    assert!(data.path().join("updates/downloads/0.4.0").is_dir());
}

#[test]
fn without_the_update_key_nothing_is_checked() {
    let data = tempfile::tempdir().unwrap();
    let endpoint = Endpoint::self_test("http://127.0.0.1:9/v1").unwrap();
    let updater = Arc::new(AppUpdater::new(
        endpoint,
        Some(data.path().to_owned()),
        "0.3.0".into(),
        None,
        None,
        Arc::new(|_| {}),
    ));
    sign_in(&updater);
    let view = updater.check();
    assert!(!view.enabled && !view.signed_in && !view.checking);
}
