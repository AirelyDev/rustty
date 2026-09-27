#!/usr/bin/env python3
"""G-VTFEAT — the `-Dvt-features` parity gate.

The reference lets a *lib* artifact trim optional features so size-conscious
embedders can shrink the binary (`terminal/build_options.zig` `Options.Features`,
`1fdbb8c91`; `-Dvt-features=-all,+render-state` builds only the render state
API). Mechanically, every feature gates a block of `@export` lines in
`src/lib_vt.zig`, so disabling one removes exactly that block's C API symbols
from the artifact.

rustty spells the same knob as one cargo feature per `Features` field on
`rustty-vt` (driving the `options::*` consts) and `rustty-vt-c` (deciding which
`#[no_mangle]` exports compile). All of them are in `default`, which is what
makes an additive cargo feature set equivalent to the reference's
subtract-from-all model.

This gate asserts, against the ORACLE rather than a hand-written list:

  1. Every `-Dvt-features` token is a cargo feature on both crates, and every
     one is in each crate's `default` list. If a feature ever silently left
     `default`, the application tier would quietly lose C API surface.
  2. Each single-feature build compiles CLEANLY (no warnings), as do the
     all-off and default builds.
  3. For each single-feature build, the exported `rustty_*` symbol set is
     EXACTLY the ungated exports plus the ones `lib_vt.zig` guards with that
     feature. This is the assertion with teeth: a missed `#[cfg]` shows up as a
     symbol that should have been trimmed, and an over-eager one as a symbol
     that should have survived.

NOT checked, deliberately: running the test suite under a trimmed build. The
reference does not support that either — `Config.terminalOptions` gives the
`.ghostty` artifact `.features = .{}` (all enabled) and only `.lib` gets
`self.vt_features`, so `zig build test` is always all-enabled, and several
reference tests (e.g. the kitty scroll-margin storage tests) would fail with
`-kitty-graphics` because they carry no `SkipZigTest` guard. Trimming is a
BUILD configuration, not a test configuration; adding guards the oracle lacks
would be an R1 divergence.
"""

import os
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(Path(__file__).resolve().parent))
import _gatelib as G  # noqa: E402

# The one resolver every sibling-oracle gate uses (it is right inside a git
# worktree, where `ROOT/../ghostty` is not).
ORACLE = Path(G.oracle_root(str(ROOT)))

failures = []


def fail(msg):
    failures.append(msg)
    print(f"  FAIL: {msg}")


def oracle_features():
    """The `-Dvt-features` tokens, read FROM THE ORACLE.

    `terminal/build_options.zig` declares `pub const Features = packed struct
    { <name>: bool = true, ... }`; its field names ARE the accepted tokens
    ("Field names are the accepted tokens for `-Dvt-features`"). The
    reference's `eqlName` makes `-` and `_` interchangeable, so the
    cargo-conventional hyphen spelling is the same token.

    This used to be a hand-written nine-entry list. P6-W3 (`32601cd79`) added
    a tenth feature (`search`) upstream, and a literal list would have kept
    reporting the nine-feature matrix as complete — the export-set arm below
    did catch the missing default-build exports, but the per-feature matrix
    would never have tried `--features search` on its own. Deriving the list
    from the oracle closes that: a new upstream feature is a new matrix row
    (and a RED "not a cargo feature" until it is declared here). A floor of
    nine guards against a parse that silently reads nothing (R5).
    """
    src = (ORACLE / "src" / "terminal" / "build_options.zig").read_text()
    m = re.search(r"pub const Features = packed struct \{(.*?)\n    \};", src, re.S)
    if not m:
        fail("could not locate `pub const Features = packed struct` in the oracle")
        return []
    body = re.sub(r"(?m)^\s*///.*$", "", m.group(1))
    names = re.findall(r"(?m)^\s*([a-z][a-z0-9_]*): bool = true,", body)
    if len(names) < 9:
        fail(f"oracle Features struct parsed to only {len(names)} field(s) — parser broke")
    return [n.replace("_", "-") for n in names]


def parse_lib_vt():
    """Map each exported symbol to the feature condition guarding it.

    Returns {symbol: condition-or-None} using rustty_* names, where condition
    is the raw `features.x and features.y` text from lib_vt.zig.
    """
    src = (ORACLE / "src" / "lib_vt.zig").read_text()
    src = src[src.index("const features = terminal.options;"):]

    out = {}
    stack = []  # (condition, depth-at-open)
    depth = 0
    for ln in src.split("\n"):
        m = re.search(r"if \(([^)]*features[^)]*)\) \{", ln)
        if m:
            stack.append((m.group(1).strip(), depth))
            depth += 1
            continue
        if ln.strip() == "}" and stack and depth - 1 == stack[-1][1]:
            depth -= 1
            stack.pop()
            continue
        e = re.search(r'\.name = "ghostty_([A-Za-z0-9_]+)"', ln)
        if e:
            out["rustty_" + e.group(1)] = stack[-1][0] if stack else None
    return out


