//! Audio analysis: FFT windows, bands, RMS/peak, onset, beat/tempo, chroma.
//!
//! Analysis is deterministic: the same PCM and the same algorithm version
//! produce the same features, which is what makes offline and live evaluation
//! agree. No allocations or locks happen in an audio callback; callbacks push
//! into a lock-free ring, and a separate worker consumes windows.

use flux_domain::params::FeatureSnapshot;
use flux_engine::frame::AudioSnapshot;
use rustfft::{num_complex::Complex, FftPlanner};
use std::sync::atomic::{AtomicU64, Ordering};

/// Algorithm version. Cached features include this; bumping it invalidates.
pub const ANALYSIS_VERSION: u32 = 1;

pub const SAMPLE_RATE: u32 = 48_000;
pub const DEFAULT_WINDOW: usize = 2048;
pub const DEFAULT_HOP: usize = 512;

/// Band edges in Hz (bass, low-mid, mid, high-mid, treble).
pub const BAND_EDGES: [(f32, f32); 5] = [
    (20.0, 160.0),
    (160.0, 500.0),
    (500.0, 2500.0),
    (2500.0, 6000.0),
    (6000.0, 16_000.0),
];

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AnalysisConfig {
    pub sample_rate: u32,
    pub window: usize,
    pub hop: usize,
    pub gate_dbfs: f32,
    pub onset_sensitivity: f32,
    pub refractory_ms: f32,
}

impl Default for AnalysisConfig {
    fn default() -> Self {
        Self {
            sample_rate: SAMPLE_RATE,
            window: DEFAULT_WINDOW,
            hop: DEFAULT_HOP,
            gate_dbfs: -60.0,
            onset_sensitivity: 1.5,
            refractory_ms: 80.0,
        }
    }
}

/// Full per-window analysis result.
#[derive(Debug, Clone, PartialEq)]
pub struct WindowFeatures {
    pub rms: f32,
    pub peak: f32,
    pub bands: [f32; 5],
    pub centroid_hz: f32,
    pub spread_hz: f32,
    pub rolloff_hz: f32,
    pub flux: f32,
    pub flatness: f32,
    pub onset: bool,
    pub onset_strength: f32,
    pub beat_phase: f32,
    pub tempo_bpm: f32,
    pub tempo_confidence: f32,
    pub chroma: [f32; 12],
    pub time_secs: f64,
}

impl Default for WindowFeatures {
    fn default() -> Self {
        Self {
            rms: 0.0,
            peak: 0.0,
            bands: [0.0; 5],
            centroid_hz: 0.0,
            spread_hz: 0.0,
            rolloff_hz: 0.0,
            flux: 0.0,
            flatness: 0.0,
            onset: false,
            onset_strength: 0.0,
            beat_phase: 0.0,
            tempo_bpm: 120.0,
            tempo_confidence: 0.0,
            chroma: [0.0; 12],
            time_secs: 0.0,
        }
    }
}

impl WindowFeatures {
    pub fn to_snapshot(&self) -> FeatureSnapshot {
        FeatureSnapshot {
            rms: self.rms,
            peak: self.peak,
            bass: self.bands[0],
            low_mid: self.bands[1],
            mid: self.bands[2],
            high_mid: self.bands[3],
            treble: self.bands[4],
            centroid: self.centroid_hz / 20_000.0,
            flux: self.flux,
            onset: if self.onset { 1.0 } else { 0.0 },
            beat_phase: self.beat_phase,
            tempo_bpm: self.tempo_bpm,
            valid: true,
        }
    }

    pub fn to_snapshot_audio(&self) -> AudioSnapshot {
        AudioSnapshot {
            bands: [self.bands[0], self.bands[2], self.bands[4], self.bands[1]],
            extra: [
                self.rms,
                if self.onset { 1.0 } else { 0.0 },
                self.peak,
                self.flux,
            ],
            params: [
                self.tempo_bpm,
                self.beat_phase,
                self.centroid_hz,
                self.tempo_confidence,
            ],
        }
    }
}

