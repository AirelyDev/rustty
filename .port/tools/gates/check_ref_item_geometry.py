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

ORACLE. The shared resolver, `_gatelib.oracle_root`: the `ghostty` checkout
beside the MAIN checkout, correct from a git worktree with no stopgap symlink
(R3: external and read-only). Nothing here is env-settable (R5). The pin is
the one checked-in file `.port/tools/gates/ORACLE_PIN`, read by
`_gatelib.oracle_pin`; a missing or malformed pin file is BLOCKED. Every git
call on the oracle goes through `_gatelib.oracle_git`, which strips `GIT_*`,
so a `GIT_DIR` decoy cannot answer for it. The oracle must be the top level of
its own repository, its HEAD must equal `ORACLE_PIN`, and it must have no
modified tracked files. A git that is missing or exits non-zero is BLOCKED,
never read as "clean".

Exit contract (`.port/tools/CI.md`): 0 GREEN, 1 RED (a difference, or the
probe no longer compiles against the pinned reference), 3 BLOCKED (no Zig, no
oracle, no usable pin file, the oracle off the pin, modified or not a git
repository, or git unavailable).

`--self-test` runs a COPY of this gate end to end in a throwaway main checkout
with a linked worktree, a fixture oracle and a stand-in `zig`, and requires
BLOCKED for an absent oracle, an off-pin HEAD, a missing or malformed pin
file, a dirty oracle, a failing `git status` at the pin (a corrupt index), an
oracle that git answers from an enclosing repository at the pin, an oracle
that is not a repository and a missing git;
GREEN at the pin (also from a worktree without the stopgap symlink, and under
a `GIT_DIR` + `GIT_WORK_TREE` decoy with a decoy cwd); and RED on a measured
difference.

Usage:
  .port/tools/gates/check_ref_item_geometry.py
  .port/tools/gates/check_ref_item_geometry.py --print      # print the measured lines
  .port/tools/gates/check_ref_item_geometry.py --self-test  # the oracle arms, on fixtures
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

# The shared resolver and the ONE pin file (`.port/ORACLE_SYNC.md`). ROOT is
# script-relative, so neither the cwd nor `GIT_DIR` can move the oracle.
sys.path.insert(0, GATES_DIR)
import _gatelib as G  # noqa: E402

ORACLE = G.oracle_root(ROOT)
ORACLE_PIN = G.oracle_pin()

TARGETS = ["aarch64-macos", "aarch64-linux-gnu", "wasm32-wasi"]
BEGIN = "RUSTTY-REF-ITEM-GEOMETRY-PROBE BEGIN"
END = "RUSTTY-REF-ITEM-GEOMETRY-PROBE END"

# Not copied into the scratch copy: VCS data, build caches and outputs, and the
# multi-GB `example/` tree, none of which the lib-vt test build reads.
SKIP = {".git", ".zig-cache", "zig-cache", "zig-out", "example"}


def blocked(msg):
    print(f"G-REFGEOM BLOCKED: {msg}")
    return 3


def oracle_check():
    """None when the oracle is the pinned reference, else the BLOCKED reason.

    Every git read goes through `G.oracle_git` (`GIT_*` stripped). A git that
    cannot run or exits non-zero is BLOCKED: it proves nothing about the tree.
    `--show-toplevel` rejects a directory that is not a repository of its own,
    which git would otherwise answer from an enclosing repository."""
    if ORACLE_PIN is None:
        return (f"the oracle pin file {G.ORACLE_PIN_FILE} is missing or not a "
                "40-hex commit, so the oracle's commit cannot be checked")
    if not os.path.isfile(os.path.join(ORACLE, "src", "terminal", "page.zig")):
        return f"the read-only ghostty oracle checkout is absent at {ORACLE}"
    try:
        r = G.oracle_git(ORACLE, "rev-parse", "--show-toplevel", "HEAD", text=True)
    except OSError as exc:
        return f"git could not run on the oracle at {ORACLE} ({exc})"
    if r.returncode != 0:
        return (f"the oracle at {ORACLE} is not a git repository "
                f"(rev-parse exit {r.returncode}: {r.stderr.strip()})")
    out = r.stdout.split()
    if len(out) != 2:
        return f"rev-parse gave an unexpected answer for the oracle: {r.stdout!r}"
    top, head = out
    if os.path.realpath(top) != os.path.realpath(ORACLE):
        return (f"the oracle at {ORACLE} is not the top level of its own "
                f"repository (git answered from {top})")
    if head != ORACLE_PIN:
        return f"the oracle is at {head}, not the pin {ORACLE_PIN}"
    try:
        r = G.oracle_git(ORACLE, "status", "--porcelain", "--untracked-files=no", text=True)
    except OSError as exc:
        return f"git could not run on the oracle at {ORACLE} ({exc})"
    if r.returncode != 0:
        return (f"status failed on the oracle at {ORACLE} "
                f"(exit {r.returncode}: {r.stderr.strip()})")
    modified = [l for l in r.stdout.splitlines() if l.strip()]
    if modified:
        return ("the oracle has modified files, so it is not the pinned reference:\n  "
                + "\n  ".join(modified[:10]))
    return None


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


