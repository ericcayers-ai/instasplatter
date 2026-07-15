# Install LightGlue matcher into LocalAppData engines.
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$dest = Join-Path $env:LOCALAPPDATA "InstaSplatter\engines\sidecars\lightglue"
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
Write-Host "pip install lightglue torch kornia opencv-python — or clone cvg/LightGlue to upstream\"
Write-Host "Then create ACCEPTED in $dest"
