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

Deliberately-not-carried comments (no Rust counterpart by a recorded decision;
stripped gag prose) are excused ONLY by a validated row of
`.port/parity-closure/COMMENT-NOT-CARRIED.tsv` and are printed as NOT-CARRIED —
see the NOT-CARRIED LEDGER section below and `_g3b_not_carried.py`.

Usage: check_comment_coverage.py --crate <name> [--strict] [--self-test]
       check_comment_coverage.py --not-carried-check   (every ledger row, stale check included)
Exit 0 = GREEN/REVIEW-only ; 1 = RED (dropped reference comment or invalid ledger
row) ; 2 = usage/blocked.
"""
import argparse, os, re, sys, difflib, bisect, glob as _glob
from collections import Counter

GATES = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, GATES)
import _gatelib as G  # noqa: E402
import _g3b_not_carried as NC  # noqa: E402  (S-8d NOT-CARRIED ledger)

# NOT env-overridable, deliberately. This used to be
# `os.environ.get("GHOSTTY_SRC", ...)`, which is a spoof hole of exactly the
# shape `ci.sh:114-129` documents for the test-count gate: point the variable at
# an empty or fake tree and every reference file is missing, so the gate finds
# zero reference sentences everywhere and reports full coverage against nothing.
# The oracle is an EXTERNAL read-only reference (R3) at one DERIVED path (N483: `ghostty` beside the main checkout);
# `P3-W10 T3` advances that real tree, which is the only supported way to change
# what this gate reads.
GHOSTTY_SRC = os.path.join(G.oracle_root(), "src")
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


def comment_bodies_text(text):
    """Comment bodies per line of `text` (split on "\\n" exactly as iterating
    the file did; the text was read with universal newlines)."""
    out = []
    lines = text.split("\n")
    for line in [ln + "\n" for ln in lines[:-1]] + ([lines[-1]] if lines[-1] else []):
        idx = CC.find_comment_start(line)
        if idx is None:
            continue
        body = line[idx:].rstrip("\n")
        if body.strip():
            out.append(body)
    return out


def comment_bodies(path):
    try:
        with open(path, encoding="utf-8", errors="replace") as fh:
            return comment_bodies_text(fh.read())
    except OSError:
        return []


def crate_ref_files(crate, cm):
    return [os.path.join(GHOSTTY_SRC, f) for f, _ in cm[crate]["files"]]


def crate_rust_sources(crate):
    """(path, text) for the crate's OWN src/ only (no relocations)."""
    return list(G.iter_rust_sources(crate, relocations={}))


