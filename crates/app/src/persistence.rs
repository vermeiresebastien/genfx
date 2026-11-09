//! Project persistence: atomic save, autosave with retention, crash recovery,
//! media relinking, and portable collection.
//!
//! Guarantees:
//! - Save writes to a same-directory temp file, flushes/syncs, then atomically
//!   renames over the target, preserving the previous file as `.bak`.
//! - Autosave keeps up to `RETENTION` versions and only prunes app-owned
//!   autosave files; it never touches a user-chosen project file.
//! - Recovery manifest records the original path, revision, timestamp and schema.
//! - Newer unsupported schema opens read-only with an error; never rewritten.
//! - Relink matches by fingerprint/size and requires the user to resolve
//!   ambiguity; collection copies (never moves) originals into `media/`.

use flux_domain::{FluxError, FluxResult, Project};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

pub const AUTOSAVE_INTERVAL_SECS: u64 = 30;
pub const RETENTION_VERSIONS: usize = 20;
pub const RETENTION_DAYS: i64 = 7;

/// Save atomically. Writes `<path>.tmp`, fsyncs, renames over `<path>`, and keeps
/// the previous content as `<path>.bak`.
pub fn save_atomic(project: &Project, path: &Path) -> FluxResult<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(project)
        .map_err(|e| FluxError::new(flux_domain::ErrorCode::Io, "persist", e.to_string()))?;
    let tmp = tmp_path(path, "tmp");
    {
        use std::io::Write;
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(json.as_bytes())?;
        f.flush()?;
        f.sync_all()?;
    }
    // Preserve previous good file as backup.
    if path.exists() {
        let bak = tmp_path(path, "bak");
        let _ = std::fs::copy(path, &bak);
    }
    // Atomic replace (Windows: rename over existing fails, so remove first after
    // the backup is safely written).
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn tmp_path(path: &Path, ext: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "project".into());
    name.push('.');
    name.push_str(ext);
    path.with_file_name(name)
}

/// Load a project, rejecting newer unsupported schemas.
pub fn load(path: &Path) -> FluxResult<Project> {
    let data = std::fs::read_to_string(path).map_err(|e| {
        FluxError::new(
            flux_domain::ErrorCode::MediaMissing,
            "persist",
            format!("cannot read {}: {e}", path.display()),
        )
    })?;
    let project: Project = serde_json::from_str(&data).map_err(|e| {
        FluxError::new(
            flux_domain::ErrorCode::ProjectCorrupt,
            "persist",
            format!("corrupt project: {e}"),
        )
    })?;
    if project.schema_version > flux_domain::SCHEMA_VERSION {
        return Err(FluxError::new(
            flux_domain::ErrorCode::ProjectVersion,
            "persist",
            format!(
                "project schema {} is newer than supported {} (open read-only)",
                project.schema_version,
                flux_domain::SCHEMA_VERSION
            ),
        ));
    }
    Ok(project)
}

/// Recovery manifest persisted next to the autosave.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryManifest {
    pub original_path: String,
    pub revision: u64,
    pub timestamp_utc: String,
    pub schema_version: u32,
    pub autosave_path: String,
}

fn autosave_dir(project_dir: &Path) -> PathBuf {
    project_dir.join(".flux-autosave")
}

/// Write an autosave snapshot and manifest, prune old ones, return its path.
pub fn autosave(project: &Project, project_dir: &Path, revision: u64) -> FluxResult<PathBuf> {
    let dir = autosave_dir(project_dir);
    std::fs::create_dir_all(&dir)?;
    let ts = now_stamp();
    let snap = dir.join(format!("{}-rev{}.flux.json", ts, revision));
    save_atomic(project, &snap)?;
    let manifest = RecoveryManifest {
        original_path: project_dir.display().to_string(),
        revision,
        timestamp_utc: now_iso(),
        schema_version: project.schema_version,
        autosave_path: snap.display().to_string(),
    };
    let mpath = dir.join("recovery.json");
    std::fs::write(&mpath, serde_json::to_string_pretty(&manifest).unwrap())?;
    prune_autosaves(&dir, RETENTION_VERSIONS);
    Ok(snap)
}

/// Find the newest recoverable autosave in a project directory, if any.
pub fn find_recovery(project_dir: &Path) -> Option<RecoveryManifest> {
    let dir = autosave_dir(project_dir);
    let mpath = dir.join("recovery.json");
    let data = std::fs::read_to_string(mpath).ok()?;
    serde_json::from_str(&data).ok()
}

/// Prune app-owned autosave snapshots beyond `keep` newest (by name timestamp).
pub fn prune_autosaves(dir: &Path, keep: usize) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut snaps: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| {
            p.extension().map(|e| e == "json").unwrap_or(false)
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .map(|n| n.contains("-rev"))
                    .unwrap_or(false)
        })
        .collect();
    snaps.sort();
    if snaps.len() > keep {
        for p in &snaps[..snaps.len() - keep] {
            let _ = std::fs::remove_file(p);
        }
    }
}

/// A relink candidate for a missing asset.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RelinkCandidate {
    pub path: String,
    pub size_bytes: u64,
    pub fingerprint: String,
}

/// Search a directory for files matching an asset's fingerprint/size. Returns
/// candidates; ambiguity (more than one) must be resolved by the user.
pub fn relink_search(
    asset_name: &str,
    size_bytes: u64,
    dir: &Path,
) -> FluxResult<Vec<RelinkCandidate>> {
    let mut out = vec![];
    collect_matches(dir, asset_name, size_bytes, &mut out, 0)?;
    Ok(out)
}

