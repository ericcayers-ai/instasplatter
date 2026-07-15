# Install Depth Anything 3 into LocalAppData engines for InstaSplatter.
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$dest = Join-Path $env:LOCALAPPDATA "InstaSplatter\engines\sidecars\depth-anything-3"
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item (Join-Path $here "run.py") $dest -Force
Copy-Item (Join-Path $here "run.bat") $dest -Force
Copy-Item (Join-Path $here "README.md") $dest -Force -ErrorAction SilentlyContinue
$common = Join-Path (Split-Path $here -Parent) "_common"
if (Test-Path $common) {
  New-Item -ItemType Directory -Force (Join-Path $dest "_common") | Out-Null
  Copy-Item (Join-Path $common "*") (Join-Path $dest "_common") -Recurse -Force
}
Write-Host "Copied launcher to $dest"
Write-Host "Next: pip install torch opencv-python pillow onnxruntime (optional) and DA3 weights."
Write-Host "Place weights.pt / weights.onnx here, then: New-Item -ItemType File (Join-Path $dest 'ACCEPTED')"
Write-Host "Delete any leftover .stub marker after weights land."
