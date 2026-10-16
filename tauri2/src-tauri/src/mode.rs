use std::sync::Arc;

use dla_remote::{Endpoint, EndpointError};

const UPLOAD_CA: &[u8] = include_bytes!("../../certs/upload-ca.crt");
const DEFAULT_SELF_TEST_SERVICE: &str = "http://127.0.0.1:9092/v1";

/// SFTPGo's web client page for changing the account password, a fallback
/// for when the app's own form cannot reach the service.
pub const PASSWORD_PAGE: &str = "https://sftpgo.dlsitearchive.com/web/client/changepwd";

pub const SELF_TEST_ACCOUNTS: [&str; 2] = ["demo-a", "demo-b"];

pub enum Mode {
    SelfTest(Endpoint),
    Private,
}

/// Selects the private service by default. The self-test is chosen only
/// explicitly and accepts only a plain-HTTP loopback service.
pub fn from_environment() -> Result<Mode, EndpointError> {
    let self_test = std::env::args().any(|argument| argument == "--self-test")
        || std::env::var("DLA_DOWNLOADER_MODE").as_deref() == Ok("self-test");
    if !self_test {
        return Ok(Mode::Private);
    }
    let service = std::env::var("DLA_SELF_TEST_SERVICE")
        .unwrap_or_else(|_| DEFAULT_SELF_TEST_SERVICE.to_owned());
    Ok(Mode::SelfTest(Endpoint::self_test(&service)?))
}

pub fn private_endpoint() -> Endpoint {
    Endpoint::private(Arc::from(UPLOAD_CA))
}
