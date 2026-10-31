[CmdletBinding()]
param([Parameter(Mandatory)][string]$Installer)
$ErrorActionPreference = 'Stop'
if ($env:GITHUB_ACTIONS -ne 'true') { throw 'Installer smoke tests run only on disposable GitHub Actions runners.' }
$directory = Join-Path $env:RUNNER_TEMP 'scry-installer-test'
$arguments = @('/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART', '/SP-', "/DIR=`"$directory`"")
$process = Start-Process -FilePath $Installer -ArgumentList $arguments -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "Installation failed: $($process.ExitCode)" }
foreach ($name in @('scry.exe', 'libmpv-2.dll', 'FONT-LICENSE.txt', 'unins000.exe')) {
    if (-not (Test-Path -LiteralPath (Join-Path $directory $name))) { throw "Installer omitted $name" }
}
$executable = Join-Path $directory 'scry.exe'
$expected = (Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash
# Reinstallation must replace the app, not leave a changed executable behind.
Set-Content -LiteralPath $executable -Value 'old app payload' -Encoding ascii
$process = Start-Process -FilePath $Installer -ArgumentList $arguments -Wait -PassThru
if ($process.ExitCode -ne 0) { throw "Reinstallation failed: $($process.ExitCode)" }
if ((Get-FileHash -LiteralPath $executable -Algorithm SHA256).Hash -ne $expected) {
    throw 'Reinstallation did not replace the app executable.'
}
$process = Start-Process -FilePath (Join-Path $directory 'unins000.exe') -ArgumentList '/VERYSILENT', '/SUPPRESSMSGBOXES', '/NORESTART' -Wait -PassThru
if ($process.ExitCode -ne 0 -or (Test-Path -LiteralPath $executable)) { throw 'Uninstallation did not remove the app.' }
