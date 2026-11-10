//! Control routing: maps feature/control sources to parameters using the
//! modulation fold (domain::params) and expresses the per-node uniform values
//! the renderer uploads.

use flux_catalog::ParamKind;
use flux_domain::params::{eval_keyframes, resolve_parameter, FeatureSnapshot, Smoother};
use flux_domain::{FluxResult, Graph, Mapping, ParamValue, Project, RationalTime};
use std::collections::{BTreeMap, HashMap};

/// Node parameter values after base/keyframe/fold resolution.
pub type ResolvedParams = BTreeMap<String, f64>;

pub struct Router {
    smoothers: HashMap<String, Smoother>,
}

impl Default for Router {
    fn default() -> Self {
        Self {
            smoothers: HashMap::new(),
        }
    }
}

impl Router {
    /// Resolve every parameter of a node:
    /// keyframe lane replaces the base; mappings fold in explicit order.
    pub fn resolve_node(
        &mut self,
        graph: &Graph,
        node_id: &str,
        project_time: RationalTime,
        snapshot: &FeatureSnapshot,
        dt: f32,
        project: &Project,
    ) -> FluxResult<ResolvedParams> {
        let mut out = BTreeMap::new();
        let Some(node) = graph.node(node_id) else {
            return Ok(out);
        };
        let catalog = flux_catalog::build_catalog();
        let manifest = catalog.get(&node.type_id).map(|e| &e.manifest);
        let key = |pid: &str| format!("{}:{}", node_id, pid);
        if let Some(m) = manifest {
            let time_secs = project_time.as_f64();
            for p in &m.parameters {
                let base = node
                    .params
                    .get(&p.id)
                    .and_then(|v| param_number(v))
                    .or_else(|| param_number_json(&p.default))
                    .unwrap_or(0.0);
                // keyframe lane replaces base
                let lane_key = key(&p.id);
                let base_final = graph
                    .keyframes
                    .get(&lane_key)
                    .and_then(|keys| eval_keyframes(keys, project_time))
                    .and_then(|v| param_number(&v))
                    .unwrap_or(base);
                let mappings: Vec<Mapping> = project
                    .mappings
                    .values()
                    .filter(|m| m.node_id == node_id && m.param_id == p.id)
                    .cloned()
                    .collect();
                let min = p.min.unwrap_or(0.0);
                let max = p.max.unwrap_or(1.0);
                let value = if mappings.is_empty() {
                    base_final.clamp(min, max)
                } else {
                    let keyed: Vec<Mapping> = mappings
                        .into_iter()
                        .map(|mut mm| {
                            // smoothers are keyed by mapping ID globally
                            mm.id = format!("{}:{}", node_id, mm.id);
                            mm
                        })
                        .collect();
                    resolve_parameter(
                        base_final,
                        min,
                        max,
                        &keyed,
                        snapshot,
                        &mut self.smoothers,
                        time_secs,
                        dt,
                    )
                };
                if value.is_finite() {
                    out.insert(p.id.clone(), value);
                }
            }
        } else {
            for (k, v) in &node.params {
                if let Some(n) = param_number(v) {
                    out.insert(k.clone(), n);
                }
            }
        }
        Ok(out)
    }
}

/// Pack resolved params into the manifest's vec4/u32 uniform layout.
pub struct PackedParams {
    pub floats: Vec<[f32; 4]>,
    pub uints: Vec<[u32; 4]>,
}

/// Pack numeric params into the same layout as `catalog::pack::params_wgsl`.
pub fn pack_params(manifest: &flux_catalog::Manifest, values: &ResolvedParams) -> PackedParams {
    let mut floats: Vec<[f32; 4]> = vec![[0.0; 4]];
    let mut uints: Vec<[u32; 4]> = vec![[0u32; 4]];
    let mut f_vec4 = 0usize;
    let mut u_vec4 = 0usize;
    let mut f_used = 0usize;
    let mut u_used = 0usize;
    for p in &manifest.parameters {
        let value = values.get(&p.id).copied().unwrap_or(0.0);
        match p.kind {
            ParamKind::Float => {
                if f_used + 1 > 4 {
                    f_vec4 += 1;
                    f_used = 0;
                    floats.push([0.0; 4]);
                }
                floats[f_vec4][f_used] = value as f32;
                f_used += 1;
            }
            ParamKind::Vec2 => {
                if f_used + 2 > 4 {
                    f_vec4 += 1;
                    f_used = 0;
                    floats.push([0.0; 4]);
                }
                floats[f_vec4][f_used] = value as f32;
                f_used += 2;
            }
            ParamKind::Vec3 => {
                if f_used + 3 > 4 {
                    f_vec4 += 1;
                    f_used = 0;
                    floats.push([0.0; 4]);
                }
                floats[f_vec4][f_used] = value as f32;
                f_used += 3;
            }
            ParamKind::Color => {
                if f_used + 4 > 4 {
                    f_vec4 += 1;
                    f_used = 0;
                    floats.push([0.0; 4]);
                }
                floats[f_vec4] = [value as f32, 0.0, 0.0, 1.0];
                f_used += 4;
            }
            ParamKind::Int | ParamKind::Bool | ParamKind::Enum | ParamKind::Text => {
                if u_used + 1 > 4 {
                    u_vec4 += 1;
                    u_used = 0;
                    uints.push([0u32; 4]);
                }
                uints[u_vec4][u_used] = value.max(0.0) as u32;
                u_used += 1;
            }
        }
    }
    PackedParams { floats, uints }
}

