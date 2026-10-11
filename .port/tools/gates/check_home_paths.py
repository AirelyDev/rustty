#!/usr/bin/env python3
"""G-HOMEPATH: no literal home-directory paths in tracked executable tooling.

WHY THIS GATE EXISTS
--------------------
Until CI hygiene A (2026-10-10) two tools drove real behaviour through a literal
path to one developer's machine: `ws10_suite.py` (G8b) found its corpora under a
spelled-out checkout path, and the xtask shell-integration oracle KAT fell back
to a spelled-out oracle path. Both worked on that machine and nowhere else: on
any other clone G8b BLOCKED for a reason that had nothing to do with the
substrate, and the KAT's fallback hid that its first candidate named a missing
directory from every worktree. The fix derives each location
(`_gatelib.main_checkout` / `oracle_root`, `git rev-parse --git-common-dir`);
this gate keeps the literal form from coming back.

WHAT IT SCANS
-------------
The TRACKED files (`git ls-files`, so build output and untracked scratch never
count) that are executable tooling:

  .port/tools/**              *.sh *.bash *.py *.mjs *.js *.ps1 *.pl
  .port/phase-0/harness/**    the same (G4's driver builders, comparator)
  .port/phase-0/baseline/**   the same (run_benchmarks.py, the perf recipe)
  xtask/src/**                *.rs
  .github/workflows/*         *.yml *.yaml

The two `.port/phase-0` directories are the live part of that record tree:
gates and the perf recipe run them today. The rest of `.port/phase-*` is
history (the phase-0 inventory generators, the phase-2+ workflow scripts),
which ran once on the machine that produced the contract and is kept as
evidence, so it is out of scope.

Comments are scanned too: a comment that spells a home path is one copy-paste
away from code, and none is needed (say "the main checkout" instead). Captured
evidence (`*.stdout`, `*.stderr`, `*.out`, ...), Markdown and the historical
records under `.port/phase-*` are NOT in scope: they record what a run printed
on the machine it ran on, and rewriting them would falsify them.

It flags `/Users/<name>` and `/home/<name>` where `<name>` is a literal path
component shaped like an account name (a letter, digit or `_`, then letters,
digits, `.`, `_`, `-`) and the leading `/` starts a path
(it is not preceded by a letter, digit, `.`, `_` or `~`, so `src/home/x` is
not a hit; a `-` may precede it, so a shell default `${VAR:-<home path>}` is one). A
placeholder such as `/Users/<name>` is not a hit: `<` is not a name character.

ALLOWLIST
---------
`home_paths_allow.tsv` beside this file: `path<TAB>literal<TAB>reason`, one row
per (file, literal) pair, e.g. a test fixture that needs an absolute path shaped
like a home directory. Every row must carry a reason, and a row that no longer
matches anything is RED (stale), so the list cannot outlive what it excuses.

EXIT CODES
----------
  0  GREEN    no unallowed literal home path
  1  RED      a hit, a stale or malformed allowlist row
  3  BLOCKED  git cannot list the tracked files (never a vacuous green)

Usage:
  check_home_paths.py               scan this checkout
  check_home_paths.py --root DIR    scan another checkout (the self-test's)
  check_home_paths.py --self-test   prove every arm flips, plus a control
"""
import os
import re
import subprocess
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
ALLOW_REL = ".port/tools/gates/home_paths_allow.tsv"

#: The tracked executable tooling, as (directory prefix, extensions). A prefix
#: ending in "/" scans its whole subtree; `.github/workflows/` is one level.
_SCRIPTS = (".sh", ".bash", ".py", ".mjs", ".js", ".ps1", ".pl")
SCOPE = [
    (".port/tools/", _SCRIPTS),
    (".port/phase-0/harness/", _SCRIPTS),
    (".port/phase-0/baseline/", _SCRIPTS),
    ("xtask/src/", (".rs",)),
    (".github/workflows/", (".yml", ".yaml")),
]

#: `/Users/<name>` or `/home/<name>`, the `/` starting a path.
HOME_PATH = re.compile(r"(?<![A-Za-z0-9._~])/(?:Users|home)/[A-Za-z0-9_][A-Za-z0-9._-]*")


def in_scope(rel):
    for prefix, exts in SCOPE:
        if not rel.startswith(prefix) or not rel.endswith(exts):
            continue
        if prefix == ".github/workflows/" and "/" in rel[len(prefix):]:
            continue
        return True
    return False


