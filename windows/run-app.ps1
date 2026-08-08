#Requires -Version 7.0
<#
  windows/run-app.ps1 -- per-app probe (Windows mirror of linux/run-app.sh).

  Strictly serial: timed clean release build (unless -SkipBuild), launch the
  binary, wait 10 s, check aliveness and visible top-level windows (including
  msedgewebview2.exe descendants for webview frameworks), kill the tree, and
  record everything to $OutputRoot/runs/<app>/<variant>/result.tsv.

  A recorded failure (compile fail, dead process, no window) is SUCCESS for
  the harness: the script still exits 0. A nonzero exit means infrastructure
  broke and no result.tsv was produced.

  Usage:
    pwsh -NoProfile -File windows/run-app.ps1 -App iced-app -OutputRoot <cohort>/windows
      [-Variant tiny-skia] [-EnvOverrides @{ ICED_BACKEND = 'tiny-skia' }] [-SkipBuild]
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$App,
    [string]$Variant = 'default',
    [hashtable]$EnvOverrides = @{},
    [Parameter(Mandatory)][string]$OutputRoot,
    [switch]$SkipBuild
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-Utf8NoBom([string]$Path,[string]$Content) { [IO.File]::WriteAllText($Path, $Content.Replace("`r`n","`n"), [Text.UTF8Encoding]::new($false)) }

$RepoRoot = Split-Path -Parent $PSScriptRoot
$AppDir = Join-Path (Join-Path $RepoRoot 'apps') $App
if (-not (Test-Path -LiteralPath (Join-Path $AppDir 'Cargo.toml'))) {
    [Console]::Error.WriteLine("run-app: missing app: apps/$App (no Cargo.toml)")
    exit 1
}
if (-not [IO.Path]::IsPathRooted($OutputRoot)) { $OutputRoot = Join-Path $RepoRoot $OutputRoot }
$ResultDir = Join-Path (Join-Path (Join-Path $OutputRoot 'runs') $App) $Variant
New-Item -ItemType Directory -Force -Path $ResultDir | Out-Null
$BuildLog = Join-Path $ResultDir 'build.log'
$RunLog = Join-Path $ResultDir 'run.log'

$EnvSerialized = ''
if ($EnvOverrides.Count -gt 0) {
    $EnvSerialized = (@($EnvOverrides.Keys) | Sort-Object | ForEach-Object { '{0}={1}' -f $_, $EnvOverrides[$_] }) -join ';'
}

$Columns = @('app', 'variant', 'compile_ok', 'run_alive_10s', 'visible_window', 'clean_build_secs',
    'binary_bytes', 'pdb_bytes', 'exit_code', 'env_overrides', 'notes')

function Format-Notes([string]$Text) {
    if ($null -eq $Text) { return '' }
    return ($Text -replace "[\t\r\n]+", ' ').Trim()
}

function Write-Result([hashtable]$Values) {
    $row = foreach ($c in $Columns) { "$($Values[$c])" }
    $content = ($Columns -join "`t") + "`n" + ($row -join "`t") + "`n"
    Write-Utf8NoBom (Join-Path $ResultDir 'result.tsv') $content
}

function Add-RunLogLine([string]$Line) {
    $stamp = (Get-Date).ToUniversalTime().ToString('yyyy-MM-ddTHH:mm:ssZ')
    [IO.File]::AppendAllText($RunLog, "[$stamp] $Line`n", [Text.UTF8Encoding]::new($false))
}

# Recursive CIM Win32_Process walk from a root PID (name + pid per descendant).
function Get-ProcessDescendants([int]$RootPid) {
    $all = @(Get-CimInstance -ClassName Win32_Process -ErrorAction SilentlyContinue |
        Select-Object -Property ProcessId, ParentProcessId, Name)
    $found = [System.Collections.Generic.List[pscustomobject]]::new()
    $seen = [System.Collections.Generic.HashSet[int]]::new()
    [void]$seen.Add($RootPid)
    $queue = [System.Collections.Generic.Queue[int]]::new()
    $queue.Enqueue($RootPid)
    while ($queue.Count -gt 0) {
        $current = $queue.Dequeue()
        foreach ($p in $all) {
            if ($null -eq $p.ProcessId -or $null -eq $p.ParentProcessId) { continue }
            $childPid = [int]$p.ProcessId
            if ([int]$p.ParentProcessId -eq $current -and -not $seen.Contains($childPid)) {
                [void]$seen.Add($childPid)
                $found.Add([pscustomobject]@{ ProcessId = $childPid; Name = "$($p.Name)" })
                $queue.Enqueue($childPid)
            }
        }
    }
    return , $found
}

