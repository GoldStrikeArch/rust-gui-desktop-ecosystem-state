#Requires -Version 7.0
<#
.SYNOPSIS
  Windows packaging head-to-head for the ten <framework>-app todo apps:
  cargo-bundle (msi) vs cargo-packager (nsis + wix) vs tauri-cli (msi + nsis,
  tauri-app only) vs dx bundle (msi, dioxus-app only).

.DESCRIPTION
  Mirrors scripts/package-macos.sh for the Windows arm. Strictly serial; every
  per-row step is wrapped in try/catch and every external process is bounded by
  a timeout, so one failure never kills the sweep — failed rows are findings,
  not aborts.

  Row inventory when all four tools run (33 rows):
    cargo-bundle   msi   x10   rustup run 1.96.1 cargo bundle --release --format msi
                               (artifact under target/release/bundle/msi/)
    cargo-packager nsis  x10   cargo-packager --release --formats nsis
    cargo-packager wix   x10   cargo-packager --release --formats wix
                               (cargo-packager does NOT build; the release exe
                               is built first if missing. If neither
                               Packager.toml nor [package.metadata.packager]
                               exists, a minimal Packager.toml is created on
                               the fly — the 9-LOC shape from
                               report/data/packaging-results.md, cargo-packager
                               head-to-head — and noted as
                               config_created=packager.toml)
    tauri          msi+nsis x2 rustup run 1.96.1 cargo tauri build --ci --no-sign --bundles msi,nsis
                               (one build, two artifact rows; apps/tauri-app)
    dx             msi   x1    dx bundle --release in apps/dioxus-app
                               (Dioxus.toml with [bundle] identifier/publisher/
                               icon already exists in the repo; dx builds into
                               its own target/dx/<app>/bundle/ profile)

  Per row: build installer -> artifact_bytes -> silent install
  (msi: msiexec /i <artifact> /qn /norestart /l*v <log>, exit 0 or 3010 = ok;
  nsis: <artifact> /S) -> launch_after_install (find the installed exe under
  ProgramFiles / ProgramFiles(x86) / LOCALAPPDATA\Programs, 8 s alive check)
  -> uninstall (msi: msiexec /x /qn; nsis: installed dir's uninstaller /S,
  best-effort) -> signed = Get-AuthenticodeSignature status verbatim (expected
  NotSigned: no signing identity is configured in this research, by design).

  status=passed means the tool produced an installer artifact; install/launch/
  uninstall outcomes live in their own columns. On failure the first error
  line lands in notes.

  Output (refuses to overwrite unless -Replace):
    <Cohort>/windows/packaging/results.csv
      app,tool,format,status,artifact_bytes,install_ok,launch_after_install,uninstall_ok,signed,notes
    <Cohort>/windows/packaging/logs/   full per-row command transcripts

.EXAMPLE
  pwsh windows/package-windows.ps1 -Cohort measurements/cohorts/2026-08-08-win -Tools cargo-bundle,dx
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Cohort,
    [ValidateSet('cargo-bundle', 'cargo-packager', 'tauri', 'dx')]
    [string[]]$Tools = @('cargo-bundle', 'cargo-packager', 'tauri', 'dx'),
    [switch]$Replace
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

if (-not $IsWindows) { throw 'package-windows.ps1 must run on Windows' }

$RepoRoot = Split-Path -Parent $PSScriptRoot
$Frameworks = @('iced', 'egui', 'gpui', 'tauri', 'xilem', 'slint', 'dioxus', 'freya', 'vizia', 'floem')
$Apps = @($Frameworks | ForEach-Object { "$_-app" })

# Product names mirror scripts/package-macos.sh (bundle display names).
$ProductNames = @{
    'iced-app'   = 'Iced Tasks'
    'egui-app'   = 'Egui Tasks'
    'gpui-app'   = 'Gpui Tasks'
    'tauri-app'  = 'tauri-app'
    'xilem-app'  = 'Xilem Tasks'
    'slint-app'  = 'Slint Tasks'
    'dioxus-app' = 'Dioxus Tasks'
    'freya-app'  = 'Freya Tasks'
    'vizia-app'  = 'Vizia Tasks'
    'floem-app'  = 'Floem Tasks'
}

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

function Add-LogText {
    param(
        [Parameter(Mandatory)][string]$Path,
        [Parameter(Mandatory)][AllowEmptyString()][string]$Text
    )
    $normalized = $Text.Replace("`r`n", "`n")
    [System.IO.File]::AppendAllText($Path, $normalized, [System.Text.UTF8Encoding]::new($false))
}

