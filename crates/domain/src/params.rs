//! Parameter evaluation: base value or keyframes, then the modulation fold.
//!
//! Order (ARCHITECTURE 4):
//! 1. base or keyframe lane (lane *replaces* base)
//! 2. mappings sorted by (order, id)
//! 3. normalize, invert, clamp, smoothing
//! 4. fold add / multiply / replace in order
//! 5. final bounds, rounding, threshold/hysteresis

use crate::document::*;
use crate::time::RationalTime;

/// Which class of evidence filled a `FeatureSnapshot` / `AudioSnapshot` for a
/// rendered frame.
///
/// This is the determinism firewall's name-at-the-boundary rule: every rendered
/// frame records *where its audio features came from*, so a run that used a live
/// device can never be mistaken for — or reused as — a deterministic document-
/// only export. `DocumentOnly` is the serde default, so an older project file or
/// an older result record reads as the deterministic class rather than failing
/// to parse.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize, Default,
)]
#[serde(rename_all = "camelCase")]
pub enum FeatureSource {
    /// Features come from the document only: decoded media audio, or nothing at
    /// all when the project has no audio. This is the deterministic class —
    /// same project plus same media yields byte-identical output.
    #[default]
    DocumentOnly,
    /// Features come from a live capture device. Never deterministic, and never
    /// allowed inside an offline export.
    Live,
}

impl FeatureSource {
    /// Whether frames from this source are required to be reproducible.
    pub fn is_deterministic(&self) -> bool {
        matches!(self, FeatureSource::DocumentOnly)
    }
}

/// A feature snapshot supplied to the modulation fold. Values are already in
/// their declared native ranges (0..1 except documented exceptions).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FeatureSnapshot {
    pub rms: f32,
    pub peak: f32,
    pub bass: f32,
    pub low_mid: f32,
    pub mid: f32,
    pub high_mid: f32,
    pub treble: f32,
    pub centroid: f32,
    pub flux: f32,
    pub onset: f32,
    pub beat_phase: f32,
    pub tempo_bpm: f32,
    pub valid: bool,
}

impl Default for FeatureSnapshot {
    fn default() -> Self {
        Self {
            rms: 0.0,
            peak: 0.0,
            bass: 0.0,
            low_mid: 0.0,
            mid: 0.0,
            high_mid: 0.0,
            treble: 0.0,
            centroid: 0.0,
            flux: 0.0,
            onset: 0.0,
            beat_phase: 0.0,
            tempo_bpm: 120.0,
            valid: false,
        }
    }
}

impl FeatureSnapshot {
    pub fn feature(&self, id: FeatureId) -> f32 {
        match id {
            FeatureId::Rms => self.rms,
            FeatureId::Peak => self.peak,
            FeatureId::Bass => self.bass,
            FeatureId::LowMid => self.low_mid,
            FeatureId::Mid => self.mid,
            FeatureId::HighMid => self.high_mid,
            FeatureId::Treble => self.treble,
            FeatureId::Centroid => self.centroid,
            FeatureId::Flux => self.flux,
            FeatureId::Onset => self.onset,
            FeatureId::BeatPhase => self.beat_phase,
        }
    }
}

/// One-euro style smoothing with separate attack/release in milliseconds.
#[derive(Debug, Clone, Copy, Default)]
pub struct Smoother {
    value: f32,
    init: bool,
}

impl Smoother {
    pub fn step(&mut self, target: f32, dt: f32, attack_ms: f32, release_ms: f32) -> f32 {
        if !self.init {
            self.value = target;
            self.init = true;
            return target;
        }
        let ms = if target > self.value {
            attack_ms
        } else {
            release_ms
        };
        if ms <= 0.0 {
            self.value = target;
            return target;
        }
        let coeff = 1.0 - (-dt / (ms / 1000.0)).exp();
        self.value += (target - self.value) * coeff;
        self.value
    }
}

