#Requires -Version 7.0
<#
  windows/verify-msi-install.ps1 -- supplemental install verification for the
  cargo-packager WiX MSIs produced by package-windows.ps1.

  Rationale: in both elevated package-windows.ps1 runs, Start-Process for
  msiexec.exe threw ERROR_BAD_EXE_FORMAT ("%1 is not a Win32 application")
  before the process launched, while the identical call succeeds in an
  isolated elevated session (see report/21). This script re-runs steps 3-5
  (silent install -> installed-exe launch check -> silent uninstall) for each
  existing WiX MSI, mirroring package-windows.ps1's Find-InstalledExe /
  Test-LaunchAfterInstall semantics, and records its own artifact:
  <cohort>/windows/packaging/install-verify.csv. packaging/results.csv is
  deliberately left untouched.

  Usage (elevated pwsh):
    pwsh -NoProfile -File windows/verify-msi-install.ps1 -Cohort <dir>
#>
[CmdletBinding()]
param(
    [Parameter(Mandatory)][string]$Cohort
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$RepoRoot = Split-Path -Parent $PSScriptRoot
if (-not [IO.Path]::IsPathRooted($Cohort)) { $Cohort = Join-Path $RepoRoot $Cohort }
$PackDir = Join-Path $Cohort 'windows/packaging'
$LogDir = Join-Path $PackDir 'logs'
$OutCsv = Join-Path $PackDir 'install-verify.csv'
New-Item -ItemType Directory -Force -Path $LogDir | Out-Null

function Write-Utf8NoBom([string]$Path,[string]$Content) { [IO.File]::WriteAllText($Path, $Content.Replace("`r`n","`n"), [Text.UTF8Encoding]::new($false)) }

$ProductNames = @{
    'iced-app'   = 'Iced Tasks'
    'egui-app'   = 'Egui Tasks'
    'gpui-app'   = 'Gpui Tasks'
    'xilem-app'  = 'Xilem Tasks'
    'slint-app'  = 'Slint Tasks'
    'dioxus-app' = 'Dioxus Tasks'
    'floem-app'  = 'Floem Tasks'
}

function Find-InstalledExe([string]$App) {
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

function Test-LaunchAfterInstall([string]$ExePath) {
    $proc = Start-Process -FilePath $ExePath -PassThru
    $null = $proc.Handle
    Start-Sleep -Seconds 8
    $alive = -not $proc.HasExited
    try { & taskkill /T /F /PID $proc.Id 2>&1 | Out-Null } catch { $null = $_ }
    if ($alive) { return 'yes' }
    return 'no'
}

$MsiExe = Join-Path $env:SystemRoot 'System32\msiexec.exe'
$Rows = [System.Collections.Generic.List[string]]::new()
$Rows.Add('app,tool,format,artifact,install_exit,install_ok,launch_after_install,uninstall_exit,uninstall_ok,notes')

$Targets = @(
    @{ App = 'iced-app';   Msi = 'apps/iced-app/target/release/packager/iced-app_0.1.0_x64_en-US.msi' }
    @{ App = 'egui-app';   Msi = 'apps/egui-app/target/packager/egui-app_0.1.0_x64_en-US.msi' }
    @{ App = 'gpui-app';   Msi = 'apps/gpui-app/target/packager/gpui-app_0.1.0_x64_en-US.msi' }
    @{ App = 'xilem-app';  Msi = 'apps/xilem-app/target/packager/xilem-app_0.1.0_x64_en-US.msi' }
    @{ App = 'slint-app';  Msi = 'apps/slint-app/target/packager/slint-app_0.1.0_x64_en-US.msi' }
    @{ App = 'dioxus-app'; Msi = 'apps/dioxus-app/target/packager/dioxus-app_0.1.0_x64_en-US.msi' }
    @{ App = 'floem-app';  Msi = 'apps/floem-app/target/packager/floem-app_0.1.0_x64_en-US.msi' }
)

foreach ($t in $Targets) {
    $app = $t.App
    $msi = Join-Path $RepoRoot $t.Msi
    $notes = [System.Collections.Generic.List[string]]::new()
    $installExit = ''
    $installOk = 'no'
    $launch = ''
    $uninstallExit = ''
    $uninstallOk = ''
    if (-not (Test-Path -LiteralPath $msi -PathType Leaf)) {
        $notes.Add('msi artifact missing')
    }
    else {
        $msiLog = Join-Path $LogDir "$app-wix-verify-install.msilog"
        try {
            $p = Start-Process -FilePath $MsiExe -ArgumentList ('/i "{0}" /qn /norestart /l*v "{1}"' -f $msi, $msiLog) -PassThru -Wait
            $installExit = [string]$p.ExitCode
            if ($p.ExitCode -eq 0 -or $p.ExitCode -eq 3010) { $installOk = 'yes' }
        } catch { $notes.Add("install: $($_.Exception.Message)") }
        if ($installOk -eq 'yes') {
            $exe = Find-InstalledExe $app
            if ($exe) {
                $notes.Add("installed_exe=$exe")
                try { $launch = Test-LaunchAfterInstall $exe } catch { $launch = 'no'; $notes.Add("launch: $($_.Exception.Message)") }
            } else {
                $launch = 'no'
                $notes.Add('installed exe not found under ProgramFiles/ProgramFiles(x86)/LOCALAPPDATA\Programs')
            }
            try {
                $u = Start-Process -FilePath $MsiExe -ArgumentList ('/x "{0}" /qn /norestart' -f $msi) -PassThru -Wait
                $uninstallExit = [string]$u.ExitCode
                $uninstallOk = if ($u.ExitCode -eq 0 -or $u.ExitCode -eq 3010) { 'yes' } else { 'no' }
            } catch { $uninstallOk = 'no'; $notes.Add("uninstall: $($_.Exception.Message)") }
        }
    }
    $noteStr = (($notes -join '; ') -replace '[\r\n,]+', ' ').Trim()
    $Rows.Add(('{0},cargo-packager,wix,{1},{2},{3},{4},{5},{6},{7}' -f $app, $t.Msi, $installExit, $installOk, $launch, $uninstallExit, $uninstallOk, $noteStr))
    Write-Host ("{0}: install={1} launch={2} uninstall={3}" -f $app, $installOk, $launch, $uninstallOk)
}

Write-Utf8NoBom $OutCsv (($Rows -join "`n") + "`n")
Write-Host "wrote $OutCsv"
exit 0