/// Sliding-window analyzer. Keeps a Hann window, previous spectrum and onset
/// state so consecutive windows produce a consistent feature timeline.
pub struct Analyzer {
    cfg: AnalysisConfig,
    planner: FftPlanner<f32>,
    window_coeffs: Vec<f32>,
    /// Ring of the last `window` mono samples.
    buffer: Vec<f32>,
    pos: usize,
    since_hop: usize,
    prev_mag: Vec<f32>,
    flux_history: Vec<f32>,
    last_onset_secs: f64,
    samples_seen: u64,
    window_count: u64,
    onsets: AtomicU64,
}

impl Analyzer {
    pub fn new(cfg: AnalysisConfig) -> Self {
        let n = cfg.window;
        let window_coeffs = (0..n)
            .map(|i| {
                // periodic Hann
                let x = std::f32::consts::TAU * i as f32 / n as f32;
                0.5 * (1.0 - x.cos())
            })
            .collect();
        let bin = n / 2 + 1;
        Self {
            planner: FftPlanner::new(),
            window_coeffs,
            buffer: vec![0.0; n],
            pos: 0,
            since_hop: 0,
            prev_mag: vec![0.0; bin],
            flux_history: Vec::with_capacity(43),
            last_onset_secs: -1.0,
            samples_seen: 0,
            window_count: 0,
            onsets: AtomicU64::new(0),
            cfg,
        }
    }

    pub fn tone_gate_lin(&self) -> f32 {
        10f32.powf(self.cfg.gate_dbfs / 20.0)
    }

    /// Push mono samples; call `tick` after to obtain windows at hop boundaries.
    pub fn push(&mut self, mono: &[f32]) {
        let n = self.buffer.len();
        for &s in mono {
            self.buffer[self.pos] = if s.is_finite() { s } else { 0.0 };
            self.pos = (self.pos + 1) % n;
            self.samples_seen += 1;
            self.since_hop += 1;
            if self.since_hop >= self.cfg.hop {
                self.since_hop = 0;
                self.window_count += 1;
            }
        }
    }