# --- (2) timed clean build --------------------------------------------------
$compileOk = 'reused'
$buildSecs = ''
if (-not $SkipBuild) {
    $buildExit = 0
    $elapsed = $null
    Push-Location $AppDir
    try {
        $elapsed = Measure-Command {
            & rustup run 1.96.1 cargo build --release --locked *> $BuildLog
        }
        $buildExit = $LASTEXITCODE
    } finally {
        Pop-Location
    }
    $buildSecs = $elapsed.TotalSeconds.ToString('0.00', [Globalization.CultureInfo]::InvariantCulture)
    if ($buildExit -ne 0) {
        $firstError = ''
        if (Test-Path -LiteralPath $BuildLog) {
            $firstError = @(Get-Content -LiteralPath $BuildLog | Where-Object { $_ -match '^\s*error(\[|:)' }) | Select-Object -First 1
            if (-not $firstError) {
                $firstError = (@(Get-Content -LiteralPath $BuildLog -Tail 5) -join ' ; ')
            }
            Write-Host "--- build log tail ($App) ---"
            Get-Content -LiteralPath $BuildLog -Tail 30 | ForEach-Object { Write-Host $_ }
        }
        Write-Result @{
            app = $App; variant = $Variant; compile_ok = 'no'; run_alive_10s = 'no'; visible_window = 'no'
            clean_build_secs = $buildSecs; binary_bytes = 0; pdb_bytes = 0; exit_code = $buildExit
            env_overrides = $EnvSerialized; notes = (Format-Notes "build failed: $firstError")
        }
        Add-RunLogLine "$App [$Variant] build FAILED (exit $buildExit) after ${buildSecs}s"
        exit 0
    }
    $compileOk = 'yes'
}

# --- (3) locate binary ------------------------------------------------------
# Binary name = package name as-is on Windows (e.g. iced-app.exe); the .pdb
# may use the underscored crate name (iced_app.pdb) -- check both.
$Binary = Join-Path (Join-Path (Join-Path $AppDir 'target') 'release') "$App.exe"
if (-not (Test-Path -LiteralPath $Binary)) {
    Write-Result @{
        app = $App; variant = $Variant; compile_ok = $compileOk; run_alive_10s = 'no'; visible_window = 'no'
        clean_build_secs = $buildSecs; binary_bytes = 0; pdb_bytes = 0; exit_code = ''
        env_overrides = $EnvSerialized; notes = (Format-Notes "binary not found: target/release/$App.exe")
    }
    Add-RunLogLine "$App [$Variant] binary missing: $Binary"
    exit 0
}
$binaryBytes = (Get-Item -LiteralPath $Binary).Length
$pdbBytes = 0
foreach ($pdbName in @("$App.pdb", (($App -replace '-', '_') + '.pdb'))) {
    $pdbPath = Join-Path (Split-Path -Parent $Binary) $pdbName
    if (Test-Path -LiteralPath $pdbPath) {
        $pdbBytes = (Get-Item -LiteralPath $pdbPath).Length
        break
    }
}

