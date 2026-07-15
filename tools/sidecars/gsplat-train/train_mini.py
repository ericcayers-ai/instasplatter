#!/usr/bin/env python3
"""Minimal COLMAP→gsplat trainer used when simple_trainer.py is unavailable.

Depends only on: torch, gsplat, numpy, pillow (and pycolmap OR a tiny
cameras/images/points3D.txt reader). Apache-2.0.

Exports `export_<iter>.ply` into --export_dir and prints STEP lines.
"""

from __future__ import annotations

import argparse
import math
import struct
from pathlib import Path

import numpy as np
import torch
import torch.nn.functional as F
from PIL import Image

try:
    from gsplat import rasterization
    from gsplat.exporter import export_splats
    from gsplat.strategy import DefaultStrategy, MCMCStrategy
except ImportError as e:
    raise SystemExit(
        f"gsplat is not installed ({e}). pip install gsplat torch"
    ) from e


SH_C0 = 0.28209479177387814


def read_colmap_txt(sparse: Path):
    """Very small COLMAP text reader for cameras/images/points3D."""
    cams = {}
    for line in (sparse / "cameras.txt").read_text(encoding="utf-8", errors="ignore").splitlines():
        if not line or line.startswith("#"):
            continue
        p = line.split()
        cid = int(p[0])
        model = p[1]
        w, h = int(p[2]), int(p[3])
        params = list(map(float, p[4:]))
        if model in ("SIMPLE_PINHOLE", "SIMPLE_RADIAL"):
            fx = fy = params[0]
            cx, cy = params[1], params[2]
        else:
            fx, fy, cx, cy = params[0], params[1], params[2], params[3]
        cams[cid] = (w, h, fx, fy, cx, cy)

    images = []
    for line in (sparse / "images.txt").read_text(encoding="utf-8", errors="ignore").splitlines():
        if not line or line.startswith("#"):
            continue
        p = line.split()
        if len(p) < 10 or not p[0].isdigit():
            continue
        # IMAGE_ID, qw,qx,qy,qz, tx,ty,tz, CAMERA_ID, NAME
        q = np.array(list(map(float, p[1:5])), dtype=np.float64)
        t = np.array(list(map(float, p[5:8])), dtype=np.float64)
        cam_id = int(p[8])
        name = " ".join(p[9:])
        images.append((name, cam_id, q, t))
        # skip next points2D line
    # Re-parse properly alternating lines:
    images = []
    lines = [
        ln
        for ln in (sparse / "images.txt").read_text(encoding="utf-8", errors="ignore").splitlines()
        if ln and not ln.startswith("#")
    ]
    i = 0
    while i < len(lines):
        p = lines[i].split()
        if len(p) >= 10 and p[0].isdigit():
            q = np.array(list(map(float, p[1:5])), dtype=np.float64)
            t = np.array(list(map(float, p[5:8])), dtype=np.float64)
            cam_id = int(p[8])
            name = " ".join(p[9:])
            images.append((name, cam_id, q, t))
            i += 2
        else:
            i += 1

    pts = []
    colors = []
    pts_path = sparse / "points3D.txt"
    if pts_path.exists():
        for line in pts_path.read_text(encoding="utf-8", errors="ignore").splitlines():
            if not line or line.startswith("#"):
                continue
            p = line.split()
            if len(p) < 7:
                continue
            pts.append([float(p[1]), float(p[2]), float(p[3])])
            colors.append([int(p[4]), int(p[5]), int(p[6])])
    return cams, images, np.asarray(pts, dtype=np.float32), np.asarray(colors, dtype=np.uint8)


def qvec_to_rotmat(q):
    w, x, y, z = q
    return np.array(
        [
            [1 - 2 * y * y - 2 * z * z, 2 * x * y - 2 * z * w, 2 * x * z + 2 * y * w],
            [2 * x * y + 2 * z * w, 1 - 2 * x * x - 2 * z * z, 2 * y * z - 2 * x * w],
            [2 * x * z - 2 * y * w, 2 * y * z + 2 * x * w, 1 - 2 * x * x - 2 * y * y],
        ],
        dtype=np.float32,
    )


