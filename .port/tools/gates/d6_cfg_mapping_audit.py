#!/usr/bin/env python3
"""d6_cfg_mapping_audit.py — detector D6 parts (b) + (c). Kills Bun class 5
(oven-sh/bun#30875): a flattened comptime build switch that blinded the ASan/LSan
lane.

Driven by .port/tools/gates/check_sanitizer_canary.sh (the D6 gate). Two checks:

(b) CFG-MAPPING — every Zig `builtin.mode` / build-option / comptime build switch
    must port to a Rust `cfg`, NEVER a constant-folded value. The exact #30875
    mechanism was a build-mode decision (mimalloc on/off) frozen to a constant so
    the sanitizer never saw the allocator. BINDING first-party check: any
    `#[global_allocator]` in a first-party crate (crates/, crates-sys/, excluding
    vendored C/Rust) must be either
        * cfg-gated  (so a sanitizer build can disable it — e.g.
          `#[cfg_attr(not(sanitize = "address"), global_allocator)]`), or
        * carry an adjacent `// D6-ALLOW(<why>)` justification.
    An un-gated, unexplained first-party global allocator is the #30875 shape and
    FAILS. (Informational: an oracle census of `builtin.mode`/`builtin.is_test`/
    build-option switches the port must eventually map to cfg — advisory only,
    since full per-switch verification needs the port complete.)

(c) SUPPRESSION WHY — every sanitizer suppression ENTRY needs a per-entry WHY:
        * each suppression entry of a `*.supp` file — a LINE in the LSan/TSan
          one-per-line format, a `{ … }` BLOCK in the Valgrind format;
        * each suppressive sanitizer-runtime toggle in the gate scripts
          (`detect_leaks=0`, `halt_on_error=0`, `detect_odr_violation=0`,
          `*_OPTIONS=…suppressions=…`);
    must have an explanatory comment on the same line (or, for `.supp` entries,
    above the entry / its comment-introduced group). An unexplained suppression
    FAILS — an unexplained `detect_leaks=0` silently disables the very leak
    detection D6 exists to protect. Block entries that predate the rule and come
    verbatim from the pinned oracle are frozen by content hash in
    `d6_supp_baseline.txt`; see `_load_supp_baseline()`.

Exit: 0 GREEN (no findings), 1 RED (>=1 finding). Oracle absence only skips the
advisory census; the binding first-party checks always run.
"""
import hashlib
import os
import re
import sys

ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", "..", ".."))
ORACLE = "/Users/gael/projects/ghostty"

RED = "\033[31m" if sys.stdout.isatty() else ""
GRN = "\033[32m" if sys.stdout.isatty() else ""
RST = "\033[0m" if sys.stdout.isatty() else ""

findings = []  # (kind, path:line, message)


def rel(p):
    return os.path.relpath(p, ROOT)


def is_vendored(path):
    # Vendored third-party sources are out of scope for a fidelity audit of the
    # PORT: they carry their own allocators/switches we don't govern.
    parts = path.replace("\\", "/").split("/")
    return any(seg in ("vendor", "third_party", "target", ".git") for seg in parts)


def first_party_rust_files():
    for base in ("crates", "crates-sys"):
        root = os.path.join(ROOT, base)
        if not os.path.isdir(root):
            continue
        for dirpath, dirnames, filenames in os.walk(root):
            dirnames[:] = [d for d in dirnames if d not in ("target", ".git")]
            for fn in filenames:
                if fn.endswith(".rs"):
                    p = os.path.join(dirpath, fn)
                    if not is_vendored(p):
                        yield p


# ---- (b) BINDING: first-party #[global_allocator] must be cfg-gated or WHY'd ---
GA_RE = re.compile(r"#\s*\[\s*global_allocator\s*\]")
CFG_ATTR_GA_RE = re.compile(r"#\s*\[\s*cfg_attr\s*\(.*global_allocator", re.S)
D6_ALLOW_RE = re.compile(r"//.*D6-ALLOW\(")


