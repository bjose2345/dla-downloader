use std::time::Duration;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use ureq::{Agent, Body, http::Response};
use zeroize::Zeroizing;

use crate::{
    error::{RemoteError, RemoteErrorKind},
    transport::{
        Credentials, Endpoint, EndpointError, JSON_TIMEOUT, MAX_RESPONSE_BYTES, PROTOCOL,
        incompatible, invalid_response, read_limited, status_error, transport_error,
    },
};

const LIST_TIMEOUT: Duration = Duration::from_secs(60);
const RANGE_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_LISTING_BYTES: u64 = 32 << 20;

/// One file or folder of an archive listing. `path` is absolute in the
/// archive, such as `/dlsite/2013-01/RJ108375__x.zip`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: Option<String>,
    pub etag: Option<String>,
}

pub enum Fetched {
    Content { data: Vec<u8>, etag: Option<String> },
    NotModified,
}

pub enum RangeChunk {
    Data {
        data: Vec<u8>,
        total: u64,
        etag: Option<String>,
    },
    /// The file changed since its validator was recorded; start again from zero.
    Changed,
}

/// What the service reports after a sign-in.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ArchiveInfo {
    pub protocol: u32,
    /// The only folder the service serves, such as `/dlsite`.
    pub root: String,
    /// How many files one account may download at the same time.
    pub max_downloads: u32,
}

/// Why an account change (password, email, recovery) did not happen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AccountRefusal {
    /// The current password is wrong.
    Current,
    /// Too many attempts for this account; it has to wait.
    TooMany,
    /// A rule was broken: for passwords `too_short`, `too_long`, `same`,
    /// `invalid`; `invalid_email`; `code` for a wrong or expired reset code;
    /// `rejected` by SFTPGo. The detail is the server's own words.
    Rule {
        rule: String,
        detail: String,
    },
    /// This account may not make this change.
    NotAllowed,
    /// The server cannot do this, or not right now.
    Unavailable,
    Failed(RemoteError),
}

#[derive(Deserialize)]
struct Listing {
    path: String,
    entries: Vec<ListedEntry>,
}

#[derive(Deserialize)]
struct ListedEntry {
    name: String,
    size: u64,
    directory: bool,
    modified: Option<String>,
}

#[derive(Deserialize)]
struct AccountDetails {
    #[serde(default)]
    email: String,
}

#[derive(Deserialize)]
struct Refusal {
    #[serde(default)]
    reason: String,
    #[serde(default)]
    error: String,
}

/// The signed-in client of the download service. Every request carries the
/// account's own credentials, and SFTPGo decides what it may read.
pub struct ArchiveClient {
    agent: Agent,
    base: String,
    username: String,
    authorization: Zeroizing<String>,
}

