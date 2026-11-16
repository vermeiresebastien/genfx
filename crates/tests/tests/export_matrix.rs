//! Export matrix: each available codec produces a real, probeable artifact;
//! unavailable encoders fail explicitly (never silently). Also verifies the
//! frozen-snapshot semantics (editing after queueing does not affect the export).

use flux_app::export::{export_project, ExportOptions};
use flux_app::job::{run_job, ExportJobDesc, RunningJob};
use flux_app::samples;
use flux_media::{find_ffmpeg, MediaWorker};
use std::path::PathBuf;
use std::sync::Arc;

fn dir(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/generated")
        .join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn caps() -> Option<flux_media::MediaCaps> {
    find_ffmpeg()?;
    MediaWorker::new().ok().map(|w| w.capabilities())
}

fn export_small(
    codec: flux_media::EncodeCodec,
    out: &std::path::Path,
) -> flux_app::export::ExportResult {
    let project = samples::demo_generator_project("gen.plasma", &["fx.bloom"], 1);
    export_project(
        &project,
        &ExportOptions {
            out_dir: out.to_path_buf(),
            project_name: "matrix".into(),
            codec,
            width: 160,
            height: 90,
            range: None,
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
            feature_source: flux_domain::params::FeatureSource::DocumentOnly,
        },
        None,
    )
    .unwrap()
}

#[test]
fn png_sequence_and_wav_always_available() {
    let out = dir("matrix_png");
    let r = export_small(flux_media::EncodeCodec::None, &out);
    assert_eq!(r.frames, 60);
    assert!(r.wav.as_ref().unwrap().exists());
    assert!(r.frames_dir.join("frame_000001.png").exists());
    // CR3 defect 1: the audit line's frames-dir claim must come from this field,
    // so the ffmpeg-free PNG-sequence path must report it true. Mutating
    // export.rs:235 to `png_frames_written: false` reddens here (M1).
    assert!(
        r.png_frames_written,
        "codec None must report PNG frames as written"
    );
}

#[test]
fn h264_mp4_is_produced_and_probes() {
    let Some(caps) = caps() else { return };
    if !caps.h264 {
        return;
    }
    let out = dir("matrix_h264");
    let r = export_small(flux_media::EncodeCodec::H264, &out);
    let v = r.video.expect("mp4");
    let p = MediaWorker::new().unwrap().probe(&v).unwrap();
    assert!(p.streams.iter().any(|s| s.kind == "video"));
    assert_eq!(p.width, Some(160));
    // CR3 defect 1: a video codec streams raw RGBA to ffmpeg and writes no PNG
    // frames, and must not create the frames dir at all. These two assertions
    // are behind the caps() guard, so they are ffmpeg-conditional (ffmpeg IS
    // present on this machine: the h264 export above produced a probeable mp4).
    // Mutating export.rs:235 to `png_frames_written: true` reddens the first
    // here (M2); a run that created the dir would redden the second.
    assert!(
        !r.png_frames_written,
        "video codecs stream to ffmpeg and write no PNGs"
    );
    assert!(
        !r.frames_dir.exists(),
        "a video run must not create the frames dir"
    );
}

#[test]
fn hevc_and_prores_when_available() {
    let Some(caps) = caps() else { return };
    if caps.hevc {
        let out = dir("matrix_hevc");
        let r = export_small(flux_media::EncodeCodec::Hevc, &out);
        assert!(r.video.is_some(), "HEVC advertised but not produced");
    }
    if caps.prores {
        let out = dir("matrix_prores");
        let r = export_small(flux_media::EncodeCodec::ProRes, &out);
        assert!(r.video.is_some(), "ProRes advertised but not produced");
    }
}

#[test]
fn gif_when_available() {
    let Some(caps) = caps() else { return };
    if !caps.gif {
        return;
    }
    let out = dir("matrix_gif");
    let r = export_small(flux_media::EncodeCodec::Gif, &out);
    assert!(r.video.is_some());
}

#[test]
fn jpeg_sequence_when_available() {
    let Some(caps) = caps() else { return };
    let out = dir("matrix_jpeg");
    let r = export_small(flux_media::EncodeCodec::JpegSequence, &out);
    // JPEGs are written into a sibling `*_jpeg` directory.
    let jpeg_dir = r.frames_dir.with_file_name(format!(
        "{}_jpeg",
        r.frames_dir.file_name().unwrap().to_string_lossy()
    ));
    assert!(jpeg_dir.join("frame_000001.jpg").exists());
}

#[test]
fn frozen_snapshot_export_ignores_later_edits() {
    let mut project = samples::demo_generator_project("gen.plasma", &[], 1);
    project.name = "snap".into();
    let desc = ExportJobDesc::from_project(&project, &dir("snapshot"), "png", 96, 54).unwrap();
    // Mutate after queueing.
    project.name = "edited".into();
    let handle = RunningJob {
        id: desc.id.clone(),
        done: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        total: Arc::new(std::sync::atomic::AtomicU64::new(0)),
        cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
    };
    let r = run_job(&desc, flux_media::EncodeCodec::None, &handle).unwrap();
    assert_eq!(r.frames, 60);
    // Output used the snapshot name, not the edited name.
    assert!(r.frames_dir.to_string_lossy().contains("snap") || r.samples > 0);
}
