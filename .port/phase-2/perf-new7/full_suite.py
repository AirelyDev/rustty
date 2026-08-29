#!/usr/bin/env python3
"""Unified full-suite comparison: the original 14 configs (phase-0 harness)
plus the 19 perf-new7 configs = 33, against one oracle binary. Methodology
identical to run.py / run_benchmarks.py: single-process time.monotonic()
wall-clock, 1 discarded warmup + N reps, median reported, output discarded.

Usage: full_suite.py <zig-bin> <rust-bin> <out-prefix> [N]
Writes <out-prefix>-zig.jsonl and <out-prefix>-rust.jsonl.
"""
import sys, os, json, time, subprocess, statistics

ZIG_BIN, RUST_BIN, OUT = sys.argv[1], sys.argv[2], sys.argv[3]
N = int(sys.argv[4]) if len(sys.argv) > 4 else 7

ROOT = "/Users/gael/projects/rustty"
C0 = f"{ROOT}/.port/phase-0/baseline/corpora"
C7 = f"{ROOT}/.port/phase-2/perf-new7/corpora"

CONFIGS = [
    # original 14 (phase-0 harness shape)
    ("terminal-parser/utf8", f"{C0}/utf8.bin", ["+terminal-parser"]),
    ("terminal-stream/ascii", f"{C0}/ascii.bin", ["+terminal-stream"]),
    ("codepoint-width.noop/utf8", f"{C0}/utf8.bin", ["+codepoint-width", "--mode=noop"]),
    ("codepoint-width.table/utf8", f"{C0}/utf8.bin", ["+codepoint-width", "--mode=table"]),
    ("codepoint-width.simd/utf8", f"{C0}/utf8.bin", ["+codepoint-width", "--mode=simd"]),
    ("codepoint-width.wcwidth/utf8", f"{C0}/utf8.bin", ["+codepoint-width", "--mode=wcwidth"]),
    ("grapheme-break.noop/utf8", f"{C0}/utf8.bin", ["+grapheme-break", "--mode=noop"]),
    ("grapheme-break.table/utf8", f"{C0}/utf8.bin", ["+grapheme-break", "--mode=table"]),
    ("is-symbol.table/utf8", f"{C0}/utf8.bin", ["+is-symbol", "--mode=table"]),
    ("is-symbol.uucode/utf8", f"{C0}/utf8.bin", ["+is-symbol", "--mode=uucode"]),
    ("osc-parser/osc-parser", f"{C0}/osc-parser.bin", ["+osc-parser"]),
    ("screen-clone.noop/small", f"{C0}/ascii-small.bin", ["+screen-clone", "--mode=noop"]),
    ("screen-clone.clone/small", f"{C0}/ascii-small.bin", ["+screen-clone", "--mode=clone"]),
    ("screen-clone.render/small", f"{C0}/ascii-small.bin", ["+screen-clone", "--mode=render"]),
    # perf-new7's 19
    ("apc-parser", f"{C7}/kitty-4m.vt", ["+apc-parser"]),
    ("hyperlink-map.lookup", None, ["+hyperlink-map", "--mode=lookup", "--loops=50"]),
    ("hyperlink-map.churn", None, ["+hyperlink-map", "--mode=churn", "--loops=50"]),
    ("terminal-resize.cols", None, ["+terminal-resize", "--mode=cols", "--loops=5"]),
    ("terminal-resize.cols-no-reflow", None, ["+terminal-resize", "--mode=cols-no-reflow", "--loops=5"]),
    ("terminal-resize.rows", None, ["+terminal-resize", "--mode=rows", "--loops=5"]),
    ("terminal-resize.both", None, ["+terminal-resize", "--mode=both", "--loops=5"]),
    ("terminal-snapshot.encode", f"{C7}/ascii-2m.vt", ["+terminal-snapshot", "--mode=encode", "--loops=20"]),
    ("terminal-snapshot.decode", f"{C7}/ascii-2m.vt", ["+terminal-snapshot", "--mode=decode", "--loops=20"]),
    ("terminal-snapshot.decode-ready", f"{C7}/ascii-2m.vt", ["+terminal-snapshot", "--mode=decode-ready", "--loops=20"]),
    ("terminal-formatter.noop", f"{C7}/styled-1m.vt", ["+terminal-formatter", "--mode=noop"]),
    ("terminal-formatter.format-vt", f"{C7}/styled-1m.vt", ["+terminal-formatter", "--emit=vt", "--loops=50"]),
    ("terminal-formatter.format-plain", f"{C7}/styled-1m.vt", ["+terminal-formatter", "--emit=plain", "--loops=50"]),
    ("scrollback-compression.compress", f"{C7}/ascii-16m.vt", ["+scrollback-compression", "--mode=compress"]),
    ("scrollback-compression.incremental", f"{C7}/ascii-16m.vt", ["+scrollback-compression", "--mode=incremental"]),
    ("scrollback-compression.restore", f"{C7}/ascii-16m.vt", ["+scrollback-compression", "--mode=restore"]),
    ("page-compression.compress", f"{C7}/ascii-16m.vt", ["+page-compression", "--mode=compress"]),
    ("page-compression.store", f"{C7}/ascii-16m.vt", ["+page-compression", "--mode=store"]),
    ("page-compression.decompress", f"{C7}/ascii-16m.vt", ["+page-compression", "--mode=decompress"]),
]


def time_one(argv):
    s = time.monotonic()
    r = subprocess.run(argv, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return time.monotonic() - s, r.returncode


def run_suite(binpath, out_path):
    with open(out_path, "w") as f:
        for id_, corpus, args in CONFIGS:
            argv = [binpath] + args
            if corpus:
                argv += ["--data=" + corpus]
            print("## %s" % id_, file=sys.stderr, flush=True)
            _, rc0 = time_one(argv)
            if rc0 != 0:
                f.write(json.dumps({"id": id_, "error": "nonzero_exit", "rc": rc0}) + "\n")
                f.flush()
                continue
            reps = [time_one(argv)[0] for _ in range(N)]
            med = statistics.median(reps)
            f.write(json.dumps({
                "id": id_, "median_s": med, "min_s": min(reps), "max_s": max(reps),
                "reps_s": reps, "n": N, "cmd": " ".join(argv),
            }) + "\n")
            f.flush()
    print("DONE: %s" % out_path, file=sys.stderr, flush=True)


run_suite(ZIG_BIN, OUT + "-zig.jsonl")
run_suite(RUST_BIN, OUT + "-rust.jsonl")
