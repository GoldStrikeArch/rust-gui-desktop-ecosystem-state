#Requires -Version 7.0
<#
  windows/capture-environment.ps1 -- write <Cohort>/windows/environment.txt.

  First line is the canonical machine line (aborts if CPU, RAM, Windows build,
  or GPU cannot be resolved); second line is run_date=<UTC yyyy-MM-dd>; the
  rest are best-effort labeled evidence blocks ('unavailable' on failure).

  Usage: pwsh -NoProfile -File windows/capture-environment.ps1 -Cohort <dir>
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Cohort
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-Utf8NoBom([string]$Path,[string]$Content) { [IO.File]::WriteAllText($Path, $Content.Replace("`r`n","`n"), [Text.UTF8Encoding]::new($false)) }

$RepoRoot = Split-Path -Parent $PSScriptRoot
if (-not [IO.Path]::IsPathRooted($Cohort)) { $Cohort = Join-Path $RepoRoot $Cohort }

# --- machine line (mandatory fields; abort on any empty) --------------------
$cpuName = ''
try {
    $cpu = Get-CimInstance -ClassName Win32_Processor | Select-Object -First 1
    if ($cpu -and $cpu.Name) { $cpuName = ("$($cpu.Name)" -replace '\s+', ' ').Trim() }
} catch { $cpuName = '' }

$ramGiB = 0
try {
    $cs = Get-CimInstance -ClassName Win32_ComputerSystem | Select-Object -First 1
    if ($cs -and $cs.TotalPhysicalMemory) { $ramGiB = [int][math]::Round([double]$cs.TotalPhysicalMemory / 1GB) }
} catch { $ramGiB = 0 }

$productName = ''
$displayVersion = ''
$buildNumber = ''
$ubr = ''
try {
    $nt = Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion'
    foreach ($pair in @(
        @{ Var = 'productName'; Prop = 'ProductName' },
        @{ Var = 'displayVersion'; Prop = 'DisplayVersion' },
        @{ Var = 'buildNumber'; Prop = 'CurrentBuildNumber' },
        @{ Var = 'ubr'; Prop = 'UBR' }
    )) {
        if ($nt.PSObject.Properties[$pair.Prop]) {
            Set-Variable -Name $pair.Var -Value ("$($nt.$($pair.Prop))".Trim())
        }
    }
} catch { }

# Windows 11 registry still reports ProductName='Windows 10 <edition>'; the
# build number is authoritative (>= 22000 means Windows 11).
if ($buildNumber -match '^\d+$' -and [int]$buildNumber -ge 22000 -and $productName -match '^Windows 10\b') {
    $productName = $productName -replace '^Windows 10', 'Windows 11'
}

$gpuName = ''
$gpuDriver = ''
try {
    $gpus = @(Get-CimInstance -ClassName Win32_VideoController)
    $gpu = $gpus | Where-Object { $_.Name -and $_.ConfigManagerErrorCode -eq 0 } | Select-Object -First 1
    if (-not $gpu) { $gpu = $gpus | Where-Object { $_.Name } | Select-Object -First 1 }
    if ($gpu) {
        $gpuName = "$($gpu.Name)".Trim()
        if ($gpu.PSObject.Properties['DriverVersion'] -and $gpu.DriverVersion) { $gpuDriver = "$($gpu.DriverVersion)".Trim() }
    }
} catch { }
if ($gpuDriver -eq '') { $gpuDriver = 'unknown' }

$missing = [System.Collections.Generic.List[string]]::new()
if ($cpuName -eq '') { $missing.Add('CPU (Win32_Processor.Name)') }
if ($ramGiB -le 0) { $missing.Add('RAM (Win32_ComputerSystem.TotalPhysicalMemory)') }
if ($productName -eq '' -or $buildNumber -eq '') { $missing.Add('Windows build (HKLM Windows NT CurrentVersion)') }
if ($gpuName -eq '') { $missing.Add('GPU (Win32_VideoController)') }
if ($missing.Count -gt 0) {
    [Console]::Error.WriteLine('capture-environment: mandatory machine facts unresolved:')
    foreach ($m in $missing) { [Console]::Error.WriteLine("  $m") }
    exit 1
}

