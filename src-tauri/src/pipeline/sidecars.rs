//! Optional neural dense-init, pose, and post-polish sidecars.
//!
//! Dual-mode policy:
//! - **Standard**: VGGT-Commercial / DA3 / MapAnything / RoMa v2 / Fixer.
//! - **Experimental** (license ack): capture-profile research adapters
//!   (Ω/MASt3R/DUSt3R/Pi3X, StreamVGGT/SLAM, MonST3R/Easi3R 4D, CityGaussian
//!   family, surface/mesh) + Difix; evaluate then confidence-fuse (never blind
//!   concatenate). Surface/4D adapters stay on separate product paths.
//!
//! Sidecar schema v2 adds frame/CRS, confidence, provenance, and metrics.
//!
//! Do **not** vendor GPL Lichtfeld densify plugin sources — RoMa orchestration is
//! clean-room (MIT RoMa v2 APIs + our filters).

use super::dense;
use super::solver::{self, HypothesisScore};
use super::JobCtx;
use crate::settings::app_data_dir;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use tokio::io::AsyncWriteExt;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SidecarStatus {
    /// Legacy DAV2 (kept for installed engines; Standard prefers DA3).
    pub depth_anything_v2: bool,
    /// Depth Anything 3 Small/Base (Apache) — Standard densify candidate.
    pub depth_anything_3: bool,
    pub vggt_commercial: bool,
    /// Newest VGGT (Ω). Non-commercial weights; Experimental only.
    pub vggt_omega: bool,
    /// Present but non-commercial / research-only; Experimental only.
    pub vggt_research: bool,
    pub mast3r: bool,
    pub dust3r: bool,
    /// Pi3X static unordered research pose.
    pub pi3x: bool,
    /// Long-video / streaming research adapters.
    pub stream_vggt: bool,
    pub vggt_long: bool,
    pub mast3r_slam: bool,
    pub slam3r: bool,
    /// Dynamic / 4D adapters (separate path from static fusion).
    pub monst3r: bool,
    pub easi3r: bool,
    /// Large aerial / urban partition adapters.
    pub city_gaussian: bool,
    pub urban_gs: bool,
    pub horizon_gs: bool,
    /// Surface / mesh experimental adapters.
    pub gs_2d: bool,
    pub gof: bool,
    pub pgsr: bool,
    pub rade_gs: bool,
    pub sugar: bool,
    pub milo: bool,
    /// MapAnything Apache pose/depth hypothesis.
    pub mapanything: bool,
    /// RoMa v2 densify (MIT densifier + user-installed DINOv3 weights).
    pub roma_v2: bool,
    /// NVIDIA Fixer (commercial Open Model License) polish launcher.
    pub fixer: bool,
    /// Difix3D+ research launcher (gated; Experimental only).
    pub difix: bool,
}

/// Sidecar output schema v2 — written beside densify/pose PLYs when available.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarArtifactV2 {
    pub schema_version: u32,
    /// Declared reconstruction frame, e.g. "colmap/enu".
    pub frame: String,
    /// Optional CRS / vertical datum label (empty when local-only).
    pub crs: String,
    pub confidence: f32,
    pub source: String,
    pub version: String,
    pub license: String,
    /// SHA-256 of weights / launcher when known.
    pub license_hash: String,
    pub supporting_views: Vec<u32>,
    pub residual: Option<f32>,
    pub has_normals: bool,
    pub static_probability: f32,
    /// "metric" | "local" | "unknown"
    pub scale_status: String,
    pub metrics: SidecarMetrics,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarMetrics {
    pub point_count: u64,
    pub registered_images: u32,
    pub median_reproj_error: Option<f32>,
    pub mean_track_length: Option<f32>,
    pub wall_seconds: Option<f32>,
}

