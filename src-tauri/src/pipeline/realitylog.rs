//! RealityLog Capture Source adapter (RealityLog seam, migration step M3).
//!
//! The seam runs **instasplatter → hyperscape-lite** and never the reverse. This
//! module is edge B: it invokes the source checkout's own one-command
//! reconstruction orchestrator, `scripts/reconstruct_session.py`, as a
//! subprocess, and injects **every** engine path the host resolves — so the
//! source never resolves an asset of its own and the implicit back-edge into
//! `%LOCALAPPDATA%/instasplatter/engines` is broken.
//!
//! Three properties are load-bearing:
//!
//! 1. **Fail closed.** A missing checkout, a missing entry point, a missing
//!    engine argument or a non-zero exit is an error naming the actual problem,
//!    never a silent degradation.
//! 2. **The checkout is read, never written.** Nothing here writes, creates or
//!    deletes anything under the source root; the source's `git status` must be
//!    byte-identical before and after a run (gate M3(d)).
//! 3. **The chain may be untracked.** At the time of the merge the source's
//!    reconstruction chain is not in its git index, so a fresh clone does not
//!    receive it. That is the single most confusing failure this seam can
//!    produce, so it gets its own named message (R4).

use crate::profiler::hidden_command;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Environment variable holding the source checkout path.
pub const REALITYLOG_ROOT_ENV: &str = "REALITYLOG_ROOT";

/// Environment variable holding the mq3drecon executable, which lives in a
/// third repo (`Programs/mq3drecon`) that this merge deliberately does not
/// touch or vendor.
pub const MQ3DRECON_ENV: &str = "MQ3DRECON_EXE";

/// The entry point inside the source checkout.
pub const ENTRY_POINT_REL: &str = "scripts/reconstruct_session.py";

/// The named failure for R4: the chain is not in the source's git index.
pub const UNTRACKED_CHAIN_MESSAGE: &str =
    "the reconstruction chain is untracked in the source checkout";

/// The stage names, in order, as the source's own module docstring lists them
/// (`scripts/reconstruct_session.py:11-23`). `convert` runs once per eye and
/// `push` only happens when the host does not pass `--no-push`.
pub const DOCUMENTED_STAGES: [&str; 7] = [
    "convert",
    "reconstruct",
    "export-colmap",
    "stage",
    "train",
    "publish",
    "push",
];

/// `train Brush for <N> steps` — the source's default, and the value its own
/// `scripts/tests/test_reconstruct_session.py` pins as producing
/// `splat_7000.ply`. M8 single-sources this through the injected `--steps`.
pub const SOURCE_DEFAULT_STEPS: u32 = 7000;

/// `scripts/brush_defaults.py::EXPORT_EVERY_BATCH` — the **batch** path's
/// `--export-every`, recorded (not injected) so the known live-vs-batch
/// divergence is a documented fact here rather than a rediscovery.
pub const SOURCE_EXPORT_EVERY_BATCH: u32 = 2000;

/// `scripts/brush_defaults.py::EXPORT_EVERY_LIVE` — the **live** `pc_receiver`
/// path's `--export-every`.
///
/// It differs from [`SOURCE_EXPORT_EVERY_BATCH`], and `brush_defaults.py`
/// records that difference as **unexplained**, not as a decision: "no receipt,
/// comment or commit message states why the batch path exports twice as rarely
/// as the live path". The host must not "fix" it, and must not cite it as
/// intentional. Only `--steps` is injected, so the host cannot change it.
pub const SOURCE_EXPORT_EVERY_LIVE: u32 = 1000;

/// `scripts/brush_defaults.py::EVAL_SPLIT_EVERY`.
pub const SOURCE_EVAL_SPLIT_EVERY: u32 = 10;

/// `scripts/brush_defaults.py::EVAL_EVERY`.
pub const SOURCE_EVAL_EVERY: u32 = 1000;

/// The step descriptions a dry run must emit, in order.
pub fn documented_step_prefixes(push: bool) -> Vec<&'static str> {
    let mut prefixes = vec![
        "convert left-eye",
        "convert right-eye",
        "pose-based reconstruction",
        "export the COLMAP model",
        "stage the dataset",
        "train Brush",
        "publish the PLY",
    ];
    if push {
        prefixes.push("push the session's");
    }
    prefixes
}

