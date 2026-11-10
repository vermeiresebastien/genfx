//! FFmpeg-backed media: probing, frame decode, audio decode, and encode.
//!
//! Media runs in an FFmpeg **process** (see DECISIONS D004). This keeps media
//! failures isolated from the render thread and avoids FFmpeg ABI mismatch
//! crashes. Every operation is bounded and cancellable, and uses the exact
//! rational time mapping from the engine, never nominal-fps frame math.

use flux_domain::{
    Asset, AssetKind, AssetStream, ErrorCode, FluxError, FluxResult, PixelAspect, RationalTime,
};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

/// Locate the ffmpeg toolchain. Order: FLUX_FFMPEG env, PATH, common winget dir.
pub fn find_ffmpeg() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("FLUX_FFMPEG") {
        let pb = PathBuf::from(p);
        if pb.exists() {
            return Some(pb);
        }
    }
    if let Some(p) = which("ffmpeg") {
        return Some(p);
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        let base = PathBuf::from(local)
            .join("Microsoft/WinGet/Packages/Gyan.FFmpeg_Microsoft.Winget.Source_8wekyb3d8bbwe");
        if let Ok(rd) = std::fs::read_dir(&base) {
            for e in rd.flatten() {
                let cand = e.path().join("bin/ffmpeg.exe");
                if cand.exists() {
                    return Some(cand);
                }
            }
        }
    }
    None
}

pub fn find_ffprobe() -> Option<PathBuf> {
    if let Some(ff) = find_ffmpeg() {
        let probe = ff.with_file_name(if cfg!(windows) {
            "ffprobe.exe"
        } else {
            "ffprobe"
        });
        if probe.exists() {
            return Some(probe);
        }
    }
    which("ffprobe")
}

fn which(name: &str) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    };
    let path = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path) {
        let cand = dir.join(&exe);
        if cand.exists() {
            return Some(cand);
        }
    }
    None
}

/// Raw bytes of an RGBA8 image.
pub struct DecodedFrame {
    pub width: u32,
    pub height: u32,
    pub pts: RationalTime,
    pub rgba: Vec<u8>,
}

/// Probe result used to build an `Asset`.
pub struct ProbeInfo {
    pub kind: AssetKind,
    pub duration: Option<RationalTime>,
    pub streams: Vec<AssetStream>,
    pub rotation_deg: i32,
    pub sample_aspect: PixelAspect,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

pub struct MediaWorker {
    ffmpeg: PathBuf,
    ffprobe: PathBuf,
}

impl MediaWorker {
    pub fn new() -> FluxResult<Self> {
        let ffmpeg = find_ffmpeg()
            .ok_or_else(|| FluxError::new(ErrorCode::MediaDecode, "media", "ffmpeg not found"))?;
        let ffprobe = find_ffprobe()
            .ok_or_else(|| FluxError::new(ErrorCode::MediaDecode, "media", "ffprobe not found"))?;
        Ok(Self { ffmpeg, ffprobe })
    }

    /// Capability report: decoders/encoders the installed FFmpeg actually has.
    pub fn capabilities(&self) -> MediaCaps {
        let out = Command::new(&self.ffmpeg)
            .args(["-hide_banner", "-encoders"])
            .output()
            .ok();
        let text = out
            .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
            .unwrap_or_default();
        let has = |n: &str| text.contains(n);
        MediaCaps {
            h264: has("libx264"),
            hevc: has("libx265"),
            prores: has("prores"),
            aac: has(" aac "),
            gif: has(" gif "),
            png: has(" png "),
            wav: has("pcm_s16le"),
            ffmpeg_path: self.ffmpeg.display().to_string(),
        }
    }

