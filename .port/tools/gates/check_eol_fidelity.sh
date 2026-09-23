#!/usr/bin/env bash
# GATE G-EOL — byte fidelity of the corpora, fixtures and vendored trees.
#
# WHY THIS EXISTS. `.gitattributes` opens with `* text=auto`, which asks git to
# DETECT text files and normalize their line endings on the way into the index.
# The byte-exact fixture families were protected by EXTENSION (`*.vt`,
# `*.golden`, `*.bin`) — but fuzz corpus seeds have no extension at all
# (`02-crlf`, `19-single-byte`, `id_000000,time_0,...`), so nothing reached them
# and `text=auto` decided.
#
# It got one. `.port/phase-0/corpora/cvt/parser-initial/02-crlf` — a seed whose
# entire purpose is to feed CRLF through the parser — was committed as
# `line1\nline2\nline3`. The reference's blob is `line1\r\nline2\r\nline3`.
#
# What makes that failure mode worth a gate rather than a one-time fix is how
# thoroughly it hides:
#
#   * `git status` reports CLEAN. The clean/smudge filter round-trips, so the
#     working tree still shows the original CRLF.
#   * The corruption therefore exists ONLY for someone who clones. It is
#     invisible on the machine that introduced it.
#   * `fuzz_corpus_replay.rs` replays these seeds from the working tree, so the
#     test exercised CRLF locally and LF on CI — green in both cases.
#
# A checksum manifest would not have caught it either: `corpora/MANIFEST.sha256`
# covers the 33 `.golden` files only, and nothing verifies it.
#
# ── WHAT IT CHECKS ───────────────────────────────────────────────────────────
# 1. Every path family that must be byte-exact actually carries a no-conversion
#    attribute (`binary` or `-text`). This is the arm that goes red if someone
#    deletes a `.gitattributes` rule.
# 2. No file in those families has an index/worktree EOL disagreement — i.e.
#    git is not silently converting them. Deliberately scoped to those families
#    rather than the whole tree so the gate is correct on Windows too, where
#    `core.autocrlf` legitimately produces `i/lf w/crlf` for ordinary sources.
# 3. When the reference checkout is reachable, the shared fuzz seeds are
#    compared against it BYTE FOR BYTE. Skipped BY NAME when absent (R3: the
#    oracle is never a build dependency), never silently passed.
#
# Exit contract (`.port/tools/CI.md`): 0 = pass, 1 = red, 3 = BLOCKED.
set -uo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
cd "$ROOT" || exit 1

blocked() { echo "G-EOL BLOCKED: $*"; exit 3; }

# The reference checkout for arm 3: `ghostty` beside the MAIN checkout, resolved
# by the shared shell resolver (the twin of `_gatelib.oracle_root`). R5: there
# is no environment override — the former `GHOSTTY_ORACLE` could point arm 3 at
# any tree — and `GIT_*` cannot redirect it. `--print-oracle` prints the result
# and exits; G-ORACLE-SH-SELF (`_oracle_root.sh --self-test`) runs a copy of
# this gate that way with the old override and a GIT_DIR decoy set.
# shellcheck source=.port/tools/gates/_oracle_root.sh
source "$ROOT/.port/tools/gates/_oracle_root.sh" || blocked "cannot source _oracle_root.sh"
ORACLE="$(rustty_oracle_root "$ROOT")"
if [ "${1:-}" = "--print-oracle" ]; then
  echo "$ORACLE"
  exit 0
fi

command -v git >/dev/null 2>&1 || blocked "git not installed"
git rev-parse --git-dir >/dev/null 2>&1 || blocked "not a git checkout"

# The families whose bytes must survive a round trip through git unchanged.
#
# These are the byte-exact SUBTREES only, never a whole top-level directory:
# `crates-sys/*/vendor` and not `crates-sys` (whose own `build.rs`/`Cargo.toml`
# are rustty source and correctly carry `text eol=lf`), `corpora/cvt` and not
# `corpora` (whose `MANIFEST.md` is ordinary markdown). Scoping it this way is
# also what keeps arm 2 correct on Windows, where `core.autocrlf` legitimately
# produces `i/lf w/crlf` for every `text=auto` source file.
FAMILIES=(
  ".port/phase-0/corpora/cvt"
  ".port/phase-0/corpora/terminfo/compiled"
  ".port/phase-0/harness/fixtures"
  # Trailing `/**` is load-bearing: as a git pathspec, `crates-sys/*/vendor`
  # matches the directory ENTRY and therefore zero files, which would make this
  # family a silent skip. Arm 1's per-family "no tracked files" report is what
  # surfaced that.
  "crates-sys/*/vendor/**"
  "dist/windows"
)

FAILED=0
RAN=0
note_fail() { echo "  FAIL    $1"; FAILED=1; }

