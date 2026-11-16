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
    assert!(
        (f.rms - expected_rms).abs() / expected_rms < 0.05,
        "rms={} expected={}",
        f.rms,
        expected_rms
    );
    // centroid near 1 kHz (within a few bins)
    assert!(
        (f.centroid_hz - 1000.0).abs() < 60.0,
        "centroid={}",
        f.centroid_hz
    );
    // peak band is mid (500-2500)
    let mid = f.bands[2];
    assert!(mid > f.bands[0], "mid {} bass {}", mid, f.bands[0]);
    assert!(mid > f.bands[4], "mid {} treble {}", mid, f.bands[4]);
}

#[test]
fn bass_tone_lands_in_bass_band() {
    let s = sine(80.0, 0.5, 4096, 48_000);
    let f = analyze(&s);
    assert!(
        f.bands[0] > f.bands[2],
        "bass {} mid {}",
        f.bands[0],
        f.bands[2]
    );
}

#[test]
fn treble_tone_lands_in_treble_band() {
    let s = sine(8000.0, 0.5, 4096, 48_000);
    let f = analyze(&s);
    assert!(
        f.bands[4] > f.bands[0],
        "treble {} bass {}",
        f.bands[4],
        f.bands[0]
    );
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
    let cfg = AnalysisConfig {
        onset_sensitivity: 1.2,
        refractory_ms: 10.0,
        ..Default::default()
    };
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

// ---------------------------------------------------------------------------
// analyse_mono / point_at_sample: the hop-alignment contract media export relies on.
// ---------------------------------------------------------------------------

#[test]
fn analyse_mono_emits_exactly_one_point_per_complete_hop() {
    let cfg = AnalysisConfig {
        sample_rate: 44_100,
        ..Default::default()
    };
    let hop = cfg.hop;
    // Three complete hops plus a trailing partial hop that must be discarded.
    let s: Vec<f32> = sine(300.0, 0.5, hop * 3 + hop / 2, cfg.sample_rate);
    let points = analyse_mono(cfg, &s);

    assert_eq!(points.len(), 3, "trailing partial hop must not be emitted");
    for (i, p) in points.iter().enumerate() {
        assert_eq!(p.end_sample, (i as u64 + 1) * hop as u64);
        assert_eq!(
            p.features.time_secs,
            (i as u64 + 1) as f64 * hop as f64 / cfg.sample_rate as f64,
            "hop {i} timestamp must advance by exactly one hop at the stream's rate"
        );
    }
    assert_eq!(analyse_mono(cfg, &[]), Vec::new());
    let zero_hop = AnalysisConfig { hop: 0, ..cfg };
    assert_eq!(analyse_mono(zero_hop, &s), Vec::new(), "hop 0 cannot align");
}

/// The falsifier for "the config sample rate comes from the actual stream".
///
/// Fed the same buffer, a 44.1 kHz config and a 48 kHz config must disagree about
/// *when* each hop happened. If any caller hands `AnalysisConfig::default()` —
/// whose `sample_rate` is the hard-coded 48 kHz — to a 44.1 kHz stream, every
/// timestamp and every tempo estimate is 44100/48000 out, and this test fails.
#[test]
fn analyse_mono_timestamps_are_a_function_of_the_config_sample_rate() {
    // Two seconds of 44.1 kHz audio, an exact whole number of hops, so the
    // 48 kHz misreading is worth ~14 hops of timestamp error and cannot be
    // mistaken for ordinary hop-rounding slack.
    let n = 172 * 512;
    assert_eq!(n % 512, 0);
    let s: Vec<f32> = sine(300.0, 0.5, n, 44_100);
    let at_44100 = analyse_mono(
        AnalysisConfig {
            sample_rate: 44_100,
            ..Default::default()
        },
        &s,
    );
    let at_48000 = analyse_mono(
        AnalysisConfig {
            sample_rate: 48_000,
            ..Default::default()
        },
        &s,
    );
    assert_eq!(at_44100.len(), at_48000.len());
    assert_eq!(at_44100.len(), 172);
    let last = at_44100.len() - 1;
    let end = at_44100[last].end_sample as f64;
    let truth = end / 44_100.0;
    let wrong = end / 48_000.0;
    assert!(
        (at_44100[last].features.time_secs - truth).abs() < 1e-12,
        "44.1k config reported {}",
        at_44100[last].features.time_secs
    );
    assert!(
        (at_48000[last].features.time_secs - wrong).abs() < 1e-12,
        "48k config reported {}",
        at_48000[last].features.time_secs
    );
    // The two differ by 8.1 % — far more than one hop — so a caller that mixes
    // them up cannot pass this by accident.
    let one_hop = 512.0 / 44_100.0;
    assert!(
        (truth - wrong).abs() > 8.0 * one_hop,
        "the two rates are only {} s apart here, which hop rounding could also explain",
        (truth - wrong).abs()
    );
    // And the 48k reading is not merely "different", it is *late* in stream time.
    assert!(at_48000[last].features.time_secs < truth);
}

#[test]
fn analyse_mono_is_repeatable() {
    let cfg = AnalysisConfig {
        sample_rate: 48_000,
        ..Default::default()
    };
    let s: Vec<f32> = sine(440.0, 0.4, cfg.hop * 8, cfg.sample_rate);
    assert_eq!(
        analyse_mono(cfg, &s),
        analyse_mono(cfg, &s),
        "same samples must give the same feature timeline"
    );
}

#[test]
fn point_at_sample_is_latest_wins_on_the_hop_boundary() {
    let cfg = AnalysisConfig::default();
    let hop = cfg.hop;
    let s: Vec<f32> = sine(300.0, 0.5, hop * 4, cfg.sample_rate);
    let points = analyse_mono(cfg, &s);
    assert_eq!(points.len(), 4);

    assert!(
        point_at_sample(&points, hop as u64 - 1).is_none(),
        "before the first hop closes there is no evidence to report"
    );
    assert!(
        point_at_sample(&points, 0).is_none(),
        "sample 0 is before the first hop closed; there is no evidence yet"
    );
    assert_eq!(
        point_at_sample(&points, hop as u64).unwrap().end_sample,
        hop as u64
    );
    assert_eq!(
        point_at_sample(&points, hop as u64 + 1).unwrap().end_sample,
        hop as u64
    );
    assert_eq!(
        point_at_sample(&points, 2 * hop as u64).unwrap().end_sample,
        2 * hop as u64
    );
    // Past the end the last point holds, which is what a clamped preview needs.
    assert_eq!(
        point_at_sample(&points, 10_000_000).unwrap().end_sample,
        4 * hop as u64
    );
}

/// Characterisation of the trap `analyse_mono` exists to close: the raw
/// `Analyzer` lets a caller ask for features twice for one hop, and the two
/// answers are not the same.
#[test]
fn features_called_twice_per_hop_disagrees_with_itself() {
    let cfg = AnalysisConfig::default();
    let mut a = Analyzer::new(cfg);
    a.push(&sine(300.0, 0.5, cfg.hop, cfg.sample_rate));
    let first = a.features();
    let second = a.features();
    assert_eq!(
        first.time_secs, second.time_secs,
        "same hop, so the reported time is identical"
    );
    assert_ne!(
        first.flux, second.flux,
        "spectral flux is consumed by the first call, so a second call per hop \
         reports a different feature set for unchanged audio"
    );
    // analyse_mono cannot do this: one push, one features call, one point.
    let points = analyse_mono(cfg, &sine(300.0, 0.5, cfg.hop, cfg.sample_rate));
    assert_eq!(points.len(), 1);
}
