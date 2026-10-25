param([switch]$AllFeatures, [switch]$Dependencies)

$ErrorActionPreference = 'Stop'
Push-Location (Split-Path $PSScriptRoot -Parent)
$previousWarnings = $env:CARGO_BUILD_WARNINGS
try {
    # Cargo 1.97 enforces warnings without changing compiler flags/build caches.
    $env:CARGO_BUILD_WARNINGS = 'deny'

    $metadataJson = & cargo metadata --no-deps --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read workspace metadata.' }
    $metadata = $metadataJson | ConvertFrom-Json
    foreach ($package in $metadata.packages) {
        if ($package.id -notin $metadata.workspace_members) { continue }
        $manifest = Get-Content -LiteralPath $package.manifest_path -Raw
        # Require the same explicit inheritance form used by existing members.
        $lints = [regex]::Match($manifest, '(?ms)^\[lints\]\s*\r?\n(?<body>.*?)(?=^\[|\z)')
        if (-not $lints.Success -or $lints.Groups['body'].Value -notmatch '(?m)^workspace\s*=\s*true\s*(?:#.*)?$') {
            throw "$($package.manifest_path) must contain [lints] with workspace = true."
        }
    }
    & cargo fmt --all -- --check
    if ($LASTEXITCODE -ne 0) { throw 'Rust formatting check failed.' }

    $cargoArgs = @('--workspace', '--all-targets', '--locked')
    if ($AllFeatures) { $cargoArgs += '--all-features' }

    & cargo clippy @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw 'Clippy check failed.' }

    & cargo test @cargoArgs
    if ($LASTEXITCODE -ne 0) { throw 'Rust tests failed.' }

    if ($Dependencies) {
        & cargo machete
        if ($LASTEXITCODE -ne 0) { throw 'Dependency hygiene check failed.' }
        & cargo audit --file Cargo.lock
        if ($LASTEXITCODE -ne 0) { throw 'Dependency security audit failed.' }
    }
} finally {
    $env:CARGO_BUILD_WARNINGS = $previousWarnings
    Pop-Location
}