if (-not [System.IO.Path]::IsPathRooted($Cohort)) {
    $Cohort = Join-Path $RepoRoot $Cohort
}
if (-not (Test-Path -LiteralPath $Cohort -PathType Container)) {
    throw "cohort directory does not exist: $Cohort"
}

$PackDir = Join-Path $Cohort 'windows/packaging'
$LogDir = Join-Path $PackDir 'logs'
$ResultsCsv = Join-Path $PackDir 'results.csv'

if ((Test-Path -LiteralPath $ResultsCsv) -or (Test-Path -LiteralPath $LogDir)) {
    if ($Replace) {
        if (Test-Path -LiteralPath $ResultsCsv) { Remove-Item -LiteralPath $ResultsCsv -Force }
        if (Test-Path -LiteralPath $LogDir) { Remove-Item -LiteralPath $LogDir -Recurse -Force }
    }
    else {
        throw "packaging output already exists (use -Replace): $PackDir"
    }
}
New-Item -ItemType Directory -Force -Path $PackDir, $LogDir | Out-Null

# ------------------------------------------------------------- CSV plumbing
$RowLines = [System.Collections.Generic.List[string]]::new()
$RowLines.Add('app,tool,format,status,artifact_bytes,install_ok,launch_after_install,uninstall_ok,signed,notes')
Write-Utf8NoBom -Path $ResultsCsv -Content (($RowLines -join "`n") + "`n")

function ConvertTo-CsvField {
    param([AllowNull()][AllowEmptyString()][string]$Value)
    if ($null -eq $Value) { return '' }
    if ($Value -match '[",\r\n]') {
        return '"' + $Value.Replace('"', '""') + '"'
    }
    return $Value
}

function Limit-Note {
    param([AllowNull()][AllowEmptyString()][string]$Text, [int]$Max = 500)
    if ($null -eq $Text) { return '' }
    $flat = ($Text -replace '\r?\n', ' | ').Trim()
    if ($flat.Length -gt $Max) { return $flat.Substring(0, $Max) + '...' }
    return $flat
}

function New-EmptyColumns {
    return @{
        artifact_bytes       = ''
        install_ok           = ''
        launch_after_install = ''
        uninstall_ok         = ''
        signed               = ''
    }
}

function Add-PackagingRow {
    param(
        [Parameter(Mandatory)][string]$App,
        [Parameter(Mandatory)][string]$Tool,
        [Parameter(Mandatory)][string]$Format,
        [Parameter(Mandatory)][string]$Status,
        [Parameter(Mandatory)][hashtable]$Columns,
        [Parameter(Mandatory)][System.Collections.Generic.List[string]]$Notes
    )
    $noteText = Limit-Note ($Notes -join '; ')
    $fields = @(
        $App, $Tool, $Format, $Status,
        $Columns['artifact_bytes'], $Columns['install_ok'],
        $Columns['launch_after_install'], $Columns['uninstall_ok'],
        $Columns['signed'], $noteText
    ) | ForEach-Object { ConvertTo-CsvField $_ }
    $RowLines.Add($fields -join ',')
    # Rewrite progressively so a hard crash mid-sweep still leaves evidence.
    Write-Utf8NoBom -Path $ResultsCsv -Content (($RowLines -join "`n") + "`n")
    Write-Host ("[{0}/{1}/{2}] {3}  {4}" -f $Tool, $App, $Format, $Status, (Limit-Note $noteText 120))
}

