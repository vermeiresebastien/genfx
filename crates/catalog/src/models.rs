//! External model / runtime capability registry for conditional generator nodes
//! (G170–G185). Conditional nodes stay **editable** when their model is absent;
//! they report an actionable status instead of rendering or crashing.
//!
//! Model presence is determined by:
//!  1. an explicit `FLUX_MODELS_DIR` scan, and
//!  2. a per-model `FLUX_MODEL_<ID>` path override,
//!  3. otherwise "missing".
//! This never fabricates a model. Tests exercise present/missing/incompatible
//! cases via the override so behavior is deterministic without shipping models.

use serde::Serialize;
use std::path::{Path, PathBuf};

/// Lifecycle state of a model-backed node.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ModelState {
    /// Model files present and version-compatible.
    Available,
    /// No model installed for this node.
    Missing,
    /// Model files present but the declared version is unsupported.
    Incompatible,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelStatus {
    pub node_type: String,
    pub requirement: String,
    pub state: ModelState,
    pub message: String,
    /// Where the model was expected (redacted to a file name in diagnostics).
    pub expected: Option<String>,
}

/// Expected model version scheme for a requirement id. Conditional nodes that
/// expect a neural model require a version >= MIN; others (e.g. G180 custom WGSL,
/// G185 external feed) are runtime-provided and are treated as always-available
/// only if their provider is configured.
fn model_requirement(requirement: &str) -> Option<ModelSpec> {
    // (env suffix, minimum version, human label). Only true model-backed rows.
    let spec = match requirement {
        "G170" => ("optical_flow", 1, "Optical flow model"),
        "G171" => ("landmarks", 1, "Face/hand/pose landmark model"),
        "G172" => ("segmentation", 1, "Segmentation model"),
        "G173" => ("depth", 1, "Monocular depth model"),
        "G174" => ("tracking", 1, "Object tracking model"),
        "G176" => ("gaussian_splat", 1, "Gaussian splat dataset"),
        "G177" => ("neural_animation", 1, "Neural animation model"),
        _ => return None,
    };
    Some(ModelSpec {
        env_key: spec.0,
        min_version: spec.1,
        label: spec.2,
    })
}

struct ModelSpec {
    env_key: &'static str,
    min_version: u32,
    label: &'static str,
}

/// A model manifest file (`model.json`) beside the model data:
/// `{ "id": "...", "version": 1 }`.
#[derive(serde::Deserialize)]
struct ModelManifest {
    #[serde(default)]
    version: u32,
}

/// Resolve the status of a model-backed node by requirement id.
pub fn status_for(requirement: &str) -> Option<ModelStatus> {
    let spec = model_requirement(requirement)?;
    let node_type = requirement.to_string();

    // 1) Explicit per-model override: FLUX_MODEL_<ENV_KEY> = path to model dir.
    let override_key = format!("FLUX_MODEL_{}", spec.env_key.to_uppercase());
    if let Ok(dir) = std::env::var(&override_key) {
        return Some(inspect(
            &node_type,
            requirement,
            spec.label,
            PathBuf::from(dir),
        ));
    }

    // 2) Scan FLUX_MODELS_DIR/<env_key>.
    if let Ok(base) = std::env::var("FLUX_MODELS_DIR") {
        let dir = Path::new(&base).join(spec.env_key);
        if dir.exists() {
            return Some(inspect(&node_type, requirement, spec.label, dir));
        }
    }

    // 3) Missing (never fabricate).
    Some(ModelStatus {
        node_type,
        requirement: requirement.to_string(),
        state: ModelState::Missing,
        message: format!(
            "{} is not installed. Set FLUX_MODEL_{} to the model directory. The node \
             stays editable; add the model to render it.",
            spec.label,
            spec.env_key.to_uppercase()
        ),
        expected: None,
    })
}