def load_views(data_dir: Path, device: torch.device, max_side: int = 1280):
    sparse = data_dir / "sparse" / "0"
    if not (sparse / "images.txt").exists():
        # binary models: ask user to convert; try txt only for mini trainer
        raise SystemExit("gsplat mini trainer needs sparse/0/*.txt (COLMAP text model)")
    cams, images, pts, cols = read_colmap_txt(sparse)
    views = []
    for name, cam_id, q, t in images:
        w, h, fx, fy, cx, cy = cams[cam_id]
        path = data_dir / "images" / name
        if not path.exists():
            # try basename match
            hits = list((data_dir / "images").glob(Path(name).name))
            if not hits:
                continue
            path = hits[0]
        img = Image.open(path).convert("RGB")
        scale = 1.0
        if max(img.size) > max_side:
            scale = max_side / max(img.size)
            img = img.resize((int(img.width * scale), int(img.height * scale)), Image.BICUBIC)
        arr = torch.from_numpy(np.asarray(img).astype(np.float32) / 255.0).to(device)
        R = qvec_to_rotmat(q)
        # world-to-camera
        viewmat = np.eye(4, dtype=np.float32)
        viewmat[:3, :3] = R
        viewmat[:3, 3] = t
        K = torch.tensor(
            [[fx * scale, 0, cx * scale], [0, fy * scale, cy * scale], [0, 0, 1]],
            dtype=torch.float32,
            device=device,
        )
        views.append(
            {
                "image": arr,
                "viewmat": torch.from_numpy(viewmat).to(device),
                "K": K,
                "width": arr.shape[1],
                "height": arr.shape[0],
            }
        )
    return views, pts, cols


