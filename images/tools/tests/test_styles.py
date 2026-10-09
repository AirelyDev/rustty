import unittest

import numpy as np

from iconlib import raster, styles


class StylesTest(unittest.TestCase):
    def setUp(self):
        self.f = raster.ferris_at(459)
        self.m = raster.classify(self.f)

    def test_contract_and_determinism(self):
        for name, fn in styles.STYLES.items():
            with self.subTest(name=name):
                a = fn(self.f, self.m, np.random.default_rng(3))
                b = fn(self.f, self.m, np.random.default_rng(3))
                self.assertEqual(a.size, (459 + 2 * styles.PAD, self.f.height + 2 * styles.PAD))
                self.assertEqual(a.mode, "RGBA")
                self.assertEqual(a.tobytes(), b.tobytes())
                alpha = np.asarray(a)[..., 3]
                self.assertGreater((alpha > 32).sum(), 0.2 * self.m.body.sum())

    def test_styles_are_not_stock_orange(self):
        for name, fn in styles.STYLES.items():
            with self.subTest(name=name):
                a = np.asarray(fn(self.f, self.m, np.random.default_rng(3))).astype(int)
                solid = a[..., 3] > 200
                orange = (abs(a[..., 0] - 247) < 12) & (abs(a[..., 1] - 76) < 12) & (a[..., 2] < 12)
                self.assertLess((orange & solid).sum(), 0.05 * solid.sum())


class CoverageTest(unittest.TestCase):
    def test_all_alternates_have_styles(self):
        from iconlib import paths
        self.assertEqual(set(styles.STYLES), set(paths.ALT_NAMES))


if __name__ == "__main__":
    unittest.main()
