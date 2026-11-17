//! Where a running Flux process looks for sidecar binaries: the Spout/NDI shims,
//! the SpoutDX helper EXE, and the FFmpeg toolchain.
//!
//! Before this module every candidate was a bare relative path, so it resolved
//! against the **working directory** and only ever worked from a source checkout —
//! while the doc comments on the loaders claimed "next to the executable". An
//! installed app has no `apps/` directory, so every shipped sidecar was
//! unreachable. The base list below is ordered **installed-first**: the exe's own
//! directory is tried before `.` so a stray source tree in the launch directory
//! cannot shadow a DLL that ships beside the binary.

use std::path::PathBuf;

/// The directory holding the running executable, when the OS will say.
///
/// `None` is possible (an unreadable `PATH` resolution for `current_exe`) and
/// means only that the installed bases drop out; the working-directory base stays.
pub fn exe_dir() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(std::path::Path::to_path_buf))
}

/// Base directories for sidecar lookup, installed-first.
///
/// `bin`, `resources` and `ffmpeg` cover the layouts the Tauri bundler produces
/// for `bundle.resources` entries; the trailing `.` keeps a source checkout working.
pub fn bases() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(dir) = exe_dir() {
        out.push(dir.clone());
        for sub in ["bin", "resources", "ffmpeg"] {
            out.push(dir.join(sub));
        }
    }
    out.push(PathBuf::from("."));
    out
}

/// First `<base>/<relative>` that is a file, scanning relatives outermost so an
/// installed candidate beats a checkout candidate for the same name.
///
/// Pure over `bases` so the ordering is testable without moving a binary.
pub fn existing_in(bases: &[PathBuf], relatives: &[&str]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for rel in relatives {
        for base in bases {
            let cand = base.join(rel);
            if cand.is_file() && !out.contains(&cand) {
                out.push(cand);
            }
        }
    }
    out
}

/// Every candidate in resolution order: the environment override first when it is
/// set — kept as a candidate even when it names nothing, so the located paths
/// follow it and a stale override cannot hide the shipped layout — then every
/// existing path under [`bases`].
///
/// A list, not a first hit: a DLL can exist and still fail to load, and every
/// caller here retries.
pub fn candidates(env_var: &str, relatives: &[&str]) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Some(over) = std::env::var_os(env_var) {
        let path = PathBuf::from(over);
        if !path.as_os_str().is_empty() {
            out.push(path);
        }
    }
    for cand in existing_in(&bases(), relatives) {
        if !out.contains(&cand) {
            out.push(cand);
        }
    }
    out
}

/// [`existing_in`] over [`bases`], as a single best path.
pub fn locate(relatives: &[&str]) -> Option<PathBuf> {
    existing_in(&bases(), relatives).into_iter().next()
}

