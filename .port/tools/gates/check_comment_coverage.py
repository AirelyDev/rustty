#!/usr/bin/env python3
"""
check_comment_coverage.py — GATE G3b: content-aware reference-comment coverage [R1].

Why this exists: G3 (check_comments_fidelity.py) compares AGGREGATE per-crate
comment-line COUNTS, so a genuinely dropped reference comment can be masked by
added PORT NOTEs (rust_total >= ref_total even though a specific reference comment
vanished). That is exactly how `rustty-build-config` dropped `/// The optimization
mode as a string.` while G3 stayed green. This gate closes that hole DETERMINISTICALLY:
every *distinctive* reference comment (a phrase/sentence, not a separator or ASCII
diagram) must have a fuzzy match somewhere in the ported crate's comments, after
applying the ghostty->rustty identity rename (R2). Clear drops are RED.

Conservative by design (catches drops, tolerates retargeting/light adaptation):
  - distinctive = >= 5 word-tokens after normalization (skips markers, separators,
    diagrams, trivially short comments).
  - covered = normalized ref comment is a substring of the port comment blob, OR
    a port comment matches it with difflib ratio >= THRESHOLD (default 0.60).
  - RED (in --strict / for a done crate) if any distinctive ref comment is below
    the MISS threshold (default 0.45 — a near-total absence, i.e. a real drop).
    Between MISS and THRESHOLD it is reported as REVIEW (soft) but not RED, to
    avoid false positives on reworded comments.

Usage: check_comment_coverage.py --crate <name> [--strict] [--self-test]
Exit 0 = GREEN/REVIEW-only ; 1 = RED (dropped reference comment) ; 2 = usage/blocked.
"""
import argparse, os, re, sys, difflib, bisect, glob as _glob
from collections import Counter

GATES = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, GATES)
import _gatelib as G  # noqa: E402

# NOT env-overridable, deliberately. This used to be
# `os.environ.get("GHOSTTY_SRC", ...)`, which is a spoof hole of exactly the
# shape `ci.sh:114-129` documents for the test-count gate: point the variable at
# an empty or fake tree and every reference file is missing, so the gate finds
# zero reference sentences everywhere and reports full coverage against nothing.
# The oracle is an EXTERNAL read-only reference (R3) at one hard-coded path;
# `P3-W10 T3` advances that real tree, which is the only supported way to change
# what this gate reads.
GHOSTTY_SRC = "/Users/gael/projects/ghostty/src"
INV = os.path.join(G.RUSTTY_ROOT, ".port", "phase-0", "inventory")
sys.path.insert(0, INV)
import census_comments as CC  # noqa: E402  (reuse string-aware find_comment_start)

# Coverage signal priority: EXACT substring match (verbatim preservation after
# ghostty->rustty retarget) is the primary "covered" signal and is threshold-free.
# Fuzzy similarity is only a fallback for lightly-adapted comments, and it is
# deliberately STRICT: coincidental cross-file matches at ~0.5 must NOT count as
# covered (that false-negative let real os drops through). A ref comment whose
# best fuzzy match is in [MISS, THRESHOLD) is REVIEW (flagged, must be vetted —
# not auto-covered); below MISS it is a hard MISSING (RED).
THRESHOLD = 0.80
MISS = 0.62
WORD_RE = re.compile(r"[a-z0-9_]{2,}")


def load_idmap():
    """ghostty->rustty token map (R2). Optional; absent => identity."""
    p = os.path.join(G.RUSTTY_ROOT, ".port", "plan", "identity-rename.map")
    pairs = []
    if os.path.exists(p):
        for line in open(p, encoding="utf-8"):
            line = line.strip()
            if not line or line.startswith("#"):
                continue
            m = re.split(r"\s*(?:=>|=)\s*", line, maxsplit=1)
            if len(m) == 2:
                pairs.append((m[0].strip().lower(), m[1].strip().lower()))
    # always normalize the bare product token so retargeting never reads as a drop
    pairs.append(("ghostty", "rustty"))
    return pairs