def check_global_allocators():
    scanned = 0
    for p in first_party_rust_files():
        try:
            lines = open(p, encoding="utf-8", errors="replace").read().splitlines()
        except OSError:
            continue
        for i, line in enumerate(lines):
            m = GA_RE.search(line)
            if not m:
                continue
            # `#[global_allocator]` NAMED INSIDE A COMMENT is prose, not an
            # attribute — the PORT NOTE in rustty-renderer/src/link.rs that
            # explains why the test uses a counting allocator was being reported
            # as a second, un-gated allocator site.
            code = line.split("//", 1)[0]
            if m.start() >= len(code):
                continue
            scanned += 1
            # cfg_attr(..., global_allocator) is a proper cfg-gated allocator.
            window = "\n".join(lines[max(0, i - 1):i + 1])
            if CFG_ATTR_GA_RE.search(window):
                continue
            # A D6-ALLOW(<why>) anywhere in the CONTIGUOUS comment block directly
            # above is an explicit waiver. (Scanning a fixed 3-line window
            # rejected a waiver whose justification ran longer than 3 lines —
            # which pushed authors toward terser, worse justifications.)
            j = i - 1
            while j >= 0 and lines[j].lstrip().startswith("//"):
                j -= 1
            ctx = "\n".join(lines[j + 1:i + 1])
            if D6_ALLOW_RE.search(ctx):
                continue
            findings.append((
                "cfg-mapping",
                f"{rel(p)}:{i + 1}",
                "first-party #[global_allocator] is neither cfg-gated nor "
                "D6-ALLOW(<why>)-justified — a constant-folded allocator choice "
                "is the exact #30875 shape that blinds ASan/LSan.",
            ))
    return scanned


# ---- (b) ADVISORY: oracle census of build-mode / comptime switches ------------
CENSUS_PATTERNS = [
    re.compile(r"\bbuiltin\.mode\b"),
    re.compile(r"\bbuiltin\.is_test\b"),
    re.compile(r"\bbuiltin\.sanitize_thread\b"),
    re.compile(r"@import\(\"build_options\"\)"),
]


def oracle_census():
    src = os.path.join(ORACLE, "src")
    if not os.path.isdir(src):
        print("  (advisory) oracle absent — build-mode/comptime census skipped "
              "(binding first-party checks still ran)")
        return
    counts = {}
    for dirpath, dirnames, filenames in os.walk(src):
        dirnames[:] = [d for d in dirnames if d not in ("zig-cache", ".zig-cache")]
        for fn in filenames:
            if not fn.endswith(".zig"):
                continue
            try:
                txt = open(os.path.join(dirpath, fn), encoding="utf-8",
                           errors="replace").read()
            except OSError:
                continue
            for pat in CENSUS_PATTERNS:
                n = len(pat.findall(txt))
                if n:
                    counts[pat.pattern] = counts.get(pat.pattern, 0) + n
    total = sum(counts.values())
    print(f"  (advisory) oracle build-mode/comptime switches to map to cfg: "
          f"{total} occurrence(s)")
    for pat, n in sorted(counts.items(), key=lambda kv: -kv[1]):
        print(f"      {n:5d}  {pat}")
    print("      NOTE: each must port as a `cfg`, never a constant-folded value "
          "(D6 rule b). Binding per-switch verification lands as crates complete.")


# ---- (c) suppression entries need a per-entry WHY -----------------------------
# Suppressive sanitizer-runtime toggles: switching one ON disables a detector.
SUPPRESSIVE_TOGGLE = re.compile(
    r"\b(detect_leaks\s*=\s*0"
    r"|halt_on_error\s*=\s*0"
    r"|detect_odr_violation\s*=\s*0"
    r"|abort_on_error\s*=\s*0"
    r"|suppressions\s*=)")
# A WHY = a shell/py comment on the SAME line explaining the toggle.
SAME_LINE_WHY = re.compile(r"#[^\n]*[A-Za-z]")