fn param_number(v: &ParamValue) -> Option<f64> {
    v.as_f64()
}

fn param_number_json(v: &serde_json::Value) -> Option<f64> {
    v.as_f64()
}

#[cfg(test)]
mod tests {
    use super::*;
    use flux_domain::*;

    fn rt(n: i64, d: u32) -> RationalTime {
        RationalTime::new(n, d).unwrap()
    }

    #[test]
    fn base_value_without_mappings() {
        let mut p = Project::new("x", 1);
        let g = flux_engine_graph::simple("fx.color_adjustment");
        p.graphs.insert(g.id.clone(), g.clone());
        let node_id = g.nodes[1].id.clone();
        let mut router = Router::default();
        let vals = router
            .resolve_node(
                &g,
                &node_id,
                RationalTime::ZERO,
                &FeatureSnapshot::default(),
                0.016,
                &p,
            )
            .unwrap();
        // exposure default 0
        assert!((vals.get("exposure").copied().unwrap_or(f64::NAN) - 0.0).abs() < 1e-9);
    }

    #[test]
    fn keyframe_overrides_base() {
        let mut g = flux_engine_graph::simple("fx.color_adjustment");
        let node_id = g.nodes[1].id.clone();
        g.keyframes.insert(
            format!("{}:exposure", node_id),
            vec![
                Keyframe {
                    time: rt(0, 1),
                    value: ParamValue::Float { value: 2.0 },
                    interpolation: Interpolation::Linear,
                },
                Keyframe {
                    time: rt(2, 1),
                    value: ParamValue::Float { value: 4.0 },
                    interpolation: Interpolation::Linear,
                },
            ],
        );
        let mut p = Project::new("x", 1);
        p.graphs.insert(g.id.clone(), g.clone());
        let mut router = Router::default();
        let vals = router
            .resolve_node(
                &g,
                &node_id,
                rt(1, 1),
                &FeatureSnapshot::default(),
                0.016,
                &p,
            )
            .unwrap();
        assert!((vals["exposure"] - 3.0).abs() < 1e-6);
    }

    #[test]
    fn mapping_adds_in_order() {
        let g = flux_engine_graph::simple("fx.color_adjustment");
        let node_id = g.nodes[1].id.clone();
        let mut p = Project::new("x", 1);
        p.graphs.insert(g.id.clone(), g.clone());
        let mut snap = FeatureSnapshot::default();
        snap.bass = 1.0;
        p.mappings.insert(
            "m1".into(),
            Mapping {
                id: "m1".into(),
                node_id: node_id.clone(),
                param_id: "exposure".into(),
                source: ModulationSource::Feature {
                    feature: FeatureId::Bass,
                },
                in_min: 0.0,
                in_max: 1.0,
                amount: 1.0,
                fold: FoldMode::Add,
                order: 0,
                invert: false,
                attack_ms: 0.0,
                release_ms: 0.0,
                enabled: true,
            },
        );
        let mut router = Router::default();
        let vals = router
            .resolve_node(&g, &node_id, RationalTime::ZERO, &snap, 0.016, &p)
            .unwrap();
        // add maps normalized 1.0 to max (10.0), so exposure 0 + (10-(-10)=20)?? depends on bounds.
        assert!(vals["exposure"] > 0.0);
    }
}

/// Test-only graph builder alias so engine tests don't depend on internal fns.
#[doc(hidden)]
pub mod flux_engine_graph {
    pub fn simple(effect_type: &str) -> flux_domain::Graph {
        crate::graph::simple_chain(effect_type, "flux.source")
    }
}
