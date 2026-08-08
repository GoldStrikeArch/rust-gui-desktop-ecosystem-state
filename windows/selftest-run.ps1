#Requires -Version 7.0
<#
.SYNOPSIS
  Serial selftest sweep for the Windows cohort arm: grid / fetch / babel /
  tray / peek suites across the ten frameworks.

.DESCRIPTION
  Runs each app's built-in scripted selftest (the same opt-in env hooks used on
  macOS) strictly serially, captures full stdout+stderr per app, and records
  one summary row per app. Failures are findings, never aborts: an app that
  crashes, times out, or prints no marker still gets a row and the sweep
  continues.

  Suites and their contracts:
    grid   <fw>-grid   GRID_SELFTEST=1, wait <=120 s, expect marker
                       'SELFTEST DONE pass=14 fail=0' on stdout/stderr.
    fetch  <fw>-fetch  builds tools/fetcher-server first (the committed
                       target/ binary is Mach-O — must rebuild for Windows),
                       starts it with FETCHER_PORT=7878, health-polls, then
                       FETCH_SELFTEST=1 FETCHER_PORT=7878 per app, expect
                       'SELFTEST DONE pass=10 fail=0'. STRICTLY SERIAL: the
                       server's /flaky counter is process-global (500,500,200
                       per 3 calls), so two clients probing in parallel would
                       consume steps from each other's cycle and corrupt both
                       retry probes. GET /flaky/reset is issued before each app
                       so every probe starts at the first 500.
    babel  <fw>-babel  BABEL_SELFTEST=1 with BABEL_SHOT=<png> (this produces
                       the DirectWrite-evidence screenshots), wait <=60 s;
                       pass = exit 0 AND the png exists AND is >10 KiB.
    tray   <fw>-tray   TRAY_SELFTEST=1 with TRAY_SELFTEST_SHOT=<png>,
                       wait <=60 s; pass = exit 0; SELFTEST evidence lines are
                       copied into notes.
    peek   <fw>-peek   PEEK_SELFTEST=1, wait <=90 s; pass = exit 0. gpui-peek
                       is expected to fail (no Windows camera path in that
                       app) — the result is still recorded honestly.

  Output (refuses to overwrite unless -Replace):
    <Cohort>/windows/selftests/<app>.log     full stdout+stderr per app
    <Cohort>/windows/selftests/results.csv   app,suite,expected,observed,status,notes
    <Cohort>/windows/babel-shots/<app>.png   babel screenshots
    <Cohort>/windows/tray-shots/<app>.png    tray screenshots

.EXAMPLE
  pwsh windows/selftest-run.ps1 -Cohort measurements/cohorts/2026-08-08-win -Suites grid,tray
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Cohort,
    [ValidateSet('grid', 'fetch', 'babel', 'tray', 'peek')]
    [string[]]$Suites = @('grid', 'fetch', 'babel', 'tray', 'peek'),
    [switch]$Replace
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $IsWindows) { throw 'selftest-run.ps1 must run on Windows' }

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Frameworks = @('iced', 'egui', 'gpui', 'tauri', 'xilem', 'slint', 'dioxus', 'freya', 'vizia', 'floem')
$FetcherPort = 7878
$FetcherBase = "http://127.0.0.1:$FetcherPort"

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

if (-not [System.IO.Path]::IsPathRooted($Cohort)) {
    $Cohort = Join-Path $RepoRoot $Cohort
}
if (-not (Test-Path -LiteralPath $Cohort -PathType Container)) {
    throw "cohort directory does not exist: $Cohort"
}

$SelftestDir = Join-Path $Cohort 'windows/selftests'
$ResultsCsv = Join-Path $SelftestDir 'results.csv'
$BabelShotDir = Join-Path $Cohort 'windows/babel-shots'
$TrayShotDir = Join-Path $Cohort 'windows/tray-shots'

if (Test-Path -LiteralPath $ResultsCsv) {
    if ($Replace) {
        Remove-Item -LiteralPath $ResultsCsv -Force
    }
    else {
        throw "output already exists (use -Replace): $ResultsCsv"
    }
}
New-Item -ItemType Directory -Force -Path $SelftestDir | Out-Null

$ResultLines = [System.Collections.Generic.List[string]]::new()
$ResultLines.Add('app,suite,expected,observed,status,notes')