def gate_script_files():
    # `.port/tools` already contains `gates/`; walking it once avoids visiting
    # every gate script twice (which would double-report any finding).
    tools = os.path.join(ROOT, ".port", "tools")
    if not os.path.isdir(tools):
        return
    for dirpath, dirnames, filenames in os.walk(tools):
        dirnames[:] = [x for x in dirnames if x not in ("__pycache__", "target")]
        for fn in filenames:
            if fn.endswith((".sh", ".py")):
                yield os.path.join(dirpath, fn)


def check_suppression_toggles():
    me = os.path.abspath(__file__)
    for p in gate_script_files():
        if os.path.abspath(p) == me:
            continue  # this audit describes toggles in prose; don't self-flag
        try:
            lines = open(p, encoding="utf-8", errors="replace").read().splitlines()
        except OSError:
            continue
        for i, line in enumerate(lines):
            m = SUPPRESSIVE_TOGGLE.search(line)
            if not m:
                continue
            # A `#` BEFORE the match means the toggle sits inside a comment /
            # prose (documentation, not a live setting) — never a suppression.
            if "#" in line[:m.start()]:
                continue
            # For a live toggle, WHY must be a comment AFTER it on the same line.
            after = line[m.end():]
            if SAME_LINE_WHY.search(after):
                continue
            findings.append((
                "suppression-why",
                f"{rel(p)}:{i + 1}",
                f"sanitizer suppression toggle `{m.group(1).strip()}` has no "
                "per-entry WHY comment on its line — an unexplained suppression "
                "silently disables the detector D6 protects (rule c).",
            ))


# A `.supp` file comes in one of two shapes, and rule (c) counts ENTRIES, not
# lines, in both:
#
#   * LINE format (LSan / TSan / UBSan): one suppression per line —
#     `leak:pattern`, `race:pattern`, `interceptor_via_fun:foo`, … Here an entry
#     IS a line, so the per-line WHY check is the entry check.
#
#   * BLOCK format (Valgrind Memcheck): an entry is a `{ … }` block whose body is
#     a name, a `Memcheck:*` kind and a call-stack of `fun:`/`obj:` frames. The
#     ORIGINAL implementation of this check was the LINE rule applied to every
#     non-comment line, so a single 15-frame Valgrind entry was reported as ~15
#     unexplained "entries" — `valgrind.supp` (233 entries) produced 2048
#     findings and D6 sat permanently RED, i.e. ignorable. That was a bug in the
#     checker, not 2048 unexplained suppressions. Block files are now parsed as
#     blocks.
#
# For a block entry the WHY may be a comment INSIDE the block, or the comment
# paragraph that introduces the entry's group — that grouping ("# Reproduction:
# 1. Launch Rustty … 5. Exit" over the two entries it explains) is how the file
# is actually written. A group ends at the first blank line AFTER an entry, so
# appending an undocumented entry to the end of a file is still a finding: the
# FAIL arm stays reachable (proved by `check_sanitizer_canary.sh --self-test`).
BLOCK_SUPP_KIND = re.compile(r"^\s*(Memcheck|Helgrind|DRD|exp-sgcheck):")


def _norm_entry(body):
    """Content identity of a block entry: its frames, whitespace-normalized."""
    return hashlib.sha256(
        "\n".join(l.strip() for l in body if l.strip()).encode()
    ).hexdigest()[:16]


def _load_supp_baseline():
    """Hashes of block entries inherited verbatim from the pinned oracle.

    `valgrind.supp` is an R1-verbatim port of the oracle's file (233 entries,
    126 comment lines — identical counts on both sides), and upstream documents
    its suppressions in GROUPS, leaving 148 entries with no WHY of their own.
    Inventing 148 rationales for third-party GTK/GLib frames we did not diagnose
    would be fabrication, so those exact entries are FROZEN by content hash.
    The freeze is not a waiver of the rule: a NEW entry, or an EDIT to a frozen
    one (the hash covers every frame), is not on the list and FAILS. The debt can
    only shrink — `--print-supp-baseline` regenerates the list and any hash that
    no longer appears in a file is reported as stale.
    """
    path = os.path.join(os.path.dirname(__file__), "d6_supp_baseline.txt")
    out = {}
    if not os.path.isfile(path):
        return out
    for line in open(path, encoding="utf-8"):
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        h, _, name = line.partition(" ")
        out[h] = name
    return out


