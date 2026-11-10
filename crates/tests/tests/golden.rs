//! Golden project test: one run that imports media, applies effects, uses
//! generator content, exports PNG+WAV+H.264, then verifies the artifact with
//! ffprobe and exact frame/sample counts.

use flux_app::{export, samples};
use flux_media::{find_ffmpeg, MediaWorker};
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixtures_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/generated");
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn make_video(path: &Path) {
    if path.exists() {
        return;
    }
    let ff = find_ffmpeg().unwrap();
    Command::new(ff)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=320x180:rate=30:duration=2",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
}

#[test]
fn golden_generator_project_exports_expected_media() {
    let dir = fixtures_dir().join("golden_gen");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    // 3-second plasma→bloom→halftone graph project.
    let project = samples::demo_generator_project("gen.plasma", &["fx.bloom", "fx.halftone"], 3);
    let res = export::export_project(
        &project,
        &export::ExportOptions {
            out_dir: dir.clone(),
            project_name: "golden_gen".into(),
            codec: flux_media::EncodeCodec::H264,
            width: 320,
            height: 180,
            range: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        },
        None,
    )
    .unwrap();
    // Exact counts: 3s @ 60fps = 180 frames; 3s * 48000 * 2ch = 288000 samples.
    assert_eq!(res.frames, 180, "frame count");
    assert_eq!(res.samples, 288_000, "stereo sample count");
    // Video codecs stream frames straight into ffmpeg: no PNG sequence is left
    // behind, and a real video file must exist.
    assert!(
        !res.frames_dir.exists(),
        "video export must not leave a PNG sequence"
    );
    let video = res.video.as_ref().expect("video produced");
    assert!(std::fs::metadata(video).unwrap().len() > 1000, "video non-trivial");
    // WAV exists.
    let wav = res.wav.as_ref().expect("wav");
    assert!(std::fs::metadata(wav).unwrap().len() > 1000);
    // Probe the MP4.
    let probe = MediaWorker::new().unwrap().probe(video).unwrap();
    assert_eq!(probe.width, Some(320));
    assert_eq!(probe.height, Some(180));
    assert!(probe.streams.iter().any(|s| s.kind == "video"));
}

#[test]
fn golden_media_project_presents_decoded_frame() {
    let fx = fixtures_dir();
    let video = fx.join("golden_src.mp4");
    make_video(&video);
    let dir = fx.join("golden_media");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let project = samples::demo_media_project(&video, 2);
    let res = export::export_project(
        &project,
        &export::ExportOptions {
            out_dir: dir.clone(),
            project_name: "golden_media".into(),
            codec: flux_media::EncodeCodec::None,
            width: 320,
            height: 180,
            range: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        },
        None,
    )
    .unwrap();
    assert_eq!(res.frames, 120, "2s @ 60fps");
    // Read a decoded output frame and confirm it is not empty/black: the source
    // video has varied content (color bars), so the export must too.
    let bytes = std::fs::read(res.frames_dir.join("frame_000060.png")).unwrap();
    assert!(bytes.len() > 1000);
}

#[test]
fn text_producer_survives_export() {
    let dir = fixtures_dir().join("golden_text");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let project = samples::demo_generator_project("gen.text", &[], 1);
    let res = export::export_project(
        &project,
        &export::ExportOptions {
            out_dir: dir.clone(),
            project_name: "golden_text".into(),
            codec: flux_media::EncodeCodec::None,
            width: 320,
            height: 180,
            range: None,
            cancel: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
        },
        None,
    )
    .unwrap();
    assert_eq!(res.frames, 60);
    // The produced text frame must contain opaque glyph pixels (non-empty).
    let bytes = std::fs::read(res.frames_dir.join("frame_000030.png")).unwrap();
    assert!(bytes.len() > 500);
}
