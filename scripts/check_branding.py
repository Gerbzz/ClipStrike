"""Approved logo asset and native-binding contracts. Requires Pillow."""
from pathlib import Path
from hashlib import sha256
from io import BytesIO
import json
import struct
import unittest
from PIL import Image
ROOT = Path(__file__).resolve().parents[1]
APP = "ClipStrike"
PACKAGE = "filmcraft"
APPROVED = "2180863c3e8962940d5335e30c8cc64c4617f1d27b9ee43b7aded3a98f29cb15"
class BrandingContract(unittest.TestCase):
    def test_exact_approved_source_and_provenance(self):
        path = ROOT / f"assets/branding/{APP}.png"
        self.assertEqual(sha256(path.read_bytes()).hexdigest(), APPROVED)
        self.assertEqual(Image.open(BytesIO(path.read_bytes())).size, (512, 512))
        metadata = json.loads((path.parent / "provenance.json").read_text())
        self.assertEqual(metadata["sha256"], APPROVED)
    def test_runtime_derivative_and_binding(self):
        source = Image.open(ROOT / f"assets/branding/{APP}.png").convert("RGBA")
        self.assertTrue((ROOT / f"assets/branding/{APP}-128.rgba").exists(), "runtime derivative missing")
        self.assertEqual((ROOT / f"assets/branding/{APP}-128.rgba").read_bytes(), source.resize((128, 128), Image.Resampling.LANCZOS).tobytes())
        code = (ROOT / f"apps/{PACKAGE}/src/main.rs").read_text()
        self.assertIn(".with_icon(app_icon())", code)
        self.assertIn(f'include_bytes!("../../../assets/branding/{APP}-128.rgba")', code)
        self.assertIn("width: 128, height: 128", code)
    def test_readme_uses_supplied_icon_without_changing_runtime_source(self):
        readme = (ROOT / "README.md").read_text(encoding="utf-8")
        self.assertIn('src="docs/assets/FilmStrike-readme.webp"', readme)
        self.assertEqual(
            sha256((ROOT / "docs/assets/FilmStrike-readme.webp").read_bytes()).hexdigest(),
            "1d781de3561bbbc1e7c06bd78a35094d741e7992482723f42cf29c9c1ca90e40",
        )
        self.assertEqual(sha256((ROOT / f"assets/branding/{APP}.png").read_bytes()).hexdigest(), APPROVED)
    def test_windows_resource(self):
        if APP == "AnimateStrike":
            self.skipTest("No Windows resource/package build in this repository; native window binding is tested")
        code = (ROOT / f"apps/{PACKAGE}/build.rs").read_text()
        self.assertIn(f'set_icon("../../assets/branding/{APP}.ico")', code)
        self.assertIn(f'cargo:rerun-if-changed=../../assets/branding/{APP}.ico', code)
        source = Image.open(ROOT / f"assets/branding/{APP}.png").convert("RGBA")
        data = (ROOT / f"assets/branding/{APP}.ico").read_bytes()
        sizes = (16, 24, 32, 48, 64, 128, 256)
        self.assertEqual(struct.unpack_from("<HHH", data), (0, 1, len(sizes)))
        for i, size in enumerate(sizes):
            w,h,_,_,_,_,length,offset = struct.unpack_from("<BBBBHHII", data, 6+16*i)
            self.assertEqual((w or 256,h or 256),(size,size))
            frame = Image.open(BytesIO(data[offset:offset+length])).convert("RGBA")
            self.assertEqual(frame.tobytes(),source.resize((size,size),Image.Resampling.LANCZOS).tobytes())
if __name__ == "__main__":
    unittest.main()
