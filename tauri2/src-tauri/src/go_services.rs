//! The receiver's real downloads listener in front of the Go mock SFTPGo, on
//! random loopback ports. Both come from DLA's server code, which is not part
//! of this repository: point DLA_GO_SELFTEST_BIN at them, or the tests that
//! need them skip. DLA_REQUIRE_GO_RECEIVER=1 makes them fail instead.

use std::{
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use dla_remote::Endpoint;

pub const PASSWORD: &str = "local-demo";

pub struct GoServices {
    children: Vec<Child>,
    port: u16,
    _directories: Vec<tempfile::TempDir>,
}

impl Drop for GoServices {
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
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../build/selftest-bin"))
}

impl GoServices {
    /// Serves `archive` as /dlsite and `releases` as /releases; `None` when
    /// the binaries are missing.
    pub fn start(archive: Option<&Path>, releases: Option<&Path>) -> Option<Self> {
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
        let mut mockdav = Command::new(binaries.join("mockdav"));
        mockdav
            .env("DLA_MOCK_LISTEN", format!("127.0.0.1:{mock}"))
            .env("DLA_MOCK_DATA", data.path())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(archive) = archive {
            mockdav.env("DLA_MOCK_DLSITE", archive);
        }
        if let Some(releases) = releases {
            mockdav.env("DLA_MOCK_RELEASES", releases);
        }
        let children = vec![
            mockdav.spawn().unwrap(),
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
        let services = Self {
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

    pub fn endpoint(&self) -> Endpoint {
        Endpoint::self_test(&format!("http://127.0.0.1:{}/v1", self.port)).unwrap()
    }
}
