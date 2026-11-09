// NDI C++ shim: compiles against the official NDI SDK headers so struct layouts
// are guaranteed correct (no ABI guessing), and exposes a flat C ABI that Rust
// loads at runtime. Provides live RECEIVE (any source -> tightly packed RGBA8)
// and SEND (RGBA8 -> NDI). The host must have the NDI runtime DLL available.
//
// Build with the NDI SDK `Include/` on the include path and link
// `Processing.NDI.Lib.x64.lib` (import lib). The shim itself does not require the
// runtime at compile time beyond the import lib.

#include "Processing.NDI.Lib.h"

#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

namespace {
struct RecvCtx {
    NDIlib_recv_instance_t recv = nullptr;
    int width = 0;
    int height = 0;
    std::vector<uint8_t> rgba;  // reused buffer
};
struct SendCtx {
    NDIlib_send_instance_t send = nullptr;
    std::string name;
};
}  // namespace

extern "C" {

__declspec(dllexport) int ndi_shim_initialize(void) {
    return NDIlib_initialize() ? 1 : 0;
}

__declspec(dllexport) void ndi_shim_destroy(void) {
    NDIlib_destroy();
}

// ---- Receive -------------------------------------------------------------

// Create a receiver connecting to the named source. Returns an opaque ctx or null.
__declspec(dllexport) void* ndi_shim_recv_create(const char* source_name) {
    if (!source_name) return nullptr;
    auto* ctx = new RecvCtx();
    NDIlib_source_t src;
    src.p_ndi_name = source_name;
    src.p_url_address = nullptr;
    NDIlib_recv_create_v3_t settings;
    settings.source_to_connect_to = src;
    settings.color_format = NDIlib_recv_color_format_RGBX_RGBA;  // deliver RGBA
    settings.bandwidth = NDIlib_recv_bandwidth_highest;
    settings.allow_video_fields = true;
    settings.p_ndi_recv_name = nullptr;
    ctx->recv = NDIlib_recv_create_v3(&settings);
    if (!ctx->recv) {
        delete ctx;
        return nullptr;
    }
    return ctx;
}

// Capture one video frame into an internal RGBA buffer.
// out_width/out_height are filled. Returns 1 on a video frame, 0 on none/timeout.
__declspec(dllexport) int ndi_shim_recv_frame(void* handle, int timeout_ms,
                                              int* out_width, int* out_height,
                                              const uint8_t** out_rgba, int* out_stride) {
    if (!handle) return 0;
    auto* ctx = static_cast<RecvCtx*>(handle);
    NDIlib_video_frame_v2_t video;
    NDIlib_frame_type_e ft = NDIlib_recv_capture_v3(
        ctx->recv, &video, nullptr, nullptr, (uint32_t)timeout_ms);
    if (ft != NDIlib_frame_type_video) {
        if (ft == NDIlib_frame_type_error) return -1;
        return 0;
    }
    const int w = video.xres;
    const int h = video.yres;
    const int stride = video.line_stride_in_bytes;
    ctx->rgba.resize((size_t)w * h * 4);
    // Source was requested as RGBX_RGBA, which delivers 4 bytes/pixel; copy row by row.
    for (int y = 0; y < h; ++y) {
        std::memcpy(ctx->rgba.data() + (size_t)y * w * 4,
                    video.p_data + (size_t)y * stride, (size_t)w * 4);
    }
    ctx->width = w;
    ctx->height = h;
    NDIlib_recv_free_video_v2(ctx->recv, &video);
    if (out_width) *out_width = w;
    if (out_height) *out_height = h;
    if (out_stride) *out_stride = w * 4;
    if (out_rgba) *out_rgba = ctx->rgba.data();
    return 1;
}

__declspec(dllexport) void ndi_shim_recv_destroy(void* handle) {
    if (!handle) return;
    auto* ctx = static_cast<RecvCtx*>(handle);
    if (ctx->recv) NDIlib_recv_destroy(ctx->recv);
    delete ctx;
}

// ---- Send ----------------------------------------------------------------

// Create a sender broadcasting `name` at width x height. Returns opaque ctx.
__declspec(dllexport) void* ndi_shim_send_create(const char* name, int width, int height) {
    if (!name) return nullptr;
    auto* ctx = new SendCtx();
    ctx->name = name;
    NDIlib_send_create_t settings;
    settings.p_ndi_name = const_cast<char*>(ctx->name.c_str());
    settings.p_groups = nullptr;
    settings.clock_video = true;
    settings.clock_audio = false;
    ctx->send = NDIlib_send_create(&settings);
    if (!ctx->send) {
        delete ctx;
        return nullptr;
    }
    (void)width;
    (void)height;
    return ctx;
}

// Send one RGBA8 frame (tightly packed, stride = width*4).
__declspec(dllexport) int ndi_shim_send_frame(void* handle, const uint8_t* rgba,
                                              int width, int height) {
    if (!handle || !rgba) return 0;
    auto* ctx = static_cast<SendCtx*>(handle);
    NDIlib_video_frame_v2_t frame;
    std::memset(&frame, 0, sizeof(frame));
    frame.xres = width;
    frame.yres = height;
    frame.FourCC = NDIlib_FourCC_video_type_RGBA;
    frame.frame_rate_N = 60000;
    frame.frame_rate_D = 1000;
    frame.picture_aspect_ratio = (float)width / (float)height;
    frame.frame_format_type = NDIlib_frame_format_type_progressive;
    frame.p_data = const_cast<uint8_t*>(rgba);
    frame.line_stride_in_bytes = width * 4;
    frame.p_metadata = nullptr;
    frame.timestamp = NDIlib_recv_timestamp_undefined;
    NDIlib_send_send_video_v2(ctx->send, &frame);
    return 1;
}

__declspec(dllexport) void ndi_shim_send_destroy(void* handle) {
    if (!handle) return;
    auto* ctx = static_cast<SendCtx*>(handle);
    if (ctx->send) NDIlib_send_destroy(ctx->send);
    delete ctx;
}

}  // extern "C"
