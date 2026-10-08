import tempfile
import unittest
from pathlib import Path

import numpy as np
from PIL import Image, ImageCms

from iconlib import paths, raster


def png_chunk_types(data: bytes) -> list[bytes]:
    assert data[:8] == b"\x89PNG\r\n\x1a\n"
    i, out = 8, []
    while i < len(data):
        n = int.from_bytes(data[i:i + 4], "big")
        out.append(data[i + 4:i + 8])
        i += 12 + n
    return out


class SavePngTest(unittest.TestCase):
    def test_deterministic_and_chunk_free(self):
        img = Image.new("RGBA", (8, 8), (247, 76, 0, 255))
        img.info["dpi"] = (144, 144)
        img.info["icc_profile"] = ImageCms.ImageCmsProfile(ImageCms.createProfile("sRGB")).tobytes()
        with tempfile.TemporaryDirectory() as tmp:
            a, b = Path(tmp) / "a.png", Path(tmp) / "b.png"
            raster.save_png(img, a)
            raster.save_png(img, b)
            self.assertEqual(a.read_bytes(), b.read_bytes())
            self.assertEqual(set(png_chunk_types(a.read_bytes())), {b"IHDR", b"IDAT", b"IEND"})


class FerrisTest(unittest.TestCase):
    def test_ferris_at_exact_width(self):
        f = raster.ferris_at(459)
        self.assertEqual(f.width, 459)
        self.assertEqual(f.height, round(459 / raster.ferris_aspect()))
        self.assertEqual(f.mode, "RGBA")

    def test_classify_finds_disjoint_regions(self):
        m = raster.classify(raster.ferris_at(1200))
        for name in ("body", "shade", "dark", "white"):
            self.assertGreater(getattr(m, name).sum(), 100, name)
        stack = np.stack([m.body, m.shade, m.dark, m.white]).astype(int)
        self.assertLessEqual(stack.sum(0).max(), 1)
        self.assertGreater(m.body.sum(), m.shade.sum())


class PlacementTest(unittest.TestCase):
    def test_ferris_box_rule(self):
        # screen 675 wide: w = round(0.68*675) = 459, h = round(459/2) = 230,
        # centre (511.5, 511.5 - 0.012*675 = 503.4)
        self.assertEqual(raster.ferris_box((174, 174, 849, 849), aspect=2.0), (282, 388, 459, 230))

    def test_opaque_bbox_and_iou(self):
        a = Image.new("RGBA", (10, 10), (0, 0, 0, 0))
        a.paste((255, 0, 0, 255), (2, 3, 6, 8))
        self.assertEqual(raster.opaque_bbox(a), (2, 3, 6, 8))
        m = np.zeros((10, 10), bool)
        m[3:8, 2:6] = True
        self.assertEqual(raster.iou(m, m), 1.0)
        self.assertEqual(raster.iou(m, ~m), 0.0)


class SrgbTest(unittest.TestCase):
    def test_untagged_passthrough(self):
        img = Image.new("RGBA", (4, 4), (10, 20, 30, 200))
        out = raster.to_srgb(img)
        self.assertEqual(out.getpixel((0, 0)), (10, 20, 30, 200))
        self.assertNotIn("icc_profile", out.info)


class PathsTest(unittest.TestCase):
    def test_repo_root_and_inputs(self):
        self.assertTrue((paths.REPO / "Cargo.toml").is_file())
        self.assertTrue(paths.FERRIS_SVG.is_file())
        self.assertTrue(paths.ICON_JSON.is_file())
        self.assertEqual(len(paths.ALT_NAMES), 8)
        self.assertEqual(paths.rel(paths.ICON_JSON), "images/Rustty.icon/icon.json")


if __name__ == "__main__":
    unittest.main()
