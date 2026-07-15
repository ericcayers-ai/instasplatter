# Install Fixer polish launcher into LocalAppData engines.
$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$dest = Join-Path $env:LOCALAPPDATA "InstaSplatter\engines\sidecars\fixer"
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item (Join-Path $here "run.py") $dest -Force
Copy-Item (Join-Path $here "run.bat") $dest -Force
$common = Join-Path (Split-Path $here -Parent) "_common"
if (Test-Path $common) {
  New-Item -ItemType Directory -Force (Join-Path $dest "_common") | Out-Null
  Copy-Item (Join-Path $common "*") (Join-Path $dest "_common") -Recurse -Force
}
Write-Host "Copied Fixer launcher to $dest"
