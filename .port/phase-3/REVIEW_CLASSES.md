# Standing review classes for Phase 3

`P3-W0 T7`. Four cross-cutting hazards that belong to no single commit and would
otherwise be re-litigated at every site. **These are binding rules for every
task in `docs/superpowers/plans/2026-07-29-phase3-upstream-resync.md`, not
advice.**

---

## 1. Release arithmetic — a byte-faithful transcription can be *worse* than the oracle

Zig's `+`/`-`/`*` trap on overflow **only in safety builds** (`Debug`,
`ReleaseSafe`). Rust's **panic in debug and WRAP in release**. So a literal
transcription of a Zig arithmetic expression is:

* **equivalent** in debug (both abort),
* **worse in release** — Zig `ReleaseFast` also wraps, but rustty ships
  `overflow-checks = true` on some profiles and not others, so the *same source*
  has two behaviours, and the wrap is silent where the oracle's safety build
  would have caught it in testing.

### The required translations

| Zig | Rust | never |
|---|---|---|
| `@abs(x)` | `x.unsigned_abs()` | `(-x) as usize` |
| `a +\| b`, `a -\| b` | `a.saturating_add(b)`, `a.saturating_sub(b)` | plain `+`/`-` |
| `std.math.add(T, a, b) catch <fallback>` | `a.checked_add(b).unwrap_or(<the oracle's fallback>)` | `unwrap()` |
| `@intCast` where the oracle relies on a safety trap | `try_into()` with the oracle's error arm | `as` |

### The required SHAPE: saturate **and** assert

The exemplar is already in the tree —
`crates/rustty-vt/src/compress/lz4.rs`'s `match_end_position` replaces the
oracle's trapping `limit - position` with:

```rust
limit.saturating_sub(position)
// plus
debug_assert!(position <= limit);
```

Saturating alone would let an invariant break be silently absorbed; the
`debug_assert` alone would leave release wrapping. **Both, always.** A site that
has only one of the two is not done.

### Instances, not exceptions

The four `P3-W1` arithmetic fixes — `c753fe4a4`, `afbf5ba15`, `e6e4a9fdc`,
`30b42f42a` — are instances of this class. So is
`.port/phase-3/REACHABILITY.md` row 4 (`pagelist/scroll.rs:253,346`,
`(-n) as usize` on `isize::MIN`), and so are rows 1/2 (`pin.rs`'s
`extra % cols - 1`). Fixing one of them without applying the rule to the others
in the same file is how this class regenerates.

---

## 2. `rustty-bench` gating

**Status: the plan's premise here is STALE, and the step was re-derived.**

The plan's step 2 says `grep -rn 'rustty-bench' .port/tools/gates/ .port/tools/ci.sh`
returns nothing, and asks for a new test asserting the complete action set, to
land **RED** because "upstream has 11 `Action` variants; `cli.rs:22-41` has 9".

None of that is true on `main`:

* `.port/tools/gates/check_bench_tests.sh` exists (**G-BENCH**, WS10 T4) and is
  registered `G-BENCH|HARD` in `ci.sh:282`. It runs the crate's lib tests in
  **both** debug and `--release` and enforces a test-count floor, precisely
  because the workspace lanes run the crate but cannot see a test being deleted.
* `cli.rs` has **11** `Action` variants, and
  `bench_every_action_round_trips_through_from_name` already asserts every one
  of them round-trips, with `assert_eq!(11, names.len())` and two negative
  probes.
* `crates/rustty-bench/src/hyperlink_map.rs` (354 lines) and `apc_parser.rs`
  (255 lines) were ported by WS10 T11 (`b8d24a6`).

### What was actually missing, and what was added

The checked-in list pins rustty against **itself**. It catches a *deleted*
action; an action **upstream adds** is invisible to it — which is exactly how
`hyperlink-map` and `apc-parser` were absent with the suite green until WS10
T11. G-BENCH now carries an **oracle-driven** arm:

```
=== G-BENCH: oracle action-set parity ===
OK: 11 actions, identical to the oracle's
```

