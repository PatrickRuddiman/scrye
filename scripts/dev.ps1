# Linux dev shell for scryd, runs in a Debian-based Rust container.
# Repo PWD mounts at /workspace. target/ and cargo registry use named
# Docker volumes so Linux build artifacts don't clobber any Windows
# build state and so dependencies cache across runs.
#
# Usage:
#   .\scripts\dev.ps1                      # interactive bash shell
#   .\scripts\dev.ps1 cargo check --workspace
#   .\scripts\dev.ps1 cargo test -p scryd-config

[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$RemainingArgs
)

$ErrorActionPreference = 'Stop'

$image       = 'rust:1-bookworm'
$workspace   = (Get-Location).Path
$targetVol   = 'mail-clawd-target'
$cargoVol    = 'mail-clawd-cargo'

docker volume create $targetVol | Out-Null
docker volume create $cargoVol  | Out-Null

$baseArgs = @(
    'run', '--rm',
    '-v', "${workspace}:/workspace",
    '-v', "${targetVol}:/workspace/target",
    '-v', "${cargoVol}:/usr/local/cargo/registry",
    '-w', '/workspace',
    '-e', 'CARGO_HOME=/usr/local/cargo',
    '-e', 'CARGO_TARGET_DIR=/workspace/target'
)

if (-not $RemainingArgs -or $RemainingArgs.Count -eq 0) {
    docker @baseArgs '-it' $image 'bash'
} else {
    docker @baseArgs $image @RemainingArgs
}

exit $LASTEXITCODE
