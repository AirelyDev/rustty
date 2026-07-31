"""Canonical byte, range, and ledger-record validation for the line audit."""

from __future__ import annotations

from datetime import datetime
import hashlib
import re
from typing import Any


CONTENT_KINDS = {"text", "binary", "empty"}
SCOPE_CLASSES = {
    "semantic_required",
    "pure_macos_out_of_scope",
    "vendored_identical",
    "generated_reproducible",
    "binary_asset",
    "documentation_nonbehavioral",
}
DISPOSITIONS = {
    "exact_equivalent",
    "semantic_equivalent",
    "port_specific_valid",
    "oracle_not_applicable",
    "intentional_divergence",
    "parity_gap",
    "pending",
}
SECONDARY_VERDICTS = {
    "upheld",
    "upheld_with_correction",
    "rejected",
    "needs_more_evidence",
}

_SHA256_RE = re.compile(r"[0-9a-f]{64}\Z")
_GIT_REVISION_RE = re.compile(r"(?:[0-9a-f]{40}|[0-9a-f]{64})\Z")
_RISK_TAG_RE = re.compile(r"[a-z][a-z0-9]*(?:_[a-z0-9]+)*\Z")
_RFC3339_UTC_RE = re.compile(
    r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?Z\Z"
)
_NONSEMANTIC_DISPOSITIONS = {
    "pure_macos_out_of_scope": "oracle_not_applicable",
    "vendored_identical": "exact_equivalent",
    "generated_reproducible": "exact_equivalent",
    "binary_asset": "oracle_not_applicable",
    "documentation_nonbehavioral": "port_specific_valid",
}


def split_physical_lines(data: bytes) -> list[bytes]:
    """Return LF-delimited raw physical lines, retaining their terminators."""
    if not data:
        return []
    fragments = data.split(b"\n")
    lines = [fragment + b"\n" for fragment in fragments[:-1]]
    if fragments[-1]:
        lines.append(fragments[-1])
    return lines


def range_sha256(lines: list[bytes], start: int, end: int) -> str:
    """Hash raw line bytes selected by 1-based inclusive bounds."""
    if start < 1:
        raise ValueError("start must be at least 1")
    if end < start:
        raise ValueError("end must not precede start")
    if end > len(lines):
        raise ValueError("end exceeds line count")
    return hashlib.sha256(b"".join(lines[start - 1:end])).hexdigest()


def _is_int(value: Any) -> bool:
    return isinstance(value, int) and not isinstance(value, bool)


