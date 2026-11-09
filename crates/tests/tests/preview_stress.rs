//! Stress the cached PreviewRenderer over many frames + edits to catch the
//! "preview goes white after a while" regression (device loss or state decay).

use flux_app::export::PreviewRenderer;
use flux_app::samples;
use flux_domain::RationalTime;

fn nonwhite_ratio(px: &[u8]) -> f32 {
    let total = px.len() / 4;
    let mut nw = 0;
    for c in px.chunks(4) {
        if c[0] != 255 || c[1] != 255 || c[2] != 255 {
            nw += 1;
        }
    }
    nw as f32 / total as f32
}

#[test]
fn preview_stays_colored_over_many_frames_and_edits() {
    let mut project = samples::demo_generator_project("gen.plasma", &[], 4);
    let mut r = match PreviewRenderer::new() {
        Ok(r) => r,
        Err(_) => {
            eprintln!("no GPU; skipping");
            return;
        }
    };
    // Render 60 frames advancing time, then mutate the project a few times.
    for i in 0..60 {
        let t = RationalTime::new(i as i64, 10).unwrap();
        let px = r.render(&project, t, 320, 180).expect("render");
        let nw = nonwhite_ratio(&px);
        assert!(nw > 0.05, "frame {i} went blank/white (nonwhite={nw:.3})");
    }
    // Edit a param and re-render (simulates inspector edits).
    for step in 0..5 {
        // Nudge every node's float params.
        if let Some(g) = project.graphs.values_mut().next() {
            for n in g.nodes.iter_mut() {
                for v in n.params.values_mut() {
                    if let flux_domain::ParamValue::Float { value } = v {
                        *value += 0.01 * step as f64;
                    }
                }
            }
        }
        let px = r
            .render(&project, RationalTime::new(2, 1).unwrap(), 320, 180)
            .expect("render");
        let nw = nonwhite_ratio(&px);
        assert!(
            nw > 0.05,
            "post-edit {step} went blank/white (nonwhite={nw:.3})"
        );
    }
}
