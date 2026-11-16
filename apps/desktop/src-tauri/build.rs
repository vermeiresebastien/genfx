use std::process::{Command, Stdio};

/// Run the webview bundler and make any failure a **build failure**.
///
/// Why this is not a `cargo:warning` any more (measured at 63bc321): both
/// failure arms used to print `cargo:warning=…` and continue, so a machine with
/// no `node` on PATH, or a bundler that throws, embedded the *committed*
/// `app.bundle.js` and finished green. D036 is the rule that settles it — a step
/// that prints a failure and exits 0 is the same defect as a skipped gate that
/// counts as a pass. No warning path is left here at all, gated or otherwise:
/// an ungated warning is the defect, and gating it would only move it.
fn bundle_ui(ui: &std::path::Path) {
    // stdout is inherited on purpose: the bundler's `bundled app.bundle.js?v=<hash>`
    // line then lands in cargo's build-script output file
    // (`target/debug/build/flux-desktop-<metadata>/output`), which is the marker
    // the I5 rerun gate reads. Only stderr is captured, because the panic message
    // has to carry the reason the child gave.
    let spawned = Command::new("node")
        .arg("bundle.mjs")
        .current_dir(ui)
        .stderr(Stdio::piped())
        .spawn();
    let out = match spawned {
        Err(e) => panic!(
            "ui bundle: could not start `node bundle.mjs` in {} ({e}). The webview bundle is \
             built here, not committed-and-trusted: a host without node on PATH cannot prove \
             which app.bundle.js it is shipping, so this build stops instead of guessing.",
            ui.display()
        ),
        Ok(child) => match child.wait_with_output() {
            Err(e) => panic!(
                "ui bundle: `node bundle.mjs` started but did not complete in {} ({e})",
                ui.display()
            ),
            Ok(out) => out,
        },
    };
    let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
    if !out.status.success() {
        panic!(
            "ui bundle failed: `node bundle.mjs` exited {} in {}\n--- child stderr ---\n{}\n\
             --- end stderr ---\nFix the bundler or install its dev dependencies \
             (`npm ci` in apps/desktop/ui); do not build the desktop app on top of a stale \
             committed bundle.",
            out.status,
            ui.display(),
            if stderr.is_empty() {
                "(the bundler printed nothing on stderr)"
            } else {
                stderr.as_str()
            },
        );
    }
    // A successful bundle that still said something is kept visible: build-script
    // stderr is recorded beside the output file and shown by `cargo build -v`,
    // and nothing here turns it into a verdict.
    if !stderr.is_empty() {
        eprintln!("node bundle.mjs wrote to stderr while succeeding:\n{stderr}");
    }
}

