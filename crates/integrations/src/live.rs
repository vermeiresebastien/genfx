//! Live video source/sink interfaces for external protocols (NDI, Spout, Syphon)
//! and for capture devices. A runtime that lacks the SDK reports
//! `unavailable` with a reason rather than pretending to work.
//!
//! Implementations are gated on the external SDK being present at build time:
//! - NDI: requires the NDI SDK headers/lib (`FLUX_NDI_SDK`).
//! - Spout (Windows): requires the Spout2 SDK.
//! - Syphon (macOS): requires the Syphon framework.
//! This module defines the transport-agnostic contract and discovery so the rest
//! of the app can enumerate sources and fail gracefully.

use serde::Serialize;
use std::time::Duration;

/// One discovered live source.
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct LiveSourceInfo {
    pub protocol: String,
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub fps: Option<f32>,
}

/// A live source that delivers RGBA8 frames at a nominal rate. Implementations
/// must be non-blocking on the consumer side and never do GPU/file work on an
/// audio thread.
pub trait LiveSource: Send {
    fn info(&self) -> &LiveSourceInfo;
    /// Pull the latest frame as tightly packed RGBA8 (`w*h*4`). Returns None when
    /// no new frame is available within `timeout`.
    fn next_frame(&mut self, timeout: Duration) -> Option<Vec<u8>>;
    fn stop(&mut self);
}

/// A live sink that accepts RGBA8 frames.
pub trait LiveSink: Send {
    fn protocol(&self) -> &str;
    fn send_frame(&mut self, width: u32, height: u32, rgba: &[u8]) -> Result<(), String>;
    fn stop(&mut self);
}

/// Availability of a protocol runtime on this host.
#[derive(Debug, Clone, Serialize)]
pub struct ProtocolStatus {
    pub protocol: String,
    pub available: bool,
    pub reason: String,
}

/// Which external protocol runtimes are present. Determined by build features
/// and, at runtime, by probing the SDK; here we report the compiled-in truth.
pub fn protocol_status() -> Vec<ProtocolStatus> {
    vec![
        ProtocolStatus {
            protocol: "ndi".into(),
            available: cfg!(feature = "ndi"),
            reason: if cfg!(feature = "ndi") {
                "NDI SDK linked".into()
            } else {
                "NDI SDK not present at build (feature `ndi` off)".into()
            },
        },
        ProtocolStatus {
            protocol: "spout".into(),
            available: cfg!(all(windows, feature = "spout")),
            reason: if cfg!(all(windows, feature = "spout")) {
                "Spout2 SDK linked".into()
            } else {
                "Spout2 SDK not present (Windows feature `spout` off)".into()
            },
        },
        ProtocolStatus {
            protocol: "syphon".into(),
            available: cfg!(all(target_os = "macos", feature = "syphon")),
            reason: if cfg!(all(target_os = "macos", feature = "syphon")) {
                "Syphon framework linked".into()
            } else {
                "Syphon requires macOS + feature `syphon`".into()
            },
        },
    ]
}

/// Enumerate live sources for a protocol. With no SDK this returns an empty list
/// (never a fake device).
pub fn discover(_protocol: &str) -> Vec<LiveSourceInfo> {
    // SDK-backed discovery would be wired here behind the corresponding feature.
    Vec::new()
}

/// Frame cadence helper: converts a nominal fps to the inter-frame duration used
/// to pace live capture without wall-clock drift in the render loop.
pub fn frame_interval(fps: f32) -> Duration {
    if fps <= 0.0 {
        Duration::from_millis(16)
    } else {
        Duration::from_secs_f64(1.0 / fps as f64)
    }
}

/// Validate a candidate RGBA frame before handing it to the renderer: exact size,
/// finite bounds. Rejects malformed SDK output rather than uploading garbage.
pub fn validate_frame(width: u32, height: u32, rgba: &[u8]) -> Result<(), String> {
    let expected = (width as usize)
        .checked_mul(height as usize)
        .and_then(|p| p.checked_mul(4))
        .ok_or_else(|| "frame dimensions overflow".to_string())?;
    if rgba.len() != expected {
        return Err(format!(
            "frame size {} != expected {} for {}x{}",
            rgba.len(),
            expected,
            width,
            height
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn statuses_are_truthful_when_sdks_absent() {
        for s in protocol_status() {
            // Without the feature flags these are unavailable by construction.
            if !s.available {
                assert!(!s.reason.is_empty());
            }
        }
    }

    #[test]
    fn discovery_empty_without_sdk() {
        assert!(discover("ndi").is_empty());
        assert!(discover("spout").is_empty());
        assert!(discover("syphon").is_empty());
    }

    #[test]
    fn frame_validation() {
        let ok = vec![0u8; 4 * 4 * 4];
        assert!(validate_frame(4, 4, &ok).is_ok());
        assert!(validate_frame(4, 4, &vec![0u8; 10]).is_err());
    }

    #[test]
    fn frame_interval_positive() {
        assert!(frame_interval(60.0).as_millis() >= 16);
        assert_eq!(frame_interval(0.0), Duration::from_millis(16));
    }
}