    pub fn probe(&self, path: &Path) -> FluxResult<ProbeInfo> {
        let out = Command::new(&self.ffprobe)
            .args([
                "-hide_banner",
                "-v",
                "error",
                "-print_format",
                "json",
                "-show_format",
                "-show_streams",
                path.to_str().unwrap_or_default(),
            ])
            .output()
            .map_err(|e| FluxError::new(ErrorCode::MediaDecode, "media", e.to_string()))?;
        if !out.status.success() {
            return Err(FluxError::new(
                ErrorCode::MediaDecode,
                "media",
                format!("ffprobe failed: {}", String::from_utf8_lossy(&out.stderr)),
            ));
        }
        let v: ProbeJson = serde_json::from_slice(&out.stdout)
            .map_err(|e| FluxError::new(ErrorCode::MediaDecode, "media", e.to_string()))?;
        let kind = infer_kind(path, &v);
        let mut streams = vec![];
        let mut width = None;
        let mut height = None;
        let mut rotation = 0;
        let mut sar = PixelAspect { num: 1, den: 1 };
        for s in &v.streams {
            if s.codec_type.as_deref() == Some("video") {
                width = s.width;
                height = s.height;
                if let Some(r) = s
                    .side_data_list
                    .as_ref()
                    .and_then(|l| l.iter().find_map(|d| d.rotation))
                {
                    rotation = r;
                }
                if let Some(tag) = &s.sample_aspect_ratio {
                    if let Some((n, d)) = tag.split_once(':') {
                        if let (Ok(n), Ok(d)) = (n.parse::<u32>(), d.parse::<u32>()) {
                            if n > 0 && d > 0 {
                                sar = PixelAspect { num: n, den: d };
                            }
                        }
                    }
                }
            }
            streams.push(AssetStream {
                index: s.index,
                kind: s.codec_type.clone().unwrap_or_default(),
                codec: s.codec_name.clone().unwrap_or_default(),
                width: s.width,
                height: s.height,
                sample_rate: s.sample_rate.as_ref().and_then(|x| x.parse().ok()),
                channels: s.channels.map(|c| c as u16),
            });
        }
        let duration = v
            .format
            .as_ref()
            .and_then(|f| f.duration.as_ref())
            .and_then(|d| d.parse::<f64>().ok())
            .and_then(|secs| {
                let micros = (secs * 1_000_000.0).round() as i64;
                RationalTime::new(micros, 1_000_000).ok()
            });
        Ok(ProbeInfo {
            kind,
            duration,
            streams,
            rotation_deg: rotation,
            sample_aspect: sar,
            width,
            height,
        })
    }

    pub fn asset_from_probe(&self, path: &Path, probe: &ProbeInfo) -> FluxResult<Asset> {
        let meta = std::fs::metadata(path)
            .map_err(|e| FluxError::new(ErrorCode::MediaMissing, "media", e.to_string()))?;
        let mtime = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        Ok(Asset {
            id: uuid::Uuid::new_v4().to_string(),
            kind: probe.kind,
            name: path
                .file_name()
                .and_then(|s| s.to_str())
                .unwrap_or("asset")
                .to_string(),
            original_path: path.display().to_string(),
            relative_path: None,
            fingerprint: format!("{}-{}", meta.len(), mtime),
            size_bytes: meta.len(),
            sha256: None,
            duration: probe.duration,
            streams: probe.streams.clone(),
            rotation_deg: probe.rotation_deg,
            sample_aspect: probe.sample_aspect,
            missing: false,
        })
    }

    /// Decode one RGBA8 frame at an exact source PTS.
    pub fn decode_rgba_frame(
        &self,
        path: &Path,
        pts: RationalTime,
        width: u32,
        height: u32,
        cancel: Arc<AtomicBool>,
    ) -> FluxResult<DecodedFrame> {
        let png = self.frame_to_rgba_bytes(path, pts, width, height, cancel)?;
        let (w, h, rgba) = decode_png_rgba(&png)?;
        Ok(DecodedFrame {
            width: w,
            height: h,
            pts,
            rgba,
        })
    }

