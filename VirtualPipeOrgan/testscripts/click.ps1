# Klik/scroll in het JM-Orgue-venster met venster-relatieve coördinaten (zoals in de
# PrintWindow-screenshots). Gebruik: click.ps1 -X 1006 -Y 57 [-Wheel -3] [-Screenshot uit.png]
param([int]$X = -1, [int]$Y = -1, [int]$Wheel = 0, [string]$Screenshot = "")
Add-Type @"
using System; using System.Runtime.InteropServices;
public class U {
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte scan, uint flags, UIntPtr extra);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out R r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, uint dx, uint dy, int data, UIntPtr extra);
  [StructLayout(LayoutKind.Sequential)] public struct R { public int L, T, Rt, B; }
}
"@
$p = Get-Process vpo-app -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $p) { Write-Error "geen vpo-app venster"; exit 1 }
$h = $p.MainWindowHandle
[U]::ShowWindow($h, 9) | Out-Null
# Alt-truc: een gesimuleerde Alt-druk maakt SetForegroundWindow toegestaan vanuit een achtergrondproces.
[U]::keybd_event(0x12, 0, 0, [UIntPtr]::Zero); [U]::SetForegroundWindow($h) | Out-Null; [U]::keybd_event(0x12, 0, 2, [UIntPtr]::Zero)
try { (New-Object -ComObject WScript.Shell).AppActivate($p.Id) | Out-Null } catch {}
Start-Sleep -Milliseconds 400
if ([U]::GetForegroundWindow() -ne $h) { Write-Output "LET OP: venster niet op de voorgrond" }
$r = New-Object U+R; [U]::GetWindowRect($h, [ref]$r) | Out-Null
if ($X -ge 0 -and $Y -ge 0) {
  $sx = $r.L + $X; $sy = $r.T + $Y
  [U]::SetCursorPos($sx, $sy) | Out-Null
  Start-Sleep -Milliseconds 120
  if ($Wheel -ne 0) {
    [U]::mouse_event(0x0800, 0, 0, $Wheel * 120, [UIntPtr]::Zero)
  } else {
    [U]::mouse_event(0x0002, 0, 0, 0, [UIntPtr]::Zero); Start-Sleep -Milliseconds 60
    [U]::mouse_event(0x0004, 0, 0, 0, [UIntPtr]::Zero)
  }
  Write-Output "actie op venster ($X,$Y) = scherm ($sx,$sy) wheel=$Wheel"
}
if ($Screenshot) {
  Start-Sleep -Milliseconds 700
  & "$PSScriptRoot\capture_window.ps1" $Screenshot | Out-Null
  Write-Output "screenshot: $Screenshot"
}
