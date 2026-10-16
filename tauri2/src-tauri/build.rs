use std::{
    collections::hash_map::RandomState,
    env, fs,
    hash::{BuildHasher, Hasher},
    path::PathBuf,
};

fn main() {
    embed_access_token();
    embed_update_key();
    tauri_build::build()
}

/// Embeds the public half of the DLA update key, which checks the signature
/// of every downloaded update. It is public, so it lives in the repository;
/// without it the build still works, with updates turned off.
fn embed_update_key() {
    println!("cargo:rerun-if-env-changed=DLA_UPDATE_KEY_FILE");
    let path = env::var_os("DLA_UPDATE_KEY_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"))
                .join("../certs/update-key.pub")
        });
    println!("cargo:rerun-if-changed={}", path.display());
    let Ok(key) = fs::read_to_string(&path) else {
        println!(
            "cargo:warning=No update key at {}; app updates are off in this build.",
            path.display()
        );
        return;
    };
    let key = key.trim();
    assert!(
        !key.is_empty()
            && key
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"+/=".contains(&byte)),
        "{} is not a `cargo tauri signer` public key",
        path.display()
    );
    println!("cargo:rustc-env=DLA_UPDATE_KEY={key}");
}

/// Embeds the shared Cloudflare Access service token from a private file that
/// never enters source control. The value is masked only so it does not appear
/// as plain text in the binary; anyone holding the app can still recover it,
/// which is why SFTPGo credentials remain the real access control.
fn embed_access_token() {
    println!("cargo:rerun-if-env-changed=DLA_ACCESS_TOKEN_FILE");
    let path = env::var_os("DLA_ACCESS_TOKEN_FILE")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").expect("manifest directory"))
                .join("../../state/tunnel/service-token.json")
        });
    println!("cargo:rerun-if-changed={}", path.display());
    let Ok(data) = fs::read(&path) else {
        println!(
            "cargo:warning=No DLA access token at {}; the private service is unreachable in this build.",
            path.display()
        );
        return;
    };
    let value: serde_json::Value =
        serde_json::from_slice(&data).expect("the access token file is JSON");
    let field = |name: &str| {
        value[name]
            .as_str()
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| panic!("the access token file has no {name}"))
            .to_owned()
    };
    let plain = serde_json::to_vec(&serde_json::json!({
        "hostname": field("hostname"),
        "client_id": field("client_id"),
        "client_secret": field("client_secret"),
    }))
    .expect("serializable token");
    let key = random_bytes(plain.len());
    let masked: Vec<u8> = plain
        .iter()
        .zip(&key)
        .map(|(byte, mask)| byte ^ mask)
        .collect();
    println!(
        "cargo:rustc-env=DLA_ACCESS_BLOB={}{}",
        hex(&key),
        hex(&masked)
    );
}

fn random_bytes(length: usize) -> Vec<u8> {
    let state = RandomState::new();
    let mut bytes = Vec::with_capacity(length + 8);
    let mut counter = 0u64;
    while bytes.len() < length {
        let mut hasher = state.build_hasher();
        hasher.write_u64(counter);
        bytes.extend_from_slice(&hasher.finish().to_le_bytes());
        counter += 1;
    }
    bytes.truncate(length);
    bytes
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
