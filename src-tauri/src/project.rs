//! Project bundles (ROADMAP-V2 1.4).
//!
//! Every job writes a `project.json` next to its workspace holding the input
//! reference, the resolved settings, where the solved poses live and which
//! splat is current. That is everything needed to reopen a finished scene or
//! resume an interrupted run, so closing the app mid-training is not fatal.

use crate::settings::ResolvedSettings;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

pub const PROJECT_FILE: &str = "project.json";
const PROJECT_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub version: u32,
    pub job_id: String,
    pub created_unix: u64,
    pub updated_unix: u64,
    pub input_path: String,
    pub workspace: String,
    pub settings: ResolvedSettings,

    /// Directory holding `cameras.*` and `images.*`, once SfM has run.
    pub sparse_dir: Option<String>,
    /// Most recent splat written by training, absolute path.
    pub latest_splat: Option<String>,
    pub latest_iter: u32,
    pub total_steps: u32,
    pub completed: bool,
    /// Orientation the user set in the viewport, row-major 3x3.
    pub model_rotation: Option<[f32; 9]>,
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl Project {
    pub fn new(
        job_id: &str,
        input_path: &Path,
        workspace: &Path,
        settings: &ResolvedSettings,
    ) -> Project {
        let t = now_unix();
        Project {
            version: PROJECT_VERSION,
            job_id: job_id.to_string(),
            created_unix: t,
            updated_unix: t,
            input_path: input_path.to_string_lossy().into_owned(),
            workspace: workspace.to_string_lossy().into_owned(),
            settings: settings.clone(),
            sparse_dir: None,
            latest_splat: None,
            latest_iter: 0,
            total_steps: settings.total_steps,
            completed: false,
            model_rotation: None,
        }
    }

    pub fn path(workspace: &Path) -> PathBuf {
        workspace.join(PROJECT_FILE)
    }

    /// Write atomically: a crash mid-save must not truncate an existing
    /// manifest, or a resumable job becomes unresumable.
    pub fn save(&self) -> Result<(), String> {
        let ws = PathBuf::from(&self.workspace);
        fs::create_dir_all(&ws).map_err(|e| e.to_string())?;
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = ws.join(format!("{PROJECT_FILE}.tmp"));
        fs::write(&tmp, json).map_err(|e| e.to_string())?;
        let dest = Project::path(&ws);
        // Windows rename fails if the destination exists.
        let _ = fs::remove_file(&dest);
        fs::rename(&tmp, &dest).map_err(|e| e.to_string())
    }

    pub fn load(workspace: &Path) -> Result<Project, String> {
        let p = Project::path(workspace);
        let text = fs::read_to_string(&p)
            .map_err(|e| format!("Cannot read {}: {e}", p.display()))?;
        let proj: Project = serde_json::from_str(&text)
            .map_err(|e| format!("{} is not a valid project file: {e}", p.display()))?;
        if proj.version > PROJECT_VERSION {
            return Err(format!(
                "This project was written by a newer version of InstaSplatter (format {}).",
                proj.version
            ));
        }
        Ok(proj)
    }

    pub fn touch(&mut self) {
        self.updated_unix = now_unix();
    }

    /// True when training stopped part way and enough state survives to pick
    /// it up again: poses on disk and at least one exported checkpoint.
    pub fn is_resumable(&self) -> bool {
        if self.completed || self.latest_iter == 0 || self.latest_iter >= self.total_steps {
            return false;
        }
        let splat_ok = self
            .latest_splat
            .as_ref()
            .map(|p| Path::new(p).exists())
            .unwrap_or(false);
        let poses_ok = self
            .sparse_dir
            .as_ref()
            .map(|p| crate::colmap::find_model_dir(Path::new(p)).is_some())
            .unwrap_or(false);
        splat_ok && poses_ok
    }
}

/// Summary shown in the "reopen a project" list.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub job_id: String,
    pub workspace: String,
    pub input_name: String,
    pub updated_unix: u64,
    pub completed: bool,
    pub resumable: bool,
    pub latest_iter: u32,
    pub total_steps: u32,
    pub result_path: Option<String>,
}

impl From<&Project> for ProjectSummary {
    fn from(p: &Project) -> ProjectSummary {
        let result = Path::new(&p.workspace).join("result.ply");
        ProjectSummary {
            job_id: p.job_id.clone(),
            workspace: p.workspace.clone(),
            input_name: Path::new(&p.input_path)
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| p.input_path.clone()),
            updated_unix: p.updated_unix,
            completed: p.completed,
            resumable: p.is_resumable(),
            latest_iter: p.latest_iter,
            total_steps: p.total_steps,
            result_path: result.exists().then(|| result.to_string_lossy().into_owned()),
        }
    }
}

