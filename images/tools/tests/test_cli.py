import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from iconlib import paths, tools

BUILD_ICONS = Path(__file__).resolve().parents[1] / "build_icons.py"


class CliTest(unittest.TestCase):
    def test_paths_command_from_foreign_cwd(self):
        with tempfile.TemporaryDirectory() as tmp:
            r = subprocess.run([sys.executable, "-I", str(BUILD_ICONS), "paths"],
                               cwd=tmp, capture_output=True, text=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertIn(str(paths.REPO), r.stdout)

    def test_missing_tool_message(self):
        old = os.environ.get("ICTOOL")
        os.environ["ICTOOL"] = "/nonexistent/ictool"
        try:
            with self.assertRaises(tools.ToolMissing) as cm:
                tools.tool_path("ictool")
            self.assertIn("ictool", str(cm.exception))
        finally:
            if old is None:
                del os.environ["ICTOOL"]
            else:
                os.environ["ICTOOL"] = old

    def test_git_blob_reads_pre_rebrand(self):
        data = tools.git_blob(paths.PRE_REBRAND_REV, "images/gnome/16.png")
        self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")


if __name__ == "__main__":
    unittest.main()
