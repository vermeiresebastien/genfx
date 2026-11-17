//! NDI live I/O via the C++ shim (`apps/ndi-bridge/ndi_shim.cpp`, feature `ndi`).
//!
//! The shim is compiled against the **official NDI SDK headers**, so all struct
//! layouts (`NDIlib_recv_create_v3_t`, `NDIlib_video_frame_v2_t`, …) are correct by
//! construction — Rust never guesses the ABI. This module runtime-loads
//! `ndi_shim.dll` and exposes a live receiver (source → RGBA8 frames) and sender
//! (RGBA8 → NDI). Requires the NDI runtime DLL next to the shim.

#![cfg(feature = "ndi")]

use serde::Serialize;
use std::ffi::{c_char, c_void, CString};
use std::path::Path;

type FnInit = unsafe extern "C" fn() -> i32;
type FnDestroy = unsafe extern "C" fn();
type FnRecvCreate = unsafe extern "C" fn(*const c_char) -> *mut c_void;
type FnRecvFrame =
    unsafe extern "C" fn(*mut c_void, i32, *mut i32, *mut i32, *mut *const u8, *mut i32) -> i32;
type FnRecvDestroy = unsafe extern "C" fn(*mut c_void);
type FnSendCreate = unsafe extern "C" fn(*const c_char, i32, i32) -> *mut c_void;
type FnSendFrame = unsafe extern "C" fn(*mut c_void, *const u8, i32, i32) -> i32;
type FnSendDestroy = unsafe extern "C" fn(*mut c_void);

/// One received NDI video frame as tightly packed RGBA8.
#[derive(Debug, Clone, Serialize)]
pub struct NdiFrame {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Loaded NDI shim.
pub struct NdiShim {
    #[allow(dead_code)]
    lib: *mut c_void,
    initialize: FnInit,
    shim_destroy: FnDestroy,
    recv_create: FnRecvCreate,
    recv_frame: FnRecvFrame,
    recv_destroy: FnRecvDestroy,
    send_create: FnSendCreate,
    send_frame: FnSendFrame,
    send_destroy: FnSendDestroy,
}

impl NdiShim {
    /// Load `ndi_shim.dll`: `FLUX_NDI_SHIM`, then beside the executable, then the
    /// source-tree bridge directory.
    pub fn load() -> Option<Self> {
        for cand in flux_platform::layout::candidates(
            "FLUX_NDI_SHIM",
            &["ndi_shim.dll", "apps/ndi-bridge/ndi_shim.dll"],
        ) {
            if let Some(s) = Self::load_path(&cand) {
                return Some(s);
            }
        }
        None
    }

    fn load_path(path: &Path) -> Option<Self> {
        if !path.exists() {
            return None;
        }
        unsafe {
            let lib = load_library(path);
            if lib.is_null() {
                return None;
            }
            let get = |n: &str| get_proc(lib, n);
            macro_rules! sym {
                ($n:literal, $t:ty) => {{
                    let p = get($n)?;
                    std::mem::transmute::<*mut c_void, $t>(p)
                }};
            }
            Some(Self {
                lib,
                initialize: sym!("ndi_shim_initialize", FnInit),
                shim_destroy: sym!("ndi_shim_destroy", FnDestroy),
                recv_create: sym!("ndi_shim_recv_create", FnRecvCreate),
                recv_frame: sym!("ndi_shim_recv_frame", FnRecvFrame),
                recv_destroy: sym!("ndi_shim_recv_destroy", FnRecvDestroy),
                send_create: sym!("ndi_shim_send_create", FnSendCreate),
                send_frame: sym!("ndi_shim_send_frame", FnSendFrame),
                send_destroy: sym!("ndi_shim_send_destroy", FnSendDestroy),
            })
        }
    }

    /// Initialize NDI (call before any create). Returns true on success.
    pub fn initialize(&self) -> bool {
        unsafe { (self.initialize)() == 1 }
    }

    pub fn receiver(&self, source_name: &str) -> Option<NdiReceiver> {
        let cname = CString::new(source_name).ok()?;
        let h = unsafe { (self.recv_create)(cname.as_ptr()) };
        if h.is_null() {
            None
        } else {
            Some(NdiReceiver {
                handle: h,
                recv_frame: self.recv_frame,
                recv_destroy: self.recv_destroy,
            })
        }
    }

