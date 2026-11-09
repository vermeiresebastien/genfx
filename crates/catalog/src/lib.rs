//! Flux catalog: manifests + WGSL sources for every effect and generator.
//!
//! The catalog is the single source of truth. UI controls, uniform packing,
//! shader generation, docs, and tests are derived from these definitions.

pub mod builders;
pub mod effects_a;
pub mod effects_b;
pub mod effects_c;
pub mod families;
pub mod font5x7;
pub mod generators_a;
pub mod generators_b;
pub mod generators_c;
pub mod generators_d;
pub mod manifest;
pub mod pack;
pub mod presets;
pub mod shader_gen;
pub mod utility;
pub mod wgsl_common;

pub use manifest::{Catalog, CatalogEntry, Manifest, ParamDecl, ParamKind};
pub use utility::build_catalog;

/// Number of request-catalog rows tracked in requirements.json.
pub fn catalog_row_count() -> usize {
    let entries = build_catalog().entries;
    entries
        .iter()
        .filter(|e| !e.manifest.requirement_ids.iter().any(|r| r == "R007"))
        .count()
}

#[cfg(test)]
mod tests;
