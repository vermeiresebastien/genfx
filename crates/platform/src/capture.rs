//! Live audio capture: WASAPI shared-mode endpoints.
//!
//! The real implementation lives in [`imp`] and is compiled only for
//! `cfg(all(windows, feature = "live-audio"))`. The same public surface is kept
//! for every other build, where each operation fails with `SdkMissing`: that way
//! `crates/app` can name these types from an always-compiled module instead of
//! scattering `#[cfg]` through the GUI and command plumbing.
//!
//! What this module deliberately does *not* do:
//!   * no `wgpu`, no `Project`, no `RationalTime` — the live path is CPU-only
//!     and never reaches into the deterministic export machinery;
//!   * no auto-gain. [`CaptureConfig::input_db`] is the owner's explicit trim
//!     and defaults to `+0.0`. Measured reason: a `volume=0.2` tone arrived at
//!     `peak = 0.0083`; compensating for that silently would make the level look
//!     like an engine property rather than a user setting.
//!   * no silent fallback on an unknown wire format. An unsupported
//!     `WAVEFORMATEX` is an error, never a buffer of zeros — zeros would read as
//!     "the device is present and quiet".

use serde::{Deserialize, Serialize};

/// Which endpoint stream to tap.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tap {
    /// The render endpoint's own mix (what the speakers are playing), tapped
    /// with `AUDCLNT_STREAMFLAGS_LOOPBACK`.
    Loopback,
    /// A capture endpoint (microphone / line in).
    Microphone,
}

impl Tap {
    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "loopback" | "render" | "mix" => Some(Self::Loopback),
            "mic" | "microphone" | "capture" => Some(Self::Microphone),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Loopback => "loopback",
            Self::Microphone => "mic",
        }
    }

    /// The data flow this tap reads from. Loopback is *not* a capture endpoint:
    /// it is a render endpoint opened with the loopback stream flag, and the two
    /// have different default devices.
    pub fn flow_is_render(&self) -> bool {
        matches!(self, Self::Loopback)
    }
}

/// One enumerated audio endpoint, named the way Windows names it.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EndpointInfo {
    /// The WASAPI endpoint id, verbatim. This is what gets persisted: it is
    /// stable across sessions and selects the device exactly, where a friendly
    /// name can collide or change.
    pub id: String,
    /// `PKEY_Device_FriendlyName` — what the Sound control panel shows — with
    /// the endpoint id as the fallback when the property store has no name.
    pub name: String,
    pub tap: Tap,
    /// Default for this tap's data flow in the `eConsole` role.
    pub is_default: bool,
}

/// The wire format the endpoint actually handed us, decoded and validated.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct WireFormat {
    pub sample_rate: u32,
    pub channels: u16,
    pub bits_per_sample: u16,
    /// `true` for 32-bit float payloads, `false` for 16-bit integer PCM.
    pub float: bool,
    pub block_align: u16,
    /// The `wFormatTag` that was accepted, kept so a report can say *why* a
    /// device was opened the way it was.
    pub format_tag: u16,
    /// Set when `wFormatTag` was 65534 and the sub-format GUID decided the
    /// payload class, rather than the tag itself.
    pub extensible: bool,
}

impl WireFormat {
    /// Bytes per frame (all channels), as the device reported it.
    pub fn frame_bytes(&self) -> usize {
        let align = self.block_align as usize;
        if align > 0 {
            align
        } else {
            (self.channels as usize * (self.bits_per_sample as usize / 8)).max(1)
        }
    }

    /// Bytes per sample of one channel.
    pub fn sample_bytes(&self) -> usize {
        (self.bits_per_sample as usize / 8).max(1)
    }
}

/// How to open a stream. Every field is an explicit user choice; nothing here
/// is inferred and nothing is auto-trimmed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CaptureConfig {
    pub tap: Tap,
    /// Exact WASAPI endpoint id. `None` means the `eConsole` default for this
    /// tap's data flow — which on a rig with several devices is a decision the
    /// owner should make on purpose, not by accident.
    pub device_id: Option<String>,
    /// Requested stream buffer duration, in milliseconds.
    pub buffer_ms: u32,
    /// Poll interval while running; kept at or below the device period.
    pub poll_ms: u32,
    /// Explicit input trim in dBFS, applied linearly to the samples returned.
    /// Default `+0.0`.
    pub input_db: f32,
}