def load_gaussian_ply(path: Path):
    """Load xyz + rgb (and optional scales) from a dense-init / Gaussian PLY."""
    raw = path.read_bytes()
    header_end = raw.find(b"end_header")
    if header_end < 0:
        raise ValueError("not a PLY")
    nl = raw.find(b"\n", header_end)
    header = raw[:nl].decode("ascii", errors="ignore")
    data = raw[nl + 1 :]
    props = []
    vertex_count = 0
    binary = "binary_little_endian" in header
    in_vertex = False
    sizes = {
        "float": 4,
        "float32": 4,
        "double": 8,
        "float64": 8,
        "uchar": 1,
        "uint8": 1,
        "char": 1,
        "int8": 1,
        "ushort": 2,
        "uint16": 2,
        "short": 2,
        "int16": 2,
        "int": 4,
        "uint": 4,
        "int32": 4,
        "uint32": 4,
    }
    offset = 0
    for line in header.splitlines():
        parts = line.split()
        if not parts:
            continue
        if parts[0] == "element":
            in_vertex = parts[1] == "vertex"
            if in_vertex:
                vertex_count = int(parts[2])
            continue
        if parts[0] == "property" and in_vertex and len(parts) >= 3:
            typ = parts[1]
            name = parts[2]
            props.append((name, offset, sizes.get(typ, 4)))
            offset += sizes.get(typ, 4)
    stride = offset
    if stride == 0 or vertex_count == 0:
        raise ValueError("empty PLY")
    find = {n: (o, s) for n, o, s in props}

    def read_f32(base, off):
        return struct.unpack_from("<f", data, base + off)[0]

    def read_u8(base, off):
        return data[base + off]

    if not binary:
        # ASCII fallback: sparse xyzrgb only.
        lines = data.decode("utf-8", errors="ignore").splitlines()
        pts, cols = [], []
        for line in lines[:vertex_count]:
            p = line.split()
            if len(p) < 3:
                continue
            pts.append([float(p[0]), float(p[1]), float(p[2])])
            if len(p) >= 6:
                cols.append([int(float(p[3])), int(float(p[4])), int(float(p[5]))])
            else:
                cols.append([180, 180, 180])
        return (
            np.asarray(pts, dtype=np.float32),
            np.asarray(cols, dtype=np.uint8),
            None,
            None,
            None,
        )

    pts = np.zeros((vertex_count, 3), dtype=np.float32)
    cols = np.full((vertex_count, 3), 180, dtype=np.uint8)
    scales = None
    quats = None
    opac = None
    if "scale_0" in find:
        scales = np.zeros((vertex_count, 3), dtype=np.float32)
    if "rot_0" in find:
        quats = np.zeros((vertex_count, 4), dtype=np.float32)
    if "opacity" in find:
        opac = np.zeros((vertex_count,), dtype=np.float32)

    for i in range(vertex_count):
        base = i * stride
        if base + stride > len(data):
            pts = pts[:i]
            cols = cols[:i]
            if scales is not None:
                scales = scales[:i]
            if quats is not None:
                quats = quats[:i]
            if opac is not None:
                opac = opac[:i]
            break
        pts[i, 0] = read_f32(base, find["x"][0])
        pts[i, 1] = read_f32(base, find["y"][0])
        pts[i, 2] = read_f32(base, find["z"][0])
        if "red" in find:
            cols[i, 0] = read_u8(base, find["red"][0])
            cols[i, 1] = read_u8(base, find["green"][0])
            cols[i, 2] = read_u8(base, find["blue"][0])
        elif "f_dc_0" in find:
            # SH DC → approximate RGB.
            for c, name in enumerate(("f_dc_0", "f_dc_1", "f_dc_2")):
                dc = read_f32(base, find[name][0])
                cols[i, c] = int(np.clip((0.5 + SH_C0 * dc) * 255.0, 0, 255))
        if scales is not None:
            scales[i, 0] = read_f32(base, find["scale_0"][0])
            scales[i, 1] = read_f32(base, find["scale_1"][0])
            scales[i, 2] = read_f32(base, find["scale_2"][0])
        if quats is not None:
            quats[i, 0] = read_f32(base, find["rot_0"][0])
            quats[i, 1] = read_f32(base, find["rot_1"][0])
            quats[i, 2] = read_f32(base, find["rot_2"][0])
            quats[i, 3] = read_f32(base, find["rot_3"][0])
        if opac is not None:
            opac[i] = read_f32(base, find["opacity"][0])
    return pts, cols, scales, quats, opac


def init_gaussians(pts, cols, device, init_ply: Path | None, sh_degree: int):
    scales_log = None
    quats_np = None
    opac_logit = None
    if init_ply and init_ply.exists():
        try:
            pts_i, cols_i, scales_i, quats_i, opac_i = load_gaussian_ply(init_ply)
            if pts_i.shape[0] >= 32:
                pts, cols = pts_i, cols_i
                scales_log = scales_i
                quats_np = quats_i
                opac_logit = opac_i
                print(f"# init.ply loaded: {pts.shape[0]} gaussians from {init_ply}", flush=True)
            else:
                print(f"# init.ply too small ({pts_i.shape[0]}); using COLMAP points", flush=True)
        except Exception as e:
            print(f"# init.ply load failed ({e}); using COLMAP points", flush=True)
    if pts.size == 0:
        pts = np.random.randn(10_000, 3).astype(np.float32) * 0.5
        cols = np.full((pts.shape[0], 3), 128, dtype=np.uint8)
    means = torch.nn.Parameter(torch.from_numpy(pts).to(device))
    n = means.shape[0]
    rgb = torch.from_numpy(cols.astype(np.float32) / 255.0).to(device)
    sh0 = ((rgb - 0.5) / SH_C0).unsqueeze(1)
    shN = torch.zeros(n, (sh_degree + 1) ** 2 - 1, 3, device=device)
    if scales_log is not None:
        scales = torch.nn.Parameter(torch.from_numpy(scales_log).to(device))
    else:
        scales = torch.nn.Parameter(torch.full((n, 3), math.log(0.01), device=device))
    if quats_np is not None:
        quats = torch.nn.Parameter(torch.from_numpy(quats_np).to(device))
    else:
        quats = torch.nn.Parameter(
            torch.tensor([[1, 0, 0, 0]], dtype=torch.float32, device=device).repeat(n, 1)
        )
    if opac_logit is not None:
        opacities = torch.nn.Parameter(torch.from_numpy(opac_logit).to(device))
    else:
        opacities = torch.nn.Parameter(torch.logit(torch.full((n,), 0.1, device=device)))
    sh0 = torch.nn.Parameter(sh0)
    shN = torch.nn.Parameter(shN)
    params = {
        "means": means,
        "scales": scales,
        "quats": quats,
        "opacities": opacities,
        "sh0": sh0,
        "shN": shN,
    }
    optimizers = {
        k: torch.optim.Adam([v], lr=lr)
        for k, v, lr in [
            ("means", means, 1.6e-4),
            ("scales", scales, 5e-3),
            ("quats", quats, 1e-3),
            ("opacities", opacities, 5e-2),
            ("sh0", sh0, 2.5e-3),
            ("shN", shN, 2.5e-3 / 20),
        ]
    }
    return params, optimizers


