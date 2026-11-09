//! Build script for `flux-integrations`.
//!
//! Only does anything when the `link` feature is enabled. It links the Ableton
//! Link static C API (`abl_link.lib`, built from https://github.com/Ableton/link)
//! and its Windows system libraries. The library directory is provided via the
//! `FLUX_ABLETON_LINK_LIB` environment variable (the directory containing
//! `abl_link.lib`). If it is not set, the build fails with a clear message —
//! enable the `link` feature only when the library is available.

fn main() {
    println!("cargo:rerun-if-env-changed=FLUX_ABLETON_LINK_LIB");
    let link_enabled = std::env::var_os("CARGO_FEATURE_LINK").is_some();
    if !link_enabled {
        return;
    }
    let dir = match std::env::var("FLUX_ABLETON_LINK_LIB") {
        Ok(d) => d,
        Err(_) => {
            panic!(
                "feature `link` is enabled but FLUX_ABLETON_LINK_LIB is not set.\n\
                 Build Ableton Link and point FLUX_ABLETON_LINK_LIB at the directory \
                 containing abl_link.lib, e.g.:\n  \
                 git clone https://github.com/Ableton/link\n  \
                 cmake -G \"Visual Studio 18 2026\" -A x64 -S link -B link/build\n  \
                 cmake --build link/build --config Release --target abl_link\n  \
                 set FLUX_ABLETON_LINK_LIB=%CD%\\link\\build\\Release"
            );
        }
    };
    println!("cargo:rustc-link-search=native={dir}");
    println!("cargo:rustc-link-lib=static=abl_link");
    // Abelton Link's platform + ASIO dependencies on Windows.
    for lib in ["winmm", "Avrt", "ws2_32", "iphlpapi", "bcrypt", "userenv"] {
        println!("cargo:rustc-link-lib={lib}");
    }
}