def tracked_files(root):
    """`git ls-files` of `root`, or None if git cannot answer (BLOCKED).

    `GIT_*` is removed so the caller's `GIT_DIR` cannot point the scan at a
    different repository (R5, as `_gatelib.git_env`)."""
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    try:
        r = subprocess.run(["git", "ls-files", "-z"], cwd=root, env=env,
                           capture_output=True)
    except OSError:
        return None
    if r.returncode != 0:
        return None
    return [p for p in r.stdout.decode("utf-8", "surrogateescape").split("\0") if p]


def load_allow(root):
    """{(path, literal): reason} and a list of malformed-row messages."""
    allow, bad = {}, []
    path = os.path.join(root, ALLOW_REL)
    if not os.path.exists(path):
        return allow, bad
    with open(path, encoding="utf-8") as fh:
        for n, line in enumerate(fh, 1):
            line = line.rstrip("\n")
            if not line.strip() or line.startswith("#"):
                continue
            cols = line.split("\t")
            if len(cols) != 3 or not all(c.strip() for c in cols):
                bad.append(f"{ALLOW_REL}:{n}: want path<TAB>literal<TAB>reason, "
                           f"got {line!r}")
                continue
            allow[(cols[0], cols[1])] = cols[2]
    return allow, bad


def scan(root):
    """(exit code, report lines)."""
    files = tracked_files(root)
    if files is None:
        return 3, [f"BLOCKED: git ls-files failed in {root}; nothing was scanned"]
    allow, bad = load_allow(root)
    used = set()
    hits = []
    scanned = 0
    for rel in files:
        if not in_scope(rel):
            continue
        full = os.path.join(root, rel)
        try:
            with open(full, encoding="utf-8", errors="surrogateescape") as fh:
                text = fh.read()
        except (OSError, IsADirectoryError):
            continue  # a tracked symlink to a dir, or a deleted-but-staged file
        scanned += 1
        for n, line in enumerate(text.split("\n"), 1):
            for m in HOME_PATH.finditer(line):
                key = (rel, m.group(0))
                if key in allow:
                    used.add(key)
                    continue
                hits.append(f"  RED {rel}:{n}: {m.group(0)}  ({line.strip()[:120]})")
    stale = [f"  RED stale allowlist row: {p}\t{lit} (matches nothing)"
             for (p, lit) in sorted(set(allow) - used)]
    out = [f"=== G-HOMEPATH :: literal home paths in {scanned} tracked tooling files ==="]
    if hits or stale or bad:
        out += hits + stale + ["  RED " + b for b in bad]
        out.append("\nDerive the location instead (the main checkout: "
                   "_gatelib.main_checkout; the oracle: _gatelib.oracle_root / "
                   "_oracle_root.sh; a user dir: $HOME), or allowlist a fixture "
                   f"with a reason in {ALLOW_REL}.")
        return 1, out
    out.append(f"  GREEN: none ({len(used)} allowlisted)")
    return 0, out


# --------------------------------------------------------------------------
# Self-test: fixture repositories, one per arm, run through a COPY of this
# script (`--root`), plus a control: a mutated copy whose pattern only knows
# `/Users/` must let the `/home/` arm ([4]) through, which that arm notices.
# --------------------------------------------------------------------------
U = "/" + "Users" + "/alice"  # spelled in pieces so this file never matches
H = "/" + "home" + "/bob"


def _fixture(td, files, allow=None):
    env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
    env.update(GIT_CONFIG_GLOBAL=os.devnull, GIT_CONFIG_NOSYSTEM="1")
    subprocess.run(["git", "init", "-q", td], env=env, check=True)
    if allow is not None:
        files = dict(files, **{ALLOW_REL: allow})
    for rel, body in files.items():
        p = os.path.join(td, rel)
        os.makedirs(os.path.dirname(p), exist_ok=True)
        with open(p, "w") as fh:
            fh.write(body)
    subprocess.run(["git", "add", "-A"], cwd=td, env=env, check=True)


def _run(script, root):
    r = subprocess.run([sys.executable, script, "--root", root],
                       capture_output=True, text=True)
    return r.returncode, r.stdout