def strip_marker(s):
    # Zig //!, ///, // ; Rust //!, ///, //, and a leading * from /* */ blocks.
    s = s.lstrip()
    for mk in ("///", "//!", "//"):
        if s.startswith(mk):
            return s[len(mk):].strip()
    if s.startswith("*"):
        return s[1:].strip()
    return s.strip()


def norm(text, idmap):
    t = strip_marker(text).lower()
    for pat, repl in idmap:
        t = t.replace(pat, repl)
    return re.sub(r"\s+", " ", t).strip()


def words(t):
    return WORD_RE.findall(t)


def comment_bodies(path):
    out = []
    try:
        for line in open(path, encoding="utf-8", errors="replace"):
            idx = CC.find_comment_start(line)
            if idx is None:
                continue
            body = line[idx:].rstrip("\n")
            if body.strip():
                out.append(body)
    except OSError:
        pass
    return out


def crate_ref_files(crate, cm):
    return [os.path.join(GHOSTTY_SRC, f) for f, _ in cm[crate]["files"]]


def _rs_under(path):
    """Every .rs file at/under `path` (a directory or a single .rs file)."""
    if os.path.isfile(path):
        return [path] if path.endswith(".rs") else []
    out = []
    for root, _, files in os.walk(path):
        for fn in files:
            if fn.endswith(".rs"):
                out.append(os.path.join(root, fn))
    return out


def crate_rust_files(crate):
    d = G.crate_src_dir(crate)
    if not d or not os.path.isdir(d):
        return []
    return _rs_under(d)


# R4 RELOCATIONS (S-8a, 2026-09-24). The crate map partitions the ORACLE files
# by crate key, but R4 moved some of those files' ports into a DIFFERENT crate
# (to break a dependency cycle or honour a layering decision). The crate's own
# `src/` then lacks the port and every comment of the relocated file reads as a
# MISSING drop that was never dropped: `--crate font` reported 1767 of its 1795
# MISSING against code living in `rustty-font-shaper`, `--crate input` all 68
# against `rustty-vt/src/input/`. Each entry below names the EXACT extra port
# location (never a whole foreign crate when a subtree suffices) plus the
# decision that put it there. The comparison then runs against the crate's own
# `src/` PLUS these paths: the same logic, the correct port side.
#
# A reference comment still has to appear in the port that implements it, so
# this admits no comment the crate's port does not carry. A listed path that
# does not exist is BLOCKED (exit 2), never a silent fall-back (a vanished
# relocation would otherwise hide that the port moved again).
EXTRA_PORT_PATHS = {
    # D1 three-tier font split (memory `apc-glyf-font-layering-d1`): the
    # shaper/backend-face tier of src/font/ (face*, shape/shaper*, Collection,
    # CodepointResolver, DeferredFace, discovery, SharedGrid*, library, backend
    # and the main.zig glue) lives ABOVE rustty-vt in rustty-font-shaper.
    "font": ["crates/rustty-font-shaper/src"],
    # The VT slice of src/input/ (function_keys.zig, kitty.zig and the VT
    # halves of key_mods.zig/config.zig) is owned by rustty-vt (see the
    # ownership doc in crates/rustty-input/src/lib.rs).
    "input": ["crates/rustty-vt/src/input"],
    # config/key.zig's `Key` enum and `Value` fn are ported with the `Config`
    # struct they reflect over (rustty-cfg-fmt stays a near-leaf, R4 break #6).
    "cfg-fmt": ["crates/rustty-config/src/config_struct.rs"],
}


def crate_port_files(crate, extra=None):
    """The port side for `crate`: its own src/ plus its R4 relocations.
    Raises FileNotFoundError naming a relocation path that does not exist."""
    extra = EXTRA_PORT_PATHS if extra is None else extra
    out = crate_rust_files(crate)
    for rel in extra.get(crate, []):
        full = rel if os.path.isabs(rel) else os.path.join(G.RUSTTY_ROOT, rel)
        if not os.path.exists(full):
            raise FileNotFoundError(full)
        out += _rs_under(full)
    return out