# R4 RELOCATIONS. The map lives in `_gatelib.RELOCATIONS`: S-8a introduced it
# here as `EXTRA_PORT_PATHS`, and S-8c hoisted it so that every crate-scanning
# gate honours the same entries. Before the map, `--crate font` reported 1767
# of its 1795 MISSING against code living in `rustty-font-shaper`, and
# `--crate input` reported all 68 against `rustty-vt/src/input/`. The
# comparison runs against the crate's own `src/` PLUS the relocated locations:
# the same logic, the correct port side. A reference comment still has to
# appear in the port that implements it, so a relocation admits no comment the
# crate's port does not carry. A listed path or item that does not exist is
# BLOCKED (exit 2), never a silent fall-back.
def crate_port_sources(crate, relocations=None):
    """(path, text) for the port side of `crate`: its own src/ plus its R4
    relocations. Raises G.RelocationMissing for a stale relocation."""
    return list(G.iter_rust_sources(crate, relocations))


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

    nc = not_carried_setup()
    if nc is None:
        return 2
    overall_red = 1 if nc["errors"] else 0
    skipped = []
    for crate in crates:
        if scope_port is not None:
            rust_sources = [(f, None) for f in scope_port]
        else:
            try:
                rust_sources = crate_port_sources(crate)
            except G.RelocationMissing as e:
                print(f"BLOCKED: {e}", file=sys.stderr)
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
        for f, text in rust_sources:
            for b in (comment_bodies(f) if text is None else comment_bodies_text(text)):
                port_norm.append(norm(b, idmap))

        missing, review = classify(ref_norm, port_norm)
        missing, review, nc_red = not_carried_apply(
            nc, crate, cm, idmap, missing, review, port_norm,
            scope_ref if scope_ref is not None else crate_ref_files(crate, cm))
        overall_red |= nc_red

        if not rust_sources:
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
        G.emit(G.RED, "dropped reference comment(s) or an invalid NOT-CARRIED row — see "
               "MISSING / NOT-CARRIED-INVALID above (R1 violation)")
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
    reloc_ok = self_test_relocations() == 0 and self_test_relocated_ports()
    return 0 if (ok and reloc_ok and self_test_not_carried()) else 1


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

        # `demo-s8c` has no crates/ dir, so its own src/ is empty; the `own`
        # directory stands in for it as a first relocation entry.
        crate = "demo-s8c"

        def port(entries):
            return [norm(b, idmap) for _f, t in
                    crate_port_sources(crate, {crate: entries})
                    for b in comment_bodies_text(t)]

        m_own, _ = classify(ref, port(({"path": own},)))
        m_all, _ = classify(ref, port(({"path": own}, {"path": moved})))
        arm1 = sorted(m_own) == sorted([ref[1], ref[2]]) and m_all == [ref[2]]
        print(f"self-test: relocation covers the moved comment, drop still caught "
              f"= {arm1} (own-only missing={len(m_own)}, with relocation={m_all})")
        ok &= arm1

        # (1b) An item-scoped relocation covers only its items' comments: the
        # moved comment sits OUTSIDE the named item and stays MISSING.
        with open(os.path.join(moved, "c.rs"), "w") as fh:
            fh.write("// zebra quartz banjo lighthouse mercury volcano outside\n\n"
                     "/// a comment inside the relocated item stays covered\n"
                     "pub mod kept {}\n")
        ref_items = [norm("// zebra quartz banjo lighthouse mercury volcano outside", idmap),
                     norm("/// a comment inside the relocated item stays covered", idmap)]
        m_items, _ = classify(ref_items, port(
            ({"path": os.path.join(moved, "c.rs"), "items": (r"^pub mod kept\b",)},)))
        arm1b = m_items == [ref_items[0]]
        print(f"self-test: item-scoped relocation covers only the item = {arm1b} "
              f"(missing={m_items})")
        ok &= arm1b

        # (2) A relocation path (or item) that does not exist raises
        # RelocationMissing (BLOCKED, exit 2, in run()).
        for label, entry in (("path", {"path": os.path.join(td, "gone")}),
                             ("item", {"path": os.path.join(moved, "c.rs"),
                                       "items": (r"^pub mod gone\b",)})):
            try:
                crate_port_sources(crate, {crate: (entry,)})
                arm2 = False
            except G.RelocationMissing:
                arm2 = True
            print(f"self-test: missing relocation {label} is an error = {arm2}")
            ok &= arm2

    # (3) Real tree: every mapped path exists, and each real row is MISSING
    # without its relocation and covered with it.
    idmap = load_idmap()
    for crate, ref_file, text in RELOCATION_REAL_ROWS:
        rc = norm("// " + text, idmap)
        in_oracle = any(norm(b, idmap) == rc for b in
                        comment_bodies(os.path.join(GHOSTTY_SRC, ref_file)))
        try:
            with_sources = crate_port_sources(crate)
        except G.RelocationMissing as e:
            print(f"self-test: [{crate}] relocation missing: {e}")
            ok = False
            continue
        own_norm = [norm(b, idmap) for _f, t in crate_rust_sources(crate)
                    for b in comment_bodies_text(t)]
        all_norm = [norm(b, idmap) for _f, t in with_sources for b in comment_bodies_text(t)]
        before, _ = classify([rc], own_norm)
        after, _ = classify([rc], all_norm)
        arm3 = in_oracle and before == [rc] and after == []
        print(f"self-test: [{crate}] real row {ref_file} MISSING without relocation, "
              f"covered with it = {arm3} (in oracle={in_oracle}, "
              f"before={len(before)}, after={len(after)})")
        ok &= arm3
    return 0 if ok else 1


def self_test_relocated_ports():
    """Per-reference-file relocation arms (S-8b's RELOCATED_PORTS, now the
    `ref`-carrying entries of `_gatelib.RELOCATIONS`): (1) a comment ported only
    into the relocation target is covered WITH the entry and MISSING without it
    (the entry is what covers it, and it adds only the named file); (2) a comment
    dropped from the relocated port stays MISSING with the entry; (3) a
    nonexistent target raises (BLOCKED, never a silent no-op); (4) every real
    `ref` entry names a reference file the crate map assigns to THAT crate, and
    every target exists."""
    import tempfile
    idmap = [("ghostty", "rustty")]
    kept = "this is the main file for the c api used to embed ghostty"
    dropped = "some comptime assertions that our c api depends on here"
    crate = "demo-s8c"  # no crates/ dir: its own src/ is empty
    with tempfile.TemporaryDirectory() as d:
        tgt = os.path.join(d, "relocated.rs")
        other = os.path.join(d, "other.rs")
        with open(tgt, "w", encoding="utf-8") as fh:
            fh.write("// This is the main file for the C API used to embed rustty\nfn f() {}\n")
        with open(other, "w", encoding="utf-8") as fh:
            fh.write("// an unrelated file that no entry names\n")
        reloc = {crate: ({"ref": "main_c.zig", "path": tgt},)}

        def verdict(rc, sources):
            nrc = norm(rc, idmap)
            pool = [norm(b, idmap) for _p, t in sources for b in comment_bodies_text(t)]
            if nrc in " \n ".join(pool):
                return "covered"
            best = max((difflib.SequenceMatcher(None, nrc, pc).ratio() for pc in pool),
                       default=0.0)
            return "missing" if best < MISS else "covered"

        with_entry = crate_port_sources(crate, reloc)
        a1 = ([p for p, _t in with_entry] == [tgt] and verdict(kept, with_entry) == "covered"
              and verdict(kept, crate_port_sources(crate, {})) == "missing")
        a2 = verdict(dropped, with_entry) == "missing"
        try:
            crate_port_sources(crate, {crate: ({"ref": "main_c.zig",
                                                "path": os.path.join(d, "nope.rs")},)})
            a3 = False
        except G.RelocationMissing:
            a3 = True
    try:
        known = G.file_to_crate()
        ref_entries = [(c, e) for c, es in G.RELOCATIONS.items() for e in es if "ref" in e]
        a4 = (len(ref_entries) > 0
              and all(known.get(e["ref"]) == c for c, e in ref_entries)
              and all(os.path.isfile(os.path.join(G.RUSTTY_ROOT, e["path"]))
                      for _c, e in ref_entries))
        G.check_relocations()
    except G.RelocationMissing as e:
        print(f"self-test: {e}")
        a4 = False
    ok = a1 and a2 and a3 and a4
    print(f"self-test: relocations covered-only-via-entry={a1} dropped-still-missing={a2} "
          f"missing-target-blocks={a3} real-entries-valid={a4}")
    return ok