impl SidecarArtifactV2 {
    pub fn new_local(source: &str, confidence: f32, point_count: usize) -> Self {
        Self {
            schema_version: 2,
            frame: "colmap/enu".into(),
            crs: String::new(),
            confidence,
            source: source.into(),
            version: env!("CARGO_PKG_VERSION").into(),
            license: license_for(source).into(),
            license_hash: String::new(),
            supporting_views: Vec::new(),
            residual: None,
            has_normals: false,
            static_probability: 1.0,
            scale_status: "unknown".into(),
            metrics: SidecarMetrics {
                point_count: point_count as u64,
                ..Default::default()
            },
        }
    }
}

fn license_for(source: &str) -> &'static str {
    match source {
        "depth-anything-3" | "mapanything" | "roma-v2" | "colmap" | "mvs" | "gs-2d" => {
            "Apache-2.0"
        }
        "depth-anything-v2" => "Apache-2.0",
        "vggt-commercial" => "commercial",
        "fixer" => "NVIDIA-OML",
        "vggt-omega"
        | "mast3r"
        | "dust3r"
        | "difix"
        | "vggt-research"
        | "pi3x"
        | "stream-vggt"
        | "vggt-long"
        | "mast3r-slam"
        | "slam3r"
        | "monst3r"
        | "easi3r"
        | "city-gaussian"
        | "urban-gs"
        | "horizon-gs"
        | "gof"
        | "pgsr"
        | "rade-gs"
        | "sugar"
        | "milo" => "NC-research",
        _ => "unknown",
    }
}

pub fn write_artifact_v2(path: &Path, art: &SidecarArtifactV2) -> Result<(), String> {
    let json = serde_json::to_string_pretty(art).map_err(|e| e.to_string())?;
    std::fs::write(path, json).map_err(|e| e.to_string())
}

fn sidecars_dir() -> PathBuf {
    app_data_dir().join("engines").join("sidecars")
}

