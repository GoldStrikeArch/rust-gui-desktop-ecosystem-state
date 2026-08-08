#Requires -Version 7.0
<#
.SYNOPSIS
  Windows port of scripts/runtime-sample.sh — runtime CPU/RSS sampling for the
  ten SPEC-2 dashboard apps (repaint-model comparison).

.DESCRIPTION
  For each of the ten <framework>-dash apps: launch the release binary
  (apps/<app>/target/release/<app>.exe — run-cohort builds first; a missing
  binary is recorded as DIED), wait 5 s warmup, then take EXACTLY 30 samples at
  ~1 s intervals of CPU% and RSS for the main PID plus any msedgewebview2.exe
  helper descendants (re-discovered each sample via a Win32_Process
  ParentProcessId walk, since WebView2 helpers come and go).

  CPU semantics (IMPORTANT — matches the macOS convention):
    macOS `ps -o %cpu` reports per-core percent: a process saturating 4 cores
    reads ~400%. On Windows we read
    Win32_PerfFormattedData_PerfProc_Process.PercentProcessorTime, which is the
    cooked "% Processor Time" process counter: CPU-seconds consumed per wall
    second x 100, summed across cores — i.e. it can exceed 100 on multi-core
    machines and is already the per-core convention. It is NOT divided by the
    logical core count (that division is what Task Manager displays, not what
    this counter reports), so NO multiplication by NUMBER_OF_PROCESSORS is
    applied here. If a run on some machine shows values that are provably
    normalized (pegged processes never exceeding 100/n), the recorded
    logical_cores value in runtime-notes.txt allows a post-hoc correction —
    the semantics actually used are recorded in that sidecar as cpu_semantics.

  Output (refuses to overwrite unless -Replace):
    <Cohort>/windows/runtime.csv          app,avg_cpu_pct,peak_cpu_pct,max_rss_mib,helper_procs
    <Cohort>/windows/runtime-samples.csv  app,sample_index,main_pid,helper_pids,cpu_pct,rss_kib,rss_mib
                                          (30 rows per surviving app, indexes 1..30 —
                                          the exact scripts/cohort.py
                                          validate_runtime_samples contract)
    <Cohort>/windows/runtime-notes.txt    cpu_semantics + environment sidecar
    <Cohort>/windows/runtime-logs/        per-app stdout/stderr

  Mirrors runtime-sample.sh's all-or-nothing rule per app: if fewer than 30
  samples complete (process died mid-window), the app is recorded as DIED in
  the summary and NO partial sample rows are written for it.

.EXAMPLE
  pwsh windows/runtime-sample.ps1 -Cohort measurements/cohorts/2026-08-08-win
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Cohort,
    [switch]$Replace
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $IsWindows) { throw 'runtime-sample.ps1 must run on Windows' }

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Invariant = [System.Globalization.CultureInfo]::InvariantCulture

# Duplicated by convention across the windows/ scripts (each stays standalone):
# every file this harness writes is UTF-8 without BOM, LF line endings.
function Write-Utf8NoBom {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Content
    )
    $normalized = $Content.Replace("`r`n", "`n")
    [System.IO.File]::WriteAllText($Path, $normalized, [System.Text.UTF8Encoding]::new($false))
}

$Frameworks = @('iced', 'egui', 'gpui', 'tauri', 'xilem', 'slint', 'dioxus', 'freya', 'vizia', 'floem')

if (-not [System.IO.Path]::IsPathRooted($Cohort)) {
    $Cohort = Join-Path $RepoRoot $Cohort
}
if (-not (Test-Path -LiteralPath $Cohort -PathType Container)) {
    throw "cohort directory does not exist: $Cohort"
}

$OutDir = Join-Path $Cohort 'windows'
$LogDir = Join-Path $OutDir 'runtime-logs'
$SummaryCsv = Join-Path $OutDir 'runtime.csv'
$SamplesCsv = Join-Path $OutDir 'runtime-samples.csv'
$NotesPath = Join-Path $OutDir 'runtime-notes.txt'

foreach ($existing in @($SummaryCsv, $SamplesCsv)) {
    if (Test-Path -LiteralPath $existing) {
        if ($Replace) {
            Remove-Item -LiteralPath $existing -Force
        }
        else {
            throw "output already exists (use -Replace): $existing"
        }
    }
}
New-Item -ItemType Directory -Force -Path $OutDir, $LogDir | Out-Null

