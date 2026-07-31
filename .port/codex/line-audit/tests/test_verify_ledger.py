"""Fail-closed integration tests for the exhaustive line-audit verifier."""

from __future__ import annotations

import copy
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


AUDIT_ROOT = Path(__file__).resolve().parents[1]
SCRIPTS = AUDIT_ROOT / "scripts"
sys.path.insert(0, str(SCRIPTS))

import build_universe
from model import range_sha256, split_physical_lines
import verify_ledger


class VerifyLedgerTests(unittest.TestCase):
    """Exercise real Git trees and serialized records without repository mocks."""

    def setUp(self) -> None:
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.rustty = self._make_repo("rustty")
        self.ghostty = self._make_repo("ghostty")
        self.rustty_bytes = {
            "r.txt": b"rust one\nrust two\nrust three\n",
            "excluded.txt": b"mac only\n",
        }
        self.ghostty_bytes = {"g.txt": b"ghost one\nghost two\nghost three\n"}
        self._commit_files(self.rustty, self.rustty_bytes)
        self._commit_files(self.ghostty, self.ghostty_bytes)
        self.universe_path = self.root / "universe.jsonl"
        self.ledger = self.root / "ledger"
        self.reviews = self.root / "reviews"
        self.findings = self.root / "findings"
        for directory in (self.ledger, self.reviews, self.findings):
            directory.mkdir()
        self._write_clean_fixture()

    def tearDown(self) -> None:
        self.temporary.cleanup()

    def _git(self, repo: Path, *arguments: str) -> str:
        return subprocess.run(
            ["git", "-C", str(repo), *arguments],
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        ).stdout.strip()

    def _make_repo(self, name: str) -> Path:
        repo = self.root / name
        repo.mkdir()
        self._git(repo, "init", "--quiet")
        self._git(repo, "config", "user.name", "Line Audit Test")
        self._git(repo, "config", "user.email", "line-audit@example.invalid")
        return repo

    def _commit_files(self, repo: Path, files: dict[str, bytes]) -> str:
        for relative, data in files.items():
            destination = repo / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_bytes(data)
        self._git(repo, "add", ".")
        self._git(repo, "commit", "--quiet", "-m", "fixture")
        return self._git(repo, "rev-parse", "HEAD")

    @staticmethod
    def _read_jsonl(path: Path) -> list[dict]:
        return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]

    @staticmethod
    def _write_jsonl(path: Path, records: list[dict]) -> None:
        path.write_text(
            "".join(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n" for record in records),
            encoding="utf-8",
        )

    def _reference(self, repo: str, path: str, start: int, end: int) -> dict:
        repository = self.rustty if repo == "rustty" else self.ghostty
        data = self.rustty_bytes[path] if repo == "rustty" else self.ghostty_bytes[path]
        return {
            "repo": repo,
            "revision": self._git(repository, "rev-parse", "HEAD"),
            "path": path,
            "start": start,
            "end": end,
            "range_sha256": range_sha256(split_physical_lines(data), start, end),
        }

    def _primary(
        self,
        range_id: str,
        reference: dict,
        disposition: str,
        counterparts: list[dict],
        *,
        scope_class: str = "semantic_required",
        finding_ids: list[str] | None = None,
    ) -> dict:
        record = {
            "batch_id": "batch-clean",
            "range_id": range_id,
            **reference,
            "scope_class": scope_class,
            "risk_tags": [],
            "counterparts": counterparts,
            "disposition": disposition,
            "contract": "observable line contract",
            "analysis": "compared exact committed bytes and behavior",
            "reachability": "reachable from the fixture entry point",
            "verification": "fixture comparison",
            "observed_result": "evidence recorded",
            "limitations": "none",
            "finding_ids": finding_ids or [],
            "primary_reviewer": "primary-auditor",
            "reviewed_at": "2026-07-31T00:00:00Z",
        }
        if disposition == "parity_gap":
            record["control_evidence"] = "control reproduces the mismatch"
        return record

    def _review(self, primary: dict) -> dict:
        checked = [
            {field: primary[field] for field in ("repo", "revision", "path", "start", "end", "range_sha256")},
            *copy.deepcopy(primary["counterparts"]),
        ]
        return {
            "batch_id": primary["batch_id"],
            "range_id": primary["range_id"],
            "review_revision": 1,
            "secondary_reviewer": "independent-reviewer",
            "reviewed_at": "2026-07-31T01:00:00Z",
            "verdict": "upheld",
            "source_ranges_checked": checked,
            "falsification": "attempted to disprove the recorded comparison",
            "counterevidence": "",
            "corrections": "",
            "supersedes": None,
        }

    def _write_clean_fixture(self) -> None:
        universe = (
            build_universe.build_records(self.ghostty, "ghostty")
            + build_universe.build_records(self.rustty, "rustty")
        )
        for record in universe:
            if record["repo"] == "rustty" and record["path"] == "excluded.txt":
                record.update(
                    scope_class="pure_macos_out_of_scope",
                    classification_reason="macOS-only fixture file",
                    classification_evidence="committed path classification",
                )
        build_universe.write_jsonl(universe, self.universe_path)

        g = [self._reference("ghostty", "g.txt", line, line) for line in (1, 2, 3)]
        r = [self._reference("rustty", "r.txt", line, line) for line in (1, 2, 3)]
        excluded = self._reference("rustty", "excluded.txt", 1, 1)
        primaries = [
            self._primary("g-first", g[0], "exact_equivalent", [r[0]]),
            self._primary("g-gap", g[1], "parity_gap", [], finding_ids=["F-LA-001"]),
            self._primary("g-last", g[2], "exact_equivalent", [r[2]]),
            self._primary("r-first", r[0], "exact_equivalent", [g[0]]),
            self._primary("r-middle", r[1], "exact_equivalent", [g[1]]),
            self._primary("r-last", r[2], "exact_equivalent", [g[2]]),
            self._primary(
                "r-excluded",
                excluded,
                "oracle_not_applicable",
                [],
                scope_class="pure_macos_out_of_scope",
            ),
        ]
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
        self._write_jsonl(self.reviews / "batch-clean.jsonl", [self._review(item) for item in primaries])
        (self.findings / "F-LA-001.md").write_text(
            '---\n{"id":"F-LA-001","range_ids":["g-gap"]}\n---\n\nFixture parity gap.\n',
            encoding="utf-8",
        )

    def _run(self) -> verify_ledger.VerificationResult:
        return verify_ledger.verify(
            universe=self.universe_path,
            ledger=self.ledger,
            reviews=self.reviews,
            findings=self.findings,
            rustty=self.rustty,
            ghostty=self.ghostty,
        )

    def _cli_command(self) -> list[str]:
        return [
            sys.executable,
            str(SCRIPTS / "verify_ledger.py"),
            "--universe", str(self.universe_path),
            "--ledger", str(self.ledger),
            "--reviews", str(self.reviews),
            "--findings", str(self.findings),
            "--rustty", str(self.rustty),
            "--ghostty", str(self.ghostty),
        ]

    def _run_cli(self, *extra: str) -> tuple[subprocess.CompletedProcess[str], list[dict]]:
        completed = subprocess.run(
            [*self._cli_command(), *extra],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
        )
        return completed, [json.loads(line) for line in completed.stdout.splitlines()]

    def _assert_code(self, code: str, exit_code: int) -> verify_ledger.VerificationResult:
        result = self._run()
        self.assertEqual(result.exit_code, exit_code, result.diagnostics)
        self.assertIn(code, [item["code"] for item in result.diagnostics])
        return result

    def _primaries(self) -> list[dict]:
        return self._read_jsonl(self.ledger / "batch-clean.jsonl")

    def _reviews(self) -> list[dict]:
        return self._read_jsonl(self.reviews / "batch-clean.jsonl")

    def test_clean_fixture_and_cli_are_complete_with_derived_summary(self) -> None:
        result = self._run()

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.diagnostics, [])
        self.assertEqual(
            result.summary["total"],
            {
                "files": 3,
                "text_lines": 7,
                "covered_lines": 7,
                "uncovered_lines": 0,
                "pending_lines": 0,
                "primary_final_lines": 7,
                "secondary_upheld_lines": 7,
                "invalid_diagnostics": 0,
                "incomplete_diagnostics": 0,
            },
        )
        command = self._cli_command()
        completed, objects = self._run_cli()
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(len(objects), 1)
        self.assertEqual(objects[-1]["status"], "COMPLETE")

        universe = self._read_jsonl(self.universe_path)
        self._write_jsonl(self.universe_path, universe + [copy.deepcopy(universe[0])])
        completed = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        objects = [json.loads(line) for line in completed.stdout.splitlines()]
        self.assertEqual(completed.returncode, 1, completed.stderr)
        self.assertIn("E_UNIVERSE_DUPLICATE", [item.get("code") for item in objects[:-1]])
        self.assertEqual(objects[-1]["status"], "INVALID")

    def test_schema_invalid_types_never_crash_cli_at_any_input_stage(self) -> None:
        def malformed_universe() -> None:
            records = self._read_jsonl(self.universe_path)
            records[0]["path"] = []
            self._write_jsonl(self.universe_path, records)

        def malformed_primary() -> None:
            primaries = self._primaries()
            primaries[0]["path"] = []
            self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)

        def malformed_counterpart() -> None:
            primaries = self._primaries()
            primaries[0]["counterparts"][0]["repo"] = []
            self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)

        def malformed_review() -> None:
            reviews = self._reviews()
            reviews[0]["review_revision"] = []
            self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)

        def malformed_checked_source() -> None:
            reviews = self._reviews()
            reviews[0]["source_ranges_checked"][0]["repo"] = []
            self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)

        def malformed_finding_header() -> None:
            (self.findings / "F-LA-001.md").write_text(
                '---\n{"id":"F-LA-001","range_ids":[[]]}\n---\n', encoding="utf-8"
            )

        cases = (
            ("universe", malformed_universe, "E_UNIVERSE_SCHEMA"),
            ("primary", malformed_primary, "E_LEDGER_SCHEMA"),
            ("counterpart", malformed_counterpart, "E_LEDGER_SCHEMA"),
            ("review", malformed_review, "E_REVIEW_SCHEMA"),
            ("checked-source", malformed_checked_source, "E_REVIEW_SCHEMA"),
            ("finding-header", malformed_finding_header, "E_FINDING_LINK"),
        )
        for name, mutate, expected_code in cases:
            with self.subTest(stage=name):
                self._write_clean_fixture()
                mutate()
                completed, objects = self._run_cli()
                self.assertEqual(completed.returncode, 1, completed.stderr)
                self.assertTrue(objects, completed.stderr)
                self.assertEqual(objects[-1]["type"], "summary")
                self.assertEqual(objects[-1]["status"], "INVALID")
                self.assertIn(expected_code, [item.get("code") for item in objects[:-1]])
                self.assertNotIn("Traceback", completed.stderr)

    def test_diagnostic_summary_is_exact_repository_and_unattributed_partition(self) -> None:
        primaries = self._primaries()
        next(item for item in primaries if item["range_id"] == "g-first").pop("contract")
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)

        result = self._run()
        ledger_schema = [item for item in result.diagnostics if item["code"] == "E_LEDGER_SCHEMA"]

        self.assertTrue(ledger_schema)
        self.assertEqual({item.get("repo") for item in ledger_schema}, {"ghostty"})
        rejected_review = [
            item for item in result.diagnostics
            if item["code"] == "E_REVIEW_UNKNOWN" and item.get("range_id") == "g-first"
        ]
        self.assertEqual({item.get("repo") for item in rejected_review}, {"ghostty"})
        self.assertEqual(result.summary["unattributed"]["invalid_diagnostics"], 0)
        self._assert_diagnostic_partition(result.summary)

        self._write_clean_fixture()
        reviews = self._reviews()
        next(item for item in reviews if item["range_id"] == "g-first").pop("falsification")
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)

        result = self._run()
        review_schema = [item for item in result.diagnostics if item["code"] == "E_REVIEW_SCHEMA"]

        self.assertTrue(review_schema)
        self.assertEqual({item.get("repo") for item in review_schema}, {"ghostty"})
        self._assert_diagnostic_partition(result.summary)

        self._write_clean_fixture()
        primaries = self._primaries()
        conflicting = copy.deepcopy(next(item for item in primaries if item["range_id"] == "g-first"))
        conflicting["range_id"] = "r-first"
        conflicting.pop("contract")
        primaries.append(conflicting)
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
        reviews = self._reviews()
        next(item for item in reviews if item["range_id"] == "r-first").pop("falsification")
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)

        result = self._run()
        ambiguous_review = [
            item for item in result.diagnostics
            if item["code"] == "E_REVIEW_SCHEMA" and item.get("range_id") is None
            and "falsification" in item["message"]
        ]

        self.assertTrue(ambiguous_review)
        self.assertTrue(all("repo" not in item for item in ambiguous_review))
        self.assertGreaterEqual(result.summary["unattributed"]["invalid_diagnostics"], 1)
        self._assert_diagnostic_partition(result.summary)

    def test_safe_validation_preserves_all_independent_schema_diagnostics(self) -> None:
        reviews = self._reviews()
        reviews[0].pop("falsification")
        reviews[0]["source_ranges_checked"][0]["repo"] = []
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)

        completed, objects = self._run_cli()
        messages = [item["message"] for item in objects[:-1] if item.get("code") == "E_REVIEW_SCHEMA"]

        self.assertEqual(completed.returncode, 1, completed.stderr)
        self.assertTrue(any("missing required field: falsification" in message for message in messages))
        self.assertTrue(any("source_ranges_checked[0]" in message and "repo" in message for message in messages))
        self.assertEqual(objects[-1]["status"], "INVALID")

        self._write_clean_fixture()
        primaries = self._primaries()
        primary = next(item for item in primaries if item["range_id"] == "g-first")
        primary.pop("contract")
        primary["risk_tags"] = [{}]
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)

        completed, objects = self._run_cli()
        messages = [item["message"] for item in objects[:-1] if item.get("code") == "E_LEDGER_SCHEMA"]

        self.assertEqual(completed.returncode, 1, completed.stderr)
        self.assertTrue(any("missing required field: contract" in message for message in messages))
        self.assertTrue(any("contract must be nonempty" in message for message in messages))
        self.assertTrue(any("risk tag at index 0" in message for message in messages))
        self.assertEqual(objects[-1]["status"], "INVALID")

    def test_stale_universe_line_count_cannot_crash_primary_or_counterpart_hashing(self) -> None:
        def assert_invalid(expected_code: str) -> None:
            completed, objects = self._run_cli()
            self.assertEqual(completed.returncode, 1, completed.stderr)
            self.assertTrue(objects, completed.stderr)
            self.assertEqual(objects[-1]["status"], "INVALID")
            self.assertIn(expected_code, [item.get("code") for item in objects[:-1]])
            self.assertNotIn("Traceback", completed.stderr)

        universe = self._read_jsonl(self.universe_path)
        next(item for item in universe if item["repo"] == "rustty" and item["path"] == "r.txt")["line_count"] = 4
        self._write_jsonl(self.universe_path, universe)
        primaries = self._primaries()
        primary = next(item for item in primaries if item["range_id"] == "r-last")
        primary.update(end=4, range_sha256="0" * 64)
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
        reviews = self._reviews()
        checked = next(item for item in reviews if item["range_id"] == "r-last")["source_ranges_checked"][0]
        checked.update(end=4, range_sha256="0" * 64)
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)
        assert_invalid("E_STALE_HASH")

        self._write_clean_fixture()
        universe = self._read_jsonl(self.universe_path)
        next(item for item in universe if item["repo"] == "ghostty" and item["path"] == "g.txt")["line_count"] = 4
        self._write_jsonl(self.universe_path, universe)
        primaries = self._primaries()
        primary = next(item for item in primaries if item["range_id"] == "r-last")
        primary["counterparts"][0].update(end=4, range_sha256="0" * 64)
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
        reviews = self._reviews()
        checked = next(item for item in reviews if item["range_id"] == "r-last")["source_ranges_checked"][1]
        checked.update(end=4, range_sha256="0" * 64)
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)
        assert_invalid("E_COUNTERPART")

    def _assert_diagnostic_partition(self, summary: dict) -> None:
        for field in ("invalid_diagnostics", "incomplete_diagnostics"):
            partition = sum(repository[field] for repository in summary["repositories"].values())
            partition += summary["unattributed"][field]
            self.assertEqual(partition, summary["total"][field])

    def test_parsed_identity_occurrences_drive_duplicates_and_review_attribution_only(self) -> None:
        cases = (
            ("valid-rejected-same", 7, True),
            ("valid-rejected-cross", 7, False),
            ("rejected-rejected-same", 6, True),
            ("rejected-rejected-cross", 5, False),
        )
        for case, expected_covered, same_repo in cases:
            with self.subTest(case=case):
                self._write_clean_fixture()
                primaries = self._primaries()
                ghost = next(item for item in primaries if item["range_id"] == "g-first")
                rust = next(item for item in primaries if item["range_id"] == "r-first")
                target_id = "g-first" if "same" in case or case == "rejected-rejected-cross" else "r-first"

                if case == "valid-rejected-same":
                    duplicate = copy.deepcopy(ghost)
                    duplicate.pop("contract")
                    primaries.append(duplicate)
                elif case == "valid-rejected-cross":
                    duplicate = copy.deepcopy(ghost)
                    duplicate["range_id"] = "r-first"
                    duplicate.pop("contract")
                    primaries.append(duplicate)
                elif case == "rejected-rejected-same":
                    ghost.pop("contract")
                    primaries.append(copy.deepcopy(ghost))
                else:
                    ghost.pop("contract")
                    rust.pop("contract")
                    rust["range_id"] = "g-first"

                self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
                reviews = self._reviews()
                next(item for item in reviews if item["range_id"] == target_id).pop("falsification")
                self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)

                result = self._run()
                duplicates = [
                    item for item in result.diagnostics
                    if item["code"] == "E_RANGE_DUPLICATE" and item.get("range_id") == target_id
                ]
                review_schema = [
                    item for item in result.diagnostics
                    if item["code"] == "E_REVIEW_SCHEMA" and "falsification" in item["message"]
                ]

                self.assertEqual(len(duplicates), 1, result.diagnostics)
                self.assertEqual({item.get("repo") for item in duplicates}, {"ghostty" if same_repo else None})
                self.assertTrue(review_schema)
                self.assertTrue(all("repo" not in item for item in review_schema))
                self.assertEqual(result.summary["total"]["covered_lines"], expected_covered)
                self.assertEqual(result.summary["total"]["primary_final_lines"], expected_covered)
                self._assert_diagnostic_partition(result.summary)

    def test_empty_universe_is_invalid(self) -> None:
        self.universe_path.write_text("\n", encoding="utf-8")
        self._assert_code("E_UNIVERSE_EMPTY", 1)

    def test_deleted_universe_record_is_a_fileset_error(self) -> None:
        records = self._read_jsonl(self.universe_path)
        self._write_jsonl(self.universe_path, records[:-1])
        self._assert_code("E_SOURCE_FILESET", 1)

    def test_duplicate_universe_record_is_invalid(self) -> None:
        records = self._read_jsonl(self.universe_path)
        self._write_jsonl(self.universe_path, records + [copy.deepcopy(records[0])])
        self._assert_code("E_UNIVERSE_DUPLICATE", 1)

    def test_current_repository_revision_drift_is_invalid(self) -> None:
        (self.rustty / "new.txt").write_text("new commit\n", encoding="utf-8")
        self._git(self.rustty, "add", "new.txt")
        self._git(self.rustty, "commit", "--quiet", "-m", "drift")
        self._assert_code("E_SOURCE_REVISION", 1)

    def test_stale_universe_hash_is_a_fileset_error(self) -> None:
        records = self._read_jsonl(self.universe_path)
        records[0]["sha256"] = "0" * 64
        self._write_jsonl(self.universe_path, records)
        self._assert_code("E_SOURCE_FILESET", 1)

    def test_uncovered_first_middle_and_last_lines_are_incomplete(self) -> None:
        original_primaries = self._primaries()
        original_reviews = self._reviews()
        for range_id in ("r-first", "r-middle", "r-last"):
            with self.subTest(range_id=range_id):
                self._write_jsonl(
                    self.ledger / "batch-clean.jsonl",
                    [item for item in original_primaries if item["range_id"] != range_id],
                )
                self._write_jsonl(
                    self.reviews / "batch-clean.jsonl",
                    [item for item in original_reviews if item["range_id"] != range_id],
                )
                result = self._assert_code("E_UNCOVERED", 2)
                self.assertEqual(result.summary["total"]["uncovered_lines"], 1)

    def test_overlap_and_out_of_bounds_ranges_are_invalid(self) -> None:
        original_primaries = self._primaries()
        original_reviews = self._reviews()

        primaries = copy.deepcopy(original_primaries)
        reviews = copy.deepcopy(original_reviews)
        primary = next(item for item in primaries if item["range_id"] == "r-middle")
        primary.update(start=1, end=2, range_sha256=self._reference("rustty", "r.txt", 1, 2)["range_sha256"])
        review = next(item for item in reviews if item["range_id"] == "r-middle")
        review["source_ranges_checked"][0] = {
            field: primary[field] for field in ("repo", "revision", "path", "start", "end", "range_sha256")
        }
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)
        self._assert_code("E_OVERLAP", 1)

        primaries = copy.deepcopy(original_primaries)
        reviews = copy.deepcopy(original_reviews)
        primary = next(item for item in primaries if item["range_id"] == "r-last")
        primary.update(end=4, range_sha256="0" * 64)
        review = next(item for item in reviews if item["range_id"] == "r-last")
        review["source_ranges_checked"][0] = {
            field: primary[field] for field in ("repo", "revision", "path", "start", "end", "range_sha256")
        }
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)
        self._assert_code("E_BOUNDS", 1)

    def test_stale_primary_range_hash_is_invalid(self) -> None:
        primaries = self._primaries()
        next(item for item in primaries if item["range_id"] == "r-middle")["range_sha256"] = "0" * 64
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
        self._assert_code("E_STALE_HASH", 1)

    def test_absent_malformed_and_mismatched_counterparts_are_invalid(self) -> None:
        original = self._primaries()
        mutations = (
            ([], "E_COUNTERPART"),
            ([{"repo": "ghostty"}], "E_COUNTERPART"),
            ([{**self._reference("ghostty", "g.txt", 1, 1), "range_sha256": "0" * 64}], "E_COUNTERPART"),
        )
        for counterpart, code in mutations:
            with self.subTest(counterpart=counterpart):
                primaries = copy.deepcopy(original)
                next(item for item in primaries if item["range_id"] == "r-first")["counterparts"] = counterpart
                self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
                self._assert_code(code, 1)

    def test_pending_disposition_is_incomplete_even_with_upheld_review(self) -> None:
        primaries = self._primaries()
        primary = next(item for item in primaries if item["range_id"] == "r-middle")
        primary["disposition"] = "pending"
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
        result = self._assert_code("E_PENDING", 2)
        self.assertEqual(result.summary["total"]["pending_lines"], 1)

    def test_high_risk_equivalence_requires_control_evidence(self) -> None:
        primaries = self._primaries()
        primary = next(item for item in primaries if item["range_id"] == "r-middle")
        primary["risk_tags"] = ["unsafe"]
        primary.pop("control_evidence", None)
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
        self._assert_code("E_HIGH_RISK_PROOF", 1)

    def test_intentional_divergence_requires_authority_evidence(self) -> None:
        primaries = self._primaries()
        primary = next(item for item in primaries if item["range_id"] == "r-middle")
        primary["disposition"] = "intentional_divergence"
        primary.pop("authority_evidence", None)
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
        self._assert_code("E_AUTHORITY_EVIDENCE", 1)

    def test_missing_or_nonaccepting_latest_review_is_incomplete(self) -> None:
        original = self._reviews()
        self._write_jsonl(
            self.reviews / "batch-clean.jsonl",
            [item for item in original if item["range_id"] != "r-middle"],
        )
        self._assert_code("E_REVIEW_MISSING", 2)

        reviews = copy.deepcopy(original)
        next(item for item in reviews if item["range_id"] == "r-middle")["verdict"] = "needs_more_evidence"
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)
        self._assert_code("E_REVIEW_MISSING", 2)

    def test_latest_self_review_is_invalid(self) -> None:
        reviews = self._reviews()
        next(item for item in reviews if item["range_id"] == "r-middle")["secondary_reviewer"] = "primary-auditor"
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)
        self._assert_code("E_SELF_REVIEW", 1)

    def test_broken_noncontiguous_review_chain_is_invalid(self) -> None:
        reviews = self._reviews()
        prior = next(item for item in reviews if item["range_id"] == "r-middle")
        later = copy.deepcopy(prior)
        later.update(review_revision=3, supersedes=1, reviewed_at="2026-07-31T02:00:00Z")
        reviews.append(later)
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)
        self._assert_code("E_REVIEW_CHAIN", 1)

    def test_review_must_check_primary_and_every_counterpart(self) -> None:
        reviews = self._reviews()
        review = next(item for item in reviews if item["range_id"] == "r-middle")
        review["source_ranges_checked"] = review["source_ranges_checked"][:1]
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)
        self._assert_code("E_REVIEW_SOURCES", 1)

    def test_exclusion_requires_nonempty_file_evidence(self) -> None:
        records = self._read_jsonl(self.universe_path)
        excluded = next(item for item in records if item["path"] == "excluded.txt")
        excluded["classification_evidence"] = ""
        self._write_jsonl(self.universe_path, records)
        self._assert_code("E_EXCLUSION_EVIDENCE", 1)

    def test_gap_without_finding_finding_without_gap_and_unknown_range_are_invalid(self) -> None:
        finding = self.findings / "F-LA-001.md"
        finding.unlink()
        self._assert_code("E_FINDING_LINK", 1)

        self._write_clean_fixture()
        (self.findings / "F-LA-002.md").write_text(
            '---\n{"id":"F-LA-002","range_ids":["r-first"]}\n---\n', encoding="utf-8"
        )
        self._assert_code("E_FINDING_LINK", 1)

        (self.findings / "F-LA-002.md").write_text(
            '---\n{"id":"F-LA-002","range_ids":["unknown-range"]}\n---\n', encoding="utf-8"
        )
        self._assert_code("E_FINDING_LINK", 1)

    def test_duplicate_range_id_is_invalid(self) -> None:
        primaries = self._primaries()
        primaries[-1]["range_id"] = primaries[0]["range_id"]
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
        self._assert_code("E_RANGE_DUPLICATE", 1)

    def test_non_gap_record_cannot_cite_a_finding(self) -> None:
        primaries = self._primaries()
        next(item for item in primaries if item["range_id"] == "r-first")["finding_ids"] = ["F-LA-001"]
        (self.findings / "F-LA-001.md").write_text(
            '---\n{"id":"F-LA-001","range_ids":["g-gap","r-first"]}\n---\n', encoding="utf-8"
        )
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
        self._assert_code("E_FINDING_LINK", 1)

    def test_schema_unknown_review_and_duplicate_review_diagnostics_are_stable(self) -> None:
        universe = self._read_jsonl(self.universe_path)
        universe[0].pop("sha256")
        self._write_jsonl(self.universe_path, universe)
        self._assert_code("E_UNIVERSE_SCHEMA", 1)

        self._write_clean_fixture()
        primaries = self._primaries()
        primaries[0].pop("contract")
        self._write_jsonl(self.ledger / "batch-clean.jsonl", primaries)
        self._assert_code("E_LEDGER_SCHEMA", 1)

        self._write_clean_fixture()
        reviews = self._reviews()
        reviews[0].pop("falsification")
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)
        self._assert_code("E_REVIEW_SCHEMA", 1)

        self._write_clean_fixture()
        reviews = self._reviews()
        reviews[0]["range_id"] = "unknown-range"
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)
        self._assert_code("E_REVIEW_UNKNOWN", 1)

        self._write_clean_fixture()
        reviews = self._reviews()
        reviews.append(copy.deepcopy(reviews[0]))
        self._write_jsonl(self.reviews / "batch-clean.jsonl", reviews)
        self._assert_code("E_REVIEW_CHAIN", 1)

    def test_only_direct_named_records_count_and_summary_cannot_be_tampered(self) -> None:
        (self.ledger / "README.md").write_text('{"disposition":"pending"}\n', encoding="utf-8")
        nested = self.ledger / "nested"
        nested.mkdir()
        (nested / "ignored.jsonl").write_text("not json\n", encoding="utf-8")
        (self.reviews / "README.md").write_text("not a review\n", encoding="utf-8")
        (self.findings / "README.md").write_text("No promoted finding here.\n", encoding="utf-8")

        result = self._run()

        self.assertEqual(result.exit_code, 0)
        self.assertEqual(result.summary["total"]["text_lines"], 7)
        self.assertEqual(result.summary["total"]["covered_lines"], 7)

    def test_allow_incomplete_cli_stays_exit_two_and_emits_json_summary(self) -> None:
        (self.ledger / "batch-clean.jsonl").unlink()
        (self.reviews / "batch-clean.jsonl").unlink()
        (self.findings / "F-LA-001.md").unlink()
        command = [
            sys.executable,
            str(SCRIPTS / "verify_ledger.py"),
            "--universe", str(self.universe_path),
            "--ledger", str(self.ledger),
            "--reviews", str(self.reviews),
            "--findings", str(self.findings),
            "--rustty", str(self.rustty),
            "--ghostty", str(self.ghostty),
            "--allow-incomplete",
        ]

        completed = subprocess.run(command, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True)
        objects = [json.loads(line) for line in completed.stdout.splitlines()]

        self.assertEqual(completed.returncode, 2)
        self.assertEqual(objects[-1]["status"], "INCOMPLETE")
        self.assertEqual(objects[-1]["total"]["uncovered_lines"], 7)
        self.assertEqual(objects[-1]["total"]["invalid_diagnostics"], 0)
        self.assertTrue(all({"code", "message"} <= set(item) for item in objects[:-1]))

    def test_missing_input_directory_is_invalid_not_incomplete(self) -> None:
        result = verify_ledger.verify(
            universe=self.universe_path,
            ledger=self.root / "missing-ledger",
            reviews=self.reviews,
            findings=self.findings,
            rustty=self.rustty,
            ghostty=self.ghostty,
        )

        self.assertEqual(result.exit_code, 1, result.diagnostics)
        self.assertIn("E_INPUT", [item["code"] for item in result.diagnostics])


if __name__ == "__main__":
    unittest.main()
