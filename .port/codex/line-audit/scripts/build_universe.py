"""Build the pinned committed-file universe for the line audit."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
from typing import Sequence

from model import split_physical_lines, validate_file


_REPO_ORDER = {"ghostty": 0, "rustty": 1}
_BINARY_REASON = "binary content has no physical text lines"
_BINARY_EVIDENCE = "content classification: NUL prefix or invalid UTF-8"


def classify_content(data: bytes) -> str:
    """Classify bytes using the audit's intentionally conservative rules."""
    if not data:
        return "empty"
    if b"\0" in data[:8192]:
        return "binary"
    try:
        data.decode("utf-8")
    except UnicodeDecodeError:
        return "binary"
    return "text"


def _git_bytes(repo: Path, *args: str) -> bytes:
    return subprocess.run(
        ["git", "-C", os.fspath(repo), *args],
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    ).stdout


def _decode_git_path(path: bytes) -> str:
    try:
        return path.decode("utf-8")
    except UnicodeDecodeError as error:
        raise ValueError("Git path is not valid UTF-8") from error


def read_committed_blobs(repo: Path, revision: str) -> list[tuple[str, bytes]]:
    """Return each recursive tree blob and exact blob bytes in Git path order."""
    tree = _git_bytes(repo, "ls-tree", "-r", "-z", "--full-tree", revision)
    blobs: list[tuple[str, bytes]] = []
    for entry in tree.split(b"\0"):
        if not entry:
            continue
        metadata, separator, path_bytes = entry.partition(b"\t")
        if not separator:
            raise ValueError("malformed git ls-tree entry")
        fields = metadata.split(b" ")
        if len(fields) != 3:
            raise ValueError("malformed git ls-tree metadata")
        _mode, entry_type, object_id = fields
        if entry_type != b"blob":
            raise ValueError(f"non-blob committed tree entry: {entry_type.decode('ascii', 'replace')}")
        path = _decode_git_path(path_bytes)
        blob = _git_bytes(repo, "cat-file", "blob", object_id.decode("ascii"))
        blobs.append((path, blob))
    return blobs


def _resolve_head(repo: Path) -> str:
    return _git_bytes(repo, "rev-parse", "--verify", "HEAD^{commit}").decode("ascii").strip()


def build_records(
    repo: Path,
    repo_name: str,
    expected_revision: str | None = None,
) -> list[dict]:
    """Inventory a repository's pinned ``HEAD`` and validate every file record."""
    if repo_name not in _REPO_ORDER:
        raise ValueError("repo_name must be exactly 'ghostty' or 'rustty'")
    revision = _resolve_head(repo)
    if expected_revision is not None and revision != expected_revision:
        raise ValueError(f"expected revision {expected_revision}, found {revision}")

    records: list[dict] = []
    for path, data in read_committed_blobs(repo, revision):
        content_kind = classify_content(data)
        binary = content_kind == "binary"
        record = {
            "repo": repo_name,
            "revision": revision,
            "path": path,
            "byte_length": len(data),
            "line_count": 0 if binary or content_kind == "empty" else len(split_physical_lines(data)),
            "sha256": hashlib.sha256(data).hexdigest(),
            "content_kind": content_kind,
            "scope_class": "binary_asset" if binary else "semantic_required",
            "classification_reason": _BINARY_REASON if binary else "",
            "classification_evidence": _BINARY_EVIDENCE if binary else "",
        }
        diagnostics = validate_file(record)
        if diagnostics:
            raise ValueError("invalid file record: " + "; ".join(diagnostics))
        records.append(record)
    return sorted(records, key=lambda record: record["path"].encode("utf-8"))


def _record_order(record: dict) -> tuple[int, bytes]:
    try:
        return _REPO_ORDER[record["repo"]], record["path"].encode("utf-8")
    except (KeyError, AttributeError, UnicodeEncodeError) as error:
        raise ValueError("invalid file record: cannot determine repository/path order") from error


def write_jsonl(records: list[dict], output: Path) -> None:
    """Validate and atomically write compact, stably ordered JSON Lines."""
    for record in records:
        diagnostics = validate_file(record)
        if diagnostics:
            raise ValueError("invalid file record: " + "; ".join(diagnostics))
    ordered = sorted(records, key=_record_order)
    output = Path(output)
    temporary_name: str | None = None
    try:
        with tempfile.NamedTemporaryFile(
            mode="w",
            encoding="utf-8",
            newline="\n",
            dir=output.parent,
            prefix=f".{output.name}.",
            suffix=".tmp",
            delete=False,
        ) as temporary:
            temporary_name = temporary.name
            for record in ordered:
                temporary.write(json.dumps(record, ensure_ascii=False, separators=(",", ":"), sort_keys=True))
                temporary.write("\n")
        os.replace(temporary_name, output)
        temporary_name = None
    finally:
        if temporary_name is not None:
            Path(temporary_name).unlink(missing_ok=True)


def _argument_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--rustty", type=Path, required=True)
    parser.add_argument("--ghostty", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--expect-rustty-revision")
    parser.add_argument("--expect-ghostty-revision")
    return parser


def main(argv: Sequence[str] | None = None) -> None:
    """Run the two-repository committed-tree inventory CLI."""
    parser = _argument_parser()
    arguments = parser.parse_args(argv)
    try:
        records = build_records(
            arguments.ghostty, "ghostty", arguments.expect_ghostty_revision
        ) + build_records(
            arguments.rustty, "rustty", arguments.expect_rustty_revision
        )
        write_jsonl(records, arguments.output)
    except (OSError, ValueError, subprocess.CalledProcessError) as error:
        parser.error(str(error))


if __name__ == "__main__":
    main()
