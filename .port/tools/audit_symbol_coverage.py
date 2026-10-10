#!/usr/bin/env python3
"""Per-SYMBOL oracle->port coverage audit (pass 2).

`audit_file_coverage.py` answers "does a counterpart file exist". This answers
the next question down: **does every public function in that oracle file have a
counterpart symbol somewhere in the port?** A file can exist, be the right size,
cite its oracle source, and still be missing individual `pub fn`s — that is
precisely the shape of a partial port, and nothing else here looks for it.

MATCHING. Zig `pub fn fooBar` becomes Rust `fn foo_bar` by R2 convention, but
this port also keeps oracle spellings in PORT NOTEs and sometimes keeps camelCase
for fidelity. So a symbol counts as PRESENT if any of these appears anywhere in
the crates tree:
    fooBar        (verbatim, e.g. in a PORT NOTE or a kept name)
    foo_bar       (the R2 snake_case form)
This is deliberately generous. A false PRESENT is a missed gap, but a false
MISSING wastes a human's time and — worse, on this project's evidence — invites
someone to "fix" the audit rather than the code. Generous matching plus a written
justification for every accepted absence is the safer bias.

CONTAINER-AWARE (P8-W10 T4). "Anywhere in the crates tree" was too generous:
`modes.zig` `ModeState.getDefault` counted as present because of an unrelated
`hb.rs` `Language::get_default` in rustty-font-shaper. The token now has to be
in a Rust file that belongs to the symbol, which is either
    - a COUNTERPART of the oracle file: the same mapping G-FILES uses
      (`audit_file_coverage.py`: a filename match, or a Rust file citing the
      oracle path), or
    - a file that names the symbol's CONTAINER: the innermost enclosing Zig
      `const X = struct/union/enum/opaque` or type-returning `fn X(...) type`,
      else the oracle file's stem (`Terminal` for Terminal.zig, `modes` for
      modes.zig), verbatim or snake_case.
The prefix rule below applies inside that scope too.

Usage:
  python3 .port/tools/audit_symbol_coverage.py                 # gaps only
  python3 .port/tools/audit_symbol_coverage.py --file src/terminal/page.zig
  python3 .port/tools/audit_symbol_coverage.py --limit 40      # cap output
  python3 .port/tools/audit_symbol_coverage.py --self-test     # negative control
"""
import os
import re
import subprocess
import sys

# The checkout this tool belongs to, from the script's own location, and the
# oracle beside the MAIN checkout, from the shared `_gatelib.oracle_root`.
# The reasoning, and the R5 history (ROOT from `--show-toplevel` and a private
# resolver, neither stripping `GIT_*`), is in `audit_file_coverage.py`.
# G-TESTS-SELF (`audit_test_coverage.py --self-test`) proves that GIT_DIR +
# GIT_WORK_TREE move none of ROOT, ORACLE or `oracle_files()` here.
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "gates"))
import _gatelib as G  # noqa: E402

ROOT = G.RUSTTY_ROOT
ORACLE = G.oracle_root(ROOT)


def oracle_files():
    """`git ls-files src` in the oracle, with `GIT_*` removed (R5)."""
    return subprocess.run(["git", "ls-files", "src"], cwd=ORACLE, env=G.git_env(),
                          capture_output=True, text=True, check=True).stdout.split()


SKIP_PREFIXES = ("src/build/", "src/bench/")

PUB_FN = re.compile(r"^\s*pub fn ([A-Za-z_][A-Za-z0-9_]*)\s*\(", re.M)


def camel_to_snake(name):
    s = re.sub(r"(.)([A-Z][a-z]+)", r"\1_\2", name)
    return re.sub(r"([a-z0-9])([A-Z])", r"\1_\2", s).lower()


TOKEN = re.compile(r"[A-Za-z_][A-Za-z0-9_]*")
ZIG_PATH = re.compile(r"[\w./-]*\.zig\b")
CONTAINER = re.compile(r"^( *)(?:pub\s+)?(?:const\s+([A-Za-z_]\w*)\s*=\s*(?:extern\s+|packed\s+)?"
                       r"(?:struct|union|enum|opaque)\b|fn\s+([A-Za-z_]\w*)\s*\(.*\)\s*type\s*\{)")
PUB_FN_LINE = re.compile(r"^( *)pub fn ([A-Za-z_][A-Za-z0-9_]*)\s*\(")


def zig_pub_fns(src, rel):
    """[(name, container)] for every `pub fn` of one oracle file. The container
    is the innermost enclosing container declared at a smaller indent (zig fmt
    indents each level by 4), else the file's stem."""
    stem = os.path.basename(rel)[:-4]
    out = []
    stack = []  # (indent, name) of open containers
    for line in src.splitlines():
        m = PUB_FN_LINE.match(line)
        indent = len(line) - len(line.lstrip(" "))
        if line.strip():
            while stack and stack[-1][0] >= indent and not line.lstrip().startswith(("}", ")")):
                stack.pop()
        if m:
            out.append((m.group(2), stack[-1][1] if stack else stem))
        c = CONTAINER.match(line)
        if c:
            stack.append((len(c.group(1)), c.group(2) or c.group(3)))
    return out


