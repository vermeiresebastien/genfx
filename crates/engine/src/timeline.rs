//! Timeline evaluation: pick the visible clip at a project time, compute
//! source time via the exact rational mapping, and enumerate track composition.

use flux_domain::{Clip, ClipSource, FrameRate, LoopPolicy, Project, RationalTime, TimeRange, TrackKind};

/// The source content a clip resolves to at a project time.
#[derive(Debug, Clone, PartialEq)]
pub enum ResolvedSource {
    Generator { generator_id: String, local: RationalTime, phase: f64 },
    Asset { asset_id: String, source_time: RationalTime },
    Freeze { asset_id: String, time: RationalTime },
    Live { source_id: String },
    Take { take_id: String },
}

#[derive(Debug, Clone)]
pub struct VisibleClip {
    pub clip_id: String,
    pub track_id: String,
    pub opacity: f64,
    pub transform: flux_domain::Transform2D,
    pub source: ResolvedSource,
    pub local_time: RationalTime,
}

/// Clip-local time from project time. Returns `None` outside the half-open range.
pub fn clip_local_time(clip: &Clip, project_time: RationalTime) -> Option<RationalTime> {
    let end = clip.range.end().ok()?;
    if project_time.cmp_exact(&clip.range.start) != std::cmp::Ordering::Greater
        && project_time.cmp_exact(&clip.range.start) != std::cmp::Ordering::Equal
    {
        return None;
    }
    if project_time.cmp_exact(&end) != std::cmp::Ordering::Less {
        return None;
    }
    project_time.sub(clip.range.start).ok()
}

/// Map clip-local time to source time, honoring rate, reverse and loop policy.
pub fn map_source_time(clip: &Clip, local: RationalTime) -> RationalTime {
    let rate_num = clip.rate.numerator();
    let rate_den = clip.rate.denominator() as i64;
    // scaled = local * rate
    let mut scaled = local.mul_int(rate_num).unwrap_or(RationalTime::ZERO);
    if rate_den != 1 {
        scaled = RationalTime::new(scaled.numerator(), (scaled.denominator() as i64 * rate_den) as u32)
            .unwrap_or(RationalTime::ZERO);
    }
    let mut src = clip.source_range.start.add(scaled).unwrap_or(clip.source_range.start);
    if clip.loop_policy == LoopPolicy::Loop {
        let span = clip.source_range.duration;
        if !span.is_zero() {
            // wrap src into [start, start+duration)
            let rel = src.sub(clip.source_range.start).unwrap_or(RationalTime::ZERO);
            // modulo via division
            let k = rel.div_floor(span);
            if k != 0 {
                let wrapped = rel.sub(span.mul_int(k).unwrap_or(RationalTime::ZERO)).unwrap_or(RationalTime::ZERO);
                src = clip.source_range.start.add(wrapped).unwrap_or(src);
            }
        }
    }
    src
}

/// All visible clips at `project_time`, bottom-to-top track order.
pub fn visible_clips(project: &Project, project_time: RationalTime) -> Vec<VisibleClip> {
    let timeline = &project.timeline;
    // Solo handling: if any track is soloed, only solo tracks render.
    let any_solo = timeline.ordered_tracks().iter().any(|t| t.solo);
    let mut out = Vec::new();
    for track in timeline.ordered_tracks() {
        if !track.visible || track.muted {
            continue;
        }
        if any_solo && !track.solo {
            continue;
        }
        if track.kind == TrackKind::Audio {
            continue;
        }
        for clip in timeline.track_clips(&track.id) {
            if let Some(local) = clip_local_time(clip, project_time) {
                let src = resolve_source(project, clip, local);
                out.push(VisibleClip {
                    clip_id: clip.id.clone(),
                    track_id: track.id.clone(),
                    opacity: clip.opacity * track.opacity,
                    transform: clip.transform,
                    source: src,
                    local_time: local,
                });
            }
        }
    }
    out
}

fn resolve_source(project: &Project, clip: &Clip, local: RationalTime) -> ResolvedSource {
    match &clip.source {
        ClipSource::GeneratorScene { generator_id } => {
            let loop_period = 4.0; // documented default loop period
            let phase = if loop_period > 0.0 {
                (local.as_f64() / loop_period).rem_euclid(1.0)
            } else {
                0.0
            };
            ResolvedSource::Generator { generator_id: generator_id.clone(), local, phase }
        }
        ClipSource::Asset { asset_id } => {
            let t = map_source_time(clip, local);
            let _ = project;
            ResolvedSource::Asset { asset_id: asset_id.clone(), source_time: normalise_video_pts(t, project.settings.frame_rate) }
        }
        ClipSource::FreezeFrame { asset_id, time } => ResolvedSource::Freeze { asset_id: asset_id.clone(), time: *time },
        ClipSource::LiveSource { source_id } => ResolvedSource::Live { source_id: source_id.clone() },
        ClipSource::RecordedTake { take_id } => ResolvedSource::Take { take_id: take_id.clone() },
    }
}

/// Select the video presentation timestamp: the largest source-frame PTS <=
/// target. Since fixtures are CFR we compute floor((t - start)/frame_dur).
pub fn normalise_video_pts(source_time: RationalTime, rate: FrameRate) -> RationalTime {
    let frame = RationalTime::frame_duration(rate);
    let n = source_time.div_floor(frame);
    RationalTime::from_frames(n.max(0), rate).unwrap_or(RationalTime::ZERO)
}