It reads `git show origin/main:src/benchmark/cli.zig` (never a checkout),
extracts the `Action` enum tokens, and diffs them **both directions** against
the checked-in list in `cli.rs`. Oracle absent -> **exit 3 BLOCKED**. Zero
actions parsed from either side -> **loud FAIL**, never a clean run.

**NEGATIVE CONTROL** (removing `apc-parser` from the checked-in list):

```
RED: the rustty-bench action set differs from the oracle's.
  in the oracle, ABSENT from rustty:
    apc-parser
rc=1
```

**Rule for the rest of Phase 3:** a checked-in expectation list is only a
regression guard. Anything that must track upstream needs the oracle on the
driving side of the comparison, or it can be satisfied by shrinking rustty.

---

## 3. `cfg(test)` is not transitive — the `is_test` translation class

Zig's `builtin.is_test` is true for the **entire module graph** compiled by
`zig build test`. Rust's `cfg(test)` is true only for the **crate currently
being compiled as a test**. A `cfg!(test)` branch ported from `is_test`
therefore changes behaviour for every *other* crate that links it under test —
silently, and in the direction of doing MORE real work, not less.

### The instance that was fixed

`crates/rustty-vt/src/mem.rs:108,173,254` branch on `cfg!(test) || cfg!(miri)`
to simulate a successful page decommit by zeroing. Upstream's benchmark tests
see that simulation; rustty's `rustty-bench` tests linked `rustty-vt` with
`cfg(test)` **false** and therefore exercised the **real kernel
`MADV_FREE_REUSABLE` path**. They pass on macOS/arm64 today, but
`scrollback_compression_restores_cold_terminal_pages` asserts
`compressed_pages > 0`, which then depends on a real kernel call succeeding: on
any target where `can_reclaim(Strict)` is false (non-Darwin, non-Linux,
non-64-bit) it **FAILS where upstream's equivalent passes**.

Fix: a crate-external feature `test-decommit-sim`
(`crates/rustty-vt/Cargo.toml`), added to the three conditions as
`cfg!(test) || cfg!(miri) || cfg!(feature = "test-decommit-sim")`, and enabled
by `rustty-bench` from a **`[dev-dependencies]`** entry — deliberately *not* a
normal dependency, so it is ON for `cargo test -p rustty-bench` and OFF for the
shipped `rustty-bench` binary, which must measure the real kernel path.

### The sweep — every `cfg!(test)` in a library crate

`grep -rn 'cfg!(test)' crates/*/src --include='*.rs'`, triaged for whether it
changes library **behaviour** (as opposed to being a test-only helper):

| site | changes behaviour? | verdict |
|---|---|---|
| `rustty-vt/src/mem.rs:108,173,254` | **YES** — real syscall vs zeroing simulation | **FIXED** above |
| `rustty-vt/src/page.rs:394` `grapheme_bytes: if cfg!(test) { 512 } else { 8192 }` | **YES** — page geometry differs | **ACCEPT, with a caveat.** This is the oracle's own `is_test` page-size shrink. It is *desirable* that `rustty-bench` sees the production 8192: a benchmark on 512-byte grapheme capacity would measure the wrong thing. But it does mean a rustty-vt test and a rustty-bench test observe different page layouts. `pagelist/mod.rs:740` already documents the consequence ("no libtest, `cfg!(test)` false => std_size 589824"), so the divergence is known and recorded rather than accidental. Owner if it ever bites: `P3-W3`. |
| `rustty-args/src/diagnostics.rs:225` | no — selects the diagnostic sink for `.exe`/`.wasm` artifacts, matching the oracle's `builtin.is_test` | accept |
| `rustty-renderer/src/shadertoy.rs:335` | no — test-only path selection | accept |
| `rustty-vaxis/src/loop_.rs:286` | no — test-only path selection | accept |
| `rustty-util/src/tripwire.rs:177` | no — the comment there **already states** the non-transitivity explicitly; it is the pre-existing statement of this exact class | accept |
| `rustty-vt/src/lib.rs:89`, `rustty-zigstd/src/memory_pool.rs:252`, `rustty-vt/src/pagelist/mod.rs:740` | no — comments/doc only | accept |

