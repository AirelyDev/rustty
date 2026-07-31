#!/usr/bin/env python3
"""Fail-closed verifier for the exhaustive Rustty/Ghostty line audit."""

from __future__ import annotations

import argparse
from collections import defaultdict
import copy
from dataclasses import dataclass
import json
from pathlib import Path
import subprocess
import sys
from typing import Any, Iterable, Sequence

from build_universe import build_records, read_committed_blobs
from model import (
    range_sha256,
    split_physical_lines,
    validate_file,
    validate_primary,
    validate_primary_with_file,
    validate_review,
)


HIGH_RISK_TAGS = {
    "platform_cfg", "unsafe", "raw_pointer", "ffi", "allocator",
    "error_conversion", "state_machine", "parser_dispatch", "compression",
    "rendering", "concurrency", "subprocess_environment", "public_abi",
    "serialization", "test_skip", "assertion_weakening",
}
FINAL_EQUIVALENCES = {"exact_equivalent", "semantic_equivalent", "port_specific_valid"}
ACCEPTABLE_REVIEWS = {"upheld", "upheld_with_correction"}
SUMMARY_FIELDS = (
    "files", "text_lines", "covered_lines", "uncovered_lines", "pending_lines",
    "primary_final_lines", "secondary_upheld_lines", "invalid_diagnostics",
    "incomplete_diagnostics",
)
FILESET_FIELDS = ("path", "byte_length", "line_count", "sha256", "content_kind")
REFERENCE_FIELDS = ("repo", "revision", "path", "start", "end", "range_sha256")
ENUM_FIELDS = {"repo", "content_kind", "scope_class", "disposition", "verdict"}


@dataclass(frozen=True)
class VerificationResult:
    """Complete in-memory result used by tests and the CLI."""

    exit_code: int
    diagnostics: list[dict[str, Any]]
    summary: dict[str, Any]


def _diagnostic(
    diagnostics: list[dict[str, Any]],
    code: str,
    message: str,
    classification: str,
    **location: Any,
) -> None:
    diagnostics.append({"code": code, "message": message, "_classification": classification, **location})


def _nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def _safe_validation(validator: Any, *arguments: Any) -> list[str]:
    """Turn validator type failures into schema diagnostics, never CLI crashes."""
    try:
        return validator(*(_schema_safe_value(argument) for argument in arguments))
    except Exception as error:  # canonical validators receive adversarial JSON values
        return [f"schema validator rejected malformed field types: {type(error).__name__}: {error}"]


def _schema_safe_value(value: Any) -> Any:
    """Copy JSON while making unhashable enum values validator-safe but invalid."""
    if isinstance(value, dict):
        copied: dict[str, Any] = {}
        for field, item in value.items():
            if field in ENUM_FIELDS and isinstance(item, (dict, list)):
                copied[field] = None
            else:
                copied[field] = _schema_safe_value(item)
        return copied
    if isinstance(value, list):
        return [_schema_safe_value(item) for item in value]
    return copy.deepcopy(value)


def _known_repo(record: Any) -> str | None:
    if not isinstance(record, dict):
        return None
    repo = record.get("repo")
    if isinstance(repo, str) and repo in {"ghostty", "rustty"}:
        return repo
    return None


def _review_repo(record: Any, attribution_by_id: dict[str, str]) -> str | None:
    if not isinstance(record, dict):
        return None
    range_id = record.get("range_id")
    return attribution_by_id.get(range_id) if isinstance(range_id, str) else None


def _attributed(location: dict[str, Any], repo: str | None) -> dict[str, Any]:
    return {**location, **({"repo": repo} if repo is not None else {})}


def _load_jsonl(
    path: Path,
    code: str,
    diagnostics: list[dict[str, Any]],
) -> list[tuple[Any, dict[str, Any]]]:
    loaded: list[tuple[Any, dict[str, Any]]] = []
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeError) as error:
        _diagnostic(diagnostics, code, f"cannot read JSONL input: {error}", "invalid", file=str(path))
        return loaded
    for number, line in enumerate(lines, 1):
        if not line.strip():
            continue
        location = {"file": str(path), "line": number}
        try:
            loaded.append((json.loads(line), location))
        except json.JSONDecodeError as error:
            _diagnostic(diagnostics, code, f"invalid JSON: {error.msg}", "invalid", **location)
    return loaded