$LogicalCores = [int]$env:NUMBER_OF_PROCESSORS

# All msedgewebview2.exe descendants of $RootPid, via a ParentProcessId walk of
# the current Win32_Process table. Re-run every sample: WebView2 spawns and
# retires helpers (GPU / network / renderer) over the window. Best-effort like
# the macOS WebKit-diff attribution — PID reuse between snapshot and read is
# theoretically possible and accepted.
function Get-WebViewHelperPids {
    param([Parameter(Mandatory)][int]$RootPid)
    $table = Get-CimInstance -ClassName Win32_Process |
        Select-Object -Property ProcessId, ParentProcessId, Name
    $byParent = @{}
    foreach ($row in $table) {
        $parent = [int]$row.ParentProcessId
        if (-not $byParent.ContainsKey($parent)) { $byParent[$parent] = [System.Collections.Generic.List[object]]::new() }
        $byParent[$parent].Add($row)
    }
    $helpers = [System.Collections.Generic.List[int]]::new()
    $queue = [System.Collections.Generic.Queue[int]]::new()
    $queue.Enqueue($RootPid)
    $seen = @{ $RootPid = $true }
    while ($queue.Count -gt 0) {
        $current = $queue.Dequeue()
        if (-not $byParent.ContainsKey($current)) { continue }
        foreach ($child in $byParent[$current]) {
            $childPid = [int]$child.ProcessId
            if ($seen.ContainsKey($childPid)) { continue }
            $seen[$childPid] = $true
            if ([string]$child.Name -ieq 'msedgewebview2.exe') {
                $helpers.Add($childPid)
            }
            $queue.Enqueue($childPid)
        }
    }
    return @($helpers | Sort-Object)
}

# Summed cooked %ProcessorTime for a PID set (per-core convention, see header).
function Get-CpuPercent {
    param([Parameter(Mandatory)][int[]]$Pids)
    $clauses = @($Pids | ForEach-Object { "IDProcess=$_" })
    $filter = $clauses -join ' OR '
    $rows = @(Get-CimInstance -ClassName Win32_PerfFormattedData_PerfProc_Process -Filter $filter -ErrorAction SilentlyContinue)
    $total = 0.0
    foreach ($row in $rows) {
        $total += [double]$row.PercentProcessorTime
    }
    return $total
}

# Summed working set in KiB for whichever PIDs still exist.
function Get-RssKib {
    param([Parameter(Mandatory)][int[]]$Pids)
    [long]$kib = 0
    foreach ($onePid in $Pids) {
        $proc = Get-Process -Id $onePid -ErrorAction SilentlyContinue
        if ($null -ne $proc) {
            $kib += [long]($proc.WorkingSet64 / 1024)
        }
    }
    return $kib
}

function Stop-ProcessTree {
    param([Parameter(Mandatory)][int]$RootPid)
    try {
        & taskkill /T /F /PID $RootPid 2>&1 | Out-Null
    }
    catch {
        # already gone
    }
}

$summaryLines = [System.Collections.Generic.List[string]]::new()
$sampleLines = [System.Collections.Generic.List[string]]::new()
$summaryLines.Add('app,avg_cpu_pct,peak_cpu_pct,max_rss_mib,helper_procs')
$sampleLines.Add('app,sample_index,main_pid,helper_pids,cpu_pct,rss_kib,rss_mib')

$measurementFailed = $false

