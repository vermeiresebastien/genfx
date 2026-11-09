// Spout2 DX11 sender helper (standalone EXE — the DLL form fails to load on this
// host with ERROR_INVALID_DLL/182, so live Spout send runs as a child process).
// Broadcasts an animated RGBA pattern for N seconds at 640x360.
//
// Usage: spout_sender.exe "<name>" <seconds>

#include "SpoutDX.h"
#include <cstdio>
#include <cstdlib>
#include <vector>
#include <string>

int main(int argc, char** argv) {
    const char* name = (argc > 1) ? argv[1] : "Flux Spout Sender";
    int secs = (argc > 2) ? atoi(argv[2]) : 10;
    if (secs <= 0) secs = 10;
    const int W = 640, H = 360;

    spoutDX spout;
    if (!spout.OpenDirectX11()) {
        printf("OpenDirectX11 FAILED\n");
        return 1;
    }
    if (!spout.SetSenderName(name)) {
        printf("SetSenderName FAILED\n");
        return 1;
    }
    printf("Spout DX sender '%s' broadcasting %dx%d for %ds\n", name, W, H, secs);
    fflush(stdout);

    std::vector<unsigned char> img((size_t)W * H * 4);
    DWORD start = GetTickCount();
    int sent = 0;
    while ((int)((GetTickCount() - start) / 1000) < secs) {
        unsigned t = (GetTickCount() - start) / 8;
        for (int y = 0; y < H; ++y) {
            for (int x = 0; x < W; ++x) {
                size_t i = ((size_t)y * W + x) * 4;
                img[i + 0] = (unsigned char)((x * 255 / W + t) % 256); // R (BGRA order handled by SendImage=RGBA? see below)
                img[i + 1] = (unsigned char)(y * 255 / H);             // G
                img[i + 2] = 128;                                      // B
                img[i + 3] = 255;                                      // A
            }
        }
        if (spout.SendImage(img.data(), W, H)) ++sent;
        Sleep(33);
    }
    printf("sent %d frames\n", sent);
    spout.ReleaseSender();
    spout.CloseDirectX11();
    return 0;
}
