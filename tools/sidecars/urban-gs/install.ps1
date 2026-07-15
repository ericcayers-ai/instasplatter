$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$dest = Join-Path $env:LOCALAPPDATA "InstaSplatter\engines\sidecars\urban-gs"
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item (Join-Path $here "run.py") $dest -Force
Copy-Item (Join-Path $here "run.bat") $dest -Force
Copy-Item (Join-Path $here "README.md") $dest -Force -ErrorAction SilentlyContinue
if (Test-Path (Join-Path $here ".stub")) { Copy-Item (Join-Path $here ".stub") $dest -Force }
$common = Join-Path (Split-Path $here -Parent) "_common"
if (Test-Path $common) {
  New-Item -ItemType Directory -Force (Join-Path $dest "_common") | Out-Null
  Copy-Item (Join-Path $common "*") (Join-Path $dest "_common") -Recurse -Force
}
Write-Host "Copied urban-gs adapter to $dest (still .stub until upstream+weights)."