def build_symbol_index(root=None):
    """One pass over the whole crates tree: per Rust file its identifier
    tokens and the oracle paths it cites. Returns (tokens_by_file,
    files_by_token, cites) with cites = {zig path token: set(files)}."""
    root = ROOT if root is None else root
    by_file = {}
    by_token = {}
    cites = {}
    for base, dirs, files in os.walk(os.path.join(root, "crates")):
        dirs[:] = [d for d in dirs if d != "target"]
        for f in files:
            if not f.endswith(".rs"):
                continue
            path = os.path.join(base, f)
            try:
                with open(path, errors="ignore") as fh:
                    text = fh.read()
            except OSError:
                continue
            toks = set(TOKEN.findall(text))
            by_file[path] = toks
            for t in toks:
                by_token.setdefault(t, set()).add(path)
            for z in set(ZIG_PATH.findall(text)):
                cites.setdefault(z, set()).add(path)
    return by_file, by_token, cites


def counterparts(rel, by_file, cites):
    """G-FILES' counterpart mapping (`audit_file_coverage.py`), on the index:
    files named after the oracle stem (`snake.rs`, `snake_impl.rs`,
    `stem.rs`, `snake/mod.rs`) plus files citing `terminal/page.zig`, or
    `page.zig` when nothing cites the longer path."""
    stem = os.path.basename(rel)[:-4]
    snake = camel_to_snake(stem)
    names = {snake, snake + "_impl", stem.lower()}
    out = set()
    for path in by_file:
        f = os.path.basename(path)
        if f[:-3] in names or (f == "mod.rs" and os.path.basename(os.path.dirname(path)) in names):
            out.add(path)
    for pat in (rel[4:], os.path.basename(rel)):
        hits = set()
        for z, files in cites.items():
            if z == pat or z.endswith("/" + pat):
                hits |= files
        if hits:
            out |= hits
            break
    return out


def container_aliases(container):
    """Spellings of a container name in the port (verbatim, snake_case, R2)."""
    out = {container, camel_to_snake(container)}
    for c in list(out):
        out.add(c.replace("Ghostty", "Rustty").replace("ghostty", "rustty"))
    return out


def present(name, rel, container, index, cp_cache):
    """THE match: `name` or its snake_case (or a >= 8-char snake prefix of a
    longer token) in a file that belongs to the symbol (module doc)."""
    by_file, by_token, cites = index
    if rel not in cp_cache:
        cp_cache[rel] = counterparts(rel, by_file, cites)
    scope = set(cp_cache[rel])
    for a in container_aliases(container):
        scope |= by_token.get(a, set())
    snake = camel_to_snake(name)
    if (by_token.get(name, set()) | by_token.get(snake, set())) & scope:
        return True
    # PREFIX match, not exact. This port routinely SUFFIXES a ported name —
    # `putAssumeCapacityNoClobber` lands as `put_assume_capacity_no_clobber_context`
    # — and exact token matching reported three such functions as absent when all
    # three were present and in use. A suffixed name is still the port of that
    # function; a prefix of a longer identifier is not a coincidence at these
    # lengths.
    if len(snake) >= 8:
        return any(snake in by_file[f] or any(t.startswith(snake) for t in by_file[f])
                   for f in scope)
    return False


def analyze(zig, read_src, index, accepted):
    """(total, gaps, acked) over oracle files `zig`; `read_src(rel)` returns
    the oracle source or None. Split out so --self-test can feed fixtures."""
    total = 0
    gaps = []
    acked = []
    cp_cache = {}
    for rel in sorted(zig):
        src = read_src(rel)
        if src is None:
            continue
        for name, container in zig_pub_fns(src, rel):
            if name in ("main", "deinit", "init"):     # too generic to be evidence
                continue
            total += 1
            if present(name, rel, container, index, cp_cache):
                continue
            if f"{rel}::{name}" in accepted:
                acked.append((rel, name))
                continue
            gaps.append((rel, name))
    return total, gaps, acked