function ConvertTo-CsvField {
    param([AllowNull()][AllowEmptyString()][string]$Value)
    if ($null -eq $Value) { return '' }
    if ($Value -match '[",\r\n]') {
        return '"' + $Value.Replace('"', '""') + '"'
    }
    return $Value
}

function Limit-Note {
    param([AllowNull()][AllowEmptyString()][string]$Text, [int]$Max = 400)
    if ($null -eq $Text) { return '' }
    $flat = ($Text -replace '\r?\n', ' | ').Trim()
    if ($flat.Length -gt $Max) { return $flat.Substring(0, $Max) + '...' }
    return $flat
}

function Add-Result {
    param(
        [Parameter(Mandatory)][string]$App,
        [Parameter(Mandatory)][string]$Suite,
        [AllowEmptyString()][string]$Expected,
        [AllowEmptyString()][string]$Observed,
        [Parameter(Mandatory)][string]$Status,
        [AllowEmptyString()][string]$Notes
    )
    $fields = @($App, $Suite, $Expected, $Observed, $Status, (Limit-Note $Notes)) |
        ForEach-Object { ConvertTo-CsvField $_ }
    $ResultLines.Add($fields -join ',')
    # Rewrite progressively so a hard crash mid-sweep still leaves evidence.
    Write-Utf8NoBom -Path $ResultsCsv -Content (($ResultLines -join "`n") + "`n")
    Write-Host ("[{0}/{1}] {2}  {3}" -f $Suite, $App, $Status, (Limit-Note $Notes 120))
}

# Launch a process with extra env vars, capture stdout+stderr, enforce a
# timeout (kill the whole tree on expiry). Returns TimedOut/ExitCode/Output.
function Invoke-CapturedProcess {
    param(
        [Parameter(Mandatory)][string]$FilePath,
        [string[]]$ArgumentList = @(),
        [Parameter(Mandatory)][string]$WorkingDirectory,
        [hashtable]$ExtraEnv = @{},
        [Parameter(Mandatory)][int]$TimeoutSec
    )
    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $FilePath
    foreach ($arg in $ArgumentList) { $psi.ArgumentList.Add($arg) }
    $psi.WorkingDirectory = $WorkingDirectory
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    foreach ($key in $ExtraEnv.Keys) { $psi.Environment[$key] = [string]$ExtraEnv[$key] }
    $proc = [System.Diagnostics.Process]::Start($psi)
    $outTask = $proc.StandardOutput.ReadToEndAsync()
    $errTask = $proc.StandardError.ReadToEndAsync()
    $timedOut = $false
    if ($proc.WaitForExit($TimeoutSec * 1000)) {
        $proc.WaitForExit()  # second wait flushes the async output readers
    }
    else {
        $timedOut = $true
        try { & taskkill /T /F /PID $proc.Id 2>&1 | Out-Null } catch { $null = $_ }
        try { $null = $proc.WaitForExit(15000) } catch { $null = $_ }
    }
    $stdout = $outTask.GetAwaiter().GetResult()
    $stderr = $errTask.GetAwaiter().GetResult()
    $combined = $stdout
    if ($stderr) { $combined = $combined + "`n--- stderr ---`n" + $stderr }
    $exitCode = if ($timedOut) { $null } else { $proc.ExitCode }
    return [pscustomobject]@{
        TimedOut = $timedOut
        ExitCode = $exitCode
        Output   = $combined
    }
}

function Get-MarkerLine {
    param([AllowNull()][AllowEmptyString()][string]$Text)
    if ($null -eq $Text) { return '' }
    foreach ($line in ($Text -split "`r?`n")) {
        if ($line -match 'SELFTEST DONE') { return $line.Trim() }
    }
    return ''
}

function Get-SelftestLines {
    param([AllowNull()][AllowEmptyString()][string]$Text)
    if ($null -eq $Text) { return '' }
    $hits = foreach ($line in ($Text -split "`r?`n")) {
        if ($line -match 'SELFTEST') { $line.Trim() }
    }
    return (@($hits) -join '; ')
}

function Get-AppExe {
    param([Parameter(Mandatory)][string]$App)
    return Join-Path $RepoRoot "apps/$App/target/release/$App.exe"
}