impl ArchiveClient {
    pub fn new(endpoint: &Endpoint, credentials: Credentials) -> Result<Self, EndpointError> {
        Ok(Self {
            agent: endpoint.agent()?,
            base: endpoint.base().trim_end_matches('/').to_owned(),
            authorization: credentials.authorization(),
            username: credentials.username,
        })
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    /// Signs in: SFTPGo checks the password, and the service confirms it
    /// speaks this app's protocol.
    pub fn verify(&self) -> Result<ArchiveInfo, RemoteError> {
        let info: ArchiveInfo = self.get_json(&self.base, JSON_TIMEOUT)?;
        if info.protocol != PROTOCOL {
            return Err(incompatible());
        }
        Ok(info)
    }

    pub fn list(&self, path: &str) -> Result<Vec<RemoteEntry>, RemoteError> {
        let wanted = normalize(path).ok_or_else(invalid_response)?;
        let listing: Listing = self.get_json(&self.url("list", &wanted), LIST_TIMEOUT)?;
        let folder = listing.path.trim_end_matches('/');
        Ok(listing
            .entries
            .into_iter()
            .filter(|entry| valid_name(&entry.name))
            .map(|entry| RemoteEntry {
                path: format!("{folder}/{}", entry.name),
                name: entry.name,
                is_dir: entry.directory,
                size: entry.size,
                modified: entry.modified,
                etag: None,
            })
            .collect())
    }

    /// Reads a whole small file. The service has no conditional requests, so
    /// every fetch returns the content; its Last-Modified is the validator.
    pub fn fetch(
        &self,
        path: &str,
        limit: u64,
        _if_none_match: Option<&str>,
    ) -> Result<Fetched, RemoteError> {
        let path = normalize(path).ok_or_else(invalid_response)?;
        let mut response = self
            .agent
            .get(self.url("file", &path))
            .config()
            .timeout_global(Some(LIST_TIMEOUT))
            .build()
            .header("Authorization", self.authorization.as_str())
            .call()
            .map_err(|error| transport_error(&error))?;
        let status = response.status().as_u16();
        let data = read_limited(&mut response, limit)?;
        if !(200..300).contains(&status) {
            return Err(status_error(status, &data));
        }
        Ok(Fetched::Content {
            etag: header(&response, "last-modified"),
            data,
        })
    }

    /// Reads `length` bytes from `offset`. With a validator (the file's
    /// Last-Modified), `If-Range` makes the server refuse to mix two
    /// versions of a file.
    pub fn read_range(
        &self,
        path: &str,
        offset: u64,
        length: u64,
        etag: Option<&str>,
    ) -> Result<RangeChunk, RemoteError> {
        if length == 0 {
            return Err(invalid_response());
        }
        let path = normalize(path).ok_or_else(invalid_response)?;
        let mut request = self
            .agent
            .get(self.url("file", &path))
            .config()
            .timeout_global(Some(RANGE_TIMEOUT))
            .build()
            .header("Authorization", self.authorization.as_str())
            .header("Range", format!("bytes={offset}-{}", offset + length - 1));
        if let Some(etag) = etag {
            request = request.header("If-Range", etag);
        }
        let mut response = request.call().map_err(|error| transport_error(&error))?;
        match response.status().as_u16() {
            206 => {
                let (start, total) = header(&response, "content-range")
                    .as_deref()
                    .and_then(content_range)
                    .ok_or_else(invalid_response)?;
                if start != offset {
                    return Err(invalid_response());
                }
                let modified = header(&response, "last-modified");
                if etag.is_some() && modified.as_deref() != etag {
                    return Ok(RangeChunk::Changed);
                }
                let data = read_limited(&mut response, length)?;
                let expected = length.min(total.saturating_sub(offset));
                if data.len() as u64 != expected {
                    return Err(RemoteError::new(
                        RemoteErrorKind::Connection,
                        "A download chunk ended early.",
                    ));
                }
                Ok(RangeChunk::Data {
                    etag: modified,
                    data,
                    total,
                })
            }
            200 | 416 => Ok(RangeChunk::Changed),
            status => {
                let body = read_limited(&mut response, 64 << 10).unwrap_or_default();
                Err(status_error(status, &body))
            }
        }
    }

    /// Changes this account's password. The client must carry the current
    /// password; SFTPGo accepts the new one at once.
    pub fn change_password(&self, new_password: &str) -> Result<(), AccountRefusal> {
        let body = Zeroizing::new(
            serde_json::to_vec(&serde_json::json!({ "new_password": new_password }))
                .map_err(|_| AccountRefusal::Failed(invalid_response()))?,
        );
        self.account(AccountMethod::Post, "/account/password", &body)
            .map(|_| ())
    }

    /// The account's email address, where reset codes go, or "" without one.
    pub fn email(&self) -> Result<String, AccountRefusal> {
        let data = self.account(AccountMethod::Get, "/account", &[])?;
        email_of(&data)
    }

    /// Sets the email address ("" removes it). The client must carry the
    /// current password. Returns the address as saved.
    pub fn set_email(&self, email: &str) -> Result<String, AccountRefusal> {
        let body = serde_json::to_vec(&serde_json::json!({ "email": email }))
            .map_err(|_| AccountRefusal::Failed(invalid_response()))?;
        let data = self.account(AccountMethod::Put, "/account/email", &body)?;
        email_of(&data)
    }

    fn account(
        &self,
        method: AccountMethod,
        path: &str,
        body: &[u8],
    ) -> Result<Vec<u8>, AccountRefusal> {
        account_request(
            &self.agent,
            method,
            &format!("{}{path}", self.base),
            Some(&self.authorization),
            body,
        )
    }

    fn url(&self, operation: &str, path: &str) -> String {
        let query: String = url::form_urlencoded::byte_serialize(path.as_bytes()).collect();
        format!("{}/{operation}?path={query}", self.base)
    }

    fn get_json<T: DeserializeOwned>(
        &self,
        url: &str,
        timeout: Duration,
    ) -> Result<T, RemoteError> {
        let response = self
            .agent
            .get(url)
            .config()
            .timeout_global(Some(timeout))
            .build()
            .header("Accept", "application/json")
            .header("Authorization", self.authorization.as_str())
            .call();
        let mut response = response.map_err(|error| transport_error(&error))?;
        let status = response.status().as_u16();
        let data = read_limited(&mut response, MAX_LISTING_BYTES)?;
        if !(200..300).contains(&status) {
            return Err(status_error(status, &data));
        }
        serde_json::from_slice(&data).map_err(|_| invalid_response())
    }
}

/// Password recovery, which works without signing in.
pub struct Recovery {
    agent: Agent,
    base: String,
}

impl Recovery {
    pub fn new(endpoint: &Endpoint) -> Result<Self, EndpointError> {
        Ok(Self {
            agent: endpoint.agent()?,
            base: endpoint.base().trim_end_matches('/').to_owned(),
        })
    }

    /// Asks for a code. The answer is the same whether or not the account
    /// exists or has an email address.
    pub fn send_code(&self, username: &str) -> Result<(), AccountRefusal> {
        let body = serde_json::to_vec(&serde_json::json!({ "username": username }))
            .map_err(|_| AccountRefusal::Failed(invalid_response()))?;
        account_request(
            &self.agent,
            AccountMethod::Post,
            &format!("{}/account/forgot", self.base),
            None,
            &body,
        )
        .map(|_| ())
    }

