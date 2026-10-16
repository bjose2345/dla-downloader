use std::sync::{Arc, Mutex, MutexGuard};

use dla_remote::{
    AccountRefusal, ArchiveClient, Credentials, Endpoint, Recovery, RemoteError, RemoteErrorKind,
};
use serde::Serialize;
use zeroize::Zeroizing;

use crate::{local_store::LocalStore, mode::SELF_TEST_ACCOUNTS};

pub type Notifier = Arc<dyn Fn(&SessionView) + Send + Sync>;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoticeCode {
    SignInRejected,
    /// Too many wrong passwords for this account; it has to wait.
    TooManyAttempts,
    SelfTestAccountOnly,
    Unreachable,
    Certificate,
    Incompatible,
    InvalidResponse,
    Server,
}

/// A stable code for the interface to phrase, plus untranslated technical
/// detail such as the server's own message.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notice {
    pub code: NoticeCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<u16>,
}

impl Notice {
    fn new(code: NoticeCode) -> Self {
        Self {
            code,
            detail: None,
            status: None,
        }
    }

    fn with_detail(code: NoticeCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            detail: Some(detail.into()),
            status: None,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionView {
    /// Increases with every change so the interface can ignore stale views.
    pub revision: u64,
    pub account: Option<String>,
    pub notice: Option<Notice>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppInfo {
    pub version: &'static str,
    pub self_test: bool,
    pub service: String,
    pub last_account: Option<String>,
    pub remember_account: bool,
}

/// Why an account change (password, email, recovery) did not happen, as a
/// stable code for the interface to phrase, with untranslated detail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AccountProblem {
    pub code: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl AccountProblem {
    pub fn new(code: &str) -> Self {
        Self {
            code: code.to_owned(),
            detail: None,
        }
    }

    fn with_detail(code: &str, detail: String) -> Self {
        Self {
            code: code.to_owned(),
            detail: Some(detail),
        }
    }
}

impl From<AccountRefusal> for AccountProblem {
    fn from(refusal: AccountRefusal) -> Self {
        match refusal {
            AccountRefusal::Current => Self::new("current"),
            AccountRefusal::TooMany => Self::new("too_many"),
            AccountRefusal::Rule { rule, detail } => Self {
                code: rule,
                detail: Some(detail).filter(|detail| !detail.is_empty()),
            },
            AccountRefusal::NotAllowed => Self::new("not_allowed"),
            AccountRefusal::Unavailable => Self::new("unavailable"),
            AccountRefusal::Failed(error) => Self::with_detail("failed", error.message),
        }
    }
}

/// The signed-in account: who it is, and its password and email.
pub struct Session {
    endpoint: Endpoint,
    store: LocalStore,
    notify: Notifier,
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    revision: u64,
    account: Option<Arc<ArchiveClient>>,
    notice: Option<Notice>,
}

impl Inner {
    fn view(&self) -> SessionView {
        SessionView {
            revision: self.revision,
            account: self
                .account
                .as_ref()
                .map(|client| client.username().to_owned()),
            notice: self.notice.clone(),
        }
    }

    fn username(&self) -> Option<String> {
        self.account
            .as_ref()
            .map(|client| client.username().to_owned())
    }
}

impl Session {
    pub fn new(endpoint: Endpoint, store: LocalStore, notify: Notifier) -> Self {
        Self {
            endpoint,
            store,
            notify,
            inner: Mutex::new(Inner::default()),
        }
    }

    pub fn info(&self) -> AppInfo {
        AppInfo {
            version: env!("CARGO_PKG_VERSION"),
            self_test: self.endpoint.is_self_test(),
            service: self.endpoint.base().to_owned(),
            last_account: self.store.last_account(),
            remember_account: self.store.remembers_account(),
        }
    }

    pub fn view(&self) -> SessionView {
        self.lock().view()
    }

    pub fn set_remember_account(&self, remember: bool) -> AppInfo {
        self.store.set_remember_account(remember);
        if remember && let Some(account) = self.lock().username() {
            self.store.set_last_account(&account);
        }
        self.info()
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().expect("session lock")
    }

    fn changed(&self, mut inner: MutexGuard<'_, Inner>) -> SessionView {
        inner.revision += 1;
        let view = inner.view();
        drop(inner);
        (self.notify)(&view);
        view
    }

    fn fail(&self, notice: Notice) -> SessionView {
        let mut inner = self.lock();
        inner.notice = Some(notice);
        self.changed(inner)
    }

    fn client(
        &self,
        username: String,
        password: Zeroizing<String>,
    ) -> Result<ArchiveClient, String> {
        ArchiveClient::new(&self.endpoint, Credentials { username, password })
            .map_err(|error| error.to_string())
    }

    /// Blocks on the network; call it outside the async runtime.
    pub fn sign_in(&self, username: String, password: Zeroizing<String>) -> SessionView {
        if self.lock().account.is_some() {
            return self.view();
        }
        if self.endpoint.is_self_test() && !SELF_TEST_ACCOUNTS.contains(&username.as_str()) {
            return self.fail(Notice::new(NoticeCode::SelfTestAccountOnly));
        }
        let client = match self.client(username.clone(), password) {
            Ok(client) => client,
            Err(detail) => return self.fail(Notice::with_detail(NoticeCode::Certificate, detail)),
        };
        if let Err(error) = client.verify() {
            let notice = match error.kind {
                // SFTPGo answers an account that may not use the service like
                // a wrong password, so the two are told apart nowhere.
                RemoteErrorKind::Status(401 | 403) => Notice::new(NoticeCode::SignInRejected),
                RemoteErrorKind::Status(429) => Notice::new(NoticeCode::TooManyAttempts),
                _ => server_notice(error),
            };
            return self.fail(notice);
        }
        self.store.set_last_account(&username);
        let mut inner = self.lock();
        if inner.account.is_none() {
            inner.account = Some(Arc::new(client));
            inner.notice = None;
        }
        self.changed(inner)
    }

    pub fn sign_out(&self) -> SessionView {
        let mut inner = self.lock();
        inner.account = None;
        inner.notice = None;
        self.changed(inner)
    }

    /// Changes the signed-in account's password and keeps the session on the
    /// new one. Returns the account name. Blocks on the network.
    pub fn change_password(
        &self,
        current: Zeroizing<String>,
        new_password: Zeroizing<String>,
    ) -> Result<String, AccountProblem> {
        let username = self
            .lock()
            .username()
            .ok_or(AccountProblem::new("signed_out"))?;
        let client = |password| {
            self.client(username.clone(), password)
                .map_err(|detail| AccountProblem::with_detail("failed", detail))
        };
        client(current)?
            .change_password(&new_password)
            .map_err(AccountProblem::from)?;
        let renewed = Arc::new(client(new_password)?);
        let mut inner = self.lock();
        if inner.username().as_deref() == Some(username.as_str()) {
            inner.account = Some(renewed);
        }
        Ok(username)
    }

    /// The signed-in account's email address, "" without one.
    pub fn account_email(&self) -> Result<String, AccountProblem> {
        let account = self
            .lock()
            .account
            .clone()
            .ok_or(AccountProblem::new("signed_out"))?;
        account.email().map_err(AccountProblem::from)
    }

    /// Sets the signed-in account's email address with the current password,
    /// asked for again. Returns the address as saved.
    pub fn set_email(
        &self,
        current: Zeroizing<String>,
        email: &str,
    ) -> Result<String, AccountProblem> {
        let username = self
            .lock()
            .username()
            .ok_or(AccountProblem::new("signed_out"))?;
        self.client(username, current)
            .map_err(|detail| AccountProblem::with_detail("failed", detail))?
            .set_email(email)
            .map_err(AccountProblem::from)
    }

    fn recovery(&self) -> Result<Recovery, AccountProblem> {
        Recovery::new(&self.endpoint)
            .map_err(|error| AccountProblem::with_detail("failed", error.to_string()))
    }

    /// Asks for a reset code by email, signed out. Blocks on the network.
    pub fn send_reset_code(&self, username: &str) -> Result<(), AccountProblem> {
        self.recovery()?
            .send_code(username.trim())
            .map_err(AccountProblem::from)
    }

    /// Sets a new password with the emailed code. Blocks on the network.
    pub fn reset_password(
        &self,
        username: &str,
        code: &str,
        new_password: Zeroizing<String>,
    ) -> Result<(), AccountProblem> {
        self.recovery()?
            .reset(username.trim(), code.trim(), &new_password)
            .map_err(AccountProblem::from)
    }
}

fn server_notice(error: RemoteError) -> Notice {
    match error.kind {
        RemoteErrorKind::Status(status) => Notice {
            code: NoticeCode::Server,
            detail: Some(error.message),
            status: Some(status),
        },
        RemoteErrorKind::Connection => Notice::with_detail(NoticeCode::Unreachable, error.message),
        RemoteErrorKind::Certificate => Notice::with_detail(NoticeCode::Certificate, error.message),
        RemoteErrorKind::Incompatible => {
            Notice::with_detail(NoticeCode::Incompatible, error.message)
        }
        RemoteErrorKind::InvalidResponse | RemoteErrorKind::Paused => {
            Notice::new(NoticeCode::InvalidResponse)
        }
    }
}