/// Engines first, then repo `tools/sidecars` (dev / unpackaged installs).
fn sidecar_search_dirs(name: &str) -> Vec<PathBuf> {
    vec![
        sidecars_dir().join(name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("tools")
            .join("sidecars")
            .join(name),
    ]
}

fn launcher_in_dir(dir: &Path) -> Option<PathBuf> {
    #[cfg(windows)]
    {
        for name in ["run.bat", "run.exe", "run.py"] {
            let p = dir.join(name);
            if p.exists() {
                return Some(p);
            }
        }
    }
    #[cfg(not(windows))]
    {
        for name in ["run.sh", "run.py", "run"] {
            let p = dir.join(name);
            if p.exists() {
                return Some(p);
            }
        }
    }
    None
}

fn launcher(name: &str) -> PathBuf {
    for dir in sidecar_search_dirs(name) {
        if let Some(p) = launcher_in_dir(&dir) {
            return p;
        }
    }
    sidecars_dir().join(name).join("run")
}

/// Template stubs ship a `.stub` marker so we never report them as "ready".
fn is_stub_sidecar(name: &str) -> bool {
    sidecar_search_dirs(name)
        .iter()
        .any(|d| d.join(".stub").exists())
}

fn launcher_ready(name: &str) -> bool {
    launcher(name).exists() && !is_stub_sidecar(name)
}

/// True when the user dropped weights / ACCEPTED / an upstream checkout.
fn install_marker_ready(name: &str) -> bool {
    const MARKERS: &[&str] = &[
        "ACCEPTED",
        "weights.onnx",
        "weights.pt",
        "weights.pth",
        "checkpoint.pt",
        "checkpoint.pth",
        "model.pt",
        "model.onnx",
        "upstream",
        "repo",
    ];
    sidecar_search_dirs(name).iter().any(|d| MARKERS.iter().any(|m| d.join(m).exists()))
}

fn neural_ready(name: &str) -> bool {
    launcher_ready(name) && install_marker_ready(name)
}

/// Ready = real launcher present AND not `.stub`.
/// Weight-gated Standard densifiers also require ACCEPTED/weights/upstream.
/// Never treat ACCEPTED alone (without a launcher) as ready.
pub fn status() -> SidecarStatus {
    let vggt_c = neural_ready("vggt-commercial");
    SidecarStatus {
        depth_anything_v2: neural_ready("depth-anything-v2"),
        depth_anything_3: neural_ready("depth-anything-3"),
        vggt_commercial: vggt_c,
        vggt_omega: launcher_ready("vggt-omega"),
        vggt_research: launcher_ready("vggt-research"),
        mast3r: launcher_ready("mast3r"),
        dust3r: launcher_ready("dust3r"),
        pi3x: launcher_ready("pi3x"),
        stream_vggt: launcher_ready("stream-vggt"),
        vggt_long: launcher_ready("vggt-long"),
        mast3r_slam: launcher_ready("mast3r-slam"),
        slam3r: launcher_ready("slam3r"),
        monst3r: launcher_ready("monst3r"),
        easi3r: launcher_ready("easi3r"),
        city_gaussian: launcher_ready("city-gaussian"),
        urban_gs: launcher_ready("urban-gs"),
        horizon_gs: launcher_ready("horizon-gs"),
        gs_2d: launcher_ready("gs-2d"),
        gof: launcher_ready("gof"),
        pgsr: launcher_ready("pgsr"),
        rade_gs: launcher_ready("rade-gs"),
        sugar: launcher_ready("sugar"),
        milo: launcher_ready("milo"),
        mapanything: neural_ready("mapanything"),
        // RoMa fails cleanly at runtime if the pip package is absent.
        roma_v2: launcher_ready("roma-v2"),
        fixer: neural_ready("fixer"),
        difix: launcher_ready("difix"),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Request<'a> {
    images_dir: &'a str,
    workspace: &'a str,
    sparse_dir: Option<&'a str>,
    max_points: u32,
    /// "sfm" | "densify" | "polish" — launchers that support multi-role.
    #[serde(skip_serializing_if = "Option::is_none")]
    task: Option<&'a str>,
    /// RoMa quality: "fast" | "base" | "high" | "precise".
    #[serde(skip_serializing_if = "Option::is_none")]
    quality: Option<&'a str>,
    /// Optional path for polish sidecars (Fixer / Difix).
    #[serde(skip_serializing_if = "Option::is_none")]
    splat_path: Option<&'a str>,
    /// Lichtfeld-recipe style knobs (ignored by launchers that do not need them).
    reference_fraction: f32,
    neighbors_per_ref: u32,
}

async fn invoke_launcher(
    ctx: &JobCtx,
    name: &str,
    images_dir: &Path,
    splat_path: Option<&Path>,
    task: Option<&str>,
    quality: Option<&str>,
) -> Result<Option<PathBuf>, String> {
    if solver::is_research_sidecar(name) && !ctx.settings.experimental_mode {
        ctx.log(format!(
            "Skipping {name}: research/NC sidecar requires Experimental Mode."
        ));
        return Ok(None);
    }

    let launch = launcher(name);
    if !launch.exists() {
        return Ok(None);
    }
    if is_stub_sidecar(name) {
        ctx.notice(format!(
            "{name} is still a stub — install real weights and delete the .stub marker."
        ));
        return Err(format!("{name} stub (not wired)"));
    }
    ctx.log(format!("Running {name} sidecar…"));

    let sparse = crate::colmap::find_model_dir(&ctx.workspace);
    let splat_s = splat_path.map(|p| p.to_string_lossy().into_owned());
    let quality_owned = quality.map(|s| s.to_string());
    let req = Request {
        images_dir: &images_dir.to_string_lossy(),
        workspace: &ctx.workspace.to_string_lossy(),
        sparse_dir: sparse.as_ref().map(|p| p.to_str().unwrap_or("")),
        max_points: if ctx.settings.experimental_mode {
            2_000_000
        } else {
            1_200_000
        },
        task,
        quality: quality_owned.as_deref(),
        splat_path: splat_s.as_deref(),
        // Lichtfeld densify plugin defaults (recipe only — not their GPL code).
        reference_fraction: 0.3,
        neighbors_per_ref: 8,
    };
    let body = serde_json::to_string(&req).map_err(|e| e.to_string())?;

    let mut cmd = if launch.extension().and_then(|e| e.to_str()) == Some("py") {
        let mut c = tokio::process::Command::new("python");
        c.arg(&launch);
        c
    } else {
        tokio::process::Command::new(&launch)
    };
    #[cfg(windows)]
    cmd.creation_flags(crate::profiler::CREATE_NO_WINDOW);
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(launch.parent().unwrap_or(Path::new(".")));

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Could not start {name}: {e}"))?;
    let pid = child.id();
    if let Some(pid) = pid {
        ctx.child_pids.lock().unwrap().push(pid);
    }
    if let Some(mut stdin) = child.stdin.take() {
        stdin
            .write_all(body.as_bytes())
            .await
            .map_err(|e| e.to_string())?;
    }
    let out = child
        .wait_with_output()
        .await
        .map_err(|e| e.to_string())?;
    if let Some(pid) = pid {
        ctx.child_pids.lock().unwrap().retain(|p| *p != pid);
    }

    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        ctx.log(format!("[{name}] {err}"));
        return Err(format!("{name} failed"));
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let path = stdout
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with('#'))
        .map(PathBuf::from);
    match path {
        Some(p) if p.exists() => Ok(Some(p)),
        Some(p) if p.to_string_lossy().eq_ignore_ascii_case("ok") => {
            // Pose sidecars may print "OK" after writing workspace/sparse.
            Ok(Some(p))
        }
        _ => {
            ctx.log(format!("[{name}] produced no output path"));
            Ok(None)
        }
    }
}

fn consume_point_ply(ply_path: &Path) -> Result<(Vec<[f32; 3]>, Vec<[u8; 3]>), String> {
    dense::read_xyzrgb_ply(ply_path).or_else(|_| {
        crate::splat::ply::read_ply(ply_path).map(|c| {
            let rgb: Vec<[u8; 3]> = c
                .sh_dc
                .iter()
                .map(|dc| {
                    let to_u8 = |v: f32| {
                        ((0.5 + crate::splat::SH_C0 * v) * 255.0).clamp(0.0, 255.0) as u8
                    };
                    [to_u8(dc[0]), to_u8(dc[1]), to_u8(dc[2])]
                })
                .collect();
            (c.positions, rgb)
        })
    })
}

fn sparse_model_usable(workspace: &Path) -> bool {
    crate::colmap::find_model_dir(workspace)
        .and_then(|d| crate::colmap::read_model(&d).ok())
        .map(|m| m.images.len() >= 2)
        .unwrap_or(false)
}

/// Run pose/SfM sidecars, **score** usable hypotheses, keep the best that passes gates.
/// Returns the winning solver name on success.
pub async fn try_neural_poses(
    ctx: &JobCtx,
    images_dir: &Path,
    chain: &[&str],
) -> Result<Option<String>, String> {
    let expected = std::fs::read_dir(images_dir)
        .map(|d| d.count())
        .unwrap_or(0);
    let mut best: Option<(HypothesisScore, PathBuf)> = None;

    for name in chain {
        ctx.check_cancel()?;
        if *name == "colmap-pose-prior" {
            // Handled by COLMAP path with pose-prior flags — skip neural launcher.
            continue;
        }
        // Isolate each hypothesis in a staging sparse so failures cannot pollute.
        let sparse = ctx.workspace.join("sparse");
        let stage = ctx.workspace.join(format!("hypothesis_{name}"));
        let _ = std::fs::remove_dir_all(&stage);
        let _ = std::fs::create_dir_all(stage.join("sparse").join("0"));
        let _ = std::fs::remove_dir_all(&sparse);
        let _ = std::fs::create_dir_all(sparse.join("0"));

        match invoke_launcher(ctx, name, images_dir, None, Some("sfm"), None).await {
            Ok(Some(path)) => {
                let looks_ok = if path.to_string_lossy().eq_ignore_ascii_case("ok") {
                    sparse_model_usable(&ctx.workspace)
                } else if path.is_dir() {
                    crate::colmap::read_model(&path)
                        .map(|m| m.images.len() >= 2)
                        .unwrap_or(false)
                        || sparse_model_usable(&ctx.workspace)
                } else if path.extension().and_then(|e| e.to_str()) == Some("txt")
                    || path.file_name().and_then(|f| f.to_str()) == Some("cameras.bin")
                {
                    sparse_model_usable(&ctx.workspace)
                } else if path.extension().and_then(|e| e.to_str()) == Some("ply") {
                    sparse_model_usable(&ctx.workspace)
                } else {
                    sparse_model_usable(&ctx.workspace)
                };

                if !looks_ok {
                    ctx.log(format!("[{name}] SfM output was not a usable COLMAP model."));
                    continue;
                }

                if path.is_dir() && path != sparse.join("0") && path != sparse {
                    if let Ok(model) = crate::colmap::read_model(&path) {
                        let dest = sparse.join("0");
                        let _ = std::fs::create_dir_all(&dest);
                        let _ = crate::colmap::write_model_txt(&dest, &model);
                    }
                }
                if !sparse_model_usable(&ctx.workspace) {
                    continue;
                }

                let model_dir = crate::colmap::find_model_dir(&ctx.workspace).unwrap();
                let model = crate::colmap::read_model(&model_dir)?;
                let score = HypothesisScore::compute(name, &model, expected);
                ctx.log(format!(
                    "[{name}] hypothesis score={:.3} (reg={:.2} reproj={:.2} track={:.2})",
                    score.composite,
                    score.registered_ratio,
                    score.median_reproj_error,
                    score.mean_track_length
                ));

                // Snapshot winning candidacy into stage for restore.
                let dest = stage.join("sparse").join("0");
                let _ = std::fs::create_dir_all(&dest);
                let _ = crate::colmap::write_model_txt(&dest, &model);
                let art = SidecarArtifactV2 {
                    schema_version: 2,
                    frame: "colmap/enu".into(),
                    crs: String::new(),
                    confidence: score.composite,
                    source: (*name).into(),
                    version: env!("CARGO_PKG_VERSION").into(),
                    license: license_for(name).into(),
                    license_hash: String::new(),
                    supporting_views: model.images.iter().map(|im| im.id).collect(),
                    residual: Some(score.median_reproj_error),
                    has_normals: false,
                    static_probability: score.cheirality_ratio,
                    scale_status: "unknown".into(),
                    metrics: SidecarMetrics {
                        point_count: model.points.len() as u64,
                        registered_images: model.images.len() as u32,
                        median_reproj_error: Some(score.median_reproj_error),
                        mean_track_length: Some(score.mean_track_length),
                        wall_seconds: None,
                    },
                };
                let _ = write_artifact_v2(&stage.join("artifact.v2.json"), &art);

                let gate = super::experimental::ExperimentalValidationGate::for_fusion_candidate(
                    true, // launcher already refused NC outside Experimental
                    score.passes_gates(),
                    true,
                    art.frame == "colmap/enu",
                    !art.scale_status.is_empty(),
                );
                if gate.all_clear() {
                    match &best {
                        Some((b, _)) if b.composite >= score.composite => {}
                        _ => best = Some((score, dest)),
                    }
                } else {
                    ctx.log(format!(
                        "[{name}] rejected by validation gates (canonical={}/hyp={}/v2={}/exp={}).",
                        gate.canonical_frame_aligned,
                        gate.hypothesis_gates_passed,
                        gate.schema_v2_artifact,
                        gate.license_acknowledged
                    ));
                }
            }
            Ok(None) => continue,
            Err(e) => {
                ctx.notice(format!("{name} pose solver skipped: {e}"));
            }
        }
    }

    if let Some((score, model_path)) = best {
        let sparse = ctx.workspace.join("sparse").join("0");
        let _ = std::fs::remove_dir_all(ctx.workspace.join("sparse"));
        let _ = std::fs::create_dir_all(&sparse);
        if let Ok(model) = crate::colmap::read_model(&model_path) {
            let _ = crate::colmap::write_model_txt(&sparse, &model);
        }
        ctx.notice(solver::camera_chip(&score.solver));
        ctx.log(format!(
            "Selected pose hypothesis {} (score={:.3})",
            score.solver, score.composite
        ));
        return Ok(Some(score.solver));
    }
    Ok(None)
}

/// Guaranteed COLMAP triangulation + bundle adjustment after neural init when a feature DB exists.
/// Always attempts BA (plan: guaranteed BA after neural init); falls back gracefully.
pub async fn maybe_refine_poses_with_colmap(
    ctx: &JobCtx,
    images_dir: &Path,
) -> Result<(), String> {
    let model_dir = match crate::colmap::find_model_dir(&ctx.workspace) {
        Some(d) => d,
        None => return Ok(()),
    };
    let model = crate::colmap::read_model(&model_dir)?;
    let n = model.images.len();
    let img_s = images_dir.to_string_lossy().into_owned();
    let model_s = model_dir.to_string_lossy().into_owned();
    let db = ctx.workspace.join("database.db");
    if !db.exists() {
        // Extract features so BA can run — guaranteed path after neural init.
        ctx.log("Building COLMAP feature DB for post-neural bundle adjustment…");
        let db_s = db.to_string_lossy().into_owned();
        let gpu = if ctx.settings.sift_gpu { "1" } else { "0" };
        if let Err(e) = super::colmap::run_colmap_pub(
            ctx,
            (0.62, 0.70),
            &[
                "feature_extractor",
                "--database_path",
                &db_s,
                "--image_path",
                &img_s,
                "--ImageReader.single_camera",
                "1",
                "--ImageReader.camera_model",
                "OPENCV",
                "--FeatureExtraction.use_gpu",
                gpu,
                "--SiftExtraction.max_num_features",
                "8192",
            ],
            n,
        )
        .await
        {
            ctx.log(format!("Feature extract for BA skipped: {e}"));
            return Ok(());
        }
        // Light matching so triangulator has pairs.
        let front = solver::matcher_front_end(&ctx.settings, lightglue_ready());
        let match_args: Vec<&str> = if front == "sequential" || (front == "auto" && n > 80) {
            vec![
                "sequential_matcher",
                "--database_path",
                &db_s,
                "--SequentialMatching.overlap",
                "15",
                "--FeatureMatching.use_gpu",
                gpu,
            ]
        } else {
            vec![
                "exhaustive_matcher",
                "--database_path",
                &db_s,
                "--FeatureMatching.use_gpu",
                gpu,
            ]
        };
        // LightGlue routing stub: log intent; COLMAP SIFT matcher runs until a LightGlue engine lands.
        if front == "lightglue" {
            ctx.log(
                "LightGlue matcher selected — routing via COLMAP feature DB until LightGlue engine is installed.",
            );
        }
        let _ = super::colmap::run_colmap_pub(ctx, (0.70, 0.78), &match_args, n).await;
    }
    let db_s = db.to_string_lossy().into_owned();
    ctx.log("Refining neural poses with COLMAP triangulation + bundle adjustment…");
    match super::colmap::run_colmap_pub(
        ctx,
        (0.78, 0.86),
        &[
            "point_triangulator",
            "--database_path",
            &db_s,
            "--image_path",
            &img_s,
            "--input_path",
            &model_s,
            "--output_path",
            &model_s,
        ],
        n,
    )
    .await
    {
        Ok(()) => {
            if let Err(e) = super::colmap::run_colmap_pub(
                ctx,
                (0.86, 0.92),
                &[
                    "bundle_adjuster",
                    "--input_path",
                    &model_s,
                    "--output_path",
                    &model_s,
                ],
                n,
            )
            .await
            {
                ctx.log(format!("Bundle adjuster failed: {e}"));
            } else {
                ctx.log("Post-neural bundle adjustment complete.");
            }
        }
        Err(e) => {
            ctx.log(format!("COLMAP refine skipped: {e}"));
        }
    }
    Ok(())
}

fn lightglue_ready() -> bool {
    neural_ready("lightglue")
}

/// Public probe for COLMAP matcher routing.
pub fn lightglue_installed() -> bool {
    lightglue_ready()
}

/// Collect points from neural densifiers.
/// Returns one confidence-fused cloud (Standard: first success, Experimental:
/// fuse every source via schema-v2 confidence — never raw concatenation).
pub async fn try_neural_points(
    ctx: &JobCtx,
    images_dir: &Path,
) -> Result<Option<(Vec<[f32; 3]>, Vec<[u8; 3]>, String)>, String> {
    if !ctx.settings.use_neural_init {
        return Ok(None);
    }

    let st = status();
    let profile = solver::detect_capture_profile(
        images_dir,
        &ctx.workspace,
        ctx.settings.experimental_mode,
    );
    let order = solver::densify_neural_order(&ctx.settings, &st, profile);
    if order.is_empty() {
        return Ok(None);
    }

    // Dynamic 4D adapters never enter static fusion — log availability only.
    if matches!(profile, solver::CaptureProfile::DynamicScene)
        && ctx.settings.experimental_mode
    {
        let four = super::experimental::experimental_four_d_candidates(&st);
        if !four.is_empty() {
            ctx.log(format!(
                "4D adapters available (separate path, not fused into init.ply): {}",
                four.join(", ")
            ));
        }
    }
    if matches!(profile, solver::CaptureProfile::LargeScene)
        && ctx.settings.experimental_mode
    {
        let large = super::experimental::experimental_large_scene_candidates(&st);
        if !large.is_empty() {
            ctx.log(format!(
                "Large-scene partition adapters (engine-specific outputs): {}",
                large.join(", ")
            ));
        }
    }

    let merge_all = ctx.settings.experimental_mode;
    let mut clouds: Vec<Vec<dense::EvidencePoint>> = Vec::new();
    let mut labels: Vec<String> = Vec::new();

    for name in order {
        ctx.check_cancel()?;
        match invoke_launcher(ctx, name, images_dir, None, Some("densify"), None).await {
            Ok(Some(ply_path)) => match consume_point_ply(&ply_path) {
                Ok((px, pr)) if px.len() >= 32 => {
                    let conf = match name {
                        "depth-anything-3" => 0.72,
                        "depth-anything-v2" => 0.65,
                        "mapanything" => 0.70,
                        "vggt-commercial" => 0.74,
                        "vggt-omega" | "vggt-research" | "pi3x" => 0.68,
                        "stream-vggt" | "vggt-long" | "mast3r-slam" | "slam3r" => 0.66,
                        "mast3r" | "dust3r" => 0.62,
                        _ => 0.60,
                    };
                    let art = SidecarArtifactV2::new_local(name, conf, px.len());
                    let meta = ply_path.with_extension("v2.json");
                    let gate = super::experimental::ExperimentalValidationGate::for_fusion_candidate(
                        true, // launcher already refused NC outside Experimental
                        true,
                        write_artifact_v2(&meta, &art).is_ok(),
                        art.frame == "colmap/enu",
                        !art.scale_status.is_empty(),
                    );
                    if !gate.all_clear() {
                        ctx.log(format!(
                            "[{name}] densify rejected by fusion validation gates."
                        ));
                        continue;
                    }
                    if merge_all {
                        clouds.push(
                            px.iter()
                                .zip(pr.iter())
                                .map(|(xyz, rgb)| dense::EvidencePoint {
                                    xyz: *xyz,
                                    rgb: *rgb,
                                    confidence: conf,
                                    source: "neural",
                                    preserve: false,
                                })
                                .collect(),
                        );
                        labels.push(name.to_string());
                    } else {
                        return Ok(Some((px, pr, name.to_string())));
                    }
                }
                Ok(_) => ctx.log(format!("[{name}] too few points")),
                Err(e) => ctx.log(format!("[{name}] could not consume output: {e}")),
            },
            Ok(None) => continue,
            Err(e) => {
                ctx.notice(format!("{name} sidecar skipped: {e}"));
            }
        }
    }

    if merge_all && !clouds.is_empty() {
        let scale = dense::scene_scale_hint(&clouds);
        let (xyz, rgb) = dense::fuse_evidence(&clouds, scale, 1_500_000);
        if xyz.len() >= 32 {
            return Ok(Some((xyz, rgb, labels.join("+"))));
        }
    }
    Ok(None)
}

/// RoMa v2 densify (Lichtfeld-style recipe via clean-room sidecar).
pub async fn try_roma_densify(
    ctx: &JobCtx,
    images_dir: &Path,
) -> Result<Option<(Vec<[f32; 3]>, Vec<[u8; 3]>)>, String> {
    if !ctx.settings.dense_init || !status().roma_v2 {
        return Ok(None);
    }
    let q = ctx.settings.roma_quality.as_str();
    match invoke_launcher(ctx, "roma-v2", images_dir, None, Some("densify"), Some(q)).await
    {
        Ok(Some(ply_path)) => match consume_point_ply(&ply_path) {
            Ok((xyz, rgb)) if xyz.len() >= 32 => {
                ctx.log(format!(
                    "RoMa v2 densify ({q}): {} points (will merge).",
                    xyz.len()
                ));
                Ok(Some((xyz, rgb)))
            }
            Ok(_) => {
                ctx.log("[roma-v2] too few points");
                Ok(None)
            }
            Err(e) => {
                ctx.log(format!("[roma-v2] could not consume output: {e}"));
                Ok(None)
            }
        },
        Ok(None) => Ok(None),
        Err(e) => {
            ctx.notice(format!("RoMa densify skipped: {e}"));
            Ok(None)
        }
    }
}

/// Try neural densifiers and write `init.ply` (kept for diagnostics / future callers).
#[allow(dead_code)]
pub async fn try_dense_init(ctx: &JobCtx, images_dir: &Path) -> Result<Option<usize>, String> {
    match try_neural_points(ctx, images_dir).await? {
        Some((xyz, rgb, name)) => {
            let n = dense::write_init_from_points(ctx, xyz, rgb, &name)?;
            Ok(Some(n))
        }
        None => Ok(None),
    }
}

/// Post-train polish. Standard: Fixer. Experimental: Difix then Fixer (both if present).
pub async fn try_polish(ctx: &JobCtx, result_ply: &Path) -> Result<bool, String> {
    if !ctx.settings.post_polish {
        return Ok(false);
    }
    let st = status();
    let order = solver::polish_order(&ctx.settings, &st);
    if order.is_empty() {
        return Ok(false);
    }

    let images = ctx
        .workspace
        .join("images")
        .canonicalize()
        .unwrap_or_else(|_| ctx.workspace.join("images"));

    let mut any = false;
    let mut labels = Vec::new();
    for name in order {
        ctx.check_cancel()?;
        ctx.stage_progress("finalize", 0.7, &format!("Polishing with {name}…"));
        match invoke_launcher(ctx, name, &images, Some(result_ply), Some("polish"), None).await
        {
            Ok(Some(out)) => {
                if out != result_ply {
                    std::fs::copy(&out, result_ply).map_err(|e| e.to_string())?;
                }
                ctx.log(format!("{name} polished the result splat."));
                labels.push(name.to_string());
                any = true;
                // Experimental: continue so Fixer can run after Difix.
                if !ctx.settings.experimental_mode {
                    break;
                }
            }
            Ok(None) => {
                ctx.log(format!("[{name}] produced no polished splat"));
            }
            Err(e) => {
                ctx.notice(format!("{name} polish skipped: {e}"));
            }
        }
    }
    if any {
        ctx.notice(format!("Polish: {}", labels.join(" → ")));
    }
    Ok(any)
}