def self_test():
    print("=== G-HOMEPATH SELF-TEST ===")
    clean = {".port/tools/gates/ok.sh": 'echo "$HOME/x" /opt/homebrew/bin src/home/x /home/-n\n',
             ".port/tools/ph.py": "# the main checkout, e.g. /Users/<name>/rustty\n"}
    arms = [
        # (label, files, allowlist, expected exit)
        ("clean tree (placeholders, $HOME, /opt/homebrew, src/home)", clean, None, 0),
        (".sh with /Users/<name>",
         dict(clean, **{".port/tools/a.sh": f'ROOT="{U}/projects/rustty"\n'}), None, 1),
        ("a ${VAR:-/Users/<name>} shell default",
         dict(clean, **{".port/phase-0/harness/b.sh": f'G="${{G:-{U}/g}}"\n'}), None, 1),
        ("run_benchmarks-style .py under .port/phase-0/baseline",
         dict(clean, **{".port/phase-0/baseline/r.py": f'COR = "{U}/c"\n'}), None, 1),
        (".py with /home/<name>",
         dict(clean, **{".port/tools/perf/b.py": f'X = "{H}/c"\n'}), None, 1),
        ("comment in a .mjs",
         dict(clean, **{".port/tools/c.mjs": f"// see {U}/x\n"}), None, 1),
        ("xtask/src .rs",
         dict(clean, **{"xtask/src/d.rs": f'const P: &str = "{U}/g";\n'}), None, 1),
        (".github/workflows .yml",
         dict(clean, **{".github/workflows/ci.yml": f"run: cd {H}/r\n"}), None, 1),
        ("out of scope: captured .stdout, .md, .port/phase-*, crates/",
         dict(clean, **{".port/tools/fixtures/x.stdout": f"{U}/x\n",
                        ".port/tools/README.md": f"{U}/x\n",
                        ".port/phase-8/run.sh": f"{U}/x\n",
                        ".port/phase-0/inventory/parse.py": f"{U}/x\n",
                        "crates/c/src/lib.rs": f"// {U}/x\n"}), None, 0),
        ("allowlisted fixture",
         dict(clean, **{"xtask/src/e.rs": f'"{H}/.rustup"\n'}),
         f"xtask/src/e.rs\t{H}\ttest fixture\n", 0),
        ("allowlist names another literal",
         dict(clean, **{"xtask/src/e.rs": f'"{H}/.rustup"\n'}),
         f"xtask/src/e.rs\t{U}\ttest fixture\n", 1),
        ("stale allowlist row", clean, f"xtask/src/gone.rs\t{H}\told fixture\n", 1),
        ("allowlist row without a reason",
         dict(clean, **{"xtask/src/e.rs": f'"{H}/.rustup"\n'}),
         f"xtask/src/e.rs\t{H}\t \n", 1),
    ]
    me = os.path.abspath(__file__)
    ok = True
    with tempfile.TemporaryDirectory() as td:
        for i, (label, files, allow, want) in enumerate(arms):
            root = os.path.join(td, f"arm{i}")
            _fixture(root, files, allow)
            rc, out = _run(me, root)
            good = rc == want
            print(f"  {'ok  ' if good else 'FAIL'} [{i}] {label} -> rc={rc} (expect {want})")
            if not good:
                print("       " + out.replace("\n", "\n       "))
            ok &= good
        # A directory that is not a git repository: BLOCKED, never green.
        nogit = os.path.join(td, "nogit")
        os.makedirs(nogit)
        rc, _ = _run(me, nogit)
        print(f"  {'ok  ' if rc == 3 else 'FAIL'} [b] not a git repository -> rc={rc} (expect 3)")
        ok &= rc == 3
        # Control: a copy that only knows `/Users/` passes the `/home/` arm,
        # so arm [2] is what catches that regression.
        src = open(me).read()
        old = "(?:Users" + "|home)"  # in pieces: only the pattern matches
        assert src.count(old) == 1
        mutant = os.path.join(td, "mutant.py")
        with open(mutant, "w") as fh:
            fh.write(src.replace(old, "(?:Users)"))
        rc, _ = _run(mutant, os.path.join(td, "arm4"))
        print(f"  {'ok  ' if rc == 0 else 'FAIL'} control: a Users-only copy "
              f"passes the /home/ fixture -> rc={rc} (expect 0; arm [4] requires 1)")
        ok &= rc == 0
    print("G-HOMEPATH-SELF", "GREEN" if ok else "RED")
    return 0 if ok else 1


def main(argv):
    if argv[1:] == ["--self-test"]:
        return self_test()
    root = ROOT
    if len(argv) == 3 and argv[1] == "--root":
        root = os.path.abspath(argv[2])
    elif len(argv) != 1:
        print(__doc__.split("Usage:")[1], file=sys.stderr)
        return 2
    rc, out = scan(root)
    print("\n".join(out))
    return rc


if __name__ == "__main__":
    sys.exit(main(sys.argv))
