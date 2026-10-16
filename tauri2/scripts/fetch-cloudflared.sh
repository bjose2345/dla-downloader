#!/bin/sh
# Downloads the pinned cloudflared release that the app bundles as a sidecar
# and verifies each file against the SHA-256 published in the release notes.
# A new version is adopted only after it passes the service-token probe in
# deploy/UPLOAD_TUNNEL.md.
set -eu
version=2026.9.3
linux_sha256=77e26d8d900e0b8469f416239d14b5f296525fdf79fee6f511ef55609e3fbac2
windows_sha256=f096265ec2fcbe9bb6e2d64268db167ced3fcbb83d894bdb9e2fcdb26f2ea7e2
base="https://github.com/cloudflare/cloudflared/releases/download/$version"
dir="$(cd "$(dirname "$0")/.." && pwd)/src-tauri/binaries"
mkdir -p "$dir"

fetch() {
  asset=$1 target=$2 expected=$3
  if [ -f "$dir/$target" ] && echo "$expected  $dir/$target" | sha256sum -c --status; then
    echo "ok       $target"
    return
  fi
  tmp="$dir/.$target.download"
  curl -fsSL --retry 3 -o "$tmp" "$base/$asset"
  if ! echo "$expected  $tmp" | sha256sum -c --status; then
    rm -f "$tmp"
    echo "checksum mismatch for $asset $version; nothing was installed" >&2
    exit 1
  fi
  chmod 0755 "$tmp"
  mv "$tmp" "$dir/$target"
  echo "fetched  $target"
}

fetch cloudflared-linux-amd64 cloudflared-x86_64-unknown-linux-gnu "$linux_sha256"
fetch cloudflared-windows-amd64.exe cloudflared-x86_64-pc-windows-msvc.exe "$windows_sha256"