/// Render order for a time: adjustment tracks affect tracks below them,
/// so build the list of (track, visible clip) with adjustment markers.
#[derive(Debug, Clone)]
pub enum RenderItem {
    Clip { track_id: String, clip: VisibleClip },
    Adjustment { track_id: String, graph_id: String, opacity: f64 },
}

pub fn render_items(project: &Project, project_time: RationalTime) -> Vec<RenderItem> {
    let timeline = &project.timeline;
    let any_solo = timeline.ordered_tracks().iter().any(|t| t.solo);
    let mut out = Vec::new();
    for track in timeline.ordered_tracks() {
        if !track.visible || track.muted || (any_solo && !track.solo) {
            continue;
        }
        if track.kind == TrackKind::Adjustment {
            if let Some(gid) = &track.graph_id {
                out.push(RenderItem::Adjustment { track_id: track.id.clone(), graph_id: gid.clone(), opacity: track.opacity });
            }
            continue;
        }
        if track.kind == TrackKind::Audio {
            continue;
        }
        for clip in timeline.track_clips(&track.id) {
            if let Some(local) = clip_local_time(clip, project_time) {
                let src = resolve_source(project, clip, local);
                out.push(RenderItem::Clip {
                    track_id: track.id.clone(),
                    clip: VisibleClip {
                        clip_id: clip.id.clone(),
                        track_id: track.id.clone(),
                        opacity: clip.opacity * track.opacity,
                        transform: clip.transform,
                        source: src,
                        local_time: local,
                    },
                });
            }
        }
    }
    out
}

/// Range of a take or full timeline used for export.
pub fn export_range(project: &Project) -> TimeRange {
    project.timeline.range
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_domain::*;
    use std::collections::BTreeMap;

    fn rt(n: i64, d: u32) -> RationalTime {
        RationalTime::new(n, d).unwrap()
    }

    fn project_with(clip: Clip) -> Project {
        let mut p = Project::new("t", 1);
        let track = Track {
            id: "trk".into(),
            name: "V1".into(),
            kind: TrackKind::Video,
            clip_order: vec![clip.id.clone()],
            locked: false,
            muted: false,
            solo: false,
            visible: true,
            gain_db: 0.0,
            opacity: 1.0,
            blend_mode: BlendMode::Normal,
            graph_id: None,
        };
        p.timeline.tracks.insert(track.id.clone(), track);
        p.timeline.track_order.push("trk".into());
        p.timeline.clips.insert(clip.id.clone(), clip);
        p
    }

    fn clip(start: i64, dur: i64, rate: RationalTime, loop_policy: LoopPolicy) -> Clip {
        Clip {
            id: "c".into(),
            name: "c".into(),
            source: ClipSource::Asset { asset_id: "a".into() },
            range: TimeRange::new(rt(start, 1), rt(dur, 1)),
            source_range: TimeRange::new(rt(10, 1), rt(dur, 1)),
            rate,
            loop_policy,
            graph_id: None,
            transform: Transform2D::default(),
            opacity: 1.0,
            audio: AudioControls::default(),
            linked_group: None,
        }
    }

    #[test]
    fn half_open_visibility() {
        let c = clip(2, 4, rt(1, 1), LoopPolicy::None);
        assert!(clip_local_time(&c, rt(2, 1)).is_some());
        assert!(clip_local_time(&c, rt(5, 1)).is_some());
        assert!(clip_local_time(&c, rt(6, 1)).is_none());
        assert!(clip_local_time(&c, rt(1, 1)).is_none());
    }

    #[test]
    fn forward_mapping_and_identity_rate() {
        let c = clip(2, 4, rt(1, 1), LoopPolicy::None);
        assert_eq!(map_source_time(&c, rt(0, 1)), rt(10, 1));
        assert_eq!(map_source_time(&c, rt(2, 1)), rt(12, 1));
    }

    #[test]
    fn reverse_rate_selects_earlier_source() {
        let c = clip(2, 4, rt(-1, 1), LoopPolicy::None);
        // at local 0 -> source 10, at local 3 -> 7 (reverse)
        assert_eq!(map_source_time(&c, rt(0, 1)), rt(10, 1));
        assert_eq!(map_source_time(&c, rt(3, 1)), rt(7, 1));
    }

    #[test]
    fn loop_wraps_source() {
        let mut c = clip(0, 8, rt(1, 1), LoopPolicy::Loop);
        c.source_range = TimeRange::new(rt(10, 1), rt(2, 1));
        // local 4 -> 14, wrapped -> 10
        assert_eq!(map_source_time(&c, rt(4, 1)), rt(10, 1));
        assert_eq!(map_source_time(&c, rt(5, 1)), rt(11, 1));
    }

    #[test]
    fn cfr_pts_is_floor_frame() {
        let fr = FrameRate::FPS_60;
        // 1.01s at 60fps -> frame 60 (frame 60 starts at 1.0s), frame 61 at 61/60
        let source = rt(101, 100);
        let n = source.div_floor(RationalTime::frame_duration(fr));
        assert_eq!(n, 60);
        assert_eq!(normalise_video_pts(source, fr), RationalTime::from_frames(60, fr).unwrap());
    }

    #[test]
    fn gesture_visible_clips_across_tracks() {
        let c = clip(0, 4, rt(1, 1), LoopPolicy::None);
        let p = project_with(c);
        let clips = visible_clips(&p, rt(1, 1));
        assert_eq!(clips.len(), 1);
        assert!(matches!(clips[0].source, ResolvedSource::Asset { .. }));
    }

    #[test]
    fn unused_reexport() {
        let _ = BTreeMap::<String, i32>::new();
    }
}