def classify(ref_norm, port_norm):
    """(missing, review) for normalized reference vs port comments; `review`
    holds (comment, rounded best ratio) pairs."""
    port_blob = " \n ".join(port_norm)

    # The original inner loop is best = max over EVERY port comment of
    # difflib's SequenceMatcher(None, rc, pc).ratio(). On the 14803-loc
    # PageList.zig reference that is ~7k fuzzy ref comments x ~6k port comments
    # of full Ratcliff-Obershelp matching, which times out. We compute a result
    # IDENTICAL to the original far faster by exploiting (a) difflib's guaranteed
    # upper bound quick_ratio() and (b) the fact that this gate only needs the
    # EXACT best when it lands in [MISS, THRESHOLD) (to flag REVIEW and round its
    # ratio) or >= THRESHOLD (covered); whenever best < MISS the only observable
    # output is "MISSING", for which the exact value is irrelevant.
    #
    #   quick_ratio(rc, pc) = 2 * sum_c min(cnt_rc[c], cnt_pc[c]) / (la + lb)
    #
    # is a guaranteed upper bound on ratio() (a char present in rc but not pc can
    # never be matched, so matched chars M <= the per-char multiset minimum) and
    # is SYMMETRIC. Two consequences make the computed classification IDENTICAL:
    #   1) Any pc with quick_ratio < MISS has ratio() < MISS, so it can neither
    #      lift best to >= MISS nor be the witness of a REVIEW/covered best — it
    #      is irrelevant to every observable outcome and is skipped. (If NO pc
    #      reaches quick_ratio >= MISS, best < MISS and rc is MISSING outright,
    #      with zero ratio() calls.)
    #   2) Among the remaining pc (quick_ratio >= MISS) we compute the EXACT best
    #      via real ratio(), visiting them in descending quick_ratio order and
    #      breaking once quick_ratio <= best (sorted desc => no later pc can beat
    #      best). The resulting best is exact on [MISS, 1], so MISSING/REVIEW/
    #      covered and the rounded REVIEW ratio are byte-identical to the original.
    # ratio() is computed in the SAME orientation as the original,
    # SequenceMatcher(None, rc, pc), because ratio() is not symmetric.
    port_cnt = [Counter(pc) for pc in port_norm]
    # Port indices sorted by length (with the parallel sorted-length list) so we
    # can binary-search the only length band that can yield quick_ratio >= MISS.
    order_by_len = sorted(range(len(port_norm)), key=lambda j: len(port_norm[j]))
    sorted_lens = [len(port_norm[j]) for j in order_by_len]

    missing, review = [], []
    for rc in ref_norm:
        if rc in port_blob:
            continue
        la = len(rc)
        rc_cnt = Counter(rc)
        rc_items = list(rc_cnt.items())
        rc_get = rc_cnt.get
        lrc = len(rc_cnt)

        # Pass 1: collect only the pc whose quick_ratio upper bound reaches MISS
        # (cheap char-multiset arithmetic, no SequenceMatcher). pc below MISS are
        # provably below MISS in ratio() too and cannot affect any outcome.
        #
        # First prune by length: quick_ratio <= real_quick_ratio =
        # 2*min(la,lb)/(la+lb), so quick_ratio >= MISS requires
        # 2*min(la,lb)/(la+lb) >= MISS, i.e. lb in
        # [la*MISS/(2-MISS), la*(2-MISS)/MISS]. pc outside that contiguous band
        # have real_quick_ratio < MISS hence quick_ratio < MISS, and are skipped
        # WITHOUT the per-char sum (this is the dominant saving: most port
        # comments differ enough in length to be excluded). We binary-search the
        # band in the length-sorted order; the char-multiset test inside still
        # decides membership exactly, so the band only narrows work, never alters
        # which pc qualify.
        lo_len = la * MISS / (2.0 - MISS)
        hi_len = la * (2.0 - MISS) / MISS
        lo = bisect.bisect_left(sorted_lens, lo_len)
        hi = bisect.bisect_right(sorted_lens, hi_len)
        cands = []
        for k in range(lo, hi):
            j = order_by_len[k]
            lb = sorted_lens[k]
            denom = la + lb
            if denom == 0:
                continue
            cnt = port_cnt[j]
            if lrc <= len(cnt):
                cg = cnt.get
                m = sum(min(n, cg(c, 0)) for c, n in rc_items)
            else:
                m = sum(min(n, rc_get(c, 0)) for c, n in cnt.items())
            qr = 2.0 * m / denom
            if qr >= MISS:
                cands.append((qr, j))

        if not cands:
            # No pc can even reach MISS, so best < MISS exactly => MISSING.
            missing.append(rc)
            continue

        # Pass 2: exact best over those candidates, in descending quick_ratio
        # order with early break (sorted desc: once qr <= best, no later
        # candidate can beat best either).
        cands.sort(reverse=True)
        sm = difflib.SequenceMatcher(None, rc)
        best = 0.0
        for qr, j in cands:
            if qr <= best:
                break
            sm.set_seq2(port_norm[j])
            r = sm.ratio()
            if r > best:
                best = r

        if best < MISS:
            missing.append(rc)
        elif best < THRESHOLD:
            review.append((rc, round(best, 2)))
    return missing, review