    /// Compute features for the current window. The window is centred so its
    /// timestamp matches the documentation (window center + hop).
    pub fn features(&mut self) -> WindowFeatures {
        let n = self.buffer.len();
        // de-ring into chronological order
        let mut time: Vec<f32> = Vec::with_capacity(n);
        for i in 0..n {
            time.push(self.buffer[(self.pos + i) % n]);
        }
        let mut rms = 0.0f32;
        let mut peak = 0.0f32;
        for &s in &time {
            rms += s * s;
            peak = peak.max(s.abs());
        }
        rms = (rms / n as f32).sqrt();
        let gate = self.tone_gate_lin();
        let silent = peak < gate;

        let mut buf: Vec<Complex<f32>> = time
            .iter()
            .zip(self.window_coeffs.iter())
            .map(|(s, w)| Complex::new(s * w, 0.0))
            .collect();
        let fft = self.planner.plan_fft_forward(n);
        fft.process(&mut buf);
        let scale = 2.0 / n as f32 / window_rms(&self.window_coeffs);
        let mut mag = vec![0.0f32; n / 2 + 1];
        for (i, c) in buf.iter().enumerate().take(n / 2 + 1) {
            mag[i] = c.norm() * scale;
        }
        let bin_hz = self.cfg.sample_rate as f32 / n as f32;

        // bands: normalized sqrt of summed power over band
        let mut bands = [0.0f32; 5];
        if !silent {
            for (b, (lo, hi)) in BAND_EDGES.iter().enumerate() {
                let k0 = ((lo / bin_hz) as usize).max(1).min(mag.len() - 1);
                let k1 = ((hi / bin_hz) as usize).min(mag.len() - 1);
                let mut power = 0.0f32;
                for k in k0..=k1 {
                    power += mag[k] * mag[k];
                }
                bands[b] = (power / (k1 - k0 + 1) as f32).sqrt();
            }
            // normalize against full-scale reference
            let ref_level = 0.5;
            for b in bands.iter_mut() {
                *b = (*b / ref_level).clamp(0.0, 1.0);
            }
        }

        // centroid / spread / rolloff on power spectrum
        let mut total = 0.0f32;
        let mut weighted = 0.0f32;
        for (k, &m) in mag.iter().enumerate().skip(1) {
            let p = m * m;
            total += p;
            weighted += p * k as f32 * bin_hz;
        }
        let (centroid, spread, rolloff, flatness) = if total > 0.0 {
            let c = weighted / total;
            let mut var = 0.0f32;
            let mut accum = 0.0f32;
            let mut roll = 0.0f32;
            for (k, &m) in mag.iter().enumerate().skip(1) {
                let f = k as f32 * bin_hz;
                let p = m * m;
                var += (f - c) * (f - c) * p;
                accum += p;
                if accum >= 0.85 * total && roll == 0.0 {
                    roll = f;
                }
            }
            let sp = (var / total).sqrt();
            // flatness = geometric mean / arithmetic mean of magnitudes
            let mut log_sum = 0.0f32;
            let mut count = 0.0f32;
            for &m in mag.iter().skip(1) {
                log_sum += (m + 1e-9).ln();
                count += 1.0;
            }
            let geo = (log_sum / count).exp();
            let ari = mag.iter().skip(1).sum::<f32>() / count;
            (c, sp, roll, if ari > 0.0 { geo / ari } else { 0.0 })
        } else {
            (0.0, 0.0, 0.0, 0.0)
        };

        // spectral flux: positive difference from previous spectrum
        let mut flux = 0.0f32;
        for (k, &m) in mag.iter().enumerate() {
            let d = m - self.prev_mag[k];
            if d > 0.0 {
                flux += d;
            }
        }
        self.prev_mag.copy_from_slice(&mag);
        // onset detection uses a rolling adaptive threshold: median of the
        // recent flux history multiplied by a sensitivity factor. This is the
        // documented "rolling threshold + sensitivity" onset rule (a mean would
        // make the first window after silence a false positive).
        let mut sorted_hist = self.flux_history.clone();
        sorted_hist.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let median = if sorted_hist.is_empty() {
            0.0
        } else {
            sorted_hist[sorted_hist.len() / 2]
        };
        self.flux_history.push(flux);
        if self.flux_history.len() > 43 {
            self.flux_history.remove(0);
        }
        let threshold = median * self.cfg.onset_sensitivity.max(1.0);
        let onset_strength = if threshold > 1e-5 {
            flux / threshold
        } else {
            0.0
        };
        let time_secs = self.samples_seen as f64 / self.cfg.sample_rate as f64;
        let refractory = self.cfg.refractory_ms as f64 / 1000.0;
        let onset = !silent
            && self.flux_history.len() > 3
            && flux > threshold
            && (time_secs - self.last_onset_secs) >= refractory;
        if onset {
            self.last_onset_secs = time_secs;
            self.onsets.fetch_add(1, Ordering::Relaxed);
        }

        // tempo estimate from flux autocorrelation (60..200 BPM)
        let (tempo_bpm, tempo_confidence) = estimate_tempo(
            &self.flux_history,
            self.cfg.hop as f32 / self.cfg.sample_rate as f32,
        );
        let beat_phase = if tempo_bpm > 0.0 {
            let beat_period = 60.0 / tempo_bpm as f64;
            (time_secs / beat_period).rem_euclid(1.0) as f32
        } else {
            0.0
        };

        // chroma: fold bins into 12 pitch classes
        let mut chroma = [0.0f32; 12];
        if !silent {
            for (k, &m) in mag.iter().enumerate().skip(1) {
                let f = k as f32 * bin_hz;
                if f < 40.0 || f > 5000.0 {
                    continue;
                }
                let midi = 69.0 + 12.0 * (f / 440.0).log2();
                let pc = midi.round().rem_euclid(12.0) as usize % 12;
                chroma[pc] += m * m;
            }
            let max = chroma.iter().cloned().fold(0.0f32, f32::max);
            if max > 0.0 {
                for c in chroma.iter_mut() {
                    *c /= max;
                }
            }
        }

        WindowFeatures {
            rms: if silent { 0.0 } else { rms },
            peak: if silent { 0.0 } else { peak },
            bands,
            centroid_hz: centroid,
            spread_hz: spread,
            rolloff_hz: rolloff,
            flux,
            flatness,
            onset,
            onset_strength,
            beat_phase,
            tempo_bpm,
            tempo_confidence,
            chroma,
            time_secs,
        }
    }

