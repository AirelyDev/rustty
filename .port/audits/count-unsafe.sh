#!/usr/bin/env bash
# count-unsafe.sh — D7 unsafe census (BUN_PORT_LEARNINGS §4-D7; PORT_RULES R9 draft).
#
# Ground truth is ONE committed rg command (Bun's bun-unsafe-audit method):
#
#   rg --no-config -n --column -o \
#      'unsafe \{|unsafe fn|unsafe extern|unsafe impl|unsafe trait' \
#      --glob '*.rs' --glob '!**/vendor/**' --glob '!**/cbits/**' \
#      crates crates-sys xtask
#
# Engine note: when the `rg` binary is installed, the script RUNS that exact
# command and the census total MUST equal its match count (hard failure on
# mismatch). When rg is absent (this dev box has no system ripgrep), a Python
# matcher applies the identical pattern to the identical file set — same
# line-oriented regex, same globs — so the committed command above remains the
# reproducible ground truth on any machine that has rg.
#
# Everything downstream (per-crate density per kLOC split by R6 tier, category
# taxonomy, block line-span distribution) aggregates the pattern's matches and
# MUST reconcile exactly to the total — the script exits non-zero otherwise.
#
# File set: workspace Rust sources under crates/, crates-sys/, xtask/.
#   - **/vendor/** and **/cbits/** are excluded entirely: vendored upstream
#     source (harfbuzz's in-tree Rust, stb, cbits C shims) is not port-authored.
#   - crates-sys/* IS counted, but in its own "sys" tier — never mixed into
#     the pure-Rust tiers (D7: vendored C source and crates-sys are excluded
#     from the pure-Rust tier).
#
# R6 tiers (crate → lane):
#   - ffi-asan : the FFI_CRATES list, read live from
#                .port/tools/gates/check_leaks.sh (single source of truth).
#   - sys      : every crates-sys/* crate (FFI by construction).
#   - pure-miri: every other crates/* crate + xtask.
#
# Snapshot: .port/audits/unsafe-census-<shortsha>.json, pinned to HEAD.
# Monotone rule (R9): per-crate density may only decrease or stay flat between
# snapshots; an increase requires a written origin/dest justification in the
# introducing change. Cadence: snapshot at every sub-project finale.
#
# SAFETY-tag accounting: reported per crate (sites whose same/preceding lines
# carry `SAFETY(`), ADVISORY ONLY until the wf-safety-tags retro-tagging pass
# (plan T17 step 2, deferred to the detectors pass) lands; then
# --enforce-safety-tags becomes the failing lint of D7.
#
# Usage:
#   .port/audits/count-unsafe.sh                     # snapshot to default path
#   .port/audits/count-unsafe.sh out.json            # snapshot to custom path
#   .port/audits/count-unsafe.sh --enforce-safety-tags [out.json]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
cd "$ROOT"

ENFORCE_TAGS=0
if [ "${1:-}" = "--enforce-safety-tags" ]; then ENFORCE_TAGS=1; shift; fi

PATTERN='unsafe \{|unsafe fn|unsafe extern|unsafe impl|unsafe trait'
SHORTSHA="$(git rev-parse --short HEAD)"
FULLSHA="$(git rev-parse HEAD)"
OUT="${1:-.port/audits/unsafe-census-${SHORTSHA}.json}"

# R6 FFI tier: read live from the gate runner so the census can never drift
# from the lane routing (check_leaks.sh is the R6 ground truth).
FFI_CRATES="$(sed -n 's/^FFI_CRATES="\(.*\)"$/\1/p' .port/tools/gates/check_leaks.sh)"
if [ -z "$FFI_CRATES" ]; then
  echo "FATAL: could not extract FFI_CRATES from .port/tools/gates/check_leaks.sh" >&2
  exit 1
fi

# Ground-truth cross-check: if rg is installed, run THE exact committed
# command and pin the census to its total.
RG_TOTAL=""
if command -v rg >/dev/null 2>&1; then
  RG_TOTAL="$(rg --no-config -n --column -o \
     'unsafe \{|unsafe fn|unsafe extern|unsafe impl|unsafe trait' \
     --glob '*.rs' --glob '!**/vendor/**' --glob '!**/cbits/**' \
     crates crates-sys xtask | wc -l | tr -d ' ')"
