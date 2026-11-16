//! Live audio: a device tap, a PCM ring, and one analyser that publishes a
//! single status slot.
//!
//! This module is the CPU half of the `live-audio` build. It mirrors
//! [`crate::control`]’s `OscBridge` in every structural respect: one worker
//! thread per role, atomics for counters, one `Arc<Mutex<Option<…>>>` slot for
//! the result, a rendezvous that reports the opened stream back to the caller
//! before `start` returns, a stop flag flipped by `stop()` or `Drop`, and no
//! process-global state.
//!
//! # Determinism firewall
//!
//! Nothing here touches `Project`, `RationalTime`, or `wgpu`. `Instant` appears
//! only to measure this bridge's own elapsed time — never a sample position —
//! and no export or preview render path reads a value produced here. Offline
//! export keeps using document and media audio
//! ([`crate::audio_features::MediaAudioTimeline`]), which reports
//! [`flux_domain::params::FeatureSource::DocumentOnly`]. The live slot reports
//! `Live`; the two never mix, and a result records which source ran.
//!
//! A live reading can reach the document only through the revision/undo seam
//! `GuiCommand::LiveOsc` already uses, so it is always a named, undoable edit
//! rather than a side effect. Nothing in this file performs such an edit.
//!
//! # Explicit controls, no auto-gain
//!
//! [`LiveAudioConfig::input_db`] is applied once, to device samples, as a
//! linear gain, and the status echoes it back. There is no automatic trim: a
//! `volume=0.2` tone measured on this rig arrived at `peak = 0.0083`, and
//! quietly compensating for that would turn an input level into an engine
//! property the owner cannot see.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use flux_audio::{AnalysisConfig, Analyzer, PcmRing};
use flux_domain::params::{FeatureSnapshot, FeatureSource};
use flux_domain::{FluxError, FluxResult};
/// Re-exported so `flux capture` and the IPC arms reach the device layer
/// through one path: the types below are the live-audio contract, and a caller
/// that has to import from two crates to use it will get the two out of step.
pub use flux_platform::capture::{
    list_endpoints, CaptureConfig, Drain, EndpointInfo, LiveCapture, Tap, WireFormat,
    LIVE_AUDIO_COMPILED,
};

/// `off` | `loopback` | `mic`. Default **off**: no device is tapped until the
/// owner says so, on this machine and in every config file.
pub const ENV_MODE: &str = "FLUX_LIVE_AUDIO";
/// Exact WASAPI endpoint id. Unset means the `eConsole` default for the tap —
/// which on a rig with several devices is the wrong kind of accident.
pub const ENV_DEVICE: &str = "FLUX_LIVE_AUDIO_DEVICE";
/// Ring depth in milliseconds, and therefore the longest burst the analyser can
/// fall behind by before samples are skipped. Default 100.
pub const ENV_RING_MS: &str = "FLUX_LIVE_AUDIO_RING_MS";
/// Explicit input trim in dBFS. Default `+0.0`, never auto-adjusted.
pub const ENV_INPUT_DB: &str = "FLUX_LIVE_AUDIO_INPUT_DB";
/// Where the status sidecar lives. Unset means one machine-global rendezvous
/// file, which is the contract `flux capture status`/`stop` depend on; naming a
/// path here is how a second deliberate run — or a second user on this machine —
/// gets its own record instead of silently sharing the first one's.
pub const ENV_STATUS_PATH: &str = "FLUX_LIVE_AUDIO_STATUS";

pub const DEFAULT_MODE: LiveMode = LiveMode::Off;
pub const DEFAULT_RING_MS: u32 = 100;
pub const DEFAULT_INPUT_DB: f32 = 0.0;
/// Stream buffer we ask WASAPI for, in ms. Measured device periods on this rig
/// are 3 ms (`30000` in 100-ns units); 10 ms is the comfortable shared-mode
/// floor and matches the `FLUX_LIVE_AUDIO_RING_MS` default's order of magnitude.
pub const DEFAULT_BUFFER_MS: u32 = 10;
/// How often both threads wake when idle, in ms.
pub const DEFAULT_POLL_MS: u32 = 3;
/// Loopback has no clock: `GetNextPacketSize` is zero while nothing renders.
/// Past this many milliseconds without a packet, the reading is stale.
pub const STALL_LIMIT_MS: u64 = 100;
/// The status sidecar is rewritten at most this often, so a 480 Hz publish rate
/// does not turn into disk churn.
pub const STATUS_WRITE_INTERVAL_MS: u64 = 250;
/// The stop file is checked this often; it is a control channel, not a sample
/// path, so it does not need to be as fast as the ring.
pub const STOP_CHECK_INTERVAL_MS: u64 = 100;
/// Fastest sample rate we accept, which is what sizes the ring: `PcmRing` is
/// allocated in samples, and 100 ms must be available at 192 kHz too.
const SAMPLES_PER_MS_CEILING: usize = 192;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LiveMode {
    Off,
    Loopback,
    Mic,
}

impl LiveMode {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "" | "off" | "0" | "none" => Some(Self::Off),
            "loopback" | "render" | "mix" => Some(Self::Loopback),
            "mic" | "microphone" | "capture" => Some(Self::Mic),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Loopback => "loopback",
            Self::Mic => "mic",
        }
    }

    pub fn tap(&self) -> Option<Tap> {
        match self {
            Self::Loopback => Some(Tap::Loopback),
            Self::Mic => Some(Tap::Microphone),
            Self::Off => None,
        }
    }
}

/// Everything the live path needs. Every field is an explicit choice.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LiveAudioConfig {
    pub mode: LiveMode,
    pub device_id: Option<String>,
    pub ring_ms: u32,
    pub input_db: f32,
    pub buffer_ms: u32,
    pub poll_ms: u32,
}

impl Default for LiveAudioConfig {
    fn default() -> Self {
        Self {
            mode: DEFAULT_MODE,
            device_id: None,
            ring_ms: DEFAULT_RING_MS,
            input_db: DEFAULT_INPUT_DB,
            buffer_ms: DEFAULT_BUFFER_MS,
            poll_ms: DEFAULT_POLL_MS,
        }
    }
}

impl LiveAudioConfig {
    /// Read the four `FLUX_LIVE_AUDIO*` variables. A value that will not parse
    /// is returned as a complaint rather than silently defaulted: an operator
    /// who writes `+3db` must not get `+0.0` and believe otherwise.
    pub fn from_env() -> (Self, Vec<String>) {
        let mut cfg = Self::default();
        let mut problems = Vec::new();

        if let Some(raw) = env(ENV_MODE) {
            match LiveMode::parse(&raw) {
                Some(mode) => cfg.mode = mode,
                None => problems.push(format!(
                    "{ENV_MODE}={raw:?} is not one of off|loopback|mic; live audio stays off"
                )),
            }
        }
        if let Some(raw) = env(ENV_DEVICE) {
            if raw.is_empty() {
                problems.push(format!(
                    "{ENV_DEVICE} is set but empty; using no device filter"
                ));
            } else {
                cfg.device_id = Some(raw);
            }
        }
        if let Some(raw) = env(ENV_RING_MS) {
            match raw.parse::<u32>() {
                Ok(v @ 10..=5000) => cfg.ring_ms = v,
                _ => problems.push(format!(
                    "{ENV_RING_MS}={raw:?} is not an integer in 10..=5000"
                )),
            }
        }
        if let Some(raw) = env(ENV_INPUT_DB) {
            match parse_db_input(&raw) {
                Some(v) => cfg.input_db = v,
                None => problems.push(format!(
                    "{ENV_INPUT_DB}={raw:?} is not a dB value in -24..=24"
                )),
            }
        }
        (cfg, problems)
    }