# ===========================================================================
# NOT-CARRIED LEDGER (parity tail S-8d, 2026-09-24) — kept in its own section.
#
# `.port/parity-closure/COMMENT-NOT-CARRIED.tsv` declares reference comments
# that are deliberately not carried (code with no Rust counterpart by a
# recorded decision; stripped gag prose). The rules, and why each is
# machine-checked, are in `_g3b_not_carried.py`'s module doc. Three properties
# matter here:
#   * an invalid row is RED and excuses nothing;
#   * a row the port now carries (STALE) is RED;
#   * an excused comment is PRINTED as `NOT-CARRIED (<class>)`, never hidden.
# ===========================================================================
def _distinctive(body, idmap=None):
    idmap = load_idmap() if idmap is None else idmap
    return len(words(norm(body, idmap))) >= 5 and not CC.is_ascii_diagram(strip_marker(body))


def _nc_port_pool(crate, idmap):
    """The crate's normalized port comments, exactly as `run` builds them: its
    own src/ plus its `_gatelib.RELOCATIONS` entries (no second mechanism)."""
    return [norm(b, idmap) for f, text in crate_port_sources(crate)
            for b in (comment_bodies(f) if text is None else comment_bodies_text(text))]


def not_carried_setup(path=None, read_oracle=None, blobs=None):
    """Load and validate the ledger. Returns {"rows", "errors"}, or None when
    it is BLOCKED (missing file, missing cap directive, malformed row, or an
    unreadable oracle): the caller exits 2, never a pass."""
    path = path or NC.ledger_path(G.RUSTTY_ROOT)
    try:
        rows, cap = NC.load(path)
        if rows is None:
            print(f"BLOCKED: the NOT-CARRIED ledger {path} does not exist.", file=sys.stderr)
            return None
        if cap is None:
            print(f"BLOCKED: {path} carries no `#!not-carried-cap entries=N` directive.",
                  file=sys.stderr)
            return None
        read_oracle = read_oracle or NC.oracle_reader_at_pin(G)
        idmap = load_idmap()
        if blobs is None:
            blobs = NC.workspace_rust_items(G.RUSTTY_ROOT, G)
        errors = NC.validate(rows, cap, root=G.RUSTTY_ROOT, G=G, CC=CC,
                             strip_marker=strip_marker, norm=norm, idmap=idmap,
                             distinctive=lambda b: _distinctive(b, idmap),
                             read_oracle=read_oracle, blobs=blobs)
    except NC.LedgerError as e:
        print(f"BLOCKED: NOT-CARRIED ledger: {e}", file=sys.stderr)
        return None
    for e in errors:
        print(f"NOT-CARRIED-INVALID: {e}")
    return {"rows": rows, "errors": errors}


def not_carried_apply(nc, crate, cm, idmap, missing, review, port_norm, ref_files):
    """Apply the ledger rows of these reference files to one crate's result.
    Returns (missing, review, red)."""
    rels = {os.path.relpath(f, GHOSTTY_SRC) for f in ref_files}
    rows = [r for r in nc["rows"] if r["file"] in rels]
    if not rows:
        return missing, review, 0
    red = 0
    ok_rows = [r for r in rows if r.get("ok")]
    for r in NC.stale_rows(ok_rows, classify, port_norm):
        print(f"NOT-CARRIED-INVALID: COMMENT-NOT-CARRIED.tsv:{r['lineno']} "
              f"({r['file']}:{r['line']}): STALE — the port now carries this comment; "
              f"delete the row")
        r["ok"] = False
        red = 1
    for r in NC.mixed_unproven(ok_rows, norm, idmap, port_norm):
        print(f"NOT-CARRIED-INVALID: COMMENT-NOT-CARRIED.tsv:{r['lineno']} "
              f"({r['file']}:{r['line']}): its MIXED-CARRIED phrase is not in the port's "
              f"comments — the line's fact is not carried")
        r["ok"] = False
        red = 1
    ref_lines = []
    for f in ref_files:
        try:
            ref_lines.append((os.path.relpath(f, GHOSTTY_SRC),
                              open(f, encoding="utf-8", errors="replace").read().split("\n")))
        except OSError:
            pass
    occ = NC.occurrences(ref_lines, norm, idmap, lambda b: _distinctive(b, idmap), CC)
    missing, review, applied = NC.apply(rows, occ, missing, review)
    for r, was in sorted(applied, key=lambda a: (a[0]["file"], a[0]["line"])):
        print(f"    NOT-CARRIED ({r['class']}, was {was}): {r['file']}:{r['line']} "
              f"{r['text'][:90]}")
    if applied:
        print(f"    ({len(applied)} reference comment(s) excused by "
              f"COMMENT-NOT-CARRIED.tsv; each row is validated above)")
    return missing, review, red


