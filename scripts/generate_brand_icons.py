"""Derive native icons from the full approved canvas. Requires Pillow; no crop/padding change."""
from pathlib import Path
from io import BytesIO
import struct
from PIL import Image
ROOT = Path(__file__).resolve().parents[1]
APP = "ClipStrike"
source = Image.open(ROOT / f"assets/branding/{APP}.png").convert("RGBA")
(ROOT / f"assets/branding/{APP}-128.rgba").write_bytes(source.resize((128, 128), Image.Resampling.LANCZOS).tobytes())
if APP != "AnimateStrike":
    sizes = (16, 24, 32, 48, 64, 128, 256)
    entries, payloads = [], []
    offset = 6 + 16 * len(sizes)
    for size in sizes:
        output = BytesIO()
        source.resize((size, size), Image.Resampling.LANCZOS).save(output, format="PNG", compress_level=9)
        payload = output.getvalue()
        entries.append(struct.pack("<BBBBHHII", size % 256, size % 256, 0, 0, 1, 32, len(payload), offset))
        payloads.append(payload)
        offset += len(payload)
    (ROOT / f"assets/branding/{APP}.ico").write_bytes(struct.pack("<HHH", 0, 1, len(sizes)) + b"".join(entries + payloads))
