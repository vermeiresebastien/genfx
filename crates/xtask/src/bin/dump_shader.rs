use flux_catalog::{build_catalog, shader_gen};
fn main() {
    let c = build_catalog();
    let id = std::env::args().nth(1).unwrap_or_else(|| "gen.solid".into());
    let e = c.get(&id).unwrap();
    println!("{}", shader_gen::generate_shader(e));
}
