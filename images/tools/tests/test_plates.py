import io
import unittest

import numpy as np
from PIL import Image

from iconlib import paths, plates, tools


class PlatesTest(unittest.TestCase):
    def test_original_comes_from_git_history(self):
        blob = tools.git_blob(paths.PRE_REBRAND_REV, paths.rel(paths.alt_output("Retro")))
        expect = np.asarray(Image.open(io.BytesIO(blob)).convert("RGBA"))
        self.assertTrue(np.array_equal(np.asarray(plates.original("Retro")), expect))

    def test_mirror_pass_removes_blob_and_keeps_outside(self):
        img = Image.new("RGBA", (1024, 1024), (40, 40, 40, 255))
        img.paste((0, 255, 0, 255), (250, 250, 400, 400))
        out = plates.apply_pass(img, ("mirror", (200, 200, 450, 450)))
        a = np.asarray(out)
        self.assertTrue((a[300, 300, :3] == 40).all())
        before, after = np.asarray(img), a
        outside = np.ones((1024, 1024), bool)
        outside[200:450, 200:450] = False
        self.assertTrue(np.array_equal(before[outside], after[outside]))

    def test_shift_pass_copies_offset_region(self):
        img = Image.new("RGBA", (1024, 1024), (0, 0, 0, 255))
        img.paste((9, 9, 9, 255), (600, 200, 900, 500))
        out = np.asarray(plates.apply_pass(img, ("shift", (200, 200, 500, 500), 400, 0)))
        self.assertTrue((out[350, 350, :3] == 9).all())

    def test_every_alternate_has_passes(self):
        self.assertEqual(set(plates.PASSES), set(paths.ALT_NAMES))


if __name__ == "__main__":
    unittest.main()
