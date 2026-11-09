//! Flux integrations: OSC control, MIDI control, and capability adapters.
//!
//! External SDKs (NDI, Spout, Syphon, Ableton Link, Max) are gated adapters that
//! report `blocked_external` when their runtime is absent; the protocol surfaces
//! that can be implemented from crates (OSC, MIDI) are real and tested here.

pub mod bridge;
pub mod live;
pub mod midi;
pub mod osc;

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
            available: live::protocol_status()
                .iter()
                .find(|p| p.protocol == "ndi")
                .map(|p| p.available)
                .unwrap_or(false),
            implemented: true,
            verified: false,
            limitation: "interface implemented; NDI SDK not present at build".into(),
        },
        IntegrationCapability {
            id: "spout".into(),
            available: live::protocol_status()
                .iter()
                .find(|p| p.protocol == "spout")
                .map(|p| p.available)
                .unwrap_or(false),
            implemented: true,
            verified: false,
            limitation: "interface implemented; Spout2 SDK not present at build".into(),
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
            available: false,
            implemented: false,
            verified: false,
            limitation: "Ableton Link license not resolved".into(),
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
