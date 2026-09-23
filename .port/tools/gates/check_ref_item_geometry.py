#!/usr/bin/env python3
"""G-REFGEOM — the reference's set-item geometry against the checked-in golden.

WHY (NEW-FINDINGS N88, N91). The port lays its style and hyperlink sets out at
the REFERENCE's `RefCountedSet.Item` size and alignment, not its own, because
that item array decides `Page.layout` and so the rows of every page. The
reference geometry is derived in the port from `#[repr(C)]` shadows of the
reference's declarations (`style.rs`, `hyperlink.rs`). Nothing in the port can
see the reference change: if upstream edits `Style`, `Style.Color`,
`color.RGB`, `Style.Flags`, `hyperlink.PageEntry`, `Offset(T).Slice` or
`RefCountedSet.Item`, the shadows go stale silently. This gate re-measures the
reference and compares it with `fixtures/ref_item_geometry_golden.txt`, the
same file `ref_geometry_golden_tests` checks the port's shadows, items and
strides against. So the reference and the port are pinned to one file, and a
pin move that changes the geometry turns this gate RED.

HOW. `fixtures/ref_item_geometry_probe.zig` is appended to a SCRATCH COPY of
the pinned oracle's `src/terminal/page.zig` (the oracle itself is read-only,
R3). The probe reports `@sizeOf` / `@alignOf` through `@compileError`, so
`zig build test-lib-vt-build -Dtarget=<t>` reports them for each target
without running anything: aarch64-macos (the macOS arm64 host's native
target), aarch64-linux-gnu and wasm32-wasi. The copy is deleted afterwards.

ORACLE. Resolved the way `check_log_reconcile.py` resolves it: the checkout
beside the rustty root (`<root>/../ghostty`; a worktree under
`.claude/worktrees/` sees it through the `ghostty` symlink there), and its
HEAD must be `ORACLE_PIN`. (There is no shared resolver or pin file on this
branch: `_gatelib.py` has none, and `_oracle_root.sh` / `ORACLE_PIN` do not
exist yet.)

Exit contract (`.port/tools/CI.md`): 0 GREEN, 1 RED (a difference, or the
probe no longer compiles against the pinned reference), 3 BLOCKED (no Zig, no
oracle, oracle not at the pin or modified).

Usage:
  .port/tools/gates/check_ref_item_geometry.py
  .port/tools/gates/check_ref_item_geometry.py --print   # print the measured lines
"""
import os
import shutil
import subprocess
import sys
import tempfile

GATES_DIR = os.path.dirname(os.path.abspath(__file__))
PORT_DIR = os.path.dirname(os.path.dirname(GATES_DIR))
ROOT = os.path.dirname(PORT_DIR)
PROBE = os.path.join(GATES_DIR, "fixtures", "ref_item_geometry_probe.zig")
GOLDEN = os.path.join(GATES_DIR, "fixtures", "ref_item_geometry_golden.txt")

# Same location and pin as `check_log_reconcile.py` (R3: external, read-only).
ORACLE = os.path.join(os.path.dirname(ROOT), "ghostty")
ORACLE_PIN = "20abdb50a6216c450d6d4d010c41c7edf5ab15b2"

TARGETS = ["aarch64-macos", "aarch64-linux-gnu", "wasm32-wasi"]
BEGIN = "RUSTTY-REF-ITEM-GEOMETRY-PROBE BEGIN"
END = "RUSTTY-REF-ITEM-GEOMETRY-PROBE END"

# Not copied into the scratch copy: VCS data, build caches and outputs, and the
# multi-GB `example/` tree, none of which the lib-vt test build reads.
SKIP = {".git", ".zig-cache", "zig-cache", "zig-out", "example"}


def blocked(msg):
    print(f"G-REFGEOM BLOCKED: {msg}")
    return 3


def oracle_head():
    head = os.path.join(ORACLE, ".git", "HEAD")
    try:
        with open(head, encoding="utf-8") as fh:
            ref = fh.read().strip()
        if ref.startswith("ref: "):
            with open(os.path.join(ORACLE, ".git", ref[5:]), encoding="utf-8") as fh:
                ref = fh.read().strip()
        return ref
    except OSError:
        return ""


def oracle_modified():
    """The oracle's modified or untracked files, or None if git is absent."""
    if shutil.which("git") is None:
        return None
    r = subprocess.run(
        ["git", "-C", ORACLE, "status", "--porcelain", "--untracked-files=no"],
        capture_output=True,
        text=True,
    )
    if r.returncode != 0:
        return None
    return [l for l in r.stdout.splitlines() if l.strip()]