def _nonempty_string(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def _path_is_valid(value: Any) -> bool:
    if not _nonempty_string(value) or value.startswith("/") or "\\" in value:
        return False
    return all(component not in {"", ".", ".."} for component in value.split("/"))


def _timestamp_is_valid(value: Any) -> bool:
    if not isinstance(value, str) or not _RFC3339_UTC_RE.fullmatch(value):
        return False
    try:
        datetime.fromisoformat(value[:-1] + "+00:00")
    except ValueError:
        return False
    return True


def _required(record: dict[str, Any], fields: tuple[str, ...]) -> list[str]:
    return [f"missing required field: {field}" for field in fields if field not in record]


def _enum(record: dict[str, Any], field: str, values: set[str]) -> list[str]:
    if field in record and (not isinstance(record[field], str) or record[field] not in values):
        return [f"{field} must be one of: {', '.join(sorted(values))}"]
    return []


def _hash(record: dict[str, Any], field: str) -> list[str]:
    if field in record and (not isinstance(record[field], str) or not _SHA256_RE.fullmatch(record[field])):
        return [f"{field} must be a 64-character lowercase hexadecimal SHA-256"]
    return []


def _revision(record: dict[str, Any], field: str = "revision") -> list[str]:
    if field in record and (not isinstance(record[field], str) or not _GIT_REVISION_RE.fullmatch(record[field])):
        return [f"{field} must be a 40- or 64-character lowercase hexadecimal Git object ID"]
    return []


def _path(record: dict[str, Any], field: str = "path") -> list[str]:
    if field in record and not _path_is_valid(record[field]):
        return [f"{field} must be a nonempty relative POSIX path without . or .. components"]
    return []


def _integer(record: dict[str, Any], field: str, *, positive: bool = False) -> list[str]:
    if field not in record:
        return []
    value = record[field]
    if not _is_int(value) or (value < 1 if positive else value < 0):
        kind = "positive integer" if positive else "nonnegative integer"
        return [f"{field} must be a {kind} and not a boolean"]
    return []


def _reference_diagnostics(reference: Any, prefix: str) -> list[str]:
    if not isinstance(reference, dict):
        return [f"{prefix} must be an object"]
    diagnostics = _required(reference, ("repo", "revision", "path", "start", "end", "range_sha256"))
    diagnostics += _enum(reference, "repo", {"ghostty", "rustty"})
    diagnostics += _revision(reference)
    diagnostics += _path(reference)
    diagnostics += _integer(reference, "start", positive=True)
    diagnostics += _integer(reference, "end", positive=True)
    diagnostics += _hash(reference, "range_sha256")
    if _is_int(reference.get("start")) and _is_int(reference.get("end")) and reference["end"] < reference["start"]:
        diagnostics.append(f"{prefix}.end must not precede {prefix}.start")
    return [f"{prefix}: {item}" for item in diagnostics]


def validate_file(record: dict) -> list[str]:
    """Return every deterministic file-record schema diagnostic."""
    if not isinstance(record, dict):
        return ["file record must be an object"]
    fields = (
        "repo", "revision", "path", "byte_length", "line_count", "sha256",
        "content_kind", "scope_class", "classification_reason", "classification_evidence",
    )
    diagnostics = _required(record, fields)
    diagnostics += _enum(record, "repo", {"ghostty", "rustty"})
    diagnostics += _revision(record)
    diagnostics += _path(record)
    diagnostics += _integer(record, "byte_length")
    diagnostics += _integer(record, "line_count")
    diagnostics += _hash(record, "sha256")
    diagnostics += _enum(record, "content_kind", CONTENT_KINDS)
    diagnostics += _enum(record, "scope_class", SCOPE_CLASSES)
    for field in ("classification_reason", "classification_evidence"):
        if field in record and not isinstance(record[field], str):
            diagnostics.append(f"{field} must be a string")
    if record.get("content_kind") == "text":
        if _is_int(record.get("byte_length")) and record["byte_length"] < 1:
            diagnostics.append("text content_kind requires a positive byte_length")
        if _is_int(record.get("line_count")) and record["line_count"] < 1:
            diagnostics.append("text content_kind requires a positive line_count")
    if record.get("content_kind") == "empty":
        if record.get("byte_length") != 0:
            diagnostics.append("empty content_kind requires byte_length of zero")
        if record.get("line_count") != 0:
            diagnostics.append("empty content_kind requires line_count of zero")
    if record.get("content_kind") == "binary" and record.get("line_count") != 0:
        diagnostics.append("binary content_kind requires line_count of zero")
    if isinstance(record.get("scope_class"), str) and record["scope_class"] in _NONSEMANTIC_DISPOSITIONS:
        for field in ("classification_reason", "classification_evidence"):
            if not _nonempty_string(record.get(field)):
                diagnostics.append(f"{field} must be nonempty for a non-semantic scope_class")
    return diagnostics


def validate_primary(record: dict) -> list[str]:
    """Return every deterministic primary-review record schema diagnostic."""
    if not isinstance(record, dict):
        return ["primary record must be an object"]
    fields = (
        "batch_id", "range_id", "repo", "revision", "path", "start", "end",
        "range_sha256", "scope_class", "risk_tags", "counterparts", "disposition",
        "contract", "analysis", "reachability", "verification", "observed_result",
        "limitations", "finding_ids", "primary_reviewer", "reviewed_at",
    )
    diagnostics = _required(record, fields)
    for field in ("batch_id", "range_id", "primary_reviewer"):
        if field in record and not _nonempty_string(record[field]):
            diagnostics.append(f"{field} must be a nonempty string")
    diagnostics += _enum(record, "repo", {"ghostty", "rustty"})
    diagnostics += _revision(record)
    diagnostics += _path(record)
    diagnostics += _integer(record, "start", positive=True)
    diagnostics += _integer(record, "end", positive=True)
    if _is_int(record.get("start")) and _is_int(record.get("end")) and record["end"] < record["start"]:
        diagnostics.append("end must not precede start")
    diagnostics += _hash(record, "range_sha256")
    diagnostics += _enum(record, "scope_class", SCOPE_CLASSES)
    diagnostics += _enum(record, "disposition", DISPOSITIONS)
    if "risk_tags" in record:
        if not isinstance(record["risk_tags"], list):
            diagnostics.append("risk_tags must be an array")
        else:
            seen: set[str] = set()
            for index, tag in enumerate(record["risk_tags"]):
                if not isinstance(tag, str) or not _RISK_TAG_RE.fullmatch(tag):
                    diagnostics.append(f"risk tag at index {index} must be nonempty lowercase snake_case")
                else:
                    if tag in seen:
                        diagnostics.append(f"duplicate risk tag: {tag}")
                    seen.add(tag)
    if "counterparts" in record:
        if not isinstance(record["counterparts"], list):
            diagnostics.append("counterparts must be an array")
        else:
            for index, counterpart in enumerate(record["counterparts"]):
                diagnostics += _reference_diagnostics(counterpart, f"counterparts[{index}]")
    if "finding_ids" in record:
        if not isinstance(record["finding_ids"], list):
            diagnostics.append("finding_ids must be an array")
        else:
            for index, finding_id in enumerate(record["finding_ids"]):
                if not _nonempty_string(finding_id):
                    diagnostics.append(f"finding_ids[{index}] must be a nonempty string")
    for field in ("contract", "analysis", "reachability", "verification", "observed_result", "limitations"):
        if field in record and not isinstance(record[field], str):
            diagnostics.append(f"{field} must be a string")
    if "reviewed_at" in record and not _timestamp_is_valid(record["reviewed_at"]):
        diagnostics.append("reviewed_at must be an RFC 3339 UTC timestamp ending in Z")
    if (
        record.get("scope_class") == "semantic_required"
        and record.get("disposition") is not None
        and record.get("disposition") != "pending"
    ):
        for field in ("contract", "analysis", "reachability", "verification", "observed_result"):
            if not _nonempty_string(record.get(field)):
                diagnostics.append(f"{field} must be nonempty for a final semantic disposition")
    if record.get("disposition") == "parity_gap":
        if not isinstance(record.get("finding_ids"), list) or not any(_nonempty_string(item) for item in record["finding_ids"]):
            diagnostics.append("parity_gap requires at least one nonempty finding_ids entry")
    scope_class = record.get("scope_class")
    if isinstance(scope_class, str) and scope_class in _NONSEMANTIC_DISPOSITIONS:
        expected = _NONSEMANTIC_DISPOSITIONS[scope_class]
        if record.get("disposition") != expected:
            diagnostics.append(f"{scope_class} requires disposition {expected}")
        if not _nonempty_string(record.get("analysis")):
            diagnostics.append("analysis must be nonempty for a non-semantic scope_class")
    return diagnostics


def validate_primary_with_file(primary: dict, file_record: dict | None) -> list[str]:
    """Validate a primary record together with its matching inventory record."""
    diagnostics = validate_primary(primary)
    if file_record is None:
        return diagnostics + ["file record is required"]
    file_diagnostics = validate_file(file_record)
    diagnostics += [f"file record: {item}" for item in file_diagnostics]
    if not isinstance(primary, dict) or not isinstance(file_record, dict):
        return diagnostics
    primary_key = tuple(primary.get(field) for field in ("repo", "revision", "path"))
    file_key = tuple(file_record.get(field) for field in ("repo", "revision", "path"))
    if primary_key != file_key:
        diagnostics.append("primary and file record must have matching repo, revision, and path")
    if primary.get("scope_class") != file_record.get("scope_class"):
        diagnostics.append("primary and file record must have matching scope_class")
    if isinstance(primary.get("scope_class"), str) and primary["scope_class"] in _NONSEMANTIC_DISPOSITIONS:
        for field in ("classification_reason", "classification_evidence"):
            if not _nonempty_string(file_record.get(field)):
                diagnostics.append(f"file record {field} must be nonempty for a non-semantic primary")
    return diagnostics


def validate_review(record: dict) -> list[str]:
    """Return every deterministic secondary-review record schema diagnostic."""
    if not isinstance(record, dict):
        return ["review record must be an object"]
    fields = (
        "batch_id", "range_id", "review_revision", "secondary_reviewer", "reviewed_at",
        "verdict", "source_ranges_checked", "falsification", "counterevidence", "corrections",
        "supersedes",
    )
    diagnostics = _required(record, fields)
    for field in ("batch_id", "range_id", "secondary_reviewer", "falsification"):
        if field in record and not _nonempty_string(record[field]):
            diagnostics.append(f"{field} must be a nonempty string")
    diagnostics += _integer(record, "review_revision", positive=True)
    if "reviewed_at" in record and not _timestamp_is_valid(record["reviewed_at"]):
        diagnostics.append("reviewed_at must be an RFC 3339 UTC timestamp ending in Z")
    diagnostics += _enum(record, "verdict", SECONDARY_VERDICTS)
    if "source_ranges_checked" in record:
        ranges = record["source_ranges_checked"]
        if not isinstance(ranges, list) or not ranges:
            diagnostics.append("source_ranges_checked must be a nonempty array")
        else:
            for index, source_range in enumerate(ranges):
                diagnostics += _reference_diagnostics(source_range, f"source_ranges_checked[{index}]")
    for field in ("counterevidence", "corrections"):
        if field in record and not isinstance(record[field], str):
            diagnostics.append(f"{field} must be a string")
    revision = record.get("review_revision")
    supersedes = record.get("supersedes")
    if _is_int(revision) and revision > 0:
        if revision == 1 and supersedes is not None:
            diagnostics.append("supersedes must be null for review_revision 1")
        elif revision > 1 and (not _is_int(supersedes) or supersedes < 1 or supersedes >= revision):
            diagnostics.append("supersedes must be a positive prior review revision")
    return diagnostics
