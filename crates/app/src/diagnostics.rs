//! Diagnostics: a redacted, machine-readable report of capabilities, counters,
//! and errors suitable for a support bundle. User paths and device identifiers
//! are redacted by default. No media payloads or secrets are ever included, and
//! no network telemetry is sent.

use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityState {
    pub component: String,
    pub platform: String,
    pub available: bool,
    pub implemented: bool,
    pub verified: bool,
    pub limitation: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsReport {
    pub app_version: String,
    pub spec_version: String,
    pub os: String,
    pub arch: String,
    pub gpu: Option<serde_json::Value>,
    pub media: Option<serde_json::Value>,
    pub capabilities: Vec<CapabilityState>,
    pub redactions: Vec<String>,
    pub counters: BTreeMap<String, u64>,
}

/// Redact a filesystem path: keep only the last component's extension context.
pub fn redact_path(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|f| format!("<redacted>/{}", f.to_string_lossy()))
        .unwrap_or_else(|| "<redacted>".into())
}

/// Redact a device id (GUID/UUID-like) to a stable short alphanumeric form.
pub fn redact_device(id: &str) -> String {
    let alnum: String = id.chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    let tail: String = alnum
        .chars()
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if tail.is_empty() {
        "<device>".into()
    } else {
        tail
    }
}

/// Build a redacted diagnostics report. `paths` and `device_ids` passed in are
/// redacted, not echoed.
pub fn build_report(paths: &[String], device_ids: &[String]) -> DiagnosticsReport {
    let gpu = flux_render::gpu::Gpu::new(flux_render::BackendPref::Auto)
        .ok()
        .map(|g| g.info.to_json());
    let media = flux_media::MediaWorker::new().ok().map(|w| {
        let c = w.capabilities();
        serde_json::json!({
            "h264": c.h264, "hevc": c.hevc, "prores": c.prores, "aac": c.aac,
            "gif": c.gif, "png": c.png, "wav": c.wav
        })
    });
    let capture = flux_platform::capture_capabilities();
    let mut caps = vec![
        CapabilityState {
            component: "render".into(),
            platform: std::env::consts::OS.into(),
            available: gpu.is_some(),
            implemented: true,
            verified: gpu.is_some(),
            limitation: String::new(),
        },
        CapabilityState {
            component: "media".into(),
            platform: std::env::consts::OS.into(),
            available: media.is_some(),
            implemented: true,
            verified: media.is_some(),
            limitation: if media.is_some() {
                String::new()
            } else {
                "FFmpeg not found".into()
            },
        },
        CapabilityState {
            component: "camera".into(),
            platform: capture.platform.clone(),
            available: capture.camera_available,
            implemented: true,
            verified: false,
            limitation: capture.limitation.clone(),
        },
        CapabilityState {
            component: "loopback".into(),
            platform: capture.platform.clone(),
            available: capture.loopback_available,
            implemented: true,
            verified: false,
            limitation: capture.limitation.clone(),
        },
    ];
    for i in flux_integrations::capability_report() {
        caps.push(CapabilityState {
            component: i.id,
            platform: "cross".into(),
            available: i.available,
            implemented: i.implemented,
            verified: i.verified,
            limitation: i.limitation,
        });
    }
    let mut redactions = vec![];
    for p in paths {
        redactions.push(redact_path(p));
    }
    for d in device_ids {
        redactions.push(redact_device(d));
    }
    DiagnosticsReport {
        app_version: env!("CARGO_PKG_VERSION").into(),
        spec_version: "2.0".into(),
        os: std::env::consts::OS.into(),
        arch: std::env::consts::ARCH.into(),
        gpu,
        media,
        capabilities: caps,
        redactions,
        counters: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn path_redaction_hides_dirs() {
        let r = redact_path("C:/Users/secret/Media/clip.mp4");
        assert!(r.contains("clip.mp4"));
        assert!(!r.contains("secret"));
        assert!(!r.contains("Users"));
    }

    #[test]
    fn device_redaction_short() {
        let r = redact_device("{0.0.1.00000000}.{0839dd8c-dc88-4c7b-aa85-452070e0ef5e}");
        assert!(r.len() <= 10);
    }

    #[test]
    fn report_has_no_paths_or_secrets() {
        let report = build_report(
            &["C:/Users/x/secret/file.mp4".into()],
            &["device-abcdef123456".into()],
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("secret"));
        assert!(json.contains("redacted"));
        assert!(report.capabilities.iter().any(|c| c.component == "render"));
        // No telemetry flag: report is a local value only.
        assert!(!json.contains("http"));
    }
}
