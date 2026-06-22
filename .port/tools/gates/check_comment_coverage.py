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
import argparse, os, re, sys, difflib

GATES = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, GATES)
import _gatelib as G  # noqa: E402

GHOSTTY_SRC = os.environ.get("GHOSTTY_SRC", "/Users/gael/projects/ghostty/src")
INV = os.path.join(G.RUSTTY_ROOT, ".port", "phase-0", "inventory")
sys.path.insert(0, INV)
import census_comments as CC  # noqa: E402  (reuse string-aware find_comment_start)

THRESHOLD = 0.60
MISS = 0.45
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

    overall_red = 0
    for crate in crates:
        rust_files = crate_rust_files(crate)
        # only meaningful for crates that have been ported (non-stub)
        done = any("PORT STATUS:" in open(f, encoding="utf-8", errors="replace").read()
                   and re.search(r"PORT STATUS:\s*done",
                                 open(f, encoding="utf-8", errors="replace").read())
                   for f in rust_files if f.endswith("lib.rs"))
        if not (args.strict or done or args.crate):
            continue
        ref_norm = []
        for f in crate_ref_files(crate, cm):
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

        missing, review = [], []
        for rc in ref_norm:
            if rc in port_blob:
                continue
            best = max((difflib.SequenceMatcher(None, rc, pc).ratio() for pc in port_norm),
                       default=0.0)
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
    a = ap.parse_args()
    if a.self_test:
        sys.exit(self_test())
    sys.exit(run(a))