impl Default for CaptureConfig {
    fn default() -> Self {
        Self {
            tap: Tap::Microphone,
            device_id: None,
            buffer_ms: 10,
            poll_ms: 3,
            input_db: 0.0,
        }
    }
}

/// What one drain call took off the device.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct Drain {
    /// Packets pulled, i.e. `GetNextPacketSize` iterations that returned work.
    pub packets: u32,
    /// Audio frames appended to the caller's buffer.
    pub frames: u32,
    /// Frames inside a packet WASAPI flagged silent.
    pub silent_frames: u32,
    /// The device told us it dropped audio mid-stream.
    pub discontinuity: bool,
}

impl Drain {
    pub fn empty() -> Self {
        Self::default()
    }

    pub fn is_work(&self) -> bool {
        self.packets > 0
    }
}

/// Is the live capture path compiled into this binary? The CLI and the status
/// read-out report this rather than pretending an empty device list is a
/// failure (D036: the apparatus, not the outcome).
pub const LIVE_AUDIO_COMPILED: bool = cfg!(all(windows, feature = "live-audio"));

/// A poll interval in milliseconds, clamped to something a device period can
/// plausibly support.
pub fn poll_ms_suggest(poll_ms: u32) -> u32 {
    poll_ms.clamp(1, 50)
}

