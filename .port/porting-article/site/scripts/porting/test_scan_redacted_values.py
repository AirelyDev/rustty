"""Regression tests for scan_redacted_values.count_text (synthetic values only)."""

import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import scan_redacted_values as srv  # noqa: E402

# DATASET is a dataset path (person values count there); OTHER is any other JSON file.
DATASET = ".port/porting-article/research/data/prompts-redacted-2026-10-11.json"
OTHER = "notes/other.json"


def vals(v: str) -> list[str]:
    """A row's value forms, as redact_rows builds them."""
    v = v.lower()
    return sorted({v, json.dumps(v)[1:-1]})


def blob(text: str) -> str:
    """A one-line JSON file holding text in a row, as the prompt log stores it."""
    return json.dumps({"rows": [{"text": text}]})


class CountTextTest(unittest.TestCase):
    """Whole-word hits on decoded JSON, substring safety net, person only in datasets."""

    def test_person_after_newline_escape(self):
        self.assertEqual(srv.count_text(DATASET, blob("first line\nsomeuser wrote"), "person", vals("someuser")), (1, 0))

    def test_person_after_tab_escape(self):
        self.assertEqual(srv.count_text(DATASET, blob("cd\tsomeuser"), "person", vals("someuser")), (1, 0))

    def test_person_after_accented_letter(self):
        # json.dumps escapes é as é, so the raw text has "9someuser".
        self.assertEqual(srv.count_text(DATASET, blob("ésomeuser"), "person", vals("someuser")), (1, 0))

    def test_person_inside_user_at_host(self):
        self.assertEqual(srv.count_text(DATASET, blob("ssh someuser@devbox:~"), "person", vals("someuser")), (1, 0))

    def test_user_host_after_newline_escape(self):
        hits, sub = srv.count_text(OTHER, blob("log\nsomeuser@devbox:~/x$ ls"), "user-host", vals("someuser@devbox:~"))
        self.assertEqual(hits, 1)
        self.assertGreaterEqual(sub, 1)

    def test_person_not_counted_outside_datasets(self):
        self.assertEqual(srv.count_text("docs/notes.md", "/Users/someuser/x", "person", vals("someuser")), (0, 0))

    def test_person_not_counted_inside_longer_word(self):
        self.assertEqual(srv.count_text(DATASET, blob("someusers and xsomeuser"), "person", vals("someuser")), (0, 0))

    def test_substring_net_catches_boundary_miss(self):
        # A secret glued to a word: no whole-word hit, but the substring pass sees it.
        hits, sub = srv.count_text("a.md", "prefixhunter2-example", "secret", vals("hunter2-example"))
        self.assertEqual((hits, sub), (0, 1))

    def test_plain_text_word_hit(self):
        self.assertEqual(srv.count_text("a.md", "mail someone@example.org now", "email", vals("someone@example.org")), (1, 1))


if __name__ == "__main__":
    unittest.main()
