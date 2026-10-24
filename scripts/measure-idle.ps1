# Measure the real native window, not the no-window playback probe.
[CmdletBinding()]
param(
    [string]$Executable = "$PSScriptRoot/../target/release/scry.exe",
    [int]$ProcessId = 0,
    [ValidateRange(1, 300)][int]$WarmupSeconds = 15,
    [ValidateRange(1, 300)][int]$Seconds = 15,
    [string]$Report
)
$ErrorActionPreference = 'Stop'
$owned = $ProcessId -eq 0
$process = if ($owned) { Start-Process -FilePath (Resolve-Path $Executable) -PassThru } else { Get-Process -Id $ProcessId }
try {
    Start-Sleep -Seconds $WarmupSeconds
    $process.Refresh()
    $cpu = $process.TotalProcessorTime.TotalSeconds
    $clock = [Diagnostics.Stopwatch]::StartNew()
    $samples = [Collections.Generic.List[object]]::new()
    while ($clock.Elapsed.TotalSeconds -lt $Seconds) {
        $process.Refresh()
        if ($process.HasExited) { throw 'Scry exited during measurement' }
        $samples.Add([pscustomobject]@{
            elapsed_s = $clock.Elapsed.TotalSeconds
            resident_bytes = $process.WorkingSet64
            private_bytes = $process.PrivateMemorySize64
            cpu_time_s = $process.TotalProcessorTime.TotalSeconds - $cpu
            threads = $process.Threads.Count
        })
        Start-Sleep -Milliseconds 250
    }
    $process.Refresh()
    $elapsed = $clock.Elapsed.TotalSeconds
    $cpuUsed = $process.TotalProcessorTime.TotalSeconds - $cpu
    # Task Manager's default Memory column is private working set, not commit.
    $privateWorkingSet = Get-CimInstance Win32_PerfRawData_PerfProc_Process -Filter "IDProcess = $($process.Id)" -ErrorAction SilentlyContinue
    $summary = [pscustomobject]@{
        samples = $samples.Count
        mean_resident_mib = [Math]::Round(($samples | Measure-Object resident_bytes -Average).Average / 1MB, 2)
        peak_resident_mib = [Math]::Round(($samples | Measure-Object resident_bytes -Maximum).Maximum / 1MB, 2)
        mean_private_mib = [Math]::Round(($samples | Measure-Object private_bytes -Average).Average / 1MB, 2)
        final_private_working_set_mib = if ($privateWorkingSet) { [Math]::Round($privateWorkingSet.WorkingSetPrivate / 1MB, 2) } else { $null }
        # 100% means one fully occupied CPU core, not the whole machine.
        cpu_percent_one_core = [Math]::Round(100 * $cpuUsed / $elapsed, 3)
        threads = $samples[-1].threads
    }
    if ($Report) {
        @{ summary = $summary; samples = $samples.ToArray() } | ConvertTo-Json -Depth 5 | Set-Content -Path $Report
    }
    $summary
} finally {
    if ($owned -and -not $process.HasExited) {
        [void]$process.CloseMainWindow()
        if (-not $process.WaitForExit(15000)) { Write-Warning "Scry did not close. PID: $($process.Id)" }
    }
    $process.Dispose()
}
