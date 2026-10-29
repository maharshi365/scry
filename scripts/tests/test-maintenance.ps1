$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/../maintenance.ps1"

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

$temporary = Join-Path $env:LOCALAPPDATA ('Temp/opencode/scry-maintenance-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $temporary
try {
    foreach ($name in @('debug', 'release', 'dev-data', 'criterion', 'perf-reports/old', 'perf-reports/new', 'perf-reports/recent', 'perf-reports/reference', 'perf-fixtures')) {
        $null = New-Item -ItemType Directory -Path (Join-Path $temporary $name) -Force
    }
    Set-Content (Join-Path $temporary 'perf-reports/reference/.keep') 'reference'
    Set-Content (Join-Path $temporary 'perf-fixtures/1080p-h264-120s.mp4') 'generated'
    Set-Content (Join-Path $temporary 'perf-fixtures/1080p-h264-120s.mp4.scry-generated') 'Scry generated fixture'
    Set-Content (Join-Path $temporary 'perf-fixtures/1080p-h264-180s.mp4') 'unowned'
    Set-Content (Join-Path $temporary 'perf-fixtures/personal.mp4') 'external'
    (Get-Item (Join-Path $temporary 'perf-reports/old')).LastWriteTime = (Get-Date).AddDays(-60)
    (Get-Item (Join-Path $temporary 'perf-reports/reference')).LastWriteTime = (Get-Date).AddDays(-60)
    (Get-Item (Join-Path $temporary 'perf-reports/recent')).LastWriteTime = (Get-Date).AddDays(-2)
    $plan = @(Get-ScryCleanupPlan -BuildRoots @($temporary) -ReportRoot (Join-Path $temporary 'perf-reports') -FixtureRoot (Join-Path $temporary 'perf-fixtures') -Builds -Reports -Fixtures -KeepReports 1)
    $paths = @($plan.Path | ForEach-Object { [IO.Path]::GetFullPath($_) })
    function Test-Selected([string]$Relative) { return [IO.Path]::GetFullPath((Join-Path $temporary $Relative)) -in $paths }
    Assert-True (Test-Selected 'debug') 'Build cleanup must include debug artifacts.'
    Assert-True (Test-Selected 'release') 'Build cleanup must include release artifacts.'
    Assert-True (-not (Test-Selected 'dev-data')) 'Build cleanup must preserve development data.'
    Assert-True (-not (Test-Selected 'criterion')) 'Build cleanup must preserve benchmark baselines.'
    Assert-True (Test-Selected 'perf-reports/old') 'Report retention must select expired reports.'
    Assert-True (-not (Test-Selected 'perf-reports/new')) 'Report retention must preserve the latest report.'
    Assert-True (-not (Test-Selected 'perf-reports/recent')) 'Report retention must preserve recent reports beyond the newest-count limit.'
    Assert-True (-not (Test-Selected 'perf-reports/reference')) 'Pinned reports must survive retention.'
    Assert-True (Test-Selected 'perf-fixtures/1080p-h264-120s.mp4') 'Explicit fixture cleanup must select generated media.'
    Assert-True (Test-Selected 'perf-fixtures/1080p-h264-120s.mp4.scry-generated') 'Fixture cleanup must remove its ownership marker.'
    Assert-True (-not (Test-Selected 'perf-fixtures/1080p-h264-180s.mp4')) 'Matching a filename alone must not permit deleting media.'
    Assert-True (-not (Test-Selected 'perf-fixtures/personal.mp4')) 'Fixture cleanup must not select other media.'
    Assert-True (Test-Path (Join-Path $temporary 'debug')) 'Planning must not delete files.'

    Assert-ScryCleanupPath (Join-Path $temporary 'debug')
    $link = Join-Path $temporary 'linked-build'
    $null = New-Item -ItemType Junction -Path $link -Target (Join-Path $temporary 'dev-data')
    $blocked = $false
    try { Assert-ScryCleanupPath $link } catch { $blocked = $true }
    Assert-True $blocked 'Cleanup must refuse a junction into development data.'
    Remove-Item -LiteralPath $link -Force
    $link = Join-Path $temporary 'debug/linked-data'
    $null = New-Item -ItemType Junction -Path $link -Target (Join-Path $temporary 'dev-data')
    $blocked = $false
    try { Assert-ScryCleanupPath (Join-Path $temporary 'debug') } catch { $blocked = $true }
    Assert-True $blocked 'Cleanup must refuse linked contents inside a build directory.'
    Remove-Item -LiteralPath $link -Force

    function Get-Process { param($ErrorAction) [pscustomobject]@{ ProcessName = 'cargo'; Path = 'C:\tools\cargo.exe' } }
    $blocked = $false
    try { Assert-ScryMaintenanceIdle -BuildRoots @($temporary) -Repository $temporary } catch { $blocked = $true }
    Assert-True $blocked 'Cleanup must refuse deletion while Cargo is active.'

    function Get-Process { param($ErrorAction) [pscustomobject]@{ ProcessName = 'python'; Id = 123 } }
    function Get-CimInstance { param($ClassName, $Filter, $ErrorAction) [pscustomobject]@{ CommandLine = 'python.exe editor-language-server.py' } }
    Assert-ScryMaintenanceIdle -BuildRoots @($temporary) -Repository $temporary
    function Get-CimInstance { param($ClassName, $Filter, $ErrorAction) [pscustomobject]@{ CommandLine = 'python.exe scripts/profile-suite.py' } }
    $blocked = $false
    try { Assert-ScryMaintenanceIdle -BuildRoots @($temporary) -Repository $temporary } catch { $blocked = $true }
    Assert-True $blocked 'Cleanup must refuse deletion while the profiling suite is active.'
} finally { Remove-Item -LiteralPath $temporary -Recurse -Force }
Write-Host 'Maintenance tests passed.'
