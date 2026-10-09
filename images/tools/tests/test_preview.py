import unittest

import numpy as np

from iconlib import mascot, paths, preview


class BlendTest(unittest.TestCase):
    def test_color_blend_keeps_backdrop_luminance(self):
        rng = np.random.default_rng(1)
        cb, cs = rng.random((64, 3)), rng.random((64, 3))
        lum = lambda c: 0.3 * c[..., 0] + 0.59 * c[..., 1] + 0.11 * c[..., 2]
        np.testing.assert_allclose(lum(preview.blend_color(cb, cs)), lum(cb), atol=1e-9)

    def test_overlay_identity_on_mid_gray(self):
        cs = np.array([[0.2, 0.7, 0.9]])
        np.testing.assert_allclose(preview.blend_overlay(np.full((1, 3), 0.5), cs), cs)

    def test_opaque_normal_composite_is_source(self):
        b = np.array([[[0.1, 0.2, 0.3, 1.0]]])
        s = np.array([[[0.9, 0.8, 0.7, 1.0]]])
        np.testing.assert_allclose(preview.composite(b, s, preview.blend_normal), s)


class CustomStyleTest(unittest.TestCase):
    def test_orange_tint_makes_an_orange_crab(self):
        img = preview.custom_style("aluminum", ["#0018B4", "#2A4BFF"], "#F74C00")
        from iconlib import raster
        x, y, w, h = raster.ferris_box(mascot.CUSTOM_SCREEN_BBOX)
        r, g, b, _ = img.getpixel((x + w // 2, y + h // 4))
        self.assertGreater(r, g + 40)
        self.assertGreater(r, b + 40)


if __name__ == "__main__":
    unittest.main()
