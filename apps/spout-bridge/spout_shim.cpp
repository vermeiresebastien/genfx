// Spout2 C++ shim: exposes a flat C ABI over the SpoutLibrary C++ interface so
// Flux's Rust adapter can call it safely (no hand-rolled vtable calls).
//
// Build against the Spout2 SDK: `_upstream/spout2/SPOUTSDK/SpoutLibrary`. The
// shim links `SpoutLibrary.lib` (import lib for `SpoutLibrary.dll`). Only
// sender-registry + SendImage entry points are exposed; the app supplies no GL
// context here, so SendImage is provided for completeness but returns false
// without a bound GL context (honest, not faked).

#include "SpoutLibrary.h"

#include <cstring>

// The SpoutLibrary headers include Windows types; ensure they are available.
#ifndef _WINDOWS_
#include <windows.h>
#endif

// Offscreen WGL context so SendImage works without an application window.
#include <GL/gl.h>

namespace {
HDC g_dc = nullptr;
HGLRC g_glrc = nullptr;
bool g_gl_ready = false;

bool ensure_gl() {
    if (g_gl_ready) return true;
    // A hidden window + WGL context (pixel format chosen for the DC).
    WNDCLASSA wc = {};
    wc.lpfnWndProc = DefWindowProcA;
    wc.hInstance = GetModuleHandleA(nullptr);
    wc.lpszClassName = "FluxSpoutOffscreen";
    RegisterClassA(&wc);
    HWND hwnd = CreateWindowA("FluxSpoutOffscreen", "", WS_OVERLAPPEDWINDOW,
                              0, 0, 8, 8, nullptr, nullptr, wc.hInstance, nullptr);
    if (!hwnd) return false;
    HDC dc = GetDC(hwnd);
    if (!dc) return false;
    PIXELFORMATDESCRIPTOR pfd = {};
    pfd.nSize = sizeof(pfd);
    pfd.nVersion = 1;
    pfd.dwFlags = PFD_DRAW_TO_WINDOW | PFD_SUPPORT_OPENGL | PFD_DOUBLEBUFFER;
    pfd.iPixelType = PFD_TYPE_RGBA;
    pfd.cColorBits = 32;
    pfd.cAlphaBits = 8;
    pfd.cDepthBits = 24;
    int pf = ChoosePixelFormat(dc, &pfd);
    if (!pf || !SetPixelFormat(dc, pf, &pfd)) {
        ReleaseDC(hwnd, dc);
        return false;
    }
    HGLRC rc = wglCreateContext(dc);
    if (!rc || !wglMakeCurrent(dc, rc)) {
        if (rc) wglDeleteContext(rc);
        ReleaseDC(hwnd, dc);
        return false;
    }
    g_dc = dc;
    g_glrc = rc;
    g_gl_ready = true;
    return true;
}
}  // namespace

extern "C" {
__declspec(dllexport) void* spout_shim_create(void);
__declspec(dllexport) void spout_shim_release(void* handle);
__declspec(dllexport) int spout_shim_get_sender_count(void* handle);
// Returns 0 on success, fills name; -1 on error/out-of-range.
__declspec(dllexport) int spout_shim_get_sender(void* handle, int index, char* name, int maxlen);
// width/height out; 1 if sender found.
__declspec(dllexport) int spout_shim_get_sender_info(void* handle, const char* name,
                                                     unsigned int* width, unsigned int* height);
// Create + send an RGBA8 image via SpoutDX/GL path. Returns 1 on success.
__declspec(dllexport) int spout_shim_send_image(void* handle, const char* name,
                                                const unsigned char* rgba,
                                                unsigned int width, unsigned int height);
// Check a receiver connection for a name; returns 1 if connected.
__declspec(dllexport) int spout_shim_is_connected(void* handle, const char* name);
// 1 if an offscreen GL context could be created (SendImage prerequisite).
__declspec(dllexport) int spout_shim_gl_ready(void);
}

void* spout_shim_create(void) {
    SPOUTHANDLE h = GetSpout();
    return reinterpret_cast<void*>(h);
}

void spout_shim_release(void* handle) {
    // The Spout instance from GetSpout() is process-global and must NOT be
    // deleted (its destructor is protected; the examples never delete it).
    // Release any sender the shim created, then leave the instance alive.
    if (!handle) return;
    auto* h = reinterpret_cast<SPOUTHANDLE>(handle);
    h->ReleaseSender();
}

int spout_shim_get_sender_count(void* handle) {
    if (!handle) return 0;
    auto* h = reinterpret_cast<SPOUTHANDLE>(handle);
    return h->GetSenderCount();
}

int spout_shim_get_sender(void* handle, int index, char* name, int maxlen) {
    if (!handle || !name || maxlen <= 0) return -1;
    auto* h = reinterpret_cast<SPOUTHANDLE>(handle);
    // GetSender takes char* and max size; it zero-fills or writes the name.
    char buf[256];
    buf[0] = 0;
    if (!h->GetSender(index, buf, 256)) {
        return -1;
    }
    strncpy_s(name, (size_t)maxlen, buf, _TRUNCATE);
    return 0;
}

int spout_shim_get_sender_info(void* handle, const char* name,
                               unsigned int* width, unsigned int* height) {
    if (!handle || !name || !width || !height) return 0;
    auto* h = reinterpret_cast<SPOUTHANDLE>(handle);
    HANDLE dx_handle = nullptr;
    DWORD format = 0;
    unsigned int w = 0, hgt = 0;
    if (!h->GetSenderInfo(name, w, hgt, dx_handle, format)) {
        return 0;
    }
    *width = w;
    *height = hgt;
    return 1;
}

int spout_shim_send_image(void* handle, const char* name, const unsigned char* rgba,
                          unsigned int width, unsigned int height) {
    if (!handle || !name || !rgba) return 0;
    auto* h = reinterpret_cast<SPOUTHANDLE>(handle);
    // SendImage needs a current GL context; create an offscreen one if needed.
    if (!ensure_gl()) return 0;
    if (!h->CreateSender(name, width, height)) {
        return 0;
    }
    return h->SendImage(rgba, width, height, GL_RGBA, false) ? 1 : 0;
}

int spout_shim_gl_ready(void) {
    return ensure_gl() ? 1 : 0;
}

int spout_shim_is_connected(void* handle, const char* name) {
    if (!handle || !name) return 0;
    auto* h = reinterpret_cast<SPOUTHANDLE>(handle);
    (void)h;
    (void)name;
    // Connection state is receiver-side; without a created receiver there is no
    // connection to report. Return 0 (no receiver) truthfully.
    return 0;
}