/// The engine and interpreter paths the host injects into the source CLI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EngineArgs {
    /// `--brush`: the host's resolved Brush executable.
    pub brush: PathBuf,
    /// `--mq3drecon`: the third-repo mq3drecon executable.
    pub mq3drecon: PathBuf,
    /// `--mq3drecon-config`: a config that disables the blocking GUI viewers.
    pub mq3drecon_config: PathBuf,
    /// `--python`: the interpreter for the RGBA->PNG converter.
    pub python: PathBuf,
    /// `--adb`: used only by the optional push step.
    pub adb: String,
}

impl EngineArgs {
    /// Resolve the injectable arguments from the host's own engine authority.
    ///
    /// Brush comes from [`crate::engines::brush_exe`]; mq3drecon is external to
    /// both repos and comes from `MQ3DRECON_EXE`; the headless config and the
    /// converter interpreter come from the source checkout, because they belong
    /// to the source's own chain and are not host assets. The caller may
    /// override any of them.
    pub fn resolve(root: &Path) -> Result<EngineArgs, String> {
        let mq3drecon = std::env::var_os(MQ3DRECON_ENV)
            .map(PathBuf::from)
            .ok_or_else(|| {
                format!(
                    "{MQ3DRECON_ENV} is not set. The source's primary reconstruction path drives \
                     mq3drecon, which lives in a third repo this host does not vendor or install. \
                     Set {MQ3DRECON_ENV} to its executable (for example \
                     Programs/mq3drecon/.venv312/Scripts/mq3drecon.exe)."
                )
            })?;
        Ok(EngineArgs {
            brush: crate::engines::brush_exe(),
            mq3drecon,
            mq3drecon_config: root.join("scripts").join("mq3drecon_headless.yaml"),
            python: resolve_converter_python(root),
            adb: "adb".to_string(),
        })
    }

    /// Every path that must exist before the source CLI will even dry-run.
    pub fn missing(&self) -> Vec<String> {
        let mut missing = Vec::new();
        if !self.brush.is_file() {
            missing.push(format!(
                "Brush executable not found: {} (host engines.rs cannot resolve it)",
                self.brush.display()
            ));
        }
        if !self.mq3drecon.is_file() {
            missing.push(format!(
                "mq3drecon executable not found: {} (the third repo is external to both merged repos)",
                self.mq3drecon.display()
            ));
        }
        if !self.mq3drecon_config.is_file() {
            missing.push(format!(
                "mq3drecon headless config not found: {}",
                self.mq3drecon_config.display()
            ));
        }
        if self.python.as_os_str().is_empty() {
            missing.push("no Python interpreter resolved for the RGBA converter".to_string());
        }
        missing
    }
}

/// Prefer the source checkout's own venv interpreter, then `python` on PATH.
pub fn resolve_converter_python(root: &Path) -> PathBuf {
    for candidate in [
        root.join(".venv").join("Scripts").join("python.exe"),
        root.join(".venv").join("bin").join("python"),
    ] {
        if candidate.is_file() {
            return candidate;
        }
    }
    PathBuf::from("python")
}

/// A resolved, validated Capture Source checkout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureSource {
    pub root: PathBuf,
    pub entry_point: PathBuf,
    /// `git rev-parse HEAD` in the checkout, when it is a git repo. Recorded in
    /// the receipt so provenance is auditable.
    pub head: Option<String>,
}

/// Resolve the Capture Source checkout, failing closed.
///
/// Order: the explicit argument, then `REALITYLOG_ROOT`, then the host's
/// configured path. The returned checkout is validated eagerly, so a caller
/// cannot get a path that will fail later with a confusing message.
pub fn resolve_source(
    explicit: Option<&Path>,
    configured: Option<&Path>,
) -> Result<CaptureSource, String> {
    resolve_source_with(
        explicit,
        std::env::var_os(REALITYLOG_ROOT_ENV).as_deref(),
        configured,
    )
}