def self_test():
    """Negative control for the container-aware match (P8-W10 T4): the
    `ModeState.getDefault` false positive, rebuilt on a fixture tree."""
    import shutil
    import tempfile
    tmp = tempfile.mkdtemp(prefix="gsyms_selftest_")
    try:
        os.makedirs(os.path.join(tmp, "crates", "rustty-vt", "src"))
        os.makedirs(os.path.join(tmp, "crates", "rustty-font-shaper", "src"))
        zig_src = {
            "src/terminal/modes.zig": (
                "pub const ModeState = struct {\n"
                "    values: u8,\n\n"
                "    pub fn get(self: *ModeState, mode: Mode) bool {\n"
                "        return true;\n    }\n\n"
                "    pub fn getDefault(self: *const ModeState, mode: Mode) bool {\n"
                "        return false;\n    }\n};\n\n"
                "pub fn modeFromInt(v: u16, ansi: bool) ?Mode {\n    return null;\n}\n"),
        }
        hb = "impl Language {\n    pub fn get_default() -> Language { Language }\n}\n"
        modes_rs = ("//! modes (terminal/modes.zig)\n"
                    "pub struct ModeState { values: u8 }\n"
                    "impl ModeState {\n    pub fn get(&self) -> bool { true }\n}\n"
                    "pub fn mode_from_int(v: u16, ansi: bool) -> Option<Mode> { None }\n")

        def write(rel, text):
            with open(os.path.join(tmp, rel), "w") as fh:
                fh.write(text)

        def gaps():
            idx = build_symbol_index(tmp)
            return analyze(list(zig_src), zig_src.get, idx, set())[1]

        failures = []
        write("crates/rustty-font-shaper/src/hb.rs", hb)
        write("crates/rustty-vt/src/modes.rs", modes_rs)
        # [a] the old match (any token anywhere) calls getDefault present.
        idx = build_symbol_index(tmp)
        old_present = "get_default" in idx[1]
        g = gaps()
        print(f"[a] getDefault only as hb.rs Language::get_default: gaps {g} "
              f"(expect getDefault); the bare-name match would say present: {old_present}")
        if g != [("src/terminal/modes.zig", "getDefault")] or not old_present:
            failures.append("a")
        # [b] ported in the counterpart file: present.
        write("crates/rustty-vt/src/modes.rs",
              modes_rs.replace("    pub fn get(&self)", "    pub fn get_default(&self) -> bool { false }\n"
                                                       "    pub fn get(&self)"))
        g = gaps()
        print(f"[b] get_default in modes.rs: gaps {g} (expect [])")
        if g:
            failures.append("b")
        # [c] ported in another file that names the container: present.
        write("crates/rustty-vt/src/modes.rs", modes_rs)
        write("crates/rustty-vt/src/mode_defaults.rs",
              "impl super::ModeState {\n    pub fn get_default(&self) -> bool { false }\n}\n")
        g = gaps()
        print(f"[c] get_default in a file naming ModeState: gaps {g} (expect [])")
        if g:
            failures.append("c")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    if failures:
        print(f"SELF-TEST RED: arm(s) {failures} failed")
        return 1
    print("SELF-TEST GREEN: the container-aware match rejects the getDefault false positive")
    return 0


def main():
    argv = sys.argv[1:]
    if "--self-test" in argv:
        return self_test()
    only = argv[argv.index("--file") + 1] if "--file" in argv else None
    limit = int(argv[argv.index("--limit") + 1]) if "--limit" in argv else 60

    nonports_path = os.path.join(ROOT, ".port/phase-3/NON_PORTS.md")
    nonported = set()
    if os.path.exists(nonports_path):
        nonported = set(re.findall(r"`(src/[\w/]+\.zig)`",
                                   open(nonports_path, errors="ignore").read()))

    files = oracle_files()
    zig = [f for f in files if f.endswith(".zig")
           and not any(f.startswith(p) for p in SKIP_PREFIXES)
           and f not in nonported]
    if only:
        zig = [f for f in zig if f == only]

    print(f"indexing port symbols…", file=sys.stderr)
    index = build_symbol_index()

    # Absences ARGUED in writing. Same standard as G-FILES: an absence is
    # acceptable when someone wrote down why, never merely because it is old.
    acc_path = os.path.join(ROOT, ".port/phase-3/SYMBOL_AUDIT.md")
    accepted = set()
    if os.path.exists(acc_path):
        accepted = set(re.findall(r"`(src/[\w/]+\.zig::[A-Za-z_][A-Za-z0-9_]*)`",
                                  open(acc_path, errors="ignore").read()))

    def read_src(rel):
        try:
            return open(os.path.join(ORACLE, rel), errors="ignore").read()
        except OSError:
            return None

    total, gaps, acked = analyze(zig, read_src, index, accepted)

    for rel, name in gaps[:limit]:
        print(f"ABSENT  {rel:52} pub fn {name}")
    if len(gaps) > limit:
        print(f"… and {len(gaps) - limit} more (raise --limit)")

    print(f"\nSYMBOL AUDIT: {total} oracle `pub fn` checked across {len(zig)} files; "
          f"{len(gaps)} unexplained, {len(acked)} accepted with a written reason")
    if total < 500:
        print(f"RED (VACUOUS): only {total} symbols checked; expected thousands.")
        return 1
    if gaps:
        print("RED: an oracle `pub fn` has no counterpart and no SYMBOL_AUDIT.md entry.")
        return 1
    print("GREEN: every oracle `pub fn` has a counterpart or a written reason.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
