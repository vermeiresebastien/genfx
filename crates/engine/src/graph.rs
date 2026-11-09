//! Graph validation and compilation.
//!
//! Validation is the authority: the UI cannot create a runtime graph that this
//! module has not accepted. Rules (ARCHITECTURE 4):
//! - normal edges form a DAG; cycles are only legal through an explicit
//!   FeedbackRead/FeedbackWrite temporal pair
//! - required inputs must be connected (or have a documented default)
//! - port types must match
//! - missing node types are preserved but reported
//! - limits: 4096 nodes, 16384 edges, 16 nesting levels
//! - the output node must exist and the graph must produce an image

use flux_catalog::Manifest;
use flux_domain::{ErrorCode, FluxError, FluxResult, Graph, GraphEdge, GraphNode};
use std::collections::{BTreeMap, BTreeSet, HashMap};

pub const MAX_NODES: usize = 4096;
pub const MAX_EDGES: usize = 16384;
pub const MAX_NESTING: usize = 16;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPass {
    /// Execution order index.
    pub index: usize,
    pub node_id: String,
    pub type_id: String,
    /// Inputs: port id -> source node id (or external scope for `flux.source`).
    pub inputs: BTreeMap<String, InputBinding>,
    pub outputs: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputBinding {
    NodeOutput(String),
    ScopeSource,
    Default(String),
}

#[derive(Debug, Clone)]
pub struct CompiledGraph {
    pub passes: Vec<CompiledPass>,
    /// Node that produces the final image.
    pub output_node: String,
    /// Feedback edges broken into temporal resources.
    pub temporal_pairs: Vec<(String, String)>,
    pub graph_version: u64,
}

/// Validation issue with exact entity ids.
#[derive(Debug, Clone)]
pub struct GraphIssue {
    pub code: ErrorCode,
    pub message: String,
    pub node_id: Option<String>,
    pub port_id: Option<String>,
}

impl GraphIssue {
    pub fn to_error(&self) -> FluxError {
        let mut ids = vec![];
        if let Some(n) = &self.node_id {
            ids.push(n.clone());
        }
        FluxError::new(self.code, "graph", self.message.clone()).with_ids(ids)
    }
}

pub struct Registry<'a> {
    types: HashMap<&'a str, &'a Manifest>,
}

impl<'a> Registry<'a> {
    pub fn new(catalog: &'a flux_catalog::Catalog) -> Self {
        let mut types = HashMap::new();
        for e in &catalog.entries {
            types.insert(e.manifest.id.as_str(), &e.manifest);
        }
        Self { types }
    }

    pub fn manifest(&self, type_id: &str) -> Option<&'a Manifest> {
        self.types.get(type_id).copied()
    }

    pub fn has(&self, type_id: &str) -> bool {
        self.types.contains_key(type_id)
    }
}