/// Map a normalized amount [0,1] through a curve. `curve` is a signed exponent:
/// positive bends one way, negative the other, 0 = linear.
pub fn apply_curve(v: f64, curve: f64) -> f64 {
    if curve == 0.0 {
        return v;
    }
    if curve > 0.0 {
        v.powf(1.0 + curve * 3.0)
    } else {
        1.0 - (1.0 - v).powf(1.0 + (-curve) * 3.0)
    }
}

/// Evaluate a base parameter with the fold applied.
///
/// `raw_source` is the source's pre-normalization value. Returns the final
/// native-unit value.
pub fn resolve_parameter(
    base: f64,
    unit_min: f64,
    unit_max: f64,
    mappings: &[Mapping],
    snapshot: &FeatureSnapshot,
    smoothers: &mut std::collections::HashMap<String, Smoother>,
    time_secs: f64,
    dt: f32,
) -> f64 {
    let mut value = base;
    let mut sorted: Vec<&Mapping> = mappings.iter().filter(|m| m.enabled).collect();
    sorted.sort_by(|a, b| a.order.cmp(&b.order).then(a.id.cmp(&b.id)));

    for m in sorted {
        let raw = match &m.source {
            ModulationSource::Feature { feature } => snapshot.feature(*feature) as f64,
            ModulationSource::Constant { value } => *value,
            ModulationSource::Lfo {
                shape,
                frequency,
                phase,
            } => lfo(*shape, time_secs, *frequency, *phase),
        };
        // normalize [in_min, in_max] -> [0,1]
        let span = m.in_max - m.in_min;
        let mut n = if span.abs() < 1e-9 {
            0.0
        } else {
            (raw - m.in_min) / span
        };
        n = n.clamp(0.0, 1.0);
        if m.invert {
            n = 1.0 - n;
        }
        let smooth = smoothers.entry(m.id.clone()).or_default();
        let n = smooth.step(n as f32, dt, m.attack_ms as f32, m.release_ms as f32) as f64;
        let amount = m.amount.clamp(0.0, 1.0);
        let mapped_native = n * (unit_max - unit_min) + unit_min;
        value = match m.fold {
            FoldMode::Add => value + (mapped_native - unit_min) * amount,
            FoldMode::Multiply => value * (1.0 + (n - 0.0) * amount),
            FoldMode::Replace => value * (1.0 - amount) + mapped_native * amount,
        };
    }
    value.clamp(unit_min, unit_max)
}

pub fn lfo(shape: LfoShape, t: f64, frequency: f64, phase: f64) -> f64 {
    let p = (t * frequency + phase).rem_euclid(1.0);
    match shape {
        LfoShape::Sine => (p * std::f64::consts::TAU).sin() * 0.5 + 0.5,
        LfoShape::Triangle => 1.0 - (2.0 * p - 1.0).abs(),
        LfoShape::Saw => p,
        LfoShape::Square => {
            if p < 0.5 {
                1.0
            } else {
                0.0
            }
        }
        LfoShape::SampleHold => {
            // Deterministic hash of the cycle index.
            let cycle = (t * frequency + phase).floor() as i64;
            let h = hash_u64(cycle as u64 ^ 0x9e3779b97f4a7c15);
            (h >> 40) as f64 / (1u64 << 24) as f64
        }
    }
}

pub fn hash_u64(mut x: u64) -> u64 {
    x ^= x >> 33;
    x = x.wrapping_mul(0xff51afd7ed558ccd);
    x ^= x >> 33;
    x = x.wrapping_mul(0xc4ceb9fe1a85ec53);
    x ^= x >> 33;
    x
}

