#!/usr/bin/env python3
"""Phase-0 performance baseline harness for the ghostty reference oracle.

Measures end-to-end wall-clock process time of `ghostty-bench <action>` over
pre-generated corpora. All timing is done inside THIS single Python process
(time.monotonic deltas around subprocess.run) so the clock epoch is consistent
-- an earlier bash version that spawned a separate python per timestamp produced
garbage because monotonic clocks are not comparable across processes.

Methodology:
  - ghostty-bench runs each step EXACTLY ONCE (RunMode .once) and PRINTS NOTHING
    (it discards the internal RunResult). The only available signal is the
    end-to-end process wall-clock, which we measure.
  - Wall-clock includes process spawn + the benchmark's (untimed) setup. For all
    actions except screen-clone, setup just opens the data file (cheap). For
    screen-clone, setup replays the corpus into the terminal, so a SMALL corpus
    keeps setup negligible vs the fixed 1000-iteration clone step loop.
  - stderr is discarded: terminal-stream/-parser emit per-byte `warn` log lines
    on control/garbage bytes; we don't want terminal logging I/O to dominate.
  - Throughput = corpus_bytes / median_seconds for data-driven benches.
    screen-clone reports clones/sec = 1000 / median_seconds.
  - N timed reps + 1 warmup (discarded). Median reported (also min/max/all reps).

Usage: run_benchmarks.py <bench-binary> <results.jsonl> [N]
"""
import sys, os, json, time, subprocess, statistics

BENCH = sys.argv[1]
RESULTS = sys.argv[2]
N = int(sys.argv[3]) if len(sys.argv) > 3 else 7

COR = "/Users/gael/projects/rustty/.port/phase-0/baseline/corpora"
UTF8 = os.path.join(COR, "utf8.bin")
ASCII = os.path.join(COR, "ascii.bin")
OSCP = os.path.join(COR, "osc-parser.bin")
SMALL = os.path.join(COR, "ascii-small.bin")


def sz(p):
    return os.path.getsize(p)


def time_one(argv):
    s = time.monotonic()
    subprocess.run(argv, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    return time.monotonic() - s


def bench(fout, id_, corpus, nbytes, args):
    argv = [BENCH] + args + ["--data=" + corpus]
    print("## %s  cmd: %s" % (id_, " ".join(argv)), file=sys.stderr, flush=True)
    time_one(argv)  # warmup (discarded)
    reps = []
    for i in range(N):
        t = time_one(argv)
        reps.append(t)
        print("   rep %d: %.4fs" % (i + 1, t), file=sys.stderr, flush=True)
    med = statistics.median(reps)
    rec = {
        "id": id_, "corpus": os.path.basename(corpus), "bytes": nbytes,
        "cmd": " ".join(argv), "reps_s": reps, "median_s": med,
        "min_s": min(reps), "max_s": max(reps), "n": N,
    }
    if nbytes > 0:
        rec["throughput_MiB_s"] = (nbytes / (1024 * 1024)) / med
    else:
        # data-independent fixed-work bench (screen-clone: 1000 clones/step)
        rec["clones_per_s"] = 1000.0 / med
    fout.write(json.dumps(rec) + "\n")
    fout.flush()


def main():
    ub, ab, ob, sb = sz(UTF8), sz(ASCII), sz(OSCP), sz(SMALL)
    with open(RESULTS, "w") as f:
        bench(f, "terminal-parser/utf8", UTF8, ub, ["+terminal-parser"])
        bench(f, "terminal-stream/ascii", ASCII, ab, ["+terminal-stream"])
        bench(f, "codepoint-width.noop/utf8", UTF8, ub, ["+codepoint-width", "--mode=noop"])
        bench(f, "codepoint-width.table/utf8", UTF8, ub, ["+codepoint-width", "--mode=table"])
        bench(f, "codepoint-width.simd/utf8", UTF8, ub, ["+codepoint-width", "--mode=simd"])
        bench(f, "codepoint-width.wcwidth/utf8", UTF8, ub, ["+codepoint-width", "--mode=wcwidth"])
        bench(f, "grapheme-break.noop/utf8", UTF8, ub, ["+grapheme-break", "--mode=noop"])
        bench(f, "grapheme-break.table/utf8", UTF8, ub, ["+grapheme-break", "--mode=table"])
        bench(f, "is-symbol.table/utf8", UTF8, ub, ["+is-symbol", "--mode=table"])
        bench(f, "is-symbol.uucode/utf8", UTF8, ub, ["+is-symbol", "--mode=uucode"])
        bench(f, "osc-parser/osc-parser", OSCP, ob, ["+osc-parser"])
        bench(f, "screen-clone.noop/small", SMALL, 0, ["+screen-clone", "--mode=noop"])
        bench(f, "screen-clone.clone/small", SMALL, 0, ["+screen-clone", "--mode=clone"])
        bench(f, "screen-clone.render/small", SMALL, 0, ["+screen-clone", "--mode=render"])
    print("DONE", file=sys.stderr, flush=True)


if __name__ == "__main__":
    main()
