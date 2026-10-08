import unittest

from iconlib import paths, verify


class VerifyTest(unittest.TestCase):
    def test_outputs_cover_every_derived_asset(self):
        outs = {paths.rel(p) for p in verify.outputs()}
        self.assertEqual(len(outs), 43)  # 1+1 mascots, 8 alts, 3 AppIcon, 8+8 gnome, 12 icons, ico, favicon
        self.assertIn("dist/windows/rustty.ico", outs)
        self.assertIn("crates/rustty-apprt/assets/images/gnome/2048.png", outs)

    def test_no_ghost_left(self):
        self.assertEqual(verify.no_ghost_left(), [])


if __name__ == "__main__":
    unittest.main()
