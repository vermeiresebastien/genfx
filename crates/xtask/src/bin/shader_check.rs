use flux_catalog::{build_catalog, shader_gen};

fn main() {
    let catalog = build_catalog();
    let mut fails = 0;
    for entry in &catalog.entries {
        if entry.manifest.implementation.kind != "fragment" && entry.manifest.id != "gen.solid" {
            // still validate present shaders
        }
        let src = shader_gen::generate_shader(entry);
        match naga::front::wgsl::parse_str(&src) {
            Ok(module) => {
                let mut v = naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::all(),
                );
                if let Err(e) = v.validate(&module) {
                    fails += 1;
                    println!("=== {} VALIDATION ===\n{:?}\n", entry.manifest.id, e);
                }
            }
            Err(e) => {
                fails += 1;
                println!("=== {} PARSE ===\n{}\n", entry.manifest.id, e.emit_to_string(&src));
            }
        }
    }
    println!("TOTAL FAILS: {fails}");
}