/// [`resolve_source`] with the environment value passed in, so the precedence
/// itself is testable without mutating process-global state.
pub fn resolve_source_with(
    explicit: Option<&Path>,
    env_root: Option<&std::ffi::OsStr>,
    configured: Option<&Path>,
) -> Result<CaptureSource, String> {
    let root = explicit
        .map(PathBuf::from)
        .or_else(|| env_root.map(PathBuf::from))
        .or_else(|| configured.map(PathBuf::from))
        .ok_or_else(|| {
            format!(
                "No RealityLog capture source configured. Pass --realitylog-root, set \
                 {REALITYLOG_ROOT_ENV}, or set the Capture Source path in settings."
            )
        })?;

    if !root.is_dir() {
        return Err(format!(
            "RealityLog capture source not found: {} is not a directory. Pass \
             --realitylog-root or set {REALITYLOG_ROOT_ENV}.",
            root.display()
        ));
    }

    let entry_point = root.join(ENTRY_POINT_REL.replace('/', std::path::MAIN_SEPARATOR_STR));
    if !entry_point.is_file() {
        return Err(format!(
            "{} has no {ENTRY_POINT_REL}: {UNTRACKED_CHAIN_MESSAGE}. A clone of the source does \
             not receive the reconstruction chain, because those scripts were never committed. \
             Point --realitylog-root at a working tree that has them on disk, or track them in \
             the source before relying on this seam.",
            root.display()
        ));
    }

    Ok(CaptureSource {
        head: git_head(&root),
        root,
        entry_point,
    })
}

/// `git rev-parse HEAD` in `root`, or `None` when it is not a readable repo.
///
/// Read-only: no git command here writes, stages or commits anything.
pub fn git_head(root: &Path) -> Option<String> {
    let out = hidden_command("git")
        .arg("-C")
        .arg(root)
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let head = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if head.is_empty() {
        None
    } else {
        Some(head)
    }
}

/// What the host asks the source to do. The stage sequence itself is the
/// source's; the host only chooses steps and selection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JobRequest {
    /// Session directory, absolute or relative to the source root.
    pub session: String,
    /// `gs_run` dataset name (defaults to the session basename in the source).
    pub name: Option<String>,
    pub steps: u32,
    pub dry_run: bool,
    /// False adds `--no-push` so no headset push is attempted.
    pub push: bool,
    /// `--interval`: use every Nth frame.
    pub interval: Option<u32>,
    /// `--select-names`: a text file of frame file names.
    pub select_names: Option<PathBuf>,
}

impl Default for JobRequest {
    fn default() -> Self {
        JobRequest {
            session: String::new(),
            name: None,
            steps: SOURCE_DEFAULT_STEPS,
            dry_run: true,
            push: false,
            interval: None,
            select_names: None,
        }
    }
}

/// The exact argv the host hands the source. Built as data so a test can assert
/// on it without spawning anything.
pub fn build_argv(
    source: &CaptureSource,
    python: &Path,
    engines: &EngineArgs,
    request: &JobRequest,
) -> Vec<String> {
    let mut argv = vec![
        python.to_string_lossy().into_owned(),
        source.entry_point.to_string_lossy().into_owned(),
        request.session.clone(),
    ];
    if let Some(name) = &request.name {
        argv.push(name.clone());
    }
    argv.push("--steps".to_string());
    argv.push(request.steps.to_string());
    // Every engine path is injected: the source never self-resolves one.
    argv.push("--brush".to_string());
    argv.push(engines.brush.to_string_lossy().into_owned());
    argv.push("--mq3drecon".to_string());
    argv.push(engines.mq3drecon.to_string_lossy().into_owned());
    argv.push("--mq3drecon-config".to_string());
    argv.push(engines.mq3drecon_config.to_string_lossy().into_owned());
    argv.push("--python".to_string());
    argv.push(engines.python.to_string_lossy().into_owned());
    argv.push("--adb".to_string());
    argv.push(engines.adb.clone());
    if let Some(interval) = request.interval {
        argv.push("--interval".to_string());
        argv.push(interval.to_string());
    }
    if let Some(select_names) = &request.select_names {
        argv.push("--select-names".to_string());
        argv.push(select_names.to_string_lossy().into_owned());
    }
    if !request.push {
        argv.push("--no-push".to_string());
    }
    if request.dry_run {
        argv.push("--dry-run".to_string());
    }
    argv
}