def copy_oracle(dst):
    # APFS clones (`cp -c`) make the copy nearly free on macOS; elsewhere a
    # plain recursive copy.
    clone = sys.platform == "darwin"
    for entry in sorted(os.listdir(ORACLE)):
        if entry in SKIP:
            continue
        cmd = ["cp", "-cR" if clone else "-R", os.path.join(ORACLE, entry), dst + "/"]
        subprocess.run(cmd, check=True)
    os.makedirs(os.path.join(dst, "example"), exist_ok=True)


def parse_block(log):
    """The probe lines of the FIRST report in `log`; every later report of the
    same run (the build summary repeats the error) must be identical."""
    blocks = []
    start = 0
    while True:
        i = log.find(BEGIN, start)
        if i < 0:
            break
        j = log.find(END, i)
        if j < 0:
            break
        lines = [l.strip() for l in log[i + len(BEGIN) : j].splitlines()]
        blocks.append([l for l in lines if l])
        start = j + len(END)
    if not blocks:
        return None, "no probe report in the build output"
    for b in blocks[1:]:
        if b != blocks[0]:
            return None, "the build output holds two different probe reports"
    return blocks[0], None


def measure(copy):
    measured = []
    for target in TARGETS:
        print(f"  probing {target} ...", flush=True)
        r = subprocess.run(
            ["zig", "build", "test-lib-vt-build", f"-Dtarget={target}"],
            cwd=copy,
            capture_output=True,
            text=True,
            errors="replace",
        )
        log = r.stdout + r.stderr
        lines, err = parse_block(log)
        if lines is None:
            tail = "\n".join(log.splitlines()[-25:])
            return None, f"{target}: {err} (exit {r.returncode}); build output tail:\n{tail}"
        measured.extend(lines)
    return measured, None


def load_golden():
    with open(GOLDEN, encoding="utf-8") as fh:
        return [
            l.strip()
            for l in fh
            if l.strip() and not l.lstrip().startswith("#")
        ]


def main():
    if shutil.which("zig") is None:
        return blocked("zig is not installed (the probe needs the reference's own toolchain)")
    if not os.path.isfile(os.path.join(ORACLE, "src", "terminal", "page.zig")):
        return blocked(f"the read-only ghostty oracle checkout is absent at {ORACLE}")
    head = oracle_head()
    if head != ORACLE_PIN:
        return blocked(f"the oracle is at {head or '<unknown>'}, not the pin {ORACLE_PIN}")
    modified = oracle_modified()
    if modified:
        return blocked(
            "the oracle has modified files, so it is not the pinned reference:\n  "
            + "\n  ".join(modified[:10])
        )
    for path in (PROBE, GOLDEN):
        if not os.path.isfile(path):
            return blocked(f"missing {path}")

    zv = subprocess.run(["zig", "version"], capture_output=True, text=True).stdout.strip()
    print(f"=== G-REFGEOM :: reference set-item geometry (oracle {ORACLE_PIN[:9]}, zig {zv}) ===")

    work = tempfile.mkdtemp(prefix="rustty-refgeom-")
    try:
        copy = os.path.join(work, "ghostty")
        os.makedirs(copy)
        copy_oracle(copy)
        page = os.path.join(copy, "src", "terminal", "page.zig")
        with open(PROBE, encoding="utf-8") as fh:
            probe = fh.read()
        with open(page, "a", encoding="utf-8") as fh:
            fh.write("\n" + probe)
        measured, err = measure(copy)
    finally:
        shutil.rmtree(work, ignore_errors=True)

    if measured is None:
        print(f"G-REFGEOM RED: the probe did not report against the pinned reference.\n{err}")
        return 1
    if "--print" in sys.argv:
        print("\n".join(measured))

    golden = load_golden()
    if measured == golden:
        print(f"G-REFGEOM GREEN: {len(measured)} lines, {len(TARGETS)} targets, equal to the golden")
        return 0

    print("G-REFGEOM RED: the reference's geometry differs from the golden")
    print(f"  golden: {os.path.relpath(GOLDEN, ROOT)}")
    width = max(len(l) for l in golden + measured)
    for k in range(max(len(golden), len(measured))):
        g = golden[k] if k < len(golden) else "<none>"
        m = measured[k] if k < len(measured) else "<none>"
        mark = "  " if g == m else "!="
        print(f"  {mark} golden {g:<{width}}  reference {m}")
    print(
        "  An upstream change moved the item geometry the port's page layout is tied to\n"
        "  (NEW-FINDINGS N88): update the shadows in style.rs / hyperlink.rs, then the\n"
        "  golden, then re-run ref_geometry_golden_tests."
    )
    return 1


if __name__ == "__main__":
    sys.exit(main())