def _load_directory_jsonl(
    directory: Path,
    code: str,
    diagnostics: list[dict[str, Any]],
) -> list[tuple[Any, dict[str, Any]]]:
    try:
        files = sorted(directory.glob("*.jsonl"), key=lambda path: path.name)
    except OSError as error:
        _diagnostic(diagnostics, code, f"cannot list input directory: {error}", "invalid", directory=str(directory))
        return []
    return [item for path in files for item in _load_jsonl(path, code, diagnostics)]


def _head(repo: Path) -> str:
    return subprocess.run(
        ["git", "-C", str(repo), "rev-parse", "--verify", "HEAD^{commit}"],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    ).stdout.strip()


def _file_key(record: dict[str, Any]) -> tuple[Any, Any, Any]:
    return record.get("repo"), record.get("revision"), record.get("path")


def _reference_key(reference: dict[str, Any]) -> tuple[Any, ...]:
    return tuple(reference.get(field) for field in REFERENCE_FIELDS)


def _reference_from_primary(primary: dict[str, Any]) -> dict[str, Any]:
    return {field: primary.get(field) for field in REFERENCE_FIELDS}


def _union_length(intervals: Iterable[tuple[int, int]]) -> int:
    ordered = sorted(intervals)
    if not ordered:
        return 0
    total = 0
    start, end = ordered[0]
    for next_start, next_end in ordered[1:]:
        if next_start <= end + 1:
            end = max(end, next_end)
        else:
            total += end - start + 1
            start, end = next_start, next_end
    return total + end - start + 1


def _uncovered_intervals(line_count: int, intervals: Iterable[tuple[int, int]]) -> list[tuple[int, int]]:
    ordered = sorted(intervals)
    uncovered: list[tuple[int, int]] = []
    cursor = 1
    furthest = 0
    for start, end in ordered:
        if start > cursor:
            uncovered.append((cursor, start - 1))
        furthest = max(furthest, end)
        cursor = max(cursor, end + 1)
    if cursor <= line_count:
        uncovered.append((cursor, line_count))
    return uncovered


def _load_findings(
    directory: Path,
    diagnostics: list[dict[str, Any]],
) -> dict[str, dict[str, Any]]:
    findings: dict[str, dict[str, Any]] = {}
    try:
        paths = sorted(directory.glob("*.md"), key=lambda path: path.name)
    except OSError as error:
        _diagnostic(
            diagnostics, "E_FINDING_LINK", f"cannot list findings directory: {error}",
            "invalid", directory=str(directory),
        )
        return findings
    for path in paths:
        try:
            lines = path.read_text(encoding="utf-8").splitlines()
        except (OSError, UnicodeError) as error:
            _diagnostic(
                diagnostics, "E_FINDING_LINK", f"cannot read finding: {error}",
                "invalid", file=str(path),
            )
            continue
        if not lines or lines[0] != "---":
            continue
        if len(lines) < 3 or lines[2] != "---":
            _diagnostic(
                diagnostics, "E_FINDING_LINK", "promoted finding lacks its closing delimiter",
                "invalid", file=str(path),
            )
            continue
        try:
            header = json.loads(lines[1])
        except (json.JSONDecodeError, IndexError) as error:
            _diagnostic(
                diagnostics, "E_FINDING_LINK", f"invalid finding header JSON: {error}",
                "invalid", file=str(path), line=2,
            )
            continue
        finding_id = header.get("id") if isinstance(header, dict) else None
        range_ids = header.get("range_ids") if isinstance(header, dict) else None
        if not _nonempty(finding_id) or not isinstance(range_ids, list) or not range_ids:
            _diagnostic(
                diagnostics, "E_FINDING_LINK", "finding requires a nonempty id and range_ids array",
                "invalid", file=str(path),
            )
            continue
        if any(not _nonempty(item) for item in range_ids) or len(set(range_ids)) != len(range_ids):
            _diagnostic(
                diagnostics, "E_FINDING_LINK", "finding range_ids must be unique nonempty strings",
                "invalid", file=str(path), finding_id=finding_id,
            )
            continue
        if finding_id in findings:
            _diagnostic(
                diagnostics, "E_FINDING_LINK", "duplicate finding id",
                "invalid", file=str(path), finding_id=finding_id,
            )
            continue
        findings[finding_id] = {"id": finding_id, "range_ids": range_ids, "file": str(path)}
    return findings