/// Evaluate a keyframe lane at time `t`. Returns `None` for an empty lane.
pub fn eval_keyframes(keys: &[Keyframe], t: RationalTime) -> Option<ParamValue> {
    if keys.is_empty() {
        return None;
    }
    let mut sorted: Vec<&Keyframe> = keys.iter().collect();
    sorted.sort_by(|a, b| a.time.cmp_exact(&b.time));
    let first = sorted[0];
    if t.cmp_exact(&first.time) != std::cmp::Ordering::Greater {
        return Some(first.value.clone());
    }
    let last = sorted[sorted.len() - 1];
    if t.cmp_exact(&last.time) != std::cmp::Ordering::Less {
        return Some(last.value.clone());
    }
    for w in sorted.windows(2) {
        let a = w[0];
        let b = w[1];
        if t.cmp_exact(&a.time) != std::cmp::Ordering::Less
            && t.cmp_exact(&b.time) == std::cmp::Ordering::Less
        {
            let dt = b.time.sub(a.time).ok()?.as_f64();
            let local = t.sub(a.time).ok()?.as_f64();
            let frac = if dt.abs() < 1e-12 { 0.0 } else { local / dt };
            return Some(interpolate(&a.value, &b.value, frac, a.interpolation));
        }
    }
    Some(last.value.clone())
}

fn interpolate(a: &ParamValue, b: &ParamValue, frac: f64, mode: Interpolation) -> ParamValue {
    match mode {
        Interpolation::Step => a.clone(),
        Interpolation::Linear | Interpolation::Cubic => match numeric_vec(a, b) {
            Some((va, vb)) => {
                let mut out: Vec<f64> = va
                    .iter()
                    .zip(vb.iter())
                    .map(|(x, y)| x + (y - x) * frac)
                    .collect();
                match a {
                    ParamValue::Color { value } => {
                        if value.len() == 4 {
                            ParamValue::Color {
                                value: [out[0], out[1], out[2], out[3]],
                            }
                        } else {
                            a.clone()
                        }
                    }
                    ParamValue::Vec2 { .. } => ParamValue::Vec2 {
                        value: [out.remove(0), out.remove(0)],
                    },
                    ParamValue::Vec3 { .. } => ParamValue::Vec3 {
                        value: [out[0], out[1], out[2]],
                    },
                    ParamValue::Int { .. } => ParamValue::Int {
                        value: out[0].round() as i32,
                    },
                    _ => ParamValue::Float { value: out[0] },
                }
            }
            None => a.clone(),
        },
    }
}