_NC_ORACLE = """const std = @import("std");

/// The frobnicator exists only for the widget build on plan nine.
pub const Frob = struct {
    // Only the plan nine linker needs this particular frob here.
    x: u8,
};

// Honestly this whole thing is a silly waste of everybody's time.
const y = 1;
// The ratio is 42 and the buffer must stay aligned to that.
const z = 2;
// Pull in the wibble module so its exports get emitted here.
_ = @import("wibble.zig");
// short one
/// Get the mutable frob. Not recommended for anyone at all really.
pub fn getMut(self: *Frob) *Frob {
    return self;
}

pub fn handler(self: *Frob) void {
    const priv = self;
    // We will check the flag again in each handler later on, fine.
    _ = priv;
}
""".split("\n")


def _nc_line(text):
    return next(i for i, ln in enumerate(_NC_ORACLE, 1) if text in ln)


_NC_P4 = """# fake phase-4
| item | verdict |
|---|---|
| `Frob` and `Frob.getMut` in `lib_vt.zig` | **NON-PORT** synthetic row for the self-test |
| `Frob` with no oracle file named | NON-PORT naming no file |
| ~~`Struck`~~ `Frob` in lib_vt.zig | struck long ago |
| `Other` in lib_vt.zig | NON-PORT naming something else |
| dup-needle `Frob` lib_vt.zig one |
| dup-needle `Frob` lib_vt.zig two |

Paragraph anchor for `Frob` in lib_vt.zig: a decision record.
RESOLVED: all of the above is now PORTED.

## big section naming Frob in lib_vt.zig
""" + "".join(f"line {i} of a long section\n" for i in range(40))

_NC_P3 = """# fake phase-3

# gag lines (`strip-ghostty-gag-prose`)

- `lib_vt.zig:9`: a joke.
- `lib_vt.zig:23`: a mixed line.

# rule-only section (`strip-ghostty-gag-prose`)

Mentions the rule, lists no line.

# unrelated section

Nothing here names the rule.
"""


