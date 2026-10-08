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


if __name__ == "__main__":
    unittest.main()
