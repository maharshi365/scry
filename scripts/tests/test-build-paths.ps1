$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/../build-paths.ps1"
function cargo {
    $global:LASTEXITCODE = 0
    return '{"target_directory":"C:/custom build/cache"}'
}
$actual = [IO.Path]::GetFullPath((Get-ScryExecutable -Release))
$expected = [IO.Path]::GetFullPath('C:/custom build/cache/release/scry.exe')
if ($actual -ne $expected) { throw 'Executable resolution must use Cargo metadata, not the repository target folder.' }
function cargo { $global:LASTEXITCODE = 1 }
$failed = $false
try { Get-ScryExecutable -Release } catch { $failed = $true }
if (-not $failed) { throw 'Metadata failure must not fall back to a stale executable.' }
Write-Host 'Build-path tests passed.'