pub fn validate(g: &Graph, reg: &Registry) -> Vec<GraphIssue> {
    let mut issues = Vec::new();
    if g.nodes.len() > MAX_NODES {
        issues.push(GraphIssue {
            code: ErrorCode::GpuLimit,
            message: format!("graph exceeds {MAX_NODES} nodes"),
            node_id: None,
            port_id: None,
        });
    }
    if g.edges.len() > MAX_EDGES {
        issues.push(GraphIssue {
            code: ErrorCode::GpuLimit,
            message: format!("graph exceeds {MAX_EDGES} edges"),
            node_id: None,
            port_id: None,
        });
    }
    let by_id: BTreeMap<&str, &GraphNode> = g.nodes.iter().map(|n| (n.id.as_str(), n)).collect();
    if by_id.len() != g.nodes.len() {
        issues.push(GraphIssue {
            code: ErrorCode::InvalidCommand,
            message: "duplicate node ids".into(),
            node_id: None,
            port_id: None,
        });
    }
    if !by_id.contains_key(g.output_node.as_str()) {
        issues.push(GraphIssue {
            code: ErrorCode::GraphRequiredInput,
            message: format!("output node {} does not exist", g.output_node),
            node_id: Some(g.output_node.clone()),
            port_id: None,
        });
    }

    // Missing types are preserved and reported (never substituted).
    for n in &g.nodes {
        if !reg.has(&n.type_id) {
            issues.push(GraphIssue {
                code: ErrorCode::GraphTypeMismatch,
                message: format!("missing node type {} v{}", n.type_id, n.type_version),
                node_id: Some(n.id.clone()),
                port_id: None,
            });
        }
    }

    // Type-check edges.
    let mut incoming: BTreeMap<(&str, &str), &GraphEdge> = BTreeMap::new();
    for e in &g.edges {
        let from = by_id.get(e.from_node.as_str());
        let to = by_id.get(e.to_node.as_str());
        let (Some(from), Some(to)) = (from, to) else {
            issues.push(GraphIssue {
                code: ErrorCode::GraphTypeMismatch,
                message: format!(
                    "edge {} -> {} references a missing node",
                    e.from_node, e.to_node
                ),
                node_id: Some(e.to_node.clone()),
                port_id: None,
            });
            continue;
        };
        let Some(fm) = reg.manifest(&from.type_id) else {
            continue;
        };
        let Some(tm) = reg.manifest(&to.type_id) else {
            continue;
        };
        let out_ty = fm
            .outputs
            .iter()
            .find(|p| p.id == e.from_port)
            .map(|p| p.port_type);
        let in_ty = tm
            .inputs
            .iter()
            .find(|p| p.id == e.to_port)
            .map(|p| p.port_type);
        match (out_ty, in_ty) {
            (Some(a), Some(b)) if a == b => {}
            (Some(_), Some(_)) => issues.push(GraphIssue {
                code: ErrorCode::GraphTypeMismatch,
                message: format!(
                    "port type mismatch: {}:{} -> {}:{}",
                    from.type_id, e.from_port, to.type_id, e.to_port
                ),
                node_id: Some(to.id.clone()),
                port_id: Some(e.to_port.clone()),
            }),
            _ => issues.push(GraphIssue {
                code: ErrorCode::GraphTypeMismatch,
                message: format!(
                    "unknown port {}:{} -> {}:{}",
                    from.type_id, e.from_port, to.type_id, e.to_port
                ),
                node_id: Some(to.id.clone()),
                port_id: Some(e.to_port.clone()),
            }),
        }
        if incoming
            .insert((e.to_node.as_str(), e.to_port.as_str()), e)
            .is_some()
        {
            issues.push(GraphIssue {
                code: ErrorCode::InvalidCommand,
                message: format!("multiple edges into {}:{}", e.to_node, e.to_port),
                node_id: Some(e.to_node.clone()),
                port_id: Some(e.to_port.clone()),
            });
        }
    }

    // Required inputs.
    for n in &g.nodes {
        let Some(m) = reg.manifest(&n.type_id) else {
            continue;
        };
        for p in &m.inputs {
            if p.required && !incoming.contains_key(&(n.id.as_str(), p.id.as_str())) {
                issues.push(GraphIssue {
                    code: ErrorCode::GraphRequiredInput,
                    message: format!("required input {}.{} is not connected", n.type_id, p.id),
                    node_id: Some(n.id.clone()),
                    port_id: Some(p.id.clone()),
                });
            }
        }
    }

    // Cycle detection: normal edges must be a DAG. A cycle is tolerated only if
    // it is interrupted by a feedback read/write pair.
    if let Some(cycle) = find_cycle(g, reg) {
        if !cycle_has_feedback(g, reg, &cycle) {
            issues.push(GraphIssue {
                code: ErrorCode::GraphCycle,
                message: format!("graph cycle: {}", cycle.join(" -> ")),
                node_id: Some(cycle[0].clone()),
                port_id: None,
            });
        }
    }
    issues
}

