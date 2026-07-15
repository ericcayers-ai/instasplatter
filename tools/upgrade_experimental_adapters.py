#!/usr/bin/env python3
"""Rewrite Experimental sidecar launchers as installable adapters.

Keeps .stub markers (host refuses ready) until the user installs upstream +
ACCEPTED/weights — but the launcher is no longer an empty refuse: it tries
real GitHub/upstream entrypoints and fails clearly when missing.
"""

from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parent / "sidecars"

TEMPLATE = '''#!/usr/bin/env python3
"""Experimental (NC) installable adapter for {name}.

Host refuses this sidecar unless Experimental Mode is ON.
Keep the `.stub` marker until upstream + weights/ACCEPTED are present; delete
`.stub` only after a successful dry-run produces real artifacts.

Protocol: JSON stdin → PLY path / sparse path / OK on stdout.
Fails clearly when weights / upstream are missing — never invents outputs.
"""

from __future__ import annotations

import shutil
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
NAME = HERE.name
sys.path.insert(0, str(HERE.parent / "_common"))
sys.path.insert(0, str(HERE / "_common"))

from adapter_util import fail, list_images, marker_ready, read_request  # noqa: E402

INSTALL = (
    "Clone upstream under ./upstream (see README), install NC weights per "
    "upstream LICENSE, touch ACCEPTED, then delete .stub after a dry-run. "
    "Optional: provide run_upstream.py next to this launcher."
)

UPSTREAM_SCRIPTS = (
    "run_upstream.py",
    "demo.py",
    "demo_colmap.py",
    "infer.py",
    "run.py",
    "scripts/demo.py",
    "scripts/demo_colmap.py",
    "scripts/infer.py",
)


def find_script() -> Path | None:
    for rel in UPSTREAM_SCRIPTS:
        for base in (HERE, HERE / "upstream", HERE / "repo"):
            p = base / rel if rel != "run.py" or base != HERE else None
            # Never recurse into our own run.py
            if rel == "run.py" and base == HERE:
                continue
            cand = base / rel
            if cand.exists():
                return cand
    return None


def run_script(script: Path, images_dir: Path, workspace: Path, task: str, splat: str | None) -> int:
    out = workspace / NAME.replace("-", "_")
    out.mkdir(parents=True, exist_ok=True)
    cmd = [
        sys.executable,
        str(script),
        "--images",
        str(images_dir),
        "--output",
        str(out),
        "--task",
        task,
    ]
    if splat:
        cmd.extend(["--splat", splat])
    proc = subprocess.run(cmd, capture_output=True, text=True)
    if proc.returncode != 0:
        return fail(proc.stderr.strip() or proc.stdout.strip() or f"{{NAME}} upstream failed")
    if task in ("sfm", "pose"):
        sparse = out / "sparse" / "0"
        if sparse.exists():
            dest = workspace / "sparse" / "0"
            if dest.exists():
                shutil.rmtree(dest)
            dest.parent.mkdir(parents=True, exist_ok=True)
            shutil.copytree(sparse, dest)
            print(str(dest.resolve()))
            return 0
        print("OK")
        return 0
    if task == "polish" and splat:
        for p in out.rglob("*.ply"):
            print(str(p.resolve()))
            return 0
    for p in out.rglob("*.ply"):
        print(str(p.resolve()))
        return 0
    line = next(
        (l.strip() for l in proc.stdout.splitlines() if l.strip() and not l.startswith("#")),
        "",
    )
    if line:
        print(line)
        return 0
    return fail(f"{{NAME}} upstream produced no output artifacts")


def main() -> int:
    req = read_request()
    task = (req.get("task") or "densify").lower()
    if not marker_ready(HERE):
        return fail(
            f"{{NAME}}: weights/upstream not installed. {{INSTALL}}",
            2,
        )
    images_dir = Path(req.get("imagesDir") or req.get("images_dir") or ".")
    workspace = Path(req.get("workspace") or ".")
    splat = req.get("splatPath") or req.get("splat_path")
    if task != "polish" and images_dir.exists() and not list_images(images_dir):
        # Some surface adapters only need splat input.
        if not splat:
            return fail("no images found")

    script = find_script()
    if script is None:
        return fail(
            f"{{NAME}}: no upstream entrypoint found under ./upstream or run_upstream.py. {{INSTALL}}",
            2,
        )
    try:
        return run_script(script, images_dir, workspace, task, splat)
    except Exception as e:
        return fail(f"{{NAME}} unavailable: {{e}}")


if __name__ == "__main__":
    raise SystemExit(main())
'''

EXPERIMENTAL = [
    "vggt-omega",
    "dust3r",
    "mast3r",
    "pi3x",
    "stream-vggt",
    "vggt-long",
    "mast3r-slam",
    "slam3r",
    "monst3r",
    "easi3r",
    "city-gaussian",
    "urban-gs",
    "horizon-gs",
    "gs-2d",
    "gof",
    "pgsr",
    "rade-gs",
    "sugar",
    "milo",
    "difix",
]

README_EXTRA = """
## Installable adapter (v0.8.1+)

1. Clone the upstream project into `./upstream` (see project URL in RESEARCH-STACK).
2. Install NC weights per upstream LICENSE (Experimental Mode only).
3. Optionally add `run_upstream.py` if demo entrypoints differ.
4. Create `ACCEPTED` after reviewing terms.
5. Dry-run once, then **delete `.stub`** so the host marks the sidecar ready.
6. Copy to `%LOCALAPPDATA%/InstaSplatter/engines/sidecars/{name}/` (or use install.ps1).

The launcher fails clearly when weights/upstream are missing — it never invents PLY/poses.
"""


def main() -> None:
    for name in EXPERIMENTAL:
        d = ROOT / name
        d.mkdir(exist_ok=True)
        (d / "run.py").write_text(TEMPLATE.format(name=name), encoding="utf-8")
        (d / "run.bat").write_text('@echo off\npython "%~dp0run.py" %*\n', encoding="utf-8")
        install = f'''$ErrorActionPreference = "Stop"
$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$dest = Join-Path $env:LOCALAPPDATA "InstaSplatter\\engines\\sidecars\\{name}"
New-Item -ItemType Directory -Force $dest | Out-Null
Copy-Item (Join-Path $here "run.py") $dest -Force
Copy-Item (Join-Path $here "run.bat") $dest -Force
Copy-Item (Join-Path $here "README.md") $dest -Force -ErrorAction SilentlyContinue
if (Test-Path (Join-Path $here ".stub")) {{ Copy-Item (Join-Path $here ".stub") $dest -Force }}
$common = Join-Path (Split-Path $here -Parent) "_common"
if (Test-Path $common) {{
  New-Item -ItemType Directory -Force (Join-Path $dest "_common") | Out-Null
  Copy-Item (Join-Path $common "*") (Join-Path $dest "_common") -Recurse -Force
}}
Write-Host "Copied {name} adapter to $dest (still .stub until upstream+weights)."
'''
        (d / "install.ps1").write_text(install, encoding="utf-8")
        # Ensure .stub remains
        stub = d / ".stub"
        if not stub.exists():
            stub.write_text(
                "installable adapter — delete after upstream+weights dry-run\n",
                encoding="utf-8",
            )
        readme = d / "README.md"
        body = readme.read_text(encoding="utf-8") if readme.exists() else f"# {name} (Experimental / NC)\n"
        if "Installable adapter (v0.8.1+" not in body:
            body = body.rstrip() + "\n" + README_EXTRA.replace("{name}", name)
            readme.write_text(body + "\n", encoding="utf-8")
        print(f"updated {name}")


if __name__ == "__main__":
    main()
