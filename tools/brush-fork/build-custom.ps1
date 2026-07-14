# Build and install a custom Brush binary for InstaSplatter

```powershell
$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$src = Join-Path $root "src"
$dest = Join-Path $env:LOCALAPPDATA "InstaSplatter\engines\brush-custom"

if (-not (Test-Path $src)) {
  git clone --depth 1 https://github.com/ArthurBrussee/brush.git $src
}

Push-Location $src
try {
  # Apply ordered patches when *.diff files appear under ../patches
  Get-ChildItem (Join-Path $root "patches\*.diff") -ErrorAction SilentlyContinue | Sort-Object Name | ForEach-Object {
    Write-Host "Applying $($_.Name)..."
    git apply $_.FullName
  }
  cargo build --release -p brush-app
  New-Item -ItemType Directory -Force $dest | Out-Null
  Copy-Item "target\release\brush_app.exe" (Join-Path $dest "brush_app.exe") -Force
  Write-Host "Installed custom Brush to $dest"
} finally {
  Pop-Location
}
```
