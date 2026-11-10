//! Render every preset and assert it shows something. This is the direct
//! regression for "half the presets output nothing but black".
//!
//!   cargo test -p flux-tests --test preset_sweep -- --nocapture --test-threads=1

use flux_app::export::PreviewRenderer;
use flux_domain::{Project, RationalTime, TimeRange};

fn visible_fraction(px: &[u8]) -> f32 {
    if px.is_empty() {
        return 0.0;
    }
    let n = px.len() / 4;
    let mut vis = 0usize;
    for c in px.chunks(4) {
        if c[3] > 8 && (c[0] > 8 || c[1] > 8 || c[2] > 8) {
            vis += 1;
        }
    }
    vis as f32 / n as f32
}

#[test]
fn every_preset_renders_something() {
    let Ok(mut r) = PreviewRenderer::new() else {
        eprintln!("no GPU; skipping");
        return;
    };

    let mut blanks: Vec<String> = Vec::new();
    let mut errors: Vec<String> = Vec::new();
    let mut ok = 0usize;

    let mut project = Project::new("presets", 1);
    project.timeline.range =
        TimeRange::new(RationalTime::ZERO, RationalTime::from_secs(4));

    for preset in flux_catalog::presets::all_presets() {
        let graph = flux_app::samples::insert_preset(&preset);
        project.graphs.insert(graph.id.clone(), graph.clone());

        let mut best = 0.0f32;
        let mut err = String::new();
        for t in [
            RationalTime::new(1, 2).unwrap(),
            RationalTime::new(2, 1).unwrap(),
        ] {
            match r.render_graph(&graph, &project, t, 64, 64) {
                Ok(px) => best = best.max(visible_fraction(&px)),
                Err(e) => err = format!("{:?}: {}", e.code, e.message),
            }
        }
        if !err.is_empty() && best == 0.0 {
            errors.push(format!("{:<28} {}", preset.id, err));
        } else if best < 0.01 {
            blanks.push(format!("{:<28} visible={:.4}", preset.id, best));
        } else {
            ok += 1;
        }
    }

    eprintln!("\n============ PRESET SWEEP ============");
    eprintln!("ok: {}  blank: {}  error: {}", ok, blanks.len(), errors.len());
    if !blanks.is_empty() {
        eprintln!("\n--- BLANK ---");
        for b in &blanks {
            eprintln!("  {}", b);
        }
    }
    if !errors.is_empty() {
        eprintln!("\n--- ERROR ---");
        for e in &errors {
            eprintln!("  {}", e);
        }
    }
    eprintln!("======================================\n");

    assert!(
        errors.is_empty(),
        "{} preset(s) failed to render",
        errors.len()
    );
    assert!(
        blanks.is_empty(),
        "{} preset(s) rendered black: {}",
        blanks.len(),
        blanks.join(", ")
    );
}
