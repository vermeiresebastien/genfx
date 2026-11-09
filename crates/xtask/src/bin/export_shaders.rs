//! Export the catalog's generated WGSL to `apps/shader-harness/shaders.json` for
//! the browser WebGPU conformance harness. Also validates every shader with Naga
//! (the same validator the native renderer uses) so a broken shader cannot ship.

use flux_catalog::{build_catalog, shader_gen};
use serde_json::json;

fn main() {
    let out =
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../apps/shader-harness");
    std::fs::create_dir_all(&out).unwrap();
    let catalog = build_catalog();
    let mut entries = vec![];
    let mut failures = 0;
    for e in &catalog.entries {
        if e.manifest.implementation.kind != "fragment" {
            continue;
        }
        let wgsl = shader_gen::generate_shader(e);
        // Validate with Naga (identical to the native path).
        match naga::front::wgsl::parse_str(&wgsl) {
            Ok(module) => {
                let mut v = naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::all(),
                );
                if v.validate(&module).is_err() {
                    failures += 1;
                    eprintln!("naga validation failed: {}", e.manifest.id);
                }
            }
            Err(err) => {
                failures += 1;
                eprintln!(
                    "naga parse failed: {}: {}",
                    e.manifest.id,
                    err.emit_to_string(&wgsl)
                );
            }
        }
        entries.push(json!({
            "id": e.manifest.id,
            "name": e.manifest.name,
            "wgsl": wgsl,
            "fragmentEntry": e.manifest.implementation.entry,
        }));
    }
    let doc = json!({
        "shaderAbiVersion": 1,
        "count": entries.len(),
        "failures": failures,
        "shaders": entries,
    });
    let path = out.join("shaders.json");
    std::fs::write(&path, serde_json::to_string(&doc).unwrap()).unwrap();
    println!(
        "shader-harness: wrote {} shaders to {} ({} naga failures)",
        entries.len(),
        path.display(),
        failures
    );
    if failures > 0 {
        std::process::exit(1);
    }
}
