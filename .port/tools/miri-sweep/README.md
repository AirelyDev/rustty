# miri-sweep — per-test Miri lane driver with portable timeout

Runs a crate's Miri test lane **one test per process** with a hard per-test
timeout, so a single intractable test can't hang the whole lane, and every
test's verdict (ok / TIMEOUT / UB / ICE / FAIL) is recorded. Built during the
first full `rustty-vt` Miri sweep (2026-07-11; see `.port/phase-2/VT_MIRI_SWEEP.md`),
which found the search-subsystem SB UB (fixed, commit 9e3fe1c).

## Usage
```
# 1. list the crate's tests
cargo +nightly miri test -p <crate> --lib -- --list | sed -n 's/: test$//p' > tests.txt
# 2. run them, N workers, T-minute per-test timeout
bash .port/tools/miri-sweep/driver.sh tests.txt <workers> <timeout_min>
# 3. read verdicts
grep DONE <dir-of-tests.txt>/progress.log | sed -E 's/.*res=//' | sort | uniq -c
```

## Notes
- `ptimeout.pl` is a pure-Perl process-group timeout (macOS lacks `timeout`/
  `gtimeout`/`setsid`); it KILLs the whole process group so no orphan `miri`
  procs survive an overrun.
- **Parallel Miri over-reports FAIL** under memory pressure (6-8 concurrent
  interpreters OOM/ICE-cascade). UB is deterministic (trustworthy); TIMEOUT/FAIL
  candidates MUST be re-verified in isolation before being excepted (R5).
- The final gate lane (`check_leaks.sh` G6) runs `cargo miri test -p <crate> --lib`
  as one process with intractables `#[cfg_attr(miri, ignore)]`'d in source; this
  driver is the *discovery* tool that identifies which to annotate.
