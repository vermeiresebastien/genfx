//! Export every format through the real API and verify the container decodes.
//! Regression for "export just makes PNGs instead of a video in my format".

use flux_app::export::{export_project, ExportOptions};
use flux_domain::{Project, RationalTime, TimeRange};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

fn project() -> Project {
    let mut p = Project::new("fmt", 7);
    p.settings.size = flux_domain::Size {
        width: 160,
        height: 90,
    };
    let g = flux_app::samples::chain_graph("gen.plasma", &["fx.bloom"]);
    p.timeline.master_graph_id = Some(g.id.clone());
    p.graphs.insert(g.id.clone(), g);
    p.timeline.range = TimeRange::new(RationalTime::ZERO, RationalTime::from_secs(1));
    p
}

fn probe(path: &std::path::Path) -> (String, f64) {
    let out = std::process::Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=codec_name",
            "-show_entries",
            "format=duration",
            "-of",
            "default=noprint_wrappers=1:nokey=1",
        ])
        .arg(path)
        .output()
        .expect("ffprobe");
    let s = String::from_utf8_lossy(&out.stdout);
    let mut lines = s.lines();
    let codec = lines.next().unwrap_or("").trim().to_string();
    let dur = lines
        .next()
        .and_then(|d| d.trim().parse::<f64>().ok())
        .unwrap_or(0.0);
    (codec, dur)
}

fn run(codec: flux_media::EncodeCodec, ext: &str, want_codec: &str) {
    let dir = std::env::temp_dir().join(format!("flux-fmt-{}-{}", ext, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let res = export_project(
        &project(),
        &ExportOptions {
            out_dir: dir.clone(),
            project_name: "fmt".into(),
            codec,
            width: 160,
            height: 90,
            range: None,
            cancel: Arc::new(AtomicBool::new(false)),
        },
        None,
    )
    .expect("export");

    let video = res
        .video
        .as_ref()
        .unwrap_or_else(|| panic!("{ext}: no video produced (frames only)"));
    assert!(video.exists(), "{ext}: missing {}", video.display());
    assert_eq!(
        video.extension().and_then(|e| e.to_str()),
        Some(ext),
        "{ext}: wrong extension {}",
        video.display()
    );

    // The PNG sequence must not be left behind for video codecs.
    assert!(
        !res.frames_dir.exists(),
        "{ext}: PNG frame directory left behind at {}",
        res.frames_dir.display()
    );

    let (codec_name, dur) = probe(video);
    assert_eq!(codec_name, want_codec, "{ext}: wrong codec");
    assert!(
        (dur - 1.0).abs() < 0.35,
        "{ext}: duration {dur} not ~1s"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn export_produces_real_video_per_format() {
    if flux_media::find_ffmpeg().is_none() {
        eprintln!("no ffmpeg; skipping");
        return;
    }
    // GPU required for rendering.
    if flux_render::Gpu::new(flux_render::BackendPref::Auto).is_err() {
        eprintln!("no GPU; skipping");
        return;
    }
    run(flux_media::EncodeCodec::H264, "mp4", "h264");
    run(flux_media::EncodeCodec::Hevc, "mp4", "hevc");
}

#[test]
fn png_sequence_format_writes_frames_and_no_video() {
    if flux_media::find_ffmpeg().is_none() {
        eprintln!("no ffmpeg; skipping");
        return;
    }
    if flux_render::Gpu::new(flux_render::BackendPref::Auto).is_err() {
        eprintln!("no GPU; skipping");
        return;
    }
    let dir = std::env::temp_dir().join(format!("flux-fmt-png-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();

    let res = export_project(
        &project(),
        &ExportOptions {
            out_dir: dir.clone(),
            project_name: "pngseq".into(),
            codec: flux_media::EncodeCodec::None,
            width: 160,
            height: 90,
            range: None,
            cancel: Arc::new(AtomicBool::new(false)),
        },
        None,
    )
    .expect("export");

    assert!(res.video.is_none(), "png sequence should not produce a video");
    assert!(res.frames_dir.exists(), "png sequence must write frames");
    let count = std::fs::read_dir(&res.frames_dir).unwrap().count();
    assert!(count >= 50, "expected ~60 frames, got {count}");
    let _ = std::fs::remove_dir_all(&dir);
}