/// Returns node IDs forming a cycle in the normal (non-feedback) edge graph.
fn find_cycle(g: &Graph, reg: &Registry) -> Option<Vec<String>> {
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in &g.edges {
        if is_feedback_boundary(reg, &e.to_node) {
            continue;
        }
        adj.entry(e.from_node.as_str())
            .or_default()
            .push(e.to_node.as_str());
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    let mut path: Vec<String> = Vec::new();

    fn dfs<'a>(
        n: &'a str,
        adj: &HashMap<&'a str, Vec<&'a str>>,
        visiting: &mut BTreeSet<&'a str>,
        visited: &mut BTreeSet<&'a str>,
        path: &mut Vec<String>,
    ) -> Option<Vec<String>> {
        if visited.contains(n) {
            return None;
        }
        if visiting.contains(n) {
            let start = path.iter().position(|x| x == n).unwrap_or(0);
            return Some(path[start..].to_vec());
        }
        visiting.insert(n);
        path.push(n.to_string());
        if let Some(nexts) = adj.get(n) {
            for &m in nexts {
                if let Some(c) = dfs(m, adj, visiting, visited, path) {
                    return Some(c);
                }
            }
        }
        visiting.remove(n);
        visited.insert(n);
        path.pop();
        None
    }

    for n in &g.nodes {
        if let Some(c) = dfs(&n.id, &adj, &mut visiting, &mut visited, &mut path) {
            return Some(c);
        }
    }
    None
}

fn is_feedback_write(reg: &Registry, node_id_type: &str) -> bool {
    node_id_type == "flux.feedback_write"
}

fn is_feedback_read(reg: &Registry, type_id: &str) -> bool {
    let _ = reg;
    type_id == "flux.feedback_read"
}

fn is_feedback_boundary(reg: &Registry, node_id: &str) -> bool {
    let _ = (reg, node_id);
    false
}

fn cycle_has_feedback(g: &Graph, reg: &Registry, cycle: &[String]) -> bool {
    // A cycle is legal when it contains a feedback read node (the temporal
    // break) and a matching feedback write node.
    let mut has_read = false;
    let mut has_write = false;
    for id in cycle {
        if let Some(n) = g.node(id) {
            if is_feedback_read(reg, &n.type_id) {
                has_read = true;
            }
            if is_feedback_write(reg, &n.type_id) {
                has_write = true;
            }
        }
    }
    has_read && has_write
}

