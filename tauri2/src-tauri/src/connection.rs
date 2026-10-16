use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use dla_connector::{Connector, ConnectorConfig, ConnectorState};
use dla_remote::Endpoint;
use serde::Serialize;

use crate::{access, mode};

const PROBE_WHILE_DOWN: Duration = Duration::from_secs(5);
const PROBE_WHILE_UP: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum RouteView {
    SelfTest,
    Starting,
    Connecting,
    Connected,
    Unreachable { detail: String },
    Restarting { seconds: u64 },
    Unavailable { detail: String },
}

/// The private route to the download service.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConnectionView {
    pub archive: RouteView,
}

pub type ConnectionNotifier = Arc<dyn Fn(&ConnectionView) + Send + Sync>;

struct Status {
    view: Mutex<ConnectionView>,
    notify: ConnectionNotifier,
}

impl Status {
    fn get(&self) -> RouteView {
        self.view.lock().expect("connection lock").archive.clone()
    }

    fn set(&self, next: RouteView) {
        let mut view = self.view.lock().expect("connection lock");
        let slot = &mut view.archive;
        if *slot != next {
            *slot = next;
            let snapshot = view.clone();
            drop(view);
            (self.notify)(&snapshot);
        }
    }
}

/// Owns the private route to DLA: the bundled `cloudflared` forward plus a
/// periodic probe that tells the window whether the service answers.
pub struct Connection {
    status: Arc<Status>,
    stop: Arc<AtomicBool>,
    connectors: Mutex<Vec<Connector>>,
}

impl Connection {
    pub fn self_test(notify: ConnectionNotifier) -> Self {
        Self::with(RouteView::SelfTest, notify)
    }

    fn with(view: RouteView, notify: ConnectionNotifier) -> Self {
        Self {
            status: Arc::new(Status {
                view: Mutex::new(ConnectionView { archive: view }),
                notify,
            }),
            stop: Arc::new(AtomicBool::new(false)),
            connectors: Mutex::new(Vec::new()),
        }
    }

    /// Starts the private route and returns the endpoint that dials it.
    pub fn private(notify: ConnectionNotifier) -> (Endpoint, Self) {
        let endpoint = mode::private_endpoint();
        let Some(access) = access::embedded() else {
            let detail = "This build has no DLA access token. Ask DLA for a configured build.";
            return (endpoint, Self::with(unavailable(detail), notify));
        };
        let program = sidecar();
        if !program.is_file() {
            let detail = "The private connector is missing from this installation. Reinstall DLA Downloader.";
            return (endpoint, Self::with(unavailable(detail), notify));
        }
        let connection = Self::with(RouteView::Starting, notify);
        let endpoint = connection.open(
            ConnectorConfig {
                program,
                hostname: access.hostname,
                token: access.token,
            },
            endpoint,
        );
        (endpoint, connection)
    }

    fn open(&self, config: ConnectorConfig, endpoint: Endpoint) -> Endpoint {
        let status = Arc::clone(&self.status);
        let connector = Connector::start(config, move |state| match state {
            ConnectorState::Starting => status.set(RouteView::Starting),
            ConnectorState::Listening { .. } => status.set(RouteView::Connecting),
            ConnectorState::Restarting { seconds, .. } => {
                status.set(RouteView::Restarting { seconds: *seconds })
            }
            ConnectorState::Stopped => {}
        });
        let endpoint = endpoint.through(connector.dial());
        let probe = endpoint.clone();
        let status = Arc::clone(&self.status);
        let stop = Arc::clone(&self.stop);
        thread::spawn(move || {
            while !stop.load(Ordering::SeqCst) {
                let listening = !matches!(
                    status.get(),
                    RouteView::Starting | RouteView::Restarting { .. }
                );
                let wait = if listening {
                    match probe.probe() {
                        Ok(()) => {
                            status.set(RouteView::Connected);
                            PROBE_WHILE_UP
                        }
                        Err(error) => {
                            status.set(RouteView::Unreachable {
                                detail: error.message,
                            });
                            PROBE_WHILE_DOWN
                        }
                    }
                } else {
                    Duration::from_millis(250)
                };
                sleep_unless(&stop, wait);
            }
        });
        self.connectors
            .lock()
            .expect("connector lock")
            .push(connector);
        endpoint
    }

    pub fn view(&self) -> ConnectionView {
        self.status.view.lock().expect("connection lock").clone()
    }

    pub fn shutdown(&self) {
        self.stop.store(true, Ordering::SeqCst);
        for mut connector in self.connectors.lock().expect("connector lock").drain(..) {
            connector.stop();
        }
    }
}

fn unavailable(detail: &str) -> RouteView {
    RouteView::Unavailable {
        detail: detail.to_owned(),
    }
}

fn sleep_unless(stop: &AtomicBool, duration: Duration) {
    let steps = (duration.as_millis() / 250).max(1);
    for _ in 0..steps {
        if stop.load(Ordering::SeqCst) {
            return;
        }
        thread::sleep(Duration::from_millis(250));
    }
}

/// Tauri installs external binaries beside the application executable.
fn sidecar() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|executable| executable.parent().map(PathBuf::from))
        .unwrap_or_default()
        .join(format!("cloudflared{}", std::env::consts::EXE_SUFFIX))
}