def self_test_not_carried():
    """NOT-CARRIED ledger arms (S-8d, hardened S-8e): every acceptance and
    rejection rule, first on synthetic rows (a fake oracle file, fake evidence
    docs, fake Rust blobs), then on the REAL ledger and REAL oracle/port —
    including the S-8d review's attacks A-1..A-3 planted verbatim, and negative
    controls that mutate a real row and must flip it RED for the stated reason."""
    import shutil
    import tempfile
    ok = True
    idmap = load_idmap()
    dist = lambda b: _distinctive(b, idmap)  # noqa: E731

    def check(name, cond):
        nonlocal ok
        ok &= bool(cond)
        print(f"  [{'ok ' if cond else 'FAIL'}] not-carried: {name}")

    with tempfile.TemporaryDirectory() as root:
        for rel, body in ((".port/phase-4/NON_PORTS.md", _NC_P4),
                          (".port/phase-3/NON_PORTS.md", _NC_P3)):
            os.makedirs(os.path.join(root, os.path.dirname(rel)), exist_ok=True)
            with open(os.path.join(root, rel), "w", encoding="utf-8") as fh:
                fh.write(body)
        reader = lambda rel: _NC_ORACLE if rel == "lib_vt.zig" else None  # noqa: E731
        P4 = ".port/phase-4/NON_PORTS.md#| `Frob` and"
        P3 = ".port/phase-3/NON_PORTS.md## gag lines"
        REASON = "synthetic self-test row with a sufficiently long reason"

        def row(text, cls, decl, ev, reason=REASON, file="lib_vt.zig"):
            ln = _nc_line(text)
            return {"file": file, "line": ln, "class": cls, "decl": decl,
                    "evidence": ev, "text": text, "reason": reason, "lineno": ln}

        A = row("The frobnicator exists only for the widget build on plan nine.",
                "no-counterpart", "Frob", P4)
        B = row("Only the plan nine linker needs this particular frob here.",
                "no-counterpart", "Frob", P4)
        C = row("Honestly this whole thing is a silly waste of everybody's time.",
                "gag-prose", "-", P3)
        D = row("Pull in the wibble module so its exports get emitted here.",
                "no-counterpart", "wibble", P4)
        E = row("Get the mutable frob. Not recommended for anyone at all really.",
                "no-counterpart", "Frob.getMut", P4)
        TXT = "We will check the flag again in each handler later on, fine."
        clean = [("crates/x/src/lib.rs", "fn unrelated() {}\nimpl Other { fn get_mut() {} }\n",
                  ["/// an unrelated doc"])]

        def v(rows, cap="n", blobs=clean):
            rows = [dict(r) for r in rows]
            errs = NC.validate(rows, len(rows) if cap == "n" else cap, root=root, G=G,
                               CC=CC, strip_marker=strip_marker, norm=norm, idmap=idmap,
                               distinctive=dist, read_oracle=reader, blobs=blobs)
            return errs, rows

        def red(rows, needle, **kw):
            errs, _ = v(rows, **kw)
            return any(needle in e for e in errs)

        errs, rows = v([A, B, C, E])
        check("ACCEPT: valid no-counterpart (doc above / inside span / qualified) and gag "
              f"rows prove themselves (errors={errs})",
              not errs and all(r["ok"] for r in rows))
        check("REJECT: cap below the row count", red([A, B], "cap mismatch", cap=1))
        check("REJECT: cap above the row count (stale ratchet)", red([A], "cap mismatch", cap=2))
        check("REJECT: missing cap directive", red([A], "BLOCKED", cap=None))
        check("REJECT: text differs from the oracle line (stale in oracle)",
              red([dict(A, text=A["text"] + " x")], "STALE"))
        check("REJECT: row points at a non-distinctive comment",
              red([dict(C, line=_nc_line("// short one"), text="short one")], "not distinctive"))
        check("REJECT: unknown class", red([dict(A, **{"class": "because"})], "unknown class"))
        check("REJECT: reason too short", red([dict(A, reason="trust me")], "reason"))
        check("REJECT: oracle file not in the crate map",
              red([dict(A, file="nope/nope.zig")], "crate map"))
        check("REJECT: oracle file absent at the pin",
              red([dict(A, file="main_c.zig")], "does not exist at the pin"))
        check("REJECT: duplicate row", red([A, dict(A)], "duplicate"))
        # `std` is declared at :1, above the comment and with no block, and the
        # code after :9 never names it.
        check("REJECT: comment not attached to decl",
              red([dict(C, **{"class": "no-counterpart", "decl": "std", "evidence": P4})],
                  "not attached"))
        # W1 — the review's A-1 shape: a local binding / keyword as decl.
        a1 = row(TXT, "no-counterpart", "priv", P4)
        check("REJECT (W1, A-1 synthetic): decl `priv` (a local binding) is denylisted",
              red([a1], "keyword or a common local name"))
        for kw_ in ("orelse", "comptime", "errdefer", "try", "self", "alloc"):
            check(f"REJECT (W1): decl `{kw_}` is denylisted",
                  red([dict(a1, decl=kw_)], "keyword or a common local name"))
        check("REJECT (W1): a used-but-not-declared identifier (`wibble`) is not a decl",
              red([D], "not DECLARED"))
        check("REJECT: declared decl has a Rust ITEM anywhere in the workspace",
              red([A], "HAS a Rust counterpart",
                  blobs=clean + [("crates/y/src/f.rs", "pub struct Frob;", [])]))
        check("REJECT (W1): declared decl has a Rust FIELD of that name",
              red([A], "HAS a Rust counterpart",
                  blobs=clean + [("crates/y/src/f.rs", "struct S {\n    pub frob: u8,\n}", [])]))
        check("REJECT (W1, renamed counterpart): a `///` doc cites the decl",
              red([A], "doc/PORTS citation",
                  blobs=clean + [("crates/y/src/f.rs", "pub const BASE: u8 = 1;",
                                  ["/// `lib_vt.zig:4` `Frob` — the renamed port"])]))
        check("REJECT (W1, renamed counterpart): a `// PORTS:` comment cites the decl",
              red([A], "doc/PORTS citation",
                  blobs=clean + [("crates/y/src/f.rs", "", ["    // PORTS: `Frob` (lib_vt.zig:4)"])]))
        check("ACCEPT: a declared decl merely USED in Rust code (not an item) does not count",
              not v([A], blobs=clean + [("crates/y/src/f.rs", "let x = frob + 1;", [])])[0])
        check("REJECT: qualified member declared inside `impl Container`",
              red([E], "HAS a Rust counterpart",
                  blobs=clean + [("crates/y/src/f.rs", "impl Frob { pub fn get_mut(&self) {} }", [])]))
        check("REJECT: evidence doc outside the allowed records",
              red([dict(A, evidence="README.md#Frob")], "not an allowed record"))
        check("REJECT: evidence needle not found",
              red([dict(A, evidence=".port/phase-4/NON_PORTS.md#no such needle")], "not found"))
        check("REJECT (W4): an ambiguous evidence needle",
              red([dict(A, evidence=".port/phase-4/NON_PORTS.md#dup-needle")], "ambiguous"))
        check("REJECT: evidence record does not name the decl",
              red([dict(A, evidence=".port/phase-4/NON_PORTS.md#| `Other`")], "does not name"))
        check("REJECT (W1): evidence record does not name the ORACLE FILE",
              red([dict(A, evidence=".port/phase-4/NON_PORTS.md#no oracle file named")],
                  "does not name the oracle file"))
        check("REJECT: struck evidence record cannot vote",
              red([dict(A, evidence=".port/phase-4/NON_PORTS.md#| ~~`Struck`~~")], "struck"))
        check("REJECT (W3, A-3 synthetic): RESOLVED/PORTED in the record BODY cannot vote",
              red([dict(A, evidence=".port/phase-4/NON_PORTS.md#Paragraph anchor for")], "struck"))
        check("REJECT (W4): a heading needle whose record exceeds the size cap",
              red([dict(A, evidence=".port/phase-4/NON_PORTS.md### big section")],
                  f"> {NC.RECORD_MAX_LINES}"))
        check("REJECT: gag evidence record that does not name the rule",
              red([dict(C, evidence=".port/phase-3/NON_PORTS.md## unrelated section")],
                  "does not name strip-ghostty-gag-prose"))
        check("REJECT (W2, A-2 synthetic): gag evidence that names the rule but not this line",
              red([dict(C, evidence=".port/phase-3/NON_PORTS.md## rule-only section")],
                  "does not list this line"))
        check("REJECT: gag row with a decl", red([dict(C, decl="Frob")], "carry decl"))
        g_tech = row(TXT, "gag-prose", "-", P3)
        check("REJECT (W2): gag text with technical words (flag, handler)",
              red([g_tech], "technical word"))
        mixed_ok = dict(g_tech, reason='MIXED-CARRIED: "we check the flag in each handler" ' + REASON)
        mixed_short = dict(g_tech, reason='MIXED-CARRIED: "we check the flag again" ' + REASON)
        check("ACCEPT (W2): a MIXED-CARRIED gag row whose phrase carries the technical words",
              not v([mixed_ok])[0])
        check("REJECT (W2): a MIXED-CARRIED phrase missing a technical word",
              red([mixed_short], "does not carry"))
        _, mrows = v([mixed_ok])
        pool_with = [norm("// we check the flag in each handler", idmap)]
        check("REJECT (W2): MIXED-CARRIED phrase absent from the port pool is unproven; "
              "present, it is proven",
              NC.mixed_unproven(mrows, norm, idmap, ["unrelated"]) == mrows
              and NC.mixed_unproven(mrows, norm, idmap, pool_with) == [])
        for bad_text, why in (("The ratio is 42 and the buffer must stay aligned to that.", "digit"),
                              ("this uses the frob_count thing badly", "snake_case"),
                              ("this uses the frobCount thing badly", "camelCase"),
                              ("this uses `frob` thing badly today", "backtick"),
                              ("this returns EINTR way too often honestly", "ALL-CAPS")):
            check(f"REJECT: gag text containing a {why} (technical content)",
                  any(why.split("_")[0] in e or why in e
                      for e in NC.gag_violations(bad_text, REASON)))
        check("REJECT (W2): gag text using a word of the surrounding oracle code",
              any("technical word" in e for e in
                  NC.gag_violations("that frobnicator is silly", REASON, {"frobnicator"})))
        check("ACCEPT: gag text with only allowlisted capitals (NOTE, XDG, GNOME)",
              not NC.gag_violations("NOTE(someone): XDG and GNOME are a mess", REASON))
        check("ACCEPT: gag text naming the proper noun X11 (whole token)",
              not NC.gag_violations("I know this is really ugly. X11 is ugly. I consider it apropos.", REASON))
        for bad_text, why in (("X11R6 is ugly and I consider it apropos", "digit"),
                              ("X11_ATOM is ugly and I consider it apropos", "snake_case"),
                              ("X12 is ugly and I consider it apropos", "digit"),
                              ("XA_ATOM and X11 are ugly and apropos", "snake_case")):
            check(f"REJECT: proper-noun allowance is exact-token only ({bad_text.split()[0]})",
                  any(why in e for e in NC.gag_violations(bad_text, REASON)))
        long = " ".join(["word"] * (NC.GAG_MAX_WORDS + 1))
        check("REJECT: gag text over the word limit",
              any("longer than" in e for e in NC.gag_violations(long, REASON)))
        check("ACCEPT: over the limit with a JUSTIFIED-LONG: reason",
              not NC.gag_violations(long, "JUSTIFIED-LONG: " + REASON))

        # apply(): excuse only on ok rows, only when every occurrence is covered.
        _, vrows = v([A, C])
        occ = NC.occurrences([("lib_vt.zig", _NC_ORACLE)], norm, idmap, dist, CC)
        nA, nC = vrows[0]["norm"], vrows[1]["norm"]
        other = norm("// The ratio is 42 and the buffer must stay aligned to that.", idmap)
        m2, r2, applied = NC.apply(vrows, occ, [nA, other], [(nC, 0.7)])
        check("APPLY: a valid row excuses its MISSING and REVIEW comment; others stay",
              m2 == [other] and r2 == [] and len(applied) == 2)
        bad = [dict(r, ok=False) for r in vrows]
        m2, r2, applied = NC.apply(bad, occ, [nA], [(nC, 0.7)])
        check("APPLY: an invalid row excuses nothing", m2 == [nA] and len(r2) == 1 and not applied)
        occ2 = {k: set(s) for k, s in occ.items()}
        occ2[nA].add(("main_c.zig", 999))
        m2, _, applied = NC.apply(vrows, occ2, [nA], [])
        check("APPLY: a text that also occurs at an uncovered site stays MISSING",
              m2 == [nA] and not applied)
        check("STALE: a row whose comment the port pool now carries is stale",
              NC.stale_rows(vrows[:1], classify, [nA]) == vrows[:1]
              and NC.stale_rows(vrows[:1], classify, ["unrelated port comment text here"]) == [])

    # ---- REAL rows: the committed ledger against the real oracle and workspace.
    try:
        rows, cap = NC.load(NC.ledger_path(G.RUSTTY_ROOT))
        reader = NC.oracle_reader_at_pin(G)
    except NC.LedgerError as e:
        print(f"  [FAIL] not-carried: real ledger BLOCKED: {e}")
        return False
    if rows is None:
        print("  [FAIL] not-carried: the real ledger is missing")
        return False
    blobs = NC.workspace_rust_items(G.RUSTTY_ROOT, G)

    def real(rs, c="n", root=G.RUSTTY_ROOT):
        rs = [dict(r) for r in rs]
        return NC.validate(rs, len(rs) if c == "n" else c, root=root, G=G, CC=CC,
                           strip_marker=strip_marker, norm=norm, idmap=idmap,
                           distinctive=dist, read_oracle=reader, blobs=blobs), rs

    def oracle_text(f, ln):
        src = reader(f)[ln - 1]
        return strip_marker(src[CC.find_comment_start(src):])

    errs, vrows = real(rows, cap)
    classes = {r["class"] for r in rows}
    check(f"REAL: all {len(rows)} committed rows prove themselves (errors={len(errs)})",
          rows and not errs and classes == set(NC.CLASSES))
    by = {(r["file"], r["line"]): r for r in rows}
    getmut = by.get(("apprt/gtk/class/config.zig", 94))
    dll = by.get(("main_c.zig", 179))
    gag = by.get(("apprt/gtk/winproto/BlurRegion.zig", 58))
    adwc = by.get(("apprt/gtk/adw_version.zig", 3))
    if not (getmut and dll and gag and adwc):
        print("  [FAIL] not-carried: the real control rows are missing from the ledger")
        return False
    # The review's attacks, planted verbatim. Line numbers are at the Phase-7 ORACLE_PIN
    # (P7-W12 T0 re-anchored them from the Phase-6 pin on the same comment text: window.zig 882/883 -> 862/863,
    # lib_vt.zig 185 -> 186).
    w863 = {"file": "apprt/gtk/class/window.zig", "line": 863, "class": "no-counterpart",
            "decl": "priv", "text": oracle_text("apprt/gtk/class/window.zig", 863),
            "evidence": ".port/parity-closure/ADJUDICATION.md#priv.global_shortcuts.setDbusConnection",
            "reason": "review attack A-1: a tail-d MISSING comment on ported code", "lineno": 0}
    check("REAL ATTACK A-1: window.zig:863 declared no-counterpart with decl `priv` is RED",
          any("keyword or a common local name" in e for e in real([w863])[0]))
    check("REAL ATTACK A-1': the same row with a non-denylisted used identifier is still RED "
          "(not declared / evidence names neither it nor window.zig)",
          any(("not DECLARED" in e) or ("does not name" in e)
              for e in real([dict(w863, decl="surface_init")])[0]))
    w862 = {"file": "apprt/gtk/class/window.zig", "line": 862, "class": "gag-prose", "decl": "-",
            "text": oracle_text("apprt/gtk/class/window.zig", 862),
            "evidence": ".port/phase-3/NON_PORTS.md#Two upstream bugs that must NOT be reproduced",
            "reason": "review attack A-2: plain-English technical text as gag prose", "lineno": 0}
    e862 = real([w862])[0]
    check("REAL ATTACK A-2: window.zig:862 as gag-prose citing a record that merely mentions "
          "the rule is RED (the record lists no such line; the text is technical)",
          any("does not list this line" in e for e in e862)
          and any("technical word" in e for e in e862))
    with tempfile.TemporaryDirectory() as troot:
        for d in NC.EVIDENCE_DOCS:
            src = os.path.join(G.RUSTTY_ROOT, d)
            if os.path.exists(src):
                os.makedirs(os.path.dirname(os.path.join(troot, d)), exist_ok=True)
                shutil.copy(src, os.path.join(troot, d))
        clean_ok = not real([adwc], root=troot)[0]
        p = os.path.join(troot, ".port/phase-2/DEFERRALS.md")
        txt = open(p, encoding="utf-8").read()
        anchor = "**AD-3 deletion set — G3b NOT-CARRIED anchor"
        i = txt.index("\n", txt.index(anchor))
        with open(p, "w", encoding="utf-8") as fh:
            fh.write(txt[:i] + "\nRESOLVED: all of the above is now PORTED." + txt[i:])
        attacked = real([adwc], root=troot)[0]
        check("REAL ATTACK A-3: RESOLVED appended to the BODY of the AD-3 anchor paragraph "
              "stops it voting (and the unmodified copy votes)",
              clean_ok and any("struck" in e for e in attacked))
    check("REAL NEG (W1 renamed counterpart): adw_version.zig:50 as a `comptime_version` row "
          "is RED — `BASELINE`'s doc cites it",
          any("doc/PORTS citation" in e for e in real([dict(
              adwc, line=50, decl="comptime_version",
              text=oracle_text("apprt/gtk/adw_version.zig", 50))])[0]))
    check("REAL NEG (W4): a heading needle on the big `L5 GTK baseline pin (AD-3)` section "
          "is over the record cap",
          any(f"> {NC.RECORD_MAX_LINES}" in e for e in real([dict(
              adwc, evidence=".port/phase-2/DEFERRALS.md### L5 GTK baseline pin (AD-3)")])[0]))
    check("REAL NEG: mutated text of a real row is STALE",
          any("STALE" in e for e in real([dict(dll, text=dll["text"] + "!")])[0]))
    check("REAL NEG: the getMut row unqualified hits `ScreenMap::get_mut` (qualification is "
          "what proves absence, per Config)",
          any("HAS a Rust counterpart" in e for e in real([dict(getmut, decl="getMut")])[0]))
    check("REAL NEG: a real row whose evidence record does not name its decl",
          any("does not name" in e for e in real([dict(dll, evidence=".port/phase-4/NON_PORTS.md"
                                                               "#`src/lib/TinyIo.zig`")])[0]))
    check("REAL NEG: a real row re-pointed at a decl it is not attached to (DllMain's doc "
          "claimed for `ghostty_string_free`, the fn just above it)",
          any("not attached" in e for e in real([dict(dll, decl="ghostty_string_free")])[0]))
    blur59 = dict(gag, line=59, text=oracle_text("apprt/gtk/winproto/BlurRegion.zig", 59))
    check("REAL NEG: BlurRegion.zig:59 (\"~30% bigger\") can never be gag prose (digit)",
          any("digit" in e for e in real([blur59])[0]))
    # A real comment the port DOES carry, declared as a row, is STALE (rule 2).
    carried_row = dict(dll, line=186, file="lib_vt.zig", decl="quirks_memset",
                       text=oracle_text("lib_vt.zig", 186))
    _, crs = real([carried_row])
    pool = _nc_port_pool("bin", idmap)
    check("REAL NEG: lib_vt.zig:186 (carried by the port) declared as a row is STALE",
          crs[0]["norm"] and NC.stale_rows(crs, classify, pool) == crs)
    real_ok = [r for r in vrows if r["file"] == "lib_vt.zig"]
    check("REAL: the committed lib_vt.zig rows are NOT stale against the real bin pool",
          real_ok and NC.stale_rows(real_ok, classify, pool) == [])
    mixed = [r for r in vrows if NC.MIXED_RE.match(r["reason"])]
    check("REAL: the committed MIXED-CARRIED row's phrase is in the apprt port",
          mixed and NC.mixed_unproven(mixed, norm, idmap, _nc_port_pool("apprt", idmap)) == [])
    return ok


