# P8 final validation

- Full run head: `f5f2bdf28` (W10 T0–T4 merged). Oracle pin: `246f70287`.
- Fixes after the full run, verified by targeted reruns at `11b4c74d0` (user direction 2026-10-10: "EXPEDITE and do partial reruns to verify the fixes, we can save the full rerun for later"):
  - FX10 `c841df229` (W10 review minors; ids N545, N546, UD379);
  - FX11 `1a32bce23` (N547, was provisional N565: `sys_log_no_alloc` counts only the measuring thread);
  - FX12 `1ed503e6d` (N548, was provisional N567: `search_thread` waits and joins);
  - FX13 `11b4c74d0` (ci.sh: Valgrind shard jobs default to cores / heavy cap).

## Verdict

GREEN for the scope run, with a named DEFERRED list (below). No open RED: each RED in the full run is either a disk-floor stop, a fixed finding verified by a scoped rerun, or N549 (closed by the perf-window quiet rerun).

## Hosts

| Host | What ran | Result |
|---|---|---|
| Mac (14 cores) | full `ci.sh --jobs auto --heavy-cap 3 --prune-targets` (`ci-full-mac.log`, `ci-timings-mac.tsv`) | 98 GREEN, 14 BLOCKED (host tooling: Valgrind, MSan, HWASan, GTKFEAT, BLUEPRINT, XTARGET-SELF, LSAN-GTK; G-NIX pre-existing; G4 drivers, then G4 GREEN with built drivers `G4-mac.log`, `G4-uni-mac.log`; G8b load cooldown), 4 RED (all resolved, see table notes). Miri G6, G6c, G6t all GREEN. |
| rvm (Fedora, 8 vCPU, 9 GB) | full `ci.sh --jobs auto --heavy-cap 2 --prune-targets` (`rvm/ci-run/`) | 94 GREEN including G-ESCTEST, G-GTKFEAT, G-BENCH, G-WINCHK, G-SAN-SELF; G-VALGRIND COMPLETE GREEN (1/4 + 2/4 in ci.sh, 3/4 + 4/4 as 8 exact sub-shards `manual2-G-VALGRIND-K-of-16.log`). Sanitizer gates did not produce results (disk floor, then an OOM thrash and VM restart): DEFERRED. |
| rwin (Windows 11 ARM64) | V26 | DEFERRED (not run). |

## Fix verification (`fixverify-*-mac.log`, Mac, `11b4c74d0`)

| Fix | Check | Result |
|---|---|---|
| FX10 | `cargo test -p rustty-vt soft_reset` / `formatter`; `cargo test -p rustty-termio`; G-SYMS + G-SYMS-SELF | 8 / 111 / 121 passed; GREEN (2911 `pub fn`, 0 unexplained) |
| FX11 | `SAN_ONLY=rustty-vt-c sanitizers.sh --gate tsan` | SCOPED-GREEN, 604 passed |
| FX12 | `SAN_ONLY=rustty-vt sanitizers.sh --gate lsan`; `cargo test -p rustty-vt search::thread` | SCOPED-GREEN, 6898 passed; 3 passed |
| FX13 | G-CI-SELF (new arm-D default + override; negative control RED) | GREEN |

## DEFERRED to the later full rerun

- rvm: G-ASAN (full), G-TSAN, G-MSAN, G-HWASAN, G-LSAN, G-LSAN-GTK. None produced a verdict this run: the first attempt stopped at the 15 GB disk floor, and the rerun was lost when the VM thrashed and was restarted.
- rvm: G4 with built drivers.
- rwin: V26.

## Perf

Mac idle (both VMs off, load < 2.0), at `11b4c74d0`; evidence `perf/`, report `W10-PERF-report.md` in the SDD ledger.

- N549 (provisional N566): `readthread_pipeline` 10/10 on a quiet machine; closed as load-sensitive.
- ws10 suite vs the oracle at 246f70287: **14/14 wins**, suite 3,240 → 2,352 ms (−27.4%). Worst: terminal-stream/ascii 0.979 (lost the first N=9 round at 1.013, won the built-in N=18 re-run; 0.965 in the G8b run). terminal-parser 0.233, osc-parser 0.304.
- G8 re-base: 14 timing rows and 3 size labels from the 246f70287 oracle; tolerances unchanged (`949584651`).
- `check_baseline_regression.py --strict`: GREEN (17 metrics, 0 regressions). G8b `--build`: GREEN, 14/14 (−27.7%).

Findings ids: `findings.md` keeps the runner's provisional ids; final ids N547 (N565), N548 (N567), N549 (N566), per the phase README.

## Re-audit

Done as a delta, a60e9e2a5..246f70287 (DECSTR, getDefault, formatter cursor origin, header docs): no findings. The full re-audit at a60e9e2a5 stands.

## Incidents and CI gaps (for the next CI pass)

- ci.sh left `CI_VALGRIND_JOBS` empty, so shards ran `--jobs 1` (rvm ~72% idle): fixed in FX13.
- `--prune-targets` pruned `ci-targets/valgrind` once killed duplicate shards "ended" the gate.
- Manual runs started beside ci.sh don't count against its heavy cap; two sanitizer builds linking ~12 test binaries at ~600 MB each OOM'd the 9 GB VM. Cap sanitizer link parallelism (`CARGO_BUILD_JOBS`) on small VMs.
- `--prune-targets` does not cover `target/debug` or `san-*`; G4 drivers are not auto-built in a fresh checkout; the G-REL latency test should run exclusive; a full parallel run needs ~70 GB free on the Mac.

## All gates (Mac and rvm, from `ci-timings-*.tsv`; seconds are wall time)

See `gate-table.md` (117 gates).
