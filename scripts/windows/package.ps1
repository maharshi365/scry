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
    if ($version -notmatch '^(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)(?:-[0-9A-Za-z.-]+)?(?:\+[0-9A-Za-z.-]+)?$') {
        throw 'Installer version must be a semantic version.'
    }
    $numericVersion = "$($Matches[1]).$($Matches[2]).$($Matches[3])"
    if (@($numericVersion.Split('.') | Where-Object { [long]$_ -gt 65535 }).Count) {
        throw 'Windows installer version components must not exceed 65535.'
    }
    $compiler = Get-Command ISCC.exe -ErrorAction SilentlyContinue
    $compilerPath = if ($compiler) { $compiler.Source } else { Join-Path ${env:ProgramFiles(x86)} 'Inno Setup 6/ISCC.exe' }
    if (-not (Test-Path -LiteralPath $compilerPath)) { throw 'Install Inno Setup 6 before packaging.' }
    $null = New-Item -ItemType Directory -Path $Destination -Force
    $Destination = (Resolve-Path -LiteralPath $Destination).Path
    $staging = Join-Path ([System.IO.Path]::GetTempPath()) ('scry-package-' + [guid]::NewGuid().ToString('N'))
    $null = New-Item -ItemType Directory -Path $staging
    try {
        Copy-Item -LiteralPath $executable -Destination (Join-Path $staging 'scry.exe')
        Copy-Item -LiteralPath 'crates/scry-app/assets/fonts/OFL.txt' -Destination (Join-Path $staging 'FONT-LICENSE.txt')
        Copy-ScryRuntime $staging
        $name = "Scry-$version-windows-x64-setup.exe"
        $archive = Join-Path $Destination $name
        & $compilerPath "/DAppVersion=$version" "/DNumericVersion=$numericVersion" "/DSourceDirectory=$staging" "/DDestinationDirectory=$Destination" "$PSScriptRoot/installer.iss"
        if ($LASTEXITCODE -ne 0 -or -not (Test-Path -LiteralPath $archive)) { throw 'Installer compilation failed.' }
        $hash = (Get-FileHash -LiteralPath $archive -Algorithm SHA256).Hash.ToLowerInvariant()
        Set-Content -LiteralPath "$archive.sha256" -Value "$hash  $name" -Encoding ascii
    } finally {
        Remove-Item -LiteralPath $staging -Recurse -Force
    }
} finally { Pop-Location }
