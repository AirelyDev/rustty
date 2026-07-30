#!/usr/bin/env python3
"""One-shot probe: how much can rustty regress before G8 (frozen-JSON, +/-7%
vs the ORACLE's numbers) notices? Answer: a lot, because rustty is 7-60%
faster than the oracle and G8 spends that entire margin as headroom. This is
the quantitative argument for why G8b (the paired self fence) is the real gate.

Usage: headroom_probe.py <candidate-metrics.json>
"""
import json
import sys

BASE = ".port/phase-0/baseline/BASELINE.json"


def main():
    base = {b["id"]: b
            for b in json.load(open(BASE))["benchmarks"]
            if b.get("optimize") == "ReleaseFast"}
    cand = {b["id"]: b for b in json.load(open(sys.argv[1]))["benchmarks"]}
    print(f"{'config':32s} {'G8 headroom':>12s}")
    print(f"{'':32s} {'(how much slower rustty may get before G8 fires)':>12s}")
    for bid, c in cand.items():
        b = base[bid]
        if b.get("metric") == "throughput_MiB_per_s":
            floor = b["throughput_MiB_per_s"] * 0.93   # higher-is-better, -7%
            head = (c["throughput_MiB_per_s"] - floor) / c["throughput_MiB_per_s"]
        else:
            ceil = b["median_s"] * 1.07                # lower-is-better, +7%
            head = (ceil - c["median_s"]) / c["median_s"]
        print(f"{bid:32s} {head*100:11.1f}%")


if __name__ == "__main__":
    main()