def not_carried_check():
    """`--not-carried-check`: validate EVERY row, including rule 2 (stale) against
    each row's own crate port pool, whatever the crates' PORT STATUS."""
    nc = not_carried_setup()
    if nc is None:
        return 2
    red = 1 if nc["errors"] else 0
    idmap = load_idmap()
    for crate in sorted({r["crate"] for r in nc["rows"] if r.get("ok")}):
        try:
            pool = _nc_port_pool(crate, idmap)
        except G.RelocationMissing as e:
            print(f"BLOCKED: {e}", file=sys.stderr)
            return 2
        rows = [r for r in nc["rows"] if r.get("ok") and r["crate"] == crate]
        for r in NC.stale_rows(rows, classify, pool):
            print(f"NOT-CARRIED-INVALID: COMMENT-NOT-CARRIED.tsv:{r['lineno']} "
                  f"({r['file']}:{r['line']}): STALE — the port now carries this comment")
            red = 1
        for r in NC.mixed_unproven(rows, norm, idmap, pool):
            print(f"NOT-CARRIED-INVALID: COMMENT-NOT-CARRIED.tsv:{r['lineno']} "
                  f"({r['file']}:{r['line']}): its MIXED-CARRIED phrase is not in the port")
            red = 1
        print(f"NOT-CARRIED [{crate}]: {len(rows)} row(s) checked")
    if red:
        G.emit(G.RED, "COMMENT-NOT-CARRIED.tsv has invalid or stale rows")
        return 1
    G.emit(G.GREEN, f"COMMENT-NOT-CARRIED.tsv: all {len(nc['rows'])} row(s) prove themselves")
    return 0


if __name__ == "__main__":
    ap = argparse.ArgumentParser()
    ap.add_argument("--crate")
    ap.add_argument("--strict", action="store_true")
    ap.add_argument("--self-test", action="store_true")
    ap.add_argument("--not-carried-check", action="store_true",
                    help="validate every COMMENT-NOT-CARRIED.tsv row, stale check included")
    ap.add_argument("--ref-files", help="comma-separated reference files (scope override, e.g. one module)")
    ap.add_argument("--port-glob", help="glob of ported .rs files (scope override, pairs with --ref-files)")
    a = ap.parse_args()
    if a.self_test:
        sys.exit(self_test())
    if a.not_carried_check:
        sys.exit(not_carried_check())
    sys.exit(run(a))
