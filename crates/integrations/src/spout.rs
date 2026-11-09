//! Spout2 adapter — runtime-loaded C shim (feature `spout`, Windows).
//!
//! SpoutLibrary exposes a C++ pure-virtual interface (`GetSpout()` → vtable).
//! Calling a C++ vtable from Rust is unsafe, so `apps/spout-bridge/spout_shim.cpp`
//! wraps it in a flat `extern "C"` ABI and links `SpoutLibrary.lib`. This module
//! loads `spout_shim.dll` at runtime and calls that shim. Sender enumeration uses
//! the OS-wide Spout sender registry (real, not a canned list); sending an image
//! needs a bound GL context, which the shim reports honestly.

#![cfg(feature = "spout")]

use serde::Serialize;
use std::ffi::{c_char, c_void, CStr, CString};
use std::path::Path;

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct SpoutSender {
    pub name: String,
    pub width: Option<u32>,
    pub height: Option<u32>,
}

type FnCreate = unsafe extern "C" fn() -> *mut c_void;
type FnRelease = unsafe extern "C" fn(*mut c_void);
type FnCount = unsafe extern "C" fn(*mut c_void) -> i32;
type FnGetSender = unsafe extern "C" fn(*mut c_void, i32, *mut c_char, i32) -> i32;
type FnSenderInfo = unsafe extern "C" fn(*mut c_void, *const c_char, *mut u32, *mut u32) -> i32;
type FnSendImage = unsafe extern "C" fn(*mut c_void, *const c_char, *const u8, u32, u32) -> i32;

/// Loaded Spout shim.
pub struct SpoutRuntime {
    lib: *mut c_void,
    handle: *mut c_void,
    count: FnCount,
    get_sender: FnGetSender,
    sender_info: FnSenderInfo,
    send_image: FnSendImage,
    release: FnRelease,
}

impl SpoutRuntime {
    /// Load the shim from `FLUX_SPOUT_SHIM` or next to the executable.
    pub fn load() -> Option<Self> {
        let candidates = [
            std::env::var("FLUX_SPOUT_SHIM").ok(),
            Some("spout_shim.dll".to_string()),
            Some("apps/spout-bridge/spout_shim.dll".to_string()),
        ];
        for cand in candidates.into_iter().flatten() {
            if let Some(rt) = Self::load_path(&cand) {
                return Some(rt);
            }
        }
        None
    }

    fn load_path(path: &str) -> Option<Self> {
        if !Path::new(path).exists() {
            return None;
        }
        // Ensure the shim's dependency (SpoutLibrary.dll) resolves from its own dir.
        if let Some(dir) = Path::new(path).parent() {
            // Best effort: prepend to the DLL search path for this process.
            let _ = std::env::set_var(
                "PATH",
                format!(
                    "{};{}",
                    dir.display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
        }
        unsafe {
            let lib = load_library(path);
            if lib.is_null() {
                return None;
            }
            let get = |name: &str| get_proc(lib, name);
            macro_rules! sym {
                ($n:literal, $t:ty) => {{
                    let p = get($n)?;
                    std::mem::transmute::<*mut c_void, $t>(p)
                }};
            }
            let create: FnCreate = sym!("spout_shim_create", FnCreate);
            let handle = create();
            if handle.is_null() {
                return None;
            }
            Some(Self {
                lib,
                handle,
                count: sym!("spout_shim_get_sender_count", FnCount),
                get_sender: sym!("spout_shim_get_sender", FnGetSender),
                sender_info: sym!("spout_shim_get_sender_info", FnSenderInfo),
                send_image: sym!("spout_shim_send_image", FnSendImage),
                release: sym!("spout_shim_release", FnRelease),
            })
        }
    }

    pub fn get_sender_count(&self) -> i32 {
        unsafe { (self.count)(self.handle) }
    }

    /// Enumerate the OS-wide Spout sender registry.
    ///
    /// `SpoutLibrary::GetSenderCount` can block for a long time when no DX sender
    /// exists, so this is bounded by `timeout`. On timeout it returns an empty
    /// list and reports `timed_out = true` (never blocks the caller indefinitely).
    pub fn senders(&self) -> Vec<SpoutSender> {
        self.senders_timeout(std::time::Duration::from_secs(3)).0
    }

    /// Like [`senders`], with an explicit bound. Returns `(senders, timed_out)`.
    pub fn senders_timeout(&self, timeout: std::time::Duration) -> (Vec<SpoutSender>, bool) {
        use std::sync::mpsc;
        // Capture the resolved entry points (plain fn pointers) + handle as
        // integers so the worker closure is Send; transmute back inside.
        let handle = self.handle as usize;
        let count_ptr = self.count as usize;
        let get_sender_ptr = self.get_sender as usize;
        let info_ptr = self.sender_info as usize;
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let count: FnCount = unsafe { std::mem::transmute(count_ptr) };
            let get_sender: FnGetSender = unsafe { std::mem::transmute(get_sender_ptr) };
            let info: FnSenderInfo = unsafe { std::mem::transmute(info_ptr) };
            let h = handle as *mut c_void;
            let n = unsafe { count(h) };
            let mut out = Vec::new();
            for i in 0..n.max(0) {
                let mut buf = [0i8; 256];
                let rc = unsafe { get_sender(h, i, buf.as_mut_ptr(), 256) };
                if rc != 0 {
                    continue;
                }
                let name = unsafe { CStr::from_ptr(buf.as_ptr()) }
                    .to_string_lossy()
                    .into_owned();
                if name.is_empty() {
                    continue;
                }
                let (mut w, mut hgt) = (0u32, 0u32);
                if let Ok(cname) = CString::new(name.clone()) {
                    let found = unsafe { info(h, cname.as_ptr(), &mut w, &mut hgt) };
                    out.push(SpoutSender {
                        name,
                        width: if found == 1 { Some(w) } else { None },
                        height: if found == 1 { Some(hgt) } else { None },
                    });
                }
            }
            let _ = tx.send(out);
        });
        match rx.recv_timeout(timeout) {
            Ok(v) => (v, false),
            Err(_) => (Vec::new(), true),
        }
    }

