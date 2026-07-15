//! Batch job queue: serialize GPU-heavy training to one active job at a time,
//! while letting the user enqueue many videos or folders from the UI.
//!
//! Jobs are suite-tagged (reconstruction | geospatial) and persisted to disk so
//! a restart can restore queued / unfinished work. CPU vs GPU lanes are tracked
//! for future concurrent hydro prep without stealing the training GPU slot.

use crate::pipeline::{self, JobCtx, JobEvent, JobHandle};
use crate::project::{Project, Suite};
use crate::settings::{self, ResolvedSettings};
use crate::{engines, profiler};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use tauri::Emitter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ResourceLane {
    #[default]
    Gpu,
    Cpu,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[allow(dead_code)]
pub enum QueueItemState {
    Queued,
    Running,
    Paused,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct QueueItem {
    pub id: String,
    pub input_path: String,
    pub display_name: String,
    pub state: QueueItemState,
    pub job_id: Option<String>,
    pub workspace: Option<String>,
    pub error: Option<String>,
    pub progress: f32,
    pub detail: String,
    #[serde(default)]
    pub suite: Suite,
    #[serde(default)]
    pub lane: ResourceLane,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct QueuePersisted {
    version: u32,
    paused: bool,
    items: Vec<QueueItem>,
}

const PERSIST_VERSION: u32 = 1;

fn queue_persist_path() -> PathBuf {
    settings::app_data_dir().join("queue.json")
}

struct QueueInner {
    items: VecDeque<QueueItem>,
    /// Soft pause: finish the current job, then stop dequeuing.
    paused: bool,
    /// Active GPU-lane job id (training).
    active_gpu_job: Option<String>,
    /// Active CPU-lane job id (future geo prep / hydro).
    active_cpu_job: Option<String>,
}

pub struct Queue {
    inner: Mutex<QueueInner>,
    /// Set while a worker task is pumping the queue.
    pumping: AtomicBool,
}

impl Default for Queue {
    fn default() -> Self {
        Self {
            inner: Mutex::new(QueueInner {
                items: VecDeque::new(),
                paused: false,
                active_gpu_job: None,
                active_cpu_job: None,
            }),
            pumping: AtomicBool::new(false),
        }
    }
}

static QUEUE: OnceLock<Arc<Queue>> = OnceLock::new();

pub fn global() -> Arc<Queue> {
    QUEUE
        .get_or_init(|| {
            let q = Arc::new(Queue::default());
            q.load_from_disk();
            q
        })
        .clone()
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("input")
        .to_string()
}

impl Queue {
    pub fn list(&self) -> Vec<QueueItem> {
        self.inner.lock().unwrap().items.iter().cloned().collect()
    }

    pub fn is_paused(&self) -> bool {
        self.inner.lock().unwrap().paused
    }

    pub fn set_paused(&self, paused: bool) {
        {
            self.inner.lock().unwrap().paused = paused;
        }
        self.persist();
    }

    /// Enqueue one or more inputs for a suite. Returns the new item ids.
    pub fn enqueue(&self, paths: Vec<String>, suite: Suite) -> Vec<String> {
        let lane = match suite {
            Suite::Reconstruction => ResourceLane::Gpu,
            Suite::Geospatial => ResourceLane::Cpu,
        };
        let mut ids = Vec::new();
        {
            let mut guard = self.inner.lock().unwrap();
            for p in paths {
                let path = PathBuf::from(&p);
                let id = format!(
                    "q_{}",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_millis()
                        + ids.len() as u128
                );
                guard.items.push_back(QueueItem {
                    id: id.clone(),
                    input_path: p,
                    display_name: display_name(&path),
                    state: QueueItemState::Queued,
                    job_id: None,
                    workspace: None,
                    error: None,
                    progress: 0.0,
                    detail: "Queued".into(),
                    suite,
                    lane,
                });
                ids.push(id);
            }
        }
        self.persist();
        ids
    }

    pub fn cancel_item(
        &self,
        id: &str,
        jobs: &Mutex<std::collections::HashMap<String, Arc<JobHandle>>>,
    ) {
        {
            let mut guard = self.inner.lock().unwrap();
            if let Some(item) = guard.items.iter_mut().find(|i| i.id == id) {
                match item.state {
                    QueueItemState::Queued | QueueItemState::Paused => {
                        item.state = QueueItemState::Cancelled;
                        item.detail = "Cancelled".into();
                    }
                    QueueItemState::Running => {
                        if let Some(jid) = &item.job_id {
                            if let Some(h) = jobs.lock().unwrap().get(jid) {
                                h.request_cancel();
                            }
                        }
                        item.state = QueueItemState::Cancelled;
                        item.detail = "Cancelling…".into();
                    }
                    _ => {}
                }
            }
            // Drop cancelled queued items from the front of the line.
            while guard
                .items
                .front()
                .map(|i| i.state == QueueItemState::Cancelled && i.job_id.is_none())
                .unwrap_or(false)
            {
                guard.items.pop_front();
            }
        }
        self.persist();
    }

    pub fn clear_finished(&self) {
        {
            let mut guard = self.inner.lock().unwrap();
            guard.items.retain(|i| {
                !matches!(
                    i.state,
                    QueueItemState::Done | QueueItemState::Failed | QueueItemState::Cancelled
                )
            });
        }
        self.persist();
    }

    #[allow(dead_code)]
    pub fn update_progress(&self, job_id: &str, progress: f32, detail: &str) {
        let mut guard = self.inner.lock().unwrap();
        if let Some(item) = guard
            .items
            .iter_mut()
            .find(|i| i.job_id.as_deref() == Some(job_id))
        {
            item.progress = progress.clamp(0.0, 1.0);
            item.detail = detail.to_string();
        }
    }

    pub fn mark_done(&self, job_id: &str, workspace: &str) {
        {
            let mut guard = self.inner.lock().unwrap();
            if let Some(item) = guard
                .items
                .iter_mut()
                .find(|i| i.job_id.as_deref() == Some(job_id))
            {
                item.state = QueueItemState::Done;
                item.progress = 1.0;
                item.detail = "Done".into();
                item.workspace = Some(workspace.to_string());
            }
            clear_active(&mut guard, job_id);
        }
        self.persist();
    }

    pub fn mark_failed(&self, job_id: &str, message: &str) {
        {
            let mut guard = self.inner.lock().unwrap();
            if let Some(item) = guard
                .items
                .iter_mut()
                .find(|i| i.job_id.as_deref() == Some(job_id))
            {
                item.state = QueueItemState::Failed;
                item.error = Some(message.to_string());
                item.detail = message.to_string();
            }
            clear_active(&mut guard, job_id);
        }
        self.persist();
    }

    pub fn mark_cancelled(&self, job_id: &str) {
        {
            let mut guard = self.inner.lock().unwrap();
            if let Some(item) = guard
                .items
                .iter_mut()
                .find(|i| i.job_id.as_deref() == Some(job_id))
            {
                item.state = QueueItemState::Cancelled;
                item.detail = "Cancelled".into();
            }
            clear_active(&mut guard, job_id);
        }
        self.persist();
    }

    fn load_from_disk(&self) {
        let path = queue_persist_path();
        let Ok(text) = fs::read_to_string(&path) else {
            return;
        };
        let Ok(mut data) = serde_json::from_str::<QueuePersisted>(&text) else {
            return;
        };
        // Interrupted runs become queued again so they can be retried after restart.
        for item in &mut data.items {
            if item.state == QueueItemState::Running {
                item.state = QueueItemState::Queued;
                item.job_id = None;
                item.progress = 0.0;
                item.detail = "Re-queued after restart".into();
            }
        }
        // Drop terminal items from a previous session to avoid clutter; keep active queue.
        data.items.retain(|i| {
            matches!(
                i.state,
                QueueItemState::Queued | QueueItemState::Paused
            )
        });
        let mut guard = self.inner.lock().unwrap();
        guard.paused = data.paused;
        guard.items = data.items.into();
        guard.active_gpu_job = None;
        guard.active_cpu_job = None;
    }

    pub fn persist(&self) {
        let (paused, items) = {
            let guard = self.inner.lock().unwrap();
            (guard.paused, guard.items.iter().cloned().collect::<Vec<_>>())
        };
        let data = QueuePersisted {
            version: PERSIST_VERSION,
            paused,
            items,
        };
        let dir = settings::app_data_dir();
        let _ = fs::create_dir_all(&dir);
        if let Ok(json) = serde_json::to_string_pretty(&data) {
            let path = queue_persist_path();
            let tmp = dir.join("queue.json.tmp");
            if fs::write(&tmp, json).is_ok() {
                let _ = fs::remove_file(&path);
                let _ = fs::rename(&tmp, &path);
            }
        }
    }

    /// Start the next queued item if its resource lane is free.
    pub fn try_start_next(
        self: &Arc<Self>,
        app: &tauri::AppHandle,
        jobs: Arc<Mutex<std::collections::HashMap<String, Arc<JobHandle>>>>,
    ) {
        if self.pumping.swap(true, Ordering::SeqCst) {
            return;
        }
        let q = Arc::clone(self);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            loop {
                let next = {
                    let mut guard = q.inner.lock().unwrap();
                    if guard.paused {
                        None
                    } else {
                        let gpu_free = guard.active_gpu_job.is_none();
                        let cpu_free = guard.active_cpu_job.is_none();
                        guard
                            .items
                            .iter_mut()
                            .find(|i| {
                                i.state == QueueItemState::Queued
                                    && match i.lane {
                                        ResourceLane::Gpu => gpu_free,
                                        ResourceLane::Cpu => cpu_free,
                                    }
                            })
                            .map(|i| {
                                i.state = QueueItemState::Running;
                                i.detail = "Starting…".into();
                                (
                                    i.id.clone(),
                                    i.input_path.clone(),
                                    i.suite,
                                    i.lane,
                                )
                            })
                    }
                };

                let Some((queue_id, input_path, suite, lane)) = next else {
                    break;
                };

                // Geospatial lane: scaffold workspace + project only (no flood
                // solver yet). Completes synchronously — do not take a lane slot.
                if suite == Suite::Geospatial {
                    if let Err(e) =
                        start_geo_placeholder(&app, &q, &queue_id, &input_path).await
                    {
                        let mut guard = q.inner.lock().unwrap();
                        if let Some(item) = guard.items.iter_mut().find(|i| i.id == queue_id) {
                            item.state = QueueItemState::Failed;
                            item.error = Some(e.clone());
                            item.detail = e;
                        }
                        drop(guard);
                        q.persist();
                    }
                    emit_snapshot(&app, &q);
                    continue;
                }

                match start_one(&app, &jobs, &q, &queue_id, &input_path).await {
                    Ok(job_id) => {
                        {
                            let mut guard = q.inner.lock().unwrap();
                            match lane {
                                ResourceLane::Gpu => guard.active_gpu_job = Some(job_id.clone()),
                                ResourceLane::Cpu => guard.active_cpu_job = Some(job_id.clone()),
                            }
                        }
                        q.persist();
                        // Wait until this job leaves its active slot.
                        while {
                            let guard = q.inner.lock().unwrap();
                            match lane {
                                ResourceLane::Gpu => {
                                    guard.active_gpu_job.as_deref() == Some(job_id.as_str())
                                }
                                ResourceLane::Cpu => {
                                    guard.active_cpu_job.as_deref() == Some(job_id.as_str())
                                }
                            }
                        } {
                            tokio::time::sleep(std::time::Duration::from_millis(400)).await;
                        }
                    }
                    Err(e) => {
                        {
                            let mut guard = q.inner.lock().unwrap();
                            if let Some(item) = guard.items.iter_mut().find(|i| i.id == queue_id) {
                                item.state = QueueItemState::Failed;
                                item.error = Some(e.clone());
                                item.detail = e;
                            }
                        }
                        q.persist();
                    }
                }
                emit_snapshot(&app, &q);
            }
            q.pumping.store(false, Ordering::SeqCst);
            emit_snapshot(&app, &q);
            // Another enqueue may have raced while we were shutting down.
            let has_queued = {
                let guard = q.inner.lock().unwrap();
                !guard.paused
                    && guard
                        .items
                        .iter()
                        .any(|i| i.state == QueueItemState::Queued)
            };
            if has_queued {
                q.try_start_next(&app, jobs);
            }
        });
    }
}

fn clear_active(guard: &mut QueueInner, job_id: &str) {
    if guard.active_gpu_job.as_deref() == Some(job_id) {
        guard.active_gpu_job = None;
    }
    if guard.active_cpu_job.as_deref() == Some(job_id) {
        guard.active_cpu_job = None;
    }
}

fn emit_snapshot(app: &tauri::AppHandle, q: &Queue) {
    let _ = app.emit(
        "queue://snapshot",
        serde_json::json!({
            "items": q.list(),
            "paused": q.is_paused(),
        }),
    );
}

/// Create a geospatial project workspace without running flood solvers yet.
async fn start_geo_placeholder(
    app: &tauri::AppHandle,
    q: &Arc<Queue>,
    queue_id: &str,
    input_path: &str,
) -> Result<String, String> {
    let input = PathBuf::from(input_path);
    if !input.exists() {
        return Err("Input path does not exist.".into());
    }

    let job_id = format!(
        "geo_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let workspace = pipeline::jobs_dir().join(&job_id);
    std::fs::create_dir_all(&workspace).map_err(|e| e.to_string())?;
    crate::geospatial::prepare_workspace(&workspace)?;

    let profile = profiler::profile();
    let resolved: ResolvedSettings = settings::resolve(&settings::Settings::load(), &profile);
    let proj = Project::new_with_suite(
        &job_id,
        &input,
        &workspace,
        &resolved,
        Suite::Geospatial,
    );
    proj.save()?;

    {
        let mut guard = q.inner.lock().unwrap();
        if let Some(item) = guard.items.iter_mut().find(|i| i.id == queue_id) {
            item.job_id = Some(job_id.clone());
            item.workspace = Some(workspace.to_string_lossy().into_owned());
            item.detail = "Geospatial workspace prepared".into();
            item.state = QueueItemState::Done;
            item.progress = 1.0;
        }
    }
    q.persist();
    emit_snapshot(app, q);
    Ok(job_id)
}

async fn start_one(
    app: &tauri::AppHandle,
    jobs: &Mutex<std::collections::HashMap<String, Arc<JobHandle>>>,
    q: &Arc<Queue>,
    queue_id: &str,
    input_path: &str,
) -> Result<String, String> {
    let input = PathBuf::from(input_path);
    if !input.exists() {
        return Err("Input path does not exist.".into());
    }
    let st = engines::status();
    if !st.colmap || !(st.brush || st.gsplat) {
        return Err("__engines_missing__".into());
    }
    if !st.ffmpeg && input.is_file() {
        return Err(
            "FFmpeg was not found. Install it (winget install ffmpeg) or drop an image folder instead."
                .into(),
        );
    }

    let job_id = format!(
        "job_{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );
    let workspace = pipeline::jobs_dir().join(&job_id);
    std::fs::create_dir_all(&workspace).map_err(|e| e.to_string())?;

    let profile = profiler::profile();
    let resolved: ResolvedSettings = settings::resolve(&settings::Settings::load(), &profile);
    let proj = Project::new(&job_id, &input, &workspace, &resolved);
    proj.save()?;

    {
        let mut guard = q.inner.lock().unwrap();
        if let Some(item) = guard.items.iter_mut().find(|i| i.id == queue_id) {
            item.job_id = Some(job_id.clone());
            item.workspace = Some(workspace.to_string_lossy().into_owned());
            item.detail = "Running".into();
            item.suite = Suite::Reconstruction;
            item.lane = ResourceLane::Gpu;
        }
    }
    q.persist();
    emit_snapshot(app, q);

    let cancel = Arc::new(AtomicBool::new(false));
    let child_pids = Arc::new(Mutex::new(Vec::new()));
    let handle = Arc::new(JobHandle {
        cancel: cancel.clone(),
        child_pids: child_pids.clone(),
    });
    jobs.lock().unwrap().insert(job_id.clone(), handle);

    let ctx = JobCtx {
        app: app.clone(),
        job_id: job_id.clone(),
        workspace: workspace.clone(),
        settings: resolved,
        cancel,
        child_pids,
        project: Arc::new(Mutex::new(proj)),
    };

    let run_ctx = ctx.clone();
    let q2 = Arc::clone(q);
    let app2 = app.clone();
    let jid = job_id.clone();
    tauri::async_runtime::spawn(async move {
        match pipeline::run_job(&run_ctx, &input).await {
            Ok(path) => {
                let ws = path
                    .parent()
                    .map(|p| p.to_string_lossy().into_owned())
                    .unwrap_or_default();
                q2.mark_done(&jid, &ws);
            }
            Err(e) if e == "__cancelled__" => {
                pipeline::discard_workspace(&run_ctx.workspace);
                let _ = app2.emit(
                    "job://event",
                    JobEvent::Cancelled {
                        job_id: jid.clone(),
                    },
                );
                q2.mark_cancelled(&jid);
            }
            Err(e) => {
                let _ = app2.emit(
                    "job://event",
                    JobEvent::Error {
                        job_id: jid.clone(),
                        message: e.clone(),
                    },
                );
                q2.mark_failed(&jid, &e);
            }
        }
        emit_snapshot(&app2, &q2);
    });

    Ok(job_id)
}

pub fn emit_now(app: &tauri::AppHandle) {
    emit_snapshot(app, &global());
}
