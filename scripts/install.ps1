[CmdletBinding()]
param([switch]$Launch, [switch]$Force)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
    throw 'This script requires x64 Windows and PowerShell 7.'
}
. "$PSScriptRoot/app-deployment.ps1"

$directory = Join-Path $env:LOCALAPPDATA 'Programs/Scry'
$executable = Build-Scry -Release
Publish-Scry $executable $directory -Force:$Force

$userPath = [Environment]::GetEnvironmentVariable('Path', 'User')
$entries = @($userPath -split ';' | Where-Object { $_ })
if ($directory -notin $entries) {
    [Environment]::SetEnvironmentVariable('Path', (($entries + $directory) -join ';'), 'User')
}
if ($directory -notin ($env:PATH -split ';')) { $env:PATH += ";$directory" }
$shell = New-Object -ComObject WScript.Shell
$shortcut = $shell.CreateShortcut((Join-Path ([Environment]::GetFolderPath('Programs')) 'Scry.lnk'))
$shortcut.TargetPath = Join-Path $directory 'scry.exe'
$shortcut.WorkingDirectory = $directory
$shortcut.Description = 'Scry'
$shortcut.Save()
Write-Host "Installed Scry in $directory. Open a new terminal to use scry from PATH."
if ($Launch) { Start-Process -FilePath (Join-Path $directory 'scry.exe') -WorkingDirectory $directory }