/// One printed step of the source's chain.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunStep {
    pub description: String,
    pub argv: Vec<String>,
}

/// The result of a dry run. `exit_code` is the process's own code: the source
/// returns 3 on any missing input, by design.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DryRunOutcome {
    pub root: PathBuf,
    pub source_head: Option<String>,
    pub argv: Vec<String>,
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    pub steps: Vec<DryRunStep>,
}

/// Parse the source's `--dry-run` output.
///
/// The source prints `[i/N] description` followed by `  $ argv`. Its argv is
/// space-joined without quoting, so a path containing a space cannot be
/// recovered here — that limitation is the source's print format, not this
/// parser's, and it is recorded rather than papered over.
pub fn parse_dry_run(stdout: &str) -> Vec<DryRunStep> {
    let mut steps: Vec<DryRunStep> = Vec::new();
    for line in stdout.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with('[') {
            if let Some(close) = trimmed.find(']') {
                steps.push(DryRunStep {
                    description: trimmed[close + 1..].trim().to_string(),
                    argv: Vec::new(),
                });
            }
        } else if let Some(rest) = trimmed.strip_prefix("$ ") {
            if let Some(last) = steps.last_mut() {
                last.argv = rest.split_whitespace().map(str::to_string).collect();
            }
        }
    }
    steps
}