def condition_holds(cond, enabled):
    """Evaluate a lib_vt.zig feature condition for a set of enabled features."""
    if cond is None:
        return True
    norm = cond.replace("features.", "").replace("_", "-")
    if " and " in norm:
        return all(p.strip() in enabled for p in norm.split(" and "))
    if " or " in norm:
        return any(p.strip() in enabled for p in norm.split(" or "))
    return norm.strip() in enabled


def cargo_features(manifest):
    """Return (declared feature names, TRANSITIVELY resolved default set).

    The default set is resolved through the crate's own features, not read as
    a literal string list. Cargo activates features transitively, so

        default = ["simd", "vt-features-default"]
        vt-features-default = ["snapshot", "formatter", ...]

    enables `snapshot` exactly as surely as spelling it into `default` would.
    A literal membership test called that RED — which it was, for nine
    features, the moment CMakeLists needed a name for "all VT features, SIMD
    off" (see crates/rustty-vt-c/Cargo.toml). The property this gate exists to
    protect is "a default build has every feature on"; resolving is what
    actually checks that property rather than one spelling of it.

    Dependency features (`rustty-vt/snapshot`, `dep:foo`) are not expanded:
    they name another crate's feature, and this gate only reasons about the
    manifest in front of it.
    """
    text = manifest.read_text()
    body = text[text.index("[features]"):]
    nxt = re.search(r"\n\[(?!features)", body)
    if nxt:
        body = body[: nxt.start()]
    body = re.sub(r"(?m)^\s*#.*$", "", body)
    declared = set(re.findall(r"(?m)^([A-Za-z0-9_-]+)\s*=", body))

    # name -> the features it activates, for every feature in this manifest.
    edges = {}
    for m in re.finditer(r"(?m)^([A-Za-z0-9_-]+)\s*=\s*\[(.*?)\]", body, re.S):
        edges[m.group(1)] = re.findall(r'"([^"]+)"', m.group(2))

    default = set()
    queue = list(edges.get("default", []))
    while queue:
        f = queue.pop()
        if f in default or "/" in f or f.startswith("dep:"):
            continue
        default.add(f)
        queue.extend(edges.get(f, []))
    return declared, default


def build(args):
    """Build rustty-vt-c with `args`; return combined output ('' == clean)."""
    p = subprocess.run(
        ["cargo", "build", "-q", "-p", "rustty-vt-c", *args],
        cwd=ROOT,
        capture_output=True,
        text=True,
    )
    return (p.stdout + p.stderr).strip()


def exported_symbols():
    """The `rustty_*` symbols exported by the built cdylib."""
    for name in ("librustty_vt_c.dylib", "librustty_vt_c.so"):
        lib = ROOT / "target" / "debug" / name
        if lib.exists():
            break
    else:
        fail("no built cdylib found under target/debug")
        return set()

    if lib.suffix == ".dylib":
        out = subprocess.run(["nm", "-gU", str(lib)], capture_output=True, text=True).stdout
        return set(re.findall(r"\s_(rustty_[a-z0-9_]+)$", out, re.M))
    out = subprocess.run(
        ["nm", "-D", "--defined-only", str(lib)], capture_output=True, text=True
    ).stdout
    return set(re.findall(r"\s(rustty_[a-z0-9_]+)$", out, re.M))


def oracle_check():
    """None when the oracle is the pinned reference, else the BLOCKED reason.

    `_gatelib.oracle_root`'s contract: a missing or off-pin oracle is BLOCKED
    (exit 3), never a pass and never a crash. Before parity tail P-1 this gate
    read the oracle unguarded, so on a host without the checkout (every CI leg)
    the uncaught FileNotFoundError exited 1, which ci.sh counts as a RED
    regression of a PROGRESS gate. The checks are G-REFGEOM's (`oracle_check`
    in check_ref_item_geometry.py): every git read goes through
    `G.oracle_git` (`GIT_*` stripped); a git that cannot run or exits non-zero
    is BLOCKED; `--show-toplevel` rejects a directory that is not a repository
    of its own (git would otherwise answer from an enclosing repository); and
    the tracked tree must be clean, because this gate reads the oracle's
    WORKING TREE, not `git show <pin>:`."""
    pin = G.oracle_pin()
    if pin is None:
        return (f"the oracle pin file {G.ORACLE_PIN_FILE} is missing or not a "
                "40-hex commit, so the oracle's commit cannot be checked")
    if not (ORACLE / "src" / "lib_vt.zig").is_file():
        return f"the ghostty oracle checkout is absent ({ORACLE})"
    try:
        r = G.oracle_git(str(ORACLE), "rev-parse", "--show-toplevel", "HEAD", text=True)
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
    if head != pin:
        return f"the oracle is at {head}, not the pin {pin}"
    try:
        r = G.oracle_git(str(ORACLE), "status", "--porcelain", "--untracked-files=no", text=True)
    except OSError as exc:
        return f"git could not run on the oracle at {ORACLE} ({exc})"
    if r.returncode != 0:
        return (f"status failed on the oracle at {ORACLE} "
                f"(exit {r.returncode}: {r.stderr.strip()})")
    modified = [line for line in r.stdout.splitlines() if line.strip()]
    if modified:
        return ("the oracle has modified files, so it is not the pinned reference:\n  "
                + "\n  ".join(modified[:10]))
    return None


