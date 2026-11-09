// Standalone test: verify SpoutDX D3D11 sender works without a DLL. Build as EXE.
#include "SpoutDX.h"
#include <cstdio>
#include <vector>

int main() {
    spoutDX spout;
    if (!spout.OpenDirectX11()) {
        printf("OpenDirectX11 FAILED\n");
        return 1;
    }
    printf("device OK\n");
    spout.SetSenderName("Flux DX Test EXE");
    std::vector<unsigned char> img(640 * 360 * 4, 200);
    bool ok = false;
    for (int i = 0; i < 30; ++i) {
        ok = spout.SendImage(img.data(), 640, 360);
        Sleep(33);
    }
    printf("SendImage last result=%d\n", ok ? 1 : 0);
    spout.ReleaseSender();
    spout.CloseDirectX11();
    return 0;
}
