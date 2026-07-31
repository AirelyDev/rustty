"""Contract tests for the committed-tree universe builder."""

from __future__ import annotations

import hashlib
import contextlib
import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


SCRIPTS = Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))

import build_universe
from model import validate_file


class BuildUniverseTests(unittest.TestCase):
    """Each test uses committed fixture data, never a mocked Git tree."""

    def setUp(self) -> None:
        self.tempdir = tempfile.TemporaryDirectory()
        self.root = Path(self.tempdir.name)
        self.rustty = self.make_repo("rustty")
        self.ghostty = self.make_repo("ghostty")

    def tearDown(self) -> None:
        self.tempdir.cleanup()

    def run_git(self, repo: Path, *args: str) -> str:
        return subprocess.run(
            ["git", "-C", str(repo), *args],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        ).stdout.strip()

    def make_repo(self, name: str) -> Path:
        repo = self.root / name
        repo.mkdir()
        self.run_git(repo, "init", "--quiet")
        self.run_git(repo, "config", "user.name", "Line Audit Test")
        self.run_git(repo, "config", "user.email", "line-audit@example.invalid")
        return repo

    def commit_files(self, repo: Path, files: dict[str, bytes]) -> str:
        for relative_path, data in files.items():
            path = repo / relative_path
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)
        self.run_git(repo, "add", ".")
        self.run_git(repo, "commit", "--quiet", "-m", "fixture")
        return self.run_git(repo, "rev-parse", "HEAD")

    def read_jsonl(self, output: Path) -> list[dict]:
        return [json.loads(line) for line in output.read_text(encoding="utf-8").splitlines()]

    def test_reads_committed_blobs_in_deterministic_byte_path_order(self) -> None:
        self.commit_files(self.rustty, {"z.txt": b"z\n", "a/last.txt": b"last\n", "a/first.txt": b"first\n"})

        first = build_universe.read_committed_blobs(self.rustty, "HEAD")
        second = build_universe.read_committed_blobs(self.rustty, "HEAD")

        self.assertEqual([path for path, _ in first], ["a/first.txt", "a/last.txt", "z.txt"])
        self.assertEqual(first, second)

    def test_build_records_classifies_content_and_counts_physical_lines(self) -> None:
        revision = self.commit_files(
            self.rustty,
            {
                "text.txt": b"one\r\ntwo\nthree\rfour",
                "nul.bin": b"before\0after",
                "invalid.bin": b"\xffnot-utf8",
                "empty": b"",
            },
        )

        records = {record["path"]: record for record in build_universe.build_records(self.rustty, "rustty", revision)}

        self.assertEqual(records["text.txt"]["content_kind"], "text")
        self.assertEqual(records["text.txt"]["line_count"], 3)
        self.assertEqual(records["empty"]["content_kind"], "empty")
        self.assertEqual(records["empty"]["line_count"], 0)
        for path in ("nul.bin", "invalid.bin"):
            self.assertEqual(records[path]["content_kind"], "binary")
            self.assertEqual(records[path]["line_count"], 0)
            self.assertEqual(records[path]["scope_class"], "binary_asset")
            self.assertEqual(records[path]["classification_reason"], "binary content has no physical text lines")
            self.assertEqual(records[path]["classification_evidence"], "content classification: NUL prefix or invalid UTF-8")

    def test_records_preserve_exact_blob_lengths_and_hashes(self) -> None:
        data = b"line\r\n\xff"
        revision = self.commit_files(self.rustty, {"bytes.dat": data})

        record = build_universe.build_records(self.rustty, "rustty", revision)[0]

        self.assertEqual(record["byte_length"], 7)
        self.assertEqual(record["sha256"], hashlib.sha256(data).hexdigest())
        self.assertEqual(record["revision"], revision)

    def test_build_records_pins_full_committed_head_and_rejects_mismatch(self) -> None:
        revision = self.commit_files(self.rustty, {"one.txt": b"one\n"})

        records = build_universe.build_records(self.rustty, "rustty", revision)

        self.assertEqual({record["revision"] for record in records}, {revision})
        with self.assertRaisesRegex(ValueError, "expected revision"):
            build_universe.build_records(self.rustty, "rustty", "0" * 40)

    def test_committed_symlink_uses_link_blob_not_worktree_target(self) -> None:
        self.commit_files(self.rustty, {"target.txt": b"committed target\n"})
        os.symlink("target.txt", self.rustty / "link")
        self.run_git(self.rustty, "add", "link")
        self.run_git(self.rustty, "commit", "--quiet", "-m", "link")
        (self.rustty / "target.txt").write_bytes(b"changed working tree target\n")

        blobs = dict(build_universe.read_committed_blobs(self.rustty, "HEAD"))
        records = {record["path"]: record for record in build_universe.build_records(self.rustty, "rustty")}

        self.assertEqual(blobs["link"], b"target.txt")
        self.assertEqual(records["link"]["sha256"], hashlib.sha256(b"target.txt").hexdigest())

    def test_untracked_files_do_not_enter_records(self) -> None:
        self.commit_files(self.rustty, {"tracked.txt": b"tracked\n"})
        (self.rustty / "untracked.txt").write_bytes(b"untracked\n")

        paths = [record["path"] for record in build_universe.build_records(self.rustty, "rustty")]

        self.assertEqual(paths, ["tracked.txt"])

    def test_jsonl_is_valid_deterministic_and_orders_ghostty_before_rustty(self) -> None:
        rustty_revision = self.commit_files(self.rustty, {"b.txt": b"rustty\n"})
        ghostty_revision = self.commit_files(self.ghostty, {"a.txt": b"ghostty\n"})
        output_one = self.root / "one.jsonl"
        output_two = self.root / "two.jsonl"
        records = (
            build_universe.build_records(self.ghostty, "ghostty", ghostty_revision)
            + build_universe.build_records(self.rustty, "rustty", rustty_revision)
        )

        build_universe.write_jsonl(records, output_one)
        build_universe.write_jsonl(records, output_two)
        written = self.read_jsonl(output_one)

        self.assertEqual([record["repo"] for record in written], ["ghostty", "rustty"])
        self.assertEqual(output_one.read_bytes(), output_two.read_bytes())
        self.assertTrue(output_one.read_bytes().endswith(b"\n"))
        self.assertTrue(all(not validate_file(record) for record in written))

    def test_cli_mismatch_and_validation_failure_leave_existing_output_unchanged(self) -> None:
        rustty_revision = self.commit_files(self.rustty, {"r.txt": b"r\n"})
        ghostty_revision = self.commit_files(self.ghostty, {"g.txt": b"g\n"})
        output = self.root / "universe.jsonl"
        output.write_bytes(b"previous universe\n")

        with contextlib.redirect_stderr(io.StringIO()):
            with self.assertRaises(SystemExit):
                build_universe.main([
                    "--rustty", str(self.rustty), "--ghostty", str(self.ghostty), "--output", str(output),
                    "--expect-rustty-revision", "0" * 40,
                    "--expect-ghostty-revision", ghostty_revision,
                ])
        self.assertEqual(output.read_bytes(), b"previous universe\n")

        invalid = {"repo": "rustty"}
        with self.assertRaisesRegex(ValueError, "invalid file record"):
            build_universe.write_jsonl([invalid], output)
        self.assertEqual(output.read_bytes(), b"previous universe\n")
        self.assertEqual(rustty_revision, self.run_git(self.rustty, "rev-parse", "HEAD"))

    def test_replace_failure_preserves_destination_and_removes_sibling_temporary_file(self) -> None:
        revision = self.commit_files(self.rustty, {"record.txt": b"valid record\n"})
        records = build_universe.build_records(self.rustty, "rustty", revision)
        output = self.root / "universe.jsonl"
        output.write_bytes(b"previous complete universe\n")

        with patch.object(build_universe.os, "replace", side_effect=OSError("injected replace failure")):
            with self.assertRaisesRegex(OSError, "injected replace failure"):
                build_universe.write_jsonl(records, output)

        self.assertEqual(output.read_bytes(), b"previous complete universe\n")
        self.assertEqual(list(self.root.glob(".universe.jsonl.*.tmp")), [])


if __name__ == "__main__":
    unittest.main()