def self_test():
    """Run COPIES of this gate against fixture oracles and assert each
    BLOCKED arm exits 3 and a clean pinned oracle passes the check.

    Layout: `<tmp>/rustty` is a git repository holding a copy of this gate,
    `_gatelib.py` and a fixture `ORACLE_PIN`, so `_gatelib.oracle_root`
    resolves the oracle to `<tmp>/ghostty` exactly as it does for the real
    checkout. The positive arm runs the copy with `--check-oracle-only`, which
    stops after the oracle check (the build matrix needs the real workspace)."""
    import shutil
    import tempfile

    print("=== G-VTFEAT SELF-TEST ===")
    ok = True

    def case(name, good, detail=""):
        nonlocal ok
        if not good:
            ok = False
        print(f"  [{'ok ' if good else 'FAIL'}] {name}"
              + (f"\n         ({detail})" if detail and not good else ""))

    env = G.git_env()
    cfg = ["-c", "user.name=selftest", "-c", "user.email=selftest@invalid",
           "-c", "commit.gpgsign=false", "-c", "core.hooksPath=/dev/null",
           "-c", "init.defaultBranch=main"]

    def git(where, *args):
        return subprocess.run(["git", *cfg, *args], cwd=where, env=env,
                              capture_output=True, text=True, check=True).stdout.strip()

    base = os.path.realpath(tempfile.mkdtemp(prefix="gvtfeat_selftest_"))
    try:
        co = os.path.join(base, "rustty")
        gates = os.path.join(co, ".port", "tools", "gates")
        oracle = os.path.join(base, "ghostty")
        os.makedirs(gates)
        for f in ("check_vt_features.py", "_gatelib.py"):
            shutil.copy(os.path.join(os.path.dirname(os.path.abspath(__file__)), f), gates)
        git(co, "init", "-q")
        git(co, "commit", "-q", "--allow-empty", "-m", "fixture")
        pin_file = os.path.join(gates, "ORACLE_PIN")

        def run_copy():
            r = subprocess.run([sys.executable, os.path.join(gates, "check_vt_features.py"),
                                "--check-oracle-only"], env=env, cwd=base,
                               capture_output=True, text=True)
            return r.returncode, r.stdout + r.stderr

        # 1. pin file missing.
        rc, out = run_copy()
        case("missing ORACLE_PIN -> BLOCKED (exit 3)", rc == 3 and "BLOCKED" in out, f"rc={rc} {out[-300:]}")
        with open(pin_file, "w") as fh:
            fh.write("0" * 40 + "\n")

        # 2. oracle absent.
        rc, out = run_copy()
        case("oracle absent -> BLOCKED (exit 3)",
             rc == 3 and "absent" in out, f"rc={rc} {out[-300:]}")

        # 3. oracle present but not a repository (nothing encloses it).
        os.makedirs(os.path.join(oracle, "src"))
        with open(os.path.join(oracle, "src", "lib_vt.zig"), "w") as fh:
            fh.write("// fixture\n")
        rc, out = run_copy()
        case("oracle not a repository -> BLOCKED (exit 3)",
             rc == 3 and "not a git repository" in out, f"rc={rc} {out[-300:]}")

        # 4. the oracle is its own repository at a commit other than the pin.
        git(oracle, "init", "-q")
        git(oracle, "add", "-A")
        git(oracle, "commit", "-q", "-m", "pinned")
        pin = git(oracle, "rev-parse", "HEAD")
        rc, out = run_copy()
        case("oracle off the pin -> BLOCKED (exit 3)",
             rc == 3 and "not the pin" in out, f"rc={rc} {out[-300:]}")

        # 5. at the pin, but a tracked file is modified.
        with open(pin_file, "w") as fh:
            fh.write(pin + "\n")
        with open(os.path.join(oracle, "src", "lib_vt.zig"), "a") as fh:
            fh.write("// edited\n")
        rc, out = run_copy()
        case("oracle dirty -> BLOCKED (exit 3)",
             rc == 3 and "modified files" in out, f"rc={rc} {out[-300:]}")
        git(oracle, "checkout", "-q", "--", "src/lib_vt.zig")

        # 6. positive control: the clean pinned oracle passes the check.
        rc, out = run_copy()
        case("clean pinned oracle -> check passes (exit 0)",
             rc == 0 and "BLOCKED" not in out, f"rc={rc} {out[-300:]}")

        # 7. a fixture oracle nested inside an enclosing repository is rejected
        #    by --show-toplevel (git would answer from the enclosing repo).
        shutil.rmtree(os.path.join(oracle, ".git"))
        git(base, "init", "-q")
        git(base, "commit", "-q", "--allow-empty", "-m", "enclosing")
        rc, out = run_copy()
        case("oracle inside an enclosing repository -> BLOCKED (exit 3)",
             rc == 3 and "not the top level" in out, f"rc={rc} {out[-300:]}")
    finally:
        shutil.rmtree(base, ignore_errors=True)

    print(f"\nG-VTFEAT SELF-TEST: {'GREEN' if ok else 'RED'}")
    return 0 if ok else 1


