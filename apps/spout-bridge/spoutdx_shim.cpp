// Spout2 DX11 sender shim: uses SpoutDX (D3D11) to send RGBA8 frames WITHOUT an
// OpenGL context. spoutDX::SendImage() uploads the pixel buffer to its own D3D11
// texture internally and creates the sender on first send. This is the correct
// Windows path for a headless / non-GL host.
//
// Flat C ABI consumed by Rust (crates/integrations/src/spout.rs, feature `spout`).

#include "SpoutDX.h"

#include <vector>

namespace {
struct DxSender {
    spoutDX spout;
    bool ready = false;
};
}  // namespace

extern "C" {

// Create a DX sender. Returns opaque handle or null.
__declspec(dllexport) void* spoutdx_shim_create_sender(const char* name, int width, int height) {
    if (!name || width <= 0 || height <= 0) return nullptr;
    auto* s = new DxSender();
    // OpenDirectX11 creates its own D3D11 device if none is supplied.
    if (!s->spout.OpenDirectX11()) {
        delete s;
        return nullptr;
    }
    if (!s->spout.SetSenderName(name)) {
        delete s;
        return nullptr;
    }
    s->ready = true;
    return s;
}

// Send one tightly packed RGBA8 frame (stride = width*4). Returns 1 on success.
// spoutDX::SendImage expects BGRA (DXGI B8G8R8A8); swizzle from RGBA first.
__declspec(dllexport) int spoutdx_shim_send(void* handle, const unsigned char* rgba,
                                            int width, int height) {
    if (!handle || !rgba) return 0;
    auto* s = static_cast<DxSender*>(handle);
    if (!s->ready) return 0;
    size_t n = (size_t)width * height;
    static thread_local std::vector<unsigned char> bgra;
    bgra.resize(n * 4);
    for (size_t i = 0; i < n; ++i) {
        bgra[i * 4 + 0] = rgba[i * 4 + 2]; // B
        bgra[i * 4 + 1] = rgba[i * 4 + 1]; // G
        bgra[i * 4 + 2] = rgba[i * 4 + 0]; // R
        bgra[i * 4 + 3] = rgba[i * 4 + 3]; // A
    }
    return s->spout.SendImage(bgra.data(), (unsigned int)width, (unsigned int)height) ? 1 : 0;
}

__declspec(dllexport) void spoutdx_shim_destroy(void* handle) {
    if (!handle) return;
    auto* s = static_cast<DxSender*>(handle);
    s->spout.ReleaseSender();
    s->spout.CloseDirectX11();
    delete s;
}

// Diagnostics: bitmask 1 = initialized, 2 = ready(name set).
__declspec(dllexport) int spoutdx_shim_status(void* handle) {
    if (!handle) return 0;
    auto* s = static_cast<DxSender*>(handle);
    int m = 0;
    if (s->spout.IsInitialized()) m |= 1;
    if (s->ready) m |= 2;
    return m;
}

}  // extern "C"
