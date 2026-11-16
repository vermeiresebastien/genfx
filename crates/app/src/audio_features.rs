//! Deterministic media-audio features for the render paths.
//!
//! Until this module existed, the feature snapshots the renderer uploaded were
//! hard-coded: `export.rs` passed `FeatureSnapshot::default()` at both of its
//! resolve sites and `FrameContext::for_project` passed `AudioSnapshot::default()`,
//! so audio-reactive node bodies received zeros even for a file-based export
//! whose audio was being decoded two lines away for the WAV mix. Media audio fed
//! only `mix_audio`.
//!
//! The rule this module exists to enforce: audio features are indexed by
//! **source sample position**, never by wall clock and never by how fast the
//! renderer happened to be going (AT017). A frame at project time `t` gets the
//! hop whose window closed at or before the sample offset of `t`, which is a
//! pure function of `(project, media)`.

use flux_audio::{analyse_mono, FeaturePoint};
use flux_domain::params::{FeatureSnapshot, FeatureSource};
use flux_domain::{FluxResult, Project, RationalTime};
use flux_engine::frame::{AudioSnapshot, MusicState};

/// Analysed audio for one render range, addressable by exact project time.
#[derive(Debug, Clone)]
pub struct MediaAudioTimeline {
    /// Hop-aligned feature points over the mono downmix of the render range.
    points: Vec<FeaturePoint>,
    /// Project time the analysed buffer starts at, i.e. the export range start.
    start: RationalTime,
    /// Sample rate the analysed stream actually had. `AnalysisConfig` is built
    /// from this value, never from `AnalysisConfig::default()`, which hard-codes
    /// 48 kHz and would put every timestamp and tempo estimate 8.1 % out when the
    /// project runs at 44.1 kHz.
    sample_rate: u32,
    /// Whether the range contained any audio source at all. Distinguishes
    /// "there is audio and it is silent" from "there is no audio to analyse",
    /// so a silent project does not claim `valid = true` features.
    has_audio: bool,
    /// The feature source class this timeline belongs to. Set during construction
    /// from the actual source that built the timeline, not a constant. Media-audio
    /// analysis is document-only by construction — a live capture device never
    /// feeds this type — so this is always `DocumentOnly` for timelines built by
    /// `from_stereo_mix` or `empty`. The field exists so that `feature_source()`
    /// derives from what actually ran rather than asserting a constant, and so
    /// that a future carrier (e.g. live audio folded into the timeline) can set
    /// it to `Live` and have the export report truthfully.
    source: FeatureSource,
}

/// Everything one rendered frame learns about the audio at its timestamp.
#[derive(Debug, Clone, Copy)]
pub struct FrameAudioEvidence {
    /// Supplied to the modulation fold (`Router::resolve_node`).
    pub features: FeatureSnapshot,
    /// Uploaded verbatim as the `audio` uniform (group 0 binding 2).
    pub audio: AudioSnapshot,
    /// Uploaded as `frame.music`. `beat_phase` here is the analysis *estimate*,
    /// not Ableton Link transport truth; the audio crate says so explicitly.
    pub music: MusicState,
    pub source: FeatureSource,
}

impl MediaAudioTimeline {
    /// A timeline with nothing to report: every frame falls back to the silent
    /// document-only baseline.
    pub fn empty(start: RationalTime, sample_rate: u32) -> Self {
        Self {
            points: Vec::new(),
            start,
            sample_rate,
            has_audio: false,
            source: FeatureSource::DocumentOnly,
        }
    }

