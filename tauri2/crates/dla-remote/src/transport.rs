use std::{
    fmt,
    net::{IpAddr, SocketAddr},
    sync::Arc,
    time::Duration,
};

use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde::{Deserialize, de::DeserializeOwned};

use crate::error::{RemoteError, RemoteErrorKind};
use thiserror::Error;
use ureq::{
    Agent, Body,
    config::Config,
    http::{Response, Uri},
    tls::{Certificate, RootCerts, TlsConfig, TlsProvider},
    unversioned::{
        resolver::{ResolvedSocketAddrs, Resolver},
        transport::{DefaultConnector, NextTimeout},
    },
};
use zeroize::Zeroizing;

pub const PRIVATE_ARCHIVE: &str = "https://172.30.250.2:5009/v1";

/// The download service's API version this app speaks.
pub(crate) const PROTOCOL: u32 = 1;

pub(crate) const MAX_RESPONSE_BYTES: u64 = 8 << 20;
pub(crate) const JSON_TIMEOUT: Duration = Duration::from_secs(15);
const CONNECT_TIMEOUT: Duration = Duration::from_secs(5);
const RESPONSE_TIMEOUT: Duration = Duration::from_secs(30);

/// Where to open the TCP connection instead of the server's own address, such
/// as the local `cloudflared access tcp` listener. TLS is still verified for
/// the server's address. `None` means the forward is not ready yet.
pub type Dial = Arc<dyn Fn() -> Option<SocketAddr> + Send + Sync>;

#[derive(Clone)]
enum Target {
    Private { upload_ca_pem: Arc<[u8]> },
    SelfTest { base: String },
}

/// The download service: the private one, or a loopback stand-in.
#[derive(Clone)]
pub struct Endpoint {
    target: Target,
    dial: Option<Dial>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EndpointError {
    #[error("the self-test endpoint must be plain HTTP on a loopback address at /v1")]
    SelfTestAddress,
    #[error("the private upload CA certificate is missing or invalid")]
    UploadCa,
}

impl Endpoint {
    /// The fixed private service, trusted only through the DLA upload CA.
    pub fn private(upload_ca_pem: Arc<[u8]>) -> Self {
        Self {
            target: Target::Private { upload_ca_pem },
            dial: None,
        }
    }

    /// A plain-HTTP service on a loopback address, used by the self-test.
    pub fn self_test(base: &str) -> Result<Self, EndpointError> {
        let url = url::Url::parse(base).map_err(|_| EndpointError::SelfTestAddress)?;
        let loopback = match url.host() {
            Some(url::Host::Ipv4(ip)) => IpAddr::V4(ip).is_loopback(),
            Some(url::Host::Ipv6(ip)) => IpAddr::V6(ip).is_loopback(),
            _ => false,
        };
        if url.scheme() != "http"
            || !loopback
            || url.path() != "/v1"
            || !url.username().is_empty()
            || url.password().is_some()
            || url.query().is_some()
            || url.fragment().is_some()
        {
            return Err(EndpointError::SelfTestAddress);
        }
        Ok(Self {
            target: Target::SelfTest {
                base: url.as_str().to_owned(),
            },
            dial: None,
        })
    }

    pub fn through(mut self, dial: Dial) -> Self {
        self.dial = Some(dial);
        self
    }

    pub fn is_self_test(&self) -> bool {
        matches!(self.target, Target::SelfTest { .. })
    }

    pub fn base(&self) -> &str {
        match &self.target {
            Target::Private { .. } => PRIVATE_ARCHIVE,
            Target::SelfTest { base } => base,
        }
    }

    pub(crate) fn agent(&self) -> Result<Agent, EndpointError> {
        let tls = match &self.target {
            Target::Private { upload_ca_pem } => {
                let ca =
                    Certificate::from_pem(upload_ca_pem).map_err(|_| EndpointError::UploadCa)?;
                TlsConfig::builder()
                    .provider(TlsProvider::Rustls)
                    .root_certs(RootCerts::new_with_certs(&[ca]))
                    .build()
            }
            Target::SelfTest { .. } => TlsConfig::default(),
        };
        let config = Agent::config_builder()
            .https_only(!self.is_self_test())
            .tls_config(tls)
            .proxy(None)
            .max_redirects(0)
            .max_redirects_will_error(false)
            .http_status_as_error(false)
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_recv_response(Some(RESPONSE_TIMEOUT))
            .timeout_recv_body(Some(RESPONSE_TIMEOUT))
            .user_agent(concat!("dla-downloader/", env!("CARGO_PKG_VERSION")))
            .build();
        Ok(match &self.dial {
            Some(dial) => Agent::with_parts(
                config,
                DefaultConnector::default(),
                DialResolver(Arc::clone(dial)),
            ),
            None => config.into(),
        })
    }

    /// Checks, without an account, that the service answers with a verified
    /// certificate and speaks this app's protocol.
    pub fn probe(&self) -> Result<(), RemoteError> {
        #[derive(Deserialize)]
        struct Health {
            service: String,
            protocol: u32,
        }
        let agent = self
            .agent()
            .map_err(|error| RemoteError::new(RemoteErrorKind::Certificate, error.to_string()))?;
        let health = format!("{}/healthz", self.base().trim_end_matches("/v1"));
        let response = agent
            .get(health)
            .config()
            .timeout_global(Some(JSON_TIMEOUT))
            .build()
            .header("Accept", "application/json")
            .call();
        let health: Health = decode_json(response, None)?;
        if health.service != "dla-downloads" || health.protocol != PROTOCOL {
            return Err(incompatible());
        }
        Ok(())
    }
}

struct DialResolver(Dial);

impl fmt::Debug for DialResolver {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("DialResolver")
    }
}