    pub fn is_off(&self) -> bool {
        self.mode == LiveMode::Off
    }

    /// Ring capacity in samples, at the fastest rate we accept so the
    /// millisecond promise holds at every supported rate.
    pub fn ring_samples(&self) -> usize {
        (self.ring_ms.max(1) as usize) * SAMPLES_PER_MS_CEILING
    }

    pub fn capture_config(&self) -> FluxResult<CaptureConfig> {
        let tap = self
            .mode
            .tap()
            .ok_or_else(|| invalid("live audio is off: set FLUX_LIVE_AUDIO=loopback|mic"))?;
        Ok(CaptureConfig {
            tap,
            device_id: self.device_id.clone(),
            buffer_ms: self.buffer_ms,
            poll_ms: self.poll_ms,
            input_db: self.input_db,
        })
    }
}

fn env(key: &str) -> Option<String> {
    std::env::var(key)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
}

/// `+3`, `3`, `-6.5`, `+0.0`. A leading sign is allowed and means what it says.
/// One parser for the environment variable and the `--input-db` flag, so the
/// two cannot disagree about what `+3db` means.
pub fn parse_db_input(raw: &str) -> Option<f32> {
    let v: f32 = raw.trim().parse().ok()?;
    if v.is_finite() && (-24.0..=24.0).contains(&v) {
        Some(v)
    } else {
        None
    }
}

fn invalid(message: impl Into<String>) -> FluxError {
    FluxError::new(
        flux_domain::ErrorCode::InvalidCommand,
        "live-audio",
        message,
    )
}

/// What the capture thread reports back to `start` once the stream is open.
/// Plain data, so it crosses the thread boundary; `LiveCapture` itself never
/// does, because it is not `Send` and its COM pointers belong to one apartment.
#[derive(Debug, Clone)]
struct StreamInfo {
    format: WireFormat,
    endpoint_id: String,
    endpoint_name: String,
    period_default_hns: i64,
    period_min_hns: i64,
    buffer_hns: i64,
}

/// What one live reading consists of: the measurement, the accounting that says
/// whether it can be trusted, and the device facts that produced it.
#[derive(Debug, Clone, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveAudioSnapshot {
    // ---- state -------------------------------------------------------------
    /// A capture thread is alive.
    pub running: bool,
    /// Whether `live-audio` is in this build at all. When false, every other
    /// field is a placeholder and `error` says why.
    pub compiled: bool,
    pub tap: String,
    pub endpoint_id: String,
    pub endpoint_name: String,
    // ---- the wire ----------------------------------------------------------
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    pub format_tag: u16,
    pub float_wire: bool,
    /// The device offered `WAVE_FORMAT_EXTENSIBLE` (65534) rather than a bare
    /// tag. Reported, not folded away: the subtype GUID is what says whether
    /// the bytes are float or PCM, and a status that hid it would be a guess.
    pub extensible: bool,
    pub buffer_ms: u32,
    /// `GetDevicePeriod` answers in 100-nanosecond units; these are ms.
    pub period_default_ms: f64,
    pub period_min_ms: f64,
    pub ring_ms: u32,
    /// What the ring really holds at this sample rate.
    pub ring_ms_effective: f64,
    pub input_db: f32,
    // ---- measurement -------------------------------------------------------
    pub valid: bool,
    pub rms: f32,
    pub peak: f32,
    pub onset: bool,
    pub flux: f32,
    pub bands: [f32; 5],
    pub centroid_hz: f32,
    pub tempo_bpm: f32,
    pub tempo_confidence: f32,
    /// Position inside the detected beat grid, 0..1. The analyser's own value;
    /// `0.0` until a grid exists, which is why it is reported next to
    /// `tempoConfidence` rather than instead of it.
    pub beat_phase: f32,
    pub windows: u64,
    pub onsets: u64,
    // ---- accounting --------------------------------------------------------
    pub packets: u64,
    pub frames: u64,
    pub silent_frames: u64,
    /// Overrun *events*, not dropped frames: how many times the ring had to
    /// move the read cursor because the analyser fell behind.
    pub overflow_events: u64,
    /// Milliseconds since the device last handed over a packet.
    pub stalled_ms: u64,
    pub elapsed_ms: u64,
    pub error: Option<String>,
    /// Always `Live` for a slot produced here — the firewall's own label.
    pub feature_source: FeatureSource,
}

impl LiveAudioSnapshot {
    /// A slot for a build that cannot do this at all.
    pub fn not_compiled(reason: impl Into<String>) -> Self {
        Self {
            compiled: false,
            running: false,
            valid: false,
            error: Some(reason.into()),
            feature_source: FeatureSource::Live,
            ..Default::default()
        }
    }

    pub fn to_json(&self) -> serde_json::Value {
        serde_json::to_value(self)
            .unwrap_or_else(|_| serde_json::json!({"error": "live status did not serialize"}))
    }

    /// Frames per second over `span`, for the CLI report. `0.0` for a zero
    /// span, because guessing there would be a made-up number.
    pub fn measured_rate(&self, span: Duration) -> f64 {
        let secs = span.as_secs_f64();
        if secs > 0.0 {
            self.frames as f64 / secs
        } else {
            0.0
        }
    }

    /// The modulation-ready form of this reading. This is the only seam by
    /// which live features could reach `Router::resolve_node`, and it is a
    /// pure conversion: no device, no clock, no document.
    pub fn to_features(&self) -> FeatureSnapshot {
        FeatureSnapshot {
            rms: self.rms,
            peak: self.peak,
            bass: self.bands[0],
            low_mid: self.bands[1],
            mid: self.bands[2],
            high_mid: self.bands[3],
            treble: self.bands[4],
            centroid: self.centroid_hz,
            flux: self.flux,
            onset: if self.onset { 1.0 } else { 0.0 },
            beat_phase: self.beat_phase,
            tempo_bpm: self.tempo_bpm,
            valid: self.valid,
        }
    }
}

/// The counters and the one slot. Atomics because the capture thread must never
/// block on the analyser, and the analyser must never block on a device.
struct Shared {
    stop: AtomicBool,
    /// Cleared as soon as the capture thread returns, for any reason.
    capture_alive: AtomicBool,
    ring: Arc<PcmRing>,
    packets: AtomicU64,
    frames: AtomicU64,
    silent_frames: AtomicU64,
    /// Whole ms since the bridge started, at the last packet.
    last_packet_ms: AtomicU64,
    slot: Mutex<Option<LiveAudioSnapshot>>,
    error: Mutex<Option<String>>,
    /// When the sidecar was last rewritten, so publishing at hop rate does not
    /// mean writing a file at hop rate.
    last_status_write_ms: AtomicU64,
    /// Which record this run publishes itself into. Held as a value, not
    /// re-resolved per write, so a test never lands on another test's file.
    sidecar: Sidecar,
}

impl Shared {
    fn with_sidecar(ring_samples: usize, sidecar: Sidecar) -> Self {
        Self {
            stop: AtomicBool::new(false),
            capture_alive: AtomicBool::new(false),
            ring: Arc::new(PcmRing::new(ring_samples)),
            packets: AtomicU64::new(0),
            frames: AtomicU64::new(0),
            silent_frames: AtomicU64::new(0),
            last_packet_ms: AtomicU64::new(0),
            slot: Mutex::new(None),
            error: Mutex::new(None),
            last_status_write_ms: AtomicU64::new(0),
            sidecar,
        }
    }

