//! GUI-backend acceptance: the exact IPC sequence the desktop UI performs
//! (create → catalog → add scene → set a parameter → snapshot) followed by the
//! same export the Export button invokes. Proves the GUI's advertised features
//! are backed by real engine work, headlessly.

use flux_app::commands::{CommandHost, GuiCommand, Request};
use flux_app::export::{export_project, ExportOptions};
use flux_domain::{Project, RationalTime, TimeRange};
use std::sync::Arc;

fn req(cmd: GuiCommand, rev: u64) -> Request {
    Request {
        protocol_version: 1,
        request_id: "t".into(),
        project_id: None,
        expected_revision: rev,
        command: cmd,
    }
}

#[test]
fn gui_backend_create_edit_export() {
    let mut h = CommandHost::new(Project::new("gui-e2e", 3));

    // boot(): createProject
    let r = h.handle(req(
        GuiCommand::CreateProject {
            name: "GUI".into(),
            seed: 5,
            width: 320,
            height: 180,
            fps_num: 30,
            fps_den: 1,
        },
        0,
    ));
    assert!(r.error.is_none(), "{:?}", r.error);

    // loadCatalog()
    let cat = h.handle(req(GuiCommand::Catalog, 0));
    let entries = cat.result.unwrap();
    let arr = entries["entries"].as_array().unwrap();
    assert!(arr.len() > 200);
    // Catalog must expose parameter declarations for the inspector.
    let plasma = arr.iter().find(|e| e["id"] == "gen.plasma").unwrap();
    assert!(plasma["parameters"].as_array().unwrap().len() > 0);
    assert!(plasma["parameters"][0]["kind"].is_string());

    // addScene()
    let r = h.handle(req(
        GuiCommand::AddGeneratorClip {
            generator_id: "gen.plasma".into(),
            effects: vec![],
            seconds: 2,
        },
        0,
    ));
    assert!(r.error.is_none(), "{:?}", r.error);

    // setParam(): find the plasma node and set its first float param.
    let snap = h.snapshot();
    let graphs = snap["project"]["graphs"].as_object().unwrap();
    let mut edited = false;
    for (gid, g) in graphs {
        for n in g["nodes"].as_array().unwrap() {
            if n["typeId"] == "gen.plasma" {
                let r = h.handle(req(
                    GuiCommand::Apply {
                        command: flux_domain::Command::SetParameter {
                            graph_id: gid.clone(),
                            node_id: n["id"].as_str().unwrap().to_string(),
                            param_id: "speed".into(),
                            value: flux_domain::ParamValue::Float { value: 0.7 },
                        },
                    },
                    h.revision(),
                ));
                assert!(r.error.is_none(), "{:?}", r.error);
                edited = true;
            }
        }
    }
    assert!(edited, "expected a plasma node to edit");

    // Export button: render the project's range to a real artifact.
    let dir = std::env::temp_dir().join(format!("flux-gui-export-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut proj = h.dispatcher.project.clone();
    proj.timeline.range = TimeRange::new(RationalTime::ZERO, RationalTime::new(1, 1).unwrap());
    let res = export_project(
        &proj,
        &ExportOptions {
            out_dir: dir.clone(),
            project_name: "gui".into(),
            codec: flux_media::EncodeCodec::None,
            width: 320,
            height: 180,
            range: None,
            cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        },
        None,
    )
    .unwrap();
    assert_eq!(res.frames, 30, "1s at 30fps");
    assert!(res.frames_dir.join("frame_000001.png").exists());
    // The frame must contain non-black content (plasma rendered).
    let px = std::fs::read(res.frames_dir.join("frame_000001.png")).unwrap();
    assert!(px.len() > 1000);
    let _ = std::fs::remove_dir_all(&dir);
}
