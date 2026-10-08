import contextlib
import io
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

    def test_runtime_error_is_reported_not_raised(self):
        import build_icons
        def boom(args):
            raise RuntimeError("boom")
        build_icons.COMMANDS["_boom"] = boom
        build_icons.COMMAND_TOOLS["_boom"] = []
        orig = build_icons.build_parser
        build_icons.build_parser = lambda: type("P", (), {"parse_args": lambda self, argv: type("A", (), {"cmd": "_boom"})()})()
        err = io.StringIO()
        try:
            with contextlib.redirect_stderr(err):
                rc = build_icons.main([])
        finally:
            build_icons.build_parser = orig
            del build_icons.COMMANDS["_boom"], build_icons.COMMAND_TOOLS["_boom"]
        self.assertEqual(rc, 1)
        self.assertIn("error: boom", err.getvalue())

    def test_review_requirements_depend_on_target(self):
        import build_icons
        self.assertEqual(build_icons.COMMAND_TOOLS["review"], ["resvg"])

    def test_git_blob_reads_pre_rebrand(self):
        data = tools.git_blob(paths.PRE_REBRAND_REV, "images/gnome/16.png")
        self.assertEqual(data[:8], b"\x89PNG\r\n\x1a\n")


if __name__ == "__main__":
    unittest.main()