$winDesc = ((@($productName, $displayVersion) | Where-Object { $_ -ne '' }) -join ' ') -replace '^Windows\s+', ''
$buildStr = if ($ubr -ne '') { "$buildNumber.$ubr" } else { $buildNumber }
$machineLine = "machine=$cpuName; $ramGiB GiB; Windows $winDesc (build $buildStr); rustc/cargo 1.96.1; $gpuName driver $gpuDriver"
$runDate = (Get-Date).ToUniversalTime().ToString('yyyy-MM-dd')

# --- best-effort evidence blocks -------------------------------------------
$script:Lines = [System.Collections.Generic.List[string]]::new()
$script:Lines.Add($machineLine)
$script:Lines.Add("run_date=$runDate")
$script:Lines.Add('')

function Add-Block([string]$Label, [scriptblock]$Body) {
    $content = 'unavailable'
    try {
        $raw = & $Body
        $flat = @($raw | ForEach-Object { "$_" } | Where-Object { $null -ne $_ })
        if ($flat.Count -gt 0 -and ($flat -join '').Trim() -ne '') {
            $content = ($flat -join "`n")
        }
    } catch { $content = 'unavailable' }
    $script:Lines.Add("[$Label]")
    $script:Lines.Add($content)
    $script:Lines.Add('')
}

Add-Block 'rustc_cargo' {
    & rustup run 1.96.1 rustc --version 2>&1
    & rustup run 1.96.1 cargo --version 2>&1
}

Add-Block 'link_exe' {
    @(& link.exe '/?' 2>&1 | ForEach-Object { "$_" } | Where-Object { $_.Trim() -ne '' }) | Select-Object -First 1
}

Add-Block 'webview2_runtime' {
    $paths = @(
        'HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}',
        'HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
    )
    foreach ($p in $paths) {
        $item = Get-ItemProperty -Path $p -Name pv -ErrorAction SilentlyContinue
        if ($item -and "$($item.pv)".Trim() -ne '') { "pv=$("$($item.pv)".Trim())"; break }
    }
}

Add-Block 'dpi_scale_primary' {
    $wm = Get-ItemProperty -Path 'HKCU:\Control Panel\Desktop\WindowMetrics' -Name AppliedDPI -ErrorAction Stop
    $dpi = [int]$wm.AppliedDPI
    '{0}% ({1} DPI)' -f [int][math]::Round($dpi / 96.0 * 100), $dpi
}

Add-Block 'defender_exclusions' {
    $paths = @(@((Get-MpPreference -ErrorAction Stop).ExclusionPath) | Where-Object { $null -ne $_ -and "$_" -ne '' })
    if ($paths.Count -gt 0) { $paths } else { 'none' }
}

Add-Block 'git' {
    $rev = @(& git -C $RepoRoot rev-parse HEAD 2>&1 | ForEach-Object { "$_" }) | Select-Object -First 1
    "rev=$rev"
    $dirty = (& git -C $RepoRoot status --porcelain | Measure-Object -Line).Lines
    "dirty_lines=$dirty"
}

Add-Block 'pwsh' {
    $PSVersionTable.PSVersion.ToString()
}

Add-Block 'timezone' {
    $tz = Get-TimeZone
    "$($tz.Id) ($($tz.DisplayName))"
}

# --- write ------------------------------------------------------------------
$outDir = Join-Path $Cohort 'windows'
New-Item -ItemType Directory -Force -Path $outDir | Out-Null
$outFile = Join-Path $outDir 'environment.txt'
Write-Utf8NoBom $outFile (($script:Lines -join "`n") + "`n")
Write-Host "wrote $outFile"
exit 0
