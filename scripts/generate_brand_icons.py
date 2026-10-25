"""Derive native icons from the full approved canvas. Requires Pillow; no crop/padding change."""
from pathlib import Path
import struct
import zlib
from PIL import Image
ROOT = Path(__file__).resolve().parents[1]
APP = "ClipStrike"

def png_payload(image):
    """Deterministic filter-0 PNG with stored DEFLATE blocks (no platform compressor)."""
    width, height = image.size
    rgba = image.tobytes()
    scanlines = b"".join(b"\0" + rgba[y * width * 4:(y + 1) * width * 4] for y in range(height))
    blocks = []
    for offset in range(0, len(scanlines), 65535):
        block = scanlines[offset:offset + 65535]
        final = int(offset + len(block) == len(scanlines))
        blocks.append(bytes([final]) + struct.pack("<HH", len(block), len(block) ^ 65535) + block)
    compressed = b"\x78\x01" + b"".join(blocks) + struct.pack(">I", zlib.adler32(scanlines))
    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", width, height, 8, 6, 0, 0, 0))
            + chunk(b"IDAT", compressed) + chunk(b"IEND", b""))

source = Image.open(ROOT / f"assets/branding/{APP}.png").convert("RGBA")
(ROOT / f"assets/branding/{APP}-128.rgba").write_bytes(source.resize((128, 128), Image.Resampling.LANCZOS).tobytes())
if APP != "AnimateStrike":
    sizes = (16, 24, 32, 48, 64, 128, 256)
    entries, payloads = [], []
    offset = 6 + 16 * len(sizes)
    for size in sizes:
        payload = png_payload(source.resize((size, size), Image.Resampling.LANCZOS))
        entries.append(struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(payload), offset))
        payloads.append(payload)
        offset += len(payload)
    (ROOT / f"assets/branding/{APP}.ico").write_bytes(struct.pack("<HHH", 0, 1, len(sizes)) + b"".join(entries + payloads))