# ----------------------------------------------------------- process runner
# Every external process in this script goes through Invoke-Timed:
# Start-Process -PassThru + WaitForExit(ms), tree-kill on expiry, stdout and
# stderr appended to the row's transcript log.
function Invoke-Timed {
    param(
        [Parameter(Mandatory)][int]$Seconds,
        [Parameter(Mandatory)][string]$File,
        [string[]]$Arguments = @(),
        [string]$WorkDir = $RepoRoot,
        [Parameter(Mandatory)][string]$LogPath
    )
    $stamp = [DateTime]::Now.ToString('HHmmssfff')
    $outPath = "$LogPath.$stamp.out.tmp"
    $errPath = "$LogPath.$stamp.err.tmp"
    $startParams = @{
        FilePath               = $File
        WorkingDirectory       = $WorkDir
        RedirectStandardOutput = $outPath
        RedirectStandardError  = $errPath
        NoNewWindow            = $true
        PassThru               = $true
    }
    if ($Arguments.Count -gt 0) {
        # Start-Process -ArgumentList joins array elements with spaces WITHOUT
        # quoting them, and artifact paths here contain spaces (e.g.
        # "Iced Tasks_0.1.0_x64_en-US.msi") — quote explicitly and pass one string.
        $quoted = foreach ($argument in $Arguments) {
            if ($argument -match '[\s"]') { '"' + $argument.Replace('"', '\"') + '"' } else { $argument }
        }
        $startParams['ArgumentList'] = (@($quoted) -join ' ')
    }
    $proc = Start-Process @startParams
    $null = $proc.Handle  # cache the handle so ExitCode is readable after exit
    $timedOut = $false
    if (-not $proc.WaitForExit($Seconds * 1000)) {
        $timedOut = $true
        try { & taskkill /T /F /PID $proc.Id 2>&1 | Out-Null } catch { $null = $_ }
        try { $null = $proc.WaitForExit(15000) } catch { $null = $_ }
    }
    Start-Sleep -Milliseconds 200  # let the redirection handles settle
    $stdout = ''
    $stderr = ''
    if (Test-Path -LiteralPath $outPath) {
        $stdout = [System.IO.File]::ReadAllText($outPath)
        Remove-Item -LiteralPath $outPath -Force -ErrorAction SilentlyContinue
    }
    if (Test-Path -LiteralPath $errPath) {
        $stderr = [System.IO.File]::ReadAllText($errPath)
        Remove-Item -LiteralPath $errPath -Force -ErrorAction SilentlyContinue
    }
    $exitCode = $null
    if (-not $timedOut) { $exitCode = $proc.ExitCode }
    $header = "### $File " + ($Arguments -join ' ') +
        " (cwd=$WorkDir timeout=${Seconds}s timed_out=$timedOut exit=$exitCode)`n"
    $body = $stdout
    if ($stderr) { $body = $body + "`n--- stderr ---`n" + $stderr }
    Add-LogText -Path $LogPath -Text ($header + $body + "`n")
    return [pscustomobject]@{
        TimedOut = $timedOut
        ExitCode = $exitCode
        Stdout   = $stdout
        Stderr   = $stderr
    }
}

function Test-Tool {
    param(
        [Parameter(Mandatory)][string]$File,
        [string[]]$Arguments = @(),
        [Parameter(Mandatory)][string]$LogPath
    )
    try {
        $result = Invoke-Timed -Seconds 300 -File $File -Arguments $Arguments -LogPath $LogPath
        return ((-not $result.TimedOut) -and ($result.ExitCode -eq 0))
    }
    catch {
        Add-LogText -Path $LogPath -Text "### probe $File failed to start: $($_.Exception.Message)`n"
        return $false
    }
}

function Get-FirstErrorLine {
    param([Parameter(Mandatory)]$Result)
    if ($Result.TimedOut) { return 'timeout (process tree killed)' }
    foreach ($text in @($Result.Stderr, $Result.Stdout)) {
        if (-not $text) { continue }
        foreach ($line in ($text -split "`r?`n")) {
            if ($line.Trim() -and $line -match '(?i)error') { return $line.Trim() }
        }
    }
    foreach ($text in @($Result.Stderr, $Result.Stdout)) {
        if (-not $text) { continue }
        foreach ($line in ($text -split "`r?`n")) {
            if ($line.Trim()) { return "exit=$($Result.ExitCode): " + $line.Trim() }
        }
    }
    return "exit=$($Result.ExitCode) (no output captured)"
}

function Get-CargoPackageField {
    param(
        [Parameter(Mandatory)][string]$ManifestPath,
        [Parameter(Mandatory)][string]$Field
    )
    if (-not (Test-Path -LiteralPath $ManifestPath -PathType Leaf)) { return $null }
    foreach ($line in (Get-Content -LiteralPath $ManifestPath)) {
        if ($line -match ('^\s*' + [regex]::Escape($Field) + '\s*=\s*"([^"]+)"')) {
            return $Matches[1]
        }
    }
    return $null
}

# Newest file matching any pattern, written at/after $Since (avoids picking up
# stale artifacts from earlier runs).
function Find-NewArtifact {
    param(
        [Parameter(Mandatory)][string[]]$Directories,
        [Parameter(Mandatory)][string[]]$Patterns,
        [Parameter(Mandatory)][datetime]$Since,
        [switch]$Shallow
    )
    $found = [System.Collections.Generic.List[object]]::new()
    foreach ($dir in $Directories) {
        if (-not (Test-Path -LiteralPath $dir -PathType Container)) { continue }
        foreach ($pattern in $Patterns) {
            $items = if ($Shallow) {
                Get-ChildItem -Path $dir -File -Filter $pattern -ErrorAction SilentlyContinue
            }
            else {
                Get-ChildItem -Path $dir -Recurse -File -Filter $pattern -ErrorAction SilentlyContinue
            }
            foreach ($item in @($items)) { $found.Add($item) }
        }
    }
    $fresh = @($found | Where-Object { $_.LastWriteTime -ge $Since } | Sort-Object LastWriteTime -Descending)
    if ($fresh.Count -gt 0) { return $fresh[0].FullName }
    return $null
}

