//! Native egui editor: a four-pane workspace (library, preview, inspector,
//! timeline) driving the same engine that the exporter uses.
//!
//! The editor never runs its own simulation. It edits a `Project` through the
//! `Dispatcher` (typed commands, revision-safe) and asks the render thread to
//! render the current graph; the resulting texture is shown as a preview.

use crate::{load_project, samples, save_project};
use flux_catalog::build_catalog;
use flux_domain::*;
use flux_render::gpu::{BackendPref, Gpu};
use std::path::PathBuf;
use std::sync::Arc;

/// Small UI state separate from the document.
pub struct UiState {
    pub selected_clip: Option<String>,
    pub selected_node: Option<String>,
    pub library_search: String,
    pub playing: bool,
}

pub fn run_gui(project_path: Option<PathBuf>) -> FluxResult<()> {
    let _ = project_path;
    editor_main()
}

// The actual window/event loop lives in `win` (kept separate so the headless
// build and CLI do not require a display).
#[path = "win.rs"]
mod win;

fn editor_main() -> FluxResult<()> {
    win::run()
}

/// Create a GPU context with a clear error if the host has no adapter.
fn gpu() -> FluxResult<Gpu> {
    Gpu::new(BackendPref::Auto)
}

#[allow(dead_code)]
fn catalog_count() -> usize {
    build_catalog().entries.len()
}

#[allow(dead_code)]
fn arc_str(s: &str) -> Arc<str> {
    Arc::from(s)
}