/// All projects under the jobs directory, newest first.
pub fn list_projects(jobs_dir: &Path) -> Vec<ProjectSummary> {
    let mut out: Vec<ProjectSummary> = fs::read_dir(jobs_dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter_map(|d| Project::load(&d).ok())
        .map(|p| ProjectSummary::from(&p))
        .collect();
    out.sort_by(|a, b| b.updated_unix.cmp(&a.updated_unix));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profiler::Preset;

    fn settings() -> ResolvedSettings {
        ResolvedSettings {
            preset: Preset::Balanced,
            max_frames: 100,
            max_resolution: 1280,
            blur_reject_fraction: 0.15,
            matcher: "auto".into(),
            sift_gpu: true,
            total_steps: 12000,
            max_splats: 3_000_000,
            sh_degree: 3,
            refine_every: 200,
            ssim_weight: 0.2,
            export_every: 500,
            progressive_resolution: false,
            mip_filter: false,
            live_init: false,
            dense_init: true,
            use_neural_init: true,
            allow_research_sidecars: false,
            experimental_mode: false,
            experimental_license_acked: false,
            post_polish: true,
            trainer: "brush".into(),
            gsplat_strategy: "mcmc".into(),
            gsplat_absgrad: true,
            gsplat_antialiased: true,
            gsplat_appearance: true,
            gsplat_bilateral_grid: true,
            roma_quality: "base".into(),
            strictness: 0.5,
            export_format: "ply".into(),
            keep_intermediates: false,
            opac_loss_weight: 1e-9,
            scale_loss_weight: 1e-8,
            mean_noise_weight: 40.0,
        }
    }

    fn temp(name: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("instasplatter_project_{name}"));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        p
    }

    #[test]
    fn project_roundtrips_through_disk() {
        let ws = temp("roundtrip");
        let mut p = Project::new("job_1", Path::new("C:/in/clip.mp4"), &ws, &settings());
        p.latest_iter = 500;
        p.save().unwrap();

        let back = Project::load(&ws).unwrap();
        assert_eq!(back.job_id, "job_1");
        assert_eq!(back.latest_iter, 500);
        assert_eq!(back.total_steps, 12000);
        assert_eq!(back.settings.preset, Preset::Balanced);
        assert!(!back.completed);
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn saving_twice_replaces_rather_than_failing() {
        let ws = temp("resave");
        let mut p = Project::new("job_2", Path::new("in"), &ws, &settings());
        p.save().unwrap();
        p.latest_iter = 999;
        p.save().unwrap();
        assert_eq!(Project::load(&ws).unwrap().latest_iter, 999);
        // No temp file is left behind.
        assert!(!ws.join(format!("{PROJECT_FILE}.tmp")).exists());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn a_future_format_version_is_refused() {
        let ws = temp("future");
        let mut p = Project::new("job_3", Path::new("in"), &ws, &settings());
        p.version = PROJECT_VERSION + 1;
        p.save().unwrap();
        let err = Project::load(&ws).unwrap_err();
        assert!(err.contains("newer version"), "{err}");
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn resumable_requires_poses_a_checkpoint_and_unfinished_training() {
        let ws = temp("resumable");
        let sparse = ws.join("sparse").join("0");
        fs::create_dir_all(&sparse).unwrap();
        fs::write(sparse.join("cameras.txt"), "").unwrap();
        let splat = ws.join("exports").join("export_500.ply");
        fs::create_dir_all(splat.parent().unwrap()).unwrap();
        fs::write(&splat, b"x").unwrap();

        let mut p = Project::new("job_4", Path::new("in"), &ws, &settings());
        assert!(!p.is_resumable(), "no checkpoint yet");

        p.latest_iter = 500;
        p.latest_splat = Some(splat.to_string_lossy().into_owned());
        p.sparse_dir = Some(ws.join("sparse").to_string_lossy().into_owned());
        assert!(p.is_resumable());

        // A finished job is not resumable.
        p.completed = true;
        assert!(!p.is_resumable());
        p.completed = false;

        // Neither is one that already reached the last step.
        p.latest_iter = p.total_steps;
        assert!(!p.is_resumable());
        p.latest_iter = 500;

        // Nor one whose checkpoint has been deleted.
        fs::remove_file(&splat).unwrap();
        assert!(!p.is_resumable());
        let _ = fs::remove_dir_all(&ws);
    }

    #[test]
    fn listing_projects_sorts_newest_first_and_skips_junk() {
        let jobs = temp("list");
        for (i, id) in ["job_a", "job_b"].iter().enumerate() {
            let ws = jobs.join(id);
            fs::create_dir_all(&ws).unwrap();
            let mut p = Project::new(id, Path::new("C:/in/clip.mp4"), &ws, &settings());
            p.updated_unix = 1000 + i as u64;
            p.save().unwrap();
        }
        // A directory with no manifest must be ignored, not crash the listing.
        fs::create_dir_all(jobs.join("not_a_project")).unwrap();

        let list = list_projects(&jobs);
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].job_id, "job_b");
        assert_eq!(list[0].input_name, "clip.mp4");
        assert!(list[0].result_path.is_none());
        let _ = fs::remove_dir_all(&jobs);
    }
}