    pub fn sender(&self, name: &str, width: u32, height: u32) -> Option<NdiSender> {
        let cname = CString::new(name).ok()?;
        let h = unsafe { (self.send_create)(cname.as_ptr(), width as i32, height as i32) };
        if h.is_null() {
            None
        } else {
            Some(NdiSender {
                handle: h,
                send_frame: self.send_frame,
                send_destroy: self.send_destroy,
            })
        }
    }

    /// Finalize NDI (call once on shutdown).
    pub fn shutdown(&self) {
        unsafe { (self.shim_destroy)() };
    }
}

/// A live NDI receiver connected to a named source.
pub struct NdiReceiver {
    handle: *mut c_void,
    recv_frame: FnRecvFrame,
    recv_destroy: FnRecvDestroy,
}

impl NdiReceiver {
    /// Receive one frame, waiting up to `timeout_ms`. Returns None on timeout,
    /// Some(frame) on success; Err on connection error.
    pub fn next_frame(&self, timeout_ms: u32) -> Result<Option<NdiFrame>, String> {
        let (mut w, mut h, mut stride) = (0i32, 0i32, 0i32);
        let mut ptr: *const u8 = std::ptr::null();
        let rc = unsafe {
            (self.recv_frame)(
                self.handle,
                timeout_ms as i32,
                &mut w,
                &mut h,
                &mut ptr,
                &mut stride,
            )
        };
        match rc {
            1 => {
                if ptr.is_null() || w <= 0 || h <= 0 {
                    return Ok(None);
                }
                let len = (w as usize) * (h as usize) * 4;
                let rgba = unsafe { std::slice::from_raw_parts(ptr, len) }.to_vec();
                Ok(Some(NdiFrame {
                    width: w as u32,
                    height: h as u32,
                    rgba,
                }))
            }
            0 => Ok(None),
            _ => Err("NDI receive error (connection lost)".into()),
        }
    }
}

impl Drop for NdiReceiver {
    fn drop(&mut self) {
        unsafe { (self.recv_destroy)(self.handle) };
    }
}

/// A live NDI sender broadcasting RGBA8 frames.
pub struct NdiSender {
    handle: *mut c_void,
    send_frame: FnSendFrame,
    send_destroy: FnSendDestroy,
}

impl NdiSender {
    /// Send one tightly packed RGBA8 frame.
    pub fn send(&self, rgba: &[u8], width: u32, height: u32) -> bool {
        let expected = (width as usize) * (height as usize) * 4;
        if rgba.len() != expected {
            return false;
        }
        unsafe { (self.send_frame)(self.handle, rgba.as_ptr(), width as i32, height as i32) == 1 }
    }
}

impl Drop for NdiSender {
    fn drop(&mut self) {
        unsafe { (self.send_destroy)(self.handle) };
    }
}

/// `LOAD_WITH_ALTERED_SEARCH_PATH`, so the shim's own import
/// `Processing.NDI.Lib.x64.dll` resolves from the directory the shim is in —
/// replacing a `set_var("PATH", …)` that raced every other reader of the
/// environment and did nothing for an already-cached search path.
#[cfg(windows)]
const LOAD_WITH_ALTERED_SEARCH_PATH: u32 = 0x0000_0008;

#[cfg(windows)]
fn load_library(path: &Path) -> *mut c_void {
    use std::os::windows::ffi::OsStrExt;
    #[link(name = "kernel32")]
    extern "system" {
        fn LoadLibraryExW(name: *const u16, file: *mut c_void, flags: u32) -> *mut c_void;
    }
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
fn load_library(_path: &Path) -> *mut c_void {
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
    fn shim_loads_and_initializes() {
        let Some(shim) = NdiShim::load() else {
            eprintln!("ndi_shim.dll not present; skipping");
            return;
        };
        assert!(shim.initialize(), "NDI initialize via shim");
        // Constructing a sender validates the send path's object lifetime without
        // needing a receiver/network. (Cross-process send/receive is verified by
        // `flux ndi-send` + `flux ndi-recv`, see evidence/ndi-io.txt.)
        let sender = shim.sender("Flux Test Unit", 320, 180);
        assert!(sender.is_some(), "NDI sender created");
        drop(sender);
        shim.shutdown();
    }
}