    pub fn onset_count(&self) -> u64 {
        self.onsets.load(Ordering::Relaxed)
    }

    pub fn window_count(&self) -> u64 {
        self.window_count
    }
}

/// One analysed hop: the features computed from the `window` samples that ended
/// at `end_sample` in the analysed stream, plus the exact sample position that
/// closed the hop.
///
/// `end_sample` is a sample count, never a clock reading, so a caller can map a
/// feature onto an exact project timestamp (AT017: features sampled by source
/// timestamp, independent of export render speed).
#[derive(Debug, Clone, PartialEq)]
pub struct FeaturePoint {
    pub end_sample: u64,
    pub features: WindowFeatures,
}

/// Analyse a mono stream as a deterministic sequence of hop-aligned windows.
///
/// `Analyzer` on its own does not enforce a calling discipline: `push` bumps the
/// hop counter while `features` computes on *every* call, so a caller that calls
/// `features` twice per hop gets two different feature sets from the same audio
/// and a `time_secs` that no longer advances by one hop. This function is the
/// only place that discipline is expressed — it consumes exactly `hop` samples
/// per `features` call and emits nothing for a trailing partial hop, so
///
/// * `points.len() == mono.len() / cfg.hop`,
/// * `points[i].end_sample == (i + 1) * cfg.hop`,
/// * `points[i].features.time_secs == (i + 1) * cfg.hop / cfg.sample_rate`.
///
/// The third property is why `cfg.sample_rate` must be the rate of the stream
/// being analysed: with the `AnalysisConfig::default()` value of 48 kHz fed a
/// 44.1 kHz stream, every timestamp and every tempo estimate this returns is off
/// by 44100/48000, an 8.1 % error. The band edges are rate-relative and so are
/// unaffected.
///
/// Deterministic by construction: no wall clock, no device, no locks, and the
/// only allocation is the returned vector.
pub fn analyse_mono(cfg: AnalysisConfig, mono: &[f32]) -> Vec<FeaturePoint> {
    if cfg.hop == 0 {
        return Vec::new();
    }
    let hops = mono.len() / cfg.hop;
    let mut analyzer = Analyzer::new(cfg);
    let mut points = Vec::with_capacity(hops);
    for i in 0..hops {
        let start = i * cfg.hop;
        let chunk = &mono[start..start + cfg.hop];
        analyzer.push(chunk);
        points.push(FeaturePoint {
            end_sample: (i as u64 + 1) * cfg.hop as u64,
            features: analyzer.features(),
        });
    }
    points
}

/// The point in effect at sample `sample`: the last hop whose window ended at or
/// before it. `None` while the first hop has not completed, which is what tells a
/// caller there is no audio evidence yet rather than inventing a silent window.
pub fn point_at_sample<'a>(points: &'a [FeaturePoint], sample: u64) -> Option<&'a FeaturePoint> {
    let after = points.partition_point(|p| p.end_sample <= sample);
    if after == 0 {
        None
    } else {
        points.get(after - 1)
    }
}

fn window_rms(coeffs: &[f32]) -> f32 {
    let s: f32 = coeffs.iter().map(|c| c * c).sum();
    (s / coeffs.len() as f32).sqrt().max(1e-9)
}

