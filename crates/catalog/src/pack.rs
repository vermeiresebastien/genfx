//! Shared WGSL preamble and helpers.
//!
//! Every generated shader shares:
//! - `FrameUniform` (96 bytes, 16-byte aligned blocks; CONTRACTS render ABI)
//! - `NodeParams` (uniform, manifest-order packing; see `pack.rs`)
//! - input texture bindings in group 1, derived from the manifest
//!
//! Shader code reads parameters through small generated accessors so the WGSL
//! and CPU side cannot drift: both derive from the manifest.

/// ABI version for generated shaders.
pub const SHADER_ABI_VERSION: u32 = 1;

/// Which slot type a parameter occupies in `NodeParams`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotKind {
    F32,
    Vec2,
    Vec3,
    Vec4,
    U32,
}

/// Compute the packed layout of a manifest's parameters for the WGSL uniform.
///
/// Rule (CONTRACTS render ABI): floats/colors/vectors use `f32` vec4 slots,
/// booleans/enums/integers use `u32` vec4 slots. Layout is manifest order.
#[derive(Debug, Clone, PartialEq)]
pub struct ParamLayout {
    pub slots: Vec<ParamSlot>,
    pub vec4_count: usize,
    /// Number of vec4 slots in the `f` array.
    pub f_count: usize,
    /// Number of vec4 slots in the `u` array.
    pub u_count: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParamSlot {
    pub id: String,
    pub kind: SlotKind,
    /// Index of the vec4 within the uniform array.
    pub vec4_index: usize,
    /// Component index within the vec4 for scalars.
    pub component: usize,
    /// True for boolean parameters (accessor returns WGSL `bool`).
    pub is_bool: bool,
}

impl ParamLayout {
    pub fn byte_size(&self) -> u64 {
        self.vec4_count as u64 * 16
    }
}

/// Generate the WGSL `NodeParams` struct and accessor functions for a set of
/// parameter descriptors. Returns (struct/accessor source, layout).
pub fn params_wgsl(params: &[crate::manifest::ParamDecl]) -> (String, ParamLayout) {
    let mut src = String::new();
    let mut layout = ParamLayout {
        slots: vec![],
        vec4_count: 0,
        f_count: 0,
        u_count: 0,
    };
    let mut f32_components_used = 0usize;
    let mut u32_components_used = 0usize;
    let mut f32_vec4 = 0usize;
    let mut u32_vec4 = 0usize;

    for p in params {
        use crate::manifest::ParamKind::*;
        let slot_kind = match p.kind {
            Float => SlotKind::F32,
            Color => SlotKind::Vec4,
            Vec2 => SlotKind::Vec2,
            Vec3 => SlotKind::Vec3,
            Int | Bool | Enum => SlotKind::U32,
        };
        let width = match slot_kind {
            SlotKind::F32 => 1,
            SlotKind::Vec2 => 2,
            SlotKind::Vec3 => 3,
            SlotKind::Vec4 => 4,
            SlotKind::U32 => 1,
        };
        let (vec4_index, component) = match slot_kind {
            SlotKind::U32 => {
                if u32_components_used + width > 4 {
                    u32_vec4 += 1;
                    u32_components_used = 0;
                }
                let idx = u32_vec4;
                let comp = u32_components_used;
                u32_components_used += width;
                (idx, comp)
            }
            _ => {
                if f32_components_used + width > 4 {
                    f32_vec4 += 1;
                    f32_components_used = 0;
                }
                let idx = f32_vec4;
                let comp = f32_components_used;
                f32_components_used += width;
                (idx, comp)
            }
        };
        let component = component;
        let is_bool = matches!(p.kind, Bool);
        layout.slots.push(ParamSlot {
            id: p.id.clone(),
            kind: slot_kind,
            vec4_index,
            component,
            is_bool,
        });
    }
    // Uniform is padded: one array for floats, then one array for ints.
    let f_count = if f32_components_used > 0 {
        f32_vec4 + 1
    } else {
        f32_vec4
    };
    let u_count = if u32_components_used > 0 {
        u32_vec4 + 1
    } else {
        u32_vec4
    };
    layout.vec4_count = f_count + u_count;
    layout.f_count = f_count.max(1);
    layout.u_count = u_count.max(1);

    src.push_str("struct NodeParams {\n");
    src.push_str(&format!("  f: array<vec4<f32>, {}>,\n", f_count.max(1)));
    src.push_str(&format!("  u: array<vec4<u32>, {}>,\n", u_count.max(1)));
    src.push_str("};\n\n");

    // Accessors reference the local `np` variable declared by each shader.
    for slot in &layout.slots {
        match slot.kind {
            SlotKind::F32 => {
                src.push_str(&format!(
                    "fn p_{id}() -> f32 {{ return np.f[{}].{}; }}\n",
                    slot.vec4_index,
                    comp_name(slot.component),
                    id = slot.id
                ));
            }
            SlotKind::Vec2 => {
                let (x, y) = (comp_name(slot.component), comp_name(slot.component + 1));
                src.push_str(&format!(
                    "fn p_{id}() -> vec2<f32> {{ return np.f[{}].{x}{y}; }}\n",
                    slot.vec4_index,
                    id = slot.id
                ));
            }
            SlotKind::Vec3 => {
                let (x, y, z) = (
                    comp_name(slot.component),
                    comp_name(slot.component + 1),
                    comp_name(slot.component + 2),
                );
                src.push_str(&format!(
                    "fn p_{id}() -> vec3<f32> {{ return np.f[{}].{x}{y}{z}; }}\n",
                    slot.vec4_index,
                    id = slot.id
                ));
            }
            SlotKind::Vec4 => {
                src.push_str(&format!(
                    "fn p_{id}() -> vec4<f32> {{ return np.f[{}]; }}\n",
                    slot.vec4_index,
                    id = slot.id
                ));
            }
            SlotKind::U32 => {
                if slot.is_bool {
                    src.push_str(&format!(
                        "fn p_{id}() -> bool {{ return np.u[{}].{} != 0; }}\n",
                        slot.vec4_index,
                        comp_name(slot.component),
                        id = slot.id
                    ));
                } else {
                    // Int/enum parameters are exposed to shader code as i32 so
                    // they compose with loop counters and comparisons.
                    src.push_str(&format!(
                        "fn p_{id}() -> i32 {{ return i32(np.u[{}].{}); }}\n",
                        slot.vec4_index,
                        comp_name(slot.component),
                        id = slot.id
                    ));
                }
            }
        }
    }
    (src, layout)
}

fn comp_name(i: usize) -> &'static str {
    ["x", "y", "z", "w"][i.min(3)]
}