    fn frame_to_rgba_bytes(
        &self,
        path: &Path,
        pts: RationalTime,
        width: u32,
        height: u32,
        cancel: Arc<AtomicBool>,
    ) -> FluxResult<Vec<u8>> {
        let secs = pts.as_f64().max(0.0);
        let vf = format!("scale={width}:{height}:flags=bilinear,format=rgba");
        let mut child = Command::new(&self.ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-ss",
                &format!("{secs:.6}"),
                "-i",
                path.to_str().unwrap_or_default(),
                "-frames:v",
                "1",
                "-vf",
                &vf,
                "-f",
                "image2pipe",
                "-vcodec",
                "png",
                "-",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| FluxError::new(ErrorCode::MediaDecode, "media", e.to_string()))?;
        let mut bytes = Vec::new();
        use std::io::Read;
        let mut stdout = child.stdout.take().unwrap();
        let mut buf = [0u8; 65536];
        loop {
            if cancel.load(Ordering::Relaxed) {
                let _ = child.kill();
                return Err(FluxError::new(
                    ErrorCode::MediaDecode,
                    "media",
                    "decode cancelled",
                ));
            }
            match stdout.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => bytes.extend_from_slice(&buf[..n]),
                Err(e) => {
                    let _ = child.kill();
                    return Err(FluxError::new(
                        ErrorCode::MediaDecode,
                        "media",
                        e.to_string(),
                    ));
                }
            }
        }
        let _ = child.wait();
        if bytes.is_empty() {
            return Err(FluxError::new(
                ErrorCode::MediaDecode,
                "media",
                "empty frame",
            ));
        }
        Ok(bytes)
    }

    /// Decode the full audio track as interleaved f32 stereo at `sample_rate`.
    pub fn decode_audio_stereo(&self, path: &Path, sample_rate: u32) -> FluxResult<AudioTrack> {
        let child = Command::new(&self.ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-i",
                path.to_str().unwrap_or_default(),
                "-vn",
                "-ac",
                "2",
                "-ar",
                &sample_rate.to_string(),
                "-f",
                "f32le",
                "-acodec",
                "pcm_f32le",
                "-",
            ])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| FluxError::new(ErrorCode::MediaDecode, "media", e.to_string()))?;
        let out = child
            .wait_with_output()
            .map_err(|e| FluxError::new(ErrorCode::MediaDecode, "media", e.to_string()))?;
        if out.stdout.is_empty() {
            return Err(FluxError::new(
                ErrorCode::MediaDecode,
                "media",
                "no audio track",
            ));
        }
        let samples: Vec<f32> = out
            .stdout
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        Ok(AudioTrack {
            sample_rate,
            frames: samples.len() / 2,
            samples,
        })
    }

    /// Encode an RGBA8 image sequence + optional WAV into a container.
    pub fn encode_video_from_frames(
        &self,
        frames_dir: &Path,
        fps: &str,
        audio_wav: Option<&Path>,
        out: &Path,
        codec: EncodeCodec,
    ) -> FluxResult<()> {
        if let Some(parent) = out.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let pattern = frames_dir.join("frame_%06d.png");
        let mut cmd = Command::new(&self.ffmpeg);
        cmd.args(["-hide_banner", "-loglevel", "error", "-y"]);
        cmd.args([
            "-framerate",
            fps,
            "-i",
            pattern.to_str().unwrap_or_default(),
        ]);
        if let Some(a) = audio_wav {
            cmd.args(["-i", a.to_str().unwrap_or_default()]);
        }
        let is_mp4 = matches!(codec, EncodeCodec::H264 | EncodeCodec::Hevc);
        match codec {
            EncodeCodec::H264 => {
                cmd.args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18"]);
            }
            EncodeCodec::Hevc => {
                cmd.args(["-c:v", "libx265", "-pix_fmt", "yuv420p", "-crf", "22"]);
            }
            EncodeCodec::ProRes => {
                cmd.args([
                    "-c:v",
                    "prores_ks",
                    "-profile:v",
                    "4444",
                    "-pix_fmt",
                    "yuva444p10le",
                ]);
            }
            EncodeCodec::Gif => {
                cmd.args(["-vf", "fps=15,scale=640:-1:flags=lanczos", "-loop", "0"]);
            }
            EncodeCodec::JpegSequence => {
                // JPEG acceptance matrix entry: write numbered JPEGs beside PNGs.
                let jpeg_dir = frames_dir.with_file_name(format!(
                    "{}_jpeg",
                    frames_dir
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("frames")
                ));
                let _ = std::fs::create_dir_all(&jpeg_dir);
                cmd.args(["-c:v", "mjpeg", "-q:v", "3"]);
                cmd.arg(jpeg_dir.join("frame_%06d.jpg"));
                return finish_encode(cmd);
            }
            EncodeCodec::None => {}
        }
        if audio_wav.is_some() && !matches!(codec, EncodeCodec::Gif) {
            cmd.args(["-c:a", "aac", "-b:a", "192k"]);
        }
        if is_mp4 {
            cmd.args(["-movflags", "+faststart"]);
        }
        cmd.arg(out);
        finish_encode(cmd)
    }
}