    fn note_error(&self, message: impl Into<String>) {
        if let Ok(mut e) = self.error.lock() {
            *e = Some(message.into());
        }
    }

    fn error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|e| e.clone())
    }

    fn publish(&self, snapshot: LiveAudioSnapshot) {
        if let Ok(mut s) = self.slot.lock() {
            *s = Some(snapshot);
        }
    }

    fn snapshot(&self) -> Option<LiveAudioSnapshot> {
        self.slot.lock().ok().and_then(|s| s.clone())
    }

    fn add_drain(&self, drain: &Drain, now_ms: u64) {
        self.packets
            .fetch_add(u64::from(drain.packets), Ordering::Relaxed);
        self.frames
            .fetch_add(u64::from(drain.frames), Ordering::Relaxed);
        if drain.silent_frames > 0 {
            self.silent_frames
                .fetch_add(u64::from(drain.silent_frames), Ordering::Relaxed);
        }
        self.last_packet_ms.store(now_ms, Ordering::Release);
    }
}

fn ms_since(start: Instant) -> u64 {
    start.elapsed().as_millis() as u64
}

/// The running live audio path.
pub struct LiveAudioBridge {
    shared: Arc<Shared>,
    capture: Option<JoinHandle<()>>,
    analyser: Option<JoinHandle<()>>,
    config: LiveAudioConfig,
    started: Instant,
    info: StreamInfo,
}

impl LiveAudioBridge {
    /// Start the capture thread, wait for it to report an open stream, and only
    /// then start the analyser with the sample rate that stream actually has.
    /// A device that cannot be opened is an error from this call, exactly like
    /// `OscBridge`'s failed bind — never a bridge that runs and hears nothing.
    pub fn start(config: &LiveAudioConfig) -> FluxResult<Self> {
        Self::start_at(config, Sidecar::default())
    }

    /// `start`, with the record file supplied rather than resolved. Production
    /// callers never see this: a test that reaches the bridge has to publish into
    /// its own sidecar, or it is asserting against whatever another thread's run
    /// last wrote.
    fn start_at(config: &LiveAudioConfig, sidecar: Sidecar) -> FluxResult<Self> {
        let capture_config = config.capture_config()?;
        let shared = Arc::new(Shared::with_sidecar(config.ring_samples(), sidecar));
        let started = Instant::now();
        let (tx, rx) = mpsc::channel::<FluxResult<StreamInfo>>();

        // Claim the sidecar before the capture thread starts reading it. Without
        // this, a `stopRequested` left behind by `flux capture stop` at the end
        // of the previous run kills the first poll of this one, and the failure
        // looks like a device that will not stream.
        shared.sidecar.claim(config);

        let pump_shared = Arc::clone(&shared);
        let pump_config = capture_config.clone();
        let pump_started = started;
        let capture = std::thread::Builder::new()
            .name("flux-live-capture".into())
            .spawn(move || {
                capture_thread(&pump_shared, &pump_config, tx, pump_started);
            })
            .map_err(|e| io_error(e.to_string()))?;

        let info = match rx.recv() {
            Ok(Ok(info)) => info,
            Ok(Err(e)) => {
                let _ = capture.join();
                shared
                    .sidecar
                    .record_failed_start(config, &format!("{e:?}"));
                return Err(e);
            }
            Err(_) => {
                let _ = capture.join();
                let detail = shared.error().unwrap_or_else(|| {
                    "the capture thread ended before it reported a stream".to_string()
                });
                shared.sidecar.record_failed_start(config, &detail);
                return Err(invalid(detail));
            }
        };

        shared.publish(Self::first_snapshot(
            config,
            &info,
            flux_platform::capture::LIVE_AUDIO_COMPILED,
        ));
        shared.sidecar.write(&shared.snapshot().unwrap_or_default());

        let feed_shared = Arc::clone(&shared);
        let feed_rate = info.format.sample_rate;
        let feed_poll = config.poll_ms;
        let feed_started = started;
        let analyser = std::thread::Builder::new()
            .name("flux-live-analyser".into())
            .spawn(move || {
                analyser_thread(&feed_shared, feed_rate, feed_poll, feed_started);
            })
            .map_err(|e| io_error(e.to_string()))?;

        Ok(Self {
            shared,
            capture: Some(capture),
            analyser: Some(analyser),
            config: config.clone(),
            started,
            info,
        })
    }

    fn first_snapshot(
        config: &LiveAudioConfig,
        info: &StreamInfo,
        compiled: bool,
    ) -> LiveAudioSnapshot {
        let format = info.format;
        LiveAudioSnapshot {
            running: true,
            compiled,
            tap: config.mode.as_str().to_string(),
            endpoint_id: info.endpoint_id.clone(),
            endpoint_name: info.endpoint_name.clone(),
            sample_rate: format.sample_rate,
            channels: format.channels,
            bits_per_sample: format.bits_per_sample,
            format_tag: format.format_tag,
            float_wire: format.float,
            extensible: format.extensible,
            // What the stream really has, not what was asked: `open` raises the
            // requested buffer to the device period, and a report that said
            // otherwise would be a claim about a number we did not measure.
            buffer_ms: (info.buffer_hns / 10_000).max(1) as u32,
            period_default_ms: info.period_default_hns as f64 / 10_000.0,
            period_min_ms: info.period_min_hns as f64 / 10_000.0,
            ring_ms: config.ring_ms,
            ring_ms_effective: Shared::ring_ms_effective(config.ring_samples(), format.sample_rate),
            input_db: config.input_db,
            valid: false,
            feature_source: FeatureSource::Live,
            ..Default::default()
        }
    }

    pub fn snapshot(&self) -> Option<LiveAudioSnapshot> {
        self.shared.snapshot()
    }

    /// The current reading with the live counters folded in — what the CLI
    /// prints, what `live_audio_status` returns, and what the read-out shows.
    pub fn status(&self) -> LiveAudioSnapshot {
        let mut s = self.snapshot().unwrap_or_default();
        fold(&self.shared, &mut s, self.started);
        s
    }

    pub fn is_running(&self) -> bool {
        self.shared.capture_alive.load(Ordering::Acquire)
    }

    pub fn config(&self) -> &LiveAudioConfig {
        &self.config
    }

    /// The wire facts of the open stream, for a report that must not re-derive
    /// them from a format guess.
    pub fn stream(&self) -> (&str, &str, WireFormat, i64, i64) {
        (
            self.info.endpoint_id.as_str(),
            self.info.endpoint_name.as_str(),
            self.info.format,
            self.info.period_default_hns,
            self.info.period_min_hns,
        )
    }

    /// Ask both threads to finish, wait for them, and return the last reading.
    pub fn stop(mut self) -> LiveAudioSnapshot {
        self.request_stop();
        if let Some(h) = self.capture.take() {
            let _ = h.join();
        }
        if let Some(h) = self.analyser.take() {
            let _ = h.join();
        }
        let mut s = self.snapshot().unwrap_or_default();
        fold(&self.shared, &mut s, self.started);
        s.running = false;
        self.shared.sidecar.write(&s);
        s
    }

    fn request_stop(&self) {
        self.shared.stop.store(true, Ordering::Release);
    }
}

impl Drop for LiveAudioBridge {
    fn drop(&mut self) {
        // Flipping the flag is all `Drop` does, as in `OscBridge`: a destructor
        // must not join a thread it may itself be running on.
        self.request_stop();
    }
}