**Rule:** every new `cfg!(test)` in a **library** crate must state, in a comment
at the site, whether a *different* crate's test build seeing the `false` arm is
correct. If it is not, use a feature.

---

## 4. `lz4_differential_exhaustive`'s false PASS

`crates/rustty-vt/src/compress/lz4_differential.rs` early-returns unless
`RUSTTY_LZ4_SLOW` is set, and that variable was set **nowhere** in the repo — no
gate, no `ci.sh` entry, no workflow. The shape is faithful (`GHOSTTY_LZ4_SLOW`
is likewise unset upstream), but it is **worse in one specific way**: Zig's
`error.SkipZigTest` reports **SKIPPED** while a Rust early return reports
**PASSED**, so every reader of `cargo test` output was told that 4 seeds x the
exhaustive budget had just run.

Both halves of the fix are in:

1. `#[ignore = "slow (~minutes); run via bash .port/tools/ci.sh --lz4-slow …"]`
   — the honest equivalent of SKIPPED. The env gate stays as a second guard so
   `--include-ignored` without the variable is still a no-op.
2. A periodic lane, `bash .port/tools/ci.sh --lz4-slow`, that actually sets it.

The lane has its **own** anti-vacuity guard, because "1 passed" is exactly what
an env-gated early return also prints: the test emits
`RUSTTY_LZ4_SLOW_RAN seeds=4` **after** all four seeds have run, the lane runs
with `--nocapture`, and a missing marker is a loud FAIL. Run it on any change
under `crates/rustty-vt/src/compress/`, alongside **G-LZ4**.

**Rule:** any test whose body can `return` early on an environment condition
must either be `#[ignore]`d or emit a positive side-channel marker that its
runner checks. "Passed" must never be able to mean "did nothing".

---

## 5. Perf-baseline invalidation — record this BEFORE anyone re-benchmarks

**The recorded `perf-baseline-rust-vs-zig` 14/14 result is not comparable to
`origin/main`, and a smaller rustty margin against it is the ORACLE GETTING
FASTER, not rustty regressing.**

Inside `4749c4e93..6ad1fe7d8` the oracle got **~2.8x faster on
`+terminal-stream`** via `20c3eae04` / `src/quirks_memset.zig`:

* Zig 0.16 globally disabled LLVM's loop auto-vectorization;
* that made compiler_rt's byte-loop `memset` ~24x slower, and it became **63 %
  of executed instructions** in that benchmark;
* upstream hand-`@Vector`-ized it, with an aarch64 `dc zva` path.

**rustty does not port it, deliberately.** `rustc` lowers `@llvm.memset` to the
platform libc `memset`, which is already the vectorized implementation the
quirk file is reconstructing. Exporting a strong `memset` from a Rust dylib
would *pessimize* rustty relative to libc and risks ODR / symbol-interposition
breakage across the staticlib + dylib + Swift-app link graph.

Consequences, binding on any future comparison:

1. A rustty-vs-`origin/main` `+terminal-stream` comparison will show a smaller
   rustty margin than the recorded 14/14. **That is expected.** Do not open a
   regression investigation on it.
2. The recorded 14/14 numbers were taken against the **pinned** oracle
   (`4749c4e93`) and remain valid *for that pin only*.
3. Re-confirm `-Doptimize=ReleaseFast` on any zigbench build before comparing —
   the recorded baseline requires it, and the default is not it.
4. The deferred perf campaign (V17, owner Phase 3, `BEYOND-PIN`) produces real
   numbers only **after** `P3-W10` advances the pin. Anything measured before
   that is comparing against a two-versions-old oracle.
5. The box is not idle: load has been sampled at 4.6-10.9 against an
   `IDLE_MAX` of 2.0. Use instantaneous `top -l`; `ps` lifetime averages
   mislead. A suite exiting **2 (provisional)** is correct behaviour, not a
   failure to work around.
