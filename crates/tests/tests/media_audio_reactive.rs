//! Media audio must reach the shader.
//!
//! This file deliberately uses only the API surface that existed *before* the
//! Step 1 wiring (`PreviewRenderer::render`, `export_project`, the domain
//! document types). It therefore compiles and runs unchanged on the pre-change
//! tree, where it is red: every rendered frame was handed `AudioSnapshot::
//! default()` and `FeatureSnapshot::default()`, so a project with a loud decoded
//! clip and a project with that clip's track muted produced byte-identical
//! pixels. After the wiring the two differ, which is the whole product claim.
//!
//! The two catalog entries that can show this are `gen.oscilloscope` and
//! `gen.glowing_wave` (its `audioWave` curve). `gen.oscilloscope` is used here
//! because its scaling is unconditional.

use flux_app::export::PreviewRenderer;
use flux_domain::{
    AudioControls, Clip, ClipSource, LoopPolicy, Project, RationalTime, TimeRange, Track, TrackKind,
};
use flux_media::{find_ffmpeg, MediaWorker};
use std::path::{Path, PathBuf};
use std::process::Command;

fn fixtures_dir() -> PathBuf {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/generated");
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Full-scale tone of `seconds`, written at `sample_rate`. Returns false when
/// FFmpeg is not available at all, which the caller turns into an explicit skip.
fn make_tone(path: &Path, sample_rate: u32, seconds: i64) -> bool {
    if path.exists() {
        return true;
    }
    let Some(ff) = find_ffmpeg() else {
        return false;
    };
    let src = format!("sine=frequency=300:sample_rate={sample_rate}:duration={seconds}");
    let st = Command::new(ff)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            &src,
            "-c:a",
            "pcm_s16le",
            path.to_str().unwrap(),
        ])
        .status()
        .expect("ffmpeg tone");
    st.success() && path.exists()
}

/// `demo_generator_project` shape, plus an audio track holding one tone clip.
fn project_with_tone(wav: &Path, sample_rate: u32, seconds: i64, muted: bool) -> Project {
    let worker = MediaWorker::new().expect("media worker");
    let probe = worker.probe(wav).expect("probe tone");
    let asset = worker.asset_from_probe(wav, &probe).expect("asset");

    let mut p = flux_app::samples::demo_generator_project("gen.oscilloscope", &[], seconds);
    // The clip references the asset by id, and `mix_audio` looks the id up in
    // the document — an unregistered asset decodes nothing and is silently
    // treated as "no audio".
    let asset_id = asset.id.clone();
    p.assets.insert(asset_id.clone(), asset);
    p.settings.audio.sample_rate = sample_rate;
    let dur = RationalTime::from_secs(seconds);

    let clip = Clip {
        id: "clip-tone".into(),
        name: "tone".into(),
        source: ClipSource::Asset { asset_id },
        range: TimeRange::new(RationalTime::ZERO, dur),
        source_range: TimeRange::new(RationalTime::ZERO, dur),
        rate: RationalTime::new(1, 1).unwrap(),
        loop_policy: LoopPolicy::Loop,
        graph_id: None,
        transform: flux_domain::Transform2D::default(),
        opacity: 1.0,
        audio: AudioControls::default(),
        linked_group: None,
    };
    let track = Track {
        id: "track-audio".into(),
        name: "A1".into(),
        kind: TrackKind::Audio,
        clip_order: vec![clip.id.clone()],
        locked: false,
        muted,
        solo: false,
        visible: true,
        gain_db: 0.0,
        opacity: 1.0,
        blend_mode: flux_domain::BlendMode::Normal,
        graph_id: None,
    };
    let track_id = track.id.clone();
    p.timeline.clips.insert(clip.id.clone(), clip);
    p.timeline.tracks.insert(track_id.clone(), track);
    p.timeline.track_order.push(track_id);
    p
}

/// Render one frame of `project` at `secs`.
fn frame(pr: &mut PreviewRenderer, project: &Project, secs: i64) -> Vec<u8> {
    let t = RationalTime::new(secs, 1).unwrap();
    pr.render(project, t, 192, 108).expect("preview render")
}

