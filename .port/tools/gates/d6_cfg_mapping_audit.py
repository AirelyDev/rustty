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

(c) SUPPRESSION WHY — every sanitizer suppression entry needs a per-entry WHY:
        * each non-comment line of a `*.supp` file;
        * each suppressive sanitizer-runtime toggle in the gate scripts
          (`detect_leaks=0`, `halt_on_error=0`, `detect_odr_violation=0`,
          `*_OPTIONS=…suppressions=…`);
    must have an explanatory comment on the same line (or, for `.supp`, the line
    above). An unexplained suppression FAILS — an unexplained `detect_leaks=0`
    silently disables the very leak detection D6 exists to protect.

Exit: 0 GREEN (no findings), 1 RED (>=1 finding). Oracle absence only skips the
advisory census; the binding first-party checks always run.
"""
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
            if not GA_RE.search(line):
                continue
            scanned += 1
            # cfg_attr(..., global_allocator) is a proper cfg-gated allocator.
            window = "\n".join(lines[max(0, i - 1):i + 1])
            if CFG_ATTR_GA_RE.search(window):
                continue
            # An adjacent D6-ALLOW(<why>) on the 3 lines above is an explicit waiver.
            ctx = "\n".join(lines[max(0, i - 3):i + 1])
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


def check_supp_files():
    for dirpath, dirnames, filenames in os.walk(ROOT):
        dirnames[:] = [d for d in dirnames if d not in ("target", ".git", "vendor")]
        for fn in filenames:
            if not fn.endswith(".supp"):
                continue
            p = os.path.join(dirpath, fn)
            lines = open(p, encoding="utf-8", errors="replace").read().splitlines()
            for i, line in enumerate(lines):
                s = line.strip()
                if not s or s.startswith("#"):
                    continue
                # Entry needs a WHY: trailing `#…` OR a `#` comment on the line above.
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


def main():
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
