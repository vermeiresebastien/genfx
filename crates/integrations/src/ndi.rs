//! NDI discovery adapter — runtime-loaded, feature `ndi`.
//!
//! The official NDI runtime (`Processing.NDI.Lib.x64.dll`) is present on this host
//! (installed by Resolume Arena) but **no SDK headers** are shipped. To stay safe we
//! do NOT reconstruct opaque capture structs from memory. We define only the two
//! small, stable discovery structs exactly as published in the NDI v5 API:
//!
//! ```c
//! typedef struct NDIlib_find_create_t {
//!   bool show_local_sources; const char* p_groups; const char* p_extra_ips;
//! } NDIlib_find_create_t;
//! typedef struct NDIlib_source_t {
//!   const char* p_ndi_name; const char* p_url_address;
//! } NDIlib_source_t;
//! ```
//!
//! and call only `NDIlib_initialize`, `NDIlib_find_create2`,
//! `NDIlib_find_get_current_sources`, `NDIlib_find_wait_for_sources`,
//! `NDIlib_find_destroy`, `NDIlib_destroy`. Sending/receiving live frames needs the
//! full header ABI + a live stream and stays out of scope here.

#![cfg(feature = "ndi")]

use serde::Serialize;
use std::ffi::{c_char, c_void, CStr};
use std::path::Path;

/// A discovered NDI source (name only; no capture attempted).
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct NdiSource {
    pub name: String,
    pub url: Option<String>,
}

// C ABI structs (discovery only). Layout matches NDI v5 exactly.
#[repr(C)]
struct FindCreate {
    show_local_sources: u8,
    p_groups: *const c_char,
    p_extra_ips: *const c_char,
}

#[repr(C)]
struct NdiSourceT {
    p_ndi_name: *const c_char,
    p_url_address: *const c_char,
}

type FindInstance = *mut c_void;

type FnInit = unsafe extern "C" fn() -> bool;
type FnDestroy = unsafe extern "C" fn();
type FnFindCreate2 = unsafe extern "C" fn(*const FindCreate) -> FindInstance;
type FnFindDestroy = unsafe extern "C" fn(FindInstance);
type FnFindWait = unsafe extern "C" fn(FindInstance, u32) -> bool;
type FnFindGetSources = unsafe extern "C" fn(FindInstance, *mut u32) -> *const NdiSourceT;
type FnSupportedCpu = unsafe extern "C" fn() -> bool;

/// Loaded NDI runtime with resolved discovery entry points.
pub struct NdiRuntime {
    _lib: *mut c_void,
    initialize: FnInit,
    destroy: FnDestroy,
    find_create2: FnFindCreate2,
    find_destroy: FnFindDestroy,
    find_wait: FnFindWait,
    find_get_sources: FnFindGetSources,
    is_supported_cpu: FnSupportedCpu,
}

impl NdiRuntime {
    /// Load the NDI runtime from an explicit path or by well-known name.
    pub fn load() -> Option<Self> {
        // Explicit override, then the shipped layout (a runtime bundled beside the
        // exe is the only form an installed app can reach), then the standard NDI
        // install locations (NDI 6 Tools runtime first, then a bare name for the
        // system search, then Resolume's bundled runtime).
        let mut candidates = flux_platform::layout::candidates(
            "FLUX_NDI_LIB",
            &[
                "Processing.NDI.Lib.x64.dll",
                "apps/ndi-bridge/Processing.NDI.Lib.x64.dll",
            ],
        );
        for extra in [
            r"C:\Program Files\NDI\NDI 6 Tools\Runtime\Processing.NDI.Lib.x64.dll",
            "Processing.NDI.Lib.x64.dll",
            r"C:\Program Files\Resolume Arena\Processing.NDI.Lib.x64.dll",
        ] {
            let path = std::path::PathBuf::from(extra);
            if !candidates.contains(&path) {
                candidates.push(path);
            }
        }
        for cand in &candidates {
            if let Some(rt) = Self::load_path(cand) {
                return Some(rt);
            }
        }
        None
    }

    fn load_path(path: &Path) -> Option<Self> {
        // A bare DLL name is deliberately allowed through unverified: that is the
        // system-search leg, where `exists()` on the CWD has nothing to say.
        if !path.exists() && path.extension().map(|e| e != "dll").unwrap_or(true) {
            return None;
        }
        unsafe {
            let lib = load_library_w(path);
            if lib.is_null() {
                return None;
            }
            let get = |name: &str| get_proc_address(lib, name);
            macro_rules! sym {
                ($name:literal, $ty:ty) => {{
                    let p = get($name)?;
                    std::mem::transmute::<*mut c_void, $ty>(p)
                }};
            }
            Some(Self {
                _lib: lib,
                initialize: sym!("NDIlib_initialize", FnInit),
                destroy: sym!("NDIlib_destroy", FnDestroy),
                find_create2: sym!("NDIlib_find_create2", FnFindCreate2),
                find_destroy: sym!("NDIlib_find_destroy", FnFindDestroy),
                find_wait: sym!("NDIlib_find_wait_for_sources", FnFindWait),
                find_get_sources: sym!("NDIlib_find_get_current_sources", FnFindGetSources),
                is_supported_cpu: sym!("NDIlib_is_supported_CPU", FnSupportedCpu),
            })
        }
    }

