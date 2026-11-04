[CmdletBinding()]
param(
    [ValidateRange(1, 20)][int]$Runs = 5,
    [ValidateSet(65536, 262144, 1048576, 4194304, 8388608, 16777216)][int]$PieceBytes = 1048576,
    [ValidateRange(16384, 104857600)][int]$UploadBytesPerSecond = 1048576,
    [string]$StremioServer
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/build-paths.ps1"
if (-not (Get-Command ffmpeg -ErrorAction SilentlyContinue)) { throw 'Install ffmpeg to generate synthetic fixtures.' }

$root = Join-Path $ScryRepository "target/startup-benchmark/$([guid]::NewGuid().ToString('N'))"
New-Item -ItemType Directory -Path $root | Out-Null
$fast = Join-Path $root 'faststart.mp4'
$tail = Join-Path $root 'tail-index.mp4'
$mkv = Join-Path $root 'video.mkv'
& ffmpeg -nostdin -hide_banner -loglevel error -f lavfi -i 'testsrc2=size=640x360:rate=24' -f lavfi -i 'sine=frequency=440:sample_rate=48000' -t 30 -c:v mpeg4 -q:v 2 -c:a aac -movflags +faststart $fast
if ($LASTEXITCODE -ne 0) { throw 'Cannot create the synthetic MP4 fixture.' }
& ffmpeg -nostdin -hide_banner -loglevel error -i $fast -map 0 -c copy $tail
if ($LASTEXITCODE -ne 0) { throw 'Cannot create the tail-index fixture.' }
& ffmpeg -nostdin -hide_banner -loglevel error -i $fast -map 0 -c copy $mkv
if ($LASTEXITCODE -ne 0) { throw 'Cannot create the Matroska fixture.' }

$previousWarnings = $env:CARGO_BUILD_WARNINGS
$previousLibrary = $env:Scry_MPV_LIBRARY
Push-Location $ScryRepository
try {
    $env:CARGO_BUILD_WARNINGS = 'deny'
    & cargo build -p scry-torrents --example startup --features profiling --release --locked
    if ($LASTEXITCODE -ne 0) { throw 'Cannot build the release benchmark.' }
    $benchmark = Join-Path (Get-ScryBuildDirectory) 'release/examples/startup.exe'
    if (-not $env:Scry_MPV_LIBRARY) {
        $env:Scry_MPV_LIBRARY = Join-Path $ScryRepository 'runtime/libmpv-2.dll'
    }
    @{
        benchmark_sha256 = (Get-FileHash -LiteralPath $benchmark -Algorithm SHA256).Hash
        mpv_sha256 = (Get-FileHash -LiteralPath $env:Scry_MPV_LIBRARY -Algorithm SHA256).Hash
        ffmpeg_version = @(& ffmpeg -version)[0]
    } | ConvertTo-Json | Set-Content -LiteralPath (Join-Path $root 'build.json')
    foreach ($fixture in @($fast, $tail, $mkv)) {
        $report = Join-Path $root "$([IO.Path]::GetFileNameWithoutExtension($fixture)).jsonl"
        $arguments = @('--media', $fixture, '--report', $report, '--runs', $Runs,
            '--piece-bytes', $PieceBytes, '--upload-bps', $UploadBytesPerSecond)
        if ($StremioServer) { $arguments += @('--stremio', $StremioServer) }
        & $benchmark @arguments
        if ($LASTEXITCODE -ne 0) { throw "Startup benchmark failed. Partial results: $report" }
    }
} finally {
    Pop-Location
    $env:CARGO_BUILD_WARNINGS = $previousWarnings
    $env:Scry_MPV_LIBRARY = $previousLibrary
}
$summary = foreach ($report in Get-ChildItem -LiteralPath $root -Filter '*.jsonl') {
    $trials = Get-Content -LiteralPath $report.FullName | ConvertFrom-Json | Where-Object event -eq 'trial'
    foreach ($group in ($trials | Group-Object engine, strategy, cache)) {
        $times = @($group.Group.playback.ready_s | Where-Object { $null -ne $_ } | Sort-Object)
        $trial = $group.Group[0]
        [pscustomobject]@{
            Fixture = $report.BaseName
            Engine = $trial.engine
            Strategy = $trial.strategy
            Cache = $trial.cache
            Runs = $group.Count
            ReadyP50ms = if ($times.Count) { [Math]::Round(1000 * $times[[Math]::Ceiling($times.Count * 0.5) - 1], 1) } else { $null }
            ReadyP95ms = if ($times.Count) { [Math]::Round(1000 * $times[[Math]::Ceiling($times.Count * 0.95) - 1], 1) } else { $null }
        }
    }
}
$summary | Format-Table -AutoSize
Write-Host "Results: $root"
Write-Host 'Readiness percentiles use nearest rank. A small run count is not a reliable tail-latency estimate.'
Write-Host 'Decode readiness only, not visible presentation. External Stremio cache is uncontrolled; use a dedicated test server.'