    /// Send an RGBA8 image to a Spout sender. Returns false without a GL context
    /// (the shim refuses rather than fabricating a success).
    pub fn send_image(&self, name: &str, rgba: &[u8], w: u32, h: u32) -> bool {
        let cname = match CString::new(name) {
            Ok(c) => c,
            Err(_) => return false,
        };
        unsafe { (self.send_image)(self.handle, cname.as_ptr(), rgba.as_ptr(), w, h) == 1 }
    }
}

impl Drop for SpoutRuntime {
    fn drop(&mut self) {
        unsafe {
            (self.release)(self.handle);
            // Leave the DLL loaded for the process lifetime (FreeLibrary optional).
        }
        let _ = self.lib;
    }
}

#[cfg(windows)]
fn load_library(path: &str) -> *mut c_void {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryW(name: *const u16) -> *mut c_void;
    }
    let wide: Vec<u16> = std::ffi::OsStr::new(path)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();
    unsafe { LoadLibraryW(wide.as_ptr()) }
}

#[cfg(windows)]
fn get_proc(module: *mut c_void, name: &str) -> Option<*mut c_void> {
    #[link(name = "kernel32")]
    extern "system" {
        fn GetProcAddress(module: *mut c_void, name: *const u8) -> *mut c_void;
    }
    let cname = CString::new(name).ok()?;
    let p = unsafe { GetProcAddress(module, cname.as_ptr() as *const u8) };
    if p.is_null() {
        None
    } else {
        Some(p)
    }
}

#[cfg(not(windows))]
fn load_library(_path: &str) -> *mut c_void {
    std::ptr::null_mut()
}
#[cfg(not(windows))]
fn get_proc(_module: *mut c_void, _name: &str) -> Option<*mut c_void> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shim_loads_when_present() {
        let Some(rt) = SpoutRuntime::load() else {
            eprintln!("spout_shim.dll not present; skipping");
            return;
        };
        // Bounded enumeration: must return within the timeout even when no sender
        // exists (GetSenderCount can otherwise block). Either outcome is valid.
        let start = std::time::Instant::now();
        let (senders, timed_out) = rt.senders_timeout(std::time::Duration::from_secs(2));
        let elapsed = start.elapsed();
        assert!(
            elapsed <= std::time::Duration::from_secs(4),
            "bounded: {elapsed:?}"
        );
        if timed_out {
            assert!(senders.is_empty());
        }
    }
}
