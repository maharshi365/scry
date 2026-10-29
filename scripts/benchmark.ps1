param(
    [string]$Baseline = 'reference',
    [switch]$SaveBaseline,
    [switch]$App,
    [string]$Report = ('target/perf-reports/benchmark-' + (Get-Date -Format 'yyyyMMdd-HHmmss-fff'))
)

$ErrorActionPreference = 'Stop'
if ($env:RUSTFLAGS -or $env:CARGO_ENCODED_RUSTFLAGS) {
    throw 'Benchmarks must run without RUSTFLAGS or CARGO_ENCODED_RUSTFLAGS.'
}
$mutex = [Threading.Mutex]::new($false, 'Local\ScryBenchmarks')
$held = $false
Push-Location (Split-Path $PSScriptRoot -Parent)
try {
    $held = $mutex.WaitOne(0)
    if (-not $held) { throw 'Another benchmark runner is active.' }
    $mode = if ($SaveBaseline) { '--save-baseline' } else { '--baseline' }
    & cargo bench --locked -p scry-library --bench library -- $mode $Baseline
    if ($LASTEXITCODE -ne 0) { throw 'Library benchmarks failed.' }
    & cargo bench --locked -p scry-addons --bench metadata -- $mode $Baseline
    if ($LASTEXITCODE -ne 0) { throw 'Metadata benchmarks failed.' }
    & cargo run --locked --release -p scry-library --example allocations
    if ($LASTEXITCODE -ne 0) { throw 'Library allocation measurement failed.' }
    & cargo run --locked --release -p scry-addons --example metadata_allocations
    if ($LASTEXITCODE -ne 0) { throw 'Metadata allocation measurement failed.' }
    if ($App) {
        & python scripts/profile-app.py --build --ui-only --report $Report
        if ($LASTEXITCODE -ne 0) { throw 'Full-app benchmark failed.' }
    }
} finally {
    if ($held) { $mutex.ReleaseMutex() }
    $mutex.Dispose()
    Pop-Location
}
