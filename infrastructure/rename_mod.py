import re, glob

files = glob.glob(r"D:\CURSOR LOCAL PROJECTS\VIDEO - GENFX\crates\catalog\src\*.rs")
for f in files:
    t = open(f, encoding="utf-8").read()
    orig = t
    # rename function definitions
    t = t.replace("fn mod_f(a: f32, b: f32)", "fn fmod_f(a: f32, b: f32)")
    t = t.replace("fn modv(a: vec2<f32>, b: vec2<f32>)", "fn fmodv(a: vec2<f32>, b: vec2<f32>)")
    t = t.replace("fn mod(a: f32, b: f32)", "fn fmod(a: f32, b: f32)")
    # rename calls. Handle nested parens crudely by targeting `mod(` occurrences
    # and replacing with fmod( ; also modv already renamed. Do NOT touch Rust codec
    # `mod ` / `mod.rs` because those are in Rust syntax (grep only WGSL bodies).
    # WGSL calls look like `mod(x, ...)` with a following '(' — Rust has `mod tests {`.
    t = re.sub(r'\bmod\(', 'fmod(', t)
    if t != orig:
        open(f, "w", encoding="utf-8", newline="").write(t)
print("done")
