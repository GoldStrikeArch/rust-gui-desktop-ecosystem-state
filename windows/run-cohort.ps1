#Requires -Version 7.0
<#
  windows/run-cohort.ps1 -- build and probe the 80-app Windows cohort,
  strictly serially (Windows mirror of linux/run-cohort.sh).

  Usage:
    pwsh -NoProfile -File windows/run-cohort.ps1 -Cohort <dir>
      [-Only '*-peek','iced-*'] [-Variant default|msmf-manifest]
      [-SkipEnvironment] [-Replace]

  Policy (mirrors the Linux harness):
    - The cohort dir must already contain cohort.json (scripts/cohort.py init).
    - Apps and variants run strictly serially.
    - Default-renderer results are retained even when a workaround is probed;
      every retry lands in its own runs/<app>/<variant>/ directory.
    - A recorded failure is a finding; only a probe that produced NO
      result.tsv counts as an infrastructure failure and blocks aggregation.
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Cohort,
    [string[]]$Only = @(),
    [string]$Variant = 'default',
    [switch]$SkipEnvironment,
    [switch]$Replace
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-Utf8NoBom([string]$Path,[string]$Content) { [IO.File]::WriteAllText($Path, $Content.Replace("`r`n","`n"), [Text.UTF8Encoding]::new($false)) }

function Abort([string]$Message, [int]$Code = 2) {
    [Console]::Error.WriteLine($Message)
    exit $Code
}

$RepoRoot = Split-Path -Parent $PSScriptRoot
if (-not [IO.Path]::IsPathRooted($Cohort)) { $Cohort = Join-Path $RepoRoot $Cohort }

# --- (1) cohort must be initialized ----------------------------------------
if (-not (Test-Path -LiteralPath (Join-Path $Cohort 'cohort.json'))) {
    Abort "no cohort.json in $Cohort -- run scripts/cohort.py init on the Mac first"
}
$WinDir = Join-Path $Cohort 'windows'
$Csv = Join-Path $WinDir 'results.csv'

# --- (3) app list, -Only filter, preflight assert --------------------------
$Frameworks = @('iced', 'egui', 'gpui', 'tauri', 'xilem', 'slint', 'dioxus', 'freya', 'vizia', 'floem')
$Suites = @('app', 'dash', 'board', 'tray', 'babel', 'grid', 'fetch', 'peek')
$AllApps = @(foreach ($f in $Frameworks) { foreach ($s in $Suites) { "$f-$s" } })

$Selected = $AllApps
if ($Only.Count -gt 0) {
    $Selected = @($AllApps | Where-Object {
        $name = $_
        @($Only | Where-Object { $name -like $_ }).Count -gt 0
    })
}
if ($Selected.Count -eq 0) { Abort "no apps match -Only patterns: $($Only -join ', ')" }
foreach ($app in $Selected) {
    if (-not (Test-Path -LiteralPath (Join-Path (Join-Path (Join-Path $RepoRoot 'apps') $app) 'Cargo.toml'))) {
        Abort "missing cohort app: apps/$app (no Cargo.toml)"
    }
}

# --- (4) refuse to clobber an existing aggregate ---------------------------
if ((Test-Path -LiteralPath $Csv) -and -not $Replace) {
    Abort "existing $Csv; use a new cohort or -Replace"
}
New-Item -ItemType Directory -Force -Path $WinDir | Out-Null

# --- (2) environment capture ------------------------------------------------
if (-not $SkipEnvironment) {
    & (Join-Path $PSScriptRoot 'capture-environment.ps1') -Cohort $Cohort
    if ($LASTEXITCODE -ne 0) { Abort 'environment capture failed' 1 }
}

# --- (5) strictly serial probe loop ----------------------------------------
$RunAppScript = Join-Path $PSScriptRoot 'run-app.ps1'
$WgpuLadderFrameworks = @('egui', 'xilem', 'floem')  # iced gets tiny-skia instead

function Invoke-Probe([string]$App, [string]$ProbeVariant, [hashtable]$ProbeEnv, [bool]$ProbeSkipBuild) {
    Write-Host "=== windows $App [$ProbeVariant] ==="
    $params = @{ App = $App; Variant = $ProbeVariant; OutputRoot = $script:WinDir }
    if ($ProbeEnv.Count -gt 0) { $params['EnvOverrides'] = $ProbeEnv }
    if ($ProbeSkipBuild) { $params['SkipBuild'] = $true }
    try {
        & $script:RunAppScript @params
    } catch {
        [Console]::Error.WriteLine("probe error for $App [$ProbeVariant]: $($_.Exception.Message)")
    }
}