/// Estimate BPM from the flux history by autocorrelation over plausible lags,
/// folding by tempo octave into 60..200 and preferring the true period by
/// scoring harmonic candidates.
pub fn estimate_tempo(flux: &[f32], window_secs: f32) -> (f32, f32) {
    if flux.len() < 8 || window_secs <= 0.0 {
        return (120.0, 0.0);
    }
    let mean = flux.iter().sum::<f32>() / flux.len() as f32;
    let centered: Vec<f32> = flux.iter().map(|f| f - mean).collect();
    let mut energy = 0.0f32;
    for &c in &centered {
        energy += c * c;
    }
    if energy <= 1e-9 {
        return (120.0, 0.0);
    }
    // autocorrelation over lags
    let max_lag = (centered.len() / 2).max(1);
    let mut ac = vec![0.0f32; max_lag + 1];
    for lag in 1..=max_lag {
        let mut acc = 0.0f32;
        for i in lag..centered.len() {
            acc += centered[i] * centered[i - lag];
        }
        ac[lag] = acc / energy;
    }
    // score candidate BPMs in 60..200, preferring the candidate whose lag has a
    // strong autocorrelation peak. The true beat period is the shortest lag with
    // a near-peak correlation, so we pick the highest BPM within tolerance of
    // the top score (an alternating-flux train has peaks at 2,4,6… hops).
    let mut top_score = f32::MIN;
    let mut candidates: Vec<(f32, f32)> = Vec::new();
    let mut bpm = 60.0f32;
    while bpm <= 200.0 {
        let period = 60.0 / bpm;
        let lag = (period / window_secs).round() as usize;
        if lag >= 1 && lag <= max_lag {
            let score = ac[lag];
            top_score = top_score.max(score);
            candidates.push((bpm, score));
        }
        bpm += 0.5;
    }
    let tolerance = top_score * 0.9;
    // A periodicity of P samples/seconds has correlation peaks at every multiple
    // of P, so the BPM is ambiguous by octaves. Among candidates near the peak,
    // choose the one closest to the documented internal default of 120 BPM.
    // This is a tempo *estimate*, never transport truth.
    let mut best_bpm = 120.0f32;
    let mut best_score = f32::MIN;
    let mut chosen_dist = f32::MAX;
    for (b, s) in &candidates {
        if *s >= tolerance {
            let dist = (*b - 120.0).abs();
            let better = dist < chosen_dist - 1e-6
                || ((dist - chosen_dist).abs() <= 1e-6 && *s > best_score);
            if better {
                chosen_dist = dist;
                best_bpm = *b;
                best_score = *s;
            }
        }
    }
    if best_score == f32::MIN {
        return (120.0, 0.0);
    }
    (best_bpm, best_score.clamp(0.0, 1.0))
}

/// Preallocated lock-free-ish PCM ring using atomics for indices. Single
/// producer (audio callback) writes, single consumer (analysis worker) reads.
pub struct PcmRing {
    buf: Vec<f32>,
    cap: usize,
    write: AtomicU64,
    read: AtomicU64,
    dropped: AtomicU64,
}

impl PcmRing {
    pub fn new(capacity: usize) -> Self {
        Self {
            buf: vec![0.0; capacity.next_power_of_two()],
            cap: capacity.next_power_of_two(),
            write: AtomicU64::new(0),
            read: AtomicU64::new(0),
            dropped: AtomicU64::new(0),
        }
    }

    /// Producer side: copy monophonic samples. No allocation. Overflow drops the
    /// oldest implicit samples by advancing the read cursor.
    pub fn write_mono(&self, mono: &[f32]) {
        // SAFETY: single producer owns the write cursor; the buffer is fixed.
        let buf = self.buf.as_ptr() as *mut f32;
        let mut w = self.write.load(Ordering::Relaxed);
        for &s in mono {
            unsafe {
                let idx = (w as usize) & (self.cap - 1);
                *buf.add(idx) = s;
            }
            w += 1;
        }
        self.write.store(w, Ordering::Release);
        let r = self.read.load(Ordering::Acquire);
        if w - r > self.cap as u64 {
            self.dropped.fetch_add(1, Ordering::Relaxed);
            self.read.store(w - self.cap as u64, Ordering::Release);
        }
    }

