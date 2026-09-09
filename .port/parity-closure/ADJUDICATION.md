# Parity-closure adjudication log

One section per DISPUTED finding ruled. Each entry cites what was actually
read on both sides of the oracle/rustty boundary, not just the audit
summaries.

## Batch 2a — src/terminal (20 rows)

### S174-F3 — WAIVED (not a defect)
- Refuting lens said: "The claimed observable difference does not follow from the cited line. ... the allocating capture's backing is `Vec::with_capacity(...)` and its growth ... is `reserve_exact` on that Vec, i.e. the Rust GLOBAL heap. So a caller-supplied CAllocator would not receive the dynamic OSC payload allocations even after the suggested fix ... Any genuine gap lives in rustty-vt's Vec-backed capture, not in osc.rs."
- Verified: read `crates/rustty-vt-c/src/osc.rs:34-90` (the `alloc: Option<CAllocator>` field on `OscParserWrapper` is only used by `free`, matching the oracle's `parser.alloc.?` release path) and `crates/rustty-vt/src/osc/mod.rs:880-953` (`ensure_allocator`/`capture_trailing`: the `alloc: Option<ZigAllocatorRef>` field is purely a presence gate) plus `:690-790` (`Capture::write_byte`/`write_slice`: the `Allocating` backing is a plain `Vec<u8>` grown via `reserve_exact`/`push`/`extend_from_slice`, which always uses the Rust global allocator regardless of what `self.alloc` holds). Applying the suggested fix (threading the caller allocator into `Parser::init`) would not change where the dynamic payload bytes are allocated, since the capture's `Vec` never consults that field. The refutation is correct.
- Disposition: state=WAIVED

### S177-F2 — WAIVED (not a defect)
- Refuting lens said: "`Duration::from_nanos(u64::MAX)` is ~18,446,744,073 seconds (~1.8e10), and on unix `Instant`'s add goes through `Timespec::checked_add_duration`, which checked-adds that into an i64 `tv_sec` ... i64::MAX is ~9.2e18, so no representable u64 `time_ns` can overflow it ... `instant_from_ns` is total over the whole u64 domain, exactly like oracle."
- Verified: read `crates/rustty-vt-c/src/selection_gesture.rs:1745-1764` (`instant_from_ns` computes `base + Duration::from_nanos(ns)` where `base` is a process-lifetime `OnceLock<Instant>`) and the oracle `src/terminal/c/selection_gesture.zig:741-743` (`instantFromNs` is `.fromNanoseconds(ns)`, total). The arithmetic checks out: u64::MAX ns is ≈18.4 billion seconds, five orders of magnitude below i64::MAX seconds; rustty is macOS/Linux-first (no freestanding-wasm target), so the `Instant + Duration` add can never actually overflow `tv_sec` for any `ns: u64`. The refutation's math is correct.
- Disposition: state=WAIVED

### S140-F2 — WAIVED (not a defect)
- Refuting lens said: "`max_scrollback_bytes` only affects pruning when rows scroll off the active area, and none of these five tests writes or scrolls on the alternate screen ... No input to the code under test and no asserted value can differ."
- Verified: read `crates/rustty-vt/src/pagelist/mod.rs:1540-1542` (`Key::Bytes => core::cmp::max(self.bytes.explicit, self.bytes.min)`) and `:1570-1600` (`min_max_size` computes at least two standard-page-sized allocations, hundreds of KB) confirming both `0` and `10_000` are floored to the identical effective limit. Also confirmed at `pagelist/mod.rs:1780` that `viewport: Viewport::Active` is the unconditional value at `PageList::init` (the only place `explicit == 0` changes observable behavior is the later `set_max_bytes` setter at `:2669-2670`, which none of these five construction-only tests calls). No input in these tests can distinguish 0 from 10_000.
- Disposition: state=WAIVED

### S145-F1 — WAIVED (not a defect)
- Refuting lens said: "Both values are clamped away by the same effective-minimum logic ... 1024 and 10_000 are orders of magnitude below that floor, so `limits.max(.bytes)` is byte-identical in the two runs."
- Verified: same `pagelist/mod.rs` floor logic as S140-F2 (`:1540-1542`, `:1570-1600`) confirms 1024 and 10,000 both floor to the same multi-hundred-KB minimum for an 80-ish-column/5-row terminal; `terminal.rs:16124` constructs via `Terminal::init` (no later `set_max_bytes` call in this test body), so the explicit-vs-floored distinction never surfaces.
- Disposition: state=WAIVED

### S146-F1 — WAIVED (not a defect)
- Refuting lens said: "Both 1024 and 10_000 are far below that floor, so the effective byte limit — and hence every pruning decision the test could observe — is byte-for-byte identical."
- Verified: identical reasoning and code (`pagelist/mod.rs:1540-1542`, `:1570-1600`) as S145-F1; `terminal.rs:16248` is a construction-only test with no subsequent `set_max_bytes` call, so the deleteLines-hyperlink-dense test cannot observe the fixture difference either.
- Disposition: state=WAIVED

### S148-F2 — WAIVED (not a defect)
- Refuting lens said: "The test asserts a differential invariant (print_slice must equal print) that holds for ANY input sequence, so the port's assertions are equal in meaning and no product behaviour differs — this is a test-input-generator substitution, not an observable functional difference."
- Verified: read `crates/rustty-vt/src/terminal.rs:17680-17742` (`TestRand`, an MMIX-style LCG, and the four `test_print_slice_differential` calls at geometries 80x24/10x4/5x2/2x2 with op counts 500/500/500/200) against the oracle's `std.Random.DefaultPrng.init(0xC0FFEE)` cited at `Terminal.zig:14495-14496`. Op alphabet, weights, geometries and op counts match; the invariant under test (`print_slice` output must equal `print` output) does not depend on which concrete sequence of legal ops is drawn, so a different PRNG algorithm changes test coverage breadth, not correctness of the assertion or observable terminal behavior.
- Disposition: state=WAIVED

### S149-F3 — WAIVED (not a defect)
- Refuting lens said: "Oracle bitmap_allocator.zig:211-217 `fn dumpBitmaps` is private ... and, grepping the ghostty tree, is never called by production or test code ... a difference confined to dead code."
- Verified: `grep -rn dumpBitmaps /Users/gael/projects/ghostty/src` returns only the declaration itself at `bitmap_allocator.zig:212` — no call site anywhere in the oracle tree. The port keeps the body as a PORT NOTE comment at `crates/rustty-vt/src/bitmap_allocator.rs:322-333`. No caller exists in either tree, so omitting the Rust counterpart is unobservable.
- Disposition: state=WAIVED

### S155-F1 — WAIVED (not a defect)
- Refuting lens said: "the set's OOM/NeedsRehash arms are either id-exhaustion-driven (next_id >= cap) or the statistically-unreachable psl_stats[31]>0 cold path ... The raw page bytes that compress/page.rs LZ4s are never oracle-compared or persisted across versions, so a self-consistent hash preserves every observable result."
- Verified: read `crates/rustty-vt/src/ref_counted_set.rs:570-620` — the `NeedsRehash`/`OutOfMemory` decisions are gated by `self.next_id.to_usize() >= self.layout.cap` and the `living` count vs. a 90% threshold, both pure insertion-count checks independent of which hash function placed which item; the only hash-sensitive cold path is `psl_stats[len-1] > 0`, which the audit's own PSL-distribution argument (and general robin-hood-hashing behavior at these capacities) makes statistically unreachable in both hash schemes. `grep`ing the crate for hyperlink-set golden/byte-comparison tests against the oracle found none (only unrelated `golden` fixtures for snapshot continuation/checkpoint records). No test or product path is shown to depend on the specific hash values chosen.
- Disposition: state=WAIVED

### S155-F3 — WAIVED (not a defect)
- Refuting lens said: "every product/test caller keys on Offset(Cell)/u32 ..., so the branch is unreachable in both trees ... a difference confined to unreachable code is refuted; it is documented in-place (W24-B03-ZST-01)."
- Verified: `grep -n "HashMap<\|AutoHashMap" crates/rustty-vt/src/*.rs` shows the only product instantiation is `AutoOffsetHashMap<Offset<Cell>, Id, 80>` (`hyperlink.rs:57`), and `Offset<Cell>`/`u32` keys used throughout `hash_map.rs`'s own tests are all non-zero-sized. The `@sizeOf(K) == 0` branch is dead in the product in both trees, and the divergence is explicitly documented at the cited site.
- Disposition: state=WAIVED

### S163-F1 — WAIVED (not a defect)
- Refuting lens said: "The only fallible paths in the sole implementer are allocation ones ... in the Rust port Rust's global allocator aborts on OOM, so the differing arm is unreachable rather than a different result."
- Verified: read `crates/rustty-inspector/src/widgets/termio.rs:1810-1846` and `crates/rustty-datastruct/src/circ_buf.rs` (`append` at :76-78 returns `Error::OutOfMemory` only as a logical "ring full" check, not a real allocation failure; `resize` at :165-193 calls `rotate_to_zero` — which, read at :198-212, has no error path and always returns `Ok(())` — then `Vec::resize`, which aborts rather than returning `Err` on genuine system OOM). So the grow-then-append / evict-then-append handling in `vt_raw` always succeeds in practice (matching the oracle's own catch-and-recover logic, ported faithfully per the code comments), and the only way the swallowed `let _ =` results could actually differ from the oracle is a true system allocation failure, which aborts the Rust process exactly as the crate-wide convention dictates. No reachable non-OOM input produces a different `vt_raw` return value.
- Disposition: state=WAIVED

### S163-F3 — WAIVED (not a defect)
- Refuting lens said: "the projection is dead in the oracle at pin 20abdb50a: `grep -rn 'Stream.Action|stream.Action' ghostty/src` yields only `.Tag`/`.Value` comptime-dispatch uses ... and no header in include/ghostty/vt exposes a stream Action C type."
- Verified: `grep -rn 'PrintSlice\|ApcPutSlice' crates/rustty-vt-c` (already cited in the audit) returns nothing, confirming rustty-vt-c never mirrors this projection; cross-checked that the PORT NOTE's claim ("the mirror lives in rustty-vt-c") is simply wrong wording, but since no oracle consumer needs the C projection at this pin, there is no missing capability, only a stale comment.
- Disposition: state=WAIVED

### S165-F1 — WAIVED (not a defect)
- Refuting lens said: "In the parity test configuration (cargo test, debug ...) all five arms run with the same assertions. The cfg_attr only affects a non-default `--release` run; it is test-harness hygiene, not an observable functional difference."
- Verified: read `crates/rustty-vt/src/stream.rs:6838-6928` — the `#[cfg_attr(not(debug_assertions), ignore)]` only skips under `cargo test --release`; all five arms (three `ContinuationDisabled`, one empty-continuation, one `WriteFailed`, plus the `#[cfg(debug_assertions)]`-gated `ContinuationUnavailable` tripwire arm) run under the default `cargo test`. Additionally confirmed at `/Users/gael/projects/ghostty/build.zig:386` that the oracle's own test step is hard-pinned to `.optimize = .Debug` regardless of any release flag passed to `zig build` — i.e. the oracle's canonical test lane is ALWAYS Debug, exactly mirroring Rust's default `cargo test`. Neither project's recognized test lane runs this test at a reduced-safety optimization level, so there is no coverage gap in the lane that matters for parity.
- Disposition: state=WAIVED

### S170-F2 — WAIVED (not a defect)
- Refuting lens said: "No terminal input or protocol path can produce non-UTF-8 reason bytes, so no input/state yields a different observable result; the difference exists only for a hypothetical external constructor of the Rust type, which is not a Ghostty parity path."
- Verified: `grep -rln "ResponseRegisterReason\|apc::glyph::response" crates/rustty-vt-c/src` returns no files — this type is not exposed across the C ABI boundary at all, so there is no external/foreign caller that could construct a non-UTF-8 `.Other`/clear reason. Confirmed in `crates/rustty-vt/src/apc/glyph/response.rs` that the only in-tree constructors use ASCII literals (`b"future_reason"` in a test, `b"out_of_memory"` in `execute.rs`). The `expect()` panic is real code but genuinely unreachable from any Ghostty-parity input path.
- Disposition: state=WAIVED

### S176-F2 — WAIVED (not a defect)
- Refuting lens said: "the registry is a plain `HashSet<*mut SearchWrapper>` ... backed by the Rust global allocator, which aborts on allocation failure by language convention ... this reduces to an unreachable error path."
- Verified: `crates/rustty-vt-c/src/terminal.rs:237` (`pub searches: HashSet<*mut SearchWrapper>`) and `:1709` (`HashSet::new()`) confirm this is `std::collections::HashSet`, whose growth aborts the process on real allocation failure rather than returning a `Result`. The `debug_assert!(inserted)` preserves the oracle's uniqueness invariant faithfully; only genuine OOM (abort, crate-wide convention) differs from the oracle's `.out_of_memory` return.
- Disposition: state=WAIVED

### S180-F2 — WAIVED (not a defect)
- Refuting lens said: "The only delta is the `#[cfg(debug_assertions)]` gate ... the rustty-vt tripwire fail point itself only exists under debug_assertions ... Zig has no release-mode test lane equivalent that this actually loses."
- Verified: same test-lane fact established for S165-F1 — `/Users/gael/projects/ghostty/build.zig:386` pins the oracle's test step to `.optimize = .Debug` unconditionally, so `zig build test` never runs this test (or any test) under a release-optimized build; Rust's default `cargo test` is the equivalent lane and runs `get_vt_processing_error` (`crates/rustty-vt-c/src/terminal.rs:6252-6316`) with full assertion parity. The `#[cfg(debug_assertions)]` gate only removes the test under the non-canonical `cargo test --release`.
- Disposition: state=WAIVED

### S190-F2 — WAIVED (not a defect)
- Refuting lens said: "under the port-wide OOM=abort policy that arm is unreachable code, and no input/state produces a different observable result ... Per the ground rules, a difference confined to unreachable code is refuted."
- Verified: `crates/rustty-vt/src/kitty/graphics_storage.rs:806-822` (`Vec::push`, PORT NOTE acknowledging the oracle's degraded-but-safe `catch { continue; }` arm is unreachable) and `:1268-1300` (`convert_image_to_rgba` is infallible, calling `rgba_from_format` which itself aborts on OOM). Both differences are confined to genuine system-allocator-exhaustion, which is the crate-wide OOM=abort convention already established across S163-F1/S174-F3/S176-F2 in this same batch; no reachable input differs.
- Disposition: state=WAIVED

### S204-F1 — WAIVED (not a defect)
- Refuting lens said: "the only divergence is behaviour under global-allocator exhaustion ... The staged buffer is bounded by Self::MAX_STAGED_PAYLOAD ... PayloadDecodeError::OutOfMemory is preserved and still produced on TerminalPage::init failure ... Difference confined to a path unreachable outside OS allocator failure."
- Verified: `crates/rustty-vt/src/snapshot/page.rs:457` (`vec![0u8; remaining]`) and `:561-573` (`grid::StyleRemap::init()` / `HyperlinkRemap::init()` returning bare values) confirm the allocations are infallible Rust `Vec`s (abort on OOM), matching the crate-wide convention; `PayloadDecodeError::OutOfMemory` remains a live variant produced elsewhere in the same file (per the audit's citation of page.rs:303/369/536), so the enum itself is not orphaned — only this specific allocator-exhaustion arm is unreachable.
- Disposition: state=WAIVED

### S207-F1 — WAIVED (not a defect)
- Refuting lens said: "the restored primary AND alternate screens both end with total_limit 320MB in both trees; the 0 at snapshot/terminal.rs:1240 lives only on an object that is deinit'd and overwritten before any kitty command can reach it."
- Verified: read `crates/rustty-vt/src/snapshot/codec.rs:870-965` directly. `ready_inner` builds a `screen_options` closure (`:899-919`) that explicitly sets `kitty_image_storage_limit: 320 * 1000 * 1000` and `Limits::DIRECT`; the subsequent per-screen decode loop (`:919-945`) calls `screen::decode(..., screen_options())` for every declared screen slot (both primary and alternate, when present) and then (per the code at `:940-965`) `deinit`s the placeholder slot from `terminal::decode` and installs the freshly-decoded replacement screen — which carries the 320MB limit, not the placeholder's `0`. The `kitty_image_storage_limit: 0` cited at `snapshot/terminal.rs:1236-1251` lives only on the transient placeholder object that is always overwritten before any Kitty command can observe it, exactly as the refutation states.
- Disposition: state=WAIVED

### S207-F2 — WAIVED (not a defect)
- Refuting lens said: "MODE_BITS_MASK is computed from the symbol `terminal_modes::ModePacked::BITS`, not from the literal, so the mask is `(1<<43)-1` and every decode/truncate path behaves identically to the oracle. A stale parenthetical inside a PORT NOTE produces no observable functional difference."
- Verified: read `crates/rustty-vt/src/snapshot/terminal.rs:285-291` — `const MODE_BITS_MASK: u64 = (1u64 << terminal_modes::ModePacked::BITS) - 1;` uses the symbol, and the "(42)" appears only inside the English PORT NOTE comment two lines above. The mask's runtime value is unaffected by the comment's wrong number.
- Disposition: state=WAIVED

### S209-F1 — WAIVED (not a defect)
- Refuting lens said: "in ReleaseFast (Ghostty's release mode) @intCast on out-of-range input is UB, so there is no reference-defined trap to port ... This is the generic Zig-safety-check-vs-Rust-`as` class, not a functional-parity finding."
- Verified: confirmed Ghostty's actual packaged/production build uses `ReleaseFast` — `/Users/gael/projects/ghostty/flake.nix:165` (`ghostty = final.callPackage ./nix/package.nix (mkPkgArgs "ReleaseFast")`, the default flake output) and `:119` (`ghostty-releasefast`). Zig's `@intCast` safety check (illegal-behavior detection) is compiled out under `ReleaseFast`/`ReleaseSmall` by language design, becoming plain UB rather than a defined trap. Since the shipped oracle binary does not actually trap on this overflow, there is no oracle-defined behavior for rustty's `as` truncation to diverge from; both trees have unchecked/UB behavior on this path in their respective release configurations. `crates/rustty-vt/src/tmux/viewer.rs:935-950` carries the oracle's own TODO acknowledging the cast is unsafe.
- Disposition: state=WAIVED
