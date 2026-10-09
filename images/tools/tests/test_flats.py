import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import numpy as np
from PIL import Image

from iconlib import flats, paths, tools

BUILD_ICONS = Path(__file__).resolve().parents[1] / "build_icons.py"


class FramingTest(unittest.TestCase):
    def _square(self, box):
        img = Image.new("RGBA", (1024, 1024), (0, 0, 0, 0))
        img.paste((200, 200, 200, 255), box)
        return img

    def test_reference_passes_against_itself(self):
        ref = flats.reference_1024()
        r = flats.framing_check(ref, ref)
        self.assertTrue(r.bbox_ok and r.ok)
        self.assertEqual(r.iou, 1.0)

    def test_shifted_frame_fails(self):
        ref = self._square(flats.REFERENCE_BBOX_1024)
        x0, y0, x1, y1 = flats.REFERENCE_BBOX_1024
        moved = self._square((x0 + 6, y0, x1 + 6, y1))
        r = flats.framing_check(moved, ref)
        self.assertFalse(r.bbox_ok)
        self.assertFalse(r.ok)

    def test_fit_to_frame_lands_on_reference_bbox(self):
        render = self._square((10, 10, 1014, 1014)).resize((2048, 2048))
        master = flats.fit_to_frame(render)
        small = flats.downscale(master, 1024)
        self.assertTrue(flats.framing_check(small, self._square(flats.REFERENCE_BBOX_1024)).bbox_ok)


class RenderTest(unittest.TestCase):
    def test_render_is_srgb_untagged(self):
        img = flats.render_icon(256)
        self.assertEqual(img.mode, "RGBA")
        self.assertNotIn("icc_profile", img.info)
        with tempfile.TemporaryDirectory() as tmp:
            raw_png = Path(tmp) / "raw.png"
            tools.run([tools.tool_path("ictool"), paths.ICON_DIR, "--export-image", "--output-file", raw_png,
                       "--platform", "macOS", "--rendition", "Default", "--width", "256", "--height", "256",
                       "--scale", "1"])
            raw = np.asarray(Image.open(raw_png).convert("RGBA")).astype(int)
        # P3 -> sRGB must actually move colours (saturated blue screen + orange Ferris).
        self.assertGreater(np.abs(np.asarray(img).astype(int) - raw)[..., :3].max(), 3)


class IcoTest(unittest.TestCase):
    def test_ico_entries(self):
        out = subprocess.run([tools.tool_path("icotool"), "-l", str(paths.WIN_ICO)],
                             capture_output=True, text=True, check=True).stdout
        widths = sorted(int(l.split("--width=")[1].split()[0]) for l in out.splitlines() if "--width=" in l)
        self.assertEqual(widths, [16, 32, 48, 256])
        self.assertTrue(all("--bit-depth=32" in l for l in out.splitlines() if l.strip()))


class MissingToolTest(unittest.TestCase):
    def test_flats_fails_before_writing(self):
        watched = [paths.GNOME_DIR / "1024.png", paths.WIN_ICO, paths.FAVICON]
        before = [p.stat().st_mtime_ns for p in watched]
        env = dict(os.environ, ICTOOL="/nonexistent/ictool")
        r = subprocess.run([sys.executable, "-I", str(BUILD_ICONS), "flats"], env=env,
                           capture_output=True, text=True)
        self.assertEqual(r.returncode, 2)
        self.assertIn("ictool", r.stderr)
        self.assertEqual(before, [p.stat().st_mtime_ns for p in watched])


class OutputsTest(unittest.TestCase):
    def test_sizes_and_mirror(self):
        for size, name in paths.APPICON_FILES.items():
            self.assertEqual(Image.open(paths.APPICON_IMAGESET / name).size, (size, size))
        for size in paths.GNOME_SIZES:
            a = (paths.GNOME_DIR / f"{size}.png").read_bytes()
            self.assertEqual(a, (paths.APPRT_GNOME_DIR / f"{size}.png").read_bytes())
            self.assertEqual(Image.open(paths.GNOME_DIR / f"{size}.png").size, (size, size))
        for name, size in paths.ICONS_MAP.items():
            self.assertEqual(Image.open(paths.ICONS_DIR / name).size, (size, size))
        self.assertEqual(Image.open(paths.FAVICON).size, (32, 32))


if __name__ == "__main__":
    unittest.main()