/// An environment override wins outright, but only when the path it names is a
/// file: a stale `FLUX_SPOUT_SHIM` left in the environment must fall through to
/// the shipped layout rather than turn into a silent "unavailable".
pub fn locate_with_env(env_var: &str, relatives: &[&str]) -> Option<PathBuf> {
    if let Some(over) = std::env::var_os(env_var) {
        let path = PathBuf::from(over);
        if path.is_file() {
            return Some(path);
        }
    }
    locate(relatives)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn probe_dir(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "flux-layout-{}-{}-{}",
            std::process::id(),
            label,
            std::time::UNIX_EPOCH.elapsed().unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).expect("create probe dir");
        dir
    }

    /// The defect this module exists for: a sidecar found **next to the
    /// executable**, which no candidate list made of CWD-relative strings can do.
    #[test]
    fn exe_dir_is_the_first_base_and_is_not_the_working_directory() {
        let dir = exe_dir().expect("current_exe has a parent here");
        let bases = bases();
        assert_eq!(
            bases.first().map(PathBuf::as_path),
            Some(dir.as_path()),
            "`bases()` must try the exe dir before anything else; got {bases:?}"
        );
        assert_ne!(
            dir.canonicalize().ok().as_deref(),
            std::env::current_dir().ok().as_deref(),
            "if this fires, the test can no longer tell exe-relative from CWD-relative"
        );
    }

    #[test]
    fn installed_base_wins_over_the_checkout_base() {
        let installed = probe_dir("installed");
        let checkout = probe_dir("checkout");
        for dir in [&installed, &checkout] {
            std::fs::write(dir.join("spout_shim.dll"), b"probe").expect("plant");
        }
        let bases = vec![installed.clone(), checkout.clone()];
        let found = existing_in(&bases, &["spout_shim.dll"]);
        for dir in [&installed, &checkout] {
            let _ = std::fs::remove_dir_all(dir);
        }
        assert_eq!(
            found.first().map(PathBuf::as_path),
            Some(installed.join("spout_shim.dll").as_path()),
            "the first base must be returned first, not the planted second copy"
        );
        assert_eq!(found.len(), 2, "both candidates are offered, in order");
    }

    #[test]
    fn a_multi_segment_relative_is_resolved_against_every_base() {
        let dir = probe_dir("nested");
        let nested = dir.join("apps").join("ndi-bridge");
        std::fs::create_dir_all(&nested).expect("create nested");
        std::fs::write(nested.join("ndi_shim.dll"), b"probe").expect("plant");
        let bases = vec![PathBuf::from("/definitely/not/here"), dir.clone()];
        let found = existing_in(&bases, &["ndi_shim.dll", "apps/ndi-bridge/ndi_shim.dll"]);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            found.first().map(PathBuf::as_path),
            Some(nested.join("ndi_shim.dll").as_path()),
            "the source-tree relative form still resolves, via the exe dir as well as via `.`"
        );
    }

    #[test]
    fn nothing_found_is_empty_and_a_directory_is_never_a_sidecar() {
        let dir = probe_dir("dirs-only");
        assert!(existing_in(&[dir.clone()], &["absent-probe.dll"]).is_empty());
        assert!(
            existing_in(&[dir.clone()], &[""]).is_empty(),
            "`base.join(\"\")` is the base itself: a directory, not a sidecar"
        );
        assert!(existing_in(&[dir.clone()], &["."]).is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Every loader retries, so it needs the whole list rather than one answer —
    /// and a stale override must not cost the caller the shipped candidate.
    #[test]
    fn candidates_keep_a_stale_override_but_do_not_stop_at_it() {
        let key = "FLUX_LAYOUT_PROBE_CANDIDATES";
        let exe = exe_dir().expect("exe dir");
        let shipped_name = format!("flux-layout-shipped-{}.dll", std::process::id());
        let shipped = exe.join(&shipped_name);
        std::fs::write(&shipped, b"probe").expect("plant beside the binary");
        let stale = std::env::temp_dir().join("flux-layout-absent.dll");

        std::env::set_var(key, &stale);
        let with_stale = candidates(key, &[&shipped_name]);
        std::env::set_var(key, &shipped);
        let with_good = candidates(key, &[&shipped_name]);
        std::env::remove_var(key);
        let _ = std::fs::remove_file(&shipped);

        assert_eq!(
            with_stale.first().map(PathBuf::as_path),
            Some(stale.as_path()),
            "the override is tried first, so an operator can still force a build"
        );
        assert!(
            with_stale.contains(&shipped),
            "a stale override must not hide the shipped candidate: {with_stale:?}"
        );
        assert_eq!(
            with_good.first().map(PathBuf::as_path),
            Some(shipped.as_path()),
            "a valid override is both first and not duplicated"
        );
        assert_eq!(
            with_good.iter().filter(|p| *p == &shipped).count(),
            1,
            "the override and the located candidate are the same file"
        );
    }

    #[test]
    fn an_env_override_only_wins_when_it_names_a_file() {
        let key = "FLUX_LAYOUT_PROBE_OVERRIDE";
        let dir = probe_dir("override");
        let file = dir.join("override.dll");
        std::fs::write(&file, b"probe").expect("plant");

        std::env::set_var(key, &file);
        let hit = locate_with_env(key, &["nothing-provides-this.dll"]);
        std::env::remove_var(key);
        assert_eq!(
            hit.as_deref(),
            Some(file.as_path()),
            "a valid override wins"
        );

        std::env::set_var(key, dir.join("stale-and-absent.dll"));
        let after_stale = locate_with_env(key, &["nothing-provides-this.dll"]);
        std::env::remove_var(key);
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(
            after_stale, None,
            "a stale override must fall through to the layout, not short-circuit it"
        );
    }

    #[test]
    fn locate_finds_a_sibling_of_the_running_binary() {
        let dir = exe_dir().expect("exe dir");
        let name = format!("flux-layout-probe-{}.dll", std::process::id());
        let file = dir.join(&name);
        std::fs::write(&file, b"probe").expect("write beside the running binary");
        let found = locate(&[&name]);
        let _ = std::fs::remove_file(&file);
        assert_eq!(
            found.as_deref(),
            Some(file.as_path()),
            "writing a sidecar next to the executable and finding it through locate() is the \
             exact capability an installed app needs; a CWD-only candidate list cannot do this"
        );
    }
}
