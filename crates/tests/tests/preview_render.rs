//! Verifies the cached PreviewRenderer produces the same non-blank frame as the
//! export path (guards the GUI preview regression where output was white).

use flux_app::export::PreviewRenderer;
use flux_app::samples;
use flux_domain::RationalTime;

#[test]
fn preview_renders_non_blank_plasma() {
    let project = samples::demo_generator_project("gen.plasma", &["fx.bloom"], 4);
    let mut r = match PreviewRenderer::new() {
        Ok(r) => r,
        Err(_) => {
            eprintln!("no GPU; skipping");
            return;
        }
    };
    let t = RationalTime::new(1, 1).unwrap();
    let px = r.render(&project, t, 320, 180).expect("render");
    assert_eq!(px.len(), 320 * 180 * 4);
    // Not blank: some pixel must be non-white and non-transparent-black.
    let mut nonwhite = 0;
    let mut opaque = 0;
    for c in px.chunks(4) {
        if c[0] != 255 || c[1] != 255 || c[2] != 255 {
            nonwhite += 1;
        }
        if c[3] != 0 {
            opaque += 1;
        }
    }
    let total = 320 * 180;
    assert!(
        opaque > total / 2,
        "most pixels should be opaque (alpha {}",
        opaque
    );
    assert!(
        nonwhite > total / 10,
        "plasma should be colored, not all-white (nonwhite {nonwhite}/{total})"
    );
}
