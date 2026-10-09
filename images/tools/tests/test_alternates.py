import unittest

import numpy as np

from iconlib import alternates, paths, raster, styles


class ComposeTest(unittest.TestCase):
    def test_compose_keeps_plate_outside_ferris(self):
        for name in styles.STYLES:
            if not paths.plate_path(name).is_file():
                continue
            with self.subTest(name=name):
                from PIL import Image
                plate = np.asarray(Image.open(paths.plate_path(name)).convert("RGBA"))
                out = np.asarray(alternates.compose(name))
                x, y, w, h = alternates.placement(name)
                keep = np.ones(plate.shape[:2], bool)
                keep[max(0, y - styles.PAD):y + h + styles.PAD, max(0, x - styles.PAD):x + w + styles.PAD] = False
                self.assertTrue(np.array_equal(plate[keep], out[keep]))
                self.assertFalse(np.array_equal(plate[~keep], out[~keep]))

    def test_microchip_offset_keeps_trace_visible(self):
        from PIL import Image
        self.assertEqual(alternates.placement("Microchip")[:2],
                         tuple(a + b for a, b in zip(raster.ferris_box(alternates.screen_bbox("Microchip"))[:2],
                                                     alternates.PLACEMENT_OFFSET["Microchip"])))
        plate = np.asarray(Image.open(paths.plate_path("Microchip")).convert("RGBA"))
        out = np.asarray(alternates.compose("Microchip"))
        # Orange bar rows 524-588 (a few px inside its 520-591 extent): identical to the plate.
        self.assertTrue(np.array_equal(plate[524:589, 180:845], out[524:589, 180:845]))

    def test_compose_is_deterministic(self):
        name = next(n for n in styles.STYLES if paths.plate_path(n).is_file())
        self.assertEqual(alternates.compose(name).tobytes(), alternates.compose(name).tobytes())


if __name__ == "__main__":
    unittest.main()
