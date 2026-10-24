# Read-only navigation workload for the actual Windows UI. Needs Cinemeta installed.
# Does not install add-ons, edit library state, request playback, or use the desktop capture.
[CmdletBinding()]
param(
    [string]$Executable = "$PSScriptRoot/../target/release/scry.exe",
    [ValidateRange(1, 60)][int]$StepSeconds = 3,
    [ValidateRange(1, 10)][int]$ScrollPasses = 1,
    [switch]$ScrollOnly,
    [string]$Report,
    [string]$CaptureDirectory
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/native-ui.ps1"
if (-not ('ScryProcessCounters' -as [type])) {
    Add-Type @'
using System;
using System.Runtime.InteropServices;
public static class ScryProcessCounters {
    [StructLayout(LayoutKind.Sequential)] public struct Counters {
        public uint Size, Faults;
        public UIntPtr PeakWorkingSet, WorkingSet, PeakPaged, Paged, PeakNonPaged, NonPaged;
        public UIntPtr Pagefile, PeakPagefile, PrivateUsage, PrivateWorkingSet, SharedCommit;
    }
    [DllImport("kernel32.dll", SetLastError=true)]
    static extern bool K32GetProcessMemoryInfo(IntPtr process, ref Counters counters, uint size);
    public static Counters Read(IntPtr process) {
        var counters = new Counters();
        counters.Size = (uint)Marshal.SizeOf<Counters>();
        if (!K32GetProcessMemoryInfo(process, ref counters, counters.Size))
            throw new System.ComponentModel.Win32Exception(Marshal.GetLastWin32Error());
        return counters;
    }
}
'@
}
if ($CaptureDirectory) { [void][IO.Directory]::CreateDirectory([IO.Path]::GetFullPath($CaptureDirectory)) }
$process = Start-Process -FilePath (Resolve-Path $Executable) -PassThru
$results = [Collections.Generic.List[object]]::new()
try {
    Start-Sleep -Seconds 15
    $window = Get-ScryWindow $process.Id
    $rect = [ScryNativeUi+Rect]::new()
    [void][ScryNativeUi]::GetClientRect($window, [ref]$rect)
    if ($rect.Right -ne 1280 -or $rect.Bottom -ne 820) {
        throw 'Workload needs a 1280x820 client area at 100% display scaling. No clicks were sent.'
    }

    function Measure-Step([string]$Name, [scriptblock]$Action, [int]$Seconds = $StepSeconds) {
        $process.Refresh()
        $before = $process.TotalProcessorTime.TotalSeconds
        $clock = [Diagnostics.Stopwatch]::StartNew()
        & $Action
        $peakPrivate = 0L
        $peakCommit = 0L
        $peakResident = 0L
        $sampleCount = 0
        do {
            $memory = [ScryProcessCounters]::Read($process.Handle)
            $peakPrivate = [Math]::Max($peakPrivate, [long]$memory.PrivateWorkingSet.ToUInt64())
            $peakCommit = [Math]::Max($peakCommit, [long]$memory.PrivateUsage.ToUInt64())
            $peakResident = [Math]::Max($peakResident, [long]$memory.WorkingSet.ToUInt64())
            $sampleCount++
            Start-Sleep -Milliseconds 100
        } while ($clock.Elapsed.TotalSeconds -lt $Seconds)
        $process.Refresh()
        $cpu = $process.TotalProcessorTime.TotalSeconds - $before
        $result = [pscustomobject]@{
            step = $Name
            elapsed_s = [Math]::Round($clock.Elapsed.TotalSeconds, 3)
            cpu_ms = [Math]::Round(1000 * $cpu, 2)
            samples = $sampleCount
            peak_private_working_set_mib = [Math]::Round($peakPrivate / 1MB, 2)
            final_private_working_set_mib = [Math]::Round($memory.PrivateWorkingSet.ToUInt64() / 1MB, 2)
            peak_private_commit_mib = [Math]::Round($peakCommit / 1MB, 2)
            peak_resident_mib = [Math]::Round($peakResident / 1MB, 2)
        }
        $results.Add($result)
        $result | ConvertTo-Json -Compress
        if ($CaptureDirectory) { Save-ScryWindow $window (Join-Path $CaptureDirectory "$Name.png") }
    }

    Measure-Step 'discover' {}
    foreach ($pass in 1..$ScrollPasses) {
        foreach ($index in 1..4) {
            Measure-Step "scroll-$pass-$index" { [ScryNativeUi]::Wheel($window, 900, 600, -1800) }
        }
        Measure-Step "return-to-top-$pass" { [ScryNativeUi]::Wheel($window, 900, 600, 100000) }
    }
    if ($ScrollOnly) {
        if ($Report) { $results.ToArray() | ConvertTo-Json -Depth 5 | Set-Content $Report }
        return
    }
    Measure-Step 'movie-detail' { [ScryNativeUi]::Click($window, 240, 260) } -Seconds 6
    Measure-Step 'back-to-discover' { [ScryNativeUi]::Click($window, 162, 34) }
    Measure-Step 'library' { [ScryNativeUi]::Click($window, 60, 212) }
    Measure-Step 'addons' { [ScryNativeUi]::Click($window, 60, 252) }
    Measure-Step 'series' { [ScryNativeUi]::Click($window, 60, 172) } -Seconds 8
    Measure-Step 'series-detail' { [ScryNativeUi]::Click($window, 240, 260) } -Seconds 6
    Measure-Step 'back-to-series' { [ScryNativeUi]::Click($window, 162, 34) }
    Measure-Step 'search' {
        [ScryNativeUi]::Click($window, 400, 34)
        [ScryNativeUi]::Shortcut(17, 65)
        [ScryNativeUi]::Type('Blade Runner')
        [ScryNativeUi]::Press(13)
    } -Seconds 8
    Measure-Step 'search-detail' { [ScryNativeUi]::Click($window, 240, 260) } -Seconds 6
    Measure-Step 'back-to-search' { [ScryNativeUi]::Click($window, 162, 34) }
    if ($Report) { $results.ToArray() | ConvertTo-Json -Depth 5 | Set-Content $Report }
} finally {
    if (-not $process.HasExited) {
        [void]$process.CloseMainWindow()
        if (-not $process.WaitForExit(15000)) { Write-Warning "Scry did not close. PID: $($process.Id)" }
    }
    $process.Dispose()
}
