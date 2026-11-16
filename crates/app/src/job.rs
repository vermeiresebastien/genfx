//! Export jobs: a frozen project snapshot, a queue with persisted state,
//! progress, and cancellation. A queued export never mutates the live document;
//! editing continues while the job uses the snapshot.

use crate::export::{export_project, ExportOptions, ExportResult};
use flux_domain::{FluxError, FluxResult, Project};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum JobState {
    Queued,
    Running,
    Done,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportJobDesc {
    pub id: String,
    pub out_dir: String,
    pub project_name: String,
    pub codec: String,
    pub width: u32,
    pub height: u32,
    pub state: JobState,
    pub frames_done: u64,
    pub frames_total: u64,
    pub error: Option<String>,
    /// The frozen project snapshot (JSON) that the job renders.
    pub snapshot: String,
}

impl ExportJobDesc {
    pub fn from_project(
        project: &Project,
        out_dir: &Path,
        codec: &str,
        w: u32,
        h: u32,
    ) -> FluxResult<Self> {
        Ok(Self {
            id: uuid::Uuid::new_v4().to_string(),
            out_dir: out_dir.display().to_string(),
            project_name: project.name.clone(),
            codec: codec.to_string(),
            width: w,
            height: h,
            state: JobState::Queued,
            frames_done: 0,
            frames_total: 0,
            error: None,
            snapshot: serde_json::to_string(project)
                .map_err(|e| FluxError::new(flux_domain::ErrorCode::Io, "export", e.to_string()))?,
        })
    }

    pub fn snapshot_project(&self) -> FluxResult<Project> {
        serde_json::from_str(&self.snapshot).map_err(|e| {
            FluxError::new(
                flux_domain::ErrorCode::ProjectCorrupt,
                "export",
                e.to_string(),
            )
        })
    }
}

/// A persisted queue of export jobs (state.json in the export dir).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExportQueue {
    pub jobs: Vec<ExportJobDesc>,
}

impl ExportQueue {
    pub fn load(queue_file: &Path) -> Self {
        std::fs::read_to_string(queue_file)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, queue_file: &Path) -> FluxResult<()> {
        if let Some(parent) = queue_file.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| FluxError::new(flux_domain::ErrorCode::Io, "export", e.to_string()))?;
        std::fs::write(queue_file, json)?;
        Ok(())
    }

    pub fn push(&mut self, job: ExportJobDesc) {
        self.jobs.push(job);
    }

    pub fn update(&mut self, id: &str, f: impl FnOnce(&mut ExportJobDesc)) {
        if let Some(j) = self.jobs.iter_mut().find(|j| j.id == id) {
            f(j);
        }
    }
}

/// A handle to a running export job for progress/cancel.
pub struct RunningJob {
    pub id: String,
    pub done: Arc<AtomicU64>,
    pub total: Arc<AtomicU64>,
    pub cancel: Arc<AtomicBool>,
}

/// Run a job synchronously from its snapshot, reporting progress through atomics.
pub fn run_job(
    desc: &ExportJobDesc,
    codec: flux_media::EncodeCodec,
    handle: &RunningJob,
) -> FluxResult<ExportResult> {
    let project = desc.snapshot_project()?;
    let done = handle.done.clone();
    let total = handle.total.clone();
    let result = export_project(
        &project,
        &ExportOptions {
            out_dir: PathBuf::from(&desc.out_dir),
            project_name: desc.project_name.clone(),
            codec,
            width: desc.width,
            height: desc.height,
            range: None,
            cancel: handle.cancel.clone(),
            feature_source: flux_domain::params::FeatureSource::DocumentOnly,
        },
        Some(Box::new(move |i, n| {
            done.store(i, Ordering::Relaxed);
            total.store(n, Ordering::Relaxed);
        })),
    );
    result
}

/// Parse a codec name to an `EncodeCodec`.
pub fn parse_codec(name: &str) -> flux_media::EncodeCodec {
    match name.to_ascii_lowercase().as_str() {
        "hevc" | "h265" => flux_media::EncodeCodec::Hevc,
        "prores" => flux_media::EncodeCodec::ProRes,
        "gif" => flux_media::EncodeCodec::Gif,
        "jpeg" | "jpg" | "jpeg_sequence" => flux_media::EncodeCodec::JpegSequence,
        "png" | "none" => flux_media::EncodeCodec::None,
        _ => flux_media::EncodeCodec::H264,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_domain::Project;

    #[test]
    fn snapshot_is_independent_of_live_edits() {
        let mut p = Project::new("job", 1);
        p.name = "before".into();
        let desc = ExportJobDesc::from_project(&p, Path::new("out"), "h264", 64, 64).unwrap();
        // Mutate the live project; the snapshot must not change.
        p.name = "after".into();
        let snap = desc.snapshot_project().unwrap();
        assert_eq!(snap.name, "before");
    }

    #[test]
    fn queue_persists_state() {
        let dir = std::env::temp_dir().join(format!("flux-queue-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let qf = dir.join("queue.json");
        let p = Project::new("q", 1);
        let mut q = ExportQueue::default();
        let desc = ExportJobDesc::from_project(&p, &dir, "h264", 32, 32).unwrap();
        let id = desc.id.clone();
        q.push(desc);
        q.update(&id, |j| j.state = JobState::Done);
        q.save(&qf).unwrap();
        let back = ExportQueue::load(&qf);
        assert_eq!(back.jobs.len(), 1);
        assert_eq!(back.jobs[0].state, JobState::Done);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn codec_parsing() {
        assert_eq!(parse_codec("hevc"), flux_media::EncodeCodec::Hevc);
        assert_eq!(parse_codec("gif"), flux_media::EncodeCodec::Gif);
        assert_eq!(parse_codec("h264"), flux_media::EncodeCodec::H264);
    }
}