fi

OUT="$OUT" SHORTSHA="$SHORTSHA" FULLSHA="$FULLSHA" PATTERN="$PATTERN" \
FFI_CRATES="$FFI_CRATES" ENFORCE_TAGS="$ENFORCE_TAGS" RG_TOTAL="$RG_TOTAL" \
python3 - <<'PYEOF'
import json, os, re, sys, datetime
from collections import defaultdict

out_path = os.environ["OUT"]
ffi_crates = set(os.environ["FFI_CRATES"].split())
enforce_tags = os.environ["ENFORCE_TAGS"] == "1"
rg_total = os.environ["RG_TOTAL"]

# The identical pattern to the committed rg command (line-oriented, no flags).
PAT = re.compile(r"unsafe \{|unsafe fn|unsafe extern|unsafe impl|unsafe trait")
CATEGORY = {
    "unsafe {":      "unsafe_block",
    "unsafe fn":     "unsafe_fn",
    "unsafe extern": "unsafe_extern",
    "unsafe impl":   "unsafe_impl",
    "unsafe trait":  "unsafe_trait",
}
CATS = list(CATEGORY.values())
ROOTS = ["crates", "crates-sys", "xtask"]
EXCLUDE_DIRS = {"vendor", "cbits"}   # == --glob '!**/vendor/**' '!**/cbits/**'

def census_files():
    for root in ROOTS:
        for dirpath, dirnames, filenames in os.walk(root):
            dirnames[:] = sorted(d for d in dirnames if d not in EXCLUDE_DIRS)
            for f in sorted(filenames):
                if f.endswith(".rs"):
                    yield os.path.join(dirpath, f)

def crate_of(path):
    parts = path.split(os.sep)
    if parts[0] == "crates":      return parts[1], None   # tier decided below
    if parts[0] == "crates-sys":  return parts[1], "sys"
    if parts[0] == "xtask":       return "xtask", "pure-miri"
    raise SystemExit(f"FATAL: path outside census roots: {path}")

def tier_of(crate, forced):
    if forced: return forced
    return "ffi-asan" if crate in ffi_crates else "pure-miri"

# ---- comment/string/char-literal mask (span computation only; totals stay
# ---- pinned to the raw pattern matches regardless of this analysis) ---------
CHAR_LIT = re.compile(r"'(?:\\(?:x[0-9a-fA-F]{2}|u\{[0-9a-fA-F_]{1,6}\}|.)|[^'\\\n])'")
RAW_STR  = re.compile(r'(?:r|br|rb)(#*)"')
def code_mask(text):
    n = len(text); mask = bytearray(b"\x01") * n
    i = 0
    while i < n:
        c = text[i]
        if c == "/" and i + 1 < n and text[i+1] == "/":
            j = text.find("\n", i); j = n if j < 0 else j
            mask[i:j] = b"\x00" * (j - i); i = j
        elif c == "/" and i + 1 < n and text[i+1] == "*":
            depth, j = 1, i + 2
            while j < n and depth:
                if text[j:j+2] == "/*":   depth += 1; j += 2
                elif text[j:j+2] == "*/": depth -= 1; j += 2
                else: j += 1
            mask[i:j] = b"\x00" * (j - i); i = j
        elif c in "rb" and (i == 0 or not (text[i-1].isalnum() or text[i-1] == "_")):
            m = RAW_STR.match(text, i)
            if m:
                close = '"' + "#" * len(m.group(1))
                j = text.find(close, m.end())
                j = n if j < 0 else j + len(close)
                mask[i:j] = b"\x00" * (j - i); i = j
            elif c == "b" and i + 1 < n and text[i+1] in "\"'":
                i += 1  # b"..." / b'..' -> handled by the '"'/"'" arms next pass
            else:
                i += 1
        elif c == '"':
            j = i + 1
            while j < n:
                if text[j] == "\\": j += 2
                elif text[j] == '"': j += 1; break
                else: j += 1
            j = min(j, n)
            mask[i:j] = b"\x00" * (j - i); i = j
        elif c == "'":
            m = CHAR_LIT.match(text, i)
            if m:
                mask[i:m.end()] = b"\x00" * (m.end() - i); i = m.end()
            else:
                i += 1  # lifetime
        else:
            i += 1
    return mask

