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

GHOSTTY_SRC = os.environ.get("GHOSTTY_SRC", "/Users/gael/projects/ghostty/src")
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


def crate_rust_files(crate):
    d = G.crate_src_dir(crate)
    if not d or not os.path.isdir(d):
        return []
    out = []
    for root, _, files in os.walk(d):
        for fn in files:
            if fn.endswith(".rs"):
                out.append(os.path.join(root, fn))
    return out


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
    scope_ref = [p.strip() for p in args.ref_files.split(",") if p.strip()] if getattr(args, "ref_files", None) else None
    scope_port = sorted(_glob.glob(args.port_glob, recursive=True)) if getattr(args, "port_glob", None) else None

    overall_red = 0
    for crate in crates:
        rust_files = scope_port if scope_port is not None else crate_rust_files(crate)
        # only meaningful for crates that have been ported (non-stub).
        # H5 fix: unified done-detection via _gatelib.is_claimed_done — the same
        # signal G1/G2 bind on (statuses done|complete|parity|ported|final all
        # count), so 'ported'-status crates (args/termio) no longer escape this
        # gate the way a literal `PORT STATUS: done` grep let them.
        done = G.is_claimed_done(crate)
        if not (args.strict or done or args.crate or scope_port is not None):
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
