[CmdletBinding()]
param([Parameter(Mandatory)][string]$Destination)
$ErrorActionPreference = 'Stop'
if (-not $IsWindows -or [System.Runtime.InteropServices.RuntimeInformation]::OSArchitecture -ne 'X64') {
    throw 'This script requires x64 Windows and PowerShell 7.'
}
. "$PSScriptRoot/app-deployment.ps1"
$executable = Build-Scry -Release
Push-Location $ScryRepository
try {
    $json = & cargo metadata --no-deps --format-version 1 --locked
    if ($LASTEXITCODE -ne 0) { throw 'Cannot read Scry version.' }
    $version = (($json | ConvertFrom-Json).packages | Where-Object name -eq 'scry-app').version
    if ($env:SCRY_BUILD_VERSION) { $version = $env:SCRY_BUILD_VERSION }
    $null = New-Item -ItemType Directory -Path $Destination -Force
    $Destination = (Resolve-Path -LiteralPath $Destination).Path
    $staging = Join-Path ([System.IO.Path]::GetTempPath()) ('scry-package-' + [guid]::NewGuid().ToString('N'))
    $null = New-Item -ItemType Directory -Path $staging
    try {
        Copy-Item -LiteralPath $executable -Destination (Join-Path $staging 'scry.exe')
        Copy-Item -LiteralPath 'crates/scry-app/assets/fonts/OFL.txt' -Destination (Join-Path $staging 'FONT-LICENSE.txt')
        Copy-ScryRuntime $staging
        $name = "Scry-$version-windows-x64.zip"
        $archive = Join-Path $Destination $name
        Compress-Archive -Path "$staging/*" -DestinationPath $archive -Force
        $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        Set-Content -LiteralPath "$archive.sha256" -Value "$hash  $name" -Encoding ascii
    } finally {
        Remove-Item -LiteralPath $staging -Recurse -Force
    }
} finally { Pop-Location }
