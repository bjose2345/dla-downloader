//! The test key, a payload it signed, and a `latest.json` for it. The files
//! in `fixtures/` were made once with `cargo tauri signer` for tests only
//! (the app's own tests read them too); they sign nothing else.
#![allow(dead_code)]

use sha2::{Digest, Sha256};

/// `cargo tauri signer generate` output (`test.key.pub`).
pub const TEST_KEY: &str = include_str!("../fixtures/test.key.pub");
/// What the key signed, and its `.sig`.
pub const PAYLOAD: &[u8] = include_bytes!("../fixtures/payload.bin");
pub const SIGNATURE: &str = include_str!("../fixtures/payload.bin.sig");
/// The same payload signed by a different key.
pub const OTHER_SIGNATURE: &str = include_str!("../fixtures/other.bin.sig");

pub const TARGET: &str = "windows-x86_64";
pub const INSTALLER: &str = "0.4.0/DLA Downloader_0.4.0_x64-setup.exe";

pub fn sha256(data: &[u8]) -> String {
    hex_string(&Sha256::digest(data))
}

pub fn hex_string(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn manifest(
    version: &str,
    minimum: Option<&str>,
    file: &str,
    data: &[u8],
    signature: &str,
) -> Vec<u8> {
    let mut document = serde_json::json!({
        "version": version,
        "notes": "  Faster downloads.  ",
        "pub_date": "2026-10-01T10:00:00Z",
        "platforms": {
            TARGET: {
                "file": file,
                "size": data.len(),
                "sha256": sha256(data).to_uppercase(),
                "signature": signature,
            }
        }
    });
    if let Some(minimum) = minimum {
        document["minimum_version"] = minimum.into();
    }
    serde_json::to_vec(&document).unwrap()
}