/// How much ink the frame holds: count of non-black, non-transparent pixels.
/// Used so a failure can say *what* changed, not just that bytes differed.
fn lit_pixels(rgba: &[u8]) -> usize {
    rgba.chunks_exact(4)
        .filter(|p| p[0] as u32 + p[1] as u32 + p[2] as u32 > 24 || p[3] as u32 > 24)
        .count()
}

/// Compact, comparable description of a frame: how much ink it holds plus an
/// FNV-1a digest of every byte. Comparing this instead of the raw buffer keeps a
/// failure message readable (a 82 944-byte pixel vec in a test log is not).
fn frame_signature(rgba: &[u8]) -> (usize, u64) {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in rgba {
        h ^= b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    (lit_pixels(rgba), h)
}

/// Count of pixels that differ between two frames of the same size.
fn changed_pixels(a: &[u8], b: &[u8]) -> usize {
    a.chunks_exact(4)
        .zip(b.chunks_exact(4))
        .filter(|(x, y)| x != y)
        .count()
}

#[test]
fn loud_media_audio_moves_the_picture_muted_does_not() {
    let fx = fixtures_dir();
    let wav = fx.join("media_audio_tone_48k.wav");
    if !make_tone(&wav, 48_000, 2) {
        eprintln!("SKIP loud_media_audio_moves_the_picture_muted_does_not: no FFmpeg");
        return;
    }
    // Preconditions must hold or the test proves nothing.
    assert!(wav.exists(), "tone fixture missing");
    assert!(
        std::fs::metadata(&wav).unwrap().len() > 100_000,
        "tone near-empty: {} bytes",
        std::fs::metadata(&wav).unwrap().len()
    );

    let loud = project_with_tone(&wav, 48_000, 2, false);
    let muted = project_with_tone(&wav, 48_000, 2, true);

    let mut pr = PreviewRenderer::new().expect("gpu");
    let a = frame(&mut pr, &loud, 1);
    let b = frame(&mut pr, &muted, 1);

    let (lit_a, sig_a) = frame_signature(&a);
    let (lit_b, sig_b) = frame_signature(&b);
    assert!(
        lit_a > 200,
        "loud frame is degenerate: lit_pixels {lit_a} of {}",
        a.len() / 4
    );
    assert_ne!(
        sig_a, sig_b,
        "a decoded loud clip and that same clip with its track muted rendered \
         identically (lit {} vs {lit_b}): the analyser output never reached the \
         audio uniform",
        lit_a
    );
    // The difference must be visible, not one dithered pixel.
    let diff = changed_pixels(&a, &b);
    assert!(
        diff > 100,
        "audio moved {diff} pixels - below the threshold that counts as seen"
    );
}

#[test]
fn audio_features_follow_the_projects_own_sample_rate() {
    let fx = fixtures_dir();
    let wav = fx.join("media_audio_tone_44k.wav");
    if !make_tone(&wav, 44_100, 2) {
        eprintln!("SKIP audio_features_follow_the_projects_own_sample_rate: no FFmpeg");
        return;
    }
    assert!(wav.exists(), "44.1k tone fixture missing");

    let loud = project_with_tone(&wav, 44_100, 2, false);
    let muted = project_with_tone(&wav, 44_100, 2, true);
    assert_eq!(loud.settings.audio.sample_rate, 44_100);

    let mut pr = PreviewRenderer::new().expect("gpu");
    let a = frame(&mut pr, &loud, 1);
    let b = frame(&mut pr, &muted, 1);
    let (_, sig_a) = frame_signature(&a);
    let (_, sig_b) = frame_signature(&b);
    assert_ne!(
        sig_a, sig_b,
        "at 44.1 kHz the analyser must be configured from the stream, not from \
         AnalysisConfig::default()'s hard-coded 48 kHz"
    );
    assert!(
        changed_pixels(&a, &b) > 100,
        "44.1 kHz audio moved fewer than 100 pixels"
    );
}
