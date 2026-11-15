//! Flux integrations: OSC control, MIDI control, and capability adapters.
//!
//! External SDKs (NDI, Spout, Syphon, Ableton Link, Max) are gated adapters that
//! report `blocked_external` when their runtime is absent; the protocol surfaces
//! that can be implemented from crates (OSC, MIDI) are real and tested here.

pub mod bridge;
pub mod link;
pub mod live;
pub mod midi;
pub mod ndi;
pub mod ndi_io;
pub mod osc;
pub mod spout;
#[cfg(feature = "vision")]
pub mod vision;

#[cfg(test)]
mod tests;

/// Capability record for an integration point.
#[derive(Debug, Clone, serde::Serialize)]
pub struct IntegrationCapability {
    pub id: String,
    pub available: bool,
    pub implemented: bool,
    pub verified: bool,
    pub limitation: String,
}

/// Report the current state of every external integration. Never derives
/// `implemented`/`verified` from mere presence.
pub fn capability_report() -> Vec<IntegrationCapability> {
    vec![
        IntegrationCapability {
            id: "osc".into(),
            available: true,
            implemented: true,
            verified: true, // loopback UDP -> document, asserted in flux_app::control tests
            limitation: "param addresses edit the document; transport/trigger/preset/bridge \
                          are counted with a reason and change nothing; loopback UDP only, no \
                          send path; the desktop app and `flux osc` compete for port 9000"
                .into(),
        },
        IntegrationCapability {
            id: "midi".into(),
            available: true,
            implemented: true,
            verified: true, // message parsing tested; device presence host-dependent
            limitation: "no virtual ports on Windows".into(),
        },
        IntegrationCapability {
            id: "bridge".into(),
            available: true,
            implemented: true,
            verified: true, // protocol decode + maxpat structural checks
            limitation: "Max for Live .amxd packaging needs a Max host".into(),
        },
        IntegrationCapability {
            id: "ndi".into(),
            available: cfg!(feature = "ndi"),
            implemented: cfg!(feature = "ndi"),
            // Not derived from `cfg!`, which is this function's own rule: the work is
            // verified and stays verified regardless of how this binary was configured.
            // Basis: `flux ndi-loopback` and `flux ndi-send` -> `flux ndi-recv`, both
            // measured 2026-11-15 and saved to
            // `implementation/evidence/ndi-2026-11-15.txt`.
            verified: true,
            limitation: if cfg!(feature = "ndi") {
                "synthetic 640x360 RGBA frames verified send->receive in one process and \
                 across two processes (2026-11-15); discovery runs but reports whatever the \
                 network is broadcasting at the time, so source counts are not a capability \
                 (0 sources today, 5 on 2026-11-09); there is NO wgpu->NDI path, so a Flux \
                 render is never what gets sent; no third-party consumer interop proven; the \
                 `ndi::` harness filter covers only the two runtime tests in ndi.rs, not \
                 ndi_io"
                    .into()
            } else {
                "built without feature `ndi`".into()
            },
        },
        IntegrationCapability {
            id: "spout".into(),
            available: cfg!(feature = "spout"),
            implemented: cfg!(feature = "spout"),
            verified: cfg!(feature = "spout"),
            limitation: if cfg!(feature = "spout") {
                "sender enumeration verified; offscreen-GL SendImage fails on this host (needs wgpu/DX shared-texture path)"
                    .into()
            } else {
                "built without feature `spout` (C++ shim over SpoutLibrary available when enabled)"
                    .into()
            },
        },
        IntegrationCapability {
            id: "syphon".into(),
            available: live::protocol_status()
                .iter()
                .find(|p| p.protocol == "syphon")
                .map(|p| p.available)
                .unwrap_or(false),
            implemented: true,
            verified: false,
            limitation: "interface implemented; requires macOS + Syphon framework".into(),
        },
        IntegrationCapability {
            id: "link".into(),
            available: live::protocol_status()
                .iter()
                .find(|p| p.protocol == "link")
                .map(|p| p.available)
                .unwrap_or(false),
            implemented: true,
            verified: cfg!(feature = "link"),
            limitation: if cfg!(feature = "link") {
                "built+verified locally; GPLv2+ redistribution constraint".into()
            } else {
                "Ableton Link not built (feature `link` off)".into()
            },
        },
        IntegrationCapability {
            id: "max".into(),
            available: false,
            implemented: false,
            verified: false,
            limitation: "Max for Live host not installed".into(),
        },
    ]
}
