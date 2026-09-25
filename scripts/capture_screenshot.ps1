# 截取 GalManager 主窗口，用于 README 配图
param(
    [string]$OutFile = "screenshots/library.png",
    [int]$DelayMs = 1500
)

Add-Type -AssemblyName System.Drawing
Add-Type -AssemblyName System.Windows.Forms

Add-Type @"
using System;
using System.Runtime.InteropServices;
public class Win32Api {
    [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr hWnd);
    [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr hWnd, int nCmdShow);
    [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr hWnd, out RECT lpRect);
    [StructLayout(LayoutKind.Sequential)]
    public struct RECT { public int Left, Top, Right, Bottom; }
}
"@

$proc = Get-Process -Name "GalManager" -ErrorAction SilentlyContinue |
    Where-Object { $_.MainWindowHandle -ne 0 } | Select-Object -First 1

if (-not $proc) {
    Write-Error "未找到 GalManager 主窗口，请先启动应用"
    exit 1
}

$handle = $proc.MainWindowHandle
[Win32Api]::ShowWindow($handle, 9) | Out-Null   # SW_RESTORE
[Win32Api]::SetForegroundWindow($handle) | Out-Null
Start-Sleep -Milliseconds $DelayMs

$rect = New-Object Win32Api+RECT
[Win32Api]::GetWindowRect($handle, [ref]$rect) | Out-Null

$width = $rect.Right - $rect.Left
$height = $rect.Bottom - $rect.Top
if ($width -le 0 -or $height -le 0) {
    Write-Error "窗口尺寸异常: ${width}x${height}"
    exit 1
}

$bitmap = New-Object System.Drawing.Bitmap $width, $height
$graphics = [System.Drawing.Graphics]::FromImage($bitmap)
$graphics.CopyFromScreen($rect.Left, $rect.Top, 0, 0, $bitmap.Size)

$full = Join-Path (Get-Location) $OutFile
$dir = Split-Path $full -Parent
if (-not (Test-Path $dir)) { New-Item -ItemType Directory -Path $dir -Force | Out-Null }

$bitmap.Save($full, [System.Drawing.Imaging.ImageFormat]::Png)
$graphics.Dispose()
$bitmap.Dispose()

Write-Output "已保存截图: $full (${width}x${height})"
