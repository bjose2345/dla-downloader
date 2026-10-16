# The DLA Downloader app

The Tauri 2 project: React renders the window and Rust does everything else.

| Path | What it is |
| --- | --- |
| `src-tauri/` | The app shell: sign-in session, connection, Library, app updates, window state |
| `crates/dla-connector` | Runs the bundled `cloudflared` forward and ties it to the app's lifetime |
| `crates/dla-remote` | The HTTPS client for DLA's download service, pinned to `certs/upload-ca.crt` |
| `crates/dla-library` | The catalog from the archive's manifests, search, verified resumable downloads |
| `crates/dla-app-update` | Finds, downloads and checks signed updates |
| `ui/` | The interface (React, TypeScript, Vite) and its eight translation catalogs |
| `certs/` | DLA's certificate authority and the public update key |
| `scripts/` | Fetching the pinned `cloudflared`, building on Windows |
| `tools/i18n.py` | Keeps the translation catalogs in step |

Building and checks: [docs/BUILDING.md](../docs/BUILDING.md).
