//! Live control bridge: map OSC/MIDI controls to engine commands, with a learned
//! binding table, value scaling, and rate safety. This is the wiring that makes
//! live controllers edit the authoritative document (not a parallel state).

use flux_domain::{Command, Dispatcher, FluxResult, ParamValue};
use flux_integrations::midi::MidiControl;
use flux_integrations::osc::OscControl;
use std::collections::HashMap;

/// A learned binding from a control source to a node parameter.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ControlBinding {
    /// OSC address path → node/param.
    Osc {
        address: String,
        node_id: String,
        param_id: String,
    },
    /// MIDI CC (channel+cc) → node/param.
    MidiCc {
        channel: u8,
        cc: u8,
        node_id: String,
        param_id: String,
    },
    /// MIDI note on (channel+note) → transport-independent trigger.
    MidiNote {
        channel: u8,
        note: u8,
        node_id: String,
        param_id: String,
    },
}

/// The control map: bindings + a target graph id.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ControlMap {
    pub graph_id: String,
    pub bindings: Vec<ControlBinding>,
}

impl ControlMap {
    pub fn learn_osc(
        &mut self,
        address: impl Into<String>,
        node: impl Into<String>,
        param: impl Into<String>,
    ) {
        self.bindings.push(ControlBinding::Osc {
            address: address.into(),
            node_id: node.into(),
            param_id: param.into(),
        });
    }

    pub fn learn_cc(
        &mut self,
        channel: u8,
        cc: u8,
        node: impl Into<String>,
        param: impl Into<String>,
    ) {
        self.bindings.push(ControlBinding::MidiCc {
            channel,
            cc,
            node_id: node.into(),
            param_id: param.into(),
        });
    }
}

/// Result of applying one control event.
#[derive(Debug, Clone, PartialEq)]
pub enum Applied {
    Parameter {
        node: String,
        param: String,
        value: f32,
    },
    Unmapped,
    Transport,
}

/// Translate an OSC control into a parameter command using the binding table.
pub fn apply_osc(map: &ControlMap, ctrl: &OscControl) -> Option<(String, String, f32)> {
    match ctrl {
        OscControl::Param { node, param, value } => {
            let addr_bound = map.bindings.iter().any(|b| {
                matches!(b, ControlBinding::Osc { node_id, param_id, .. } if node_id == node && param_id == param)
            });
            if addr_bound || map.bindings.is_empty() {
                Some((node.clone(), param.clone(), *value as f32))
            } else {
                None
            }
        }
        _ => None,
    }
}

/// Translate a MIDI control into a parameter command using the binding table.
pub fn apply_midi(map: &ControlMap, ctrl: &MidiControl) -> Option<(String, String, f32)> {
    match ctrl {
        MidiControl::Cc { channel, cc, value } => map.bindings.iter().find_map(|b| match b {
            ControlBinding::MidiCc {
                channel: c,
                cc: k,
                node_id,
                param_id,
            } if c == channel && k == cc => {
                Some((node_id.clone(), param_id.clone(), *value as f32))
            }
            _ => None,
        }),
        MidiControl::NoteOn {
            channel,
            note,
            velocity,
        } => map.bindings.iter().find_map(|b| match b {
            ControlBinding::MidiNote {
                channel: c,
                note: n,
                node_id,
                param_id,
            } if c == channel && n == note => {
                Some((node_id.clone(), param_id.clone(), *velocity as f32))
            }
            _ => None,
        }),
        _ => None,
    }
}

/// Apply a resolved (node,param,value) to the document as a SetParameter command.
/// Missing nodes are ignored (stale binding) rather than erroring the control path.
pub fn apply_to_dispatcher(
    dispatcher: &mut Dispatcher,
    graph_id: &str,
    node: &str,
    param: &str,
    value: f32,
) -> FluxResult<bool> {
    if dispatcher
        .project
        .graphs
        .get(graph_id)
        .and_then(|g| g.node(node))
        .is_none()
    {
        return Ok(false);
    }
    dispatcher.apply(
        Command::SetParameter {
            graph_id: graph_id.to_string(),
            node_id: node.to_string(),
            param_id: param.to_string(),
            value: ParamValue::Float {
                value: value as f64,
            },
        },
        None,
    )?;
    Ok(true)
}