/// Run the source's orchestrator with `--dry-run` and capture stdout/stderr.
///
/// Dry run only: this function refuses to execute a non-dry-run request, so
/// nothing in the seam can accidentally start a real reconstruction.
pub fn run_dry_run(
    explicit_root: Option<&Path>,
    configured: Option<&Path>,
    request: &JobRequest,
) -> Result<DryRunOutcome, String> {
    if !request.dry_run {
        return Err(
            "run_dry_run refuses a non-dry-run request; a real reconstruction needs a capture, \
             a GPU and disk headroom and is deliberately not reachable from this entry point."
                .to_string(),
        );
    }
    let source = resolve_source(explicit_root, configured)?;
    let engines = EngineArgs::resolve(&source.root)?;
    let missing = engines.missing();
    if !missing.is_empty() {
        return Err(format!(
            "Cannot drive the RealityLog capture source at {}:\n  - {}",
            source.root.display(),
            missing.join("\n  - ")
        ));
    }

    let argv = build_argv(&source, &engines.python, &engines, request);
    let (program, rest) = argv
        .split_first()
        .expect("argv always starts with the interpreter");
    let output = hidden_command(program)
        .args(rest)
        .output()
        .map_err(|e| format!("Could not start {program}: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    let exit_code = output.status.code().unwrap_or(-1);
    let steps = parse_dry_run(&stdout);

    Ok(DryRunOutcome {
        root: source.root,
        source_head: source.head,
        argv,
        exit_code,
        stdout,
        stderr,
        steps,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// A fixture checkout with the entry point and the source-side assets the
    /// adapter resolves, plus stub engines. Nothing here is executed: the dry
    /// run only ever stats these paths.
    fn fixture_source(tag: &str) -> PathBuf {
        let root = std::env::temp_dir().join(format!("instasplatter_realitylog_{tag}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("scripts")).unwrap();
        fs::write(root.join("scripts/reconstruct_session.py"), b"# stub\n").unwrap();
        fs::write(
            root.join("scripts/mq3drecon_headless.yaml"),
            b"reconstruction:\n",
        )
        .unwrap();
        fs::create_dir_all(root.join("engine-stubs")).unwrap();
        for exe in ["brush_app.exe", "mq3drecon.exe"] {
            fs::write(root.join("engine-stubs").join(exe), b"stub").unwrap();
        }
        root
    }

    fn fixture_session(tag: &str) -> PathBuf {
        let session = std::env::temp_dir().join(format!("instasplatter_realitylog_session_{tag}"));
        let _ = fs::remove_dir_all(&session);
        for side in ["left", "right"] {
            let frames = session.join(format!("{side}_camera_mruk_rgba"));
            fs::create_dir_all(&frames).unwrap();
            fs::write(frames.join("frame.rgba"), b"rgba").unwrap();
            fs::write(
                session.join(format!("{side}_camera_mruk_intrinsics.json")),
                b"{\"currentResolution\":{\"width\":2,\"height\":2}}",
            )
            .unwrap();
        }
        session
    }

    fn fixture_engines(root: &Path) -> EngineArgs {
        EngineArgs {
            brush: root.join("engine-stubs/brush_app.exe"),
            mq3drecon: root.join("engine-stubs/mq3drecon.exe"),
            mq3drecon_config: root.join("scripts/mq3drecon_headless.yaml"),
            python: PathBuf::from("python"),
            adb: "adb".to_string(),
        }
    }

    fn invoke(root: &Path, session: &Path, engines: &EngineArgs) -> DryRunOutcome {
        let source = resolve_source(Some(root), None).expect("fixture source should resolve");
        let argv = build_argv(
            &source,
            &engines.python,
            engines,
            &JobRequest {
                session: session.to_string_lossy().into_owned(),
                name: Some("cap".to_string()),
                ..JobRequest::default()
            },
        );
        let (program, rest) = argv.split_first().unwrap();
        let output = std::process::Command::new(program)
            .args(rest)
            .output()
            .expect("python should be runnable");
        DryRunOutcome {
            root: source.root,
            source_head: source.head,
            argv,
            exit_code: output.status.code().unwrap_or(-1),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            steps: parse_dry_run(&String::from_utf8_lossy(&output.stdout)),
        }
    }

    /// Gate (a), hermetic half: the parser recovers the documented stage list
    /// from a recorded transcript of the real orchestrator.
    ///
    /// The transcript is a normalised capture of a real
    /// `reconstruct_session.py --dry-run ... --no-push` run (exit 0, 7 steps),
    /// recorded 2026-10-11 against the source checkout at HEAD `d2c8c0a`. The
    /// live half — actually spawning the source — is
    /// [`real_checkout_dry_run_leaves_the_source_untouched`], which needs the
    /// machine-local checkout and is therefore `#[ignore]`d.
    #[test]
    fn parse_dry_run_recovers_the_documented_chain_from_the_recorded_transcript() {
        let transcript = include_str!("../../tests/fixtures/realitylog/dry_run_no_push.txt");
        let steps = parse_dry_run(transcript);

        let prefixes = documented_step_prefixes(false);
        assert_eq!(DOCUMENTED_STAGES.len(), 7);
        assert_eq!(steps.len(), prefixes.len(), "transcript:\n{transcript}");
        for (step, prefix) in steps.iter().zip(prefixes.iter()) {
            assert!(
                step.description.starts_with(prefix),
                "stage {:?} does not match the documented prefix {prefix:?}",
                step.description
            );
        }

        // `--no-push` means the docstring's 7th stage is absent, which is the
        // only stage the host ever drops.
        assert!(
            !steps.iter().any(|s| s.description.starts_with("push")),
            "push must be dropped by --no-push"
        );

        // The export step must carry the flag the source's own test pins.
        let export = steps
            .iter()
            .find(|s| s.description.starts_with("export the COLMAP model"))
            .unwrap();
        assert!(
            export.argv.iter().any(|a| a == "--use-colored-pointcloud"),
            "{:?}",
            export.argv
        );
        assert!(
            export.argv.iter().any(|a| a == "export-colmap"),
            "{:?}",
            export.argv
        );

        // M8's oracle, read off the transcript: the chain the host drives names
        // `splat_<steps>.ply` in both the Brush `--export-path` and the publish
        // target, with `--export-every` from the source's own brush defaults.
        let train = steps
            .iter()
            .find(|s| s.description.starts_with("train Brush"))
            .unwrap();
        assert!(
            train
                .description
                .contains(&SOURCE_DEFAULT_STEPS.to_string()),
            "{:?}",
            train.description
        );
        let flag = |name: &str| {
            train
                .argv
                .iter()
                .position(|a| a == name)
                .map(|i| train.argv[i + 1].clone())
        };
        assert_eq!(flag("--total-steps").as_deref(), Some("7000"));
        assert_eq!(flag("--export-every").as_deref(), Some("2000"));
        assert_eq!(flag("--export-name").as_deref(), Some("splat_{iter}.ply"));
        let publish = steps
            .iter()
            .find(|s| s.description.starts_with("publish the PLY"))
            .unwrap();
        assert!(
            publish.argv[1].ends_with("splat_7000.ply"),
            "{:?}",
            publish.argv
        );
    }

    /// Gate (b): a bogus root fails closed, and the message names the actual
    /// problem rather than reporting a generic missing file.
    #[test]
    fn a_bogus_root_fails_closed_and_names_the_problem() {
        let err = resolve_source(Some(Path::new("Z:/definitely/not/here")), None).unwrap_err();
        assert!(err.contains("not a directory"), "{err}");
        assert!(err.contains("REALITYLOG_ROOT"), "{err}");

        let root = std::env::temp_dir().join("instasplatter_realitylog_no_chain");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        let err = resolve_source(Some(&root), None).unwrap_err();
        assert!(err.contains(UNTRACKED_CHAIN_MESSAGE), "{err}");
        assert!(err.contains(ENTRY_POINT_REL), "{err}");
        let _ = fs::remove_dir_all(&root);
    }

    /// Gate (c): the emitted argv carries the injected engine paths, so the
    /// source never self-resolves one.
    #[test]
    fn emitted_argv_injects_every_engine_path() {
        let root = fixture_source("argv");
        let engines = fixture_engines(&root);
        let source = resolve_source(Some(&root), None).unwrap();
        let argv = build_argv(
            &source,
            &engines.python,
            &engines,
            &JobRequest {
                session: "sessions/x".to_string(),
                ..JobRequest::default()
            },
        );

        for flag in [
            "--brush",
            "--mq3drecon",
            "--mq3drecon-config",
            "--python",
            "--adb",
            "--steps",
            "--dry-run",
            "--no-push",
        ] {
            assert!(
                argv.contains(&flag.to_string()),
                "argv is missing {flag}: {argv:?}"
            );
        }
        assert_eq!(
            argv[argv.iter().position(|a| a == "--brush").unwrap() + 1],
            engines.brush.to_string_lossy()
        );
        assert_eq!(
            argv[argv.iter().position(|a| a == "--mq3drecon").unwrap() + 1],
            engines.mq3drecon.to_string_lossy()
        );
        assert_eq!(
            argv[argv.iter().position(|a| a == "--steps").unwrap() + 1],
            SOURCE_DEFAULT_STEPS.to_string()
        );
        // --dry-run is what makes the argv safe to run, so it must be last and
        // present whenever the request asks for it.
        assert_eq!(argv.last().unwrap(), "--dry-run");

        let _ = fs::remove_dir_all(&root);
    }

    /// M8's hermetic half: the cadence the host *injects* is the step count the
    /// source's own test pins, and the numbers the host merely *records* are the
    /// numbers the source's own recorded chain shows.
    ///
    /// The oracle is `scripts/tests/test_reconstruct_session.py`, which asserts
    /// that `--steps 7000` produces `splat_7000.ply` in both the Brush step and
    /// the publish step — so a drift in either number fails there first.
    #[test]
    fn injected_cadence_is_the_step_count_the_source_pins() {
        let root = fixture_source("cadence");
        let engines = fixture_engines(&root);
        let source = resolve_source(Some(&root), None).unwrap();

        // The default request injects the source's documented training length.
        let argv = build_argv(&source, &engines.python, &engines, &JobRequest::default());
        let steps = argv
            .iter()
            .position(|a| a == "--steps")
            .map(|i| argv[i + 1].clone())
            .expect("the host always injects --steps");
        assert_eq!(steps, SOURCE_DEFAULT_STEPS.to_string());
        assert_eq!(steps, "7000");

        // ... and it is the only cadence lever the host has, because the source
        // CLI exposes no other one. A flag the host cannot set is a flag it also
        // cannot silently change.
        for not_a_flag in [
            "--export-every",
            "--eval-every",
            "--eval-split-every",
            "--export-name",
        ] {
            assert!(
                !argv.iter().any(|a| a == not_a_flag),
                "the host must not inject {not_a_flag}: the source owns that cadence"
            );
        }

        // The recorded divergence is the source's own, and the host records it
        // rather than resolving it (brush_defaults.py calls it unexplained).
        assert_eq!(SOURCE_EXPORT_EVERY_BATCH, 2000);
        assert_eq!(SOURCE_EXPORT_EVERY_LIVE, 1000);
        assert_ne!(
            SOURCE_EXPORT_EVERY_BATCH, SOURCE_EXPORT_EVERY_LIVE,
            "if these ever converge, the recorded divergence must be revisited"
        );

        // The recorded transcript of the real chain shows the batch cadence.
        let transcript = include_str!("../../tests/fixtures/realitylog/dry_run_no_push.txt");
        assert!(transcript.contains("--total-steps 7000"), "{transcript}");
        assert!(transcript.contains("--export-every 2000"), "{transcript}");
        assert!(transcript.contains("splat_7000.ply"), "{transcript}");
        assert!(transcript.contains(&format!("--eval-split-every {SOURCE_EVAL_SPLIT_EVERY}")));
        assert!(transcript.contains(&format!("--eval-every {SOURCE_EVAL_EVERY}")));

        let _ = fs::remove_dir_all(&root);
    }

    /// M8's live half, run against the real checkout: the host's recorded cadence
    /// constants must equal the source's own defaults, read from the source's
    /// own files. Ignored by default because it needs `REALITYLOG_ROOT`.
    #[test]
    #[ignore = "needs REALITYLOG_ROOT pointing at a real hyperscape-lite checkout"]
    fn trainer_cadence_matches_the_sources_own_defaults_and_suite() {
        let root = match std::env::var_os(REALITYLOG_ROOT_ENV) {
            Some(root) => PathBuf::from(root),
            None => {
                eprintln!("{REALITYLOG_ROOT_ENV} is not set; skipping the cadence parity gate");
                return;
            }
        };

        let defaults_path = root.join("scripts/brush_defaults.py");
        let suite_path = root.join("scripts/tests/test_reconstruct_session.py");
        let defaults = fs::read_to_string(&defaults_path)
            .unwrap_or_else(|e| panic!("{} should be readable: {e}", defaults_path.display()));
        let suite = fs::read_to_string(&suite_path)
            .unwrap_or_else(|e| panic!("{} should be readable: {e}", suite_path.display()));

        // Pull `NAME = <int>` out of the source's own defaults module.
        let value_of = |name: &str| -> u32 {
            defaults
                .lines()
                .find_map(|line| {
                    let line = line.trim();
                    let rest = line.strip_prefix(name)?.trim_start();
                    let rest = rest.strip_prefix('=')?.trim();
                    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    digits.parse().ok()
                })
                .unwrap_or_else(|| panic!("{name} not found in {}", defaults_path.display()))
        };

        let total_steps = value_of("TOTAL_STEPS");
        let export_every_batch = value_of("EXPORT_EVERY_BATCH");
        let export_every_live = value_of("EXPORT_EVERY_LIVE");
        let eval_split_every = value_of("EVAL_SPLIT_EVERY");
        let eval_every = value_of("EVAL_EVERY");

        // The source's own suite pins TOTAL_STEPS into the PLY name.
        assert!(
            suite.contains(&format!("splat_{total_steps}.ply")),
            "{} does not pin splat_{total_steps}.ply",
            suite_path.display()
        );

        // The host's constants are the source's values, not re-typed ones.
        assert_eq!(SOURCE_DEFAULT_STEPS, total_steps, "TOTAL_STEPS drifted");
        assert_eq!(
            SOURCE_EXPORT_EVERY_BATCH, export_every_batch,
            "EXPORT_EVERY_BATCH drifted"
        );
        assert_eq!(
            SOURCE_EXPORT_EVERY_LIVE, export_every_live,
            "EXPORT_EVERY_LIVE drifted"
        );
        assert_eq!(
            SOURCE_EVAL_SPLIT_EVERY, eval_split_every,
            "EVAL_SPLIT_EVERY drifted"
        );
        assert_eq!(SOURCE_EVAL_EVERY, eval_every, "EVAL_EVERY drifted");

        eprintln!(
            "source cadence: TOTAL_STEPS={total_steps} EXPORT_EVERY_BATCH={export_every_batch} \
             EXPORT_EVERY_LIVE={export_every_live} EVAL_SPLIT_EVERY={eval_split_every} \
             EVAL_EVERY={eval_every} (host-injected lever: --steps only)"
        );
    }

    /// The env-var override is honoured, and a source with no configured root
    /// at all is refused rather than defaulting to the current directory.
    ///
    /// The environment value is passed in rather than read, so this ordering
    /// test cannot be broken by whatever `REALITYLOG_ROOT` the gate harness
    /// happens to export.
    #[test]
    fn resolution_order_is_explicit_then_env_then_configured() {
        use std::ffi::OsStr;

        let explicit = fixture_source("explicit");
        let from_env = fixture_source("envroot");
        let configured = fixture_source("configured");
        let env_arg: Option<&OsStr> = Some(from_env.as_os_str());

        // 1. The explicit argument wins over both.
        let resolved = resolve_source_with(Some(&explicit), env_arg, Some(&configured)).unwrap();
        assert_eq!(resolved.root, explicit);
        // 2. Then REALITYLOG_ROOT.
        let resolved = resolve_source_with(None, env_arg, Some(&configured)).unwrap();
        assert_eq!(resolved.root, from_env);
        // 3. Then the configured path.
        let resolved = resolve_source_with(None, None, Some(&configured)).unwrap();
        assert_eq!(resolved.root, configured);
        // 4. Nothing at all fails closed, naming all three levers.
        let err = resolve_source_with(None, None::<&OsStr>, None).unwrap_err();
        assert!(
            err.contains("No RealityLog capture source configured"),
            "{err}"
        );
        assert!(err.contains(REALITYLOG_ROOT_ENV), "{err}");
        assert!(err.contains("--realitylog-root"), "{err}");

        // The public entry point delegates to it; with the environment set it
        // must not return the configured path.
        if std::env::var_os(REALITYLOG_ROOT_ENV).is_some() {
            if let Ok(resolved) = resolve_source(None, Some(&configured)) {
                assert_ne!(resolved.root, configured);
            }
        }

        let _ = fs::remove_dir_all(&explicit);
        let _ = fs::remove_dir_all(&from_env);
        let _ = fs::remove_dir_all(&configured);
    }

    /// The M3(d) gate, run against the real checkout when the environment
    /// points at one. Ignored by default because it depends on a machine-local
    /// path and on the source's own untracked reconstruction chain; run it with
    /// `cargo test -- --ignored realitylog`. The caller must compare the
    /// source's `git status --porcelain` hash before and after.
    #[test]
    #[ignore = "needs REALITYLOG_ROOT pointing at a real hyperscape-lite checkout"]
    fn real_checkout_dry_run_leaves_the_source_untouched() {
        let root = match std::env::var_os(REALITYLOG_ROOT_ENV) {
            Some(root) => PathBuf::from(root),
            None => {
                eprintln!("{REALITYLOG_ROOT_ENV} is not set; skipping");
                return;
            }
        };
        let session = fixture_session("real");
        let source = resolve_source(Some(&root), None).expect("real checkout should resolve");
        assert!(
            source.head.is_some(),
            "the real checkout should report a HEAD"
        );

        let mut engines =
            EngineArgs::resolve(&source.root).expect("host engine authority should resolve");
        // The fixture session has no converter venv requirement beyond the
        // interpreter, and the real Brush/mq3drecon binaries exist on this box.
        engines.python = resolve_converter_python(&source.root);

        let outcome = invoke(&source.root, &session, &engines);
        assert_eq!(outcome.exit_code, 0, "stderr:\n{}", outcome.stderr);
        let prefixes = documented_step_prefixes(false);
        assert_eq!(
            outcome.steps.len(),
            prefixes.len(),
            "stdout:\n{}",
            outcome.stdout
        );
        for (step, prefix) in outcome.steps.iter().zip(prefixes.iter()) {
            assert!(
                step.description.starts_with(prefix),
                "{:?}",
                step.description
            );
        }
        eprintln!("source HEAD: {:?}", outcome.source_head);
        eprintln!(
            "stage list: {:?}",
            outcome
                .steps
                .iter()
                .map(|s| &s.description)
                .collect::<Vec<_>>()
        );

        let _ = fs::remove_dir_all(&session);
    }
}