def ssim_loss(pred, gt):
    # Cheap luminance SSIM surrogate.
    c1, c2 = 0.01**2, 0.03**2
    mu_x = F.avg_pool2d(pred.permute(2, 0, 1).unsqueeze(0), 3, 1, 1)
    mu_y = F.avg_pool2d(gt.permute(2, 0, 1).unsqueeze(0), 3, 1, 1)
    sigma_x = F.avg_pool2d(pred.permute(2, 0, 1).unsqueeze(0) ** 2, 3, 1, 1) - mu_x**2
    sigma_y = F.avg_pool2d(gt.permute(2, 0, 1).unsqueeze(0) ** 2, 3, 1, 1) - mu_y**2
    sigma_xy = (
        F.avg_pool2d(
            (pred * gt).permute(2, 0, 1).unsqueeze(0),
            3,
            1,
            1,
        )
        - mu_x * mu_y
    )
    ssim = ((2 * mu_x * mu_y + c1) * (2 * sigma_xy + c2)) / (
        (mu_x**2 + mu_y**2 + c1) * (sigma_x + sigma_y + c2) + 1e-8
    )
    return 1.0 - ssim.mean()


@torch.no_grad()
def write_ply(path: Path, params):
    means = params["means"].detach().cpu().numpy()
    scales = params["scales"].detach().exp().cpu().numpy()
    quats = F.normalize(params["quats"].detach(), dim=-1).cpu().numpy()
    opac = torch.sigmoid(params["opacities"].detach()).cpu().numpy()
    sh0 = params["sh0"].detach().cpu().numpy()[:, 0, :]
    n = means.shape[0]
    # Minimal 3DGS PLY compatible with Brush / InstaSplatter.
    header = f"""ply
format binary_little_endian 1.0
element vertex {n}
property float x
property float y
property float z
property float nx
property float ny
property float nz
property float f_dc_0
property float f_dc_1
property float f_dc_2
property float opacity
property float scale_0
property float scale_1
property float scale_2
property float rot_0
property float rot_1
property float rot_2
property float rot_3
end_header
"""
    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "wb") as f:
        f.write(header.encode("ascii"))
        for i in range(n):
            f.write(struct.pack("<fff", *means[i]))
            f.write(struct.pack("<fff", 0.0, 0.0, 0.0))
            f.write(struct.pack("<fff", *sh0[i]))
            # store logit opacity like 3DGS
            o = float(np.log(opac[i] / max(1e-6, 1 - opac[i])))
            f.write(struct.pack("<f", o))
            f.write(struct.pack("<fff", *np.log(np.maximum(scales[i], 1e-8))))
            f.write(struct.pack("<ffff", quats[i, 0], quats[i, 1], quats[i, 2], quats[i, 3]))


