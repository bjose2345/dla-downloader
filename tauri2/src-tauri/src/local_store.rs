use std::{fs, io, path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};

use crate::system::write_private;

/// Remembers the account name on this computer when the user asks for it.
/// Credentials are never stored.
pub struct LocalStore {
    path: Option<PathBuf>,
    lock: Mutex<()>,
}

#[derive(Default, Serialize, Deserialize)]
struct Document {
    #[serde(default)]
    last_account: Option<String>,
    /// Off unless the user turns it on in Settings: the Welcome fields start
    /// empty, and no username is written here.
    #[serde(default)]
    remember_account: bool,
}

impl LocalStore {
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            lock: Mutex::new(()),
        }
    }

    pub fn last_account(&self) -> Option<String> {
        let _guard = self.lock.lock().expect("local store lock");
        self.load().last_account
    }

    pub fn set_last_account(&self, account: &str) {
        self.update(|document| {
            if document.remember_account {
                document.last_account = Some(account.to_owned());
            }
        });
    }

    pub fn remembers_account(&self) -> bool {
        let _guard = self.lock.lock().expect("local store lock");
        self.load().remember_account
    }

    /// Turning it off also forgets the username already saved.
    pub fn set_remember_account(&self, remember: bool) {
        self.update(|document| {
            document.remember_account = remember;
            if !remember {
                document.last_account = None;
            }
        });
    }

    fn update(&self, change: impl FnOnce(&mut Document)) {
        let _guard = self.lock.lock().expect("local store lock");
        let mut document = self.load();
        change(&mut document);
        // Best effort: without this file the user types the name again.
        let _ = self.save(&document);
    }

    fn load(&self) -> Document {
        self.path
            .as_deref()
            .and_then(|path| fs::read(path).ok())
            .and_then(|data| serde_json::from_slice(&data).ok())
            .unwrap_or_default()
    }

    fn save(&self, document: &Document) -> io::Result<()> {
        match &self.path {
            Some(path) => write_private(path, &serde_json::to_vec_pretty(document)?),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn stores_state_privately() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("state/account.json");
        let store = LocalStore::new(Some(path.clone()));
        store.set_remember_account(true);
        store.set_last_account("demo-a");
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            LocalStore::new(Some(path)).last_account().as_deref(),
            Some("demo-a")
        );
    }

    #[test]
    fn a_username_is_not_kept_when_the_user_opts_out() {
        let directory = tempfile::tempdir().unwrap();
        let store = LocalStore::new(Some(directory.path().join("account.json")));
        // Nothing is remembered until the user asks for it.
        assert!(!store.remembers_account());
        store.set_last_account("demo-a");
        assert_eq!(store.last_account(), None);
        store.set_remember_account(true);
        store.set_last_account("demo-a");
        assert_eq!(store.last_account().as_deref(), Some("demo-a"));
        store.set_remember_account(false);
        assert_eq!(store.last_account(), None);
        store.set_last_account("demo-b");
        assert_eq!(store.last_account(), None);
        store.set_remember_account(true);
        store.set_last_account("demo-b");
        assert_eq!(store.last_account().as_deref(), Some("demo-b"));
    }

    #[test]
    fn a_corrupt_file_is_treated_as_empty() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("account.json");
        fs::write(&path, b"{not json").unwrap();
        let store = LocalStore::new(Some(path));
        assert_eq!(store.last_account(), None);
        // It can be written again once the user opts in.
        store.set_remember_account(true);
        store.set_last_account("demo-a");
        assert_eq!(store.last_account().as_deref(), Some("demo-a"));
    }
}
