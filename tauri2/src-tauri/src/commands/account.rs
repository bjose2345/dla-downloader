//! The account: signing in and out, and its password and email.

use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;
use zeroize::Zeroizing;

use super::blocking;
use crate::{
    app_update::AppUpdater,
    connection::{Connection, ConnectionView},
    library::{DownloadState, Library},
    mode::PASSWORD_PAGE,
    session::{AccountProblem, AppInfo, Session, SessionView},
};

type Shared<'a> = State<'a, Arc<Session>>;
type SharedLibrary<'a> = State<'a, Arc<Library>>;
type SharedUpdater<'a> = State<'a, Arc<AppUpdater>>;

#[tauri::command]
pub fn app_info(session: Shared<'_>) -> AppInfo {
    session.info()
}

#[tauri::command]
pub fn connection_status(connection: State<'_, Arc<Connection>>) -> ConnectionView {
    connection.view()
}

#[tauri::command]
pub fn session(session: Shared<'_>) -> SessionView {
    session.view()
}

/// The download service checks the password; the Library and app updates
/// then use the same account.
#[tauri::command]
pub async fn sign_in(
    session: Shared<'_>,
    library: SharedLibrary<'_>,
    updater: SharedUpdater<'_>,
    username: String,
    password: String,
) -> Result<SessionView, String> {
    let password = Zeroizing::new(password);
    let library = Arc::clone(&library);
    let updater = Arc::clone(&updater);
    blocking(&session, move |session| {
        let view = session.sign_in(username.clone(), password.clone());
        if view.account.as_deref() == Some(username.as_str()) {
            updater.sign_in(username.clone(), password.clone());
            library.sign_in(username, password);
        }
        view
    })
    .await
}

/// A running download pauses and resumes at the next sign-in.
#[tauri::command]
pub fn sign_out(
    session: Shared<'_>,
    library: SharedLibrary<'_>,
    updater: SharedUpdater<'_>,
) -> SessionView {
    library.sign_out();
    updater.sign_out();
    session.sign_out()
}

#[tauri::command]
pub fn set_remember_account(session: Shared<'_>, remember: bool) -> AppInfo {
    session.set_remember_account(remember)
}

/// The result of an account change: done, or why not; with the email
/// address where one is involved.
#[derive(Debug, Default, serde::Serialize)]
pub struct AccountOutcome {
    changed: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    email: Option<String>,
    problem: Option<AccountProblem>,
}

impl AccountOutcome {
    fn from_result(result: Result<Option<String>, AccountProblem>) -> Self {
        match result {
            Ok(email) => Self {
                changed: true,
                email,
                problem: None,
            },
            Err(problem) => Self {
                problem: Some(problem),
                ..Self::default()
            },
        }
    }
}

/// The signed-in account's email address, for Settings.
#[tauri::command]
pub async fn account_email(session: Shared<'_>) -> Result<AccountOutcome, String> {
    blocking(&session, |session| {
        AccountOutcome::from_result(session.account_email().map(Some))
    })
    .await
}

/// Sets the email address reset codes go to, with the current password.
#[tauri::command]
pub async fn set_account_email(
    session: Shared<'_>,
    current: String,
    email: String,
) -> Result<AccountOutcome, String> {
    let current = Zeroizing::new(current);
    blocking(&session, move |session| {
        AccountOutcome::from_result(session.set_email(current, &email).map(Some))
    })
    .await
}

/// Emails a reset code for a forgotten password, signed out.
#[tauri::command]
pub async fn send_reset_code(
    session: Shared<'_>,
    username: String,
) -> Result<AccountOutcome, String> {
    blocking(&session, move |session| {
        AccountOutcome::from_result(session.send_reset_code(&username).map(|()| None))
    })
    .await
}

/// Sets a new password with the emailed code, signed out.
#[tauri::command]
pub async fn reset_password(
    session: Shared<'_>,
    username: String,
    code: String,
    new_password: String,
) -> Result<AccountOutcome, String> {
    let new_password = Zeroizing::new(new_password);
    blocking(&session, move |session| {
        AccountOutcome::from_result(
            session
                .reset_password(&username, &code, new_password)
                .map(|()| None),
        )
    })
    .await
}

/// Changes the signed-in account's password. Downloads and app updates carry
/// on with the new one, so none may be running.
#[tauri::command]
pub async fn change_password(
    session: Shared<'_>,
    library: SharedLibrary<'_>,
    updater: SharedUpdater<'_>,
    current: String,
    new_password: String,
) -> Result<AccountOutcome, String> {
    let current = Zeroizing::new(current);
    let new_password = Zeroizing::new(new_password);
    let library = Arc::clone(&library);
    let updater = Arc::clone(&updater);
    blocking(&session, move |session| {
        let downloads = library.view();
        let transferring = downloads.refreshing.is_some()
            || downloads
                .downloads
                .iter()
                .any(|download| download.state == DownloadState::Running);
        let result = if transferring {
            Err(AccountProblem::new("busy"))
        } else {
            session.change_password(current, new_password.clone())
        };
        AccountOutcome::from_result(result.map(|username| {
            updater.sign_in(username.clone(), new_password.clone());
            library.sign_in(username, new_password);
            None
        }))
    })
    .await
}

/// Opens the web page where the account's password can also be changed.
#[tauri::command]
pub fn open_password_page(app: AppHandle) -> Result<(), String> {
    app.opener()
        .open_url(PASSWORD_PAGE, None::<&str>)
        .map_err(|error| error.to_string())
}