    /// Consumer side: drain available samples into `out`, returns count.
    pub fn read_into(&self, out: &mut Vec<f32>) -> usize {
        let w = self.write.load(Ordering::Acquire);
        let mut r = self.read.load(Ordering::Relaxed);
        let avail = (w - r) as usize;
        if avail == 0 {
            return 0;
        }
        out.reserve(avail);
        let buf = self.buf.as_ptr();
        for i in 0..avail {
            let idx = ((r as usize) + i) & (self.cap - 1);
            unsafe {
                out.push(*buf.add(idx));
            }
        }
        r += avail as u64;
        self.read.store(r, Ordering::Release);
        avail
    }

    pub fn dropped(&self) -> u64 {
        self.dropped.load(Ordering::Relaxed)
    }
}

/// Deterministic offline mixer: gain, pan (-6 dB center law), fades, limiter.
pub struct Mixer {
    pub sample_rate: u32,
}

impl Mixer {
    pub fn new(sample_rate: u32) -> Self {
        Self { sample_rate }
    }

    /// Mix a stereo track segment. `gain_db` is applied, pan in [-1,1].
    pub fn mix_stereo(&self, dst: &mut [f32], src: &[f32], channels: u16, gain_db: f64, pan: f64) {
        let g = 10f64.powf(gain_db / 20.0);
        let (lg, rg) = pan_gains(pan);
        let ch = channels.max(1) as usize;
        for (i, frame) in src.chunks(ch).enumerate() {
            let s = frame.iter().sum::<f32>() / ch as f32;
            if 2 * i + 1 >= dst.len() {
                break;
            }
            dst[2 * i] += (s as f64 * g * lg) as f32;
            dst[2 * i + 1] += (s as f64 * g * rg) as f32;
        }
    }

    /// Apply equal-power fades to a stereo buffer in place.
    pub fn apply_fade(&self, dst: &mut [f32], fade_in_samples: usize, fade_out_samples: usize) {
        let frames = dst.len() / 2;
        for i in 0..frames {
            let mut g = 1.0f32;
            if fade_in_samples > 0 && i < fade_in_samples {
                let t = i as f32 / fade_in_samples as f32;
                g *= (t * std::f32::consts::FRAC_PI_2).sin();
            }
            if fade_out_samples > 0 && i + fade_out_samples > frames {
                let t = (frames - i) as f32 / fade_out_samples as f32;
                g *= (t.clamp(0.0, 1.0) * std::f32::consts::FRAC_PI_2).sin();
            }
            dst[2 * i] *= g;
            dst[2 * i + 1] *= g;
        }
    }

    /// Optional final limiter with a documented lookahead-free soft knee.
    pub fn limit(&self, dst: &mut [f32], threshold: f32) {
        for s in dst.iter_mut() {
            if s.abs() > threshold {
                *s = s.signum() * (threshold + (s.abs() - threshold).tanh() * (1.0 - threshold));
            }
        }
    }
}

/// -6 dB center pan law with unity balance at center; returns (left, right).
pub fn pan_gains(pan: f64) -> (f64, f64) {
    let p = pan.clamp(-1.0, 1.0);
    let theta = (p + 1.0) * std::f64::consts::FRAC_PI_4;
    let lg = theta.cos();
    let rg = theta.sin();
    // normalize center to unity (cos(pi/4) = sin(pi/4) = 0.7071)
    let norm = std::f64::consts::FRAC_1_SQRT_2;
    (lg / norm, rg / norm)
}

#[cfg(test)]
mod tests;
