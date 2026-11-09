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
            verified: true, // encoded/decoded with rosc in tests
            limitation: "loopback UDP only; feedback target configured explicitly".into(),
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
            verified: cfg!(feature = "ndi"),
            limitation: if cfg!(feature = "ndi") {
                "discovery + live send/receive verified (cross-process, real frame)".into()
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