foreach ($framework in $Frameworks) {
    $app = "$framework-dash"
    $exe = Join-Path $RepoRoot "apps/$app/target/release/$app.exe"
    Write-Host "=== $app ==="

    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
        Write-Host "!! ${app}: no release binary (run-cohort builds first)"
        $summaryLines.Add("$app,DIED,,,")
        $measurementFailed = $true
        continue
    }

    $stdoutLog = Join-Path $LogDir "$app.out.log"
    $stderrLog = Join-Path $LogDir "$app.err.log"
    try {
        $proc = Start-Process -FilePath $exe `
            -WorkingDirectory (Join-Path $RepoRoot "apps/$app") `
            -RedirectStandardOutput $stdoutLog `
            -RedirectStandardError $stderrLog `
            -PassThru
    }
    catch {
        Write-Host "!! ${app}: failed to launch ($($_.Exception.Message))"
        $summaryLines.Add("$app,DIED,,,")
        $measurementFailed = $true
        continue
    }
    $mainPid = $proc.Id

    Start-Sleep -Seconds 5
    if ($proc.HasExited) {
        Write-Host "!! ${app}: died during warmup"
        $summaryLines.Add("$app,DIED,,,")
        $measurementFailed = $true
        continue
    }

    $appRows = [System.Collections.Generic.List[string]]::new()
    [double]$cpuSum = 0.0
    [double]$cpuPeak = 0.0
    [long]$rssMaxKib = 0
    [int]$maxHelperCount = 0
    [int]$completed = 0

    foreach ($index in 1..30) {
        if ($proc.HasExited) { break }

        # Helpers are re-discovered every sample (WebView2 pool churns).
        [int[]]$helperPids = @(Get-WebViewHelperPids -RootPid $mainPid)
        if ($helperPids.Count -gt $maxHelperCount) { $maxHelperCount = $helperPids.Count }
        [int[]]$pidSet = @($mainPid) + $helperPids

        $cpu = Get-CpuPercent -Pids $pidSet
        $rssKib = Get-RssKib -Pids $pidSet
        $rssMib = ($rssKib / 1024.0).ToString('0.000', $Invariant)
        $helperField = ($helperPids | ForEach-Object { [string]$_ }) -join ';'
        $cpuField = $cpu.ToString('0.0', $Invariant)

        $appRows.Add("$app,$index,$mainPid,$helperField,$cpuField,$rssKib,$rssMib")
        $cpuSum += $cpu
        if ($cpu -gt $cpuPeak) { $cpuPeak = $cpu }
        if ($rssKib -gt $rssMaxKib) { $rssMaxKib = $rssKib }
        $completed = $index

        Start-Sleep -Seconds 1
    }

    Stop-ProcessTree -RootPid $mainPid
    Start-Sleep -Seconds 1

    if ($completed -ne 30) {
        # All-or-nothing per app, exactly like runtime-sample.sh: no partial rows.
        Write-Host "!! ${app}: died mid-window after $completed samples"
        $summaryLines.Add("$app,DIED,,,")
        $measurementFailed = $true
        continue
    }

    foreach ($row in $appRows) { $sampleLines.Add($row) }
    $avg = ($cpuSum / 30.0).ToString('0.0', $Invariant)
    $peak = $cpuPeak.ToString('0.0', $Invariant)
    $maxRssMib = [math]::Round($rssMaxKib / 1024.0).ToString('0', $Invariant)
    $summaryLines.Add("$app,$avg,$peak,$maxRssMib,$maxHelperCount")
    Write-Host "    avg_cpu=$avg% peak=$peak% max_rss=${maxRssMib}MiB helpers=$maxHelperCount"
}

Write-Utf8NoBom -Path $SummaryCsv -Content (($summaryLines -join "`n") + "`n")
Write-Utf8NoBom -Path $SamplesCsv -Content (($sampleLines -join "`n") + "`n")

$notes = @(
    "# runtime-sample.ps1 sidecar — $(([DateTime]::UtcNow).ToString('yyyy-MM-ddTHH:mm:ssZ', $Invariant))"
    '# cpu_semantics=per-core-percent; source=Win32_PerfFormattedData_PerfProc_Process.PercentProcessorTime summed over {main_pid + msedgewebview2.exe descendants}; values can exceed 100 on multi-core (same convention as macOS `ps -o %cpu`); NOT divided by logical core count and NOT multiplied by NUMBER_OF_PROCESSORS (the counter is already core-seconds per wall-second x 100)'
    "# logical_cores=$LogicalCores"
    '# rss_semantics=WorkingSet64/1024 KiB summed over the same PID set (Get-Process)'
    '# helper_attribution=msedgewebview2.exe descendants of the main PID via Win32_Process ParentProcessId walk, re-discovered each sample (best-effort, PID reuse accepted)'
    '# cadence=5s warmup, then exactly 30 samples at ~1s intervals (Start-Sleep 1 between samples; query time makes intervals slightly longer)'
)
Write-Utf8NoBom -Path $NotesPath -Content (($notes -join "`n") + "`n")

Write-Host ''
Write-Host "Results: $SummaryCsv"
Write-Host "Raw samples: $SamplesCsv"
Write-Host "Notes: $NotesPath"

if ($measurementFailed) {
    Write-Warning 'one or more apps recorded as DIED; runtime-samples.csv will NOT satisfy validate_runtime_samples for the full app set'
    exit 1
}
