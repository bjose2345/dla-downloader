//! The HTTPS client for DLA's read-only download service, reached through the
//! private tunnel: sign-in, archive listings, ranged downloads and the
//! account's password and email. It trusts only the DLA upload CA and never
//! follows proxies or redirects.

mod archive;
mod error;
mod transport;

pub use archive::{
    AccountRefusal, ArchiveClient, ArchiveInfo, Fetched, RangeChunk, Recovery, RemoteEntry,
};
pub use error::{PauseSignal, RemoteError, RemoteErrorKind, is_digest};
pub use transport::{Credentials, Dial, Endpoint, EndpointError, PRIVATE_ARCHIVE};
