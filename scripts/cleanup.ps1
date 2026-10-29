[CmdletBinding()]
param(
    [switch]$Apply,
    [switch]$Builds,
    [switch]$Reports,
    [switch]$Fixtures,
    [ValidateRange(0, 1000)][int]$KeepReports = 5,
    [ValidateRange(1, 3650)][int]$ReportDays = 30,
    [ValidateRange(1, 10000)][int]$BudgetGiB = 30
)
$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/build-paths.ps1"
. "$PSScriptRoot/maintenance.ps1"
$roots = @((Join-Path $ScryRepository 'target'), (Get-ScryBuildDirectory)) | Select-Object -Unique
$plan = @(Get-ScryCleanupPlan -BuildRoots $roots -ReportRoot (Join-Path $ScryRepository 'target/perf-reports') -FixtureRoot (Join-Path $ScryRepository 'target/perf-fixtures') -Builds:$Builds -Reports:$Reports -Fixtures:$Fixtures -KeepReports $KeepReports -ReportDays $ReportDays)
$bytes = 0L
foreach ($root in $roots) {
    foreach ($profile in @('debug', 'release')) {
        $path = Join-Path $root $profile
        if (Test-Path -LiteralPath $path) {
            Assert-ScryCleanupPath $path
            $size = (Get-ChildItem -LiteralPath $path -File -Recurse -Force | Measure-Object Length -Sum).Sum
            $bytes += $size
            Write-Host ('{0:N2} GiB  {1}' -f ($size / 1GB), $path)
        }
    }
}
if ($bytes -gt $BudgetGiB * 1GB) { Write-Warning "Build outputs exceed the $BudgetGiB GiB budget. Preview selective cleanup with -Builds." }
foreach ($entry in $plan) { Write-Host "$($entry.Reason): $($entry.Path)" }
if (-not $Apply) { Write-Host 'Preview only. Add -Apply to delete the listed paths.'; return }
if (-not $plan.Count) { Write-Host 'Nothing selected for cleanup.'; return }
Assert-ScryMaintenanceIdle -BuildRoots $roots -Repository $ScryRepository
foreach ($entry in $plan) { Assert-ScryCleanupPath $entry.Path }
foreach ($entry in $plan) { Remove-Item -LiteralPath $entry.Path -Recurse -Force }
Write-Host 'Selected cleanup completed. Development data and Criterion baselines were preserved.'