# ----------------------------------------------- install / launch / uninstall
function Find-InstalledExe {
    param([Parameter(Mandatory)][string]$App)
    $framework = $App -replace '-app$', ''
    $product = [string]$ProductNames[$App]
    $terms = @($product, $App, $framework) | Where-Object { $_ } | Select-Object -Unique
    $roots = @($env:ProgramFiles, ${env:ProgramFiles(x86)}, (Join-Path $env:LOCALAPPDATA 'Programs')) |
        Where-Object { $_ -and (Test-Path -LiteralPath $_ -PathType Container) }
    $candidates = [System.Collections.Generic.List[object]]::new()
    foreach ($root in $roots) {
        foreach ($dir in @(Get-ChildItem -LiteralPath $root -Directory -ErrorAction SilentlyContinue)) {
            $match = $false
            foreach ($term in $terms) {
                if ($dir.Name -like "*$term*") { $match = $true; break }
            }
            if (-not $match) { continue }
            foreach ($exe in @(Get-ChildItem -LiteralPath $dir.FullName -Recurse -Filter '*.exe' -ErrorAction SilentlyContinue)) {
                if ($exe.BaseName -match '(?i)unins') { continue }
                $candidates.Add($exe)
            }
        }
    }
    if ($candidates.Count -eq 0) { return $null }
    $preferred = @($candidates | Where-Object {
            $_.BaseName -ieq $App -or $_.BaseName -ieq $product -or $_.BaseName -ieq $framework
        })
    $pool = $candidates
    if ($preferred.Count -gt 0) { $pool = $preferred }
    return @($pool | Sort-Object LastWriteTime -Descending)[0].FullName
}

function Test-LaunchAfterInstall {
    param([Parameter(Mandatory)][string]$ExePath)
    $proc = Start-Process -FilePath $ExePath -PassThru
    $null = $proc.Handle
    Start-Sleep -Seconds 8
    $alive = -not $proc.HasExited
    try { & taskkill /T /F /PID $proc.Id 2>&1 | Out-Null } catch { $null = $_ }
    if ($alive) { return 'yes' }
    return 'no'
}