impl Resolver for DialResolver {
    fn resolve(
        &self,
        _uri: &Uri,
        _config: &Config,
        _timeout: NextTimeout,
    ) -> Result<ResolvedSocketAddrs, ureq::Error> {
        let address = (self.0)().ok_or(ureq::Error::HostNotFound)?;
        let mut addresses = self.empty();
        addresses.push(address);
        Ok(addresses)
    }
}

pub struct Credentials {
    pub username: String,
    pub password: Zeroizing<String>,
}

impl Credentials {
    pub(crate) fn authorization(&self) -> Zeroizing<String> {
        let token = Zeroizing::new(format!("{}:{}", self.username, self.password.as_str()));
        Zeroizing::new(format!("Basic {}", STANDARD.encode(token.as_bytes())))
    }
}

#[derive(Deserialize)]
struct Failure {
    error: String,
}

/// Reads at most `limit` bytes. ureq's own limit fails on a body of exactly
/// the limit, so one extra byte is allowed and then rejected here.
pub(crate) fn read_limited(
    response: &mut Response<Body>,
    limit: u64,
) -> Result<Vec<u8>, RemoteError> {
    let interrupted = || {
        RemoteError::new(
            RemoteErrorKind::Connection,
            "The response from DLA was interrupted.",
        )
    };
    let data = response
        .body_mut()
        .with_config()
        .limit(limit.saturating_add(1))
        .read_to_vec()
        .map_err(|_| interrupted())?;
    if data.len() as u64 > limit {
        return Err(interrupted());
    }
    Ok(data)
}

/// Maps a non-success reply to an error, keeping the server's own short
/// explanation (receiver JSON or SFTPGo's plain text) when there is one.
pub(crate) fn status_error(status: u16, body: &[u8]) -> RemoteError {
    let message = serde_json::from_slice::<Failure>(body)
        .ok()
        .map(|failure| failure.error)
        .or_else(|| {
            std::str::from_utf8(body)
                .ok()
                .map(str::trim)
                .filter(|text| !text.is_empty() && text.len() <= 200 && !text.starts_with('<'))
                .map(str::to_owned)
        })
        .filter(|message| !message.trim().is_empty())
        .unwrap_or_else(|| {
            match status {
                401 => "Your account or its access was rejected.",
                403 => "Your account is not allowed to do this.",
                404 => "Not found on the DLA server.",
                429 => "Too many connections for this account right now.",
                _ => "The DLA server could not complete this request.",
            }
            .to_owned()
        });
    RemoteError::new(RemoteErrorKind::Status(status), message)
}

pub(crate) fn decode_json<T: DeserializeOwned>(
    response: Result<Response<Body>, ureq::Error>,
    paused: Option<&dyn Fn() -> bool>,
) -> Result<T, RemoteError> {
    let mut response = response.map_err(|error| {
        if paused.is_some_and(|paused| paused()) {
            RemoteError::new(RemoteErrorKind::Paused, "Paused.")
        } else {
            transport_error(&error)
        }
    })?;
    let status = response.status().as_u16();
    let data = read_limited(&mut response, MAX_RESPONSE_BYTES)?;
    if !(200..300).contains(&status) {
        return Err(status_error(status, &data));
    }
    serde_json::from_slice(&data).map_err(|_| invalid_response())
}

pub(crate) fn transport_error(error: &ureq::Error) -> RemoteError {
    match error {
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => RemoteError::new(
            RemoteErrorKind::Certificate,
            "The DLA server's certificate could not be verified with the DLA upload CA.",
        ),
        ureq::Error::HostNotFound => RemoteError::new(
            RemoteErrorKind::Connection,
            "The private connection to DLA is not ready yet.",
        ),
        _ => RemoteError::new(
            RemoteErrorKind::Connection,
            "The connection to DLA was interrupted. Downloaded progress is kept; reconnect to continue.",
        ),
    }
}

pub(crate) fn incompatible() -> RemoteError {
    RemoteError::new(
        RemoteErrorKind::Incompatible,
        "The DLA download service does not match this version of DLA Downloader.",
    )
}

pub(crate) fn invalid_response() -> RemoteError {
    RemoteError::new(
        RemoteErrorKind::InvalidResponse,
        "The DLA server returned an invalid response.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn self_test_endpoints_must_be_loopback_http_at_v1() {
        for accepted in ["http://127.0.0.1:9092/v1", "http://[::1]:9092/v1"] {
            assert!(Endpoint::self_test(accepted).is_ok(), "{accepted}");
        }
        for rejected in [
            "https://127.0.0.1:9092/v1",
            "http://localhost:9092/v1",
            "http://10.0.0.5:9092/v1",
            "http://127.0.0.1:9092/v2",
            "http://127.0.0.1:9092/v1/",
            "http://user:pass@127.0.0.1:9092/v1",
            "http://127.0.0.1:9092/v1?x=1",
        ] {
            assert_eq!(
                Endpoint::self_test(rejected).err(),
                Some(EndpointError::SelfTestAddress),
                "{rejected}"
            );
        }
    }

    #[test]
    fn the_private_base_is_fixed() {
        let ca: Arc<[u8]> = Arc::from(&b"x"[..]);
        assert_eq!(Endpoint::private(ca).base(), PRIVATE_ARCHIVE);
    }

    #[test]
    fn server_text_is_kept_but_html_is_not() {
        assert_eq!(
            status_error(403, b"permission denied").message,
            "permission denied"
        );
        assert_eq!(
            status_error(403, br#"{"error":"Quota exceeded."}"#).message,
            "Quota exceeded."
        );
        assert_eq!(
            status_error(404, b"<html>nope</html>").message,
            "Not found on the DLA server."
        );
    }
}
