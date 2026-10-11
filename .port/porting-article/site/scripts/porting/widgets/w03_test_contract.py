"""Data builder for "The test contract" widget (slug test-contract).

Ports the data step of the mockup's assemble script (mockup-build/w03_assemble.py)
onto the persisted research files (the first five at the current snapshot, ``snapshot.dated``):

- research/data/mock-test-contract-rust-per-module-daily.csv  Rust #[test] per module, one revision per day
- research/data/mock-test-contract-rust-per-crate-daily.csv   the same revisions, per crate
- research/data/mock-test-contract-zig-per-module-per-pin.csv Zig tests per module at the 9 reference commits
- research/data/mock-test-contract-pins.csv                   mapped and src/ totals per reference commit
- research/data/strategy-oracle-pin-advances.csv              upstream non-merge commits per sync
- research/data/mock-test-contract-silent-skips-185.csv       the 185 silent-skip sites before the fix
- research/data/r2-skip-summary.csv                           Rust and Zig skip-mechanism census
- mockup-build/w03_names.json                                 crate of each module (from the gate's map)

The two Zig packages libvaxis and zf were pinned at older versions by the first
reference commit; the assemble script recounted them from those packages, which
are not persisted, so the recounted values are carried here as constants.
"""
import csv, json
from pathlib import Path

import jargon

from .snapshot import dated

SLUG = "test-contract"

# BASELINE_PKG_TESTS are the Zig test counts of libvaxis 0.5.1 and zf 0.10.3, the
# package versions build.zig.zon pinned at the first reference commit (4749c4e93),
# as recounted by w03_assemble.py; from the first sync on the CSV values apply.
BASELINE_PKG_TESTS = {"vaxis": 87, "zf": 8}

# MIRI_AND_PLATFORM_IGNORE is the number of Rust tests that carry both a Miri
# cfg_attr ignore and a platform/feature one (research/r2-data-fixes.md, fact-check
# of BRIEF row 92): they are in the 475 Miri ignores, and 136 + 16 = 152 tests carry
# a platform/feature ignore. r2-skip-summary.csv does not list it.
MIRI_AND_PLATFORM_IGNORE = 16


def _read_csv(path: Path) -> list:
    """Return the rows of a CSV file as dicts."""
    with open(path, newline="") as f:
        return list(csv.DictReader(f))


def _clean(text: str, changes: list) -> str:
    """Return text with internal ids and process labels stripped, recording each change."""
    out, changed = jargon.strip_internal_ids(text)
    if changed:
        changes.append((text, out))
    return out


def _change_points(values: list) -> list:
    """Compress a per-day series into [[day, value], …] at each change (day 0 always included)."""
    out, prev = [], None
    for i, v in enumerate(values):
        if v != prev:
            out.append([i, v])
            prev = v
    return out


def _skip_census(rows: list) -> dict:
    """Return the Rust and Zig skip-mechanism counts from r2-skip-summary.csv, keyed by short names."""
    def val(side: str, prefix: str) -> int:
        """Return the integer value of the first row of side whose metric starts with prefix."""
        for r in rows:
            if r["side"] == side and r["metric"].startswith(prefix):
                return int(r["value"])
        raise KeyError(f"{SLUG}: no {side} skip metric starting {prefix!r}")

    return {
        "zig": {
            "total": val("zig", "test blocks in Ghostty src/"),
            "skip": val("zig", "test blocks whose body contains error.SkipZigTest"),
            "build": val("zig", "of which guard is comptime"),
            "runtime": val("zig", "of which guard is runtime"),
        },
        "rust": {
            "total": val("rust", "test fns (#[test] family)"),
            "none": val("rust", "test fns with no skip mechanism"),
            "miri": val("rust", "cfg_attr(<pred> ignore) naming miri"),
            "miriDebugOff": val("rust", "of which all(miri debug_assertions)"),
            "cfgOnly": val("rust", "test fns with a non-test #[cfg(..)]"),
            "cfgIgnore": val("rust", "cfg_attr(<pred> ignore) not naming miri"),
            "ignore": val("rust", "plain #[ignore]"),
            "miriAndCfgIgnore": MIRI_AND_PLATFORM_IGNORE,
        },
    }