#[cfg(all(windows, feature = "live-audio"))]
mod imp {
    use super::{CaptureConfig, Drain, EndpointInfo, Tap, WireFormat};
    use flux_domain::{ErrorCode, FluxError, FluxResult};
    use std::mem::ManuallyDrop;
    use std::ptr::{addr_of, read_unaligned};
    use windows::core::GUID;
    use windows::Win32::Devices::FunctionDiscovery::PKEY_Device_FriendlyName;
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, eRender, EDataFlow, IAudioCaptureClient, IAudioClient, IMMDevice,
        IMMDeviceEnumerator, MMDeviceEnumerator, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK, DEVICE_STATE_ACTIVE, WAVEFORMATEX, WAVEFORMATEXTENSIBLE,
    };
    use windows::Win32::System::Com::StructuredStorage::{PropVariantClear, PROPVARIANT};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED, STGM_READ,
    };
    use windows::Win32::System::Variant::VT_LPWSTR;

    // `WAVE_FORMAT_*` and `KSDATAFORMAT_SUBTYPE_*` sit behind windows features
    // this crate does not enable (`Win32_Media_Multimedia`,
    // `Win32_Media_KernelStreaming`), so the values are local. They are checked
    // against the crate's own published definitions: it exports
    // `KSDATAFORMAT_SUBTYPE_PCM = 0x00000001_0000_0010_8000_00aa00389b71`, and
    // the float subtype as `MEDIASUBTYPE_IEEE_FLOAT` / `MFAudioFormat_Float =
    // 0x00000003_0000_0010_8000_00aa00389b71` — the standard template with the
    // `WAVE_FORMAT_*` tag in the leading field.
    const WAVE_FORMAT_PCM: u16 = 1;
    const WAVE_FORMAT_IEEE_FLOAT: u16 = 3;
    const WAVE_FORMAT_EXTENSIBLE: u16 = 0xfffe;
    const KSDATAFORMAT_SUBTYPE_PCM: GUID = GUID::from_u128(0x00000001_0000_0010_8000_00aa00389b71);
    const KSDATAFORMAT_SUBTYPE_FLOAT: GUID =
        GUID::from_u128(0x00000003_0000_0010_8000_00aa00389b71);

    // The packet flags this build reads, from the same `_AUDCLNT_BUFFERFLAGS`
    // set the crate publishes (`AUDCLNT_BUFFERFLAGS_SILENT = 2`,
    // `AUDCLNT_BUFFERFLAGS_DATA_DISCONTINUITY = 1`), written as plain `u32`
    // because `GetBuffer` hands back a bare flag word.
    const SILENT: u32 = 2;
    const DISCONTINUITY: u32 = 1;
    /// Fallback buffer duration in 100-nanosecond units when the device will
    /// not say: 10 ms.
    const DEFAULT_HNS: i64 = 100_000;

    fn err(message: impl Into<String>) -> FluxError {
        FluxError::new(ErrorCode::AudioDeviceLost, "capture", message)
    }

    /// `WAVEFORMATEX` is `#[repr(C, packed(1))]`: reading it through a normal
    /// deref of the device's pointer is an unaligned access. `read_unaligned`
    /// copies it into aligned local storage, which is also what `Initialize`
    /// is then handed.
    unsafe fn wave_copy(ptr: *const WAVEFORMATEX) -> WAVEFORMATEX {
        unsafe { read_unaligned(ptr) }
    }

    unsafe fn extensible_copy(wave: *const WAVEFORMATEX) -> WAVEFORMATEXTENSIBLE {
        unsafe { read_unaligned(wave as *const WAVEFORMATEXTENSIBLE) }
    }

    /// Validate the wire format the device offered. Accepts 32-bit float, plain
    /// 16-bit PCM, and `WAVE_FORMAT_EXTENSIBLE` naming one of those two.
    /// Anything else is an error — never zeros.
    ///
    /// `ext` is the extension *as the engine allocated it*: `None` when the
    /// block carries no readable `WAVEFORMATEXTENSIBLE`. A tag-65534 header with
    /// no extension is refused rather than guessed, because the payload class
    /// lives only in that GUID.
    ///
    /// Both structs are taken *by value* and every field is copied out before
    /// use. `WAVEFORMATEX` and `WAVEFORMATEXTENSIBLE` are `#[repr(C, packed(1))]`,
    /// and rustc rejects `&wave.nChannels` — even inside `format_args!` — as an
    /// unaligned reference, because creating one is undefined behaviour whether
    /// or not it is ever read.
    fn classify(wave: WAVEFORMATEX, ext: Option<WAVEFORMATEXTENSIBLE>) -> FluxResult<WireFormat> {
        let WAVEFORMATEX {
            wFormatTag: tag,
            nChannels: channels,
            nSamplesPerSec: rate,
            nBlockAlign: block_align,
            wBitsPerSample: bits_reported,
            cbSize,
            ..
        } = wave;

        let mut float = None;
        let mut extensible = false;
        match tag {
            WAVE_FORMAT_IEEE_FLOAT => float = Some(true),
            WAVE_FORMAT_PCM => float = Some(false),
            WAVE_FORMAT_EXTENSIBLE => {
                extensible = true;
                let Some(WAVEFORMATEXTENSIBLE { SubFormat, .. }) = ext else {
                    return Err(err(format!(
                        "extensible format (tag 65534) with cbSize {cbSize}: the sub-format GUID \
                         is not readable from this block, so the payload class cannot be \
                         determined"
                    )));
                };
                if SubFormat == KSDATAFORMAT_SUBTYPE_FLOAT {
                    float = Some(true);
                } else if SubFormat == KSDATAFORMAT_SUBTYPE_PCM {
                    float = Some(false);
                } else {
                    return Err(err(format!(
                        "extensible format (tag 65534) names sub-format {SubFormat:?}, which is \
                         neither IEEE float nor PCM: {bits_reported} bits, {channels} channels"
                    )));
                }
            }
            _ => {}
        }
        let float = float.ok_or_else(|| {
            err(format!(
                "unsupported wire format: tag {tag}, {bits_reported} bits, {channels} channels — \
                 only float32 (tag 3 or extensible float) and 16-bit PCM (tag 1 or extensible \
                 PCM) are decoded"
            ))
        })?;
        if channels == 0 {
            return Err(err("wire format claims zero channels"));
        }
        if rate < 8_000 || rate > 192_000 {
            return Err(err(format!("implausible sample rate {rate} Hz")));
        }
        let bits: u16 = if float { 32 } else { 16 };
        if bits_reported != bits {
            return Err(err(format!(
                "format tag {tag} reports {bits_reported} bits per sample, expected {bits}"
            )));
        }
        Ok(WireFormat {
            sample_rate: rate,
            channels,
            bits_per_sample: bits,
            float,
            block_align,
            format_tag: tag,
            extensible,
        })
    }

    /// Read `PKEY_Device_FriendlyName` out of an endpoint's property store.
    /// Also used by the always-compiled capability enumeration in `lib.rs`,
    /// which printed a GUID tail where Windows prints a device name.
    pub fn friendly_name(device: &IMMDevice) -> Option<String> {
        unsafe {
            let store = device.OpenPropertyStore(STGM_READ).ok()?;
            let key = PKEY_Device_FriendlyName;
            let mut value = store.GetValue(&key).ok()?;
            // `GetValue` answers with a `PROPVARIANT` that has no `Drop`: the
            // string it points at is CoTaskMem-owned, so it must be cleared
            // here or every enumeration leaks one name per device.
            let read = read_lpstr(&mut value);
            let _ = PropVariantClear(&mut value as *mut PROPVARIANT);
            read.filter(|s| !s.trim().is_empty())
        }
    }

    /// The wide string inside a `PROPVARIANT`, or `None` when the variant is
    /// not `VT_LPWSTR`. Three `Anonymous` hops deep, because `PROPVARIANT`
    /// wraps a union that wraps the tagged struct that wraps the value union.
    unsafe fn read_lpstr(value: &mut PROPVARIANT) -> Option<String> {
        unsafe {
            let tagged = &value.Anonymous.Anonymous;
            if tagged.vt != VT_LPWSTR {
                return None;
            }
            let pw = tagged.Anonymous.pwszVal;
            if pw.is_null() {
                return None;
            }
            pw.to_string().ok()
        }
    }
    fn flow(tap: Tap) -> EDataFlow {
        if tap.flow_is_render() {
            eRender
        } else {
            eCapture
        }
    }

    /// The endpoint id, with the CoTaskMem block freed. `GetId` returns a
    /// `PWSTR` the newtype does not own, so an unfreed id leaks per device.
    fn device_id(device: &IMMDevice) -> String {
        unsafe {
            device
                .GetId()
                .ok()
                .map(|pw| {
                    let s = pw.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(pw.as_ptr() as *const core::ffi::c_void));
                    s
                })
                .unwrap_or_default()
        }
    }

    fn enumerator() -> FluxResult<IMMDeviceEnumerator> {
        unsafe {
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|e| err(format!("MMDeviceEnumerator: {e}")))
        }
    }

    /// Every active endpoint for both taps, friendly-named, with the `eConsole`
    /// default marked per data flow.
    pub fn list_endpoints() -> FluxResult<Vec<EndpointInfo>> {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            // Only uninitialise when *this call* initialised the apartment; the
            // thread may already carry one.
            let uninit = hr.is_ok();
            let out = list_endpoints_inner();
            if uninit {
                CoUninitialize();
            }
            out
        }
    }

    /// Separate function so the COM objects it owns are released *before*
    /// `CoUninitialize` runs. Measured: an enumerator that outlived apartment
    /// uninitialisation crashed a probe with exit code 139, after all of its
    /// output had already printed.
    fn list_endpoints_inner() -> FluxResult<Vec<EndpointInfo>> {
        unsafe {
            let enumerator = enumerator()?;
            let mut out = Vec::new();
            for tap in [Tap::Microphone, Tap::Loopback] {
                let f = flow(tap);
                let default_id = enumerator
                    .GetDefaultAudioEndpoint(f, eConsole)
                    .ok()
                    .map(|d| device_id(&d));
                let Ok(collection) = enumerator.EnumAudioEndpoints(f, DEVICE_STATE_ACTIVE) else {
                    continue;
                };
                let count = collection.GetCount().unwrap_or(0);
                for i in 0..count {
                    let Ok(device) = collection.Item(i) else {
                        continue;
                    };
                    let id = device_id(&device);
                    let name = friendly_name(&device).unwrap_or_else(|| id.clone());
                    let is_default = match default_id.as_deref() {
                        Some("") | None => false,
                        Some(d) => d == id,
                    };
                    out.push(EndpointInfo {
                        id,
                        name,
                        tap,
                        is_default,
                    });
                }
            }
            Ok(out)
        }
    }

    /// An open, validated capture stream. Deliberately *not* `Send`: it is
    /// created inside the thread that holds the COM apartment and dropped there.
    pub struct LiveCapture {
        com: ManuallyDrop<Com>,
        format: WireFormat,
        input_gain: f32,
        period_default_hns: i64,
        period_min_hns: i64,
        buffer_hns: i64,
        com_initialized: bool,
        running: bool,
        endpoint_id: String,
        tap: Tap,
    }

    /// The COM objects this stream holds. `ManuallyDrop` because field drops
    /// happen *after* `Drop::drop` returns, and the measured requirement is the
    /// reverse order: everything released before `CoUninitialize`.
    struct Com {
        capture: IAudioCaptureClient,
        client: IAudioClient,
        device: IMMDevice,
        enumerator: IMMDeviceEnumerator,
    }

    impl LiveCapture {
        pub fn open(cfg: &CaptureConfig) -> FluxResult<Self> {
            unsafe {
                let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
                let initialized = hr.is_ok();
                let opened = Self::open_inner(cfg, initialized);
                if opened.is_err() && initialized {
                    CoUninitialize();
                }
                opened
            }
        }

        fn open_inner(cfg: &CaptureConfig, com_initialized: bool) -> FluxResult<Self> {
            unsafe {
                let enumerator = enumerator()?;
                let device = match &cfg.device_id {
                    Some(id) => {
                        let wide: Vec<u16> = id.encode_utf16().chain(std::iter::once(0)).collect();
                        enumerator
                            .GetDevice(windows::core::PCWSTR(wide.as_ptr()))
                            .map_err(|e| {
                                err(format!(
                                    "endpoint {id} could not be opened ({e}); it may have been \
                                     disconnected since it was listed"
                                ))
                            })?
                    }
                    None => enumerator
                        .GetDefaultAudioEndpoint(flow(cfg.tap), eConsole)
                        .map_err(|e| {
                            err(format!("no default {} endpoint ({e})", cfg.tap.as_str()))
                        })?,
                };
                let endpoint_id = device_id(&device);
                let client: IAudioClient = device
                    .Activate(CLSCTX_ALL, None)
                    .map_err(|e| err(format!("Activate<IAudioClient> on {endpoint_id}: {e}")))?;

                // Shared mode must use the engine's own mix format.
                let mix = client
                    .GetMixFormat()
                    .map_err(|e| err(format!("GetMixFormat on {endpoint_id}: {e}")))?;
                let wave = wave_copy(mix);
                // The extension has to be read from the allocation itself, and
                // before it is freed: `wave_copy` truncates to the 18-byte
                // header, so a `SubFormat` taken out of the local copy is not
                // the device's GUID but whatever sat past those 18 bytes on the
                // stack. Measured here: the S/PDIF render endpoint's float32 mix
                // format was refused as "unsupported wire format: tag 65534".
                let ext = if wave.wFormatTag == WAVE_FORMAT_EXTENSIBLE && wave.cbSize >= 22 {
                    // SAFETY: the enclosing block; `cbSize >= 22` is the engine's
                    // own word that the block continues for the 22 bytes after
                    // the header.
                    Some(extensible_copy(mix))
                } else {
                    None
                };
                // Freed once the whole block — header and extension — has been
                // copied out, and before any early return could skip it.
                CoTaskMemFree(Some(mix as *const core::ffi::c_void));
                let format = classify(wave, ext)?;

                // `GetDevicePeriod` answers in 100-nanosecond units:
                // 100000 = 10 ms, 30000 = 3 ms.
                let mut default_hns: i64 = 0;
                let mut min_hns: i64 = 0;
                let _ = client.GetDevicePeriod(
                    Some(&mut default_hns as *mut i64),
                    Some(&mut min_hns as *mut i64),
                );
                if default_hns <= 0 {
                    default_hns = DEFAULT_HNS;
                }
                let wanted_hns = i64::from(cfg.buffer_ms.max(1)) * 10_000;
                let buffer_hns = wanted_hns.max(default_hns);
                let flags = match cfg.tap {
                    Tap::Loopback => AUDCLNT_STREAMFLAGS_LOOPBACK,
                    Tap::Microphone => 0,
                };
                // Hand `Initialize` the same shape the engine offered. A
                // tag-65534 header with `cbSize` 22 promises 22 bytes that
                // follow it, so the 18-byte header on its own is not a valid
                // argument — measured here as
                // "Initialize failed … The parameter is incorrect. (0x80070057)"
                // on the default render endpoint, after classification had
                // already been fixed to read the sub-format from the block.
                let block = match ext {
                    Some(ref e) => core::ptr::from_ref(e).cast::<WAVEFORMATEX>(),
                    None => addr_of!(wave),
                };
                client
                    .Initialize(
                        AUDCLNT_SHAREMODE_SHARED,
                        flags,
                        buffer_hns,
                        0,
                        block,
                        None,
                    )
                    .map_err(|e| {
                        err(format!(
                            "Initialize failed on {endpoint_id} ({} channels, {} Hz, tag {}, \
                             {} ms buffer): {e}",
                            format.channels,
                            format.sample_rate,
                            format.format_tag,
                            buffer_hns / 10_000,
                        ))
                    })?;

                let capture: IAudioCaptureClient = client.GetService().map_err(|e| {
                    err(format!(
                        "GetService<IAudioCaptureClient> on {endpoint_id}: {e}"
                    ))
                })?;

                Ok(Self {
                    com: ManuallyDrop::new(Com {
                        capture,
                        client,
                        device,
                        enumerator,
                    }),
                    format,
                    // Explicit trim, linear. No auto-gain anywhere.
                    input_gain: 10f32.powf(cfg.input_db / 20.0),
                    period_default_hns: default_hns,
                    period_min_hns: min_hns,
                    buffer_hns,
                    com_initialized,
                    running: false,
                    endpoint_id,
                    tap: cfg.tap,
                })
            }
        }

        pub fn endpoint_id(&self) -> &str {
            &self.endpoint_id
        }

        pub fn tap(&self) -> Tap {
            self.tap
        }

        pub fn format(&self) -> WireFormat {
            self.format
        }

        /// `GetDevicePeriod` results in 100-ns units: (default, minimum).
        pub fn periods_hns(&self) -> (i64, i64) {
            (self.period_default_hns, self.period_min_hns)
        }

        pub fn buffer_hns(&self) -> i64 {
            self.buffer_hns
        }

        pub fn start(&mut self) -> FluxResult<()> {
            unsafe {
                self.com
                    .client
                    .Start()
                    .map_err(|e| err(format!("Start on {}: {e}", self.endpoint_id)))?
            };
            self.running = true;
            Ok(())
        }

        pub fn stop(&mut self) -> FluxResult<()> {
            if !self.running {
                return Ok(());
            }
            self.running = false;
            unsafe {
                self.com
                    .client
                    .Stop()
                    .map_err(|e| err(format!("Stop: {e}")))
            }
        }

        pub fn is_running(&self) -> bool {
            self.running
        }

        /// Take everything the device has queued and append the mono downmix to
        /// `dst`; samples are scaled by the explicit input trim only.
        ///
        /// A call that finds no packet returns `Drain::empty()`. On a loopback
        /// endpoint that is the *normal* state while nothing renders, which is
        /// why the caller runs an idle watchdog instead of assuming the stream
        /// died.
        pub fn drain_into(&mut self, dst: &mut Vec<f32>) -> FluxResult<Drain> {
            let mut drain = Drain::empty();
            let channels = usize::from(self.format.channels);
            let frame_bytes = self.format.frame_bytes();
            let sample_bytes = self.format.sample_bytes();
            unsafe {
                loop {
                    let pending = self
                        .com
                        .capture
                        .GetNextPacketSize()
                        .map_err(|e| err(format!("GetNextPacketSize: {e}")))?;
                    if pending == 0 {
                        break;
                    }
                    let mut data: *mut u8 = std::ptr::null_mut();
                    let mut frames: u32 = 0;
                    let mut flags: u32 = 0;
                    self.com
                        .capture
                        .GetBuffer(
                            &mut data as *mut *mut u8,
                            &mut frames as *mut u32,
                            &mut flags as *mut u32,
                            None,
                            None,
                        )
                        .map_err(|e| err(format!("GetBuffer: {e}")))?;
                    if frames == 0 {
                        // Non-zero packet size with no frames should not happen.
                        // Stop rather than spin on it; the watchdog will report
                        // the stall.
                        break;
                    }
                    dst.reserve(frames as usize);
                    let silent = flags & SILENT != 0;
                    drain.discontinuity |= flags & DISCONTINUITY != 0;
                    if silent {
                        drain.silent_frames += frames;
                    }
                    for frame in 0..frames as usize {
                        let base = frame * frame_bytes;
                        let mut acc = 0.0f32;
                        for channel in 0..channels {
                            acc += self.sample_at(data, base + channel * sample_bytes);
                        }
                        dst.push((acc / channels as f32) * self.input_gain);
                    }
                    drain.frames += frames;
                    drain.packets += 1;
                    self.com
                        .capture
                        .ReleaseBuffer(frames)
                        .map_err(|e| err(format!("ReleaseBuffer: {e}")))?;
                }
            }
            Ok(drain)
        }

        /// One sample from the raw device block. The pointer is not guaranteed
        /// aligned for the payload type, so every read is unaligned.
        unsafe fn sample_at(&self, data: *mut u8, offset: usize) -> f32 {
            unsafe {
                if self.format.float {
                    read_unaligned(data.add(offset) as *const f32)
                } else {
                    read_unaligned(data.add(offset) as *const i16) as f32 / 32768.0
                }
            }
        }
    }

    impl Drop for LiveCapture {
        fn drop(&mut self) {
            unsafe {
                if self.running {
                    let _ = self.com.client.Stop();
                    self.running = false;
                }
                let com = ManuallyDrop::take(&mut self.com);
                drop(com.capture);
                drop(com.client);
                drop(com.device);
                drop(com.enumerator);
                if self.com_initialized {
                    CoUninitialize();
                }
            }
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn endpoints_carry_readable_names_not_guid_tails() {
            let list = list_endpoints().expect("WASAPI enumeration");
            // The apparatus, not the outcome: a rig with no audio endpoints may
            // legitimately list none. But a non-empty list where every entry is
            // its own id means we are back to printing GUID tails.
            if !list.is_empty() {
                let named = list.iter().filter(|e| e.name != e.id).count();
                assert!(
                    named > 0,
                    "{} endpoints enumerated and none carried a friendly name",
                    list.len()
                );
            }
        }

        #[test]
        fn every_tap_marks_at_most_one_default() {
            let list = list_endpoints().expect("WASAPI enumeration");
            for tap in [Tap::Microphone, Tap::Loopback] {
                let defaults = list.iter().filter(|e| e.tap == tap && e.is_default).count();
                assert!(
                    defaults <= 1,
                    "{}: {defaults} endpoints claim to be the default",
                    tap.as_str()
                );
            }
        }

        #[test]
        fn an_unknown_endpoint_id_fails_loudly() {
            let cfg = CaptureConfig {
                device_id: Some("this-endpoint-does-not-exist".into()),
                ..Default::default()
            };
            let Err(e) = LiveCapture::open(&cfg) else {
                panic!("a made-up endpoint id opened a stream");
            };
            assert_eq!(e.code, ErrorCode::AudioDeviceLost);
            assert!(
                e.message.contains("could not be opened"),
                "unhelpful message: {}",
                e.message
            );
        }

        // The three tests below exist because of a measured failure, not a
        // hypothetical one: `flux capture start --tap loopback` on this rig's
        // default render endpoint reported
        // "unsupported wire format: tag 65534, 32 bits, 2 channels" and exited 1
        // while the engine was handing out an ordinary float32 mix format. The
        // sub-format GUID was being read out of the 18-byte *header copy*, i.e.
        // past the end of it, so the payload class was stack noise. Windows
        // shared-mode mix formats are extensible far more often than not, which
        // made that the path every real device takes.

        fn header(tag: u16, bits: u16, channels: u16, cb_size: u16) -> WAVEFORMATEX {
            WAVEFORMATEX {
                wFormatTag: tag,
                nChannels: channels,
                nSamplesPerSec: 48_000,
                nAvgBytesPerSec: 48_000 * channels as u32 * bits as u32 / 8,
                nBlockAlign: channels * bits / 8,
                wBitsPerSample: bits,
                cbSize: cb_size,
            }
        }

        fn extension(cb_size: u16, sub: GUID) -> WAVEFORMATEXTENSIBLE {
            WAVEFORMATEXTENSIBLE {
                Format: header(WAVE_FORMAT_EXTENSIBLE, 32, 2, cb_size),
                // The union's default is all-zero, which is the `wReserved`
                // member. `classify` reads only `SubFormat`: that GUID is the
                // payload class, and nothing else in the extension is.
                Samples: Default::default(),
                dwChannelMask: 3,
                SubFormat: sub,
            }
        }

        #[test]
        fn an_extensible_float32_block_is_the_common_case_not_a_refusal() {
            let wave = header(WAVE_FORMAT_EXTENSIBLE, 32, 2, 22);
            let ext = extension(22, KSDATAFORMAT_SUBTYPE_FLOAT);
            let f = classify(wave, Some(ext)).expect("float32 extensible mix format must open");
            assert!(f.float, "extensible float32 was classified as integer PCM");
            assert!(f.extensible, "the sub-format, not the tag, decided this");
            assert_eq!((f.sample_rate, f.channels, f.bits_per_sample), (48_000, 2, 32));
            assert_eq!(f.frame_bytes(), 8);
        }

        /// The header alone does not name a payload class. Accepting it anyway
        /// would mean either a guessed float decode — the silent-zeros failure the
        /// brief forbids — or reading past the copy, which is what used to happen.
        #[test]
        fn an_extensible_header_without_its_extension_is_refused_not_guessed() {
            let wave = header(WAVE_FORMAT_EXTENSIBLE, 32, 2, 22);
            let Err(e) = classify(wave, None) else {
                panic!("a tag-65534 header was classified without its sub-format GUID");
            };
            assert_eq!(e.code, ErrorCode::AudioDeviceLost);
            assert!(
                e.message.contains("sub-format") && e.message.contains("cannot be"),
                "unhelpful message: {}",
                e.message
            );
        }

        #[test]
        fn an_extensible_block_naming_another_subformat_is_refused_by_name() {
            let wave = header(WAVE_FORMAT_EXTENSIBLE, 32, 2, 22);
            let alien = extension(22, GUID::from_u128(0x00000016_0000_0010_8000_00aa00389b71));
            let Err(e) = classify(wave, Some(alien)) else {
                panic!("sub-format 0x16 (ALAW) opened a stream");
            };
            assert!(
                e.message.contains("sub-format"),
                "unhelpful message: {}",
                e.message
            );
            // A plain 16-bit PCM and a plain float32 header still open, so the
            // two refusals above are about the extension, not about the guard.
            assert!(classify(header(WAVE_FORMAT_PCM, 16, 2, 0), None).is_ok());
            assert!(classify(header(WAVE_FORMAT_IEEE_FLOAT, 32, 2, 0), None).is_ok());
        }
    }
}

