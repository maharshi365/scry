# Resolve Cargo's existing location, including config and CARGO_TARGET_DIR overrides.
$ScryRepository = Split-Path $PSScriptRoot -Parent

function Get-ScryBuildDirectory {
    Push-Location $ScryRepository
    try {
        $json = & cargo metadata --no-deps --format-version 1 --locked
        if ($LASTEXITCODE -ne 0) { throw 'Cannot locate Cargo build output.' }
        return ($json | ConvertFrom-Json).target_directory
    } finally { Pop-Location }
}

function Get-ScryExecutable([switch]$Release) {
    $profile = if ($Release) { 'release' } else { 'debug' }
    return Join-Path (Get-ScryBuildDirectory) "$profile/scry.exe"
}