function Read-ResultField([string]$App, [string]$ProbeVariant, [string]$Field) {
    $path = Join-Path $script:WinDir "runs/$App/$ProbeVariant/result.tsv"
    if (-not (Test-Path -LiteralPath $path)) { return $null }
    $rows = @(Get-Content -LiteralPath $path)
    if ($rows.Count -lt 2) { return $null }
    $header = $rows[0] -split "`t"
    $values = $rows[1] -split "`t"
    $idx = [array]::IndexOf($header, $Field)
    if ($idx -lt 0 -or $idx -ge $values.Count) { return $null }
    return $values[$idx]
}

function Test-ProbeHealthy([string]$App, [string]$ProbeVariant) {
    $alive = Read-ResultField $App $ProbeVariant 'run_alive_10s'
    $visible = Read-ResultField $App $ProbeVariant 'visible_window'
    return ($alive -eq 'yes') -and ($visible -eq 'yes')
}

$infraFailures = [System.Collections.Generic.List[string]]::new()
foreach ($app in $Selected) {
    Invoke-Probe $app $Variant @{} $false
    $primaryResult = Join-Path $WinDir "runs/$app/$Variant/result.tsv"
    if (-not (Test-Path -LiteralPath $primaryResult)) {
        [Console]::Error.WriteLine("missing result from $app [$Variant] probe")
        $infraFailures.Add($app)
        continue
    }
    if ($Variant -ne 'default') { continue }  # e.g. msmf-manifest: no workaround ladder
    if (Test-ProbeHealthy $app 'default') { continue }
    # A build failure gets no render-backend ladder: the retries could only
    # report "binary not found" and would later shadow real variant results.
    if ((Read-ResultField $app 'default' 'compile_ok') -ne 'yes') { continue }

    # Workaround ladder: only after an unhealthy default probe; every retry is
    # its own variant directory and the default result is never overwritten.
    $framework = ($app -split '-')[0]
    if ($framework -eq 'iced') {
        Invoke-Probe $app 'tiny-skia' @{ ICED_BACKEND = 'tiny-skia' } $true
    } elseif ($WgpuLadderFrameworks -contains $framework) {
        Invoke-Probe $app 'wgpu-dx12' @{ WGPU_BACKEND = 'dx12' } $true
        if (-not (Test-ProbeHealthy $app 'wgpu-dx12')) {
            Invoke-Probe $app 'wgpu-gl' @{ WGPU_BACKEND = 'gl' } $true
        }
    }
}

# --- (6) infrastructure failures block aggregation -------------------------
if ($infraFailures.Count -gt 0) {
    [Console]::Error.WriteLine("$($infraFailures.Count) app probe(s) produced no result; aggregate not published:")
    foreach ($f in $infraFailures) { [Console]::Error.WriteLine("  $f") }
    exit 1
}

# --- (7) aggregate ----------------------------------------------------------
$python = $null
foreach ($cand in @('python3', 'python')) {
    if (-not (Get-Command $cand -ErrorAction SilentlyContinue)) { continue }
    # The Microsoft Store app-execution alias resolves but exits 9009 with no
    # version -- accept only a candidate that actually reports Python >= 3.11.
    $verOut = ''
    try { $verOut = (@(& $cand --version 2>&1 | ForEach-Object { "$_" }) -join ' ') } catch { continue }
    if ($verOut -match 'Python\s+3\.(1[1-9]|[2-9]\d)') { $python = $cand; break }
}
if ($null -eq $python) { Abort 'python3/python not found; cannot aggregate' 1 }
$aggArgs = @(
    (Join-Path $PSScriptRoot 'aggregate-results.py'),
    '--runs', (Join-Path $WinDir 'runs'),
    '--output', $Csv
)
foreach ($pattern in $Only) { $aggArgs += @('--only', $pattern) }
& $python @aggArgs
if ($LASTEXITCODE -ne 0) { Abort "aggregation failed (exit $LASTEXITCODE)" 1 }

# --- (8) failures-as-findings summary --------------------------------------
$rows = @(Import-Csv -LiteralPath $Csv)
$findings = @($rows | Where-Object { $_.default_result -ne 'alive+window' })
Write-Host ''
Write-Host ("wrote {0} ({1} apps)" -f $Csv, $rows.Count)
if ($findings.Count -eq 0) {
    Write-Host 'no failures: every aggregated app is alive with a visible window on the default path'
} else {
    Write-Host ("failures-as-findings ({0} of {1} apps):" -f $findings.Count, $rows.Count)
    $findings |
        Select-Object app, default_result, workaround_variant, workaround_result, notes |
        Format-Table -AutoSize | Out-String -Width 300 | Write-Host
}
exit 0
