# Shared by install.ps1 and dev.ps1. Dot-source; do not run directly.
$ScryRepository = Split-Path $PSScriptRoot -Parent

function Build-Scry([switch]$Release) {
    Push-Location $ScryRepository
    try {
        $arguments = @('build', '-p', 'scry-app', '--bin', 'scry', '--locked')
        if ($Release) { $arguments += '--release' }
        & cargo @arguments
        if ($LASTEXITCODE -ne 0) { throw 'Scry build failed; the installed app was not changed.' }
        $json = & cargo metadata --no-deps --format-version 1 --locked
        if ($LASTEXITCODE -ne 0) { throw 'Cannot locate Cargo build output.' }
        $profile = if ($Release) { 'release' } else { 'debug' }
        return Join-Path ($json | ConvertFrom-Json).target_directory "$profile/scry.exe"
    } finally { Pop-Location }
}

function Stop-ScryExecutable([string]$Executable, [switch]$Force) {
    $processes = @(Get-Process -Name scry -ErrorAction SilentlyContinue | Where-Object {
        $_.Path -and [string]::Equals($_.Path, $Executable, [StringComparison]::OrdinalIgnoreCase)
    })
    foreach ($process in $processes) {
        try {
            if ($process.HasExited) { continue }
            $null = $process.CloseMainWindow()
            if (-not $process.WaitForExit(30000)) {
                if (-not $Force) { throw 'Scry did not close in 30 seconds. Close it yourself or rerun with -Force.' }
                $process.Kill()
                if (-not $process.WaitForExit(10000)) { throw 'Scry did not exit after force-stop.' }
            }
        } finally { $process.Dispose() }
    }
}

function Copy-ScryRuntime([string]$Destination) {
    $runtime = Join-Path $ScryRepository 'runtime'
    $archive = Join-Path $runtime 'mpv-dev-x86_64-20261021.7z'
    if (-not (Test-Path -LiteralPath $archive)) {
        & (Join-Path $PSScriptRoot 'setup-runtime.ps1')
    }
    $expected = 'efc719eff3dc5954b4a5efc800632808bcec01cc9034940dd14e5eb7fe9ea633'
    if ((Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
        throw 'Runtime checksum mismatch; deployment stopped.'
    }
    # Extract the verified archive, rather than trusting a possibly changed DLL.
    & 7z x $archive "-o$Destination" 'libmpv-2.dll' -y | Out-Host
    if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath (Join-Path $Destination 'libmpv-2.dll'))) {
        throw 'Cannot extract the verified libmpv runtime.'
    }
}

function Publish-Scry([string]$Executable, [string]$Directory, [switch]$Force) {
    $null = New-Item -ItemType Directory -Path $Directory -Force
    $staging = Join-Path $Directory ('.staging-' + [guid]::NewGuid().ToString('N'))
    $null = New-Item -ItemType Directory -Path $staging
    try {
        Copy-Item -LiteralPath $Executable -Destination (Join-Path $staging 'scry.exe')
        Copy-ScryRuntime $staging
        Stop-ScryExecutable (Join-Path $Directory 'scry.exe') -Force:$Force
        # Keep staging inside the destination so preparation never touches live files.
        Copy-Item -LiteralPath (Join-Path $staging 'libmpv-2.dll') -Destination $Directory -Force
        Copy-Item -LiteralPath (Join-Path $staging 'scry.exe') -Destination $Directory -Force
    } finally {
        Remove-Item -LiteralPath $staging -Recurse -Force
    }
}