def build(research: Path, mockbuild: Path) -> dict:
    """Return the test-contract widget data (see module docstring for sources)."""
    d = Path(research) / "data"
    changes: list = []

    rows = _read_csv(d / dated("mock-test-contract-rust-per-module-daily.csv"))
    crows = _read_csv(d / dated("mock-test-contract-rust-per-crate-daily.csv"))
    zrows = _read_csv(d / dated("mock-test-contract-zig-per-module-per-pin.csv"))
    pin_rows = _read_csv(d / dated("mock-test-contract-pins.csv"))
    advances = {r["to_sha"]: int(r["upstream_non_merge"])
                for r in _read_csv(d / dated("strategy-oracle-pin-advances.csv"))}
    names = json.loads((Path(mockbuild) / "w03_names.json").read_text())

    pin_ids = [p["pin"] for p in pin_rows]
    zig = {r["module"]: [int(r[p]) for p in pin_ids] for r in zrows}
    for mod, n in BASELINE_PKG_TESTS.items():
        zig[mod][0] = n

    mods = [{"m": _clean(m, changes), "c": _clean(names[m]["crate"], changes), "z": z,
             "r": _change_points([int(r[m]) for r in rows])}
            for m, z in zig.items()]

    pins = [{"date": p["advanced"], "sha": p["pin"],
             "mapped": sum(z[k] for z in zig.values()),
             "src": int(p["zig_src_test_blocks_regex"]),
             "upstream": advances.get(p["pin"], 0)}
            for k, p in enumerate(pin_rows)]

    crate_cols = [k for k in crows[0] if k not in ("date", "rev")]
    rust = {
        "all": [sum(int(c[k]) for k in crate_cols) for c in crows],
        "vt": [int(c["rustty-vt"]) for c in crows],
        "vtc": [int(c["rustty-vt-c"]) for c in crows],
    }

    skips = _read_csv(d / "mock-test-contract-silent-skips-185.csv")
    # Guard classes are shown as bar labels; "oracle" is the port's word for the reference build.
    cls_names = {"oracle / corpus / git checkout": "reference build / corpus / git checkout"}
    by_cls: dict = {}
    by_kind: dict = {}
    by_crate: dict = {}
    grp: dict = {}
    for s in skips:
        name = cls_names.get(s["guard_class"], s["guard_class"])
        by_cls[name] = by_cls.get(name, 0) + 1
        by_kind[s["lint_kind"]] = by_kind.get(s["lint_kind"], 0) + 1
        crate = s["file"].split("/")[1]
        by_crate[crate] = by_crate.get(crate, 0) + 1
        grp[s["guard_group"]] = grp.get(s["guard_group"], 0) + 1
    skip = {
        "byCls": by_cls,
        "byKind": by_kind,
        "byCrate": [[k, n] for k, n in sorted(by_crate.items(), key=lambda kv: -kv[1])],
        "grp": grp,
        "zigPort": sum(int(s["body_mentions_SkipZigTest"]) for s in skips),
        **_skip_census(_read_csv(d / "r2-skip-summary.csv")),
    }

    _record_review(changes)
    return {
        "_meta": {
            "slug": SLUG,
            "generatedFrom": [
                f"research/data/{dated('mock-test-contract-rust-per-module-daily.csv')}",
                f"research/data/{dated('mock-test-contract-rust-per-crate-daily.csv')}",
                f"research/data/{dated('mock-test-contract-zig-per-module-per-pin.csv')}",
                f"research/data/{dated('mock-test-contract-pins.csv')}",
                f"research/data/{dated('strategy-oracle-pin-advances.csv')}",
                "research/data/mock-test-contract-silent-skips-185.csv",
                "research/data/r2-skip-summary.csv",
                "research/r2-data-fixes.md (16 tests with both a Miri and a platform ignore)",
                "mockup-build/w03_names.json (crate per module)",
            ],
            "asOf": rows[-1]["rev"],
        },
        "dates": [r["date"] for r in rows],
        "revs": [r["rev"] for r in rows],
        "rust": rust,
        "pins": pins,
        "mods": mods,
        "skip": skip,
    }


def _record_review(changes: list) -> None:
    """Append each (before, after) rewrite to jargon-review.txt once, for the hand review."""
    path = Path(__file__).resolve().parent.parent / "jargon-review.txt"
    existing = set(path.read_text().splitlines()) if path.exists() else set()
    lines = [f"{SLUG}\t{b}\t{a}" for b, a in dict.fromkeys(changes)]
    new = [ln for ln in lines if ln not in existing]
    if new:
        with open(path, "a") as f:
            f.write("".join(ln + "\n" for ln in new))