fn inspect(node_type: &str, requirement: &str, label: &str, dir: PathBuf) -> ModelStatus {
    if !dir.exists() {
        return ModelStatus {
            node_type: node_type.to_string(),
            requirement: requirement.to_string(),
            state: ModelState::Missing,
            message: format!("{label}: model directory not found (node stays editable)."),
            expected: dir.file_name().map(|s| s.to_string_lossy().into_owned()),
        };
    }
    // Read model.json if present; a missing/broken manifest is Incompatible, not
    // a silent success.
    let manifest_path = dir.join("model.json");
    let min = model_requirement(requirement)
        .map(|s| s.min_version)
        .unwrap_or(1);
    match std::fs::read_to_string(&manifest_path) {
        Ok(txt) => match serde_json::from_str::<ModelManifest>(&txt) {
            Ok(m) if m.version >= min => ModelStatus {
                node_type: node_type.to_string(),
                requirement: requirement.to_string(),
                state: ModelState::Available,
                message: format!("{label}: present (v{}).", m.version),
                expected: Some("model.json".into()),
            },
            Ok(m) => ModelStatus {
                node_type: node_type.to_string(),
                requirement: requirement.to_string(),
                state: ModelState::Incompatible,
                message: format!(
                    "{label}: version {} is older than required v{min}. Update the model.",
                    m.version
                ),
                expected: Some("model.json".into()),
            },
            Err(e) => ModelStatus {
                node_type: node_type.to_string(),
                requirement: requirement.to_string(),
                state: ModelState::Incompatible,
                message: format!("{label}: model.json is invalid ({e}). Reinstall the model."),
                expected: Some("model.json".into()),
            },
        },
        Err(_) => ModelStatus {
            node_type: node_type.to_string(),
            requirement: requirement.to_string(),
            state: ModelState::Incompatible,
            message: format!("{label}: model.json missing; cannot verify version."),
            expected: Some("model.json".into()),
        },
    }
}

/// All model-backed conditional nodes and their current status.
pub fn all_statuses() -> Vec<ModelStatus> {
    ["G170", "G171", "G172", "G173", "G174", "G176", "G177"]
        .iter()
        .filter_map(|r| status_for(r))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    // These tests mutate process-global env vars (FLUX_MODEL_*), so they must not
    // run concurrently. Serialize them.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn lock() -> std::sync::MutexGuard<'static, ()> {
        ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner())
    }

    #[test]
    fn missing_when_unconfigured() {
        let _g = lock();
        std::env::remove_var("FLUX_MODEL_DEPTH");
        std::env::remove_var("FLUX_MODELS_DIR");
        let s = status_for("G173").expect("depth status");
        assert_eq!(s.state, ModelState::Missing);
        assert!(s.message.contains("editable"));
    }

    #[test]
    fn present_and_available_when_manifest_valid() {
        let _g = lock();
        let dir = std::env::temp_dir().join(format!("flux-model-depth-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("model.json"), r#"{"version":2}"#).unwrap();
        std::env::set_var("FLUX_MODEL_DEPTH", &dir);
        let s = status_for("G173").expect("status");
        assert_eq!(s.state, ModelState::Available);
        std::env::remove_var("FLUX_MODEL_DEPTH");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn incompatible_when_version_too_old() {
        let _g = lock();
        let dir = std::env::temp_dir().join(format!("flux-model-old-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("model.json"), r#"{"version":0}"#).unwrap();
        std::env::set_var("FLUX_MODEL_DEPTH", &dir);
        let s = status_for("G173").expect("status");
        assert_eq!(s.state, ModelState::Incompatible);
        std::env::remove_var("FLUX_MODEL_DEPTH");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn invalid_manifest_is_incompatible() {
        let _g = lock();
        let dir = std::env::temp_dir().join(format!("flux-model-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("model.json"), "not json").unwrap();
        std::env::set_var("FLUX_MODEL_DEPTH", &dir);
        let s = status_for("G173").expect("status");
        assert_eq!(s.state, ModelState::Incompatible);
        std::env::remove_var("FLUX_MODEL_DEPTH");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn non_model_rows_have_no_status() {
        let _g = lock();
        assert!(status_for("G001").is_none());
        assert!(status_for("G180").is_none()); // custom WGSL: runtime, not a model
    }
}
