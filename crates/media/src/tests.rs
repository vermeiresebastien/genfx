//! Media integration tests using real FFmpeg-generated fixtures.

use super::*;
use std::path::PathBuf;

fn fixture_dir() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/generated");
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn make_video(path: &Path, seconds: f64, fps: &str) {
    if path.exists() {
        return;
    }
    let ff = find_ffmpeg().expect("ffmpeg available");
    let status = Command::new(ff)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "lavfi",
            "-i",
            &format!("testsrc2=size=160x90:rate={fps}:duration={seconds}"),
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            path.to_str().unwrap(),
        ])
        .status()
        .unwrap();
    assert!(status.success());
}

fn make_audio(path: &Path, seconds: f64) {
    if path.exists() {
        return;
    }
    let ff = find_ffmpeg().unwrap();
    // Generate a full-scale 1 kHz sine deterministically.
    let n = (48_000.0 * seconds) as i64;
    let mut pcm = Vec::with_capacity(n as usize * 2);
    for i in 0..n {
        let t = i as f64 / 48_000.0;
        let v = (std::f64::consts::TAU * 1000.0 * t).sin() * 0.8;
        let s = (v * 32767.0) as i16;
        pcm.push(s.to_le_bytes()[0]);
        pcm.push(s.to_le_bytes()[1]);
        pcm.push(s.to_le_bytes()[0]);
        pcm.push(s.to_le_bytes()[1]);
    }
    let mut f = std::fs::File::create(path).unwrap();
    use std::io::Write;
    let data_len = pcm.len() as u32;
    let byte_rate = 48_000u32 * 2 * 2;
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVE").unwrap();
    f.write_all(b"fmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&48_000u32.to_le_bytes()).unwrap();
    f.write_all(&byte_rate.to_le_bytes()).unwrap();
    f.write_all(&4u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    f.write_all(&pcm).unwrap();
    let _ = ff;
}

#[test]
fn probe_reports_streams_and_duration() {
    let dir = fixture_dir();
    let v = dir.join("test_10s_60fps.mp4");
    make_video(&v, 10.0, "60");
    let worker = MediaWorker::new().unwrap();
    let info = worker.probe(&v).unwrap();
    assert!(matches!(info.kind, AssetKind::Video));
    assert_eq!(info.width, Some(160));
    assert!(info.streams.iter().any(|s| s.kind == "video"));
    let d = info.duration.unwrap();
    assert!((d.as_f64() - 10.0).abs() < 0.2);
}

#[test]
fn decode_frame_is_correct_size_and_not_blank() {
    let dir = fixture_dir();
    let v = dir.join("test_10s_60fps.mp4");
    make_video(&v, 10.0, "60");
    let worker = MediaWorker::new().unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let frame = worker.decode_rgba_frame(&v, RationalTime::ZERO, 64, 36, cancel).unwrap();
    assert_eq!(frame.width, 64);
    assert_eq!(frame.height, 36);
    assert_eq!(frame.rgba.len(), 64 * 36 * 4);
    // testsrc2 has non-uniform content: at least two distinct pixel values
    let first = &frame.rgba[0..4];
    assert!(frame.rgba.chunks(4).any(|p| p != first));
}

#[test]
fn decode_audio_has_expected_samples_and_sine() {
    let dir = fixture_dir();
    let a = dir.join("sine_1s.wav");
    make_audio(&a, 1.0);
    let worker = MediaWorker::new().unwrap();
    let track = worker.decode_audio_stereo(&a, 48_000).unwrap();
    assert_eq!(track.sample_rate, 48_000);
    assert!((track.frames as i64 - 48_000).abs() <= 64, "frames={}", track.frames);
    // energy present
    let rms: f32 = (track.samples.iter().map(|s| s * s).sum::<f32>() / track.samples.len() as f32).sqrt();
    assert!(rms > 0.4, "rms={rms}");
}

#[test]
fn wav_roundtrip_preserves_samples() {
    let dir = fixture_dir();
    let p = dir.join("roundtrip.wav");
    let samples: Vec<f32> = (0..1000).map(|i| ((i as f32) * 0.01).sin() * 0.5).collect();
    write_wav_stereo(&p, 48_000, &samples).unwrap();
    let (rate, ch, back) = read_wav_f32(&p).unwrap();
    assert_eq!(rate, 48_000);
    assert_eq!(ch, 2);
    assert_eq!(back.len(), samples.len());
    for (a, b) in samples.iter().zip(back.iter()) {
        assert!((a - b).abs() < 1.0 / 32768.0 * 2.0);
    }
}

#[test]
fn png_roundtrip() {
    let dir = fixture_dir();
    let p = dir.join("frame_rt.png");
    let mut rgba = vec![0u8; 16 * 8 * 4];
    for (i, px) in rgba.chunks_exact_mut(4).enumerate() {
        px[0] = (i % 16) as u8 * 16;
        px[3] = 255;
    }
    write_png_rgba(&p, 16, 8, &rgba).unwrap();
    let bytes = std::fs::read(&p).unwrap();
    let (w, h, back) = decode_png_rgba(&bytes).unwrap();
    assert_eq!((w, h), (16, 8));
    assert_eq!(back.len(), rgba.len());
    assert_eq!(&back[..4], &rgba[..4]);
}

#[test]
fn capabilities_detect_encoders() {
    let worker = MediaWorker::new().unwrap();
    let c = worker.capabilities();
    assert!(c.h264, "libx264 expected in the installed FFmpeg");
    assert!(c.wav);
    assert!(c.png);
}