def verify(
    *,
    universe: Path,
    ledger: Path,
    reviews: Path,
    findings: Path,
    rustty: Path,
    ghostty: Path,
) -> VerificationResult:
    """Validate all audit inputs and derive exact coverage/review totals."""
    diagnostics: list[dict[str, Any]] = []
    universe_path = Path(universe)
    repository_paths = {"rustty": Path(rustty), "ghostty": Path(ghostty)}
    if not universe_path.is_file():
        _diagnostic(
            diagnostics, "E_INPUT", "universe input is not a readable file",
            "invalid", file=str(universe_path),
        )
    for label, directory in (
        ("ledger", Path(ledger)), ("reviews", Path(reviews)), ("findings", Path(findings))
    ):
        if not directory.is_dir():
            _diagnostic(
                diagnostics, "E_INPUT", f"{label} input is not a directory",
                "invalid", directory=str(directory),
            )

    universe_loaded = _load_jsonl(universe_path, "E_UNIVERSE_SCHEMA", diagnostics)
    universe_records: list[dict[str, Any]] = []
    seen_file_keys: set[tuple[Any, Any, Any]] = set()
    for raw, location in universe_loaded:
        errors = _safe_validation(validate_file, raw)
        repo = _known_repo(raw)
        for error in errors:
            _diagnostic(
                diagnostics, "E_UNIVERSE_SCHEMA", error, "invalid",
                **_attributed(location, repo),
            )
        if isinstance(raw, dict) and raw.get("scope_class") != "semantic_required" and (
            not _nonempty(raw.get("classification_reason"))
            or not _nonempty(raw.get("classification_evidence"))
        ):
            _diagnostic(
                diagnostics, "E_EXCLUSION_EVIDENCE",
                "nonsemantic file requires classification reason and evidence",
                "invalid", path=raw.get("path"), **_attributed(location, repo),
            )
        if errors or not isinstance(raw, dict):
            continue
        universe_records.append(raw)
        key = _file_key(raw)
        if key in seen_file_keys:
            _diagnostic(
                diagnostics, "E_UNIVERSE_DUPLICATE", "duplicate universe file key",
                "invalid", revision=raw["revision"], path=raw["path"],
                **_attributed(location, repo),
            )
        seen_file_keys.add(key)
    if not universe_loaded:
        _diagnostic(
            diagnostics, "E_UNIVERSE_EMPTY", "universe has no nonblank file records",
            "invalid", file=str(universe_path),
        )

    heads: dict[str, str] = {}
    fresh_records: dict[str, list[dict[str, Any]]] = {}
    blobs: dict[str, dict[str, bytes]] = {}
    for repo_name, repo_path in repository_paths.items():
        try:
            heads[repo_name] = _head(repo_path)
            fresh_records[repo_name] = build_records(repo_path, repo_name)
        except (OSError, ValueError, subprocess.CalledProcessError) as error:
            _diagnostic(
                diagnostics, "E_SOURCE_REVISION", f"cannot inspect supplied repository: {error}",
                "invalid", repo=repo_name, repository=str(repo_path),
            )
            fresh_records[repo_name] = []
            blobs[repo_name] = {}
            continue
        revisions = {record.get("revision") for record in universe_records if record.get("repo") == repo_name}
        if revisions != {heads[repo_name]}:
            _diagnostic(
                diagnostics, "E_SOURCE_REVISION",
                f"universe revisions {sorted(str(item) for item in revisions)} do not equal current HEAD {heads[repo_name]}",
                "invalid", repo=repo_name, repository=str(repo_path),
            )
        expected = sorted(
            tuple(record.get(field) for field in FILESET_FIELDS)
            for record in universe_records if record.get("repo") == repo_name
        )
        actual = sorted(tuple(record.get(field) for field in FILESET_FIELDS) for record in fresh_records[repo_name])
        if expected != actual:
            _diagnostic(
                diagnostics, "E_SOURCE_FILESET",
                "universe file set or committed file metadata differs from the supplied tree",
                "invalid", repo=repo_name, repository=str(repo_path),
            )

    file_by_key = {_file_key(record): record for record in universe_records}
    ledger_loaded = _load_directory_jsonl(Path(ledger), "E_LEDGER_SCHEMA", diagnostics)
    identity_occurrences: dict[str, list[tuple[str, dict[str, Any]]]] = defaultdict(list)
    for raw, location in ledger_loaded:
        if not isinstance(raw, dict):
            continue
        range_id = raw.get("range_id")
        repo = _known_repo(raw)
        if _nonempty(range_id) and repo is not None:
            identity_occurrences[range_id].append((repo, location))
    primary_attribution_by_id = {
        range_id: occurrences[0][0]
        for range_id, occurrences in identity_occurrences.items()
        if len(occurrences) == 1
    }
    duplicate_range_ids = {
        range_id for range_id, occurrences in identity_occurrences.items() if len(occurrences) > 1
    }
    for range_id in sorted(duplicate_range_ids):
        occurrences = identity_occurrences[range_id]
        repositories = {repo for repo, _location in occurrences}
        repo = next(iter(repositories)) if len(repositories) == 1 else None
        _diagnostic(
            diagnostics, "E_RANGE_DUPLICATE", "duplicate primary range_id",
            "invalid", range_id=range_id, occurrences=len(occurrences),
            **_attributed(occurrences[1][1], repo),
        )
    validated_primaries: list[tuple[dict[str, Any], dict[str, Any]]] = []
    for raw, location in ledger_loaded:
        repo = _known_repo(raw)
        errors = _safe_validation(validate_primary, raw)
        if errors:
            for error in errors:
                _diagnostic(
                    diagnostics, "E_LEDGER_SCHEMA", error, "invalid",
                    **_attributed(location, repo),
                )
            if any("counterparts" in error for error in errors):
                _diagnostic(
                    diagnostics, "E_COUNTERPART", "counterpart record is malformed",
                    "invalid", range_id=raw.get("range_id") if isinstance(raw, dict) else None,
                    **_attributed(location, repo),
                )
            continue
        matched_file = file_by_key.get(_file_key(raw))
        errors = _safe_validation(validate_primary_with_file, raw, matched_file)
        for error in errors:
            _diagnostic(
                diagnostics, "E_LEDGER_SCHEMA", error, "invalid",
                **_attributed(location, repo),
            )
        if errors:
            continue
        validated_primaries.append((raw, location))

    needed_blob_repositories = {
        reference.get("repo")
        for raw, _location in validated_primaries
        for reference in [raw, *(raw.get("counterparts") if isinstance(raw.get("counterparts"), list) else [])]
        if isinstance(reference, dict) and isinstance(reference.get("repo"), str)
        and reference.get("repo") in repository_paths
    }
    for repo_name in needed_blob_repositories:
        if repo_name not in heads:
            continue
        try:
            blobs[repo_name] = dict(read_committed_blobs(repository_paths[repo_name], heads[repo_name]))
        except (OSError, ValueError, subprocess.CalledProcessError) as error:
            _diagnostic(
                diagnostics, "E_SOURCE_FILESET", f"cannot read committed blobs for range verification: {error}",
                "invalid", repo=repo_name, repository=str(repository_paths[repo_name]),
            )
    primaries: list[dict[str, Any]] = []
    primary_locations: dict[int, dict[str, Any]] = {}
    primary_by_id: dict[str, dict[str, Any]] = {}
    intervals: dict[tuple[Any, Any, Any], list[tuple[int, int]]] = defaultdict(list)
    pending_intervals: dict[tuple[Any, Any, Any], list[tuple[int, int]]] = defaultdict(list)
    final_intervals: dict[tuple[Any, Any, Any], list[tuple[int, int]]] = defaultdict(list)
    in_bounds: dict[str, bool] = {}

    for raw, location in validated_primaries:
        primaries.append(raw)
        primary_locations[id(raw)] = location
        range_id = raw.get("range_id")
        if isinstance(range_id, str) and range_id not in primary_by_id:
            primary_by_id[range_id] = raw

        key = _file_key(raw)
        file_record = file_by_key.get(key)
        start, end = raw.get("start"), raw.get("end")
        bounds_ok = (
            file_record is not None
            and file_record.get("content_kind") == "text"
            and isinstance(start, int) and not isinstance(start, bool)
            and isinstance(end, int) and not isinstance(end, bool)
            and 1 <= start <= end <= file_record.get("line_count", 0)
        )
        if isinstance(range_id, str):
            in_bounds[range_id] = bounds_ok
        if not bounds_ok:
            _diagnostic(
                diagnostics, "E_BOUNDS", "primary range is not within an existing text file",
                "invalid", range_id=range_id, repo=raw.get("repo"), path=raw.get("path"), **location,
            )
        else:
            intervals[key].append((start, end))
            if raw.get("disposition") == "pending":
                pending_intervals[key].append((start, end))
            else:
                final_intervals[key].append((start, end))
            data = blobs.get(raw.get("repo"), {}).get(raw.get("path"))
            actual_lines = split_physical_lines(data) if data is not None else []
            hash_matches = (
                data is not None
                and end <= len(actual_lines)
                and range_sha256(actual_lines, start, end) == raw.get("range_sha256")
            )
            if not hash_matches:
                _diagnostic(
                    diagnostics, "E_STALE_HASH", "primary range hash does not match committed blob bytes",
                    "invalid", range_id=range_id, repo=raw.get("repo"), path=raw.get("path"), **location,
                )

        disposition = raw.get("disposition")
        if disposition == "pending":
            _diagnostic(
                diagnostics, "E_PENDING", "primary range disposition is pending",
                "incomplete", range_id=range_id, repo=raw.get("repo"), path=raw.get("path"), **location,
            )
        risk_tags = raw.get("risk_tags") if isinstance(raw.get("risk_tags"), list) else []
        needs_control = disposition == "parity_gap" or (
            disposition in FINAL_EQUIVALENCES and bool(HIGH_RISK_TAGS.intersection(risk_tags))
        )
        if needs_control and not _nonempty(raw.get("control_evidence")):
            _diagnostic(
                diagnostics, "E_HIGH_RISK_PROOF", "high-risk final or parity gap lacks control_evidence",
                "invalid", range_id=range_id, repo=raw.get("repo"), path=raw.get("path"), **location,
            )
        if disposition == "intentional_divergence" and not _nonempty(raw.get("authority_evidence")):
            _diagnostic(
                diagnostics, "E_AUTHORITY_EVIDENCE", "intentional divergence lacks authority_evidence",
                "invalid", range_id=range_id, repo=raw.get("repo"), path=raw.get("path"), **location,
            )

        counterparts = raw.get("counterparts")
        counterpart_list = counterparts if isinstance(counterparts, list) else []
        if disposition in {"exact_equivalent", "semantic_equivalent"} and not counterpart_list:
            _diagnostic(
                diagnostics, "E_COUNTERPART", "equivalence requires at least one counterpart",
                "invalid", range_id=range_id, repo=raw.get("repo"), path=raw.get("path"), **location,
            )
        for index, counterpart in enumerate(counterpart_list):
            counterpart_ok = isinstance(counterpart, dict)
            target = file_by_key.get(_file_key(counterpart)) if counterpart_ok else None
            cp_start = counterpart.get("start") if counterpart_ok else None
            cp_end = counterpart.get("end") if counterpart_ok else None
            counterpart_ok = (
                counterpart_ok
                and target is not None
                and target.get("content_kind") == "text"
                and isinstance(cp_start, int) and not isinstance(cp_start, bool)
                and isinstance(cp_end, int) and not isinstance(cp_end, bool)
                and 1 <= cp_start <= cp_end <= target.get("line_count", 0)
            )
            if counterpart_ok:
                cp_data = blobs.get(counterpart.get("repo"), {}).get(counterpart.get("path"))
                actual_lines = split_physical_lines(cp_data) if cp_data is not None else []
                counterpart_ok = (
                    cp_data is not None
                    and cp_end <= len(actual_lines)
                    and range_sha256(actual_lines, cp_start, cp_end) == counterpart.get("range_sha256")
                )
            if disposition == "semantic_equivalent" and isinstance(counterpart, dict):
                counterpart_ok = counterpart_ok and counterpart.get("repo") != raw.get("repo")
            if not counterpart_ok:
                _diagnostic(
                    diagnostics, "E_COUNTERPART", "counterpart does not resolve to exact committed text bytes",
                    "invalid", range_id=range_id, counterpart_index=index,
                    repo=raw.get("repo"), path=raw.get("path"), **location,
                )

    finding_records = _load_findings(Path(findings), diagnostics)
    for primary in primaries:
        range_id = primary.get("range_id")
        finding_ids = primary.get("finding_ids") if isinstance(primary.get("finding_ids"), list) else []
        if primary.get("disposition") != "parity_gap" and finding_ids:
            _diagnostic(
                diagnostics, "E_FINDING_LINK", "non-gap primary cites a finding",
                "invalid", range_id=range_id, repo=primary.get("repo"), path=primary.get("path"),
            )
        if primary.get("disposition") == "parity_gap":
            for finding_id in finding_ids:
                finding = finding_records.get(finding_id)
                if finding is None or range_id not in finding["range_ids"]:
                    _diagnostic(
                        diagnostics, "E_FINDING_LINK", "parity-gap finding does not link back to its range",
                        "invalid", range_id=range_id, finding_id=finding_id,
                        repo=primary.get("repo"), path=primary.get("path"),
                    )
    for finding_id, finding in finding_records.items():
        for range_id in finding["range_ids"]:
            primary = primary_by_id.get(range_id)
            if (
                primary is None
                or primary.get("disposition") != "parity_gap"
                or finding_id not in (primary.get("finding_ids") or [])
            ):
                _diagnostic(
                    diagnostics, "E_FINDING_LINK", "finding range does not resolve bidirectionally to a parity gap",
                    "invalid", finding_id=finding_id, range_id=range_id, file=finding["file"],
                    **({"repo": primary["repo"]} if primary is not None else {}),
                )

    review_loaded = _load_directory_jsonl(Path(reviews), "E_REVIEW_SCHEMA", diagnostics)
    review_groups: dict[str, list[tuple[dict[str, Any], dict[str, Any], bool]]] = defaultdict(list)
    review_pairs: set[tuple[Any, Any]] = set()
    for raw, location in review_loaded:
        errors = _safe_validation(validate_review, raw)
        repo = _review_repo(raw, primary_attribution_by_id)
        for error in errors:
            _diagnostic(
                diagnostics, "E_REVIEW_SCHEMA", error, "invalid",
                **_attributed(location, repo),
            )
        if errors or not isinstance(raw, dict):
            continue
        range_id = raw.get("range_id")
        if range_id not in primary_by_id:
            _diagnostic(
                diagnostics, "E_REVIEW_UNKNOWN", "review names an unknown primary range",
                "invalid", range_id=range_id, **_attributed(location, repo),
            )
        pair = (range_id, raw.get("review_revision"))
        if pair in review_pairs:
            _diagnostic(
                diagnostics, "E_REVIEW_CHAIN", "duplicate range/review revision pair",
                "invalid", range_id=range_id, review_revision=raw.get("review_revision"),
                **_attributed(location, repo),
            )
        review_pairs.add(pair)
        if isinstance(range_id, str):
            review_groups[range_id].append((raw, location, True))

    acceptable_range_ids: set[str] = set()
    for range_id, primary in primary_by_id.items():
        group = review_groups.get(range_id, [])
        if not group:
            _diagnostic(
                diagnostics, "E_REVIEW_MISSING", "primary range has no secondary review",
                "incomplete", range_id=range_id, repo=primary.get("repo"), path=primary.get("path"),
            )
            continue
        ordered = sorted(group, key=lambda item: item[0].get("review_revision", 0) if isinstance(item[0].get("review_revision"), int) else 0)
        revisions = [item[0].get("review_revision") for item in ordered]
        chain_ok = revisions == list(range(1, len(ordered) + 1))
        if chain_ok:
            for index, (review, _location, _schema_ok) in enumerate(ordered):
                expected_supersedes = None if index == 0 else index
                if review.get("supersedes") != expected_supersedes:
                    chain_ok = False
                    break
        if not chain_ok:
            _diagnostic(
                diagnostics, "E_REVIEW_CHAIN", "review revisions must be contiguous and supersede the immediate prior revision",
                "invalid", range_id=range_id, repo=primary.get("repo"), path=primary.get("path"),
            )
        latest, latest_location, schema_ok = ordered[-1]
        independent = latest.get("secondary_reviewer") != primary.get("primary_reviewer")
        if not independent:
            _diagnostic(
                diagnostics, "E_SELF_REVIEW", "latest secondary reviewer equals the primary reviewer",
                "invalid", range_id=range_id, repo=primary.get("repo"), path=primary.get("path"), **latest_location,
            )
        required_sources = {_reference_key(_reference_from_primary(primary))}
        counterparts = primary.get("counterparts") if isinstance(primary.get("counterparts"), list) else []
        required_sources.update(_reference_key(item) for item in counterparts if isinstance(item, dict))
        checked = latest.get("source_ranges_checked")
        checked_sources = {
            _reference_key(item) for item in checked if isinstance(item, dict)
        } if isinstance(checked, list) else set()
        sources_ok = required_sources <= checked_sources
        if not sources_ok:
            _diagnostic(
                diagnostics, "E_REVIEW_SOURCES", "latest review omits the primary or a counterpart exact range",
                "invalid", range_id=range_id, repo=primary.get("repo"), path=primary.get("path"), **latest_location,
            )
        verdict_ok = latest.get("verdict") in ACCEPTABLE_REVIEWS
        if not verdict_ok:
            _diagnostic(
                diagnostics, "E_REVIEW_MISSING", "latest review is not upheld",
                "incomplete", range_id=range_id, repo=primary.get("repo"), path=primary.get("path"), **latest_location,
            )
        if chain_ok and independent and sources_ok and verdict_ok and schema_ok and range_id not in duplicate_range_ids:
            acceptable_range_ids.add(range_id)

    upheld_intervals: dict[tuple[Any, Any, Any], list[tuple[int, int]]] = defaultdict(list)
    for range_id in acceptable_range_ids:
        primary = primary_by_id[range_id]
        if in_bounds.get(range_id):
            upheld_intervals[_file_key(primary)].append((primary["start"], primary["end"]))

    repository_summaries: dict[str, dict[str, int]] = {}
    for repo_name in ("ghostty", "rustty"):
        summary = {field: 0 for field in SUMMARY_FIELDS}
        records = [record for record in universe_records if record.get("repo") == repo_name]
        summary["files"] = len(records)
        summary["text_lines"] = sum(
            record.get("line_count", 0)
            for record in records
            if record.get("content_kind") == "text" and isinstance(record.get("line_count"), int)
        )
        for record in records:
            if record.get("content_kind") != "text" or not isinstance(record.get("line_count"), int):
                continue
            key = _file_key(record)
            file_intervals = intervals.get(key, [])
            for previous, current in zip(sorted(file_intervals), sorted(file_intervals)[1:]):
                if current[0] <= previous[1]:
                    _diagnostic(
                        diagnostics, "E_OVERLAP", "primary ranges overlap",
                        "invalid", repo=repo_name, path=record.get("path"),
                        previous_start=previous[0], previous_end=previous[1],
                        start=current[0], end=current[1],
                    )
            uncovered = _uncovered_intervals(record["line_count"], file_intervals)
            for start, end in uncovered:
                _diagnostic(
                    diagnostics, "E_UNCOVERED", "text interval has no primary ledger coverage",
                    "incomplete", repo=repo_name, path=record.get("path"), start=start, end=end,
                )
            summary["covered_lines"] += _union_length(file_intervals)
            summary["uncovered_lines"] += sum(end - start + 1 for start, end in uncovered)
            summary["pending_lines"] += _union_length(pending_intervals.get(key, []))
            summary["primary_final_lines"] += _union_length(final_intervals.get(key, []))
            summary["secondary_upheld_lines"] += _union_length(upheld_intervals.get(key, []))
        repository_summaries[repo_name] = summary

    for repo_name, summary in repository_summaries.items():
        summary["invalid_diagnostics"] = sum(
            item["_classification"] == "invalid" and item.get("repo") == repo_name
            for item in diagnostics
        )
        summary["incomplete_diagnostics"] = sum(
            item["_classification"] == "incomplete" and item.get("repo") == repo_name
            for item in diagnostics
        )

    unattributed = {
        "invalid_diagnostics": sum(
            item["_classification"] == "invalid" and item.get("repo") not in repository_summaries
            for item in diagnostics
        ),
        "incomplete_diagnostics": sum(
            item["_classification"] == "incomplete" and item.get("repo") not in repository_summaries
            for item in diagnostics
        ),
    }
    total = {
        field: sum(repository_summaries[repo][field] for repo in repository_summaries)
        for field in SUMMARY_FIELDS[:-2]
    }
    for field in ("invalid_diagnostics", "incomplete_diagnostics"):
        total[field] = sum(summary[field] for summary in repository_summaries.values()) + unattributed[field]
    invalid_count = total["invalid_diagnostics"]
    incomplete_count = total["incomplete_diagnostics"]
    status = "INVALID" if invalid_count else "INCOMPLETE" if incomplete_count else "COMPLETE"
    exit_code = 1 if invalid_count else 2 if incomplete_count else 0
    public_diagnostics = [
        {key: value for key, value in item.items() if key != "_classification"}
        for item in diagnostics
    ]
    summary_object = {
        "type": "summary", "status": status, "repositories": repository_summaries,
        "unattributed": unattributed, "total": total,
    }
    return VerificationResult(exit_code=exit_code, diagnostics=public_diagnostics, summary=summary_object)


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--universe", type=Path, required=True)
    parser.add_argument("--ledger", type=Path, required=True)
    parser.add_argument("--reviews", type=Path, required=True)
    parser.add_argument("--findings", type=Path, required=True)
    parser.add_argument("--rustty", type=Path, required=True)
    parser.add_argument("--ghostty", type=Path, required=True)
    parser.add_argument("--allow-incomplete", action="store_true")
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    arguments = _parser().parse_args(argv)
    result = verify(
        universe=arguments.universe,
        ledger=arguments.ledger,
        reviews=arguments.reviews,
        findings=arguments.findings,
        rustty=arguments.rustty,
        ghostty=arguments.ghostty,
    )
    uncovered_emitted = 0
    for diagnostic in result.diagnostics:
        if diagnostic["code"] == "E_UNCOVERED":
            if uncovered_emitted >= 100:
                continue
            uncovered_emitted += 1
        print(json.dumps(diagnostic, ensure_ascii=False, separators=(",", ":"), sort_keys=True))
    print(json.dumps(result.summary, ensure_ascii=False, separators=(",", ":"), sort_keys=True))
    return result.exit_code


if __name__ == "__main__":
    sys.exit(main())