def run(args):
    cm = G.crate_map()
    if args.crate:
        args.crate = G.normalize_crate(args.crate, cm)
        if args.crate not in cm:
            G.emit(G.RED, f"unknown crate '{args.crate}' (not in crate_map)")
            return 2
    crates = [args.crate] if args.crate else G.workspace_crates(cm)
    idmap = load_idmap()

    # Optional SCOPE OVERRIDE (R6/right-tool): for a huge crate ported incrementally
    # (rustty-vt maps to 94 reference files, mostly still stubs), a whole-crate run is
    # both slow and meaninglessly RED. --ref-files/--port-glob restrict the comparison
    # to one module (e.g. PageList.zig <-> pagelist/*.rs) — same logic, scoped inputs.
    #
    # VACUITY FIX (deferral re-audit, 2026-07-31). --ref-files used to be passed
    # to comment_bodies() verbatim, so a path relative to the oracle root (the
    # natural spelling, e.g. `renderer/Thread.zig`) never resolved; comment_bodies
    # swallows the open() error, the gate found ZERO reference sentences and
    # printed "distinctive ref comments=0 missing=0 ... GREEN", exit 0. A
    # nonexistent file did the same. That is precisely the "full coverage against
    # nothing" spoof hole this module's GHOSTTY_SRC note says it refuses to have,
    # reintroduced through the scope override. Relative entries now resolve
    # against GHOSTTY_SRC, and a scope file that does not exist is BLOCKED (2),
    # never a green.
    scope_ref = None
    if getattr(args, "ref_files", None):
        scope_ref = []
        for p in args.ref_files.split(","):
            p = p.strip()
            if not p:
                continue
            full = p if os.path.isabs(p) else os.path.join(GHOSTTY_SRC, p)
            if not os.path.isfile(full):
                print(
                    f"BLOCKED: --ref-files entry does not exist: {p!r} "
                    f"(resolved to {full!r}). A missing scope file would yield a "
                    f"vacuous GREEN, so this is a hard error.",
                    file=sys.stderr,
                )
                return 2
            scope_ref.append(full)
        if not scope_ref:
            print("BLOCKED: --ref-files was given but names no files.", file=sys.stderr)
            return 2
    scope_port = sorted(_glob.glob(args.port_glob, recursive=True)) if getattr(args, "port_glob", None) else None
    if getattr(args, "port_glob", None) and not scope_port:
        print(
            f"BLOCKED: --port-glob {args.port_glob!r} matched no files; every "
            f"reference comment would report MISSING against an empty port side.",
            file=sys.stderr,
        )
        return 2

    overall_red = 0
    skipped = []
    for crate in crates:
        if scope_port is not None:
            rust_files = scope_port
        else:
            try:
                rust_files = crate_port_files(crate)
            except FileNotFoundError as e:
                print(f"BLOCKED: R4 relocation path for [{crate}] does not exist: "
                      f"{e} (EXTRA_PORT_PATHS is stale)", file=sys.stderr)
                return 2
        # only meaningful for crates that have been ported (non-stub).
        # H5 fix: unified done-detection via _gatelib.is_claimed_done — the same
        # signal G1/G2 bind on (statuses done|complete|parity|ported|final all
        # count), so 'ported'-status crates (args/termio) no longer escape this
        # gate the way a literal `PORT STATUS: done` grep let them.
        done = G.is_claimed_done(crate)
        if not (args.strict or done or args.crate or scope_port is not None):
            # ANNOUNCE the skip. This used to be a bare `continue`, and the
            # silence was the problem: a default run printed 22 `G3b [crate]`
            # lines out of 29 workspace crates, and nothing said the other seven
            # existed. `renderer` IS in the crate map (35 oracle files, including
            # Thread.zig) — it was simply never reached, so "G3b GREEN" said
            # nothing about it. Across apprt/apprt-api/bin/cfg-fmt/font/input/
            # renderer that is 169 of 512 mapped oracle files, 33% of the corpus,
            # invisible to the reader.
            #
            # Skipping in-progress crates is the right BEHAVIOUR — their comment
            # drift is expected and would drown the signal. Skipping them
            # invisibly is not. This changes no verdict; it makes the scope of the
            # verdict legible.
            n_ref = len(crate_ref_files(crate, cm))
            print(f"G3b [{crate}]: SKIPPED — PORT STATUS is not done "
                  f"({n_ref} oracle file(s) unexamined)")
            skipped.append((crate, n_ref))
            continue
        ref_norm = []
        for f in (scope_ref if scope_ref is not None else crate_ref_files(crate, cm)):
            for b in comment_bodies(f):
                nb = norm(b, idmap)
                if len(words(nb)) >= 5 and not CC.is_ascii_diagram(strip_marker(b)):
                    ref_norm.append(nb)
        # dedupe identical reference comments
        ref_norm = sorted(set(ref_norm))
        port_norm = []
        for f in rust_files:
            for b in comment_bodies(f):
                port_norm.append(norm(b, idmap))

        missing, review = classify(ref_norm, port_norm)

        if not rust_files:
            print(f"G3b [{crate}]: no ported Rust yet (progress)")
            continue
        print(f"G3b [{crate}]: distinctive ref comments={len(ref_norm)} "
              f"missing={len(missing)} review={len(review)}")
        for rc, r in review[:10]:
            print(f"    REVIEW (ratio {r}): {rc[:90]}")
        if missing:
            overall_red = 1
            for rc in missing[:25]:
                print(f"    MISSING ref comment: {rc[:100]}")

    # State the scope of the verdict alongside the verdict. A GREEN that silently
    # covers 67% of the corpus reads exactly like one that covers all of it.
    if skipped:
        n_files = sum(n for _c, n in skipped)
        print(f"\nG3b SCOPE: {len(skipped)} crate(s) SKIPPED (PORT STATUS not done) "
              f"= {n_files} oracle file(s) unexamined: "
              f"{', '.join(c for c, _n in sorted(skipped))}")
        print("  The verdict below covers the EXAMINED crates only. Use --strict or "
              "--crate <name> to include a skipped crate.")

    if overall_red:
        G.emit(G.RED, "dropped reference comment(s) — see MISSING above (R1 violation)")
        return 1
    G.emit(G.GREEN, "every distinctive reference comment has a match in the port (G3b)")
    return 0


