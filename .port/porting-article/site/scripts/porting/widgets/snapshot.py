"""The article's data snapshot: which research datasets are the current ones.

The research directory keeps every snapshot side by side. A dataset regenerated for a later
snapshot is a new file whose name carries the snapshot date (``<name>-2026-10-10.csv``); the
undated file is the earlier 2026-10-08 snapshot. Builders name the current file through
``dated()`` so one constant moves them all; there is no silent fallback to the undated file.
"""

# SNAPSHOT is the date suffix of the current research datasets.
SNAPSHOT = "2026-10-10"

# RUSTTY_HEAD and GHOSTTY_REF are the Rustty commit and the Ghostty reference commit of the snapshot.
RUSTTY_HEAD = "58aa62a81"
GHOSTTY_REF = "246f70287"


def dated(name: str) -> str:
    """Return the current snapshot's file name for an undated research dataset name (``a.csv`` -> ``a-2026-10-10.csv``)."""
    stem, dot, ext = name.rpartition(".")
    if not dot:
        raise ValueError(f"dataset name without extension: {name!r}")
    return f"{stem}-{SNAPSHOT}.{ext}"
