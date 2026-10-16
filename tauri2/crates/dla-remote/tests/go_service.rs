//! The client against the receiver's real downloads listener and the Go mock
//! SFTPGo behind it. Both come from DLA's server code, which is not part of
//! this repository: point DLA_GO_SELFTEST_BIN at them, or these tests skip.
//! DLA_REQUIRE_GO_RECEIVER=1 makes them fail instead.

use std::{
    fs,
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use dla_remote::{
    AccountRefusal, ArchiveClient, Credentials, Endpoint, Fetched, RangeChunk, Recovery,
    RemoteErrorKind,
};
use zeroize::Zeroizing;

const PASSWORD: &str = "local-demo";

struct Services {
    children: Vec<Child>,
    port: u16,
    _directories: Vec<tempfile::TempDir>,
}

impl Drop for Services {
    fn drop(&mut self) {
        for child in &mut self.children {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

fn free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn binaries() -> PathBuf {
    std::env::var_os("DLA_GO_SELFTEST_BIN")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../build/selftest-bin")
        })
}

/// Serves `archive` as /dlsite through the receiver's downloads listener.
fn start(archive: &Path) -> Option<Services> {
    let binaries = binaries();
    if !binaries.join("mockdav").is_file() || !binaries.join("receiver").is_file() {
        assert!(
            std::env::var_os("DLA_REQUIRE_GO_RECEIVER").is_none(),
            "Go self-test binaries are missing"
        );
        eprintln!("skipped: no Go self-test binaries (see DLA_GO_SELFTEST_BIN)");
        return None;
    }
    let data = tempfile::tempdir().unwrap();
    let staging = tempfile::tempdir().unwrap();
    let (mock, receiver, port) = (free_port(), free_port(), free_port());
    let children = vec![
        Command::new(binaries.join("mockdav"))
            .env("DLA_MOCK_LISTEN", format!("127.0.0.1:{mock}"))
            .env("DLA_MOCK_DATA", data.path())
            .env("DLA_MOCK_DLSITE", archive)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
        Command::new(binaries.join("receiver"))
            .env("DLA_RECEIVER_MODE", "mock")
            .env("DLA_RECEIVER_DATA", staging.path())
            .env(
                "DLA_MOCK_URL",
                format!("http://127.0.0.1:{mock}/dav/upload"),
            )
            .env("DLA_RECEIVER_LISTEN", format!("127.0.0.1:{receiver}"))
            .env("DLA_DOWNLOADS_LISTEN", format!("127.0.0.1:{port}"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    ];
    let services = Services {
        children,
        port,
        _directories: vec![data, staging],
    };
    for port in [mock, port] {
        let deadline = Instant::now() + Duration::from_secs(10);
        while TcpStream::connect(("127.0.0.1", port)).is_err() {
            assert!(Instant::now() < deadline, "port {port} did not open");
            thread::sleep(Duration::from_millis(50));
        }
    }
    Some(services)
}

impl Services {
    fn endpoint(&self) -> Endpoint {
        Endpoint::self_test(&format!("http://127.0.0.1:{}/v1", self.port)).unwrap()
    }

    fn client(&self, username: &str, password: &str) -> ArchiveClient {
        ArchiveClient::new(
            &self.endpoint(),
            Credentials {
                username: username.to_owned(),
                password: Zeroizing::new(password.to_owned()),
            },
        )
        .unwrap()
    }
}

fn archive() -> tempfile::TempDir {
    let archive = tempfile::tempdir().unwrap();
    fs::create_dir_all(archive.path().join("2026-10")).unwrap();
    fs::write(
        archive.path().join("2026-10/RJ01234567 作品.zip"),
        b"0123456789abcdefghij",
    )
    .unwrap();
    fs::write(archive.path().join("2026-10/_manifest.json"), b"{}").unwrap();
    archive
}

#[test]
fn signs_in_lists_fetches_and_resumes_through_the_download_service() {
    let archive = archive();
    let Some(services) = start(archive.path()) else {
        return;
    };
    assert!(services.endpoint().probe().is_ok());
    let client = services.client("demo-a", PASSWORD);
    let info = client.verify().unwrap();
    assert_eq!(info.root, "/dlsite");
    assert_eq!(info.max_downloads, 1);

    let months = client.list("/dlsite").unwrap();
    assert!(
        months
            .iter()
            .any(|entry| entry.name == "2026-10" && entry.is_dir)
    );
    let files = client.list("/dlsite/2026-10").unwrap();
    let zip = files
        .iter()
        .find(|entry| entry.name == "RJ01234567 作品.zip")
        .expect("the archive is listed");
    assert_eq!(zip.path, "/dlsite/2026-10/RJ01234567 作品.zip");
    assert_eq!(zip.size, 20);
    assert!(zip.modified.is_some());

    let Fetched::Content { data, .. } = client
        .fetch("/dlsite/2026-10/_manifest.json", 1 << 20, None)
        .unwrap()
    else {
        panic!("the manifest has content");
    };
    assert_eq!(data, b"{}");

    let RangeChunk::Data { data, total, etag } = client.read_range(&zip.path, 0, 8, None).unwrap()
    else {
        panic!("a first chunk");
    };
    assert_eq!((data.as_slice(), total), (&b"01234567"[..], 20));
    let RangeChunk::Data { data, .. } = client
        .read_range(&zip.path, 15, 8, etag.as_deref())
        .unwrap()
    else {
        panic!("a resumed chunk");
    };
    assert_eq!(data, b"fghij");
    assert!(matches!(
        client.read_range(&zip.path, 8, 8, Some("Thu, 01 Jan 1970 00:00:00 GMT")),
        Ok(RangeChunk::Changed)
    ));

    let refused = services.client("demo-a", "wrong").verify().unwrap_err();
    assert_eq!(refused.kind, RemoteErrorKind::Status(401));
    let outside = client.list("/private").unwrap_err();
    assert_eq!(outside.kind, RemoteErrorKind::Status(400));
}

#[test]
fn the_account_changes_its_email_and_password_through_the_download_service() {
    let archive = archive();
    let Some(services) = start(archive.path()) else {
        return;
    };
    let client = services.client("demo-a", PASSWORD);
    assert_eq!(client.email().unwrap(), "");
    assert_eq!(
        client.set_email(" t3@example.org ").unwrap(),
        "t3@example.org"
    );
    assert_eq!(client.email().unwrap(), "t3@example.org");
    assert_eq!(
        client.change_password("short"),
        Err(AccountRefusal::Rule {
            rule: "too_short".into(),
            detail: "The new password is too short.".into()
        })
    );
    assert_eq!(
        services
            .client("demo-a", "wrong")
            .change_password("a-new-password"),
        Err(AccountRefusal::Current)
    );
    client.change_password("a-new-password").unwrap();
    services
        .client("demo-a", "a-new-password")
        .verify()
        .unwrap();
    assert_eq!(
        services
            .client("demo-a", PASSWORD)
            .verify()
            .unwrap_err()
            .kind,
        RemoteErrorKind::Status(401)
    );

    let recovery = Recovery::new(&services.endpoint()).unwrap();
    recovery.send_code("demo-a").unwrap();
    assert!(matches!(
        recovery.reset("demo-a", "not-the-code", "another-password"),
        Err(AccountRefusal::Rule { rule, .. }) if rule == "code"
    ));
}