    /// Sets a new password with the emailed code.
    pub fn reset(
        &self,
        username: &str,
        code: &str,
        new_password: &str,
    ) -> Result<(), AccountRefusal> {
        let body = Zeroizing::new(
            serde_json::to_vec(
                &serde_json::json!({ "username": username, "code": code, "new_password": new_password }),
            )
            .map_err(|_| AccountRefusal::Failed(invalid_response()))?,
        );
        account_request(
            &self.agent,
            AccountMethod::Post,
            &format!("{}/account/reset", self.base),
            None,
            &body,
        )
        .map(|_| ())
    }
}

#[derive(Clone, Copy)]
enum AccountMethod {
    Get,
    Put,
    Post,
}

fn account_request(
    agent: &Agent,
    method: AccountMethod,
    url: &str,
    authorization: Option<&Zeroizing<String>>,
    body: &[u8],
) -> Result<Vec<u8>, AccountRefusal> {
    let response = match method {
        AccountMethod::Get => {
            let request = agent
                .get(url)
                .config()
                .timeout_global(Some(JSON_TIMEOUT))
                .build()
                .header("Accept", "application/json");
            match authorization {
                Some(authorization) => request.header("Authorization", authorization.as_str()),
                None => request,
            }
            .call()
        }
        AccountMethod::Put | AccountMethod::Post => {
            let request = if matches!(method, AccountMethod::Put) {
                agent.put(url)
            } else {
                agent.post(url)
            }
            .config()
            .timeout_global(Some(JSON_TIMEOUT))
            .build()
            .header("Accept", "application/json")
            .header("Content-Type", "application/json");
            match authorization {
                Some(authorization) => request.header("Authorization", authorization.as_str()),
                None => request,
            }
            .send(body)
        }
    };
    let mut response = response.map_err(|error| AccountRefusal::Failed(transport_error(&error)))?;
    let status = response.status().as_u16();
    let data = read_limited(&mut response, MAX_RESPONSE_BYTES).map_err(AccountRefusal::Failed)?;
    if (200..300).contains(&status) {
        return Ok(data);
    }
    let refusal = serde_json::from_slice::<Refusal>(&data).ok();
    let reason = refusal
        .as_ref()
        .map_or("", |refusal| refusal.reason.as_str());
    Err(match (status, reason) {
        (401, _) if authorization.is_none() => AccountRefusal::Unavailable,
        (401, _) => AccountRefusal::Current,
        (429, _) => AccountRefusal::TooMany,
        (403, "not_allowed") => AccountRefusal::NotAllowed,
        (422, rule) if !rule.is_empty() => AccountRefusal::Rule {
            rule: rule.to_owned(),
            detail: refusal.map(|refusal| refusal.error).unwrap_or_default(),
        },
        // A download service from before these requests answers 404 (or 405).
        (404 | 405, _) | (503, "unavailable") => AccountRefusal::Unavailable,
        _ => AccountRefusal::Failed(status_error(status, &data)),
    })
}

fn email_of(data: &[u8]) -> Result<String, AccountRefusal> {
    serde_json::from_slice::<AccountDetails>(data)
        .map(|details| details.email)
        .map_err(|_| AccountRefusal::Failed(invalid_response()))
}

fn header(response: &Response<Body>, name: &str) -> Option<String> {
    response
        .headers()
        .get(name)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned)
}

/// Parses `bytes start-end/total`.
fn content_range(value: &str) -> Option<(u64, u64)> {
    let (range, total) = value.strip_prefix("bytes ")?.split_once('/')?;
    let (start, end) = range.split_once('-')?;
    let (start, end, total) = (
        start.parse().ok()?,
        end.parse::<u64>().ok()?,
        total.parse().ok()?,
    );
    (start <= end && end < total).then_some((start, total))
}

/// An absolute archive path without `.` or `..` segments or empty names.
fn normalize(path: &str) -> Option<String> {
    let trimmed = path.trim_end_matches('/');
    if trimmed.is_empty() {
        return Some("/".to_owned());
    }
    if !trimmed.starts_with('/') {
        return None;
    }
    trimmed
        .split('/')
        .skip(1)
        .all(valid_name)
        .then(|| trimmed.to_owned())
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name != "." && name != ".." && !name.contains(['/', '\\', '\0'])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_are_absolute_without_dot_segments() {
        assert_eq!(
            normalize("/dlsite/2013-01/").as_deref(),
            Some("/dlsite/2013-01")
        );
        assert_eq!(normalize("/").as_deref(), Some("/"));
        for rejected in [
            "dlsite",
            "/dlsite/../etc",
            "/dlsite//x",
            "/dlsite/./x",
            "/a\\b",
        ] {
            assert_eq!(normalize(rejected), None, "{rejected}");
        }
    }

    #[test]
    fn content_ranges_are_checked() {
        assert_eq!(content_range("bytes 0-9/100"), Some((0, 100)));
        assert_eq!(content_range("bytes 90-99/100"), Some((90, 100)));
        assert_eq!(content_range("bytes 9-0/100"), None);
        assert_eq!(content_range("bytes 0-100/100"), None);
        assert_eq!(content_range("items 0-9/100"), None);
    }
}
