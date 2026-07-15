# InstaSplatter local smoke
param(
  [switch]$WithDevBatch,
  [string]$VideoPath = "",
  [switch]$SkipTauri
)

$ErrorActionPreference = "Stop"

$root = Split-Path -Parent (Split-Path -Parent $MyInvocation.MyCommand.Path)
Set-Location $root

Write-Host "== cargo test =="
Push-Location src-tauri
cargo test
if ($LASTEXITCODE -ne 0) { throw "cargo test failed" }
Pop-Location

Write-Host "== tsc =="
npx tsc --noEmit
if ($LASTEXITCODE -ne 0) { throw "tsc failed" }

Write-Host "== npm run build =="
npm run build
if ($LASTEXITCODE -ne 0) { throw "npm run build failed" }

Write-Host "== sidecar adapter dry fails (clear errors) =="
$py = "python"
$adapters = @(
  "tools\sidecars\depth-anything-3\run.py",
  "tools\sidecars\mapanything\run.py",
  "tools\sidecars\lightglue\run.py",
  "tools\sidecars\roma-v2\run.py",
  "tools\sidecars\anuga\run.py"
)
foreach ($a in $adapters) {
  if (-not (Test-Path $a)) { throw "missing $a" }
  '{}' | & $py $a 2>&1 | Out-Null
  $code = $LASTEXITCODE
  if ($code -eq 0) {
    Write-Warning "$a unexpectedly succeeded on empty request"
  } else {
    Write-Host "ok fail-clearly: $a (exit $code)"
  }
}

Write-Host "== experimental adapters still refuse without upstream =="
$exp = "tools\sidecars\vggt-omega\run.py"
'{"task":"densify","imagesDir":".","workspace":"."}' | & $py $exp 2>&1 | Out-Null
if ($LASTEXITCODE -eq 0) { throw "vggt-omega must not succeed without weights" }
Write-Host "ok: vggt-omega refuse ($LASTEXITCODE)"

if ($WithDevBatch) {
  if (-not $VideoPath -or -not (Test-Path $VideoPath)) {
    throw "-WithDevBatch requires -VideoPath to an existing media file"
  }
  $appData = Join-Path $env:LOCALAPPDATA "InstaSplatter"
  New-Item -ItemType Directory -Force $appData | Out-Null
  Set-Content -Path (Join-Path $appData "batch.txt") -Value $VideoPath -Encoding utf8
  $env:INSTASPLATTER_DEV = "1"
  Write-Host "Wrote batch.txt — launch a built instasplatter.exe to enqueue."
  $exe = Join-Path $root "src-tauri\target\release\instasplatter.exe"
  if (Test-Path $exe) {
    Write-Host "Starting $exe (detach)..."
    Start-Process $exe
  } else {
    Write-Warning "No release exe yet — run npm run tauri build first"
  }
}

Write-Host "SMOKE OK (full tauri build: npm run tauri build)"
