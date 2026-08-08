#Requires -Version 7.0
<#
  windows/window-count.ps1 -- enumerate visible top-level windows.

  Prints one TSV row per qualifying window to stdout:
      pid<TAB>hwnd<TAB>width<TAB>height<TAB>title
  A window qualifies if it is visible AND (has a non-empty title OR is
  non-trivially sized: width > 50 and height > 50 -- some GUI frameworks set
  no title). With -TargetPid nonzero, only rows whose pid is in
  ($TargetPid + $ExtraPids) are printed. Always exits 0; callers count lines.

  Usage: pwsh -NoProfile -File windows/window-count.ps1 [-TargetPid <pid>] [-ExtraPids <pid,...>]
#>
[CmdletBinding()]
param(
    [int]$TargetPid = 0,
    [int[]]$ExtraPids = @()
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Write-Utf8NoBom([string]$Path,[string]$Content) { [IO.File]::WriteAllText($Path, $Content.Replace("`r`n","`n"), [Text.UTF8Encoding]::new($false)) }

try {
    if (-not ('GuiEcoWinEnum' -as [type])) {
        Add-Type -TypeDefinition @'
using System;
using System.Runtime.InteropServices;
using System.Text;

public static class GuiEcoWinEnum
{
    public delegate bool EnumWindowsProc(IntPtr hWnd, IntPtr lParam);

    [StructLayout(LayoutKind.Sequential)]
    public struct RECT
    {
        public int Left;
        public int Top;
        public int Right;
        public int Bottom;
    }

    [DllImport("user32.dll", SetLastError = true)]
    public static extern bool EnumWindows(EnumWindowsProc lpEnumFunc, IntPtr lParam);

    [DllImport("user32.dll")]
    public static extern bool IsWindowVisible(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint lpdwProcessId);

    [DllImport("user32.dll", CharSet = CharSet.Unicode)]
    public static extern int GetWindowTextW(IntPtr hWnd, StringBuilder lpString, int nMaxCount);

    [DllImport("user32.dll")]
    public static extern int GetWindowTextLengthW(IntPtr hWnd);

    [DllImport("user32.dll")]
    public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
}
'@
    }

    $script:AllowedPids = $null
    if ($TargetPid -ne 0) {
        $script:AllowedPids = [System.Collections.Generic.HashSet[uint32]]::new()
        [void]$script:AllowedPids.Add([uint32]$TargetPid)
        foreach ($extra in $ExtraPids) {
            if ($extra -gt 0) { [void]$script:AllowedPids.Add([uint32]$extra) }
        }
    }

    $script:Rows = [System.Collections.Generic.List[string]]::new()

    $callback = [GuiEcoWinEnum+EnumWindowsProc] {
        param([IntPtr]$hwnd, [IntPtr]$lparam)
        try {
            if (-not [GuiEcoWinEnum]::IsWindowVisible($hwnd)) { return $true }

            $windowPid = [uint32]0
            [void][GuiEcoWinEnum]::GetWindowThreadProcessId($hwnd, [ref]$windowPid)
            if ($null -ne $script:AllowedPids -and -not $script:AllowedPids.Contains($windowPid)) { return $true }

            $title = ''
            $len = [GuiEcoWinEnum]::GetWindowTextLengthW($hwnd)
            if ($len -gt 0) {
                $sb = [System.Text.StringBuilder]::new($len + 1)
                [void][GuiEcoWinEnum]::GetWindowTextW($hwnd, $sb, $sb.Capacity)
                $title = $sb.ToString()
            }

            $width = 0
            $height = 0
            $rect = [GuiEcoWinEnum+RECT]::new()
            if ([GuiEcoWinEnum]::GetWindowRect($hwnd, [ref]$rect)) {
                $width = $rect.Right - $rect.Left
                $height = $rect.Bottom - $rect.Top
            }

            $hasTitle = $title.Trim().Length -gt 0
            $nonTrivial = ($width -gt 50) -and ($height -gt 50)
            if ($hasTitle -or $nonTrivial) {
                $cleanTitle = $title -replace "[\t\r\n]", ' '
                $script:Rows.Add(("{0}`t{1}`t{2}`t{3}`t{4}" -f $windowPid, $hwnd.ToInt64(), $width, $height, $cleanTitle))
            }
        } catch {
            # never let a single window abort the enumeration
        }
        return $true
    }

    [void][GuiEcoWinEnum]::EnumWindows($callback, [IntPtr]::Zero)

    foreach ($row in $script:Rows) { Write-Output $row }
} catch {
    Write-Warning "window-count failed: $($_.Exception.Message)"
}
exit 0
