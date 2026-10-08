import json
import shutil
import tempfile
import unittest
from pathlib import Path

from iconlib import official, paths, tools


class IconJsonTest(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.path = Path(self.tmp.name) / "icon.json"
        self.path.write_bytes(tools.git_blob(paths.PRE_REBRAND_REV, "images/Rustty.icon/icon.json"))
        self.before = json.loads(self.path.read_text())

    def tearDown(self):
        self.tmp.cleanup()

    def test_only_mascot_fields_change(self):
        self.assertTrue(official.update_icon_json(self.path))
        after = json.loads(self.path.read_text())
        mascot_layers = [l for g in after["groups"] for l in g["layers"] if l.get("image-name") == "Rustty.png"]
        self.assertEqual(len(mascot_layers), 2)
        for l in mascot_layers:
            self.assertEqual(l["position"], {"scale": 1, "translation-in-points": [0, -10]})
        blur = [l for l in mascot_layers if l["name"] == "RusttyBlur"][0]
        self.assertEqual(blur["fill"], {"solid": official.ORANGE_FILL})
        for g_before, g_after in zip(self.before["groups"], after["groups"]):
            for l_before, l_after in zip(g_before["layers"], g_after["layers"]):
                if l_before.get("image-name") != "Rustty.png":
                    self.assertEqual(l_before, l_after)
        self.assertEqual({k: v for k, v in self.before.items() if k != "groups"},
                         {k: v for k, v in after.items() if k != "groups"})

    def test_idempotent_and_format_preserving(self):
        official.update_icon_json(self.path)
        once = self.path.read_bytes()
        self.assertFalse(official.update_icon_json(self.path))
        self.assertEqual(self.path.read_bytes(), once)
        self.assertFalse(once.endswith(b"\n"))
        self.assertIn(b'"name" : "RusttyBlur"', once)


if __name__ == "__main__":
    unittest.main()