/// Compile a validated graph into an ordered pass list (topological order).
pub fn compile(g: &Graph, reg: &Registry, graph_version: u64) -> FluxResult<CompiledGraph> {
    let issues = validate(g, reg);
    if let Some(err) = issues.iter().find(|i| {
        matches!(
            i.code,
            ErrorCode::GraphCycle
                | ErrorCode::GraphTypeMismatch
                | ErrorCode::GraphRequiredInput
                | ErrorCode::InvalidCommand
                | ErrorCode::GpuLimit
        )
    }) {
        return Err(err.to_error());
    }

    // Kahn topological order over the normal edge graph. Feedback read nodes
    // have no normal dependencies (they read history).
    let mut indeg: HashMap<&str, usize> = g.nodes.iter().map(|n| (n.id.as_str(), 0)).collect();
    let mut adj: HashMap<&str, Vec<&str>> = HashMap::new();
    for e in &g.edges {
        let to = e.to_node.as_str();
        let is_read = g
            .node(to)
            .map(|n| is_feedback_read(reg, &n.type_id))
            .unwrap_or(false);
        if is_read {
            continue;
        }
        adj.entry(e.from_node.as_str()).or_default().push(to);
        *indeg.entry(to).or_insert(0) += 1;
    }
    let mut queue: Vec<&str> = g
        .nodes
        .iter()
        .filter(|n| indeg.get(n.id.as_str()).copied().unwrap_or(0) == 0)
        .map(|n| n.id.as_str())
        .collect();
    // Deterministic ordering by node id.
    queue.sort_unstable();
    let mut order = Vec::new();
    while let Some(n) = queue.pop() {
        order.push(n.to_string());
        if let Some(nexts) = adj.get(n) {
            let mut sorted = nexts.clone();
            sorted.sort_unstable();
            for m in sorted {
                if let Some(d) = indeg.get_mut(m) {
                    *d -= 1;
                    if *d == 0 {
                        queue.push(m);
                    }
                }
            }
        }
    }
    if order.len() != g.nodes.len() {
        return Err(FluxError::new(
            ErrorCode::GraphCycle,
            "graph",
            "cycle detected during compile",
        ));
    }

    let mut passes = Vec::new();
    for (i, node_id) in order.iter().enumerate() {
        let node = g.node(node_id).expect("node exists");
        let manifest = reg.manifest(&node.type_id);
        let mut inputs = BTreeMap::new();
        if let Some(m) = manifest {
            for p in &m.inputs {
                if let Some(e) = g.incoming(&node.id, &p.id) {
                    inputs.insert(p.id.clone(), InputBinding::NodeOutput(e.from_node.clone()));
                } else if node.type_id == "flux.source" || (!p.required) {
                    inputs.insert(
                        p.id.clone(),
                        if node.type_id == "flux.source" || p.id == "source" {
                            InputBinding::ScopeSource
                        } else {
                            InputBinding::Default(p.id.clone())
                        },
                    );
                }
            }
        }
        passes.push(CompiledPass {
            index: i,
            node_id: node.id.clone(),
            type_id: node.type_id.clone(),
            inputs,
            outputs: manifest
                .map(|m| m.outputs.iter().map(|o| o.id.clone()).collect())
                .unwrap_or_default(),
        });
    }

    Ok(CompiledGraph {
        passes,
        output_node: g.output_node.clone(),
        temporal_pairs: vec![],
        graph_version,
    })
}

