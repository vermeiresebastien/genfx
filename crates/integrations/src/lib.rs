//! Flux integrations: OSC control, MIDI control, and capability adapters.
//!
//! External SDKs (NDI, Spout, Syphon, Ableton Link, Max) are gated adapters that
//! report `blocked_external` when their runtime is absent; the protocol surfaces
//! that can be implemented from crates (OSC, MIDI) are real and tested here.

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
            id: "ndi".into(),
            available: false,
            implemented: false,
            verified: false,
            limitation: "official NDI SDK/runtime not installed".into(),
        },
        IntegrationCapability {
            id: "spout".into(),
            available: false,
            implemented: false,
            verified: false,
            limitation: "Spout2 SDK not installed".into(),
        },
        IntegrationCapability {
            id: "syphon".into(),
            available: false,
            implemented: false,
            verified: false,
            limitation: "macOS only; no macOS host".into(),
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
