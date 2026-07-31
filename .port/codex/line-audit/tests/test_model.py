import pathlib
import sys
import unittest


SCRIPTS = pathlib.Path(__file__).resolve().parents[1] / "scripts"
sys.path.insert(0, str(SCRIPTS))

from model import (  # noqa: E402
    range_sha256,
    split_physical_lines,
    validate_file,
    validate_primary,
    validate_primary_with_file,
    validate_review,
)


REVISION = "5243303fbb2f2cfd0832edfb8fa49d72ce322d81"
HASH = "a" * 64
STAMP = "2026-07-31T12:34:56Z"


def valid_file(**changes):
    record = {
        "repo": "ghostty",
        "revision": REVISION,
        "path": "src/main.zig",
        "byte_length": 12,
        "line_count": 2,
        "sha256": HASH,
        "content_kind": "text",
        "scope_class": "semantic_required",
        "classification_reason": "",
        "classification_evidence": "",
    }
    record.update(changes)
    return record


def valid_primary(**changes):
    record = {
        "batch_id": "batch-001",
        "range_id": "ghostty:src/main.zig:1-2",
        "repo": "ghostty",
        "revision": REVISION,
        "path": "src/main.zig",
        "start": 1,
        "end": 2,
        "range_sha256": HASH,
        "scope_class": "semantic_required",
        "risk_tags": ["terminal_state", "parser"],
        "counterparts": [{
            "repo": "rustty",
            "revision": REVISION,
            "path": "crates/rustty-vt/src/lib.rs",
            "start": 4,
            "end": 6,
            "range_sha256": HASH,
        }],
        "disposition": "semantic_equivalent",
        "contract": "Writes the same terminal state.",
        "analysis": "Both paths parse the same byte sequence.",
        "reachability": "Exercised by an escape sequence.",
        "verification": "Compared the resulting screen state.",
        "observed_result": "The screen states matched.",
        "limitations": "No performance measurement.",
        "finding_ids": [],
        "primary_reviewer": "reviewer-a",
        "reviewed_at": STAMP,
    }
    record.update(changes)
    return record


def valid_review(**changes):
    record = {
        "batch_id": "batch-001",
        "range_id": "ghostty:src/main.zig:1-2",
        "review_revision": 1,
        "secondary_reviewer": "reviewer-b",
        "reviewed_at": STAMP,
        "verdict": "upheld",
        "source_ranges_checked": [{
            "repo": "ghostty",
            "revision": REVISION,
            "path": "src/main.zig",
            "start": 1,
            "end": 2,
            "range_sha256": HASH,
        }],
        "falsification": "Checked an alternate parser path.",
        "counterevidence": "",
        "corrections": "",
        "supersedes": None,
    }
    record.update(changes)
    return record


class LineOperationsTests(unittest.TestCase):
    def test_split_preserves_mixed_endings_and_unterminated_tail(self):
        self.assertEqual(
            split_physical_lines(b"one\r\ntwo\nthree"),
            [b"one\r\n", b"two\n", b"three"],
        )

    def test_split_empty_input_has_no_physical_lines(self):
        self.assertEqual(split_physical_lines(b""), [])

    def test_split_keeps_bare_carriage_return_as_content_before_lf(self):
        self.assertEqual(split_physical_lines(b"one\rtwo\n"), [b"one\rtwo\n"])

    def test_split_keeps_bare_carriage_return_in_unterminated_tail(self):
        self.assertEqual(split_physical_lines(b"one\rtwo"), [b"one\rtwo"])

    def test_range_hash_uses_one_based_inclusive_raw_lines(self):
        lines = [b"aa\n", b"ab\n", b"cd\n", b"tail"]
        self.assertEqual(
            range_sha256(lines, 2, 3),
            "5141648ccbe924f6462cfc7085ccd21779b89d8cee1438281bf1b4cd8d63ac2a",
        )

    def test_range_hash_rejects_start_before_one(self):
        with self.assertRaises(ValueError):
            range_sha256([b"one\n"], 0, 1)

    def test_range_hash_rejects_end_before_start(self):
        with self.assertRaises(ValueError):
            range_sha256([b"one\n"], 1, 0)

    def test_range_hash_rejects_end_beyond_eof(self):
        with self.assertRaises(ValueError):
            range_sha256([b"one\n"], 1, 2)


