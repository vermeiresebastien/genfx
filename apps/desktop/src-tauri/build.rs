use std::process::Command;

fn main() {
    // Embed the git short hash so the running app can report exactly which build
    // it is. Without this there is no way to tell a stale binary from a bug.
    if let Ok(out) = Command::new("git").args(["rev-parse", "--short", "HEAD"]).output() {
        if out.status.success() {
            let hash = String::from_utf8_lossy(&out.stdout).trim().to_string();
            println!("cargo:rustc-env=FLUX_GIT_HASH={hash}");
        }
    }

    // Rebuild the webview bundle before embedding it, and version the script URL
    // by content hash. This is what prevents a stale app.bundle.js in WebView2's
    // cache from serving an old inspector.
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui");
    match Command::new("node").arg("bundle.mjs").current_dir(&ui).status() {
        Ok(s) if s.success() => {}
        Ok(s) => println!("cargo:warning=ui bundle failed with exit {s}"),
        Err(e) => println!("cargo:warning=ui bundle not rebuilt ({e}); using committed bundle"),
    }

    // Watch the git ref, not just HEAD: committing rewrites refs/heads/<branch>,
    // not HEAD itself, so watching HEAD alone left the embedded hash stale.
    println!("cargo:rerun-if-changed=../../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../../.git/refs");
    println!("cargo:rerun-if-changed=../ui/main.js");
    println!("cargo:rerun-if-changed=../ui/view");
    println!("cargo:rerun-if-changed=../ui/store.js");
    println!("cargo:rerun-if-changed=../ui/commands.js");
    println!("cargo:rerun-if-changed=../ui/ipc.js");
    println!("cargo:rerun-if-changed=../ui/index.html");
    println!("cargo:rerun-if-changed=../ui/style.css");
    tauri_build::build()
}