fn main() {
    // Embed the git short hash so the running app can report exactly which build
    // it is. Without this there is no way to tell a stale binary from a bug.
    //
    // What a failure here means, stated instead of left implicit: `git rev-parse
    // --short HEAD` fails when git is not on PATH, when this tree is not a
    // repository, or when HEAD does not point at a commit yet (a fresh `git init`).
    // All three answer the same question — there is no commit to name — so the
    // script embeds the literal `unknown` rather than leaving FLUX_GIT_HASH unset.
    // `build_info` then reports `gitHash: "unknown"` for one reason and not
    // because a build feature happened to be absent. The consequence is stated
    // where it matters: a build reading `unknown` proves nothing about which
    // commit it came from, so no stale-binary conclusion may be drawn from it.
    // Gated by a PATH-restricted build (git off PATH, node left in place):
    // `smoke/I5-1-git-absent.log` shows the build exiting 0 with this line in the
    // build script's stderr file and `cargo:rustc-env=FLUX_GIT_HASH=unknown` in
    // its output file, and the control leg on the normal PATH re-embeds the hash.
    let hash = match Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
    {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        Ok(out) => {
            eprintln!(
                "flux-desktop build: `git rev-parse --short HEAD` exited {} ({}) — embedding \
                 FLUX_GIT_HASH=unknown",
                out.status,
                String::from_utf8_lossy(&out.stderr).trim()
            );
            String::new()
        }
        Err(e) => {
            eprintln!(
                "flux-desktop build: git could not be run ({e}) — embedding FLUX_GIT_HASH=unknown"
            );
            String::new()
        }
    };
    let hash = if hash.is_empty() {
        "unknown".to_string()
    } else {
        hash
    };
    println!("cargo:rustc-env=FLUX_GIT_HASH={hash}");

    // Rebuild the webview bundle before embedding it, and version the script URL
    // by content hash. This is what prevents a stale app.bundle.js in WebView2's
    // cache from serving an old inspector. A failure is fatal (see bundle_ui).
    let ui = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../ui");
    bundle_ui(&ui);

    // Watch the git ref, not just HEAD: committing rewrites refs/heads/<branch>,
    // not HEAD itself, so watching HEAD alone left the embedded hash stale.
    //
    // What this list can and cannot see, measured (cargo 1.99, 2026-11-16):
    //  * Cargo runs this script once per change set and only re-runs it when a
    //    path below is newer than the last run. A path that is *not* listed here
    //    is not a reason to re-bundle: that was the defect — bundle.mjs,
    //    monitor.js, monitor.html, package.json and package-lock.json were all
    //    missing, so editing the bundler or the monitor shipped whatever
    //    app.bundle.js was already committed, under a green build.
    //  * `.git/refs` is a *directory* watch. Cargo compares the mtime of the
    //    directory itself, which a commit updates because it writes a ref file
    //    inside it; it does not walk the tree. A `git checkout` that only
    //    rewrites `packed-refs`, or any ref update that leaves the directory's
    //    own mtime alone, therefore leaves the embedded hash stale. Making that
    //    exact would mean resolving the branch ref inside this script, which
    //    would then need a watch of its own.
    //  * `app.bundle.js` is deliberately NOT watched even though the bundler
    //    rewrites it every run: watching one of this script's own outputs makes
    //    every build re-run it, so the build never reaches fresh.
    //  * `index.html` IS watched and the bundler rewrites it, which is the same
    //    self-trigger by another route: measured at 63bc321 two consecutive
    //    `cargo build -p flux-desktop` invocations both re-ran this script with
    //    nothing else touched (index.html's mtime equalled each build, and a
    //    fresh state is only reachable once the write is skipped when the
    //    content is unchanged). Until `bundle.mjs` guards that write, this watch
    //    makes the rerun set unobservable — recorded as an interim consequence
    //    of item 1 and closed by item 2.
    //  * What no list here can see: the *contents* of `node_modules`, which is
    //    not a watched path and is gitignored. A different esbuild install on the
    //    same lockfile builds a different bundle without this script noticing;
    //    `package-lock.json` is the closest thing to a handle on it.
    println!("cargo:rerun-if-changed=../../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../../.git/refs");
    println!("cargo:rerun-if-changed=../ui/bundle.mjs");
    println!("cargo:rerun-if-changed=../ui/main.js");
    println!("cargo:rerun-if-changed=../ui/monitor.js");
    println!("cargo:rerun-if-changed=../ui/monitor.html");
    println!("cargo:rerun-if-changed=../ui/package.json");
    println!("cargo:rerun-if-changed=../ui/package-lock.json");
    println!("cargo:rerun-if-changed=../ui/view");
    println!("cargo:rerun-if-changed=../ui/store.js");
    println!("cargo:rerun-if-changed=../ui/commands.js");
    println!("cargo:rerun-if-changed=../ui/ipc.js");
    println!("cargo:rerun-if-changed=../ui/index.html");
    println!("cargo:rerun-if-changed=../ui/style.css");
    // tauri_build::build() adds its own watches on top of these (measured in
    // target/debug/build/flux-desktop-*/output: tauri.conf.json and
    // capabilities/), so this list is a floor, not the whole dep set.
    tauri_build::build()
}
