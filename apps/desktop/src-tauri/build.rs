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
    println!("cargo:rerun-if-changed=../../../.git/HEAD");
    tauri_build::build()
}