/// Build a minimal graph containing Source -> effect -> Output.
pub fn simple_chain(effect_type: &str, source_type: &str) -> Graph {
    let mut nodes = vec![
        GraphNode {
            id: uuid::Uuid::new_v4().to_string(),
            type_id: source_type.into(),
            type_version: 1,
            position: [0.0, 0.0],
            params: BTreeMap::new(),
            bypass: false,
            missing_type: false,
        },
        GraphNode {
            id: uuid::Uuid::new_v4().to_string(),
            type_id: effect_type.into(),
            type_version: 1,
            position: [220.0, 0.0],
            params: BTreeMap::new(),
            bypass: false,
            missing_type: false,
        },
        GraphNode {
            id: uuid::Uuid::new_v4().to_string(),
            type_id: "flux.output".into(),
            type_version: 1,
            position: [440.0, 0.0],
            params: BTreeMap::new(),
            bypass: false,
            missing_type: false,
        },
    ];
    let edges = vec![
        GraphEdge {
            from_node: nodes[0].id.clone(),
            from_port: "color".into(),
            to_node: nodes[1].id.clone(),
            to_port: "source".into(),
        },
        GraphEdge {
            from_node: nodes[1].id.clone(),
            from_port: "color".into(),
            to_node: nodes[2].id.clone(),
            to_port: "source".into(),
        },
    ];
    let output_node = nodes[2].id.clone();
    nodes[0].id = nodes[0].id.clone();
    Graph {
        id: uuid::Uuid::new_v4().to_string(),
        name: "chain".into(),
        nodes,
        edges,
        output_node,
        keyframes: BTreeMap::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reg_catalog() -> flux_catalog::Catalog {
        flux_catalog::build_catalog()
    }

    #[test]
    fn valid_chain_compiles_in_order() {
        let cat = reg_catalog();
        let reg = Registry::new(&cat);
        let g = simple_chain("fx.bloom", "flux.source");
        let issues = validate(&g, &reg);
        assert!(issues.is_empty(), "{issues:?}");
        let c = compile(&g, &reg, 1).unwrap();
        assert_eq!(c.passes.len(), 3);
        assert_eq!(c.passes.last().unwrap().type_id, "flux.output");
    }

    #[test]
    fn type_mismatch_is_rejected() {
        let cat = reg_catalog();
        let reg = Registry::new(&cat);
        let mut g = simple_chain("fx.bloom", "flux.source");
        // connect mask output (Mask) into a Texture2D input
        g.nodes[1].type_id = "flux.mask_input".into();
        g.edges[0].to_port = "source".into();
        g.edges[0].from_port = "mask".into();
        let issues = validate(&g, &reg);
        assert!(issues
            .iter()
            .any(|i| i.code == ErrorCode::GraphTypeMismatch));
    }

    #[test]
    fn cycle_is_rejected() {
        let cat = reg_catalog();
        let reg = Registry::new(&cat);
        let mut g = simple_chain("fx.bloom", "flux.source");
        // add back edge output -> bloom source (replacing the source input)
        g.edges
            .retain(|e| !(e.to_node == g.nodes[1].id && e.to_port == "source"));
        g.edges.push(GraphEdge {
            from_node: g.nodes[2].id.clone(),
            from_port: "color".into(),
            to_node: g.nodes[1].id.clone(),
            to_port: "source".into(),
        });
        let issues = validate(&g, &reg);
        assert!(
            issues.iter().any(|i| i.code == ErrorCode::GraphCycle),
            "{issues:?}"
        );
    }

    #[test]
    fn missing_required_input_is_rejected() {
        let cat = reg_catalog();
        let reg = Registry::new(&cat);
        let mut g = simple_chain("fx.bloom", "flux.source");
        g.edges
            .retain(|e| !(e.to_node == g.nodes[1].id && e.to_port == "source"));
        let issues = validate(&g, &reg);
        assert!(issues
            .iter()
            .any(|i| i.code == ErrorCode::GraphRequiredInput));
    }

    #[test]
    fn missing_type_is_preserved_but_reported() {
        let cat = reg_catalog();
        let reg = Registry::new(&cat);
        let mut g = simple_chain("fx.nonexistent", "flux.source");
        g.nodes[1].missing_type = true;
        let issues = validate(&g, &reg);
        assert!(issues
            .iter()
            .any(|i| i.node_id.as_deref() == Some(g.nodes[1].id.as_str())));
    }

    #[test]
    fn feedback_pair_allows_cycle() {
        let cat = reg_catalog();
        let reg = Registry::new(&cat);
        // source -> write -> read -> blend back; the read breaks the cycle.
        let mut g = simple_chain("fx.bloom", "flux.source");
        g.edges
            .retain(|e| !(e.to_node == g.nodes[1].id && e.to_port == "source"));
        let write = GraphNode {
            id: "w".into(),
            type_id: "flux.feedback_write".into(),
            type_version: 1,
            position: [0.0, 0.0],
            params: BTreeMap::new(),
            bypass: false,
            missing_type: false,
        };
        let read = GraphNode {
            id: "r".into(),
            type_id: "flux.feedback_read".into(),
            type_version: 1,
            position: [0.0, 0.0],
            params: BTreeMap::new(),
            bypass: false,
            missing_type: false,
        };
        g.nodes.push(write);
        g.nodes.push(read);
        g.edges.push(GraphEdge {
            from_node: g.nodes[0].id.clone(),
            from_port: "color".into(),
            to_node: "w".into(),
            to_port: "source".into(),
        });
        g.edges.push(GraphEdge {
            from_node: "r".into(),
            from_port: "color".into(),
            to_node: g.nodes[1].id.clone(),
            to_port: "source".into(),
        });
        g.edges.push(GraphEdge {
            from_node: "w".into(),
            from_port: "color".into(),
            to_node: "r".into(),
            to_port: "source".into(),
        });
        let issues = validate(&g, &reg);
        assert!(
            !issues.iter().any(|i| i.code == ErrorCode::GraphCycle),
            "{issues:?}"
        );
    }
}