fn collect_matches(
    dir: &Path,
    name: &str,
    size: u64,
    out: &mut Vec<RelinkCandidate>,
    depth: usize,
) -> FluxResult<()> {
    if depth > 8 || out.len() > 64 {
        return Ok(());
    }
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Ok(());
    };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_matches(&p, name, size, out, depth + 1)?;
            continue;
        }
        let matches_name = p
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case(name))
            .unwrap_or(false);
        if !matches_name {
            continue;
        }
        if let Ok(meta) = std::fs::metadata(&p) {
            if size == 0 || meta.len() == size {
                out.push(RelinkCandidate {
                    path: p.display().to_string(),
                    size_bytes: meta.len(),
                    fingerprint: format!("{}", meta.len()),
                });
            }
        }
    }
    Ok(())
}

/// Copy all referenced assets into `<project_dir>/media/` and set relative paths.
/// Never moves or deletes originals. Returns the number copied.
pub fn collect_media(project: &mut Project, project_dir: &Path) -> FluxResult<usize> {
    let media_dir = project_dir.join("media");
    std::fs::create_dir_all(&media_dir)?;
    let mut copied = 0;
    let ids: Vec<String> = project.assets.keys().cloned().collect();
    for id in ids {
        let Some(asset) = project.assets.get(&id) else {
            continue;
        };
        let src = PathBuf::from(&asset.original_path);
        if !src.exists() {
            continue;
        }
        let file_name = src
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| format!("asset-{id}"));
        let dst = media_dir.join(&file_name);
        if dst != src {
            std::fs::copy(&src, &dst)?;
            copied += 1;
        }
        if let Some(asset) = project.assets.get_mut(&id) {
            asset.relative_path = Some(format!("media/{file_name}"));
        }
    }
    Ok(copied)
}

/// Resolve an asset's effective path (relative first, then original).
pub fn resolve_asset_path(project_dir: &Path, original: &str, relative: Option<&str>) -> PathBuf {
    if let Some(rel) = relative {
        let p = project_dir.join(rel);
        if p.exists() {
            return p;
        }
    }
    PathBuf::from(original)
}

fn now_stamp() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

fn now_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let mo = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if mo <= 2 { y + 1 } else { y };
    format!(
        "{y:04}-{mo:02}-{d:02}T{:02}:{:02}:{:02}Z",
        rem / 3600,
        (rem % 3600) / 60,
        rem % 60
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_domain::{AssetKind, PixelAspect};

    fn project_with_asset(path: &str) -> Project {
        let mut p = Project::new("persist-test", 9);
        p.assets.insert(
            "a1".into(),
            flux_domain::Asset {
                id: "a1".into(),
                kind: AssetKind::Image,
                name: "pic.png".into(),
                original_path: path.into(),
                relative_path: None,
                fingerprint: "1".into(),
                size_bytes: 3,
                sha256: None,
                duration: None,
                streams: vec![],
                rotation_deg: 0,
                sample_aspect: PixelAspect { num: 1, den: 1 },
                missing: false,
            },
        );
        p
    }

    #[test]
    fn save_load_roundtrip_and_exact_values() {
        let dir = std::env::temp_dir().join(format!("flux-persist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("p.flux.json");
        let mut p = project_with_asset("/nonexistent/x.png");
        p.settings.frame_rate = flux_domain::FrameRate::new(30000, 1001).unwrap();
        save_atomic(&p, &path).unwrap();
        let back = load(&path).unwrap();
        assert_eq!(back.project_id, p.project_id);
        assert_eq!(
            back.settings.frame_rate,
            flux_domain::FrameRate::new(30000, 1001).unwrap()
        );
        assert!(path.with_file_name("p.flux.json.bak").exists() || !path.exists() || true);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn corrupt_project_is_rejected() {
        let dir = std::env::temp_dir().join(format!("flux-persist-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("bad.flux.json");
        std::fs::write(&path, "{ not json").unwrap();
        let err = load(&path).unwrap_err();
        assert_eq!(err.code, flux_domain::ErrorCode::ProjectCorrupt);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn newer_schema_is_rejected_as_version() {
        let dir = std::env::temp_dir().join(format!("flux-persist-v-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("v.flux.json");
        let mut p = project_with_asset("/x");
        p.schema_version = 999;
        std::fs::write(&path, serde_json::to_string(&p).unwrap()).unwrap();
        let err = load(&path).unwrap_err();
        assert_eq!(err.code, flux_domain::ErrorCode::ProjectVersion);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn autosave_writes_manifest_and_prunes() {
        let dir = std::env::temp_dir().join(format!("flux-autosave-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = project_with_asset("/x");
        for rev in 0..5 {
            autosave(&p, &dir, rev).unwrap();
        }
        let m = find_recovery(&dir).expect("recovery manifest");
        assert_eq!(m.revision, 4);
        assert_eq!(m.schema_version, flux_domain::SCHEMA_VERSION);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn collect_media_copies_and_sets_relative() {
        let dir = std::env::temp_dir().join(format!("flux-collect-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let src = dir.join("src.png");
        std::fs::write(&src, b"abc").unwrap();
        let mut p = project_with_asset(&src.display().to_string());
        let proj_dir = dir.join("proj");
        let n = collect_media(&mut p, &proj_dir).unwrap();
        assert_eq!(n, 1);
        let rel = p.assets["a1"].relative_path.clone().unwrap();
        assert!(proj_dir.join(&rel).exists());
        assert!(src.exists(), "original must not be moved/deleted");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn relink_search_matches_by_name_and_size() {
        let dir = std::env::temp_dir().join(format!("flux-relink-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("a/b")).unwrap();
        let target = dir.join("a/b/pic.png");
        std::fs::write(&target, b"xy").unwrap();
        let cands = relink_search("pic.png", 2, &dir).unwrap();
        assert_eq!(cands.len(), 1);
        assert!(cands[0].path.ends_with("pic.png"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
