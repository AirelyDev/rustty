"""Whole-run checks: every derived asset changed since the pre-rebrand rev; reruns are byte-stable."""
from __future__ import annotations

import argparse
import hashlib
from pathlib import Path

from . import alternates, flats, mascot, official, paths, tools


def run_all(args) -> int:
    for step in (mascot.run_mascot, official.run, flats.run,
                 lambda a: alternates.run(argparse.Namespace(names=[]))):
        rc = step(args)
        if rc != 0:
            return rc
    return 0


def outputs() -> list[Path]:
    out = [paths.ICON_MASCOT, paths.CUSTOM_MASCOT]
    out += [paths.alt_output(n) for n in paths.ALT_NAMES]
    out += [paths.APPICON_IMAGESET / n for n in paths.APPICON_FILES.values()]
    out += [paths.GNOME_DIR / f"{s}.png" for s in paths.GNOME_SIZES]
    out += [paths.APPRT_GNOME_DIR / f"{s}.png" for s in paths.GNOME_SIZES]
    out += [paths.ICONS_DIR / n for n in paths.ICONS_MAP]
    out += [paths.WIN_ICO, paths.FAVICON]
    return out


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def snapshot() -> dict[str, str]:
    return {paths.rel(p): _sha(p.read_bytes()) for p in outputs() + [paths.ICON_JSON]}


def diff_snapshots(a: dict[str, str], b: dict[str, str]) -> list[str]:
    """Sorted paths whose hash differs between two snapshots (or exist in only one)."""
    return sorted(k for k in a.keys() | b.keys() if a.get(k) != b.get(k))


def no_ghost_left() -> list[str]:
    bad = []
    for p in outputs() + [paths.ICON_JSON]:
        if _sha(p.read_bytes()) == _sha(tools.git_blob(paths.PRE_REBRAND_REV, paths.rel(p))):
            bad.append(paths.rel(p))
    return bad


def run(args) -> int:
    ok = True
    bad = no_ghost_left()
    print(f"no-ghost: {'OK' if not bad else 'UNCHANGED: ' + ', '.join(bad)}")
    ok &= not bad
    committed = snapshot()
    if run_all(args) != 0:
        return 1
    first = snapshot()
    changed = diff_snapshots(committed, first)
    print(f"in-sync with committed: {'OK' if not changed else 'CHANGED: ' + ', '.join(changed)}")
    ok &= not changed
    if run_all(args) != 0:
        return 1
    second = snapshot()
    drift = diff_snapshots(first, second)
    print(f"determinism: {'OK' if not drift else 'DRIFT: ' + ', '.join(drift)}")
    ok &= not drift
    return 0 if ok else 1
