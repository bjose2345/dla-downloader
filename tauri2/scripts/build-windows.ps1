# Builds the DLA Downloader Windows installer (NSIS) on a Windows 10/11 PC.
#
# Needs, already installed: Visual Studio Build Tools 2022 with the C++
# workload, rustup with Rust 1.97.1 (MSVC), and Node 26.7.0. pnpm and the
# Tauri CLI are installed at pinned versions into tauri2\.windows-tools, and
# Tauri downloads NSIS itself on the first build.
#
# Run from a copy of the repository:
#   powershell -ExecutionPolicy Bypass -File tauri2\scripts\build-windows.ps1
# Without state\tunnel\service-token.json the app builds but cannot reach DLA.

param([switch]$SkipTests)

$ErrorActionPreference = 'Stop'
$ProgressPreference = 'SilentlyContinue'

$RustVersion = '1.97.1'
$NodeVersion = 'v26.7.0'
$PnpmVersion = '11.21.0'
$TauriCliVersion = '2.11.4'
$CloudflaredSha256 = 'f096265ec2fcbe9bb6e2d64268db167ced3fcbb83d894bdb9e2fcdb26f2ea7e2'

$root = (Resolve-Path (Join-Path $PSScriptRoot '..\..')).Path
$tauri = Join-Path $root 'tauri2'

function Invoke-Checked([string]$what, [scriptblock]$command) {
    Write-Host "==> $what"
    & $command
    if ($LASTEXITCODE -ne 0) { throw "$what failed (exit code $LASTEXITCODE)" }
}

$rustc = ((rustc --version) -split ' ')[1]
if ($rustc -ne $RustVersion) { throw "Rust $RustVersion is required, found $rustc (rustup default $RustVersion)" }
$node = node --version
if ($node -ne $NodeVersion) { throw "Node $NodeVersion is required, found $node" }

$sidecar = Join-Path $tauri 'src-tauri\binaries\cloudflared-x86_64-pc-windows-msvc.exe'
if (-not (Test-Path $sidecar)) { throw "Missing $sidecar; run tauri2/scripts/fetch-cloudflared.sh on Linux and copy it." }
$hash = (Get-FileHash $sidecar -Algorithm SHA256).Hash.ToLowerInvariant()
if ($hash -ne $CloudflaredSha256) { throw "cloudflared checksum mismatch: $hash" }

$token = Join-Path $root 'state\tunnel\service-token.json'
if (Test-Path $token) {
    Write-Host '==> Access token found; this is a private build.'
} else {
    Write-Warning 'No state\tunnel\service-token.json: the installer will not reach DLA.'
}

# Pinned build tools, local to this checkout. The .cmd shims also work where
# PowerShell's script policy blocks npm.ps1.
$tools = Join-Path $tauri '.windows-tools'
$bin = Join-Path $tools 'node_modules\.bin'
if (-not (Test-Path (Join-Path $bin 'pnpm.cmd')) -or -not (Test-Path (Join-Path $bin 'tauri.cmd'))) {
    New-Item -ItemType Directory -Force $tools | Out-Null
    Invoke-Checked 'Installing pnpm and the Tauri CLI' {
        npm.cmd install --prefix $tools --no-audit --no-fund --save-exact "pnpm@$PnpmVersion" "@tauri-apps/cli@$TauriCliVersion"
    }
}
$env:PATH = "$bin;$env:PATH"

Push-Location $tauri
try {
    # No TTY here: pnpm must not stop to ask before replacing a node_modules
    # that was installed under another path.
    Invoke-Checked 'Installing UI dependencies' { pnpm.cmd --dir ui install --frozen-lockfile --config.confirmModulesPurge=false }
    Invoke-Checked 'Checking and testing the UI' { pnpm.cmd --dir ui typecheck; if ($LASTEXITCODE -eq 0) { pnpm.cmd --dir ui test } }
    if (-not $SkipTests) {
        Invoke-Checked 'Testing the Rust workspace' { cargo test --workspace }
    }
    $env:CI = 'true'
    # Only this build's installer may be left for the caller to pick up.
    Remove-Item (Join-Path $tauri 'target\release\bundle\nsis\*-setup.exe') -ErrorAction SilentlyContinue
    Invoke-Checked 'Building the installer' { tauri.cmd build --ci --bundles nsis }
} finally {
    Pop-Location
}

$installer = Get-ChildItem (Join-Path $tauri 'target\release\bundle\nsis\*-setup.exe') |
    Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $installer) { throw 'The build finished but no installer was found.' }
$sha = (Get-FileHash $installer.FullName -Algorithm SHA256).Hash.ToLowerInvariant()
Write-Host ''
Write-Host "Installer: $($installer.FullName)"
Write-Host ("Size:      {0:N1} MB" -f ($installer.Length / 1MB))
Write-Host "SHA-256:   $sha"
