# Planning is read-only. Only cleanup.ps1 applies an explicitly requested plan.
function Get-ScryCleanupPlan {
    param(
        [string[]]$BuildRoots,
        [string]$ReportRoot,
        [string]$FixtureRoot,
        [switch]$Builds,
        [switch]$Reports,
        [switch]$Fixtures,
        [int]$KeepReports = 5,
        [int]$ReportDays = 30
    )
    if ($Builds) {
        foreach ($root in ($BuildRoots | Select-Object -Unique)) {
            foreach ($profile in @('debug', 'release')) {
                $path = Join-Path $root $profile
                if (Test-Path -LiteralPath $path -PathType Container) {
                    [pscustomobject]@{ Path = $path; Reason = 'build artifacts' }
                }
            }
        }
    }
    if ($Reports -and (Test-Path -LiteralPath $ReportRoot)) {
        $reportDirectories = @(Get-ChildItem -LiteralPath $ReportRoot -Directory | Sort-Object LastWriteTime -Descending)
        $cutoff = (Get-Date).AddDays(-$ReportDays)
        $reportDirectories | Select-Object -Skip $KeepReports | Where-Object {
            $_.LastWriteTime -lt $cutoff -and -not (Test-Path -LiteralPath (Join-Path $_.FullName '.keep'))
        } | ForEach-Object { [pscustomobject]@{ Path = $_.FullName; Reason = 'expired report' } }
    }
    if ($Fixtures -and (Test-Path -LiteralPath $FixtureRoot)) {
        Get-ChildItem -LiteralPath $FixtureRoot -File | Where-Object {
            $_.Name -match '^1080p-h264-\d+s\.mp4$' -and
            (Test-Path -LiteralPath ($_.FullName + '.scry-generated') -PathType Leaf)
        } | ForEach-Object {
            [pscustomobject]@{ Path = $_.FullName; Reason = 'generated fixture' }
            [pscustomobject]@{ Path = ($_.FullName + '.scry-generated'); Reason = 'fixture ownership marker' }
        }
    }
}

function Assert-ScryMaintenanceIdle([string[]]$BuildRoots, [string]$Repository) {
    foreach ($process in (Get-Process -ErrorAction Stop)) {
        if ($process.ProcessName -in @('cargo', 'rustc', 'rustdoc', 'clippy-driver', 'ffmpeg')) {
            throw "Close build/profiling tools before cleanup ($($process.ProcessName), PID $($process.Id))."
        }
        if ($process.ProcessName -match '^python(?:\d+(?:\.\d+)?)?$') {
            $details = Get-CimInstance Win32_Process -Filter "ProcessId = $($process.Id)" -ErrorAction Stop
            if (-not $details.CommandLine) { throw 'Cannot verify a Python process; close it before cleanup.' }
            if ($details.CommandLine -match 'profile-(?:app|suite)\.py') {
                throw 'Close the profiling runner before cleanup.'
            }
        }
        if ($process.ProcessName -eq 'scry') {
            $path = $process.Path
            if (-not $path) { throw 'Cannot verify a running Scry process; close it before cleanup.' }
            foreach ($root in (@($Repository) + $BuildRoots)) {
                if ($path.StartsWith([IO.Path]::GetFullPath($root).TrimEnd('\', '/') + [IO.Path]::DirectorySeparatorChar, [StringComparison]::OrdinalIgnoreCase)) {
                    throw 'Close the development/profiling app before cleanup.'
                }
            }
        }
    }
}

function Assert-ScryCleanupPath([string]$Path) {
    # Never follow junctions/symlinks into data outside the planned directory.
    $item = Get-Item -LiteralPath $Path -Force
    if ($item.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Cleanup refuses linked paths.' }
    $ancestor = if ($item.PSIsContainer) { $item } else { $item.Directory }
    for ($parent = $ancestor; $null -ne $parent; $parent = $parent.Parent) {
        if ($parent.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Cleanup refuses linked paths.' }
    }
    if ($item.PSIsContainer) {
        foreach ($child in (Get-ChildItem -LiteralPath $Path -Force -Recurse)) {
            if ($child.Attributes -band [IO.FileAttributes]::ReparsePoint) { throw 'Cleanup refuses linked contents.' }
        }
    }
}