def train(args):
    device = torch.device("cuda" if torch.cuda.is_available() else "cpu")
    if device.type != "cuda":
        raise SystemExit("gsplat mini trainer requires CUDA")
    views, pts, cols = load_views(Path(args.data_dir), device)
    if not views:
        raise SystemExit("no training views")
    params, optimizers = init_gaussians(pts, cols, device, Path(args.init_ply) if args.init_ply else None, args.sh_degree)

    if args.strategy == "mcmc":
        strategy = MCMCStrategy(cap_max=args.max_splats, verbose=False)
    else:
        strategy = DefaultStrategy(absgrad=args.absgrad, verbose=False)
    strategy.check_sanity(params, optimizers)
    state = strategy.initialize_state(
        **({"scene_scale": 1.0} if args.strategy == "default" else {})
    )

    export_dir = Path(args.export_dir)
    export_dir.mkdir(parents=True, exist_ok=True)
    sh_degree = args.sh_degree

    for step in range(1, args.max_steps + 1):
        view = views[step % len(views)]
        colors = torch.cat([params["sh0"], params["shN"]], dim=1)
        render_mode = "RGB"
        raster_kwargs = dict(
            means=params["means"],
            quats=F.normalize(params["quats"], dim=-1),
            scales=torch.exp(params["scales"]),
            opacities=torch.sigmoid(params["opacities"]),
            colors=colors,
            viewmats=view["viewmat"].unsqueeze(0),
            Ks=view["K"].unsqueeze(0),
            width=view["width"],
            height=view["height"],
            sh_degree=sh_degree,
            render_mode=render_mode,
            packed=False,
            absgrad=args.absgrad and args.strategy == "default",
            rasterize_mode="antialiased" if args.antialiased else "classic",
        )
        if args.strategy == "default":
            strategy.step_pre_backward(params, optimizers, state, step, info={})

        renders, alphas, info = rasterization(**raster_kwargs)
        pred = renders[0]
        gt = view["image"]
        l1 = (pred - gt).abs().mean()
        loss = (1.0 - args.ssim_weight) * l1 + args.ssim_weight * ssim_loss(pred, gt)
        if args.opacity_reg > 0:
            loss = loss + args.opacity_reg * torch.sigmoid(params["opacities"]).mean()
        if args.scale_reg > 0:
            loss = loss + args.scale_reg * torch.exp(params["scales"]).mean()

        for opt in optimizers.values():
            opt.zero_grad(set_to_none=True)
        loss.backward()
        if args.strategy == "default":
            strategy.step_post_backward(params, optimizers, state, step, info)
        else:
            strategy.step_post_backward(
                params, optimizers, state, step, info, lr=optimizers["means"].param_groups[0]["lr"]
            )
        for opt in optimizers.values():
            opt.step()

        if step % 50 == 0 or step == 1:
            print(f"STEP {step} loss={float(loss):.4f} n={params['means'].shape[0]}", flush=True)
        if step % args.export_every == 0 or step == args.max_steps:
            out = export_dir / f"export_{step}.ply"
            write_ply(out, params)
            print(f"STEP {step}", flush=True)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--data_dir", required=True)
    p.add_argument("--result_dir", required=True)
    p.add_argument("--export_dir", required=True)
    p.add_argument("--max_steps", type=int, default=7000)
    p.add_argument("--max_splats", type=int, default=1_500_000)
    p.add_argument("--export_every", type=int, default=500)
    p.add_argument("--sh_degree", type=int, default=3)
    p.add_argument("--strategy", choices=["mcmc", "default"], default="mcmc")
    p.add_argument("--ssim_weight", type=float, default=0.2)
    p.add_argument("--opacity_reg", type=float, default=0.01)
    p.add_argument("--scale_reg", type=float, default=0.01)
    p.add_argument("--antialiased", action="store_true")
    p.add_argument("--absgrad", action="store_true")
    p.add_argument("--init_ply", default=None)
    args = p.parse_args()
    Path(args.result_dir).mkdir(parents=True, exist_ok=True)
    train(args)


if __name__ == "__main__":
    main()