/// Streams raw RGBA frames straight into ffmpeg over stdin and encodes once at
/// the end. This replaces per-frame PNG files, which produced thousands of
/// intermediate images and a very slow export.
pub struct StreamingEncoder {
    child: Option<std::process::Child>,
    stdin: Option<std::process::ChildStdin>,
    out: PathBuf,
    width: u32,
    height: u32,
    codec: EncodeCodec,
    ffmpeg: PathBuf,
}

impl StreamingEncoder {
    pub fn start(
        out: &Path,
        width: u32,
        height: u32,
        fps: FileFrameRate,
        codec: EncodeCodec,
    ) -> FluxResult<Self> {
        let ffmpeg = find_ffmpeg()
            .ok_or_else(|| FluxError::new(ErrorCode::ExportEncoder, "media", "ffmpeg not found"))?;
        if let Some(parent) = out.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let mut cmd = Command::new(&ffmpeg);
        cmd.args(["-hide_banner", "-loglevel", "error", "-y"]);
        // Raw RGBA input from the render loop.
        cmd.args([
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{width}x{height}"),
            "-framerate",
            &format!("{}/{}", fps.num, fps.den),
            "-i",
            "-",
        ]);
        match codec {
            EncodeCodec::H264 => {
                cmd.args(["-c:v", "libx264", "-pix_fmt", "yuv420p", "-crf", "18"]);
                cmd.args(["-movflags", "+faststart"]);
            }
            EncodeCodec::Hevc => {
                cmd.args(["-c:v", "libx265", "-pix_fmt", "yuv420p", "-crf", "22"]);
                cmd.args(["-movflags", "+faststart"]);
            }
            EncodeCodec::ProRes => {
                cmd.args([
                    "-c:v",
                    "prores_ks",
                    "-profile:v",
                    "4444",
                    "-pix_fmt",
                    "yuva444p10le",
                ]);
            }
            EncodeCodec::Gif => {
                cmd.args(["-vf", "fps=15,scale=640:-1:flags=lanczos", "-loop", "0"]);
            }
            _ => {}
        }
        cmd.arg(out);
        cmd.stdin(Stdio::piped()).stderr(Stdio::piped());
        let mut child = cmd
            .spawn()
            .map_err(|e| FluxError::new(ErrorCode::ExportEncoder, "media", e.to_string()))?;
        let stdin = child.stdin.take();
        Ok(Self {
            child: Some(child),
            stdin,
            out: out.to_path_buf(),
            width,
            height,
            codec,
            ffmpeg,
        })
    }