impl Shared {
    fn ring_ms_effective(ring_samples: usize, sample_rate: u32) -> f64 {
        // `PcmRing::new` rounds the capacity up to a power of two, so the real
        // buffering is reported from what it actually holds, not what we asked.
        let held = ring_samples.next_power_of_two();
        held as f64 * 1000.0 / f64::from(sample_rate.max(1))
    }
}

fn io_error(detail: String) -> FluxError {
    FluxError::new(flux_domain::ErrorCode::Io, "live-audio", detail)
}

/// The loopback idle watchdog's rule, in one place. `GetNextPacketSize` is zero
/// for as long as nothing is rendering, so a tap that has handed over no packet
/// for longer than `STALL_LIMIT_MS` is not measuring anything — it is reporting
/// the silence of a device nobody is playing, which looks identical to a quiet
/// take. Strictly greater: the limit is what we agree to tolerate, not what we
/// start distrusting.
fn is_stalled(stalled_ms: u64) -> bool {
    stalled_ms > STALL_LIMIT_MS
}

/// Apply the watchdog to a reading. `valid` goes false and `onset` freezes, so
/// nothing stale can fire a transition; the levels keep their last real values,
/// so a read-out can still tell "quiet" from "gone".
fn mark_stalled(snapshot: &mut LiveAudioSnapshot) {
    snapshot.valid = false;
    snapshot.onset = false;
}

/// Fold the live counters into a snapshot. One place, so `status`, `stop` and
/// the sidecar can never disagree about what the numbers mean.
fn fold(shared: &Arc<Shared>, s: &mut LiveAudioSnapshot, started: Instant) {
    s.packets = shared.packets.load(Ordering::Relaxed);
    s.frames = shared.frames.load(Ordering::Relaxed);
    s.silent_frames = shared.silent_frames.load(Ordering::Relaxed);
    s.overflow_events = shared.ring.dropped();
    s.running = shared.capture_alive.load(Ordering::Acquire);
    s.error = shared.error();
    s.elapsed_ms = ms_since(started);
    let last = shared.last_packet_ms.load(Ordering::Acquire);
    s.stalled_ms = s.elapsed_ms.saturating_sub(last);
    s.compiled = flux_platform::capture::LIVE_AUDIO_COMPILED;
}

/// The capture thread: open the device *here*, so the apartment, the stream and
/// the `LiveCapture` value all live and die on this one thread.
fn capture_thread(
    shared: &Arc<Shared>,
    config: &CaptureConfig,
    tx: mpsc::Sender<FluxResult<StreamInfo>>,
    started: Instant,
) {
    let mut capture = match LiveCapture::open(config) {
        Ok(c) => c,
        Err(e) => {
            shared.note_error(format!("{e:?}"));
            let _ = tx.send(Err(e));
            return;
        }
    };
    let endpoint_id = capture.endpoint_id().to_string();
    let endpoint_name = endpoint_name_for(&endpoint_id);
    let format = capture.format();
    let (period_default_hns, period_min_hns) = capture.periods_hns();
    let buffer_hns = capture.buffer_hns();
    if let Err(e) = capture.start() {
        shared.note_error(format!("start: {e:?}"));
        let _ = tx.send(Err(e));
        return;
    }
    shared.capture_alive.store(true, Ordering::Release);
    let _ = tx.send(Ok(StreamInfo {
        format,
        endpoint_id,
        endpoint_name,
        period_default_hns,
        period_min_hns,
        buffer_hns,
    }));

    let poll = Duration::from_millis(u64::from(config.poll_ms.max(1)));
    let mut tmp: Vec<f32> = Vec::with_capacity(8192);
    let mut last_check = Instant::now();
    loop {
        if shared.stop.load(Ordering::Acquire) || shared.sidecar.stop_requested(&mut last_check) {
            break;
        }
        tmp.clear();
        match capture.drain_into(&mut tmp) {
            Ok(drain) => {
                if drain.is_work() {
                    shared.add_drain(&drain, ms_since(started));
                    // Latest-wins: the ring holds the newest `ring_ms` and
                    // counts each time it had to skip forward.
                    shared.ring.write_mono(&tmp);
                }
            }
            Err(e) => {
                shared.note_error(format!("drain: {e:?}"));
                break;
            }
        }
        std::thread::sleep(poll);
    }
    // Stop and release here, on this thread, before it leaves its apartment.
    let _ = capture.stop();
    shared.capture_alive.store(false, Ordering::Release);
}

/// The analyser thread: consume the ring one hop at a time and publish exactly
/// one slot update per hop, plus a stale reading whenever nothing has arrived.
fn analyser_thread(shared: &Arc<Shared>, sample_rate: u32, poll_ms: u32, started: Instant) {
    // Configured from the stream, never from `AnalysisConfig::default()`'s
    // hard-coded 48 kHz.
    let cfg = AnalysisConfig {
        sample_rate,
        ..Default::default()
    };
    let hop = cfg.hop.max(1);
    let mut analyzer = Analyzer::new(cfg);
    let mut pending: Vec<f32> = Vec::with_capacity(hop * 16);
    let mut chunk: Vec<f32> = Vec::with_capacity(hop * 16);
    let poll = Duration::from_millis(u64::from(poll_ms.max(1)));

    loop {
        chunk.clear();
        if shared.ring.read_into(&mut chunk) > 0 {
            pending.extend_from_slice(&chunk);
        }
        while pending.len() >= hop {
            let frame: Vec<f32> = pending.drain(..hop).collect();
            analyzer.push(&frame);
            // Exactly one `features()` per hop: it recomputes on every call and
            // consumes the previous magnitude history, so a second call in the
            // same hop would publish a different, unrepeatable flux.
            let w = analyzer.features();
            let mut s = shared.snapshot().unwrap_or_default();
            s.valid = true;
            s.rms = w.rms;
            s.peak = w.peak;
            s.onset = w.onset;
            s.flux = w.flux;
            s.bands = w.bands;
            s.centroid_hz = w.centroid_hz;
            s.tempo_bpm = w.tempo_bpm;
            s.tempo_confidence = w.tempo_confidence;
            s.beat_phase = w.beat_phase;
            s.windows = analyzer.window_count();
            s.onsets = analyzer.onset_count();
            fold(shared, &mut s, started);
            shared.publish(s);
            throttle_status_write(shared, started);
        }
        let stalled =
            ms_since(started).saturating_sub(shared.last_packet_ms.load(Ordering::Acquire));
        if is_stalled(stalled) {
            // Nothing has arrived. That is not a measurement: mark the reading
            // stale and freeze onset, but keep the last real values so a
            // read-out can still tell "quiet" from "gone".
            let mut s = shared.snapshot().unwrap_or_default();
            mark_stalled(&mut s);
            fold(shared, &mut s, started);
            s.stalled_ms = stalled;
            shared.publish(s);
            throttle_status_write(shared, started);
        }
        if shared.stop.load(Ordering::Acquire) {
            let mut s = shared.snapshot().unwrap_or_default();
            fold(shared, &mut s, started);
            s.running = false;
            shared.publish(s);
            return;
        }
        std::thread::sleep(poll);
    }
}

/// Rewrite the sidecar at most every `STATUS_WRITE_INTERVAL_MS`.
fn throttle_status_write(shared: &Arc<Shared>, started: Instant) {
    let now = ms_since(started);
    let last = shared.last_status_write_ms.load(Ordering::Relaxed);
    if now.saturating_sub(last) >= STATUS_WRITE_INTERVAL_MS {
        shared.last_status_write_ms.store(now, Ordering::Relaxed);
        if let Some(s) = shared.snapshot() {
            shared.sidecar.write(&s);
        }
    }
}

