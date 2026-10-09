import unittest

from iconlib import mascot, raster


class IconMascotTest(unittest.TestCase):
    def test_size_follows_d2(self):
        _, _, w, h = raster.ferris_box(mascot.ICON_SCREEN_BBOX)
        self.assertEqual(w, 561)  # round(0.68 * 825)
        img = mascot.icon_mascot()
        self.assertEqual(img.size, (w + 2 * mascot.ICON_PAD, h + 2 * mascot.ICON_PAD))
        self.assertEqual(img.mode, "RGBA")

    def test_ferris_centred_in_layer(self):
        img = mascot.icon_mascot()
        x0, y0, x1, y1 = raster.opaque_bbox(img, thr=250)
        self.assertLessEqual(abs((x0 + x1) / 2 - img.width / 2), 2)
        self.assertLessEqual(abs((y0 + y1) / 2 - img.height / 2), 2)

    def test_stock_colours_survive(self):
        img = mascot.icon_mascot()
        px = img.getpixel((img.width // 2, mascot.ICON_PAD + img.height // 5))
        self.assertTrue(abs(px[0] - 247) <= 3 and abs(px[1] - 76) <= 3 and px[2] <= 3, px)


import numpy as np


class CustomMascotTest(unittest.TestCase):
    def test_mode_size_and_neutral(self):
        img = mascot.custom_mascot()
        self.assertEqual(img.mode, "LA")   # same as the pre-rebrand ghosty.png
        self.assertEqual(img.size, (1024, 1024))

    def test_placement_follows_d2(self):
        img = mascot.custom_mascot().convert("RGBA")
        x, y, w, h = raster.ferris_box(mascot.CUSTOM_SCREEN_BBOX)
        bx0, by0, bx1, by1 = raster.opaque_bbox(img, thr=250)
        self.assertLessEqual(abs((bx0 + bx1) / 2 - (x + w / 2)), 3)
        self.assertLessEqual(abs((by0 + by1) / 2 - (y + h / 2)), 3)
        self.assertLessEqual(abs((bx1 - bx0) - w), 4)

    def test_body_luminance(self):
        img = mascot.custom_mascot()
        x, y, w, h = raster.ferris_box(mascot.CUSTOM_SCREEN_BBOX)
        m = raster.classify(raster.ferris_at(w))
        lum = np.asarray(img)[y:y + h, x:x + w, 0].astype(int)
        self.assertTrue(125 <= lum[m.body].mean() <= 145)
        self.assertLess(lum[m.dark].mean(), 60)


if __name__ == "__main__":
    unittest.main()