# Run one marker-style app (grid/fetch): expects a SELFTEST DONE line.
function Invoke-MarkerApp {
    param(
        [Parameter(Mandatory)][string]$App,
        [Parameter(Mandatory)][string]$Suite,
        [Parameter(Mandatory)][string]$Expected,
        [Parameter(Mandatory)][hashtable]$ExtraEnv,
        [Parameter(Mandatory)][int]$TimeoutSec
    )
    $exe = Get-AppExe -App $App
    if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
        Add-Result -App $App -Suite $Suite -Expected $Expected -Observed '' -Status 'no-binary' -Notes 'release exe missing (run-cohort builds first)'
        return
    }
    $result = Invoke-CapturedProcess -FilePath $exe -WorkingDirectory (Join-Path $RepoRoot "apps/$App") -ExtraEnv $ExtraEnv -TimeoutSec $TimeoutSec
    Write-Utf8NoBom -Path (Join-Path $SelftestDir "$App.log") -Content $result.Output
    $marker = Get-MarkerLine $result.Output
    if ($result.TimedOut) {
        Add-Result -App $App -Suite $Suite -Expected $Expected -Observed $marker -Status 'timeout' -Notes "no exit within ${TimeoutSec}s; tree killed"
    }
    elseif ($marker -eq $Expected) {
        Add-Result -App $App -Suite $Suite -Expected $Expected -Observed $marker -Status 'pass' -Notes "exit=$($result.ExitCode)"
    }
    elseif ($marker) {
        Add-Result -App $App -Suite $Suite -Expected $Expected -Observed $marker -Status 'fail' -Notes "exit=$($result.ExitCode)"
    }
    else {
        Add-Result -App $App -Suite $Suite -Expected $Expected -Observed '' -Status 'no-marker' -Notes "exit=$($result.ExitCode)"
    }
}

# ---------------------------------------------------------------- grid suite
function Invoke-GridSuite {
    $expected = 'SELFTEST DONE pass=14 fail=0'
    foreach ($framework in $Frameworks) {
        try {
            Invoke-MarkerApp -App "$framework-grid" -Suite 'grid' -Expected $expected `
                -ExtraEnv @{ GRID_SELFTEST = '1' } -TimeoutSec 120
        }
        catch {
            Add-Result -App "$framework-grid" -Suite 'grid' -Expected $expected -Observed '' -Status 'error' -Notes $_.Exception.Message
        }
    }
}

# --------------------------------------------------------------- fetch suite
function Invoke-FetchSuite {
    $expected = 'SELFTEST DONE pass=10 fail=0'
    $serverDir = Join-Path $RepoRoot 'tools/fetcher-server'
    $serverExe = Join-Path $serverDir 'target/release/fetcher-server.exe'

    # The committed tools/fetcher-server/target/ binary is a Mach-O executable
    # from the macOS runs — it must be rebuilt for Windows first.
    $build = Invoke-CapturedProcess -FilePath 'rustup' `
        -ArgumentList @('run', '1.96.1', 'cargo', 'build', '--release', '--locked') `
        -WorkingDirectory $serverDir -TimeoutSec 900
    Write-Utf8NoBom -Path (Join-Path $SelftestDir 'fetcher-server-build.log') -Content $build.Output
    if ($build.TimedOut -or $build.ExitCode -ne 0 -or -not (Test-Path -LiteralPath $serverExe -PathType Leaf)) {
        Add-Result -App 'fetcher-server' -Suite 'fetch' -Expected 'cargo build --release exit 0' -Observed "exit=$($build.ExitCode)" -Status 'error' -Notes 'fetcher-server build failed; see fetcher-server-build.log'
        foreach ($framework in $Frameworks) {
            Add-Result -App "$framework-fetch" -Suite 'fetch' -Expected $expected -Observed '' -Status 'skipped' -Notes 'fetcher-server unavailable'
        }
        return
    }

    $psi = [System.Diagnostics.ProcessStartInfo]::new()
    $psi.FileName = $serverExe
    $psi.WorkingDirectory = $serverDir
    $psi.UseShellExecute = $false
    $psi.RedirectStandardOutput = $true
    $psi.RedirectStandardError = $true
    $psi.Environment['FETCHER_PORT'] = [string]$FetcherPort
    $serverProc = [System.Diagnostics.Process]::Start($psi)
    $serverOutTask = $serverProc.StandardOutput.ReadToEndAsync()
    $serverErrTask = $serverProc.StandardError.ReadToEndAsync()

    try {
        $healthy = $false
        foreach ($attempt in 1..30) {
            try {
                $response = Invoke-WebRequest -Uri "$FetcherBase/health" -UseBasicParsing -TimeoutSec 2
                if ($response.StatusCode -eq 200) { $healthy = $true; break }
            }
            catch { }
            # Sleep on non-200 answers too, not only on exceptions -- otherwise a
            # slow-starting server burns all 30 attempts in under a second.
            Start-Sleep -Seconds 1
        }
        if (-not $healthy) {
            Add-Result -App 'fetcher-server' -Suite 'fetch' -Expected '/health -> 200 ok' -Observed '' -Status 'error' -Notes 'server never became healthy on port 7878'
            foreach ($framework in $Frameworks) {
                Add-Result -App "$framework-fetch" -Suite 'fetch' -Expected $expected -Observed '' -Status 'skipped' -Notes 'fetcher-server unavailable'
            }
            return
        }

        # STRICTLY SERIAL by design: /flaky keeps ONE process-global counter
        # (500, 500, 200 per three calls). Two apps probing concurrently would
        # each consume steps of the other's cycle and both retry probes would
        # report garbage. Each app also gets a /flaky/reset so its probe
        # deterministically starts at the first 500 response.
        foreach ($framework in $Frameworks) {
            $app = "$framework-fetch"
            try {
                try {
                    $null = Invoke-WebRequest -Uri "$FetcherBase/flaky/reset" -UseBasicParsing -TimeoutSec 5
                }
                catch {
                    $null = $_  # older server without /flaky/reset: probe still runs, just not cycle-aligned
                }
                Invoke-MarkerApp -App $app -Suite 'fetch' -Expected $expected `
                    -ExtraEnv @{ FETCH_SELFTEST = '1'; FETCHER_PORT = [string]$FetcherPort } -TimeoutSec 120
            }
            catch {
                Add-Result -App $app -Suite 'fetch' -Expected $expected -Observed '' -Status 'error' -Notes $_.Exception.Message
            }
        }
    }
    finally {
        try { & taskkill /T /F /PID $serverProc.Id 2>&1 | Out-Null } catch { $null = $_ }
        try { $null = $serverProc.WaitForExit(15000) } catch { $null = $_ }
        $serverLog = $serverOutTask.GetAwaiter().GetResult()
        $serverErrText = $serverErrTask.GetAwaiter().GetResult()
        if ($serverErrText) { $serverLog = $serverLog + "`n--- stderr ---`n" + $serverErrText }
        Write-Utf8NoBom -Path (Join-Path $SelftestDir 'fetcher-server.log') -Content $serverLog
    }
}