/// Latest-value coalescing store so a burst of control updates yields one edit
/// per parameter per frame (avoids command spam from 60 Hz CC streams).
#[derive(Default)]
pub struct Coalescer {
    pending: HashMap<(String, String), f32>,
    pub applied_last: usize,
}

impl Coalescer {
    pub fn push(&mut self, node: impl Into<String>, param: impl Into<String>, value: f32) {
        self.pending.insert((node.into(), param.into()), value);
    }

    /// Drain pending updates into `dispatcher` for `graph_id`, returning count.
    pub fn flush(&mut self, dispatcher: &mut Dispatcher, graph_id: &str) -> usize {
        let drained: Vec<(String, String, f32)> =
            self.pending.drain().map(|((n, p), v)| (n, p, v)).collect();
        let mut n = 0;
        for (node, param, value) in drained {
            if apply_to_dispatcher(dispatcher, graph_id, &node, &param, value).unwrap_or(false) {
                n += 1;
            }
        }
        self.applied_last = n;
        n
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::samples;

    #[test]
    fn osc_param_applies_when_bound_or_empty() {
        let ctrl = OscControl::Param {
            node: "n".into(),
            param: "p".into(),
            value: 0.5,
        };
        let empty = ControlMap::default();
        assert_eq!(
            apply_osc(&empty, &ctrl),
            Some(("n".into(), "p".into(), 0.5))
        );
        let mut map = ControlMap::default();
        map.learn_osc("/x", "n", "p");
        assert!(apply_osc(&map, &ctrl).is_some());
        let other = OscControl::Param {
            node: "z".into(),
            param: "q".into(),
            value: 0.1,
        };
        assert!(apply_osc(&map, &other).is_none());
    }

    #[test]
    fn midi_cc_and_note_binding() {
        let mut map = ControlMap::default();
        map.learn_cc(0, 7, "n1", "radius");
        assert_eq!(
            apply_midi(
                &map,
                &MidiControl::Cc {
                    channel: 0,
                    cc: 7,
                    value: 0.8
                }
            ),
            Some(("n1".into(), "radius".into(), 0.8))
        );
        assert!(apply_midi(
            &map,
            &MidiControl::Cc {
                channel: 0,
                cc: 8,
                value: 0.8
            }
        )
        .is_none());
    }

    #[test]
    fn applies_parameter_to_real_node() {
        let proj = samples::demo_generator_project("gen.plasma", &["fx.bloom"], 2);
        let mut d = Dispatcher::new(proj);
        let graph_id = d.project.graphs.keys().next().cloned().unwrap();
        // Seed a float param on a real node (params are optional overrides).
        let node_id = {
            let g = d.project.graphs.get_mut(&graph_id).unwrap();
            let n = g.nodes.first_mut().expect("a node");
            n.params
                .insert("intensity".into(), ParamValue::Float { value: 0.5 });
            n.id.clone()
        };
        assert!(apply_to_dispatcher(&mut d, &graph_id, &node_id, "intensity", 0.42).unwrap());
        let stored =
            d.project.graphs[&graph_id].node(&node_id).unwrap().params["intensity"].clone();
        match stored {
            ParamValue::Float { value } => assert!((value - 0.42).abs() < 1e-5),
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn coalescer_merges_duplicates() {
        let proj = samples::demo_generator_project("gen.plasma", &[], 1);
        let mut d = Dispatcher::new(proj);
        let graph_id = d.project.graphs.keys().next().cloned().unwrap();
        let node_id = {
            let g = d.project.graphs.get_mut(&graph_id).unwrap();
            let n = g.nodes.first_mut().unwrap();
            n.params
                .insert("intensity".into(), ParamValue::Float { value: 0.0 });
            n.id.clone()
        };
        let mut c = Coalescer::default();
        for i in 0..10 {
            c.push(node_id.clone(), "intensity".to_string(), i as f32 / 10.0);
        }
        let applied = c.flush(&mut d, &graph_id);
        assert_eq!(applied, 1, "10 pushes coalesce into one edit");
    }

    #[test]
    fn stale_binding_ignored() {
        let proj = samples::demo_generator_project("gen.plasma", &[], 1);
        let mut d = Dispatcher::new(proj);
        let graph_id = d.project.graphs.keys().next().cloned().unwrap();
        assert!(!apply_to_dispatcher(&mut d, &graph_id, "nonexistent", "x", 0.5).unwrap());
    }
}
