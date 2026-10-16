use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

use thiserror::Error;

/// Asks a running transfer to stop at its next chunk.
#[derive(Debug, Clone, Default)]
pub struct PauseSignal(Arc<AtomicBool>);

impl PauseSignal {
    pub fn request(&self) {
        self.0.store(true, Ordering::SeqCst);
    }

    pub fn is_requested(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteErrorKind {
    Status(u16),
    Connection,
    Certificate,
    InvalidResponse,
    Incompatible,
    Paused,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{message}")]
pub struct RemoteError {
    pub kind: RemoteErrorKind,
    pub message: String,
}

impl RemoteError {
    pub fn new(kind: RemoteErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    /// 429 is also the download service's "this account is already
    /// downloading", which clears when that download ends.
    pub fn is_transient(&self) -> bool {
        match self.kind {
            RemoteErrorKind::Connection | RemoteErrorKind::InvalidResponse => true,
            RemoteErrorKind::Status(code) => {
                matches!(code, 408 | 423 | 429 | 500 | 502 | 503 | 504)
            }
            RemoteErrorKind::Certificate
            | RemoteErrorKind::Incompatible
            | RemoteErrorKind::Paused => false,
        }
    }
}

/// A lower-case SHA-256 in hex, as the archive manifests carry them.
pub fn is_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
