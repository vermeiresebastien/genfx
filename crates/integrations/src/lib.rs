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

/// Report the current state of every external integration.
///
/// `available` and `implemented` describe **this binary**: a feature compiled out is not a
/// capability the running process has, so those two track the build configuration honestly.
/// `verified` does not belong to that rule — it claims a measurement happened, and no build
/// configuration performs one. It is a constant per row, identical in every build, and the
/// row's `limitation` carries the date and the artifact the claim rests on. A prerequisite that
/// is merely absent is `false`, never a pass.
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
            // A constant, not a build artifact, per this function's doc: the transport was
            // measured and stays measured regardless of how this binary was configured.
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
                "built without feature `ndi`; the 2026-11-15 transport measurement \
                 (`implementation/evidence/ndi-2026-11-15.txt`) was made in a `--features ndi` \
                 build, so this process can do no NDI at all"
                    .into()
            },
        },
        IntegrationCapability {
            id: "spout".into(),
            available: cfg!(feature = "spout"),
            implemented: cfg!(feature = "spout"),
            // False in every build. Re-measured 2026-11-15: the shim loads, the bounded
            // enumeration runs and answers 0 senders, and the send path still does not work
            // on this host. A call path that finds nothing to talk to is not verification.
            verified: false,
            limitation: if cfg!(feature = "spout") {
                "sender enumeration re-measured 2026-11-15 \
                 (implementation/evidence/spout-2026-11-15.txt): the shim loads and the bounded \
                 query returns 0 senders without timing out, so the call path works and there is \
                 nothing on this machine to interoperate with; `CreateSender`/`SendImage` fail \
                 through a bare offscreen WGL context, so Flux has no sender at all until a \
                 wgpu/DX11 shared-texture path exists; AT058 (actual Windows Spout send/receive \
                 with an independent sender/consumer) is unmet, and no third-party receiver has \
                 ever confirmed a Flux frame"
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
            // False in every build. The 6 assertions ran on 2026-11-15 in a `link` feature
            // build and passed, but every one is two sessions inside one process, one of them
            // failed on a repeat run, AT063's Max-transport and seek/loop clauses are asserted
            // by nothing in any configuration, and no Ableton Live peer has ever been observed.
            verified: false,
            limitation: if cfg!(feature = "link") {
                "peer discovery, tempo convergence, single-master arbitration, start/stop \
                 propagation, clock advance and enable are asserted by 6 tests that exist only \
                 in a `link` feature build; the group ran 13 times on 2026-11-15 against the \
                 vendored abl_link.lib (implementation/evidence/link-2026-11-15.txt): 11 clean \
                 and 2 failures of `set_playing_reflected_in_state` at link.rs:203, so the gate \
                 is not run-independent; every test holds both sessions in one process, the \
                 2026-11-09 cross-process artifact (evidence/link-a.txt + link-b.txt) shows \
                 peers 0->1 with `playing=false` on every line, so it evidences discovery and \
                 tempo rather than start/stop, and no Live interop has been observed; AT063's \
                 Max-transport and seek/loop clauses are unasserted; GPLv2+ redistribution \
                 constraint"
                    .into()
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