# --------------------------------------------------------------- babel suite
function Invoke-BabelSuite {
    New-Item -ItemType Directory -Force -Path $BabelShotDir | Out-Null
    $expected = 'exit=0 png>10240B'
    foreach ($framework in $Frameworks) {
        $app = "$framework-babel"
        try {
            $exe = Get-AppExe -App $app
            if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
                Add-Result -App $app -Suite 'babel' -Expected $expected -Observed '' -Status 'no-binary' -Notes 'release exe missing (run-cohort builds first)'
                continue
            }
            $shot = Join-Path $BabelShotDir "$app.png"
            if (Test-Path -LiteralPath $shot) { Remove-Item -LiteralPath $shot -Force }
            $result = Invoke-CapturedProcess -FilePath $exe -WorkingDirectory (Join-Path $RepoRoot "apps/$app") `
                -ExtraEnv @{ BABEL_SELFTEST = '1'; BABEL_SHOT = $shot } -TimeoutSec 60
            Write-Utf8NoBom -Path (Join-Path $SelftestDir "$app.log") -Content $result.Output
            [long]$pngBytes = 0
            if (Test-Path -LiteralPath $shot -PathType Leaf) {
                $pngBytes = (Get-Item -LiteralPath $shot).Length
            }
            $observed = "exit=$($result.ExitCode) png_bytes=$pngBytes"
            if ($result.TimedOut) {
                Add-Result -App $app -Suite 'babel' -Expected $expected -Observed $observed -Status 'timeout' -Notes "no exit within 60s; png_bytes=$pngBytes"
            }
            elseif ($result.ExitCode -eq 0 -and $pngBytes -gt 10240) {
                Add-Result -App $app -Suite 'babel' -Expected $expected -Observed $observed -Status 'pass' -Notes "png_bytes=$pngBytes ($shot)"
            }
            else {
                Add-Result -App $app -Suite 'babel' -Expected $expected -Observed $observed -Status 'fail' -Notes "png_bytes=$pngBytes"
            }
        }
        catch {
            Add-Result -App $app -Suite 'babel' -Expected $expected -Observed '' -Status 'error' -Notes $_.Exception.Message
        }
    }
}

# ---------------------------------------------------------------- tray suite
function Invoke-TraySuite {
    New-Item -ItemType Directory -Force -Path $TrayShotDir | Out-Null
    $expected = 'exit=0'
    foreach ($framework in $Frameworks) {
        $app = "$framework-tray"
        try {
            $exe = Get-AppExe -App $app
            if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
                Add-Result -App $app -Suite 'tray' -Expected $expected -Observed '' -Status 'no-binary' -Notes 'release exe missing (run-cohort builds first)'
                continue
            }
            $shot = Join-Path $TrayShotDir "$app.png"
            if (Test-Path -LiteralPath $shot) { Remove-Item -LiteralPath $shot -Force }
            $result = Invoke-CapturedProcess -FilePath $exe -WorkingDirectory (Join-Path $RepoRoot "apps/$app") `
                -ExtraEnv @{ TRAY_SELFTEST = '1'; TRAY_SELFTEST_SHOT = $shot } -TimeoutSec 60
            Write-Utf8NoBom -Path (Join-Path $SelftestDir "$app.log") -Content $result.Output
            $evidence = Get-SelftestLines $result.Output
            $observed = "exit=$($result.ExitCode)"
            if ($result.TimedOut) {
                Add-Result -App $app -Suite 'tray' -Expected $expected -Observed $observed -Status 'timeout' -Notes "no exit within 60s; $evidence"
            }
            elseif ($result.ExitCode -eq 0) {
                Add-Result -App $app -Suite 'tray' -Expected $expected -Observed $observed -Status 'pass' -Notes $evidence
            }
            else {
                Add-Result -App $app -Suite 'tray' -Expected $expected -Observed $observed -Status 'fail' -Notes $evidence
            }
        }
        catch {
            Add-Result -App $app -Suite 'tray' -Expected $expected -Observed '' -Status 'error' -Notes $_.Exception.Message
        }
    }
}

