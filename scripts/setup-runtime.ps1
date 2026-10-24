param([string]$SevenZip = "7z")
$ErrorActionPreference = "Stop"
$root = Split-Path $PSScriptRoot -Parent
$runtime = Join-Path $root "runtime"
$archive = Join-Path $runtime "mpv-dev-x86_64-20261021.7z"
$url = "https://github.com/shinchiro/mpv-winbuild-cmake/releases/download/20261021/mpv-dev-x86_64-20261021-git-e471496c56.7z"
$expected = "efc719eff3dc5954b4a5efc800632808bcec01cc9034940dd14e5eb7fe9ea633"
if (-not [Environment]::Is64BitProcess) { throw "Use 64-bit PowerShell." }
Get-Command $SevenZip -ErrorAction Stop | Out-Null
New-Item -ItemType Directory -Path $runtime -Force | Out-Null
if (-not (Test-Path $archive)) { Invoke-WebRequest $url -OutFile $archive }
if ((Get-FileHash $archive -Algorithm SHA256).Hash.ToLowerInvariant() -ne $expected) {
    throw "Runtime checksum mismatch. Do not use this archive."
}
& $SevenZip x $archive "-o$runtime" -y | Out-Null
if ($LASTEXITCODE -ne 0) { throw "Cannot extract libmpv runtime." }
if (-not (Test-Path (Join-Path $runtime "libmpv-2.dll"))) { throw "Archive did not contain libmpv-2.dll." }
Write-Host "Verified libmpv runtime is ready in runtime/. Review upstream mpv/FFmpeg licensing before redistribution."
