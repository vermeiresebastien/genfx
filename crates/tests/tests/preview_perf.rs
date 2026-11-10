//! Measure per-frame render cost at several preview sizes, to find what makes
//! the editor choppy.
//!
//!   cargo test -p flux-tests --test preview_perf -- --nocapture

use flux_app::export::PreviewRenderer;
use flux_domain::{Project, RationalTime, TimeRange};
use std::time::Instant;

fn p() -> Project {
    let mut p = Project::new("perf", 1);
    let g = flux_app::samples::chain_graph("gen.plasma", &[]);
    p.timeline.master_graph_id = Some(g.id.clone());
    p.graphs.insert(g.id.clone(), g);
    p.timeline.range = TimeRange::new(RationalTime::ZERO, RationalTime::from_secs(8));
    p
}

#[test]
fn measure_preview_cost() {
    let Ok(mut r) = PreviewRenderer::new() else {
        eprintln!("no GPU; skipping");
        return;
    };
    let project = p();
    let graph = project.timeline.master_graph_id.clone().unwrap();

    for (w, h) in [(320u32, 180u32), (352, 198), (640, 360), (960, 540)] {
        // Warm the cache first (pipeline compile), then time steady state.
        let g = project.graphs.get(&graph).unwrap();
        let _ = r.render_graph(g, &project, RationalTime::new(1, 60).unwrap(), w, h);

        let n = 20;
        let t0 = Instant::now();
        for i in 0..n {
            let t = RationalTime::new(1 + i as i64, 30).unwrap();
            r.render_graph(g, &project, t, w, h).expect("render");
        }
        let ms = t0.elapsed().as_secs_f64() * 1000.0 / n as f64;
        eprintln!("preview {w}x{h}: {ms:.1} ms/frame  ({:.0} fps)", 1000.0 / ms);
    }
}