#[cfg(all(windows, feature = "live-audio"))]
pub use imp::{friendly_name, list_endpoints, LiveCapture};

/// The same surface, compiled when the live path is not. Every operation fails
/// with `SdkMissing` so the CLI can exit 1 for a reason the owner can act on.
#[cfg(not(all(windows, feature = "live-audio")))]
mod imp {
    use super::{CaptureConfig, Drain, EndpointInfo, Tap, WireFormat};
    use flux_domain::{ErrorCode, FluxError, FluxResult};

    fn not_compiled() -> FluxError {
        FluxError::new(
            ErrorCode::SdkMissing,
            "capture",
            "live audio capture is not in this build: rebuild with --features live-audio \
             (Windows only, WASAPI shared mode)",
        )
    }

    /// Never constructible from outside: `open` is the only path to one, and it
    /// always fails here.
    pub struct LiveCapture {
        _inert: (),
    }

    impl LiveCapture {
        pub fn open(_cfg: &CaptureConfig) -> FluxResult<Self> {
            Err(not_compiled())
        }
        pub fn endpoint_id(&self) -> &str {
            ""
        }
        pub fn tap(&self) -> Tap {
            Tap::Microphone
        }
        pub fn format(&self) -> WireFormat {
            WireFormat::default()
        }
        pub fn periods_hns(&self) -> (i64, i64) {
            (0, 0)
        }
        pub fn buffer_hns(&self) -> i64 {
            0
        }
        pub fn start(&mut self) -> FluxResult<()> {
            Err(not_compiled())
        }
        pub fn stop(&mut self) -> FluxResult<()> {
            Err(not_compiled())
        }
        pub fn is_running(&self) -> bool {
            false
        }
        pub fn drain_into(&mut self, _dst: &mut Vec<f32>) -> FluxResult<Drain> {
            Err(not_compiled())
        }
    }

    pub fn list_endpoints() -> FluxResult<Vec<EndpointInfo>> {
        Err(not_compiled())
    }
}

#[cfg(not(all(windows, feature = "live-audio")))]
pub use imp::{list_endpoints, LiveCapture};
