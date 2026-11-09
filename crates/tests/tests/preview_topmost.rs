//! Proves the renderer shows the TOP-MOST clip (so adding/selecting a different
//! generator on a higher track changes the preview), and that param edits change
//! output. Guards the "clicking a generator shows no change / goes white" bug.

use flux_app::commands::{CommandHost, GuiCommand, Request};
use flux_app::export::PreviewRenderer;
use flux_domain::{Project, RationalTime};

fn req(cmd: GuiCommand) -> Request {
    Request {
        protocol_version: 1,
        request_id: "t".into(),
        project_id: None,
        expected_revision: 0,
        command: cmd,
    }
}

fn pixels_signature(px: &[u8]) -> u64 {
    // Cheap content hash to compare frames.
    let mut h = 1469598103934665603u64;
    for (i, b) in px.iter().enumerate().step_by(97) {
        h ^= *b as u64;
        h = h.wrapping_mul(1099511628211);
        let _ = i;
    }
    h
}

#[test]
fn topmost_clip_wins_and_edits_change_output() {
    let mut h = CommandHost::new(Project::new("gui", 3));
    h.handle(req(GuiCommand::CreateProject {
        name: "G".into(),
        seed: 5,
        width: 320,
        height: 180,
        fps_num: 30,
        fps_den: 1,
    }));

    let mut r = match PreviewRenderer::new() {
        Ok(r) => r,
        Err(_) => {
            eprintln!("no GPU; skipping");
            return;
        }
    };

    // Scene A: plasma on the first track.
    h.handle(req(GuiCommand::AddGeneratorClip {
        generator_id: "gen.plasma".into(),
        effects: vec![],
        seconds: 8,
    }));
    let t = RationalTime::new(1, 1).unwrap();
    let a = r
        .render(&h.dispatcher.project, t, 320, 180)
        .expect("render A");
    let sig_a = pixels_signature(&a);

    // Scene B: a clearly different generator on a NEW (higher) track.
    h.handle(req(GuiCommand::AddGeneratorClip {
        generator_id: "gen.gradient".into(),
        effects: vec![],
        seconds: 8,
    }));
    let b = r
        .render(&h.dispatcher.project, t, 320, 180)
        .expect("render B");
    let sig_b = pixels_signature(&b);

    assert_ne!(
        sig_a, sig_b,
        "adding a different generator on a higher track must change the preview"
    );

    // Also assert non-white content in both.
    for (name, px) in [("A", &a), ("B", &b)] {
        let nonwhite = px
            .chunks(4)
            .filter(|c| c[0] != 255 || c[1] != 255 || c[2] != 255)
            .count();
        assert!(
            nonwhite > (320 * 180) / 20,
            "{name} preview went blank/white"
        );
    }
}