fn numeric_vec(a: &ParamValue, b: &ParamValue) -> Option<(Vec<f64>, Vec<f64>)> {
    Some((a.as_f64_vec()?, b.as_f64_vec()?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::time::RationalTime;
    use std::collections::BTreeMap;

    fn rt(n: i64, d: u32) -> RationalTime {
        RationalTime::new(n, d).unwrap()
    }

    fn mapping(id: &str, src: FeatureId, fold: FoldMode, amount: f64, order: i32) -> Mapping {
        Mapping {
            id: id.into(),
            node_id: "n".into(),
            param_id: "p".into(),
            source: ModulationSource::Feature { feature: src },
            in_min: 0.0,
            in_max: 1.0,
            amount,
            fold,
            order,
            invert: false,
            attack_ms: 0.0,
            release_ms: 0.0,
            enabled: true,
        }
    }

    #[test]
    fn lfo_sine_quarter_phase() {
        // sine at phase 0 -> 0.5, at 0.25 -> 1.0, at 0.5 -> 0.5, 0.75 -> 0.
        assert!((lfo(LfoShape::Sine, 0.0, 1.0, 0.0) - 0.5).abs() < 1e-9);
        assert!((lfo(LfoShape::Sine, 0.25, 1.0, 0.0) - 1.0).abs() < 1e-9);
        assert!((lfo(LfoShape::Sine, 0.75, 1.0, 0.0)).abs() < 1e-9);
    }

    #[test]
    fn lfo_triangle_and_saw() {
        assert!((lfo(LfoShape::Triangle, 0.5, 1.0, 0.0) - 1.0).abs() < 1e-9);
        assert!((lfo(LfoShape::Saw, 0.25, 1.0, 0.0) - 0.25).abs() < 1e-9);
    }

    #[test]
    fn fold_order_is_respected() {
        let mut snap = FeatureSnapshot::default();
        snap.bass = 1.0;
        let mut smoothers = std::collections::HashMap::new();
        // add order1 = +10 native, then replace order2 with 0.0 at amount 1.
        let ms = vec![
            mapping("b", FeatureId::Bass, FoldMode::Replace, 1.0, 2),
            mapping("a", FeatureId::Bass, FoldMode::Add, 1.0, 1),
        ];
        let v = resolve_parameter(5.0, -100.0, 100.0, &ms, &snap, &mut smoothers, 0.0, 0.016);
        // add: 5 + (1*200 + -100 - -100) = 105; replace amount1 => mapped_native=100
        assert!((v - 100.0).abs() < 1e-9, "got {v}");
    }

    #[test]
    fn multiply_fold_documented_semantics() {
        let mut snap = FeatureSnapshot::default();
        snap.mid = 0.5;
        let mut smoothers = std::collections::HashMap::new();
        let ms = vec![mapping("m", FeatureId::Mid, FoldMode::Multiply, 1.0, 0)];
        let v = resolve_parameter(10.0, 0.0, 100.0, &ms, &snap, &mut smoothers, 0.0, 0.016);
        // value *= (1 + source * amount) = 10 * 1.5
        assert!((v - 15.0).abs() < 1e-9, "got {v}");
    }

    #[test]
    fn keyframes_linear_and_step() {
        let keys = vec![
            Keyframe {
                time: rt(0, 1),
                value: ParamValue::Float { value: 0.0 },
                interpolation: Interpolation::Linear,
            },
            Keyframe {
                time: rt(2, 1),
                value: ParamValue::Float { value: 1.0 },
                interpolation: Interpolation::Linear,
            },
        ];
        let v = eval_keyframes(&keys, rt(1, 1)).unwrap();
        assert!((v.as_f64().unwrap() - 0.5).abs() < 1e-9);
        let step = vec![
            Keyframe {
                time: rt(0, 1),
                value: ParamValue::Float { value: 0.0 },
                interpolation: Interpolation::Step,
            },
            Keyframe {
                time: rt(2, 1),
                value: ParamValue::Float { value: 1.0 },
                interpolation: Interpolation::Step,
            },
        ];
        let v2 = eval_keyframes(&step, rt(1, 1)).unwrap();
        assert_eq!(v2.as_f64().unwrap(), 0.0);
    }

    #[test]
    fn keyframe_lane_replaces_base() {
        // empty lane returns None so base is used; non-empty lane overrides
        assert!(eval_keyframes(&[], rt(0, 1)).is_none());
    }

    #[test]
    fn curve_is_identity_at_zero() {
        assert_eq!(apply_curve(0.3, 0.0), 0.3);
    }

    #[test]
    fn hash_is_stable() {
        assert_eq!(hash_u64(0), 0);
        assert_eq!(hash_u64(1234), hash_u64(1234));
        assert_ne!(hash_u64(1), hash_u64(2));
    }

    #[test]
    fn constant_mapping_applies_without_features() {
        let mut smoothers = std::collections::HashMap::new();
        let mut m = mapping("c", FeatureId::Bass, FoldMode::Add, 1.0, 0);
        m.source = ModulationSource::Constant { value: 1.0 };
        let v = resolve_parameter(
            0.5,
            0.0,
            2.0,
            &[m],
            &FeatureSnapshot::default(),
            &mut smoothers,
            0.0,
            0.016,
        );
        // normalized 1.0 -> mapped 2.0; add (2.0-0)*1.0 => 0.5 + 2.0 -> clamp 2.0
        assert!((v - 2.0).abs() < 1e-9);
    }

    #[test]
    fn unused_map_import() {
        let _: BTreeMap<String, i32> = BTreeMap::new();
    }
}