# ---------------------------------------------------------------------------
# Self-test: the oracle arms, run on a COPY of this gate in hermetic fixtures
# ---------------------------------------------------------------------------
# The stand-in `zig` answers `version`, and for `build ... -Dtarget=<t>` prints
# one probe line for <t> between the markers, as the real probe's compile
# error does. `$REFGEOM_SELFTEST_SIZE` (read by the stand-in only, never by the
# gate) changes the size it reports, for the RED arm.
FAKE_ZIG = """#!/bin/sh
case "$1" in version) echo 0.16.0-selftest; exit 0;; esac
t=
for a in "$@"; do case "$a" in -Dtarget=*) t="${a#-Dtarget=}";; esac; done
echo "error: %s"
echo "$t selftest.Item ${REFGEOM_SELFTEST_SIZE:-4} 4"
echo "%s"
exit 1
"""
FIXTURE_GOLDEN = "# self-test golden\n" + "".join(
    f"{t} selftest.Item 4 4\n" for t in TARGETS)


def self_test():
    """Build a throwaway main checkout `<tmp>/rustty` with a linked worktree
    at `.claude/worktrees/wt` (the agent layout) and a fixture oracle at
    `<tmp>/ghostty`, copy this gate, `_gatelib.py`, a fixture `ORACLE_PIN`
    naming the fixture oracle's commit, a fixture golden and the probe into
    both checkouts, and RUN THE COPIES. The fixture is fresh, so the
    worktree-sibling path (`.claude/worktrees/ghostty`) cannot exist; the first
    arm asserts that, so no stopgap symlink can make the worktree arms pass."""
    import tempfile

    print("=== G-REFGEOM SELF-TEST ===")
    ok = True

    def case(name, good, detail=""):
        nonlocal ok
        if not good:
            ok = False
        print(f"  [{'ok ' if good else 'FAIL'}] {name}"
              + (f"\n         ({detail})" if detail and not good else ""))

    env = G.git_env()
    git_cfg = ["-c", "user.name=selftest", "-c", "user.email=selftest@invalid",
               "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null",
               "-c", "init.defaultBranch=main"]

    def git(where, *args):
        return subprocess.run(["git", *git_cfg, *args], cwd=where, env=env,
                              capture_output=True, text=True, check=True).stdout.strip()

    base = os.path.realpath(tempfile.mkdtemp(prefix="grefgeom_selftest_"))
    try:
        main_co = os.path.join(base, "rustty")
        wt = os.path.join(main_co, ".claude", "worktrees", "wt")
        oracle = os.path.join(base, "ghostty")
        stale = os.path.join(main_co, ".claude", "worktrees", "ghostty")
        fakebin = os.path.join(base, "fakebin")
        os.makedirs(main_co)
        git(main_co, "init", "-q")
        git(main_co, "commit", "-q", "--allow-empty", "-m", "fixture")
        git(main_co, "worktree", "add", "-q", "--detach", wt)

        # The fixture oracle: a repository of its own with a tracked page.zig.
        os.makedirs(os.path.join(oracle, "src", "terminal"))
        with open(os.path.join(oracle, "src", "terminal", "page.zig"), "w") as fh:
            fh.write("// fixture\n")
        git(oracle, "init", "-q")
        git(oracle, "add", "-A")
        git(oracle, "commit", "-q", "-m", "pinned")
        pin = git(oracle, "rev-parse", "HEAD")

        os.makedirs(fakebin)
        with open(os.path.join(fakebin, "zig"), "w") as fh:
            fh.write(FAKE_ZIG % (BEGIN, END))
        os.chmod(os.path.join(fakebin, "zig"), 0o755)

        def install(checkout):
            gates = os.path.join(checkout, ".port", "tools", "gates")
            os.makedirs(os.path.join(gates, "fixtures"))
            for f in ("check_ref_item_geometry.py", "_gatelib.py"):
                shutil.copy(os.path.join(GATES_DIR, f), gates)
            shutil.copy(PROBE, os.path.join(gates, "fixtures"))
            with open(os.path.join(gates, "fixtures", os.path.basename(GOLDEN)), "w") as fh:
                fh.write(FIXTURE_GOLDEN)
            with open(os.path.join(gates, "ORACLE_PIN"), "w") as fh:
                fh.write(pin + "\n")
            return gates

        gates_main = install(main_co)
        gates_wt = install(wt)

        # A decoy repository OUTSIDE the fixture, with a modified tracked file
        # and a sibling `ghostty` at the pin. A gate that consulted the cwd or
        # honoured GIT_DIR would read the decoy's HEAD (not the pin) or its
        # dirty status instead of the fixture oracle's.
        decoy = os.path.join(base, "decoy", "rustty")
        os.makedirs(decoy)
        git(decoy, "init", "-q")
        with open(os.path.join(decoy, "f"), "w") as fh:
            fh.write("a\n")
        git(decoy, "add", "f")
        git(decoy, "commit", "-q", "-m", "decoy")
        with open(os.path.join(decoy, "f"), "w") as fh:
            fh.write("b\n")
        shutil.copytree(oracle, os.path.join(base, "decoy", "ghostty"), symlinks=True)
        decoy_env = {"GIT_DIR": os.path.join(decoy, ".git"), "GIT_WORK_TREE": decoy}

        def gate(gates=gates_main, extra_env=None, cwd=decoy, path=None):
            e = dict(env)
            e["PATH"] = path if path is not None else fakebin + os.pathsep + env.get("PATH", "")
            e.update(extra_env or {})
            r = subprocess.run(
                [sys.executable, os.path.join(gates, "check_ref_item_geometry.py")],
                cwd=cwd, env=e, capture_output=True, text=True)
            return r.returncode, r.stdout + r.stderr

        def expect(name, rc_want, needle, **kw):
            rc, out = gate(**kw)
            case(name, rc == rc_want and needle in out,
                 f"rc={rc} want {rc_want} and {needle!r}; output={out.strip()!r}")

        def pin_file(gates, text):
            p = os.path.join(gates, "ORACLE_PIN")
            if text is None:
                os.remove(p)
            else:
                with open(p, "w") as fh:
                    fh.write(text)

        case("fixture has no worktree-sibling oracle (no stopgap)",
             not os.path.lexists(stale), stale)

        # GREEN at the pin, from both checkouts; the worktree one resolves the
        # MAIN checkout's sibling.
        expect("pinned clean oracle is GREEN (main checkout, decoy cwd)", 0,
               "G-REFGEOM GREEN")
        expect("from a worktree without the stopgap symlink it resolves the main "
               "checkout's sibling and is GREEN", 0, "G-REFGEOM GREEN", gates=gates_wt)
        # A measured difference is RED.
        expect("a measured difference is RED", 1, "differs from the golden",
               extra_env={"REFGEOM_SELFTEST_SIZE": "8"})

        # The pin file.
        pin_file(gates_main, None)
        expect("missing ORACLE_PIN is BLOCKED", 3, "is missing or not a 40-hex commit")
        for label, text in (("empty", ""), ("39-hex", pin[:39] + "\n"),
                            ("upper-case", pin.upper() + "\n"),
                            ("two lines", pin + "\n" + pin + "\n")):
            pin_file(gates_main, text)
            expect(f"malformed ORACLE_PIN ({label}) is BLOCKED", 3,
                   "is missing or not a 40-hex commit")
        pin_file(gates_main, pin + "\n")

        # GIT_DIR + GIT_WORK_TREE name the dirty, off-pin decoy: no effect.
        expect("a GIT_DIR + GIT_WORK_TREE decoy (dirty, off the pin) does not "
               "redirect HEAD or status (GREEN)", 0, "G-REFGEOM GREEN",
               extra_env=decoy_env)
        expect("... nor from a worktree", 0, "G-REFGEOM GREEN", gates=gates_wt,
               extra_env=decoy_env)

        # Off the pin.
        git(oracle, "commit", "-q", "--allow-empty", "-m", "past the pin")
        expect("an off-pin HEAD is BLOCKED", 3, "not the pin")
        git(oracle, "reset", "-q", "--hard", pin)

        # Dirty; and a clean decoy cannot mask it.
        with open(os.path.join(oracle, "src", "terminal", "page.zig"), "a") as fh:
            fh.write("// edit\n")
        expect("a dirty oracle is BLOCKED", 3, "modified files")
        git(decoy, "checkout", "-q", "--", "f")
        expect("a clean GIT_DIR decoy does not mask a dirty oracle", 3,
               "modified files", extra_env=decoy_env)
        git(oracle, "checkout", "-q", "--", ".")
        expect("restored oracle is GREEN again", 0, "G-REFGEOM GREEN")

        # No git on PATH (the stand-in zig only): BLOCKED, never "clean". From
        # the main checkout the resolver's fallback is the same sibling, so the
        # run reaches the oracle's own git reads.
        expect("a missing git is BLOCKED", 3, "git could not run", path=fakebin)

        # A failing `status` at the pin (a corrupt index): `rev-parse` still
        # answers the right toplevel and the pin, and `status` exits 128. That
        # proves nothing about the tree, so it is BLOCKED, never read as clean.
        index = os.path.join(oracle, ".git", "index")
        with open(index, "rb") as fh:
            saved_index = fh.read()
        with open(index, "wb") as fh:
            fh.write(b"garbage")
        expect("a corrupt index (rev-parse ok, status exits non-zero) is BLOCKED",
               3, "status failed")
        with open(index, "wb") as fh:
            fh.write(saved_index)
        expect("restored index is GREEN again", 0, "G-REFGEOM GREEN")

        # Not the top level of its own repository: the oracle has no `.git`
        # and sits inside an enclosing repository whose HEAD IS the pin and
        # whose tree is clean. Only the `--show-toplevel` check can tell: the
        # pin and status checks both pass on the enclosing repository's
        # answers. (Realistic: an unpacked source tree inside a checkout.)
        os.rename(os.path.join(oracle, ".git"), os.path.join(base, "oracle.git"))
        git(base, "init", "-q")
        git(base, "add", os.path.join("ghostty", "src", "terminal", "page.zig"))
        git(base, "commit", "-q", "-m", "enclosing")
        pin_file(gates_main, git(base, "rev-parse", "HEAD") + "\n")
        expect("an oracle answered by an enclosing repository at the pin is "
               "BLOCKED", 3, "not the top level of its own repository")
        shutil.rmtree(os.path.join(base, ".git"))
        pin_file(gates_main, pin + "\n")
        os.rename(os.path.join(base, "oracle.git"), os.path.join(oracle, ".git"))
        expect("oracle restored after the enclosing-repository arm is GREEN", 0,
               "G-REFGEOM GREEN")

        # Not a repository: HEAD file says the pin, but there is no repository.
        os.rename(os.path.join(oracle, ".git"), os.path.join(base, "oracle.git"))
        os.makedirs(os.path.join(oracle, ".git"))
        with open(os.path.join(oracle, ".git", "HEAD"), "w") as fh:
            fh.write(pin + "\n")
        expect("an oracle that is not a git repository is BLOCKED (not RED)", 3,
               "not a git repository")
        shutil.rmtree(os.path.join(oracle, ".git"))
        expect("an oracle with no .git at all is BLOCKED (not RED)", 3,
               "not a git repository")
        os.rename(os.path.join(base, "oracle.git"), os.path.join(oracle, ".git"))

        # Absent.
        os.rename(oracle, oracle + ".away")
        expect("an absent oracle is BLOCKED, naming the main checkout's sibling", 3,
               f"oracle checkout is absent at {oracle}")
        expect("... also from a worktree", 3, f"oracle checkout is absent at {oracle}",
               gates=gates_wt)
    except (OSError, subprocess.CalledProcessError) as exc:
        case("fixture could not be built", False, str(exc))
    finally:
        shutil.rmtree(base, ignore_errors=True)

    # The live run: inside a linked worktree the resolved oracle must be the
    # main checkout's sibling, not the worktree's own (a string compare, so a
    # stopgap symlink at the worktree sibling cannot satisfy it).
    try:
        gd = subprocess.run(["git", "rev-parse", "--path-format=absolute", "--git-dir",
                             "--git-common-dir"], cwd=ROOT, env=G.git_env(),
                            capture_output=True, text=True).stdout.split()
    except OSError:
        gd = []
    if len(gd) == 2 and os.path.realpath(gd[0]) != os.path.realpath(gd[1]):
        want = os.path.join(os.path.dirname(os.path.dirname(gd[1])), "ghostty")
        case("live worktree run resolves to the main checkout's sibling",
             ORACLE == want and ORACLE != os.path.join(os.path.dirname(ROOT), "ghostty"),
             f"got {ORACLE}, want {want}")
    else:
        print("  [ok ] live run is from the main checkout (the worktree case is "
              "the fixture above)")

    print("SELF-TEST GREEN: an absent, off-pin, dirty, non-repository or git-less\n"
          "oracle, a failing status, an enclosing repository at the pin and a\n"
          "missing or malformed ORACLE_PIN are BLOCKED; a GIT_DIR decoy\n"
          "redirects nothing; a worktree resolves the main checkout's sibling; a\n"
          "measured difference is RED."
          if ok else "SELF-TEST RED: an arm above did not hold.")
    return 0 if ok else 1


def main():
    if "--self-test" in sys.argv:
        return self_test()
    reason = oracle_check()
    if reason:
        return blocked(reason)
    if shutil.which("zig") is None:
        return blocked("zig is not installed (the probe needs the reference's own toolchain)")
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