def iter_supp_entries(lines):
    """Yield (start_line, body_lines, has_why) for each block-format entry."""
    group_why = False
    seen_entry = False
    i, n = 0, len(lines)
    while i < n:
        s = lines[i].strip()
        if s.startswith("#") and re.search(r"[A-Za-z]", s):
            group_why, seen_entry = True, False
            i += 1
            continue
        if not s:
            # A blank line ends a group only once the group has produced an
            # entry, so `# why` / blank / `{ … }` still counts as documented.
            if seen_entry:
                group_why, seen_entry = False, False
            i += 1
            continue
        if s == "{":
            start, body = i, []
            i += 1
            while i < n and lines[i].strip() != "}":
                body.append(lines[i])
                i += 1
            i += 1
            seen_entry = True
            inline = any(l.strip().startswith("#") and re.search(r"[A-Za-z]", l)
                         for l in body)
            yield start + 1, body, (group_why or inline)
            continue
        i += 1


def check_supp_files(print_baseline=False, check_stale=True):
    baseline = _load_supp_baseline()
    seen_hashes = set()
    frozen = 0
    regen = []
    for dirpath, dirnames, filenames in os.walk(ROOT):
        dirnames[:] = [d for d in dirnames if d not in ("target", ".git", "vendor")]
        for fn in sorted(filenames):
            if not fn.endswith(".supp"):
                continue
            p = os.path.join(dirpath, fn)
            lines = open(p, encoding="utf-8", errors="replace").read().splitlines()
            if any(l.strip() == "{" for l in lines):
                # BLOCK format (Valgrind & friends): one finding per ENTRY.
                for ln, body, has_why in iter_supp_entries(lines):
                    h = _norm_entry(body)
                    seen_hashes.add(h)
                    name = body[0].strip() if body else "<unnamed>"
                    if BLOCK_SUPP_KIND.match(name) or not name:
                        name = "<unnamed>"
                    if has_why:
                        continue
                    if print_baseline:
                        regen.append(f"{h} {rel(p)}: {name}")
                        continue
                    if h in baseline:
                        frozen += 1
                        continue
                    findings.append((
                        "suppression-why",
                        f"{rel(p)}:{ln}",
                        f"suppression entry `{name}` has no per-entry WHY comment "
                        "and is not in d6_supp_baseline.txt (rule c). Add a `#` "
                        "comment above the entry (or inside the block) saying what "
                        "it hides and why that cannot hide a rustty defect.",
                    ))
                continue
            # LINE format (LSan/TSan/UBSan): one suppression per line.
            for i, line in enumerate(lines):
                s = line.strip()
                if not s or s.startswith("#"):
                    continue
                has_inline = "#" in line and SAME_LINE_WHY.search(line[line.index("#"):])
                prev = lines[i - 1].strip() if i > 0 else ""
                has_above = prev.startswith("#") and re.search(r"[A-Za-z]", prev)
                if has_inline or has_above:
                    continue
                findings.append((
                    "suppression-why",
                    f"{rel(p)}:{i + 1}",
                    "suppression entry has no per-entry WHY comment (rule c).",
                ))
    if print_baseline:
        print("# d6_supp_baseline.txt — regenerated by "
              "`d6_cfg_mapping_audit.py --print-supp-baseline`.")
        print("# See _load_supp_baseline() for what this freeze does and does not "
              "waive.")
        for r in regen:
            print(r)
        return 0
    stale = [h for h in baseline if h not in seen_hashes] if check_stale else []
    for h in stale:
        findings.append((
            "suppression-why",
            "d6_supp_baseline.txt",
            f"stale freeze `{h}` ({baseline[h]}) matches no suppression entry any "
            "more — the entry was edited or removed; drop the line (the debt only "
            "shrinks).",
        ))
    if frozen:
        print(f"  (c) {frozen} block suppression entr(ies) frozen by "
              f"d6_supp_baseline.txt (oracle-verbatim, no upstream WHY); a new or "
              f"edited entry is NOT frozen and fails.")
    return 0