class ValidationTests(unittest.TestCase):
    def test_valid_records_have_no_diagnostics(self):
        self.assertEqual(validate_file(valid_file()), [])
        self.assertEqual(validate_primary(valid_primary()), [])
        self.assertEqual(validate_review(valid_review()), [])

    def test_file_rejects_invalid_enums(self):
        diagnostics = validate_file(valid_file(
            repo="other", content_kind="source", scope_class="ignored"
        ))
        self.assertGreaterEqual(len(diagnostics), 3)
        self.assertTrue(any("repo" in item for item in diagnostics))
        self.assertTrue(any("content_kind" in item for item in diagnostics))
        self.assertTrue(any("scope_class" in item for item in diagnostics))

    def test_file_rejects_booleans_for_integer_fields(self):
        diagnostics = validate_file(valid_file(byte_length=True, line_count=False))
        self.assertTrue(any("byte_length" in item for item in diagnostics))
        self.assertTrue(any("line_count" in item for item in diagnostics))

    def test_file_rejects_noncanonical_paths_and_hashes(self):
        diagnostics = validate_file(valid_file(
            path="../src\\main.zig", revision="A" * 40, sha256="deadbeef"
        ))
        self.assertTrue(any("path" in item for item in diagnostics))
        self.assertTrue(any("revision" in item for item in diagnostics))
        self.assertTrue(any("sha256" in item for item in diagnostics))

    def test_file_accepts_sha256_git_revision_and_rejects_wrong_length(self):
        self.assertEqual(validate_file(valid_file(revision=HASH)), [])
        diagnostics = validate_file(valid_file(revision="b" * 39))
        self.assertTrue(any("revision" in item for item in diagnostics))

    def test_text_file_requires_positive_byte_length_and_line_count(self):
        diagnostics = validate_file(valid_file(byte_length=0, line_count=1))
        self.assertTrue(any("byte_length" in item for item in diagnostics))

    def test_file_exclusion_requires_reason_and_evidence(self):
        diagnostics = validate_file(valid_file(
            scope_class="binary_asset", content_kind="binary",
            byte_length=4, line_count=0,
            classification_reason="", classification_evidence="",
        ))
        self.assertTrue(any("classification_reason" in item for item in diagnostics))
        self.assertTrue(any("classification_evidence" in item for item in diagnostics))

    def test_primary_parity_gap_requires_finding(self):
        diagnostics = validate_primary(valid_primary(disposition="parity_gap", finding_ids=[]))
        self.assertTrue(any("finding_ids" in item for item in diagnostics))

    def test_primary_semantic_final_requires_all_evidence_fields(self):
        diagnostics = validate_primary(valid_primary(
            contract="", analysis="", reachability="", verification="", observed_result=""
        ))
        for field in ("contract", "analysis", "reachability", "verification", "observed_result"):
            self.assertTrue(any(field in item for item in diagnostics), field)

    def test_primary_rejects_duplicate_or_invalid_risk_tags(self):
        diagnostics = validate_primary(valid_primary(risk_tags=["parser", "parser", "Bad-tag", ""]))
        self.assertTrue(any("duplicate risk tag" in item for item in diagnostics))
        self.assertGreaterEqual(sum("risk tag" in item for item in diagnostics), 3)

    def test_primary_rejects_invalid_timestamp(self):
        diagnostics = validate_primary(valid_primary(reviewed_at="2026-07-31 12:34:56+00:00"))
        self.assertTrue(any("reviewed_at" in item for item in diagnostics))

    def test_cross_record_validator_accepts_matching_nonsemantic_file(self):
        primary = valid_primary(
            scope_class="binary_asset", disposition="oracle_not_applicable", counterparts=[]
        )
        file_record = valid_file(
            content_kind="binary", byte_length=4, line_count=0,
            scope_class="binary_asset", classification_reason="Icon data.",
            classification_evidence="Inventory identified opaque bytes.",
        )
        self.assertEqual(validate_primary_with_file(primary, file_record), [])

    def test_cross_record_validator_rejects_absent_or_mismatched_file(self):
        primary = valid_primary(
            scope_class="binary_asset", disposition="oracle_not_applicable", counterparts=[]
        )
        self.assertTrue(any("file record is required" in item for item in validate_primary_with_file(primary, None)))
        diagnostics = validate_primary_with_file(primary, valid_file(
            content_kind="binary", byte_length=4, line_count=0,
            scope_class="binary_asset", path="assets/icon.bin",
            classification_reason="Icon data.", classification_evidence="Inventory evidence.",
        ))
        self.assertTrue(any("repo, revision, and path" in item for item in diagnostics))

    def test_cross_record_validator_rejects_scope_and_evidence_failures(self):
        primary = valid_primary(
            scope_class="binary_asset", disposition="oracle_not_applicable", counterparts=[]
        )
        mismatched_scope = valid_file(
            scope_class="documentation_nonbehavioral", classification_reason="Document.",
            classification_evidence="Inventory evidence.",
        )
        self.assertTrue(any("scope_class" in item for item in validate_primary_with_file(primary, mismatched_scope)))
        empty_evidence = valid_file(
            content_kind="binary", byte_length=4, line_count=0, scope_class="binary_asset",
            classification_reason="", classification_evidence="",
        )
        diagnostics = validate_primary_with_file(primary, empty_evidence)
        self.assertTrue(any("classification_reason" in item for item in diagnostics))
        self.assertTrue(any("classification_evidence" in item for item in diagnostics))

    def test_review_enforces_revision_and_supersedes_relationship(self):
        diagnostics = validate_review(valid_review(review_revision=1, supersedes=1))
        self.assertTrue(any("supersedes" in item for item in diagnostics))
        diagnostics = validate_review(valid_review(review_revision=3, supersedes=3))
        self.assertTrue(any("supersedes" in item for item in diagnostics))

    def test_validation_reports_all_independent_errors(self):
        diagnostics = validate_primary({
            "repo": "other", "revision": "bad", "path": "../bad", "start": False,
            "end": 0, "range_sha256": "bad", "scope_class": "other",
            "risk_tags": "not-an-array", "counterparts": {}, "disposition": "other",
            "reviewed_at": "not-a-timestamp",
        })
        self.assertGreaterEqual(len(diagnostics), 15)


if __name__ == "__main__":
    unittest.main()
