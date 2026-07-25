# Window capture + click helpers for Part F manual matrix.
param(
  [Parameter(Mandatory = $true)][string]$OutPath,
  [int]$PidHint = 0,
  [switch]$Activate
)

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinShot {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
  public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
  public const uint MOUSEEVENTF_LEFTUP = 0x0004;
  public const int SW_RESTORE = 9;
}
"@

function Find-InstaWindow {
  param([int]$PidHint)
  $procs = @()
  if ($PidHint -gt 0) {
    $procs += Get-Process -Id $PidHint -ErrorAction SilentlyContinue
  }
  $procs += Get-Process -ErrorAction SilentlyContinue | Where-Object {
    $_.ProcessName -match 'instasplatter' -or $_.MainWindowTitle -match 'InstaSplatter'
  }
  foreach ($p in ($procs | Sort-Object Id -Unique)) {
    if ($p.MainWindowHandle -ne [IntPtr]::Zero) { return $p }
  }
  return $null
}

$proc = Find-InstaWindow -PidHint $PidHint
if (-not $proc) { throw "InstaSplatter window not found" }

$hwnd = $proc.MainWindowHandle
if ([WinShot]::IsIconic($hwnd)) { [WinShot]::ShowWindow($hwnd, [WinShot]::SW_RESTORE) | Out-Null }

if ($Activate) {
  $fg = [WinShot]::GetForegroundWindow()
  $fgPid = 0
  $curTid = [WinShot]::GetCurrentThreadId()
  $fgTid = [WinShot]::GetWindowThreadProcessId($fg, [ref]$fgPid)
  $targetPid = 0
  $targetTid = [WinShot]::GetWindowThreadProcessId($hwnd, [ref]$targetPid)
  [WinShot]::AttachThreadInput($curTid, $fgTid, $true) | Out-Null
  [WinShot]::AttachThreadInput($curTid, $targetTid, $true) | Out-Null
  [WinShot]::SetForegroundWindow($hwnd) | Out-Null
  [WinShot]::AttachThreadInput($curTid, $fgTid, $false) | Out-Null
  [WinShot]::AttachThreadInput($curTid, $targetTid, $false) | Out-Null
  Start-Sleep -Milliseconds 250
}

$rect = New-Object WinShot+RECT
[WinShot]::GetWindowRect($hwnd, [ref]$rect) | Out-Null
$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
if ($width -lt 100 -or $height -lt 100) { throw "Window too small: ${width}x${height}" }

$bitmap = New-Object System.Drawing.Bitmap $width, $height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, (New-Object System.Drawing.Size $width, $height))
$dir = Split-Path -Parent $OutPath
if ($dir) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
$bitmap.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()

Write-Output ("captured pid={0} title={1} size={2}x{3} path={4}" -f $proc.Id, $proc.MainWindowTitle, $width, $height, $OutPath)
Write-Output ("rect left={0} top={1} right={2} bottom={3}" -f $rect.Left, $rect.Top, $rect.Right, $rect.Bottom)