if "--self-test" in sys.argv[1:]:
    sys.exit(self_test())

print("G-VTFEAT: -Dvt-features parity")
print(f"  oracle: {ORACLE}")

_blocked = oracle_check()
if _blocked is not None:
    print(f"G-VTFEAT BLOCKED: {_blocked}")
    sys.exit(3)
if "--check-oracle-only" in sys.argv[1:]:
    print("G-VTFEAT: oracle check passed (--check-oracle-only)")
    sys.exit(0)

exports = parse_lib_vt()
print(f"  lib_vt.zig exports: {len(exports)}")
FEATURES = oracle_features()
print(f"  build_options.zig Features: {len(FEATURES)} ({', '.join(FEATURES)})")

# --- 1. feature declarations -------------------------------------------------
for crate in ("rustty-vt", "rustty-vt-c"):
    declared, default = cargo_features(ROOT / "crates" / crate / "Cargo.toml")
    for f in FEATURES:
        if f not in declared:
            fail(f"{crate}: `-Dvt-features` token `{f}` is not a cargo feature")
        elif f not in default:
            fail(
                f"{crate}: feature `{f}` is not in `default` — the application "
                f"tier must build with every feature enabled"
            )
print(f"  [1/3] feature declarations + default set")

# --- 2 & 3. build matrix and symbol sets -------------------------------------
# Symbols exported unconditionally (outside every `if (features.X)` block).
# The wasm-only allocator exports are target-gated, not feature-gated, and are
# absent from a host build; drop them from the expectation.
WASM_ONLY = {
    "rustty_wasm_alloc",
    "rustty_wasm_free",
    "rustty_wasm_alloc_opaque",
    "rustty_wasm_free_opaque",
    "rustty_wasm_take_opaque",
}

matrix = [([f], f"--features {f}") for f in FEATURES]
matrix.append(([], "-all (no features)"))

for enabled, label in matrix:
    args = ["--no-default-features"]
    if enabled:
        args += ["--features", ",".join(enabled)]
    out = build(args)
    if out:
        fail(f"{label}: build was not clean:\n{out[:2000]}")
        continue

    got = exported_symbols()
    want = {
        sym
        for sym, cond in exports.items()
        if condition_holds(cond, set(enabled)) and sym not in WASM_ONLY
    }
    extra, missing = got - want, want - got
    if extra:
        fail(f"{label}: {len(extra)} symbol(s) should have been trimmed: {sorted(extra)[:8]}")
    if missing:
        fail(f"{label}: {len(missing)} symbol(s) missing from the artifact: {sorted(missing)[:8]}")
    if not extra and not missing:
        print(f"  [2/3] {label:<28} clean build, {len(got):>3} exports — exact")

out = build([])
if out:
    fail(f"default build was not clean:\n{out[:2000]}")
else:
    got = exported_symbols()
    want = {s for s in exports if s not in WASM_ONLY}
    if got != want:
        fail(
            f"default build exports {len(got)}, oracle all-enabled expects {len(want)}; "
            f"extra={sorted(got - want)[:5]} missing={sorted(want - got)[:5]}"
        )
    else:
        print(f"  [3/3] default (all features)      clean build, {len(got):>3} exports — exact")

if failures:
    print(f"\nVT-FEATURES GATE: RED — {len(failures)} failure(s)")
    sys.exit(1)
print("\nVT-FEATURES GATE: GREEN — every -Dvt-features token trims exactly its "
      "lib_vt.zig export block")
