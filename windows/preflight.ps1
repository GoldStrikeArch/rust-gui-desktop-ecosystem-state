#Requires -Version 7.0
<#
  windows/preflight.ps1 -- gate checks for the Windows measurement harness.
  Mirrors linux/run-cohort.sh preconditions: every check prints an [ok]/[FAIL]
  line ([warn]/[info] for advisory checks), an itemized summary follows, and
  the script exits 1 if any check FAILed.

  Usage: pwsh -NoProfile -File windows/preflight.ps1 [-Packaging]

  -Packaging adds packaging-tool checks (cargo-bundle, cargo-packager,
  dioxus-cli 0.7.10, WiX v3 candle/light, NSIS makensis).
#>
[CmdletBinding()]
param(
    [switch]$Packaging
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-Utf8NoBom([string]$Path,[string]$Content) { [IO.File]::WriteAllText($Path, $Content.Replace("`r`n","`n"), [Text.UTF8Encoding]::new($false)) }

$RepoRoot = Split-Path -Parent $PSScriptRoot

$script:Checks = [System.Collections.Generic.List[object]]::new()

function Add-Check([string]$Name, [string]$Status, [string]$Detail = '') {
    $script:Checks.Add([pscustomobject]@{ Name = $Name; Status = $Status; Detail = $Detail })
    $tag = switch ($Status) {
        'ok'   { '[ok]' }
        'FAIL' { '[FAIL]' }
        'warn' { '[warn]' }
        default { '[info]' }
    }
    if ($Detail -ne '') {
        Write-Host "$tag $Name -- $Detail"
    } else {
        Write-Host "$tag $Name"
    }
}

# Collect a native command's combined output as an array of strings.
function Get-NativeOutput([string]$Exe, [string[]]$Arguments) {
    @(& $Exe @Arguments 2>&1 | ForEach-Object { "$_" })
}

# --- (a) pwsh major version -------------------------------------------------
try {
    if ($PSVersionTable.PSVersion.Major -ge 7) {
        Add-Check 'pwsh major version >= 7' 'ok' "$($PSVersionTable.PSVersion)"
    } else {
        Add-Check 'pwsh major version >= 7' 'FAIL' "found $($PSVersionTable.PSVersion)"
    }
} catch { Add-Check 'pwsh major version >= 7' 'FAIL' $_.Exception.Message }

# --- (b) VS 2022 Build Tools with C++ workload (via vswhere) ---------------
try {
    $pf86 = ${env:ProgramFiles(x86)}
    if ([string]::IsNullOrEmpty($pf86)) {
        Add-Check 'VS 2022 Build Tools (VC.Tools.x86.x64)' 'FAIL' 'ProgramFiles(x86) not defined'
    } else {
        $vswhere = Join-Path $pf86 'Microsoft Visual Studio\Installer\vswhere.exe'
        if (-not (Test-Path -LiteralPath $vswhere)) {
            Add-Check 'VS 2022 Build Tools (VC.Tools.x86.x64)' 'FAIL' "vswhere.exe not found at $vswhere"
        } else {
            $vsPath = Get-NativeOutput $vswhere @('-products', '*', '-requires', 'Microsoft.VisualStudio.Component.VC.Tools.x86.x64', '-latest', '-property', 'installationPath') |
                Where-Object { $_.Trim() -ne '' } | Select-Object -First 1
            if ($vsPath) {
                Add-Check 'VS 2022 Build Tools (VC.Tools.x86.x64)' 'ok' $vsPath
            } else {
                Add-Check 'VS 2022 Build Tools (VC.Tools.x86.x64)' 'FAIL' 'no installation advertises Microsoft.VisualStudio.Component.VC.Tools.x86.x64'
            }
        }
    }
} catch { Add-Check 'VS 2022 Build Tools (VC.Tools.x86.x64)' 'FAIL' $_.Exception.Message }

# --- (c) rust toolchain 1.96.1, msvc host ----------------------------------
try {
    $verLine = Get-NativeOutput 'rustup' @('run', '1.96.1', 'rustc', '--version') | Select-Object -First 1
    if ($null -ne $verLine -and $verLine.StartsWith('rustc 1.96.1')) {
        Add-Check 'rustc 1.96.1 via rustup' 'ok' $verLine
    } else {
        Add-Check 'rustc 1.96.1 via rustup' 'FAIL' "got: $verLine"
    }
    $hostLine = Get-NativeOutput 'rustup' @('run', '1.96.1', 'rustc', '-vV') | Where-Object { $_ -like 'host:*' } | Select-Object -First 1
    if ($hostLine -eq 'host: x86_64-pc-windows-msvc') {
        Add-Check 'rustc host x86_64-pc-windows-msvc' 'ok' $hostLine
    } else {
        Add-Check 'rustc host x86_64-pc-windows-msvc' 'FAIL' "got: $hostLine"
    }
} catch {
    Add-Check 'rustc 1.96.1 via rustup' 'FAIL' $_.Exception.Message
    Add-Check 'rustc host x86_64-pc-windows-msvc' 'FAIL' 'rustup unavailable'
}

# --- (d) WebView2 Runtime ---------------------------------------------------
try {
    $wvPaths = @(
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    )
    $pv = ''
    foreach ($p in $wvPaths) {
        $item = Get-ItemProperty -Path $p -Name pv -ErrorAction SilentlyContinue
        if ($item -and "$($item.pv)".Trim() -ne '') { $pv = "$($item.pv)".Trim(); break }
    }
    if ($pv -ne '') {
        Add-Check 'WebView2 Runtime installed' 'ok' "pv=$pv"
    } else {
        Add-Check 'WebView2 Runtime installed' 'FAIL' 'no pv value under EdgeUpdate\Clients (WOW6432Node or native)'
    }
} catch { Add-Check 'WebView2 Runtime installed' 'FAIL' $_.Exception.Message }

# --- (e) python >= 3.11 -----------------------------------------------------
try {
    $pyOk = $false
    $pyDetail = 'no python3/python >= 3.11 on PATH'
    foreach ($cand in @('python3', 'python')) {
        $cmd = Get-Command $cand -ErrorAction SilentlyContinue
        if (-not $cmd) { continue }
        $out = ''
        try { $out = Get-NativeOutput $cand @('--version') | Select-Object -First 1 } catch { continue }
        if ($null -ne $out -and $out -match 'Python\s+(\d+)\.(\d+)') {
            $maj = [int]$Matches[1]; $min = [int]$Matches[2]
            if ($maj -gt 3 -or ($maj -eq 3 -and $min -ge 11)) {
                $pyOk = $true
                $pyDetail = "$cand -> $out"
                break
            }
            $pyDetail = "$cand -> $out (need >= 3.11)"
        }
    }
    if ($pyOk) { Add-Check 'python >= 3.11' 'ok' $pyDetail } else { Add-Check 'python >= 3.11' 'FAIL' $pyDetail }
} catch { Add-Check 'python >= 3.11' 'FAIL' $_.Exception.Message }

# --- (f) git config in repo -------------------------------------------------
try {
    $autocrlf = Get-NativeOutput 'git' @('-C', $RepoRoot, 'config', '--get', 'core.autocrlf') | Select-Object -First 1
    if ($autocrlf -eq 'false') {
        Add-Check 'git core.autocrlf=false' 'ok'
    } else {
        Add-Check 'git core.autocrlf=false' 'FAIL' "got '$autocrlf' (checkout would rewrite line endings)"
    }
} catch { Add-Check 'git core.autocrlf=false' 'FAIL' $_.Exception.Message }
try {
    $longpaths = Get-NativeOutput 'git' @('-C', $RepoRoot, 'config', '--get', 'core.longpaths') | Select-Object -First 1
    if ($longpaths -eq 'true') {
        Add-Check 'git core.longpaths=true' 'ok'
    } else {
        Add-Check 'git core.longpaths=true' 'FAIL' "got '$longpaths'"
    }
} catch { Add-Check 'git core.longpaths=true' 'FAIL' $_.Exception.Message }

# --- (g) NTFS long paths enabled -------------------------------------------
try {
    $lp = Get-ItemProperty -Path 'HKLM:\SYSTEM\CurrentControlSet\Control\FileSystem' -Name LongPathsEnabled -ErrorAction SilentlyContinue
    if ($lp -and [int]$lp.LongPathsEnabled -eq 1) {
        Add-Check 'LongPathsEnabled=1 (HKLM FileSystem)' 'ok'
    } else {
        $got = if ($lp) { "$($lp.LongPathsEnabled)" } else { 'unset' }
        Add-Check 'LongPathsEnabled=1 (HKLM FileSystem)' 'FAIL' "got $got"
    }
} catch { Add-Check 'LongPathsEnabled=1 (HKLM FileSystem)' 'FAIL' $_.Exception.Message }

# --- (h) camera/microphone desktop-app consent (warn-only) -----------------
foreach ($device in @('webcam', 'microphone')) {
    try {
        $p = "HKCU:\SOFTWARE\Microsoft\Windows\CurrentVersion\CapabilityAccessManager\ConsentStore\$device\NonPackaged"
        $item = Get-ItemProperty -Path $p -Name Value -ErrorAction SilentlyContinue
        $val = if ($item) { "$($item.Value)" } else { 'unset' }
        if ($val -eq 'Allow') {
            Add-Check "$device desktop-app consent" 'ok' 'Allow'
        } else {
            Add-Check "$device desktop-app consent" 'warn' "expected 'Allow', got '$val' (peek apps may fail to open the device)"
        }
    } catch { Add-Check "$device desktop-app consent" 'warn' $_.Exception.Message }
}

# --- (i) Defender exclusion status (info-only) -----------------------------
try {
    $exclusions = @(@((Get-MpPreference -ErrorAction Stop).ExclusionPath) | Where-Object { $null -ne $_ -and "$_" -ne '' })
    $repoNorm = $RepoRoot.TrimEnd('\')
    $hit = @($exclusions | Where-Object { "$_".TrimEnd('\') -ieq $repoNorm })
    if ($hit.Count -gt 0) {
        Add-Check 'Defender exclusion for repo root' 'info' 'present'
    } else {
        Add-Check 'Defender exclusion for repo root' 'info' 'absent (builds will be scanned; timings may be noisier)'
    }
} catch { Add-Check 'Defender exclusion for repo root' 'info' "unavailable: $($_.Exception.Message)" }

# --- packaging checks (-Packaging) -----------------------------------------
if ($Packaging) {
    foreach ($tool in @(
        @{ Name = 'cargo-bundle'; Exe = 'cargo-bundle'; Arguments = @('--version') },
        @{ Name = 'cargo-packager'; Exe = 'cargo-packager'; Arguments = @('--version') }
    )) {
        try {
            $cmd = Get-Command $tool.Exe -ErrorAction SilentlyContinue
            if (-not $cmd) {
                Add-Check "$($tool.Name) on PATH" 'FAIL' 'not found'
            } else {
                $ver = Get-NativeOutput $tool.Exe $tool.Arguments | Where-Object { $_.Trim() -ne '' } | Select-Object -First 1
                Add-Check "$($tool.Name) on PATH" 'ok' "$ver"
            }
        } catch { Add-Check "$($tool.Name) on PATH" 'FAIL' $_.Exception.Message }
    }

    try {
        $dxCmd = Get-Command 'dx' -ErrorAction SilentlyContinue
        if (-not $dxCmd) {
            Add-Check 'dioxus-cli 0.7.10 (dx)' 'FAIL' 'dx not on PATH'
        } else {
            $dxOut = (Get-NativeOutput 'dx' @('--version')) -join ' '
            if ($dxOut -match '(?i)deno') {
                Add-Check 'dioxus-cli 0.7.10 (dx)' 'warn' "resolved dx appears to be Deno's ($($dxCmd.Source)); put dioxus-cli first on PATH"
            } elseif ($dxOut -match '0\.7\.10') {
                Add-Check 'dioxus-cli 0.7.10 (dx)' 'ok' $dxOut.Trim()
            } else {
                Add-Check 'dioxus-cli 0.7.10 (dx)' 'FAIL' "expected 0.7.10, got: $($dxOut.Trim()) ($($dxCmd.Source))"
            }
        }
    } catch { Add-Check 'dioxus-cli 0.7.10 (dx)' 'FAIL' $_.Exception.Message }

    foreach ($wixTool in @('candle.exe', 'light.exe')) {
        try {
            $found = ''
            $cmd = Get-Command $wixTool -ErrorAction SilentlyContinue
            if ($cmd) {
                $found = $cmd.Source
            } elseif (-not [string]::IsNullOrEmpty($env:WIX)) {
                $candidate = Join-Path (Join-Path $env:WIX 'bin') $wixTool
                if (Test-Path -LiteralPath $candidate) { $found = $candidate }
            }
            if ($found -ne '') {
                Add-Check "WiX v3 $wixTool" 'ok' $found
            } else {
                Add-Check "WiX v3 $wixTool" 'FAIL' 'not on PATH and not under ${env:WIX}bin'
            }
        } catch { Add-Check "WiX v3 $wixTool" 'FAIL' $_.Exception.Message }
    }

    try {
        $nsis = Get-Command 'makensis' -ErrorAction SilentlyContinue
        if (-not $nsis) { $nsis = Get-Command 'makensis.exe' -ErrorAction SilentlyContinue }
        if ($nsis) {
            Add-Check 'NSIS makensis on PATH' 'ok' $nsis.Source
        } else {
            Add-Check 'NSIS makensis on PATH' 'FAIL' 'not found'
        }
    } catch { Add-Check 'NSIS makensis on PATH' 'FAIL' $_.Exception.Message }
}

# --- summary ----------------------------------------------------------------
Write-Host ''
Write-Host '== preflight summary =='
foreach ($c in $script:Checks) {
    $suffix = if ($c.Detail -ne '') { " -- $($c.Detail)" } else { '' }
    Write-Host ('  [{0}] {1}{2}' -f $c.Status, $c.Name, $suffix)
}
$failCount = @($script:Checks | Where-Object { $_.Status -eq 'FAIL' }).Count
$warnCount = @($script:Checks | Where-Object { $_.Status -eq 'warn' }).Count
Write-Host ('{0} checks: {1} FAIL, {2} warn' -f $script:Checks.Count, $failCount, $warnCount)
if ($failCount -gt 0) { exit 1 }
exit 0
