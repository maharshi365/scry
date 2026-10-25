$ErrorActionPreference = 'Stop'
. "$PSScriptRoot/../app-deployment.ps1"

function Assert-True([bool]$Condition, [string]$Message) {
    if (-not $Condition) { throw $Message }
}

function New-TestProcess([string]$Path, [bool]$ExitsNormally) {
    $process = [pscustomobject]@{
        Path = $Path
        HasExited = $false
        CloseRequested = $false
        Killed = $false
        ExitsNormally = $ExitsNormally
    }
    $process | Add-Member ScriptMethod CloseMainWindow { $this.CloseRequested = $true; return $true }
    $process | Add-Member ScriptMethod WaitForExit { param($Timeout) return $this.ExitsNormally -or $this.Killed }
    $process | Add-Member ScriptMethod Kill { $this.Killed = $true }
    $process | Add-Member ScriptMethod Dispose { }
    return $process
}

function Get-Process { param($Name, $ErrorAction) return $script:Processes }

$installed = 'C:\Apps\Scry\scry.exe'
$production = New-TestProcess $installed $true
$development = New-TestProcess 'C:\repo\target\dev-run\scry.exe' $true
$script:Processes = @($production, $development)
Stop-ScryExecutable $installed
Assert-True $production.CloseRequested 'The installed app must receive a normal close request.'
Assert-True (-not $production.Killed) 'Normal shutdown must not force-stop the app.'
Assert-True (-not $development.CloseRequested) 'Updating production must not close development.'

$stuck = New-TestProcess $installed $false
$script:Processes = @($stuck)
$failed = $false
try { Stop-ScryExecutable $installed } catch { $failed = $true }
Assert-True $failed 'A shutdown timeout must abort deployment without -Force.'
Assert-True (-not $stuck.Killed) 'A shutdown timeout must not imply permission to force-stop.'
Stop-ScryExecutable $installed -Force
Assert-True $stuck.Killed 'Explicit -Force must stop an unresponsive installed app.'

$temporary = Join-Path ([IO.Path]::GetTempPath()) ('scry-deployment-test-' + [guid]::NewGuid().ToString('N'))
$null = New-Item -ItemType Directory -Path $temporary
try {
    $source = Join-Path $temporary 'new.exe'
    $destination = Join-Path $temporary 'installed'
    $null = New-Item -ItemType Directory -Path $destination
    Set-Content -LiteralPath $source -Value 'new'
    Set-Content -LiteralPath (Join-Path $destination 'scry.exe') -Value 'old'
    function Copy-ScryRuntime { param($Destination) throw 'Simulated runtime preparation failure.' }
    $running = New-TestProcess (Join-Path $destination 'scry.exe') $true
    $script:Processes = @($running)
    $failed = $false
    try { Publish-Scry $source $destination } catch { $failed = $true }
    Assert-True $failed 'Runtime preparation failure must abort deployment.'
    Assert-True (-not $running.CloseRequested) 'Prepare all files before closing the running app.'
    Assert-True ((Get-Content -LiteralPath (Join-Path $destination 'scry.exe') -Raw).Trim() -eq 'old') 'Preparation failure must preserve the installed executable.'
    Assert-True (@(Get-ChildItem -LiteralPath $destination -Force).Count -eq 1) 'Failed deployment must clean its staging files.'

    function Copy-ScryRuntime {
        param($Destination)
        Set-Content -LiteralPath (Join-Path $Destination 'libmpv-2.dll') -Value 'verified-runtime'
    }
    Publish-Scry $source $destination
    Assert-True $running.CloseRequested 'Publishing an update must close the deployed app.'
    Assert-True ((Get-Content -LiteralPath (Join-Path $destination 'scry.exe') -Raw).Trim() -eq 'new') 'Publishing must replace the deployed executable.'
    Assert-True ((Get-Content -LiteralPath (Join-Path $destination 'libmpv-2.dll') -Raw).Trim() -eq 'verified-runtime') 'Publishing must deploy the prepared runtime beside the executable.'
    Assert-True (@(Get-ChildItem -LiteralPath $destination -Force).Count -eq 2) 'Successful deployment must clean its staging files.'
} finally { Remove-Item -LiteralPath $temporary -Recurse -Force }

Write-Host 'Deployment tests passed.'
