# Build and install a custom Brush binary for InstaSplatter

$ErrorActionPreference = "Stop"
$root = Split-Path -Parent $MyInvocation.MyCommand.Path
$src = Join-Path $root "src"
$dest = Join-Path $env:LOCALAPPDATA "InstaSplatter\engines\brush-custom"
$pin = "3b80985"

if (-not (Test-Path $src)) {
  git clone --depth 1 https://github.com/ArthurBrussee/brush.git $src
}

Push-Location $src
try {
  $head = git rev-parse --short HEAD
  Write-Host "Brush tree at $head (patches authored vs $pin)"

  $diffs = Get-ChildItem (Join-Path $root "patches\*.diff") -ErrorAction SilentlyContinue | Sort-Object Name
  foreach ($diff in $diffs) {
    Write-Host "Applying $($diff.Name)..."
    git apply --check $diff.FullName
    if ($LASTEXITCODE -ne 0) {
      Write-Warning "Patch $($diff.Name) does not apply cleanly - skipping"
    } else {
      git apply $diff.FullName
    }
  }

  Write-Host "Building brush-app (release) - this can take a long time..."
  cargo build --release -p brush-app
  if ($LASTEXITCODE -ne 0) { throw "cargo build failed" }

  $exe = Join-Path (Get-Location) "target\release\brush_app.exe"
  if (-not (Test-Path $exe)) {
    $found = Get-ChildItem "target\release\brush*.exe" -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($found) { $exe = $found.FullName }
  }
  if (-not $exe -or -not (Test-Path $exe)) { throw "brush_app.exe not found after build" }

  New-Item -ItemType Directory -Force $dest | Out-Null
  Copy-Item $exe (Join-Path $dest "brush_app.exe") -Force

  $patchNames = @()
  foreach ($diff in $diffs) { $patchNames += $diff.Name }
  $meta = [ordered]@{
    builtAt = (Get-Date).ToString("o")
    brushHead = (git rev-parse HEAD)
    patches = $patchNames
  }
  $meta | ConvertTo-Json | Set-Content (Join-Path $dest "INSTASPLATTER_BUILD.json") -Encoding utf8
  Write-Host "Installed custom Brush to $dest"
} finally {
  Pop-Location
}