/// Look up the name Windows shows for an endpoint id, so a status line reads
/// `Input (7- SSL 2 USB Audio Device)` rather than a GUID.
fn endpoint_name_for(id: &str) -> String {
    flux_platform::capture::list_endpoints()
        .ok()
        .and_then(|list| {
            list.into_iter().find(|e| e.id == id).map(|e| {
                if e.name.is_empty() {
                    id.to_string()
                } else {
                    e.name
                }
            })
        })
        .unwrap_or_else(|| id.to_string())
}

/// Where a running bridge records itself, so `flux capture status` and
/// `flux capture stop` — separate processes — can reach it. One machine, one
/// live capture at a time unless `FLUX_LIVE_AUDIO_STATUS` says otherwise: that is
/// the contract this file makes, and the CLI says so rather than pretending to
/// multiplex.
///
/// Resolved once per process, by whoever starts a run or issues a command. Every
/// other path takes the resulting [`Sidecar`] as a value — the module header's
/// "no process-global state" claim is only true if nothing re-reads the
/// environment mid-run.
pub fn status_file() -> PathBuf {
    match std::env::var_os(ENV_STATUS_PATH) {
        Some(raw) if !raw.is_empty() => PathBuf::from(raw),
        _ => std::env::temp_dir().join("flux-live-audio.json"),
    }
}

/// One sidecar path, held as a value.
///
/// This type exists because the suite used to have every test in this file write
/// and delete the *same* machine-global file, mid-assertion: `cargo test -p
/// flux-app --features live-audio` failed 4 of 58 with the default parallel
/// runner and passed only with `--test-threads=1` (measured at `9e113b4`, and the
/// failure set was not reproducible — one sibling's `remove_file` landed inside
/// another's `read`). Requiring a runner flag would have hidden the same defect
/// in production, where two `flux capture` runs collide exactly the same way; the
/// fix is that a run carries its own path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sidecar {
    path: PathBuf,
}

impl Default for Sidecar {
    fn default() -> Self {
        Self {
            path: status_file(),
        }
    }
}

