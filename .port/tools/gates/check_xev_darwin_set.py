#!/usr/bin/env python3
"""check_xev_darwin_set.py — the rustty-xev Darwin-gate scanner.

`libxev` gates its kqueue family on `.ios, .macos, .visionos` (`backend.zig:20,38`,
`kqueue.zig:29-36,45-46,1861-1862`) and its mach paths on `isDarwin()`, which
covers all three. A rustty `#[cfg]` that names `macos` WITHOUT `ios`/`visionos` is
therefore a NARROWING of the reference's own target set -- the defect that left an
iOS build holding `no_backend_seam`'s `unimplemented!()` placeholders while
`backend.rs` answered `Backend::Kqueue`.

This scans the cfg ATTRIBUTES (never comments, never doc text) of the crate's
backend-selection files and fails on any such narrowing.

An arm that genuinely means "the macOS HOST specifically" -- a host test asserting
what `cargo test` sees on this box -- is exempt, but must SAY so: put a
`GATE-OK(darwin-set):` comment on a line immediately above the attribute. That
keeps the exemption explicit and greppable rather than encoded in this file as a
line number that rots.

Exit 0 = clean, 1 = a narrowing was found.
"""

import io
import os
import re
import subprocess
import sys

FILES = ["lib.rs", "kqueue.rs", "async.rs", "thread_pool.rs", "backend.rs"]
CFG_ATTR = re.compile(r"^\s*#!?\[cfg[(!]")
EXEMPT = "GATE-OK(darwin-set):"


def main() -> int:
    root = subprocess.check_output(
        ["git", "rev-parse", "--show-toplevel"], text=True
    ).strip()
    src = os.path.join(root, "crates", "rustty-xev", "src")

    findings = []
    scanned = 0
    for name in FILES:
        path = os.path.join(src, name)
        lines = io.open(path, encoding="utf-8").read().splitlines()
        for i, line in enumerate(lines):
            if not CFG_ATTR.match(line):
                continue
            scanned += 1
            if 'target_os = "macos"' not in line:
                continue
            if "visionos" in line:
                continue
            # Exempt only when the exemption is stated on an immediately
            # preceding comment line.
            j = i - 1
            exempt = False
            while j >= 0 and lines[j].lstrip().startswith("//"):
                if EXEMPT in lines[j]:
                    exempt = True
                    break
                j -= 1
            if exempt:
                continue
            findings.append((name, i + 1, line.strip()))

    # Vacuity floor: if the scanner stops SEEING cfg attributes at all (a file
    # renamed, a path typo), it must not report a clean bill of health.
    if scanned < 40:
        print(
            "VACUOUS: only %d cfg attributes scanned across %s -- the scanner is "
            "not reaching the code it claims to check." % (scanned, ", ".join(FILES)),
            file=sys.stderr,
        )
        return 1

    if findings:
        print(
            "rustty-xev Darwin-set NARROWING: %d cfg attribute(s) gate `macos` "
            "without `ios`/`visionos`.\n" % len(findings),
            file=sys.stderr,
        )
        for name, lineno, text in findings:
            print("  crates/rustty-xev/src/%s:%d  %s" % (name, lineno, text), file=sys.stderr)
        print(
            "\nlibxev gates these on isDarwin() / `.ios, .macos, .visionos`. Widen "
            "the cfg to match, or -- if the arm really is macOS-host-specific -- "
            "put a `%s` comment directly above it saying why." % EXEMPT,
            file=sys.stderr,
        )
        return 1

    print("rustty-xev Darwin set OK: %d cfg attributes scanned, 0 narrowings." % scanned)
    return 0


if __name__ == "__main__":
    sys.exit(main())