def self_test():
    """Synthesize a ref/port pair and prove a dropped comment is caught."""
    idmap = [("ghostty", "rustty")]
    ref = ["the optimization mode as a string",
           "returns a circular buffer containing type t"]
    port_present = ["the optimization mode as a string"]  # dropped the circ buf one
    missing = []
    for rc in ref:
        blob = " ".join(port_present)
        if rc in blob:
            continue
        best = max((difflib.SequenceMatcher(None, rc, pc).ratio() for pc in port_present),
                   default=0.0)
        if best < MISS:
            missing.append(rc)
    ok = missing == ["returns a circular buffer containing type t"]
    print(f"self-test: dropped-comment detected = {ok} (missing={missing})")
    return 0 if (ok and self_test_relocations() == 0) else 1


# One REAL reference comment per R4 relocation: (crate, oracle file, the
# comment as it reads in the oracle). Each must be MISSING against the crate's
# own src/ alone and covered once the relocation is added: proof that the
# mapping is what closes it, on the real tree, not on a synthetic stand-in.
RELOCATION_REAL_ROWS = [
    ("font", "font/Collection.zig",
     "The available faces we have. This shouldn't be modified manually."),
    ("input", "input/function_keys.zig", "A single entry in the table of keys."),
    ("cfg-fmt", "config/key.zig",
     "Key is an enum of all the available configuration keys. This is used"),
]