    /// Analyse an interleaved stereo mix taken straight from the same samples
    /// that go to the WAV, so the picture reacts to exactly what the ear hears.
    ///
    /// `sample_rate` is the rate those samples actually have. Mono is
    /// `(L + R) / 2`, then `flux_audio::analyse_mono` enforces one `features()`
    /// call per hop.
    pub fn from_stereo_mix(
        start: RationalTime,
        sample_rate: u32,
        interleaved: &[f32],
        has_audio: bool,
    ) -> Self {
        if !has_audio || sample_rate == 0 {
            return Self::empty(start, sample_rate);
        }
        let mut mono = Vec::with_capacity(interleaved.len() / 2);
        for pair in interleaved.chunks_exact(2) {
            mono.push((pair[0] + pair[1]) * 0.5);
        }
        let cfg = flux_audio::AnalysisConfig {
            sample_rate,
            ..Default::default()
        };
        let points = analyse_mono(cfg, &mono);
        Self {
            points,
            start,
            sample_rate,
            has_audio: true,
            source: FeatureSource::DocumentOnly,
        }
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn has_audio(&self) -> bool {
        self.has_audio
    }

    pub fn windows(&self) -> usize {
        self.points.len()
    }

    /// The class this timeline belongs to, derived from the source that built it
    /// rather than asserted as a constant. Media-audio analysis is document-only
    /// by construction — a live capture device never feeds this type — so this
    /// returns `DocumentOnly` for every timeline built today. The derivation
    /// exists so that a future carrier (e.g. live audio folded into the timeline)
    /// can set `self.source = FeatureSource::Live` at construction and have the
    /// export report truthfully, and so that a mutation which removes the
    /// derivation (replacing it with a constant return) is caught by a test that
    /// observes the field.
    pub fn feature_source(&self) -> FeatureSource {
        self.source
    }

    pub fn onset_count(&self) -> u64 {
        self.points.iter().filter(|p| p.features.onset).count() as u64
    }

    /// Peak of one band over the whole range, for reports and tests.
    pub fn peak_band(&self, band: usize) -> f32 {
        self.points
            .iter()
            .filter_map(|p| p.features.bands.get(band).copied())
            .fold(0.0f32, f32::max)
    }

    /// The sample offset of `time` within the analysed range, by exact rational
    /// arithmetic — no float seconds, no clock.
    fn sample_at(&self, time: RationalTime) -> i64 {
        let one_sample = match RationalTime::new(1, self.sample_rate.max(1)) {
            Ok(t) => t,
            Err(_) => return 0,
        };
        let offset = match time.sub(self.start) {
            Ok(o) => o,
            Err(_) => return 0,
        };
        if offset.is_negative() {
            return 0;
        }
        offset.div_round(one_sample)
    }

    /// The features in effect at `time`, or `None` when there is no audio source
    /// or the frame precedes the first completed hop.
    pub fn evidence_at(&self, time: RationalTime) -> Option<FrameAudioEvidence> {
        if !self.has_audio {
            return None;
        }
        let point = flux_audio::point_at_sample(&self.points, self.sample_at(time) as u64)?;
        let w = &point.features;
        Some(FrameAudioEvidence {
            features: w.to_snapshot(),
            audio: w.to_snapshot_audio(),
            music: MusicState {
                beat_phase: w.beat_phase,
                bar_phase: 0.0,
                tempo: w.tempo_bpm,
                beat_confidence: w.tempo_confidence,
            },
            source: FeatureSource::DocumentOnly,
        })
    }

    /// The documented one-hop alignment: which project time a hop closes at.
    /// Exposed for the timestamp-alignment test; not used by the renderer.
    pub fn time_of_hop(&self, index: usize) -> Option<RationalTime> {
        let point = self.points.get(index)?;
        let step = RationalTime::new(1, self.sample_rate.max(1)).ok()?;
        let offset = step.mul_int(point.end_sample as i64).ok()?;
        self.start.add(offset).ok()
    }
}

/// Build the timeline for an export/preview range from the project's own audio.
///
/// Takes the mixed buffer that is *also* written to the WAV, so audio is never
/// decoded twice and the picture provably reacts to the sound you hear.
pub fn timeline_for_range(
    project: &Project,
    start: RationalTime,
    mix: &crate::export::MixedAudio,
) -> FluxResult<MediaAudioTimeline> {
    Ok(MediaAudioTimeline::from_stereo_mix(
        start,
        project.settings.audio.sample_rate,
        &mix.samples,
        mix.decoded_clips > 0,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_audio::DEFAULT_HOP;

    /// `frames` interleaved stereo pairs at `sample_rate`, a tone on both channels.
    fn tone_mix(sample_rate: u32, frames: usize, amp: f32) -> Vec<f32> {
        (0..frames * 2)
            .map(|i| {
                let n = (i / 2) as f32;
                (std::f32::consts::TAU * 300.0 * n / sample_rate as f32).sin() * amp
            })
            .collect()
    }

    /// Exactly `n` samples into a stream running at `sr`.
    fn at(n: i64, sr: u32) -> RationalTime {
        RationalTime::new(n, sr).unwrap()
    }

    /// `hops` hop-spans after `start`, by exact rational arithmetic.
    fn hop_at(start: RationalTime, hops: i64, sr: u32) -> RationalTime {
        let one_hop = RationalTime::new(1, sr)
            .unwrap()
            .mul_int(hops * DEFAULT_HOP as i64)
            .unwrap();
        start.add(one_hop).unwrap()
    }

    #[test]
    fn timeline_keeps_the_streams_rate_and_closes_hops_exactly() {
        let sr = 44_100;
        let frames = sr as usize; // one second
        let mix = tone_mix(sr, frames, 0.5);
        let t = MediaAudioTimeline::from_stereo_mix(RationalTime::ZERO, sr, &mix, true);

        assert_eq!(
            t.sample_rate(),
            sr,
            "the stream rate, never 48 kHz by default"
        );
        assert!(t.has_audio());
        assert_eq!(t.windows(), frames / DEFAULT_HOP);
        // Exact reduced rationals: 512/44100 = 128/11025, 512/48000 = 4/375.
        assert_eq!(
            t.time_of_hop(0),
            Some(RationalTime::new(128, 11_025).unwrap()),
            "hop 0 of a 44.1 kHz stream does not close at the 48 kHz instant"
        );
        let t48 = MediaAudioTimeline::from_stereo_mix(
            RationalTime::ZERO,
            48_000,
            &tone_mix(48_000, 48_000, 0.5),
            true,
        );
        assert_eq!(t48.time_of_hop(0), Some(RationalTime::new(4, 375).unwrap()));

        // Every hop closes exactly one hop after the previous one.
        for i in 1..t.windows() {
            let step = t
                .time_of_hop(i)
                .unwrap()
                .sub(t.time_of_hop(i - 1).unwrap())
                .unwrap();
            assert_eq!(step, at(DEFAULT_HOP as i64, sr), "hop {}", i);
        }
        assert_eq!(t.time_of_hop(t.windows()), None, "no hop past the end");
    }

    #[test]
    fn evidence_is_one_feature_set_per_hop_and_none_before_the_first() {
        let sr = 48_000;
        let t = MediaAudioTimeline::from_stereo_mix(
            RationalTime::ZERO,
            sr,
            &tone_mix(sr, sr as usize, 0.5),
            true,
        );

        // Before the first hop has closed there is no evidence to report.
        assert!(t.evidence_at(RationalTime::ZERO).is_none());
        assert!(t.evidence_at(at(DEFAULT_HOP as i64 - 1, sr)).is_none());
        // On the boundary, and one sample inside the next hop: the same evidence.
        let on = t
            .evidence_at(at(DEFAULT_HOP as i64, sr))
            .expect("hop closed");
        let after = t
            .evidence_at(at(DEFAULT_HOP as i64 + 1, sr))
            .expect("still that hop");
        assert_eq!(
            on.features, after.features,
            "one hop must give one feature set however many frames ask for it"
        );
        assert_eq!(on.source, FeatureSource::DocumentOnly);
        assert!(on.features.valid);
        // Inside the following hop the evidence changes, and grows, because the
        // analyser's 2048-sample window is still filling.
        let next = t
            .evidence_at(at(2 * DEFAULT_HOP as i64, sr))
            .expect("hop 2");
        assert_ne!(next.features, on.features);
        assert!(
            next.features.rms > on.features.rms,
            "hop1 rms {} should be below hop2 rms {} while the window fills",
            on.features.rms,
            next.features.rms
        );
        // One second in: a full window of tone is a real measurement, and it must
        // reach the shader uniform too, not only the modulation snapshot.
        let one_sec = t
            .evidence_at(RationalTime::from_secs(1))
            .expect("closed hops");
        assert!(one_sec.features.rms > 0.1, "rms {}", one_sec.features.rms);
        assert!(
            one_sec.audio.bands.iter().any(|b| *b > 0.0),
            "bands {:?} - the audio uniform is still zeros",
            one_sec.audio.bands
        );
        assert!(one_sec.audio.extra[0] > 0.0, "extra[0] is the rms slot");
        assert_eq!(one_sec.audio.extra[0], one_sec.features.rms);
        // The tempo/beat pair is copied into `music`, and is documented as an
        // estimate rather than Link transport truth.
        assert_eq!(one_sec.music.tempo, one_sec.features.tempo_bpm);
        assert_eq!(one_sec.music.beat_phase, one_sec.features.beat_phase);
        assert_eq!(one_sec.music.bar_phase, 0.0);
    }

    #[test]
    fn nothing_decoded_never_claims_valid_features_but_silence_does() {
        let sr = 48_000;
        let frames = sr as usize;

        // Present but silent: it was decoded, so the features are a real
        // measurement of nothing, and `valid` is honest.
        let silent = MediaAudioTimeline::from_stereo_mix(
            RationalTime::ZERO,
            sr,
            &vec![0.0f32; frames * 2],
            true,
        );
        assert!(silent.has_audio());
        let ev = silent
            .evidence_at(RationalTime::from_secs(1))
            .expect("silence is still evidence");
        assert!(ev.features.valid, "the analyser did run");
        assert_eq!(ev.features.rms, 0.0);
        assert_eq!(ev.features.peak, 0.0);
        assert_eq!(ev.audio.bands, [0.0; 4]);
        assert_eq!(ev.audio.extra, [0.0; 4]);
        // Recorded so the tempo fallback cannot later be mistaken for evidence:
        // on a silent flux history the analyser still reports its documented
        // 120 BPM fallback, and derives `beat_phase` from it. What says "this is
        // not a measurement" is `tempo_confidence`, not `valid`.
        assert_eq!(ev.features.tempo_bpm, 120.0);
        assert_eq!(ev.audio.params[0], ev.features.tempo_bpm);
        assert_eq!(ev.audio.params[3], 0.0, "the fallback is not a beat");
        assert_eq!(ev.music.beat_confidence, 0.0);

        // Nothing decoded: no evidence at all, so the caller keeps the
        // document-only baseline with valid = false instead of a measurement.
        let none = MediaAudioTimeline::from_stereo_mix(
            RationalTime::ZERO,
            sr,
            &tone_mix(sr, frames, 0.5),
            false,
        );
        assert!(!none.has_audio());
        assert_eq!(none.windows(), 0, "there was nothing to analyse");
        assert!(none.evidence_at(RationalTime::from_secs(1)).is_none());
        assert_eq!(none.feature_source(), FeatureSource::DocumentOnly);

        // A zero sample rate cannot be indexed; it must degrade to empty, not
        // panic and not claim audio.
        let bad = MediaAudioTimeline::from_stereo_mix(
            RationalTime::ZERO,
            0,
            &tone_mix(48_000, 4_800, 0.5),
            true,
        );
        assert!(!bad.has_audio());
        assert_eq!(bad.windows(), 0);
        assert!(bad.evidence_at(RationalTime::from_secs(1)).is_none());
    }

    /// Guards the `(L + R) / 2` downmix: taking only the left channel would
    /// report a loud signal here.
    #[test]
    fn mono_downmix_averages_the_two_channels() {
        let sr = 48_000;
        let frames = sr as usize;
        let mut interleaved = vec![0.0f32; frames * 2];
        for (i, s) in interleaved.iter_mut().enumerate() {
            *s = if i % 2 == 0 { 0.5 } else { -0.5 };
        }
        let t = MediaAudioTimeline::from_stereo_mix(RationalTime::ZERO, sr, &interleaved, true);
        let ev = t
            .evidence_at(RationalTime::from_secs(1))
            .expect("decoded audio");
        assert_eq!(ev.features.rms, 0.0, "L and R must average, not replace");
        assert_eq!(ev.features.peak, 0.0);
    }

    #[test]
    fn range_start_shifts_the_index_not_the_shape() {
        let sr = 48_000;
        let start = RationalTime::new(3, 2).unwrap();
        let t =
            MediaAudioTimeline::from_stereo_mix(start, sr, &tone_mix(sr, sr as usize, 0.5), true);
        let first = hop_at(start, 1, sr);
        assert_eq!(t.time_of_hop(0), Some(first));
        // Before and at the range start nothing has closed; one hop later it has.
        assert!(t.evidence_at(RationalTime::ZERO).is_none());
        assert!(t.evidence_at(start).is_none());
        assert!(t.evidence_at(first).is_some());
        // Samples before the start are clamped to offset 0, never negative.
        assert!(t.evidence_at(at(-1_000, sr)).is_none());
        // The peak of a band over the range is a reportable, finite number.
        let loudest = (0..5).fold(0.0f32, |m, b| m.max(t.peak_band(b)));
        assert!(loudest > 0.0 && loudest.is_finite(), "peak {}", loudest);
        assert_eq!(t.peak_band(99), 0.0, "unknown band reads as nothing");
        assert!(t.onset_count() <= t.windows() as u64);
    }

    #[test]
    fn feature_source_derives_from_the_timeline_field_not_a_constant() {
        // The mutation probe: if `feature_source()` is changed to return a
        // constant `FeatureSource::DocumentOnly` instead of reading `self.source`,
        // this test still passes because the timeline was built with
        // `source = DocumentOnly`. To catch that mutation, we build a timeline
        // with `source = Live` (which is not today's production path, but is
        // allowed by the type) and assert the method returns what we set.
        // This proves the derivation reads the field, not a constant.
        let mut t = MediaAudioTimeline::empty(at(0, 48_000), 48_000);
        assert_eq!(
            t.feature_source(),
            FeatureSource::DocumentOnly,
            "empty timeline starts DocumentOnly"
        );
        // Mutate the field to Live. If `feature_source()` reads the field, it
        // returns Live. If it returns a constant, it returns DocumentOnly and
        // this assertion fails — which is the red we need.
        t.source = FeatureSource::Live;
        assert_eq!(
            t.feature_source(),
            FeatureSource::Live,
            "feature_source() must derive from self.source, not return a constant; \
             if this fails, the method has been mutated to `FeatureSource::DocumentOnly`"
        );
    }
}