# ---- self-test: prove every arm of (b)+(c) can still turn RED ----------------
SELF_TEST_CASES = [
    ("un-gated first-party #[global_allocator]",
     {"crates/x/src/lib.rs": "#[global_allocator]\nstatic A: X = X;\n"},
     "cfg-mapping"),
    ("NEW undocumented Valgrind block entry",
     {"t.supp": "# why: documented one\n{\n  a\n  Memcheck:Leak\n  fun:g_malloc\n}"
                "\n\n{\n  undocumented\n  Memcheck:Leak\n  fun:g_free\n}\n"},
     "suppression-why"),
    ("EDITED frozen Valgrind entry (hash no longer matches)",
     {"t.supp": "{\n  a\n  Memcheck:Leak\n  fun:g_malloc_EDITED\n}\n"},
     "suppression-why"),
    ("undocumented LSan line suppression",
     {"t.supp": "leak:libfoo.so\n"},
     "suppression-why"),
    ("un-WHY'd detect_leaks=0 in a gate script",
     {".port/tools/gates/x.sh": "ASAN_OPTIONS=detect_leaks=0\n"},
     "suppression-why"),
]


def self_test():
    """Every FAIL arm must be reachable, and the clean tree must be GREEN."""
    import shutil
    import tempfile
    global ROOT, findings
    real_root = ROOT
    ok = True
    for name, files, kind in SELF_TEST_CASES:
        td = tempfile.mkdtemp(prefix="d6-selftest-")
        try:
            for rp, txt in files.items():
                fp = os.path.join(td, rp)
                os.makedirs(os.path.dirname(fp), exist_ok=True)
                open(fp, "w").write(txt)
            ROOT, findings = td, []
            check_global_allocators()
            check_suppression_toggles()
            check_supp_files(check_stale=False)
            hit = [f for f in findings if f[0] == kind]
            print(f"  {'✅' if hit else '❌'} FAIL arm reachable: {name} "
                  f"-> {len(hit)} [{kind}] finding(s)")
            ok = ok and bool(hit)
        finally:
            shutil.rmtree(td, ignore_errors=True)
    # And the negative control: an empty tree must produce NO finding, so the
    # arms above are the rule firing, not the checker failing open on anything.
    td = tempfile.mkdtemp(prefix="d6-selftest-clean-")
    try:
        open(os.path.join(td, "t.supp"), "w").write(
            "# why: documented\n{\n  a\n  Memcheck:Leak\n  fun:g_malloc\n}\n")
        ROOT, findings = td, []
        check_global_allocators()
        check_suppression_toggles()
        check_supp_files(check_stale=False)
        clean = not findings
        print(f"  {'✅' if clean else '❌'} negative control: a documented tree is "
              f"GREEN -> {len(findings)} finding(s)")
        ok = ok and clean
    finally:
        shutil.rmtree(td, ignore_errors=True)
        ROOT = real_root
        findings = []
    return 0 if ok else 1


def main():
    if "--print-supp-baseline" in sys.argv:
        return check_supp_files(print_baseline=True)
    if "--self-test" in sys.argv:
        print("D6 (b)+(c) audit self-test — each arm must be able to go RED")
        return self_test()
    print("D6 (b)+(c) cfg-mapping + suppression audit")
    ga = check_global_allocators()
    print(f"  (b) first-party #[global_allocator] sites scanned: {ga} "
          f"(each must be cfg-gated or D6-ALLOW-justified)")
    oracle_census()
    check_suppression_toggles()
    check_supp_files()

    if findings:
        print(f"\n{RED}D6 AUDIT: {len(findings)} finding(s){RST}")
        for kind, loc, msg in findings:
            print(f"  {RED}✗{RST} [{kind}] {loc}\n      {msg}")
        return 1
    print(f"{GRN}  ✅ D6 audit clean: no constant-folded first-party allocator; "
          f"every suppression toggle carries a WHY.{RST}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
