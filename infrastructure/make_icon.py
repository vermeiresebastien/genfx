import struct, os

def make_ico(path, size=32, rgba=(40, 120, 220, 255)):
    # 32-bit BGRA DIB icon.
    w = h = size
    # BITMAPINFOHEADER
    bih = struct.pack("<IiiHHIIiiII", 40, w, h * 2, 1, 32, 0, 0, 0, 0, 0, 0)
    pixels = bytearray()
    for y in range(h - 1, -1, -1):
        for x in range(w):
            b, g, r, a = rgba[2], rgba[1], rgba[0], rgba[3]
            pixels += bytes([b, g, r, a])
    mask_row = ((w + 31) // 32) * 4
    mask = bytes(mask_row * h)
    data = bih + bytes(pixels) + mask
    # ICONDIR
    icondir = struct.pack("<HHH", 0, 1, 1)
    entry = struct.pack("<BBBBHHII", w if w < 256 else 0, h if h < 256 else 0, 0, 0, 1, 32, len(data), 6 + 16)
    with open(path, "wb") as f:
        f.write(icondir + entry + data)

os.makedirs("apps/desktop/src-tauri/icons", exist_ok=True)
make_ico("apps/desktop/src-tauri/icons/icon.ico")
print("wrote icon.ico")

# Minimal PNG (32x32) via zlib
import zlib
def make_png(path, size=32, rgba=(40,120,220,255)):
    def chunk(typ, data):
        c = struct.pack(">I", len(data)) + typ + data
        return c + struct.pack(">I", zlib.crc32(typ + data) & 0xffffffff)
    ihdr = struct.pack(">IIBBBBB", size, size, 8, 6, 0, 0, 0)
    raw = b""
    for y in range(size):
        raw += b"\x00" + bytes(rgba) * size
    idat = zlib.compress(raw)
    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", ihdr) + chunk(b"IDAT", idat) + chunk(b"IEND", b"")
    open(path, "wb").write(png)
make_png("apps/desktop/src-tauri/icons/icon.png")
print("wrote icon.png")
