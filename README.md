# DLA Downloader

DLA Downloader is an open-source desktop app for Linux and Windows that lets DLA
Elite subscribers browse, search and download the DLA archive with their own
account, instead of through the web client. It is built with Tauri 2, Rust,
React and TypeScript.

This repository holds the complete source of the app as it is released, so
anyone can read what it does and build it themselves.

> DLA Downloader is pre-release software.

## What it does

- Signs in with the subscriber's own DLA account. The password stays in memory
  and is never written to disk.
- Lists the archive by month and searches the whole catalog by work code,
  title or circle, from a local copy of the archive's published manifests.
- Downloads one file at a time in resumable ranges, then checks every file
  against the archive's SHA-256 before giving it its final name.
- Skips files already in the chosen folder when their size and SHA-256 match,
  so downloading a whole month again fetches only what is new.
- Changes the account's password and email address, and recovers a forgotten
  password by email.
- Updates itself: a new version is downloaded in the background, checked
  against its SHA-256 and the release signature, and installed when the user
  restarts.

The app writes only to its own data folder and the download folders the user
chooses. It runs no local web server and keeps no credentials.

## How it reaches DLA

The app talks to one service: DLA's read-only download service. It connects
through a bundled [`cloudflared`](https://github.com/cloudflare/cloudflared)
over a Cloudflare Access route, and trusts only DLA's own certificate authority
([`tauri2/certs/upload-ca.crt`](tauri2/certs/upload-ca.crt)) for the
connection behind it. Every request carries the subscriber's own credentials;
the server decides what the account may read and how much of its allowance a
download uses.

The route's service token is embedded at build time and is not in this
repository. It only keeps unsolicited traffic off the route: access to the
archive is decided by the subscriber's account. A build without it works but
cannot reach DLA.

## Updates and their signature

Releases are built by this repository's GitHub workflow. Copies update from
DLA's releases folder, and an update is installed only when its signature
matches the public key in [`tauri2/certs/update-key.pub`](tauri2/certs/update-key.pub).
The private half of that key never leaves the maintainer's machine.

## Build from source

See [Building](docs/BUILDING.md).

## License

AGPL-3.0-only. See [LICENSE](LICENSE) and [NOTICE](NOTICE).