# ---------------------------------------------------------------- peek suite
function Invoke-PeekSuite {
    $expected = 'exit=0'
    foreach ($framework in $Frameworks) {
        $app = "$framework-peek"
        try {
            $exe = Get-AppExe -App $app
            if (-not (Test-Path -LiteralPath $exe -PathType Leaf)) {
                Add-Result -App $app -Suite 'peek' -Expected $expected -Observed '' -Status 'no-binary' -Notes 'release exe missing (run-cohort builds first)'
                continue
            }
            $result = Invoke-CapturedProcess -FilePath $exe -WorkingDirectory (Join-Path $RepoRoot "apps/$app") `
                -ExtraEnv @{ PEEK_SELFTEST = '1' } -TimeoutSec 90
            Write-Utf8NoBom -Path (Join-Path $SelftestDir "$app.log") -Content $result.Output
            $evidence = Get-SelftestLines $result.Output
            $observed = "exit=$($result.ExitCode)"
            # gpui-peek has no Windows camera path — a failure here is the
            # expected outcome, recorded honestly rather than special-cased away.
            $known = if ($app -eq 'gpui-peek') { ' [known: gpui-peek has no Windows camera path — failure expected]' } else { '' }
            if ($result.TimedOut) {
                Add-Result -App $app -Suite 'peek' -Expected $expected -Observed $observed -Status 'timeout' -Notes ("no exit within 90s; $evidence" + $known)
            }
            elseif ($result.ExitCode -eq 0) {
                Add-Result -App $app -Suite 'peek' -Expected $expected -Observed $observed -Status 'pass' -Notes $evidence
            }
            else {
                Add-Result -App $app -Suite 'peek' -Expected $expected -Observed $observed -Status 'fail' -Notes ($evidence + $known)
            }
        }
        catch {
            Add-Result -App $app -Suite 'peek' -Expected $expected -Observed '' -Status 'error' -Notes $_.Exception.Message
        }
    }
}

# ------------------------------------------------------------------- driver
foreach ($suite in $Suites) {
    Write-Host "=== suite: $suite ==="
    switch ($suite) {
        'grid' { Invoke-GridSuite }
        'fetch' { Invoke-FetchSuite }
        'babel' { Invoke-BabelSuite }
        'tray' { Invoke-TraySuite }
        'peek' { Invoke-PeekSuite }
    }
}

Write-Host ''
Write-Host "Results: $ResultsCsv"
Write-Host "Logs:    $SelftestDir"