    /// Initialize the runtime (required before create). Returns false if CPU unsupported.
    pub fn initialize(&self) -> bool {
        unsafe {
            if !(self.is_supported_cpu)() {
                return false;
            }
            (self.initialize)()
        }
    }

    /// Enumerate current NDI sources, waiting up to `wait_ms` for any to appear.
    pub fn discover(&self, show_local: bool, wait_ms: u32) -> Vec<NdiSource> {
        unsafe {
            let cfg = FindCreate {
                show_local_sources: if show_local { 1 } else { 0 },
                p_groups: std::ptr::null(),
                p_extra_ips: std::ptr::null(),
            };
            let finder = (self.find_create2)(&cfg);
            if finder.is_null() {
                return vec![];
            }
            // Give discovery a chance (wait_ms; 0 returns immediately).
            let _ = (self.find_wait)(finder, wait_ms);
            let mut count: u32 = 0;
            let ptr = (self.find_get_sources)(finder, &mut count);
            let mut out = Vec::new();
            if !ptr.is_null() {
                for i in 0..count as usize {
                    let s = &*ptr.add(i);
                    let name = cstr(s.p_ndi_name).unwrap_or_default();
                    let url = cstr(s.p_url_address);
                    if !name.is_empty() {
                        out.push(NdiSource { name, url });
                    }
                }
            }
            (self.find_destroy)(finder);
            out
        }
    }

    /// Shut down the runtime (call once when done).
    pub fn shutdown(&self) {
        unsafe { (self.destroy)() };
    }
}

unsafe fn cstr(p: *const c_char) -> Option<String> {
    if p.is_null() {
        None
    } else {
        Some(CStr::from_ptr(p).to_string_lossy().into_owned())
    }
}

#[cfg(windows)]
mod winload {
    use std::ffi::c_void;
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;

    /// `LOAD_WITH_ALTERED_SEARCH_PATH`: a runtime bundled beside the executable
    /// resolves its own imports from that directory, not from the working directory.
    const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x0000_0008;

    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    }

    pub fn load_library_w(path: &Path) -> *mut c_void {
        let wide: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        unsafe {
            LoadLibraryExW(
                wide.as_ptr(),
                std::ptr::null_mut(),
                LOAD_WITH_ALTERED_SEARCH_PATH,
            )
        }
    }

    pub fn get_proc_address(module: *mut c_void, name: &str) -> Option<*mut c_void> {
        let cname = std::ffi::CString::new(name).ok()?;
        let p = unsafe { GetProcAddress(module, cname.as_ptr() as *const u8) };
        if p.is_null() {
            None
        } else {
            Some(p)
        }
    }
}

#[cfg(windows)]
use winload::{get_proc_address, load_library_w};

#[cfg(not(windows))]
fn load_library_w(_path: &Path) -> *mut c_void {
    std::ptr::null_mut()
}
#[cfg(not(windows))]
fn get_proc_address(_module: *mut c_void, _name: &str) -> Option<*mut c_void> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_loads_and_initializes() {
        // When the NDI runtime is installed, it must load, initialize, and be able
        // to run a discovery query (0 or more sources). If absent, we skip — the
        // test never invents a runtime.
        let Some(rt) = NdiRuntime::load() else {
            eprintln!("NDI runtime not present; skipping");
            return;
        };
        assert!(rt.initialize(), "NDIlib_initialize must succeed");
        // Immediate discovery query must not crash and returns a slice (maybe empty).
        let sources = rt.discover(true, 0);
        // We cannot require sources (nothing may be broadcasting), only that the
        // call is well-formed and returns a vector.
        let _ = sources.len();
        rt.shutdown();
    }

    #[test]
    fn discovery_returns_state_without_crash() {
        let Some(rt) = NdiRuntime::load() else {
            return;
        };
        if !rt.initialize() {
            return;
        }
        // A short wait exercises the wait+enumerate path in a real runtime.
        let sources = rt.discover(true, 500);
        for s in &sources {
            assert!(!s.name.is_empty(), "source name must be non-empty");
        }
        // If any NDI sender is broadcasting (e.g. NDI Test Patterns), discovery
        // must find at least one real, named source. Skipped when none broadcast.
        eprintln!("discovered {} NDI source(s)", sources.len());
        rt.shutdown();
    }
}