def block_span(text, mask, start, line):
    """Line span of the unsafe block whose match starts at byte `start`
    (1-based source line `line`). Returns (span, resolved, in_code)."""
    brace = start + len("unsafe ")
    if brace >= len(text) or text[brace] != "{":
        return (0, False, False)
    if not mask[brace]:
        return (0, True, False)  # match text sits in a comment/string
    depth = 0
    for k in range(brace, len(text)):
        if not mask[k]: continue
        ch = text[k]
        if ch == "{": depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                end_line = text.count("\n", start, k + 1) + line
                return (end_line - line + 1, True, True)
    return (0, False, True)

SAFETY_RE = re.compile(r"SAFETY\(")

# ---- census -------------------------------------------------------------------
per_crate = defaultdict(lambda: {"sites": 0, "loc": 0, "safety_tagged": 0,
                                 "by_category": {c: 0 for c in CATS}})
crate_tier = {}
total = 0
by_cat_total = defaultdict(int)
spans = []          # line spans of resolved, in-code unsafe blocks
non_code = 0        # pattern matches sitting inside comments/strings (still counted)
unresolved = 0      # blocks whose closing brace the masker could not find

for path in census_files():
    text = open(path, encoding="utf-8", errors="replace").read()
    lines = text.splitlines()
    crate, forced = crate_of(path)
    crate_tier.setdefault(crate, tier_of(crate, forced))
    rec = per_crate[crate]
    rec["loc"] += len(lines)
    if "unsafe" not in text:
        continue
    mask = None
    offset = 0
    for lineno, line_text in enumerate(lines, 1):
        for m in PAT.finditer(line_text):
            cat = CATEGORY[m.group(0)]
            rec["sites"] += 1; rec["by_category"][cat] += 1
            total += 1; by_cat_total[cat] += 1
            lo = max(0, lineno - 4)   # same line + up to 3 preceding lines
            if any(SAFETY_RE.search(lines[k]) for k in range(lo, lineno)):
                rec["safety_tagged"] += 1
            if cat == "unsafe_block":
                if mask is None:
                    mask = code_mask(text)
                span, resolved, in_code = block_span(text, mask, offset + m.start(), lineno)
                if not in_code and resolved: non_code += 1
                elif not resolved: unresolved += 1
                else: spans.append(span)
        offset += len(line_text) + 1

# ---- reconciliation (D7: categories sum EXACTLY to the total) -------------------
assert sum(by_cat_total.values()) == total, "category sum != total"
assert sum(r["sites"] for r in per_crate.values()) == total, "crate sum != total"
blocks = by_cat_total["unsafe_block"]
assert len(spans) + non_code + unresolved == blocks, "span partition != block count"
if rg_total:
    if int(rg_total) != total:
        print(f"FATAL: python matcher total {total} != committed rg command total {rg_total}",
              file=sys.stderr)
        sys.exit(1)

def bucket(span):
    if span == 1: return "1"
    if span <= 5: return "2-5"
    if span <= 20: return "6-20"
    if span <= 50: return "21-50"
    return "51+"
buckets = defaultdict(int)
for s in spans: buckets[bucket(s)] += 1

tiers = defaultdict(lambda: {"crates": 0, "sites": 0, "loc": 0,
                             "by_category": {c: 0 for c in CATS}})