def self_test_relocations():
    """R4 relocation arm of the self-test (S-8a)."""
    import tempfile
    ok = True

    # (1) Synthetic: a relocated comment is covered only with the relocation,
    # and a genuine drop stays MISSING with it (the mapping masks nothing).
    with tempfile.TemporaryDirectory() as td:
        own = os.path.join(td, "own")
        moved = os.path.join(td, "moved")
        os.makedirs(own)
        os.makedirs(moved)
        with open(os.path.join(own, "a.rs"), "w") as fh:
            fh.write("/// the crate keeps this comment in its own sources\n")
        with open(os.path.join(moved, "b.rs"), "w") as fh:
            fh.write("// this comment moved with the relocated port code\n")
        idmap = [("ghostty", "rustty")]
        ref = [norm("/// The crate keeps this comment in its own sources", idmap),
               norm("// This comment moved with the relocated port code", idmap),
               norm("// A comment the port genuinely dropped on the floor", idmap)]

        def port(files):
            return [norm(b, idmap) for f in files for b in comment_bodies(f)]

        m_own, _ = classify(ref, port(_rs_under(own)))
        m_all, _ = classify(ref, port(_rs_under(own) + _rs_under(moved)))
        arm1 = sorted(m_own) == sorted([ref[1], ref[2]]) and m_all == [ref[2]]
        print(f"self-test: relocation covers the moved comment, drop still caught "
              f"= {arm1} (own-only missing={len(m_own)}, with relocation={m_all})")
        ok &= arm1

        # (2) A relocation path that does not exist raises (BLOCKED in run()).
        try:
            crate_port_files("font", {"font": [os.path.join(td, "gone")]})
            arm2 = False
        except FileNotFoundError:
            arm2 = True
        print(f"self-test: missing relocation path is an error = {arm2}")
        ok &= arm2

    # (3) Real tree: every mapped path exists, and each real row is MISSING
    # without its relocation and covered with it.
    idmap = load_idmap()
    for crate, ref_file, text in RELOCATION_REAL_ROWS:
        rc = norm("// " + text, idmap)
        in_oracle = any(norm(b, idmap) == rc for b in
                        comment_bodies(os.path.join(GHOSTTY_SRC, ref_file)))
        try:
            with_files = crate_port_files(crate)
        except FileNotFoundError as e:
            print(f"self-test: [{crate}] relocation path missing: {e}")
            ok = False
            continue
        own_norm = [norm(b, idmap) for f in crate_rust_files(crate) for b in comment_bodies(f)]
        all_norm = [norm(b, idmap) for f in with_files for b in comment_bodies(f)]
        before, _ = classify([rc], own_norm)
        after, _ = classify([rc], all_norm)
        arm3 = in_oracle and before == [rc] and after == []
        print(f"self-test: [{crate}] real row {ref_file} MISSING without relocation, "
              f"covered with it = {arm3} (in oracle={in_oracle}, "
              f"before={len(before)}, after={len(after)})")
        ok &= arm3
    return 0 if ok else 1


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--crate")
    ap.add_argument("--strict", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--ref-files", help="comma-separated reference files (scope override, e.g. one module)")
    ap.add_argument("--port-glob", help="glob of ported .rs files (scope override, pairs with --ref-files)")
    a = ap.parse_args()
    if a.self_test:
        sys.exit(self_test())
    sys.exit(run(a))
