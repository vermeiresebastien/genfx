//! Audio analysis numeric tests with exact signals.

use super::*;

fn sine(freq: f32, amp: f32, n: usize, sr: u32) -> Vec<f32> {
    (0..n)
        .map(|i| (std::f32::consts::TAU * freq * i as f32 / sr as f32).sin() * amp)
        .collect()
}

fn analyze(samples: &[f32]) -> WindowFeatures {
    let mut a = Analyzer::new(AnalysisConfig::default());
    a.push(samples);
    a.features()
}

#[test]
fn silence_yields_zeros_and_no_nans() {
    let mut a = Analyzer::new(AnalysisConfig::default());
    a.push(&vec![0.0f32; 4096]);
    let f = a.features();
    assert_eq!(f.rms, 0.0);
    assert_eq!(f.peak, 0.0);
    assert_eq!(f.bands, [0.0; 5]);
    assert!(!f.onset);
    assert!(f.centroid_hz.is_finite());
    assert!(f.chroma.iter().all(|c| c.is_finite() && *c == 0.0));
}

#[test]
fn sine_frequency_and_rms() {
    // -12 dBFS = amplitude 0.251; rms = 0.251/sqrt2 = 0.1776
    let amp = 10f32.powf(-12.0 / 20.0);
    let s = sine(1000.0, amp, 4096, 48_000);
    let f = analyze(&s);
    let expected_rms = amp / std::f32::consts::SQRT_2;
    assert!((f.rms - expected_rms).abs() / expected_rms < 0.05, "rms={} expected={}", f.rms, expected_rms);
    // centroid near 1 kHz (within a few bins)
    assert!((f.centroid_hz - 1000.0).abs() < 60.0, "centroid={}", f.centroid_hz);
    // peak band is mid (500-2500)
    let mid = f.bands[2];
    assert!(mid > f.bands[0], "mid {} bass {}", mid, f.bands[0]);
    assert!(mid > f.bands[4], "mid {} treble {}", mid, f.bands[4]);
}

#[test]
fn bass_tone_lands_in_bass_band() {
    let s = sine(80.0, 0.5, 4096, 48_000);
    let f = analyze(&s);
    assert!(f.bands[0] > f.bands[2], "bass {} mid {}", f.bands[0], f.bands[2]);
}

#[test]
fn treble_tone_lands_in_treble_band() {
    let s = sine(8000.0, 0.5, 4096, 48_000);
    let f = analyze(&s);
    assert!(f.bands[4] > f.bands[0], "treble {} bass {}", f.bands[4], f.bands[0]);
}

#[test]
fn band_edges_cap_at_nyquist() {
    // default edges up to 16k < 24k nyquist; make sure no panic/NaN
    let s = sine(20_000.0, 0.4, 4096, 48_000);
    let f = analyze(&s);
    assert!(f.bands.iter().all(|b| b.is_finite()));
}

#[test]
fn onset_detected_on_impulse() {
    let cfg = AnalysisConfig { onset_sensitivity: 1.2, refractory_ms: 10.0, ..Default::default() };
    let mut a = Analyzer::new(cfg);
    // Speak a steady tone first, then a broadband impulse window.
    let steady = sine(440.0, 0.2, 2048, 48_000);
    a.push(&steady);
    let _f0 = a.features();
    a.push(&steady);
    let _f1 = a.features();
    // Impulse window: full-band energy in the first half, quiet second half.
    let mut imp = vec![0.0f32; 2048];
    for i in 0..24 {
        imp[i] = if i % 2 == 0 { 0.95 } else { -0.95 };
    }
    let rising = sine(3000.0, 0.4, 2048, 48_000);
    for (i, v) in rising.iter().enumerate() {
        imp[i] += v;
    }
    a.push(&imp);
    let f = a.features();
    assert!(
        f.onset || f.onset_strength > 1.0,
        "expected onset evidence, strength={} flux={}",
        f.onset_strength,
        f.flux
    );
}

#[test]
fn no_onset_in_steady_sine() {
    let mut a = Analyzer::new(AnalysisConfig::default());
    let s = sine(440.0, 0.3, 8192, 48_000);
    a.push(&s);
    let f = a.features();
    // steady sine -> flat flux history -> strength below threshold
    assert!(!f.onset, "unexpected onset strength {}", f.onset_strength);
}

#[test]
fn ring_preserves_order_and_reports_overflow() {
    let ring = PcmRing::new(8);
    let input: Vec<f32> = (0..6).map(|i| i as f32).collect();
    ring.write_mono(&input);
    let mut out = Vec::new();
    let n = ring.read_into(&mut out);
    assert_eq!(n, 6);
    assert_eq!(out, input);
    // overflow 20 into 8-cap ring
    let big: Vec<f32> = (0..20).map(|i| i as f32).collect();
    ring.write_mono(&big);
    let mut out2 = Vec::new();
    let n2 = ring.read_into(&mut out2);
    assert_eq!(n2, 8);
    assert_eq!(out2, big[12..].to_vec());
    assert!(ring.dropped() >= 1);
}

#[test]
fn pan_center_is_unity_both_channels() {
    let (l, r) = pan_gains(0.0);
    assert!((l - 1.0).abs() < 1e-9);
    assert!((r - 1.0).abs() < 1e-9);
}

#[test]
fn pan_full_left_silences_right() {
    let (l, r) = pan_gains(-1.0);
    assert!((l - std::f64::consts::SQRT_2).abs() < 1e-9);
    assert!(r < 1e-9);
}

#[test]
fn mixer_gain_and_pan() {
    let m = Mixer::new(48_000);
    let src = vec![0.5f32; 200];
    let mut dst = vec![0.0f32; 200];
    m.mix_stereo(&mut dst, &src, 2, -6.0, 0.0);
    let g = 10f64.powf(-6.0 / 20.0) as f32;
    assert!((dst[0] - 0.5 * g).abs() < 1e-5);
    assert!((dst[1] - 0.5 * g).abs() < 1e-5);
}

#[test]
fn equal_power_fade_ends() {
    let m = Mixer::new(48_000);
    let mut dst = vec![1.0f32; 200];
    m.apply_fade(&mut dst, 50, 50);
    assert!(dst[0] < 0.1);
    // middle unaffected
    assert!((dst[100] - 1.0).abs() < 1e-6);
    assert!(dst[199] < 0.1);
}

#[test]
fn tempo_estimate_in_plausible_range() {
    // Alternating flux pulses at 0.5 s/hop give a strong correlation peak at a
    // 1-hop lag (120 BPM) and every multiple. Any plausible octave estimate is
    // acceptable; tempo is explicitly an estimate, never transport truth.
    let hop_secs = 0.5;
    let mut flux = vec![0.05f32; 64];
    for (i, f) in flux.iter_mut().enumerate() {
        if i % 2 == 0 {
            *f = 1.0;
        }
    }
    let (bpm, conf) = estimate_tempo(&flux, hop_secs);
    // report the estimate; assert it is a plausible integer-ish BPM value and
    // that confidence is nonzero, not a specific octave.
    assert!(bpm >= 60.0 && bpm <= 200.0, "bpm={bpm}");
    assert!(conf > 0.0, "conf={conf}");
}

#[test]
fn chroma_picks_expected_pitch_class() {
    // A4 = 440 Hz -> pitch class 9
    let s = sine(440.0, 0.4, 4096, 48_000);
    let f = analyze(&s);
    let max_idx = f
        .chroma
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
        .unwrap()
        .0;
    assert_eq!(max_idx, 9, "chroma={:?}", f.chroma);
}
