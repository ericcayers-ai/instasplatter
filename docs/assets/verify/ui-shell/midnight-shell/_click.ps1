# Click at client-relative coords inside InstaSplatter window, then optional capture.
param(
  [int]$X,
  [int]$Y,
  [string]$OutPath = "",
  [string]$Keys = "",
  [int]$SleepMs = 400,
  [switch]$DragTo,
  [int]$X2 = 0,
  [int]$Y2 = 0
)

Add-Type -AssemblyName System.Windows.Forms
Add-Type @"
using System;
using System.Runtime.InteropServices;
public class WinInput {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT rect);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
  [DllImport("user32.dll")] public static extern bool IsIconic(IntPtr hWnd);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int X, int Y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint dwFlags, uint dx, uint dy, uint dwData, UIntPtr dwExtraInfo);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr hWnd, out uint pid);
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint idAttach, uint idAttachTo, bool fAttach);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  public const uint MOUSEEVENTF_LEFTDOWN = 0x0002;
  public const uint MOUSEEVENTF_LEFTUP = 0x0004;
  public const int SW_RESTORE = 9;
}
"@

$proc = Get-Process | Where-Object { $_.ProcessName -eq 'instasplatter' -and $_.MainWindowHandle -ne [IntPtr]::Zero } | Select-Object -First 1
if (-not $proc) { throw "InstaSplatter not running" }
$hwnd = $proc.MainWindowHandle
if ([WinInput]::IsIconic($hwnd)) { [WinInput]::ShowWindow($hwnd, [WinInput]::SW_RESTORE) | Out-Null }

$fg = [WinInput]::GetForegroundWindow()
$fgPid = 0; $targetPid = 0
$curTid = [WinInput]::GetCurrentThreadId()
$fgTid = [WinInput]::GetWindowThreadProcessId($fg, [ref]$fgPid)
$targetTid = [WinInput]::GetWindowThreadProcessId($hwnd, [ref]$targetPid)
[WinInput]::AttachThreadInput($curTid, $fgTid, $true) | Out-Null
[WinInput]::AttachThreadInput($curTid, $targetTid, $true) | Out-Null
[WinInput]::SetForegroundWindow($hwnd) | Out-Null
[WinInput]::AttachThreadInput($curTid, $fgTid, $false) | Out-Null
[WinInput]::AttachThreadInput($curTid, $targetTid, $false) | Out-Null
Start-Sleep -Milliseconds 150

$rect = New-Object WinInput+RECT
[WinInput]::GetWindowRect($hwnd, [ref]$rect) | Out-Null

function Click-At([int]$cx, [int]$cy) {
  $sx = $rect.Left + $cx
  $sy = $rect.Top + $cy
  [WinInput]::SetCursorPos($sx, $sy) | Out-Null
  Start-Sleep -Milliseconds 40
  [WinInput]::mouse_event([WinInput]::MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
  Start-Sleep -Milliseconds 30
  [WinInput]::mouse_event([WinInput]::MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
}

if ($PSBoundParameters.ContainsKey('X')) {
  Click-At $X $Y
  if ($DragTo) {
    Start-Sleep -Milliseconds 80
    $sx2 = $rect.Left + $X2
    $sy2 = $rect.Top + $Y2
    [WinInput]::mouse_event([WinInput]::MOUSEEVENTF_LEFTDOWN, 0, 0, 0, [UIntPtr]::Zero)
    Start-Sleep -Milliseconds 40
    [WinInput]::SetCursorPos($sx2, $sy2) | Out-Null
    Start-Sleep -Milliseconds 120
    [WinInput]::mouse_event([WinInput]::MOUSEEVENTF_LEFTUP, 0, 0, 0, [UIntPtr]::Zero)
  }
}

if ($Keys) {
  Start-Sleep -Milliseconds 80
  [System.Windows.Forms.SendKeys]::SendWait($Keys)
}

Start-Sleep -Milliseconds $SleepMs

if ($OutPath) {
  & "$PSScriptRoot\_capture.ps1" -OutPath $OutPath -Activate
}