rows = []
for crate in sorted(per_crate, key=lambda c: -(per_crate[c]["sites"] / max(per_crate[c]["loc"], 1))):
    rec = per_crate[crate]; tier = crate_tier[crate]
    t = tiers[tier]
    t["crates"] += 1; t["sites"] += rec["sites"]; t["loc"] += rec["loc"]
    for c in CATS: t["by_category"][c] += rec["by_category"][c]
    rows.append({
        "crate": crate, "tier": tier, "loc": rec["loc"], "sites": rec["sites"],
        "density_per_kloc": round(rec["sites"] * 1000.0 / rec["loc"], 2) if rec["loc"] else 0.0,
        "by_category": rec["by_category"],
        "safety_tagged": rec["safety_tagged"],
    })
for t in tiers.values():
    t["density_per_kloc"] = round(t["sites"] * 1000.0 / t["loc"], 2) if t["loc"] else 0.0

total_loc = sum(r["loc"] for r in per_crate.values())
tagged_total = sum(r["safety_tagged"] for r in per_crate.values())
snapshot = {
    "commit": os.environ["SHORTSHA"],
    "commit_full": os.environ["FULLSHA"],
    "generated": datetime.date.today().isoformat(),
    "tool": ".port/audits/count-unsafe.sh",
    "pattern": os.environ["PATTERN"],
    "engine": "rg-verified" if rg_total else "python-fallback (identical pattern/file set; install rg to cross-check)",
    "file_set": "crates/**/*.rs + crates-sys/**/*.rs + xtask/**/*.rs, "
                "excluding **/vendor/** and **/cbits/** (vendored upstream source)",
    "tier_rule": "ffi-asan = FFI_CRATES from .port/tools/gates/check_leaks.sh; "
                 "sys = crates-sys/* (never mixed into pure-Rust tiers); "
                 "pure-miri = every other crate",
    "totals": {
        "sites": total,
        "by_category": dict(by_cat_total),
        "loc": total_loc,
        "density_per_kloc": round(total * 1000.0 / total_loc, 2) if total_loc else 0.0,
        "reconciles": True,
        "safety_tagged": tagged_total,
        "safety_tag_enforcement": "advisory until wf-safety-tags (T17 step 2) lands",
    },
    "by_tier": {k: tiers[k] for k in sorted(tiers)},
    "crates": rows,
    "block_span_distribution": {
        "blocks": blocks,
        "single_line": buckets.get("1", 0),
        "structural": blocks - buckets.get("1", 0) - non_code - unresolved,
        "buckets": {k: buckets.get(k, 0) for k in ["1", "2-5", "6-20", "21-50", "51+"]},
        "max_span": max(spans) if spans else 0,
        "in_comment_or_string": non_code,
        "unresolved": unresolved,
    },
}
with open(out_path, "w", encoding="utf-8") as f:
    json.dump(snapshot, f, indent=2)
    f.write("\n")

# ---- human summary ------------------------------------------------------------
print(f"unsafe census @ {snapshot['commit']} -> {out_path}  [{snapshot['engine']}]")
print(f"  total sites: {total}  (loc {total_loc}, {snapshot['totals']['density_per_kloc']}/kLOC)")
print(f"  by category: " + ", ".join(f"{c}={by_cat_total[c]}" for c in CATS))
for tname in sorted(tiers):
    t = tiers[tname]
    print(f"  tier {tname:9s}: {t['sites']:6d} sites / {t['loc']:7d} loc = {t['density_per_kloc']:7.2f}/kLOC ({t['crates']} crates)")
print(f"  block spans: single-line={snapshot['block_span_distribution']['single_line']} "
      f"structural={snapshot['block_span_distribution']['structural']} "
      f"max={snapshot['block_span_distribution']['max_span']} "
      f"in-comment/string={non_code} unresolved={unresolved}")
print("  top density:")
for r in rows[:8]:
    print(f"    {r['crate']:22s} {r['tier']:9s} {r['sites']:6d} sites {r['density_per_kloc']:8.2f}/kLOC tagged={r['safety_tagged']}")
print(f"  SAFETY(origin/dest) tags: {tagged_total}/{total} (ADVISORY until T17 step 2 retro-tagging)")
if enforce_tags and tagged_total < total:
    print(f"FAIL: --enforce-safety-tags: {total - tagged_total} unsafe sites lack a SAFETY(origin/dest) tag", file=sys.stderr)
    sys.exit(1)
PYEOF
