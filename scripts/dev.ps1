[CmdletBinding()]
param([switch]$Release, [switch]$Force)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
    throw 'This script requires x64 Windows and PowerShell 7.'
}
. "$PSScriptRoot/app-deployment.ps1"

$executable = Build-Scry -Release:$Release
# Each checkout gets its own run directory and isolated data.
$directory = Join-Path $ScryRepository 'target/dev-run'
Publish-Scry $executable $directory -Force:$Force
$previousRoot = $env:SCRY_DATA_ROOT
try {
    $env:SCRY_DATA_ROOT = Join-Path $ScryRepository 'target/dev-data'
    Start-Process -FilePath (Join-Path $directory 'scry.exe') -WorkingDirectory $directory
} finally { $env:SCRY_DATA_ROOT = $previousRoot }
Write-Host 'Development app launched with isolated data in target/dev-data.'