function Uninstall-InstallerArtifact {
    param(
        [Parameter(Mandatory)][string]$Format,
        [Parameter(Mandatory)][string]$Artifact,
        [AllowNull()][string]$InstalledExe,
        [Parameter(Mandatory)][string]$LogPath,
        [Parameter(Mandatory)][System.Collections.Generic.List[string]]$Notes
    )
    if ($Format -eq 'msi') {
        # Absolute path: PATH in the elevated dev-shell can resolve 'msiexec'
        # to a non-executable (ERROR_BAD_EXE_FORMAT before any MSI logic runs).
        $result = Invoke-Timed -Seconds 300 -File (Join-Path $env:SystemRoot 'System32\msiexec.exe') `
            -Arguments @('/x', $Artifact, '/qn', '/norestart') -LogPath $LogPath
        if ((-not $result.TimedOut) -and ($result.ExitCode -eq 0 -or $result.ExitCode -eq 3010)) { return 'yes' }
        if ($result.TimedOut) { $Notes.Add('uninstall timeout') } else { $Notes.Add("uninstall exit=$($result.ExitCode)") }
        return 'no'
    }
    # NSIS: best-effort — locate an uninstaller exe in the install directory.
    $uninstaller = $null
    if ($InstalledExe) {
        $installDir = Split-Path -Parent $InstalledExe
        $found = @(Get-ChildItem -Path $installDir -Filter '*.exe' -ErrorAction SilentlyContinue |
                Where-Object { $_.BaseName -match '(?i)unins' })
        if ($found.Count -gt 0) { $uninstaller = $found[0].FullName }
    }
    if (-not $uninstaller) {
        $Notes.Add('no-uninstaller-found')
        return 'no-uninstaller-found'
    }
    $result = Invoke-Timed -Seconds 300 -File $uninstaller -Arguments @('/S') -LogPath $LogPath
    # NSIS silent uninstall hands off to a temp copy (Un_A.exe) and can return
    # before the removal finishes.
    Start-Sleep -Seconds 5
    if ((-not $result.TimedOut) -and ($result.ExitCode -eq 0)) { return 'yes' }
    if ($result.TimedOut) { $Notes.Add('uninstall timeout') } else { $Notes.Add("uninstall exit=$($result.ExitCode)") }
    return 'no'
}

# Steps 2-5 for a produced artifact. Each sub-step has its own try/catch so a
# failure is recorded in notes and the remaining steps still run/record.
function Complete-ArtifactSteps {
    param(
        [Parameter(Mandatory)][string]$App,
        [Parameter(Mandatory)][string]$Format,
        [Parameter(Mandatory)][string]$Artifact,
        [Parameter(Mandatory)][string]$LogPath,
        [Parameter(Mandatory)][System.Collections.Generic.List[string]]$Notes
    )
    $columns = New-EmptyColumns

    try {
        $columns['artifact_bytes'] = [string](Get-Item -LiteralPath $Artifact).Length
    }
    catch {
        $Notes.Add("size: $($_.Exception.Message)")
    }

    # signed: recorded verbatim (expected NotSigned — no identity configured).
    try {
        $columns['signed'] = (Get-AuthenticodeSignature -LiteralPath $Artifact).Status.ToString()
    }
    catch {
        $Notes.Add("signature: $($_.Exception.Message)")
    }

    $installOk = $false
    try {
        if ($Format -eq 'msi') {
            $msiLog = Join-Path $LogDir "$App-$Format-install.msilog"
            # Absolute path: see Uninstall-Artifact note.
            $result = Invoke-Timed -Seconds 300 -File (Join-Path $env:SystemRoot 'System32\msiexec.exe') `
                -Arguments @('/i', $Artifact, '/qn', '/norestart', '/l*v', $msiLog) -LogPath $LogPath
            if ((-not $result.TimedOut) -and ($result.ExitCode -eq 0 -or $result.ExitCode -eq 3010)) {
                $installOk = $true
            }
            elseif ($result.TimedOut) { $Notes.Add('install timeout') }
            else { $Notes.Add("install exit=$($result.ExitCode)") }
        }
        else {
            $result = Invoke-Timed -Seconds 300 -File $Artifact -Arguments @('/S') -LogPath $LogPath
            if ((-not $result.TimedOut) -and ($result.ExitCode -eq 0)) {
                $installOk = $true
            }
            elseif ($result.TimedOut) { $Notes.Add('install timeout') }
            else { $Notes.Add("install exit=$($result.ExitCode)") }
        }
    }
    catch {
        $Notes.Add("install: $($_.Exception.Message)")
    }
    if ($installOk) { $columns['install_ok'] = 'yes' } else { $columns['install_ok'] = 'no' }
    if (-not $installOk) { return $columns }

    $installedExe = $null
    try {
        $installedExe = Find-InstalledExe -App $App
        if ($installedExe) {
            $Notes.Add("installed_exe=$installedExe")
            $columns['launch_after_install'] = Test-LaunchAfterInstall -ExePath $installedExe
        }
        else {
            $columns['launch_after_install'] = 'no'
            $Notes.Add('installed exe not found under ProgramFiles/ProgramFiles(x86)/LOCALAPPDATA\Programs')
        }
    }
    catch {
        $columns['launch_after_install'] = 'no'
        $Notes.Add("launch: $($_.Exception.Message)")
    }

    try {
        $columns['uninstall_ok'] = Uninstall-InstallerArtifact -Format $Format -Artifact $Artifact `
            -InstalledExe $installedExe -LogPath $LogPath -Notes $Notes
    }
    catch {
        $columns['uninstall_ok'] = 'no'
        $Notes.Add("uninstall: $($_.Exception.Message)")
    }

    return $columns
}

# Ensure the app's release exe exists (cargo-packager does not build).
function Confirm-ReleaseBinary {
    param(
        [Parameter(Mandatory)][string]$App,
        [Parameter(Mandatory)][string]$LogPath,
        # AllowEmptyCollection: callers pass a fresh empty notes list; Mandatory
        # alone rejects empty collections at bind time.
        [Parameter(Mandatory)][AllowEmptyCollection()][System.Collections.Generic.List[string]]$Notes
    )
    $exe = Join-Path $RepoRoot "apps/$App/target/release/$App.exe"
    if (Test-Path -LiteralPath $exe -PathType Leaf) { return $true }
    $Notes.Add('release exe missing; ran cargo build --locked --release')
    $result = Invoke-Timed -Seconds 1800 -File 'rustup' `
        -Arguments @('run', '1.96.1', 'cargo', 'build', '--locked', '--release') `
        -WorkDir (Join-Path $RepoRoot "apps/$App") -LogPath $LogPath
    return ((-not $result.TimedOut) -and ($result.ExitCode -eq 0) -and (Test-Path -LiteralPath $exe -PathType Leaf))
}

# --------------------------------------------------------------- tool sweeps

if ($Tools -contains 'cargo-bundle') {
    Write-Host '=== tool: cargo-bundle (msi x10) ==='
    $toolOk = Test-Tool -File 'rustup' -Arguments @('run', '1.96.1', 'cargo', 'bundle', '--version') `
        -LogPath (Join-Path $LogDir 'tool-probe-cargo-bundle.log')
    foreach ($app in $Apps) {
        $log = Join-Path $LogDir "$app-cargo-bundle.log"
        $notes = [System.Collections.Generic.List[string]]::new()
        try {
            if (-not $toolOk) {
                $notes.Add('tool unavailable: rustup run 1.96.1 cargo bundle --version failed')
                Add-PackagingRow -App $app -Tool 'cargo-bundle' -Format 'msi' -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
                continue
            }
            $appDir = Join-Path $RepoRoot "apps/$app"
            $since = Get-Date
            $result = Invoke-Timed -Seconds 1800 -File 'rustup' `
                -Arguments @('run', '1.96.1', 'cargo', 'bundle', '--release', '--format', 'msi') `
                -WorkDir $appDir -LogPath $log
            $artifact = Find-NewArtifact -Directories @(Join-Path $appDir 'target/release/bundle/msi') `
                -Patterns @('*.msi') -Since $since
            if ((-not $result.TimedOut) -and ($result.ExitCode -eq 0) -and $artifact) {
                $notes.Add("artifact=$artifact")
                $columns = Complete-ArtifactSteps -App $app -Format 'msi' -Artifact $artifact -LogPath $log -Notes $notes
                Add-PackagingRow -App $app -Tool 'cargo-bundle' -Format 'msi' -Status 'passed' -Columns $columns -Notes $notes
            }
            else {
                if ((-not $result.TimedOut) -and ($result.ExitCode -eq 0)) {
                    $notes.Add('build exited 0 but no .msi artifact under target/release/bundle/msi')
                }
                else {
                    $notes.Add((Get-FirstErrorLine -Result $result))
                }
                Add-PackagingRow -App $app -Tool 'cargo-bundle' -Format 'msi' -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
            }
        }
        catch {
            $notes.Add($_.Exception.Message)
            Add-PackagingRow -App $app -Tool 'cargo-bundle' -Format 'msi' -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
        }
    }
}

if ($Tools -contains 'cargo-packager') {
    Write-Host '=== tool: cargo-packager (nsis x10 + wix x10) ==='
    $toolOk = Test-Tool -File 'cargo-packager' -Arguments @('--version') `
        -LogPath (Join-Path $LogDir 'tool-probe-cargo-packager.log')
    foreach ($app in $Apps) {
        $appDir = Join-Path $RepoRoot "apps/$app"
        $prepLog = Join-Path $LogDir "$app-cargo-packager-prep.log"
        $prepNotes = [System.Collections.Generic.List[string]]::new()
        $prepOk = $false
        $configCreated = $false
        try {
            if ($toolOk) {
                $prepOk = Confirm-ReleaseBinary -App $app -LogPath $prepLog -Notes $prepNotes
                if (-not $prepOk) { $prepNotes.Add('release binary unavailable; cargo-packager has nothing to package') }
                $packagerToml = Join-Path $appDir 'Packager.toml'
                $manifest = Join-Path $appDir 'Cargo.toml'
                $manifestText = ''
                if (Test-Path -LiteralPath $manifest -PathType Leaf) {
                    $manifestText = Get-Content -LiteralPath $manifest -Raw
                }
                $hasMetadata = $manifestText -match '\[package\.metadata\.packager\]'
                if ($prepOk -and -not (Test-Path -LiteralPath $packagerToml) -and -not $hasMetadata) {
                    # Minimal on-the-fly config: the 9-LOC shape from
                    # report/data/packaging-results.md (cargo-packager head-to-head).
                    # `name` must be explicit (0.11.8 auto-detection bug), and a
                    # standalone Packager.toml infers nothing from Cargo.toml, so
                    # binaries/binaries-dir/out-dir are spelled out.
                    $pkgName = Get-CargoPackageField -ManifestPath $manifest -Field 'name'
                    if (-not $pkgName) { $pkgName = $app }
                    $pkgVersion = Get-CargoPackageField -ManifestPath $manifest -Field 'version'
                    if (-not $pkgVersion) { $pkgVersion = '0.1.0' }
                    $framework = $app -replace '-app$', ''
                    $tomlLines = @(
                        "name = `"$pkgName`""
                        "identifier = `"org.rcn.${framework}app`""
                        "product-name = `"$pkgName`""
                        "version = `"$pkgVersion`""
                        'out-dir = "target/packager"'
                        'binaries-dir = "target/release"'
                        ''
                        '[[binaries]]'
                        "path = `"$pkgName`""
                        'main = true'
                    )
                    Write-Utf8NoBom -Path $packagerToml -Content (($tomlLines -join "`n") + "`n")
                    $configCreated = $true
                }
            }
        }
        catch {
            $prepNotes.Add("prep: $($_.Exception.Message)")
            $prepOk = $false
        }

        foreach ($format in @('nsis', 'wix')) {
            $log = Join-Path $LogDir "$app-cargo-packager-$format.log"
            $notes = [System.Collections.Generic.List[string]]::new()
            foreach ($note in $prepNotes) { $notes.Add($note) }
            if ($configCreated) { $notes.Add('config_created=packager.toml') }
            try {
                if (-not $toolOk) {
                    $notes.Add('tool unavailable: cargo-packager --version failed')
                    Add-PackagingRow -App $app -Tool 'cargo-packager' -Format $format -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
                    continue
                }
                if (-not $prepOk) {
                    Add-PackagingRow -App $app -Tool 'cargo-packager' -Format $format -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
                    continue
                }
                $since = Get-Date
                $result = Invoke-Timed -Seconds 900 -File 'cargo-packager' `
                    -Arguments @('--release', '--formats', $format) -WorkDir $appDir -LogPath $log
                $patterns = @('*.msi')
                if ($format -eq 'nsis') { $patterns = @('*-setup.exe', '*.exe') }
                $artifact = Find-NewArtifact -Directories @(Join-Path $appDir 'target/packager') `
                    -Patterns $patterns -Since $since
                if (-not $artifact) {
                    # honor an out-dir declared by a pre-existing Packager.toml
                    # (e.g. iced-app's committed macOS config uses
                    # target/release/packager)
                    $tomlPath = Join-Path $appDir 'Packager.toml'
                    if (Test-Path -LiteralPath $tomlPath) {
                        $m = [regex]::Match((Get-Content -LiteralPath $tomlPath -Raw), '(?m)^\s*out-dir\s*=\s*"([^"]+)"')
                        if ($m.Success) {
                            $artifact = Find-NewArtifact -Directories @(Join-Path $appDir $m.Groups[1].Value) -Patterns $patterns -Since $since
                        }
                    }
                }
                if (-not $artifact) {
                    # pre-existing configs may use another out-dir; check the
                    # app root shallowly (0.11.8's default out-dir is the root)
                    $artifact = Find-NewArtifact -Directories @($appDir) -Patterns $patterns -Since $since -Shallow
                }
                if ((-not $result.TimedOut) -and ($result.ExitCode -eq 0) -and $artifact) {
                    $notes.Add("artifact=$artifact")
                    $columns = Complete-ArtifactSteps -App $app -Format $format -Artifact $artifact -LogPath $log -Notes $notes
                    Add-PackagingRow -App $app -Tool 'cargo-packager' -Format $format -Status 'passed' -Columns $columns -Notes $notes
                }
                else {
                    if ((-not $result.TimedOut) -and ($result.ExitCode -eq 0)) {
                        $notes.Add("packager exited 0 but no $format artifact found")
                    }
                    else {
                        $notes.Add((Get-FirstErrorLine -Result $result))
                    }
                    Add-PackagingRow -App $app -Tool 'cargo-packager' -Format $format -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
                }
            }
            catch {
                $notes.Add($_.Exception.Message)
                Add-PackagingRow -App $app -Tool 'cargo-packager' -Format $format -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
            }
        }
    }
}

if ($Tools -contains 'tauri') {
    Write-Host '=== tool: tauri-cli (msi + nsis, tauri-app only) ==='
    $app = 'tauri-app'
    $appDir = Join-Path $RepoRoot "apps/$app"
    $log = Join-Path $LogDir "$app-tauri.log"
    $toolOk = Test-Tool -File 'rustup' -Arguments @('run', '1.96.1', 'cargo', 'tauri', '--version') `
        -LogPath (Join-Path $LogDir 'tool-probe-tauri.log')
    $buildResult = $null
    $since = Get-Date
    if ($toolOk) {
        try {
            # One build, two artifact rows (msi + nsis).
            $buildResult = Invoke-Timed -Seconds 2400 -File 'rustup' `
                -Arguments @('run', '1.96.1', 'cargo', 'tauri', 'build', '--ci', '--no-sign', '--bundles', 'msi,nsis') `
                -WorkDir $appDir -LogPath $log
        }
        catch {
            Add-LogText -Path $log -Text "### tauri build failed to start: $($_.Exception.Message)`n"
        }
    }
    foreach ($format in @('msi', 'nsis')) {
        $notes = [System.Collections.Generic.List[string]]::new()
        try {
            if (-not $toolOk) {
                $notes.Add('tool unavailable: rustup run 1.96.1 cargo tauri --version failed')
                Add-PackagingRow -App $app -Tool 'tauri' -Format $format -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
                continue
            }
            if ($null -eq $buildResult) {
                $notes.Add('tauri build did not run')
                Add-PackagingRow -App $app -Tool 'tauri' -Format $format -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
                continue
            }
            $patterns = @('*.msi')
            if ($format -eq 'nsis') { $patterns = @('*-setup.exe', '*.exe') }
            $artifact = Find-NewArtifact -Directories @(Join-Path $appDir "target/release/bundle/$format") `
                -Patterns $patterns -Since $since
            if ($artifact) {
                if ($buildResult.TimedOut -or $buildResult.ExitCode -ne 0) {
                    $notes.Add('build reported failure but artifact exists: ' + (Get-FirstErrorLine -Result $buildResult))
                }
                $notes.Add("artifact=$artifact")
                $columns = Complete-ArtifactSteps -App $app -Format $format -Artifact $artifact -LogPath $log -Notes $notes
                Add-PackagingRow -App $app -Tool 'tauri' -Format $format -Status 'passed' -Columns $columns -Notes $notes
            }
            else {
                if ($buildResult.TimedOut -or $buildResult.ExitCode -ne 0) {
                    $notes.Add((Get-FirstErrorLine -Result $buildResult))
                }
                else {
                    $notes.Add("build exited 0 but no $format artifact under target/release/bundle/$format")
                }
                Add-PackagingRow -App $app -Tool 'tauri' -Format $format -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
            }
        }
        catch {
            $notes.Add($_.Exception.Message)
            Add-PackagingRow -App $app -Tool 'tauri' -Format $format -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
        }
    }
}

if ($Tools -contains 'dx') {
    Write-Host '=== tool: dx bundle (dioxus-app only) ==='
    $app = 'dioxus-app'
    $appDir = Join-Path $RepoRoot "apps/$app"
    $log = Join-Path $LogDir "$app-dx.log"
    $notes = [System.Collections.Generic.List[string]]::new()
    try {
        $toolOk = Test-Tool -File 'dx' -Arguments @('--version') `
            -LogPath (Join-Path $LogDir 'tool-probe-dx.log')
        if (-not $toolOk) {
            $notes.Add('tool unavailable: dx --version failed')
            Add-PackagingRow -App $app -Tool 'dx' -Format 'msi' -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
        }
        else {
            # Dioxus.toml with [bundle] identifier/publisher/icon already exists
            # in the repo (2026-08-08 dx experiment). dx compiles into its own
            # target/dx/<app> profile and bundles under target/dx/<app>/bundle/.
            $since = Get-Date
            $result = Invoke-Timed -Seconds 2400 -File 'dx' -Arguments @('bundle', '--release') `
                -WorkDir $appDir -LogPath $log
            $bundleDir = Join-Path $appDir "target/dx/$app/bundle"
            $format = 'msi'
            $artifact = Find-NewArtifact -Directories @($bundleDir) -Patterns @('*.msi') -Since $since
            if (-not $artifact) {
                $artifact = Find-NewArtifact -Directories @($bundleDir) -Patterns @('*-setup.exe', '*.exe') -Since $since
                if ($artifact) {
                    $format = 'nsis'
                    $notes.Add('expected msi; dx produced an exe installer instead')
                }
            }
            if ((-not $result.TimedOut) -and ($result.ExitCode -eq 0) -and $artifact) {
                $notes.Add("artifact=$artifact")
                $columns = Complete-ArtifactSteps -App $app -Format $format -Artifact $artifact -LogPath $log -Notes $notes
                Add-PackagingRow -App $app -Tool 'dx' -Format $format -Status 'passed' -Columns $columns -Notes $notes
            }
            else {
                if ((-not $result.TimedOut) -and ($result.ExitCode -eq 0)) {
                    $notes.Add("dx bundle exited 0 but no installer artifact under $bundleDir")
                }
                else {
                    $notes.Add((Get-FirstErrorLine -Result $result))
                }
                Add-PackagingRow -App $app -Tool 'dx' -Format $format -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
            }
        }
    }
    catch {
        $notes.Add($_.Exception.Message)
        Add-PackagingRow -App $app -Tool 'dx' -Format 'msi' -Status 'failed' -Columns (New-EmptyColumns) -Notes $notes
    }
}

Write-Host ''
Write-Host "Results: $ResultsCsv"
Write-Host "Logs:    $LogDir"
Write-Host ("Rows:    {0} (expect 33 when all four tools run)" -f ($RowLines.Count - 1))