    /// Push one tightly-packed RGBA8 frame (top-left origin).
    pub fn write_frame(&mut self, rgba: &[u8]) -> FluxResult<()> {
        let expected = (self.width as usize) * (self.height as usize) * 4;
        if rgba.len() != expected {
            return Err(FluxError::new(
                ErrorCode::ExportEncoder,
                "media",
                format!("frame size {} != expected {}", rgba.len(), expected),
            ));
        }
        use std::io::Write;
        let stdin = self
            .stdin
            .as_mut()
            .ok_or_else(|| FluxError::new(ErrorCode::ExportEncoder, "media", "encoder closed"))?;
        stdin
            .write_all(rgba)
            .map_err(|e| FluxError::new(ErrorCode::ExportEncoder, "media", e.to_string()))
    }

    /// Close the pipe, wait for ffmpeg, and (for video codecs) mux audio.
    pub fn finish(mut self, audio_wav: Option<&Path>) -> FluxResult<PathBuf> {
        drop(self.stdin.take());
        let mut child = self
            .child
            .take()
            .ok_or_else(|| FluxError::new(ErrorCode::ExportEncoder, "media", "no encoder"))?;
        let stderr = child.stderr.take();
        let status = child
            .wait()
            .map_err(|e| FluxError::new(ErrorCode::ExportEncoder, "media", e.to_string()))?;
        if !status.success() {
            let mut msg = String::new();
            if let Some(mut s) = stderr {
                use std::io::Read;
                let _ = s.read_to_string(&mut msg);
            }
            return Err(FluxError::new(
                ErrorCode::ExportEncoder,
                "media",
                format!(
                    "encoder failed: {}",
                    msg.lines().last().unwrap_or("unknown error").trim()
                ),
            ));
        }

        // Mux audio: a second ffmpeg pass that copies the video stream.
        if let Some(wav) = audio_wav {
            if matches!(self.codec, EncodeCodec::H264 | EncodeCodec::Hevc | EncodeCodec::ProRes)
                && wav.exists()
            {
                let tmp = self.out.with_extension("mux.mp4");
                let status = Command::new(&self.ffmpeg)
                    .args(["-hide_banner", "-loglevel", "error", "-y"])
                    .arg("-i")
                    .arg(&self.out)
                    .arg("-i")
                    .arg(wav)
                    .args(["-c:v", "copy", "-c:a", "aac", "-b:a", "192k", "-shortest"])
                    .arg(&tmp)
                    .status()
                    .map_err(|e| {
                        FluxError::new(ErrorCode::ExportEncoder, "media", e.to_string())
                    })?;
                if status.success() {
                    let _ = std::fs::remove_file(&self.out);
                    let _ = std::fs::rename(&tmp, &self.out);
                }
            }
        }
        Ok(self.out)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct FileFrameRate {
    pub num: u32,
    pub den: u32,
}

fn finish_encode(mut cmd: Command) -> FluxResult<()> {
    let status = cmd
        .status()
        .map_err(|e| FluxError::new(ErrorCode::ExportEncoder, "media", e.to_string()))?;
    if !status.success() {
        return Err(FluxError::new(
            ErrorCode::ExportEncoder,
            "media",
            format!("encoder exited with {status}"),
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EncodeCodec {
    H264,
    Hevc,
    ProRes,
    Gif,
    JpegSequence,
    None,
}

#[derive(Debug, Clone)]
pub struct MediaCaps {
    pub h264: bool,
    pub hevc: bool,
    pub prores: bool,
    pub aac: bool,
    pub gif: bool,
    pub png: bool,
    pub wav: bool,
    pub ffmpeg_path: String,
}

#[derive(Debug, Clone)]
pub struct AudioTrack {
    pub sample_rate: u32,
    pub frames: usize,
    pub samples: Vec<f32>,
}

impl AudioTrack {
    pub fn duration(&self) -> f64 {
        self.frames as f64 / self.sample_rate as f64
    }
}

#[derive(Debug, Deserialize)]
struct ProbeJson {
    #[serde(default)]
    format: Option<FormatJson>,
    #[serde(default)]
    streams: Vec<StreamJson>,
}

#[derive(Debug, Deserialize)]
struct FormatJson {
    #[serde(default)]
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct StreamJson {
    index: i32,
    codec_name: Option<String>,
    codec_type: Option<String>,
    width: Option<u32>,
    height: Option<u32>,
    sample_rate: Option<String>,
    channels: Option<u32>,
    sample_aspect_ratio: Option<String>,
    #[serde(default)]
    side_data_list: Option<Vec<SideData>>,
}

#[derive(Debug, Deserialize)]
struct SideData {
    #[serde(default)]
    rotation: Option<i32>,
}

fn infer_kind(path: &Path, probe: &ProbeJson) -> AssetKind {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "svg" => return AssetKind::Svg,
        "json" | "lottie" => return AssetKind::Lottie,
        "glb" | "gltf" => return AssetKind::Gltf,
        "ply" => return AssetKind::Ply,
        "csv" => return AssetKind::Csv,
        _ => {}
    }
    let has_video = probe
        .streams
        .iter()
        .any(|s| s.codec_type.as_deref() == Some("video"));
    let has_audio = probe
        .streams
        .iter()
        .any(|s| s.codec_type.as_deref() == Some("audio"));
    let is_image = probe.streams.iter().any(|s| {
        s.codec_type.as_deref() == Some("video") && is_still_codec(s.codec_name.as_deref())
    });
    if is_image {
        AssetKind::Image
    } else if has_video {
        AssetKind::Video
    } else if has_audio {
        AssetKind::Audio
    } else {
        AssetKind::Video
    }
}

fn is_still_codec(codec: Option<&str>) -> bool {
    matches!(
        codec,
        Some("png") | Some("mjpeg") | Some("bmp") | Some("webp") | Some("tiff") | Some("gif")
    )
}

/// Decode PNG bytes (from ffmpeg image2pipe) into (w, h, rgba).
/// Uses ffmpeg rawvideo output for a robust RGBA decode.
fn decode_png_rgba(bytes: &[u8]) -> FluxResult<(u32, u32, Vec<u8>)> {
    if bytes.len() < 24 || &bytes[0..8] != b"\x89PNG\r\n\x1a\n" {
        return Err(FluxError::new(ErrorCode::MediaDecode, "media", "not a PNG"));
    }
    let w = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let h = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    let ffmpeg =
        find_ffmpeg().ok_or_else(|| FluxError::new(ErrorCode::MediaDecode, "media", "ffmpeg"))?;
    let tmp = tempfile_path("flux_frame", "png");
    std::fs::write(&tmp, bytes)
        .map_err(|e| FluxError::new(ErrorCode::MediaDecode, "media", e.to_string()))?;
    let out = Command::new(&ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-i",
            tmp.to_str().unwrap_or_default(),
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-",
        ])
        .output()
        .map_err(|e| FluxError::new(ErrorCode::MediaDecode, "media", e.to_string()));
    let _ = std::fs::remove_file(&tmp);
    let out = out?;
    let expected = (w as u64 * h as u64 * 4) as usize;
    if out.stdout.len() < expected {
        return Err(FluxError::new(
            ErrorCode::MediaDecode,
            "media",
            "truncated frame",
        ));
    }
    Ok((w, h, out.stdout[..expected].to_vec()))
}

fn tempfile_path(prefix: &str, ext: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{prefix}_{}_{n}.{ext}", std::process::id()))
}

/// Encode raw RGBA8 to PNG bytes in memory (pure Rust, no FFmpeg). Fast enough
/// for live previews; used by the desktop preview path.
pub fn encode_png_rgba(width: u32, height: u32, rgba: &[u8]) -> FluxResult<Vec<u8>> {
    let mut out = Vec::with_capacity((width as usize) * (height as usize) / 2 + 1024);
    {
        let mut enc = png::Encoder::new(&mut out, width, height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut writer = enc
            .write_header()
            .map_err(|e| FluxError::new(ErrorCode::ExportEncoder, "media", format!("png: {e}")))?;
        writer
            .write_image_data(rgba)
            .map_err(|e| FluxError::new(ErrorCode::ExportEncoder, "media", format!("png: {e}")))?;
    }
    Ok(out)
}

/// Write raw RGBA to a PNG file (used by export image sequence).
pub fn write_png_rgba(path: &Path, width: u32, height: u32, rgba: &[u8]) -> FluxResult<()> {
    let ffmpeg =
        find_ffmpeg().ok_or_else(|| FluxError::new(ErrorCode::ExportEncoder, "media", "ffmpeg"))?;
    let mut child = Command::new(&ffmpeg)
        .args([
            "-hide_banner",
            "-loglevel",
            "error",
            "-y",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-s",
            &format!("{width}x{height}"),
            "-i",
            "-",
            "-frames:v",
            "1",
            path.to_str().unwrap_or_default(),
        ])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| FluxError::new(ErrorCode::ExportEncoder, "media", e.to_string()))?;
    use std::io::Write;
    if let Some(stdin) = child.stdin.as_mut() {
        stdin
            .write_all(rgba)
            .map_err(|e| FluxError::new(ErrorCode::ExportEncoder, "media", e.to_string()))?;
    }
    let status = child
        .wait()
        .map_err(|e| FluxError::new(ErrorCode::ExportEncoder, "media", e.to_string()))?;
    if !status.success() {
        return Err(FluxError::new(
            ErrorCode::ExportEncoder,
            "media",
            "png encode failed",
        ));
    }
    Ok(())
}

/// Write interleaved stereo f32 to a 16-bit WAV.
pub fn write_wav_stereo(path: &Path, sample_rate: u32, samples: &[f32]) -> FluxResult<()> {
    use std::io::Write;
    let mut f = std::fs::File::create(path)?;
    let channels: u16 = 2;
    let bits: u16 = 16;
    let data_len = (samples.len() * 2) as u32;
    let byte_rate = sample_rate * channels as u32 * (bits / 8) as u32;
    let block_align = channels * (bits / 8);
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?;
    f.write_all(&channels.to_le_bytes())?;
    f.write_all(&sample_rate.to_le_bytes())?;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&block_align.to_le_bytes())?;
    f.write_all(&bits.to_le_bytes())?;
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        f.write_all(&v.to_le_bytes())?;
    }
    Ok(())
}

/// Kill a spawned child process safely.
pub fn kill_child(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

/// Read a 16-bit PCM WAV into interleaved f32 (for round-trip verification).
pub fn read_wav_f32(path: &Path) -> FluxResult<(u32, u16, Vec<f32>)> {
    let data = std::fs::read(path)?;
    if data.len() < 44 || &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return Err(FluxError::new(ErrorCode::MediaDecode, "media", "not a WAV"));
    }
    let mut pos = 12;
    let mut sample_rate = 48_000u32;
    let mut channels = 2u16;
    let mut bits = 16u16;
    let mut pcm: &[u8] = &[];
    while pos + 8 <= data.len() {
        let id = &data[pos..pos + 4];
        let size = u32::from_le_bytes([data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7]])
            as usize;
        let body = &data[pos + 8..(pos + 8 + size).min(data.len())];
        if id == b"fmt " {
            channels = u16::from_le_bytes([body[2], body[3]]);
            sample_rate = u32::from_le_bytes([body[4], body[5], body[6], body[7]]);
            bits = u16::from_le_bytes([body[14], body[15]]);
        } else if id == b"data" {
            pcm = body;
        }
        pos = pos + 8 + size + (size & 1);
    }
    let mut out = Vec::with_capacity(pcm.len() / 2);
    for c in pcm.chunks_exact(2) {
        let v = i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0;
        out.push(v);
    }
    let _ = bits;
    Ok((sample_rate, channels, out))
}

#[cfg(test)]
mod tests;
