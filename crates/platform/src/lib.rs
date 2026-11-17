//! Platform capture adapters.
//!
//! Windows: WASAPI audio endpoint enumeration (microphone capture endpoints and
//! the default render endpoint used for system loopback). This performs real
//! device enumeration on Windows. Camera enumeration and live frame capture
//! require a device + permission and are reported through the source contract;
//! non-Windows builds return a clearly-labelled capability set.

use serde::Serialize;

/// Live audio capture: WASAPI endpoints, feature-gated. The type definitions
/// compile everywhere; only the implementation is behind `live-audio`, so the
/// app layer can name these types without a `#[cfg]` of its own.
pub mod capture;

/// Where sidecar binaries live for this process — see the module's own note on
/// why every pre-existing candidate was working-directory-relative.
pub mod layout;

#[derive(Debug, Clone, Serialize)]
pub struct DeviceInfo {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub is_default: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CaptureCapabilities {
    pub platform: String,
    pub cameras: Vec<DeviceInfo>,
    pub microphones: Vec<DeviceInfo>,
    pub loopback: Vec<DeviceInfo>,
    pub camera_available: bool,
    pub loopback_available: bool,
    pub limitation: String,
}

/// Enumerate capture capabilities for the running host.
pub fn capture_capabilities() -> CaptureCapabilities {
    #[cfg(windows)]
    {
        win::enumerate()
    }
    #[cfg(not(windows))]
    {
        CaptureCapabilities {
            platform: std::env::consts::OS.into(),
            cameras: vec![],
            microphones: vec![],
            loopback: vec![],
            camera_available: false,
            loopback_available: false,
            limitation: "capture adapters are implemented for Windows x64 only".into(),
        }
    }
}

#[cfg(windows)]
mod win {
    use super::{CaptureCapabilities, DeviceInfo};
    use windows::Win32::Media::Audio::{
        eCapture, eConsole, eRender, EDataFlow, IMMDevice, IMMDeviceEnumerator, MMDeviceEnumerator,
        DEVICE_STATE_ACTIVE,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };

    struct ComGuard {
        initialized: bool,
    }
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.initialized {
                unsafe { CoUninitialize() };
            }
        }
    }

    unsafe fn endpoints(flow: EDataFlow, kind: &str) -> Vec<DeviceInfo> {
        let mut out = vec![];
        let Ok(enumerator) =
            CoCreateInstance::<_, IMMDeviceEnumerator>(&MMDeviceEnumerator, None, CLSCTX_ALL)
        else {
            return out;
        };
        let Ok(collection) = enumerator.EnumAudioEndpoints(flow, DEVICE_STATE_ACTIVE) else {
            return out;
        };
        let default_id = enumerator
            .GetDefaultAudioEndpoint(flow, eConsole)
            .ok()
            .map(|d| endpoint_id(&d));
        let count = collection.GetCount().unwrap_or(0);
        for i in 0..count {
            if let Ok(device) = collection.Item(i) {
                let id = endpoint_id(&device);
                let name = endpoint_name(&device, &id, kind, i);
                out.push(DeviceInfo {
                    is_default: !id.is_empty() && Some(&id) == default_id.as_ref(),
                    id,
                    name,
                    kind: kind.to_string(),
                });
            }
        }
        out
    }

    /// The endpoint id, with the CoTaskMem block freed. `IMMDevice::GetId`
    /// hands back a `PWSTR` the newtype does not own, so the old code leaked
    /// one string per device per enumeration.
    fn endpoint_id(device: &IMMDevice) -> String {
        unsafe {
            device
                .GetId()
                .ok()
                .map(|pw| {
                    let s = pw.to_string().unwrap_or_default();
                    CoTaskMemFree(Some(pw.as_ptr() as *const core::ffi::c_void));
                    s
                })
                .unwrap_or_default()
        }
    }

    /// What to call this endpoint.
    ///
    /// The readable name is `PKEY_Device_FriendlyName` from the device's
    /// property store. Reading it needs four windows API surfaces that this
    /// crate only enables together with `live-audio`, so a build without that
    /// feature keeps the previous id-tail label — which is why `flux.exe
    /// capture` used to print `microphone { 00000000-... }` for what Windows
    /// calls `Input (7- SSL 2 USB Audio Device)`.
    #[cfg_attr(not(feature = "live-audio"), allow(unused_variables))]
    fn endpoint_name(device: &IMMDevice, id: &str, kind: &str, i: u32) -> String {
        #[cfg(feature = "live-audio")]
        {
            if let Some(name) = crate::capture::friendly_name(device) {
                return name;
            }
        }
        let short = id.rsplit('.').next().unwrap_or(id);
        if short.is_empty() {
            format!("{kind} {i}")
        } else {
            format!("{kind} {{ {short} }}")
        }
    }

    /// Best-effort camera enumeration via the PnP imaging device class key in the
    /// registry (what Media Foundation reads its camera list from).
    fn camera_devices() -> Vec<DeviceInfo> {
        use windows::core::HSTRING;
        use windows::Win32::Foundation::ERROR_SUCCESS;
        use windows::Win32::System::Registry::{
            RegCloseKey, RegEnumKeyExW, RegOpenKeyExW, HKEY_LOCAL_MACHINE, KEY_READ,
        };
        let mut out = Vec::new();
        let path = HSTRING::from(
            "SYSTEM\\CurrentControlSet\\Control\\DeviceClasses\\{e5323777-f976-4f5b-9b55-b94699c46e44}",
        );
        unsafe {
            let mut key = windows::Win32::System::Registry::HKEY::default();
            if RegOpenKeyExW(HKEY_LOCAL_MACHINE, &path, Some(0), KEY_READ, &mut key)
                != ERROR_SUCCESS
            {
                return out;
            }
            let mut idx = 0u32;
            loop {
                let mut name_buf = [0u16; 256];
                let mut len = name_buf.len() as u32;
                let rc = RegEnumKeyExW(
                    key,
                    idx,
                    Some(windows::core::PWSTR(name_buf.as_mut_ptr())),
                    &mut len,
                    None,
                    None,
                    None,
                    None,
                );
                if rc == ERROR_SUCCESS {
                    let name = String::from_utf16_lossy(&name_buf[..len as usize]);
                    out.push(DeviceInfo {
                        id: name.clone(),
                        name: format!("camera {{ {name} }}"),
                        kind: "camera".into(),
                        is_default: idx == 0,
                    });
                    idx += 1;
                } else {
                    break;
                }
            }
            let _ = RegCloseKey(key);
        }
        out
    }

    pub fn enumerate() -> CaptureCapabilities {
        unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            let _guard = ComGuard {
                initialized: hr.is_ok(),
            };
            let microphones = endpoints(eCapture, "microphone");
            let loopback = endpoints(eRender, "loopback");
            let cameras = camera_devices();
            CaptureCapabilities {
                platform: "windows".into(),
                camera_available: !cameras.is_empty(),
                loopback_available: !loopback.is_empty(),
                cameras,
                microphones,
                loopback,
                limitation: "WASAPI endpoints + PnP camera class enumerated for real; \
live frame capture needs a device and OS permission"
                    .into(),
            }
        }
    }
}
