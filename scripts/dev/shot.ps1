# Screenshot a window by process name. Usage: .\scripts\shot.ps1 <procName> <outPath>
param(
  [string]$ProcName = "linkfyr-desktop",
  [string]$OutPath = "$env:TEMP\opencode\linkfyr-shot.png"
)
$ErrorActionPreference = "Stop"
Add-Type -AssemblyName System.Windows.Forms,System.Drawing
if (-not ("LF.Native" -as [type])) {
  Add-Type -Namespace LF -Name Native -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int cmd);
public struct RECT { public int L; public int T; public int R; public int B; }
'@
}
$proc = Get-Process $ProcName -ErrorAction SilentlyContinue | Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1
if (-not $proc) { throw "no window for $ProcName" }
[LF.Native]::ShowWindow($proc.MainWindowHandle, 9) | Out-Null   # SW_RESTORE
[LF.Native]::SetForegroundWindow($proc.MainWindowHandle) | Out-Null
Start-Sleep -Milliseconds 1200
$r = New-Object LF.Native+RECT
[LF.Native]::GetWindowRect($proc.MainWindowHandle, [ref]$r) | Out-Null
$w = $r.R - $r.L; $h = $r.B - $r.T
if ($w -lt 200 -or $h -lt 200) { throw "window too small ${w}x${h}" }
$bmp = New-Object System.Drawing.Bitmap($w, $h)
$g = [System.Drawing.Graphics]::FromImage($bmp)
$g.CopyFromScreen($r.L, $r.T, 0, 0, $bmp.Size)
$dir = Split-Path $OutPath -Parent
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Force -Path $dir | Out-Null }
$bmp.Save($OutPath, [System.Drawing.Imaging.ImageFormat]::Png)
Write-Output "saved $OutPath ${w}x${h}"
