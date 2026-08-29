#!/usr/bin/env python3
"""Perf comparison harness for the 7 benchmarks added to ghostty src/benchmark/
in the last ~2 months (2026-06-28..2026-08-28) and ported to crates/rustty-bench:
scrollback-compression, page-compression, apc-parser, hyperlink-map,
terminal-resize, terminal-snapshot, terminal-formatter.

Same methodology as .port/phase-0/baseline/run_benchmarks.py (the original
14-config comparison): single-process time.monotonic() wall-clock around each
`<bin> +action ...` invocation (RunMode .once, no internal timing printed),
1 discarded warmup + N timed reps, median reported, stdout+stderr -> DEVNULL.

Both binaries are driven with byte-identical CLI flags (rustty-bench's arg
parser preserves ghostty-bench's kebab-case flag names verbatim) and
byte-identical corpora (generated once by rustty-synthetic; ghostty-gen output
is independently verified byte-identical to rustty-synthetic's, see
PERF_COMPARISON.md, so either generator would give the same bytes).

Usage: run.py <zig-bin> <rust-bin> <out-dir> [N]
"""
import sys, os, json, time, subprocess, statistics

ZIG_BIN = sys.argv[1]
RUST_BIN = sys.argv[2]
OUT_DIR = sys.argv[3]
N = int(sys.argv[4]) if len(sys.argv) > 4 else 7

COR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "corpora")
ASCII_SMALL = os.path.join(COR, "ascii-2m.vt")   # terminal-snapshot
ASCII_BIG = os.path.join(COR, "ascii-16m.vt")     # scrollback-compression, page-compression
STYLED = os.path.join(COR, "styled-1m.vt")        # terminal-formatter
KITTY = os.path.join(COR, "kitty-4m.vt")          # apc-parser


def sz(p):
    return os.path.getsize(p)


def time_one(argv):
    s = time.monotonic()
    r = subprocess.run(argv, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return time.monotonic() - s, r.returncode


def bench(fout, id_, binpath, corpus, nbytes, args):
    argv = [binpath] + args
    if corpus:
        argv = argv + ["--data=" + corpus]
    print("## %s  cmd: %s" % (id_, " ".join(argv)), file=sys.stderr, flush=True)
    _, rc0 = time_one(argv)  # warmup (discarded)
    if rc0 != 0:
        print("   WARMUP NONZERO EXIT %d -- SKIPPING" % rc0, file=sys.stderr, flush=True)
        fout.write(json.dumps({"id": id_, "error": "nonzero_exit", "rc": rc0, "cmd": " ".join(argv)}) + "\n")
        fout.flush()
        return
    reps = []
    for i in range(N):
        t, rc = time_one(argv)
        reps.append(t)
        print("   rep %d: %.4fs" % (i + 1, t), file=sys.stderr, flush=True)
    med = statistics.median(reps)
    rec = {
        "id": id_, "corpus": os.path.basename(corpus) if corpus else None, "bytes": nbytes,
        "cmd": " ".join(argv), "reps_s": reps, "median_s": med,
        "min_s": min(reps), "max_s": max(reps), "n": N,
    }
    if nbytes > 0:
        rec["throughput_MiB_s"] = (nbytes / (1024 * 1024)) / med
    fout.write(json.dumps(rec) + "\n")
    fout.flush()


def run_suite(binpath, out_path):
    ab, sb, kb, bigb = sz(ASCII_SMALL), sz(STYLED), sz(KITTY), sz(ASCII_BIG)
    with open(out_path, "w") as f:
        # apc-parser
        bench(f, "apc-parser", binpath, KITTY, kb, ["+apc-parser"])

        # hyperlink-map (synthetic, no corpus)
        bench(f, "hyperlink-map.lookup", binpath, None, 0, ["+hyperlink-map", "--mode=lookup", "--loops=50"])
        bench(f, "hyperlink-map.churn", binpath, None, 0, ["+hyperlink-map", "--mode=churn", "--loops=50"])

        # terminal-resize (synthetic fill, no corpus)
        bench(f, "terminal-resize.cols", binpath, None, 0, ["+terminal-resize", "--mode=cols", "--loops=5"])
        bench(f, "terminal-resize.cols-no-reflow", binpath, None, 0, ["+terminal-resize", "--mode=cols-no-reflow", "--loops=5"])
        bench(f, "terminal-resize.rows", binpath, None, 0, ["+terminal-resize", "--mode=rows", "--loops=5"])
        bench(f, "terminal-resize.both", binpath, None, 0, ["+terminal-resize", "--mode=both", "--loops=5"])

        # terminal-snapshot
        bench(f, "terminal-snapshot.encode", binpath, ASCII_SMALL, ab, ["+terminal-snapshot", "--mode=encode", "--loops=20"])
        bench(f, "terminal-snapshot.decode", binpath, ASCII_SMALL, ab, ["+terminal-snapshot", "--mode=decode", "--loops=20"])
        bench(f, "terminal-snapshot.decode-ready", binpath, ASCII_SMALL, ab, ["+terminal-snapshot", "--mode=decode-ready", "--loops=20"])

        # terminal-formatter
        bench(f, "terminal-formatter.noop", binpath, STYLED, sb, ["+terminal-formatter", "--mode=noop"])
        bench(f, "terminal-formatter.format-vt", binpath, STYLED, sb, ["+terminal-formatter", "--emit=vt", "--loops=50"])
        bench(f, "terminal-formatter.format-plain", binpath, STYLED, sb, ["+terminal-formatter", "--emit=plain", "--loops=50"])

        # scrollback-compression
        bench(f, "scrollback-compression.compress", binpath, ASCII_BIG, bigb, ["+scrollback-compression", "--mode=compress"])
        bench(f, "scrollback-compression.incremental", binpath, ASCII_BIG, bigb, ["+scrollback-compression", "--mode=incremental"])
        bench(f, "scrollback-compression.restore", binpath, ASCII_BIG, bigb, ["+scrollback-compression", "--mode=restore"])

        # page-compression
        bench(f, "page-compression.compress", binpath, ASCII_BIG, bigb, ["+page-compression", "--mode=compress"])
        bench(f, "page-compression.store", binpath, ASCII_BIG, bigb, ["+page-compression", "--mode=store"])
        bench(f, "page-compression.decompress", binpath, ASCII_BIG, bigb, ["+page-compression", "--mode=decompress"])

    print("DONE: %s" % out_path, file=sys.stderr, flush=True)


def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    print("=== ZIG (%s) ===" % ZIG_BIN, file=sys.stderr, flush=True)
    run_suite(ZIG_BIN, os.path.join(OUT_DIR, "zig.jsonl"))
    print("=== RUST (%s) ===" % RUST_BIN, file=sys.stderr, flush=True)
    run_suite(RUST_BIN, os.path.join(OUT_DIR, "rust.jsonl"))


if __name__ == "__main__":
    main()
