# Building DLA Downloader

Build on the operating system you intend to package: the Windows installer on
Windows, the Linux AppImage on Linux. The GitHub workflow in
`.github/workflows/desktop-build-release.yml` does exactly the steps below.

## Pinned tools

| Tool | Version |
| --- | --- |
| Rust | 1.97.1 (from `rust-toolchain.toml`) |
| Node.js | 26.7.0 |
| pnpm | 11.21.0 (from `tauri2/ui/package.json`) |
| Tauri CLI | 2.11.4 |

```bash
corepack enable
cd tauri2/ui && pnpm install --frozen-lockfile && cd ../..
cargo install tauri-cli --version 2.11.4 --locked
```

## The bundled cloudflared

The app carries a pinned `cloudflared` for its private route to DLA. Fetch it
once before building; each download is checked against the SHA-256 pinned in
the script:

```bash
sh tauri2/scripts/fetch-cloudflared.sh
```

It places the Linux and Windows binaries in `tauri2/src-tauri/binaries/`, which
stays out of Git. A Windows build machine takes a copy of that folder;
`tauri2/scripts/build-windows.ps1` checks the checksum again before building.

## The access token

Reaching DLA needs the route's Cloudflare Access service token, which the
project issues. Put it in `state/tunnel/service-token.json` (ignored by Git) or
point `DLA_ACCESS_TOKEN_FILE` at it before building:

```json
{"hostname": "…", "client_id": "….access", "client_secret": "…"}
```

Without it the build succeeds and the app reports that it cannot reach DLA.
The token only keeps unsolicited traffic off the route; access to the archive
is decided by the subscriber's own account.

## Linux (Debian or Ubuntu)

```bash
sudo apt update
sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev \
  libssl-dev libayatana-appindicator3-dev librsvg2-dev pkg-config libfuse2 xdg-utils
cd tauri2
cargo tauri build --bundles appimage
```

The AppImage is the Linux package because it can replace itself when the app
updates.

## Windows

With Visual Studio Build Tools 2022 (C++ workload), Rust and Node at the
versions above, from a copy of the repository that includes the fetched
`cloudflared`:

```powershell
powershell -ExecutionPolicy Bypass -File tauri2\scripts\build-windows.ps1
```

## Checks

```bash
cd tauri2/ui && pnpm typecheck && pnpm test && cd ..
python3 tools/i18n.py check
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

A few integration tests run against DLA's server components (the download
service and a mock of its file server), which are not part of this repository.
Without them those tests skip; point `DLA_GO_SELFTEST_BIN` at the binaries to
run them.

## Updates

The app installs an update only when its signature matches
`tauri2/certs/update-key.pub`, which is built into every copy. Without that
file a build simply has updates turned off.