impl Sidecar {
    /// A file no other test can be touching: named after the test thread, which
    /// under libtest is the test's own path, and removed before and after.
    /// Production code must not call this — it is `#[cfg(test)]`-only so an
    /// isolated path can never be mistaken for the rendezvous.
    #[cfg(test)]
    fn for_test() -> Self {
        use std::sync::atomic::AtomicUsize;
        static ANON: AtomicUsize = AtomicUsize::new(0);
        let name = std::thread::current()
            .name()
            .map(|n| n.replace("::", "-"))
            .unwrap_or_else(|| format!("anon-{}", ANON.fetch_add(1, Ordering::Relaxed)));
        let path = std::env::temp_dir().join(format!(
            "flux-live-audio-test-{}-{}.json",
            name,
            std::process::id()
        ));
        let sidecar = Self { path };
        let _ = std::fs::remove_file(&sidecar.path);
        sidecar
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Mark the sidecar as owned by this process, and as not wanting a stop,
    /// before any thread of this run reads it. `flux capture stop` leaves
    /// `stopRequested` set; a new run that did not clear it first would be killed
    /// by the last frame of the previous one. The wire facts are not known yet, so
    /// the claimed record says `running: true, valid: false`: a capture that is
    /// starting, not a capture that has measured.
    fn claim(&self, config: &LiveAudioConfig) {
        // `false`, not whatever the last run left: see the doc comment above.
        self.write_with(
            &LiveAudioSnapshot {
                running: true,
                compiled: LIVE_AUDIO_COMPILED,
                tap: config.mode.as_str().to_string(),
                ring_ms: config.ring_ms,
                input_db: config.input_db,
                feature_source: FeatureSource::Live,
                ..Default::default()
            },
            false,
        );
    }

    /// Record a start that produced no stream, so a status read after a failed
    /// `start` says what failed instead of keeping the `running: true` written by
    /// the claim that preceded it.
    fn record_failed_start(&self, config: &LiveAudioConfig, detail: &str) {
        self.write_with(
            &LiveAudioSnapshot {
                running: false,
                compiled: LIVE_AUDIO_COMPILED,
                tap: config.mode.as_str().to_string(),
                ring_ms: config.ring_ms,
                input_db: config.input_db,
                valid: false,
                error: Some(detail.to_string()),
                feature_source: FeatureSource::Live,
                ..Default::default()
            },
            false,
        );
    }

    /// Is a stop pending for whichever run owns this sidecar right now?
    fn pending_stop_request(&self) -> bool {
        self.read()
            .and_then(|v| v.get("stopRequested").and_then(|b| b.as_bool()))
            .unwrap_or(false)
    }

    /// Publish a reading. A stop request that arrived while this run was measuring
    /// is carried forward instead of being rewritten to `false`: the status writer
    /// runs every `STATUS_WRITE_INTERVAL_MS` and the reader every
    /// `STOP_CHECK_INTERVAL_MS`, so a plain reset put the control channel back into
    /// a race — measured here, where the capture thread saw the request and ended
    /// the stream while the CLI's own wait ignored it.
    fn write(&self, snapshot: &LiveAudioSnapshot) {
        let stop = self.pending_stop_request();
        self.write_with(snapshot, stop);
    }

    fn write_with(&self, snapshot: &LiveAudioSnapshot, stop_requested: bool) {
        // The rate is written here rather than recomputed by whoever reads the
        // file, so `flux capture status` reports what the bridge measured over the
        // span it actually ran.
        let mut status = snapshot.to_json();
        if let Some(obj) = status.as_object_mut() {
            let span = Duration::from_millis(snapshot.elapsed_ms);
            obj.insert(
                "measuredFramesPerSecond".into(),
                serde_json::json!(snapshot.measured_rate(span)),
            );
        }
        if let Ok(text) = serde_json::to_string_pretty(&serde_json::json!({
            "pid": std::process::id(),
            "stopRequested": stop_requested,
            "status": status,
        })) {
            // Best effort: a status file that will not write must not take a live
            // performance down with it. The error is visible on the console path.
            let _ = std::fs::write(&self.path, text);
        }
    }

    /// Read the sidecar, if any. `None` means no live capture has reported itself.
    fn read(&self) -> Option<serde_json::Value> {
        let text = std::fs::read_to_string(&self.path).ok()?;
        serde_json::from_str(&text).ok()
    }

    /// Has another process asked the running capture to stop? Checked on an
    /// interval, because it is a control channel and not a sample path.
    fn stop_requested(&self, last_check: &mut Instant) -> bool {
        if last_check.elapsed() < Duration::from_millis(STOP_CHECK_INTERVAL_MS) {
            return false;
        }
        *last_check = Instant::now();
        self.read()
            .and_then(|v| v.get("stopRequested")?.as_bool())
            .unwrap_or(false)
    }

    /// `flux capture stop`: mark the sidecar so the running bridge exits. Returns
    /// false when no bridge has ever written one.
    fn request_external_stop(&self) -> bool {
        match self.read() {
            Some(mut v) => {
                if let Some(obj) = v.as_object_mut() {
                    obj.insert("stopRequested".into(), serde_json::json!(true));
                }
                let text = v.to_string();
                std::fs::write(&self.path, text).is_ok()
            }
            None => false,
        }
    }

    /// Sleep `budget`, but come back early when another process has asked for a
    /// stop. `--secs` is a ceiling, not a promise to keep a stopped capture alive:
    /// measured here, `flux capture stop` ended the stream at 4.9 s of a
    /// `capture start --secs 20` run and the CLI still slept to 20.1 s, publishing
    /// `stalledMs=15212` for a take that had already been stopped.
    ///
    /// Returns the span actually waited, so the rate in the report is computed over
    /// the run that happened rather than the run that was asked for.
    fn wait_or_stop(&self, budget: Duration) -> Duration {
        let started = Instant::now();
        let mut last_check = started;
        let step = Duration::from_millis(STOP_CHECK_INTERVAL_MS.max(1));
        while started.elapsed() < budget {
            if self.stop_requested(&mut last_check) {
                break;
            }
            let left = budget.saturating_sub(started.elapsed());
            std::thread::sleep(left.min(step));
        }
        started.elapsed()
    }
}

/// Free-function view of [`Sidecar::read`], for callers that mean the machine's
/// rendezvous file and have no run of their own to carry a path.
pub fn read_status_file() -> Option<serde_json::Value> {
    Sidecar::default().read()
}

/// Free-function view of [`Sidecar::request_external_stop`].
pub fn request_external_stop() -> bool {
    Sidecar::default().request_external_stop()
}

/// A short measurement run, for the CLI: start, wait at most `secs`, stop, and
/// hand back the final reading plus the span it covers.
pub fn measure(config: &LiveAudioConfig, secs: u32) -> FluxResult<(LiveAudioSnapshot, Duration)> {
    // Resolved once, so the run publishes into and the CLI waits on the same
    // record — a second `status_file()` call could not disagree today, but it
    // should not be able to.
    let sidecar = Sidecar::default();
    let bridge = LiveAudioBridge::start_at(config, sidecar.clone())?;
    let span = sidecar.wait_or_stop(Duration::from_secs(u64::from(secs.max(1))));
    let snapshot = bridge.stop();
    Ok((snapshot, span))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mode_defaults_to_off_and_parses_only_the_three_names() {
        assert_eq!(LiveMode::parse("off"), Some(LiveMode::Off));
        assert_eq!(LiveMode::parse(""), Some(LiveMode::Off));
        assert_eq!(LiveMode::parse("LOOPBACK"), Some(LiveMode::Loopback));
        assert_eq!(LiveMode::parse(" mic "), Some(LiveMode::Mic));
        assert_eq!(LiveMode::parse("auto"), None);
        assert_eq!(LiveMode::Off.tap(), None);
        assert_eq!(LiveMode::Loopback.tap(), Some(Tap::Loopback));
        assert_eq!(LiveMode::Mic.tap(), Some(Tap::Microphone));
        assert_eq!(DEFAULT_MODE, LiveMode::Off);
    }

    #[test]
    fn an_off_config_refuses_to_open_a_device() {
        let cfg = LiveAudioConfig::default();
        assert!(cfg.is_off());
        let Err(e) = cfg.capture_config() else {
            panic!("off mode must not produce a CaptureConfig");
        };
        assert_eq!(e.code, flux_domain::ErrorCode::InvalidCommand);
    }

    #[test]
    fn input_trim_is_parsed_explicitly_and_never_auto_adjusted() {
        assert_eq!(parse_db_input("+3"), Some(3.0));
        assert_eq!(parse_db_input("+0.0"), Some(0.0));
        assert_eq!(parse_db_input("-6.5"), Some(-6.5));
        assert_eq!(parse_db_input("+3db"), None);
        assert_eq!(parse_db_input("30"), None, "beyond +-24 dB is not a trim");
        let mut cfg = LiveAudioConfig::default();
        cfg.mode = LiveMode::Mic;
        cfg.input_db = -6.0;
        let cc = cfg.capture_config().expect("config");
        assert_eq!(cc.input_db, -6.0);
        assert_eq!(cc.buffer_ms, DEFAULT_BUFFER_MS);
        assert_eq!(cc.tap, Tap::Microphone);
    }

    #[test]
    fn ring_sizing_holds_the_requested_milliseconds_at_any_rate() {
        let cfg = LiveAudioConfig {
            ring_ms: 100,
            ..Default::default()
        };
        let samples = cfg.ring_samples();
        assert!(
            samples >= 100 * 44,
            "100 ms at 44.1 kHz needs 4410, got {samples}"
        );
        assert!(samples >= 100 * SAMPLES_PER_MS_CEILING);
        // `PcmRing::new` rounds up, so the effective buffering is reported from
        // what it really holds at the observed rate.
        let at_48k = Shared::ring_ms_effective(samples, 48_000);
        assert!(
            at_48k >= 100.0,
            "100 ms of ring promised, {at_48k} ms delivered"
        );
        let at_192k = Shared::ring_ms_effective(samples, 192_000);
        assert!(at_192k >= 100.0, "{at_192k} ms at 192 kHz");
    }

    #[test]
    fn a_stalled_reading_is_marked_invalid_and_freezes_onset() {
        // The watchdog's whole point: silence with no packets is not a
        // measurement. The rule itself lives in `is_stalled` / `mark_stalled`,
        // which is what the analyser calls, so this exercises production
        // behaviour rather than a copy of it written inside the test.
        //
        // At the boundary: `STALL_LIMIT_MS` is what we agree to tolerate, so
        // exactly the limit is still believed and one millisecond past it is
        // not.
        assert!(!is_stalled(0));
        assert!(!is_stalled(STALL_LIMIT_MS));
        assert!(is_stalled(STALL_LIMIT_MS + 1));

        let shared = Arc::new(Shared::with_sidecar(
            100 * SAMPLES_PER_MS_CEILING,
            Sidecar::for_test(),
        ));
        shared.publish(LiveAudioSnapshot {
            running: true,
            valid: true,
            onset: true,
            rms: 0.25,
            peak: 0.5,
            feature_source: FeatureSource::Live,
            ..Default::default()
        });
        // `last_packet_ms` stays 0 and the bridge "started" 150 ms ago, so the
        // stall is over the limit.
        let started = Instant::now() - Duration::from_millis(150);
        let mut s = shared.snapshot().unwrap();
        fold(&shared, &mut s, started);
        assert!(is_stalled(s.stalled_ms), "stalled_ms = {}", s.stalled_ms);
        mark_stalled(&mut s);
        assert!(!s.valid, "a stalled reading must not claim it measured");
        assert!(!s.onset, "and must not fire a transition off stale data");
        // The values that were measured before the stall survive, so the
        // read-out can tell quiet from gone.
        assert_eq!(s.rms, 0.25);
        assert_eq!(s.peak, 0.5);
    }

    #[test]
    fn overflow_is_reported_as_events_not_frames() {
        // `PcmRing::dropped()` counts the number of times the ring had to skip,
        // not the number of frames skipped: `ring_ms` is a buffering promise,
        // and this is the number that says whether it was kept.
        let ring = PcmRing::new(16);
        assert_eq!(ring.dropped(), 0);
        ring.write_mono(&[0.1; 40]);
        assert_eq!(ring.dropped(), 1, "one overflow event for one overrun");
        ring.write_mono(&[0.2; 40]);
        assert_eq!(ring.dropped(), 2);
        let shared = Arc::new(Shared::with_sidecar(16, Sidecar::for_test()));
        // The overflow has to happen in the ring `fold` actually reads, or the
        // test proves nothing about the status slot.
        shared.ring.write_mono(&[0.1; 40]);
        shared.ring.write_mono(&[0.2; 40]);
        let mut s = LiveAudioSnapshot::default();
        fold(&shared, &mut s, Instant::now());
        assert_eq!(
            s.overflow_events, 2,
            "folded from the ring, not from frames"
        );
        assert_eq!(s.frames, 0, "an overflow event is not a captured frame");
        assert_eq!(s.packets, 0, "and not a packet either");
    }

    #[test]
    fn to_features_is_a_pure_conversion_of_the_slot() {
        let s = LiveAudioSnapshot {
            valid: true,
            rms: 0.2,
            peak: 0.4,
            onset: true,
            bands: [0.1, 0.2, 0.3, 0.4, 0.5],
            centroid_hz: 1234.0,
            tempo_bpm: 128.0,
            ..Default::default()
        };
        let f = s.to_features();
        assert!(f.valid);
        assert_eq!(f.onset, 1.0);
        assert_eq!(f.bass, 0.1);
        assert_eq!(f.treble, 0.5);
        assert_eq!(f.tempo_bpm, 128.0);
        assert_eq!(f.centroid, 1234.0);
        // A stale reading must not reach the fold as a measurement.
        let stale = LiveAudioSnapshot {
            valid: false,
            rms: 0.2,
            ..Default::default()
        };
        assert!(!stale.to_features().valid);
    }

    #[test]
    fn not_compiled_says_so_instead_of_reporting_silence() {
        let s = LiveAudioSnapshot::not_compiled("test reason");
        assert!(!s.compiled);
        assert!(!s.valid);
        assert!(!s.running);
        assert_eq!(s.error.as_deref(), Some("test reason"));
        assert_eq!(s.feature_source, FeatureSource::Live);
        assert!(!s.feature_source.is_deterministic());
    }

    #[test]
    fn status_json_uses_the_names_the_readout_asks_for() {
        let s = LiveAudioSnapshot {
            compiled: flux_platform::capture::LIVE_AUDIO_COMPILED,
            running: false,
            tap: "loopback".into(),
            sample_rate: 48_000,
            peak: 0.5,
            feature_source: FeatureSource::Live,
            ..Default::default()
        };
        let j = s.to_json();
        for key in [
            "sampleRate",
            "bitsPerSample",
            "extensible",
            "beatPhase",
            "peak",
            "stalledMs",
            "overflowEvents",
            "featureSource",
            "ringMsEffective",
            "periodDefaultMs",
            "inputDb",
            "endpointName",
        ] {
            assert!(j.get(key).is_some(), "status JSON is missing {key}: {j}");
        }
        assert_eq!(j["sampleRate"], serde_json::json!(48_000));
        // The names the read-out asks for are camelCase, not the Rust field
        // spellings: a `float_wire` or `beat_phase` that reaches the webview is
        // a contract break, not a style choice.
        assert!(j.get("floatWire").is_some(), "snake_case leaked: {j}");
        assert!(j.get("beat_phase").is_none(), "snake_case leaked: {j}");
        assert!(j.get("extensible_wire").is_none(), "snake_case leaked: {j}");
    }

    #[test]
    fn measured_rate_is_frames_over_the_span_and_zero_is_not_guessed() {
        let mut s = LiveAudioSnapshot::default();
        s.frames = 4800;
        assert!((s.measured_rate(Duration::from_secs(2)) - 2400.0).abs() < 1e-9);
        assert_eq!(s.measured_rate(Duration::ZERO), 0.0);
    }

    /// A made-up endpoint id fails out of `start` with the device-fault code,
    /// before `Activate` and before any stream exists — so a bridge can never
    /// come up deaf. Needs the real implementation to be compiled in; the
    /// stub's answer for the same request is the next test.
    #[cfg(feature = "live-audio")]
    #[test]
    fn an_unknown_endpoint_fails_from_start_rather_than_running_deaf() {
        let sidecar = Sidecar::for_test();
        let cfg = LiveAudioConfig {
            mode: LiveMode::Mic,
            device_id: Some("no-such-endpoint".into()),
            ..Default::default()
        };
        let Err(e) = LiveAudioBridge::start_at(&cfg, sidecar.clone()) else {
            panic!("a made-up endpoint must not start a bridge");
        };
        assert_eq!(e.code, flux_domain::ErrorCode::AudioDeviceLost);
        // The record of that start must not claim a capture is running. Asserted
        // here, against a file only this test can see, in the build that has the
        // real device layer — not only in the stub build below.
        let v = sidecar.read().expect("the failed start recorded itself");
        assert_eq!(v["status"]["running"].as_bool(), Some(false));
        assert_eq!(v["status"]["valid"].as_bool(), Some(false));
        let _ = std::fs::remove_file(sidecar.path());
    }

    /// The same request against a build that has no capture apparatus answers
    /// `SdkMissing`, in the default build, from the same call. The two paths
    /// share one shape on purpose: `start` never returns a running bridge for a
    /// device it did not open, whichever reason it is.
    #[cfg(not(feature = "live-audio"))]
    #[test]
    fn a_start_without_the_apparatus_names_the_feature_instead_of_running_deaf() {
        assert!(!LIVE_AUDIO_COMPILED);
        let sidecar = Sidecar::for_test();
        let cfg = LiveAudioConfig {
            mode: LiveMode::Loopback,
            ..Default::default()
        };
        let Err(e) = LiveAudioBridge::start_at(&cfg, sidecar.clone()) else {
            panic!("a build with no capture must not report a running bridge");
        };
        assert_eq!(e.code, flux_domain::ErrorCode::SdkMissing);
        assert!(
            e.message.contains("live-audio"),
            "the message has to name the feature: {e:?}"
        );
        // And the sidecar must say the same thing, not keep the `running: true`
        // the claim wrote a moment earlier.
        let v = sidecar.read().expect("the failed start recorded itself");
        assert_eq!(v["status"]["running"].as_bool(), Some(false));
        assert_eq!(v["status"]["valid"].as_bool(), Some(false));
        assert!(v["status"]["error"]
            .as_str()
            .unwrap_or_default()
            .contains("live-audio"));
        let _ = std::fs::remove_file(sidecar.path());
    }

    #[test]
    fn a_stop_request_lands_in_the_sidecar_a_running_bridge_reads() {
        // One file per test, named after this test. It used to be `status_file()`
        // — the machine-global rendezvous — which every test in this file wrote
        // and deleted underneath the others: the default parallel invocation
        // failed here and in three siblings, and only `--test-threads=1` hid it.
        let sidecar = Sidecar::for_test();
        // Write a status as if a bridge were alive, then ask it to stop. This
        // is the whole cross-process control channel: `start` publishes the
        // same shape from a real device, and `stop` only flips one field.
        let snapshot = LiveAudioSnapshot {
            running: true,
            compiled: flux_platform::capture::LIVE_AUDIO_COMPILED,
            tap: "loopback".into(),
            sample_rate: 48_000,
            feature_source: FeatureSource::Live,
            ..Default::default()
        };
        sidecar.write(&snapshot);
        let before = sidecar.read().expect("the sidecar the bridge would write");
        assert_eq!(before["pid"].as_u64(), Some(u64::from(std::process::id())));
        assert_eq!(
            before["stopRequested"].as_bool(),
            Some(false),
            "a fresh status must not already mean stop"
        );
        assert_eq!(before["status"]["sampleRate"].as_u64(), Some(48_000));
        assert!(
            sidecar.request_external_stop(),
            "the sidecar exists, so the stop request must have landed"
        );
        let after = sidecar.read().expect("still there after stop");
        assert_eq!(after["stopRequested"].as_bool(), Some(true));
        assert_eq!(
            after["status"]["sampleRate"].as_u64(),
            Some(48_000),
            "stopping must not discard the last measurement"
        );
        let _ = std::fs::remove_file(sidecar.path());
    }

    // Every test below reaches a sidecar through a `Sidecar` value it created
    // itself. The production path stays machine-global (`status_file()`), because
    // that is how `flux capture status` and `flux capture stop` reach a run from a
    // different process; what is no longer shared is the test suite.

    /// A pending stop is an instruction to this process, not a field of the
    /// reading. Republishing it as `false` is how `flux capture stop` is lost:
    /// the status writer runs every 250 ms and the stop reader every 100 ms, so
    /// the request sat in a race against its own process.
    #[test]
    fn a_status_write_carries_a_pending_stop_request_forward() {
        let sidecar = Sidecar::for_test();
        let snapshot = LiveAudioSnapshot {
            running: true,
            compiled: flux_platform::capture::LIVE_AUDIO_COMPILED,
            tap: "loopback".into(),
            sample_rate: 48_000,
            feature_source: FeatureSource::Live,
            ..Default::default()
        };
        sidecar.write(&snapshot);
        assert_eq!(
            sidecar.read().expect("sidecar")["stopRequested"].as_bool(),
            Some(false),
            "a reading published before any stop must not mean stop"
        );
        assert!(
            sidecar.request_external_stop(),
            "the stop request must land"
        );
        // The same run publishes the next frame 250 ms later.
        sidecar.write(&LiveAudioSnapshot {
            elapsed_ms: 250,
            ..snapshot
        });
        assert_eq!(
            sidecar.read().expect("sidecar after a publish")["stopRequested"].as_bool(),
            Some(true),
            "a status write erased a stop request that had already landed"
        );
        assert_eq!(
            sidecar.read().expect("sidecar")["status"]["elapsedMs"].as_u64(),
            Some(250),
            "carrying the stop forward must not discard the new reading"
        );
        // Only a new run's claim resets it.
        sidecar.claim(&LiveAudioConfig {
            mode: LiveMode::Loopback,
            ..Default::default()
        });
        assert_eq!(
            sidecar.read().expect("claimed sidecar")["stopRequested"].as_bool(),
            Some(false),
            "the previous run's stop must not be inherited"
        );
        let _ = std::fs::remove_file(sidecar.path());
    }

    /// `--secs` is a ceiling. Measured against a device, an unconditional sleep
    /// kept a `capture start --secs 20` process running for 20.1 s after
    /// `capture stop` had already ended its stream at 4.9 s.
    #[test]
    fn an_external_stop_ends_the_cli_wait_before_its_deadline() {
        let sidecar = Sidecar::for_test();
        sidecar.write(&LiveAudioSnapshot {
            running: true,
            tap: "mic".into(),
            sample_rate: 48_000,
            feature_source: FeatureSource::Live,
            ..Default::default()
        });

        // Nobody asks, so the whole budget is spent: the ceiling is not an
        // estimate the CLI is free to shorten.
        let budget = Duration::from_millis(200);
        let waited = sidecar.wait_or_stop(budget);
        assert!(
            waited >= budget,
            "a stop-free wait came back early after {waited:?}"
        );

        // Somebody asks 60 ms in, so 10 s must not become the runtime. The
        // stopper is a second *thread*, which is exactly why the sidecar has to
        // be reachable by path: it takes the same value the wait is polling.
        let stopper_sidecar = sidecar.clone();
        let stopper = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(60));
            stopper_sidecar.request_external_stop()
        });
        let waited = sidecar.wait_or_stop(Duration::from_secs(10));
        let landed = stopper.join().unwrap_or(false);
        let _ = std::fs::remove_file(sidecar.path());
        assert!(
            landed,
            "there was a sidecar, so the request must have landed"
        );
        assert!(
            waited < Duration::from_secs(3),
            "the wait ignored the stop request and slept {waited:?}"
        );
    }

    /// `flux capture stop` leaves `stopRequested` set. A run that started after
    /// it must clear that before its capture thread ever reads the file, or the
    /// new take dies on the first poll and the fault looks like the device.
    #[test]
    fn a_new_run_claims_the_sidecar_before_any_thread_reads_it() {
        let sidecar = Sidecar::for_test();
        let cfg = LiveAudioConfig {
            mode: LiveMode::Loopback,
            ..Default::default()
        };
        let first = LiveAudioSnapshot {
            running: true,
            tap: "loopback".into(),
            sample_rate: 48_000,
            ..Default::default()
        };
        sidecar.write(&first);
        assert!(
            sidecar.request_external_stop(),
            "the previous run asked to stop"
        );
        assert_eq!(
            sidecar.read().expect("sidecar")["stopRequested"].as_bool(),
            Some(true)
        );

        sidecar.claim(&cfg);
        let claimed = sidecar.read().expect("the claimed sidecar");
        assert_eq!(
            claimed["stopRequested"].as_bool(),
            Some(false),
            "a stale stop request must not be able to kill the new run"
        );
        assert_eq!(
            claimed["status"]["running"].as_bool(),
            Some(true),
            "a capture that is starting is a capture that is running"
        );
        assert_eq!(
            claimed["status"]["valid"].as_bool(),
            Some(false),
            "but it has not measured anything yet"
        );
        assert_eq!(claimed["status"]["tap"].as_str(), Some("loopback"));
        let _ = std::fs::remove_file(sidecar.path());
    }

    /// A start that never got a stream must not leave the claimed
    /// `running: true` behind: the next `flux capture status` would report a
    /// capture that does not exist.
    #[test]
    fn a_failed_start_replaces_the_claim_with_the_failure() {
        let sidecar = Sidecar::for_test();
        let cfg = LiveAudioConfig {
            mode: LiveMode::Mic,
            device_id: Some("no-such-endpoint".into()),
            input_db: -3.0,
            ..Default::default()
        };
        sidecar.record_failed_start(&cfg, "AudioDeviceLost: the endpoint id was not found");
        let v = sidecar.read().expect("the failure was recorded");
        assert_eq!(v["status"]["running"].as_bool(), Some(false));
        assert_eq!(v["status"]["valid"].as_bool(), Some(false));
        assert_eq!(
            v["status"]["error"].as_str(),
            Some("AudioDeviceLost: the endpoint id was not found")
        );
        assert_eq!(v["status"]["tap"].as_str(), Some("mic"));
        assert_eq!(
            v["status"]["inputDb"].as_f64(),
            Some(-3.0),
            "the trim the run asked for is part of the record"
        );
        let _ = std::fs::remove_file(sidecar.path());
    }

    /// Every test that reaches the sidecar must reach *its own* copy of it. This
    /// is the guard against the defect the fixture fix was for: if a test goes
    /// back to `status_file()`, or `Sidecar::for_test` ever returns the default
    /// path again, this reddens — including under a parallel runner, where the
    /// two files would otherwise be the same file.
    #[test]
    fn a_test_sidecar_is_never_the_machines_rendezvous() {
        let mine = Sidecar::for_test();
        let default = Sidecar::default();
        assert_ne!(
            mine.path(),
            default.path(),
            "a test published into the machine's live-capture record"
        );
        assert!(
            mine.path()
                .file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("flux-live-audio-test-")),
            "a test sidecar must be identifiable on disk: {:?}",
            mine.path()
        );
        // And two different tests must not land on the same path. Called from
        // this thread, `for_test` is keyed on the test name, so a second call
        // from here is the same test — what it must still never be is the
        // rendezvous, which the assertion above already covers. What is checked
        // here is that the path survives a clone, because a bridge thread is
        // handed the same value and a clone that lost the path would silently
        // fall back to the default file.
        assert_eq!(mine.clone().path(), mine.path());
        let _ = std::fs::remove_file(mine.path());
    }
}