# --- (4) launch with env overrides, wait 10 s -------------------------------
$proc = $null
$launchError = ''
$savedEnv = @{}
foreach ($k in @($EnvOverrides.Keys)) {
    $savedEnv[$k] = [Environment]::GetEnvironmentVariable($k)
    [Environment]::SetEnvironmentVariable($k, [string]$EnvOverrides[$k])
}
try {
    # Redirecting stdout/stderr (UseShellExecute=false) preserves env-override
    # propagation and window creation, and keeps panic/missing-DLL output as
    # evidence for died-before-10s findings.
    $proc = Start-Process -FilePath $Binary -WorkingDirectory $AppDir -PassThru `
        -RedirectStandardOutput (Join-Path $ResultDir 'app-stdout.log') `
        -RedirectStandardError (Join-Path $ResultDir 'app-stderr.log')
    try { $null = $proc.Handle } catch { }  # retain handle so ExitCode stays readable
} catch {
    $launchError = $_.Exception.Message
} finally {
    foreach ($k in @($savedEnv.Keys)) {
        [Environment]::SetEnvironmentVariable($k, $savedEnv[$k])
    }
}
if ($null -eq $proc) {
    Write-Result @{
        app = $App; variant = $Variant; compile_ok = $compileOk; run_alive_10s = 'no'; visible_window = 'no'
        clean_build_secs = $buildSecs; binary_bytes = $binaryBytes; pdb_bytes = $pdbBytes; exit_code = ''
        env_overrides = $EnvSerialized; notes = (Format-Notes "launch failed: $launchError")
    }
    Add-RunLogLine "$App [$Variant] launch FAILED: $launchError"
    exit 0
}
$mainPid = [int]$proc.Id
Start-Sleep -Seconds 10

$alive = $false
try { $alive = -not $proc.HasExited } catch { $alive = $false }

# --- (5) visible-window check (main PID + msedgewebview2 descendants) ------
$descendants = Get-ProcessDescendants $mainPid
$helperPids = @($descendants | Where-Object { $_.Name -ieq 'msedgewebview2.exe' } | ForEach-Object { [int]$_.ProcessId })

$windowLines = @()
try {
    $wcParams = @{ TargetPid = $mainPid }
    if ($helperPids.Count -gt 0) { $wcParams['ExtraPids'] = [int[]]$helperPids }
    $windowLines = @(& (Join-Path $PSScriptRoot 'window-count.ps1') @wcParams | ForEach-Object { "$_" } | Where-Object { $_ -ne '' })
} catch {
    $windowLines = @()
}
$windowsTsvContent = if ($windowLines.Count -gt 0) { ($windowLines -join "`n") + "`n" } else { '' }
Write-Utf8NoBom (Join-Path $ResultDir 'windows.tsv') $windowsTsvContent
$visible = $windowLines.Count -gt 0

# --- (6) kill the process tree ---------------------------------------------
$killPids = [System.Collections.Generic.HashSet[int]]::new()
foreach ($d in $descendants) { [void]$killPids.Add([int]$d.ProcessId) }
foreach ($d in (Get-ProcessDescendants $mainPid)) { [void]$killPids.Add([int]$d.ProcessId) }  # catch late children
[void]$killPids.Add($mainPid)
foreach ($kp in $killPids) {
    Stop-Process -Id $kp -Force -ErrorAction SilentlyContinue
}

# --- (7) record -------------------------------------------------------------
$exitCode = 'na'  # still running at 10 s; killed by the harness
if (-not $alive) {
    try { $exitCode = "$($proc.ExitCode)" } catch { $exitCode = 'unknown' }
}
$aliveStr = if ($alive) { 'yes' } else { 'no' }
$visibleStr = if ($visible) { 'yes' } else { 'no' }
$notes = ''
if (-not $alive) {
    $notes = "process exited before 10s (exit=$exitCode)"
} elseif (-not $visible) {
    $notes = 'alive but no visible top-level window'
}
Write-Result @{
    app = $App; variant = $Variant; compile_ok = $compileOk; run_alive_10s = $aliveStr; visible_window = $visibleStr
    clean_build_secs = $buildSecs; binary_bytes = $binaryBytes; pdb_bytes = $pdbBytes; exit_code = $exitCode
    env_overrides = $EnvSerialized; notes = (Format-Notes $notes)
}
$summary = "$App [$Variant] compile=$compileOk alive10s=$aliveStr window=$visibleStr windows=$($windowLines.Count) build=${buildSecs}s bin=$binaryBytes pdb=$pdbBytes exit=$exitCode env=$EnvSerialized"
Add-RunLogLine $summary
Write-Host $summary
exit 0
