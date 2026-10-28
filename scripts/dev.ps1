[CmdletBinding()]
param([switch]$Release, [switch]$Force, [switch]$Playground)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
    throw 'This script requires x64 Windows and PowerShell 7.'
}
. "$PSScriptRoot/app-deployment.ps1"

$executable = Build-Scry -Release:$Release -Development
# Each checkout gets its own run directory and isolated data.
$runDirectory = if ($Playground) { 'target/playground-run' } else { 'target/dev-run' }
$directory = Join-Path $ScryRepository $runDirectory
Publish-Scry $executable $directory -Force:$Force
$previousRoot = $env:SCRY_DATA_ROOT
try {
    $env:SCRY_DATA_ROOT = Join-Path $ScryRepository 'target/dev-data'
    $launch = @{ FilePath = (Join-Path $directory 'scry.exe'); WorkingDirectory = $directory }
    if ($Playground) { $launch.ArgumentList = @('--playground') }
    Start-Process @launch
} finally { $env:SCRY_DATA_ROOT = $previousRoot }
if ($Playground) {
    Write-Host 'UI playground launched with in-memory fixtures only.'
} else {
    Write-Host 'Development app launched with isolated data in target/dev-data.'
}