# ── 1. the guarding attributes are in force ──────────────────────────────────
echo "=== G-EOL 1/3 :: no-conversion attributes are in force ==="
for fam in "${FAMILIES[@]}"; do
  # Sample the family's tracked files and ask git what it would do to them.
  # `while read` rather than `mapfile`: macOS ships bash 3.2, which has no
  # mapfile, and this gate runs on every leg.
  sample=()
  while IFS= read -r _f; do
    sample+=("$_f")
  done < <(git ls-files -- "$fam" | head -400)
  if [ "${#sample[@]}" -eq 0 ]; then
    echo "  skipped $fam (no tracked files)"
    continue
  fi
  # `text: unset` is `-text`; `binary` implies it. Anything reporting
  # `text: auto` or `text: set` is a file git may rewrite.
  converted="$(git check-attr text -- ${sample[@]+"${sample[@]}"} 2>/dev/null \
    | grep -c ': text: \(auto\|set\)$' || true)"
  RAN=$((RAN + 1))
  if [ "$converted" -ne 0 ]; then
    echo "      $converted of ${#sample[@]} sampled file(s) are still under line-ending conversion:"
    git check-attr text -- ${sample[@]+"${sample[@]}"} 2>/dev/null \
      | grep ': text: \(auto\|set\)$' | head -5 | sed 's/^/        /'
    echo "      add a path rule to .gitattributes (see this gate's header)"
    note_fail "$fam (unguarded)"
  else
    echo "  ok      $fam (${#sample[@]} sampled, none convertible)"
  fi
done

# ── 2. no silent index/worktree divergence in those families ─────────────────
# `git ls-files --eol` prints `i/<index-eol> w/<worktree-eol>`. For a file git
# is not allowed to convert, those two MUST agree; a disagreement is the exact
# signature of the 02-crlf bug.
echo "=== G-EOL 2/3 :: index bytes == worktree bytes ==="
mismatches="$(git ls-files --eol -- "${FAMILIES[@]}" 2>/dev/null \
  | awk '{ i = $1; w = $2; sub(/^i\//, "", i); sub(/^w\//, "", w); if (i != w) print }')"
checked="$(git ls-files --eol -- "${FAMILIES[@]}" 2>/dev/null | wc -l | tr -d ' ')"
# Anti-vacuity: an empty file list would make this arm a silent pass.
if [ "$checked" -eq 0 ]; then
  echo "G-EOL RED: no files matched the byte-exact families — this arm would check nothing." >&2
  exit 1
fi
if [ -n "$mismatches" ]; then
  echo "$mismatches" | head -20 | sed 's/^/      /'
  echo "      these blobs differ from their working-tree bytes; git is converting them."
  echo "      fix: add the path rule, then \`git add --renormalize <path>\`."
  note_fail "index/worktree EOL divergence"
else
  echo "  ok      $checked tracked file(s), no index/worktree divergence"
fi
RAN=$((RAN + 1))

# ── 3. the shared fuzz seeds match the reference byte for byte ───────────────
echo "=== G-EOL 3/3 :: seed corpora vs the reference ==="
REF_CORPUS="$ORACLE/test/fuzz-libghostty/corpus"
if [ ! -d "$REF_CORPUS" ]; then
  echo "  skipped reference byte-comparison (no oracle checkout at $ORACLE — R3, local-only)"
else
  cmp_fail=0
  cmp_ran=0
  for tier in osc-initial parser-initial stream-initial osc-cmin parser-cmin stream-cmin; do
    port_dir="$ROOT/.port/phase-0/corpora/cvt/$tier"
    ref_dir="$REF_CORPUS/$tier"
    [ -d "$port_dir" ] && [ -d "$ref_dir" ] || continue
    n=0
    bad=0
    while IFS= read -r f; do
      base="$(basename "$f")"
      if [ -f "$ref_dir/$base" ]; then
        n=$((n + 1))
        cmp -s "$f" "$ref_dir/$base" || { bad=$((bad + 1)); [ "$bad" -le 3 ] && echo "      differs: $tier/$base"; }
      fi
    done < <(find "$port_dir" -type f)
    cmp_ran=$((cmp_ran + n))
    if [ "$bad" -ne 0 ]; then
      echo "  FAIL    $tier ($bad of $n differ from the reference)"
      cmp_fail=1
    else
      echo "  ok      $tier ($n seed(s) byte-identical to the reference)"
    fi
  done
  if [ "$cmp_ran" -eq 0 ]; then
    echo "  skipped reference byte-comparison (oracle present but no shared seeds found)"
  elif [ "$cmp_fail" -ne 0 ]; then
    note_fail "seed corpora diverge from the reference"
  fi
  RAN=$((RAN + 1))
fi

echo "--- G-EOL summary: $RAN arm(s) ran ---"
if [ "$RAN" -eq 0 ]; then
  echo "G-EOL BLOCKED: nothing was checked."
  exit 3
fi
if [ "$FAILED" -ne 0 ]; then
  echo "G-EOL RED: see above." >&2
  exit 1
fi
echo "G-EOL GREEN ($RAN arm(s) clean)."
exit 0
