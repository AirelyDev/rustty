# Parity-closure adjudication log

One section per DISPUTED finding ruled. Each entry cites what was actually
read on both sides of the oracle/rustty boundary, not just the audit
summaries.

## Batch 2a — src/terminal (20 rows)

### S174-F3 — UPHELD (real defect) [REVISED — see fix report below]
- Refuting lens said: "The claimed observable difference does not follow from the cited line. ... the allocating capture's backing is `Vec::with_capacity(...)` and its growth ... is `reserve_exact` on that Vec, i.e. the Rust GLOBAL heap. So a caller-supplied CAllocator would not receive the dynamic OSC payload allocations even after the suggested fix ... Any genuine gap lives in rustty-vt's Vec-backed capture, not in osc.rs."
- Why that is wrong: the refutation only attacks the *citation* (whether `osc.rs`'s own lines are the mechanism), not the *claim*. The claim as stated by the finding — "C-created OSC parser does not use the caller-supplied allocator for its dynamic payloads" — is true and embedder-visible regardless of which file the root cause lives in. Oracle `ghostty/src/terminal/c/osc.zig:35` does `ptr.* = .init(alloc)` where `alloc = lib.alloc.default(alloc_)` (the caller-supplied `CAllocator`, resolved a few lines above): the parser's own allocator IS the embedder's allocator, so every dynamic OSC growth (payloads past `MAX_BUF`, e.g. OSC 52 clipboard) goes through it. Rustty `crates/rustty-vt-c/src/osc.rs:70` instead does `osc::Parser::init(Some(ZigAllocatorRef::new(&GLOBAL_ZIG_ALLOCATOR)))` — a process-static allocator — and only retains the caller's `CAllocator` for `free` (`osc.rs:100-115`). Confirmed independently that `crates/rustty-vt/src/osc/mod.rs`'s `Capture::backing` growth (`:690-790`, `Vec::reserve_exact`/`push`) never consults `self.alloc` at all, i.e. the parser's `alloc` field is currently a dead presence-gate everywhere in vt, not just at this C-created call site — so the divergence is real at BOTH layers, and relocating it to rustty-vt does not make it not-a-defect; it just re-scopes which file the fix belongs in. No ledger row besides S174-F3 tracks this embedder-allocator gap for OSC (`grep -n osc LEDGER.tsv` for allocator-related rows returns only this one), and its three same-shape siblings — S175-F1 (RenderState), S176-F1 (search engine), S179-F2 (Terminal internals), all "bypass the caller-supplied C allocator" — remain CONFIRMED/OPEN, so waiving this one alone would be an inconsistent, and terminal, gap in coverage.
- Disposition: state=OPEN, flows into its severity phase (medium)

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

### S209-F1 — UPHELD (real defect) [REVISED — see fix report below]
- Refuting lens said: "in ReleaseFast (Ghostty's release mode) @intCast on out-of-range input is UB, so there is no reference-defined trap to port ... This is the generic Zig-safety-check-vs-Rust-`as` class, not a functional-parity finding."
- Why that is wrong (revised): the original waiver picked the ReleaseFast lane for this ruling while the S165-F1/S180-F2 rulings in this same batch picked the Debug lane (justified by ghostty's own `build.zig:386` pinning its test step to `.optimize = .Debug`) — an inconsistent choice of "canonical" oracle configuration used to reach opposite outcomes (waive here, waive there, for different reasons). Resolving this with a single doctrine: the safety-checked builds (Debug and ReleaseSafe) are still first-class, supported Ghostty configurations — `nix/package.nix:29` (`strip = optimize != "Debug" && optimize != "ReleaseSafe"`) and the flake's own non-ReleaseFast outputs show ReleaseSafe/Debug are shipped configurations, not just a dev-only artifact — and in those configurations `ghostty/src/terminal/tmux/viewer.zig:1131-1140`'s `@intCast(layout.width)`/`@intCast(layout.height)` DOES trap (a defined, observable abort) on tmux control-mode layout input reporting width/height > 65535. `crates/rustty-vt/src/tmux/viewer.rs:940-947`'s `d.width as size::CellCountInt` / `d.height as size::CellCountInt` never traps in any Rust build profile — it silently wraps modulo 2^16, producing a wrong (possibly zero) pane size instead. So on a malformed/hostile tmux layout string, the two trees diverge observably in at least one real, shipped oracle configuration (crash-with-diagnostic vs. silently-mis-sized-pane-with-downstream-fallout), which is exactly the class of defect this audit exists to catch. Per the asymmetry rule (when the lane choice is inconsistent and reasonable people could read it either way, UPHOLD rather than WAIVE), this is flipped.
- Disposition: state=OPEN, flows into its severity phase (low)

### S45-F3 — WAIVED (not a defect)
- Refuting lens said: "The two differ only for values > isize::MAX (>= 2^63 on 64-bit), i.e. a search match count or match index exceeding nine quintillion, which cannot be constructed — every match/index originates from terminal screen contents bounded by addressable memory ... Refuted as a non-observable difference."
- Verified: read oracle `src/apprt/action.zig:1034-1051` (`.total = if (self.total) |t| @intCast(t) else -1,` and the identical `.selected` arm) and rustty `crates/rustty-apprt-api/src/action.rs:1479-1487,1502-1510` (`total: match self.total { Some(t) => t as isize, None => -1 }`). Both sides carry an `Option<usize>`/`?usize` sourced from search match counts/indices, which can never approach `isize::MAX` (~9.2×10^18) given any real terminal scrollback bounded by system memory. The `None -> -1` sentinel path is byte-identical on both sides.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s45-f3

### S47-F2 — WAIVED (not a defect)
- Refuting lens said: "Under adw_version.zig:47-63 atLeast folds at comptime against the build headers, so with the AD-3-pinned adw>=1.5 headers every one of those predicates is compile-time true ... Documented, ratified deviation with equivalent behaviour: refuted for this file."
- Verified: read oracle `src/apprt/gtk/adw_version.zig:101-120` (five `pub inline fn supports*()` wrapping `atLeast(1,x,0)` at or below the 1.5 floor) and rustty `crates/rustty-apprt/src/gtk/adw_version.rs:1-34`, whose module doc enumerates every oracle call site (`dialog.zig:21`, `surface_child_exited.zig:15`, `debug_warning.zig:14,42`, `clipboard_confirmation_dialog.zig:17`) as comptime type/const selections that collapse to the modern arm under AD-3's ratified `adw >= 1.5` floor, plus the one runtime-`if` extension (`window.zig:1799`, tracked separately as S58-F2) explicitly flagged in `.port/phase-2/DEFERRALS.md` as a documented deviation, not silently dropped.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s47-f2

### S47-F3 — WAIVED (not a defect)
- Refuting lens said: "The oracle's short-circuit is driven by the BUILD BOX's gtk/gtk.h macros, not by a fixed floor ... rustty's BASELINE (4.14/1.5) is documented ... as an API-surface ceiling, not a version oracle, and folding it would const-false window.zig's live 4.16 CSS-class check ... Refuted."
- Verified: read oracle `src/apprt/gtk/gtk_version.zig:1-66` (`atLeast` = `comptime comptime_version.order(threshold)==.lt -> return false` then `runtimeAtLeast`) and rustty `crates/rustty-apprt/src/gtk/gtk_version.rs:1-160`, whose module doc and `at_least` docstring cite the exact site (`window.zig:695`, the `window-theme-ghostty` CSS-class gate) that a naive `BASELINE`-fold would silently break, and record that DEFERRALS ratifies the live-delegate choice. This is a fully reasoned, ratified AD-3 architectural decision, not an unexamined gap.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s47-f3

### S47-F4 — WAIVED (not a defect)
- Refuting lens said: "`grep -rn getAlloc /Users/gael/projects/ghostty/src` shows no caller of gsettings.getAlloc anywhere ... and Key.@\"gtk-font-name\" is read by nothing. Zig never even semantically analyses the unreferenced pub fn ... Difference is confined to code unreachable in the oracle; refuted."
- Verified: `grep -rn "getAlloc" /Users/gael/projects/ghostty/src` returns only the definition site and two unrelated `environ.getAlloc` hits, confirming zero callers of `gsettings.getAlloc`/`Key.gtk-font-name` in the oracle. Read oracle `src/apprt/gtk/gsettings.zig:1-90` (defines `getAlloc`, `getImpl`'s `[]const u8` arm) against rustty `crates/rustty-apprt/src/gtk/gsettings.rs:1-170`, which ports `Key`/`KeyType`/`GValueType`/`requires_allocation` (including the `GtkFontName` variant and all four oracle tests) faithfully and only omits the allocating reader, whose own module doc states plainly "lands with its first consumer (none at this pin)".
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s47-f4

### S48-F2 — WAIVED (not a defect)
- Refuting lens said: "Trigger.key.unicode is populated only from validated UTF-8 decodes ... or from an unshifted_codepoint produced by the apprt key layer, so a surrogate/out-of-range value is not reachable through any real input path ... Difference confined to effectively unreachable input — refuted."
- Verified: read oracle `src/apprt/gtk/key.zig:70-90` and `:373-391` (`.unicode => |cp| if (gdk.keyvalName(cp)) |name| ... else try writer.print("{u}", .{cp})`) against rustty `crates/rustty-apprt/src/gtk/key.rs:130-152` and `:335-355` (`else if let Some(c) = char::from_u32(cp) { out.push(c); }`, silently appending nothing when `char::from_u32` fails). Checked `input::Binding.Trigger`'s `unicode` field type (`u21` at `Binding.zig:1717`) is populated only via `std.unicode.Utf8View` decode of validated UTF-8 (`Binding.zig:1804-1813`) or ASCII literals (`Binding.zig:1786,1849-1860`) — never a surrogate or codepoint > 0x10FFFF. Confirmed the same on the rustty side: `crates/rustty-input/src/binding.rs:3225` (`TriggerKey::Unicode(cp as u21)`) is likewise fed only from a validated codepoint decode. No reachable trigger construction can carry an invalid scalar value on either side.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s48-f2

### S51-F1 — WAIVED (not a defect)
- Refuting lens said: "`grep -rn 'getMut\\b' /Users/gael/projects/ghostty/src` returns exactly one hit — the definition itself ... so the function has zero call sites in the entire oracle tree ... Under the semantic-equivalence lens this is a difference confined to unreachable code."
- Verified: `grep -rn "getMut\b" /Users/gael/projects/ghostty/src` returns only `src/apprt/gtk/class/config.zig:98` (the definition). Read oracle `config.zig:85-100` (`get`/`getMut` are twin accessors over the same private field) against rustty `crates/rustty-apprt/src/gtk/class/config.rs:143-174`, which ports `new`/`get` faithfully and simply has no `get_mut` — dead, uncalled API surface on the oracle side, so omitting it changes no observable behaviour.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s51-f1

### S51-F8 — WAIVED (not a defect)
- Refuting lens said: "I grepped both trees for writers: oracle has only application.zig:1371 (open_uri) and :1509 (`priv.global_shortcuts.setDbusConnection(...)`); rustty has only application.rs:465 and :864 calling `set_dbus_connection` ... No g_object_set(\"dbus-connection\") / construct-time write exists in either tree ... Refuted per the unreachable-code rule."
- Verified: read oracle `src/apprt/gtk/class/global_shortcuts.zig:40-59` (plain `privateObjFieldAccessor` on the property, no side effects) and `:508-540` (`setDbusConnection` performs close/unref/ref/refresh/notify) against rustty `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs:145-152,177-198` (the `#[property(set = Self::set_dbus_connection)]` derive folds the same logic into the property setter). Confirmed via grep that oracle `application.zig:1371,1509` and rustty `application.rs:465,864` are the sole writers on each side, and both always route through the method with side effects — no `g_object_set("dbus-connection")` call exists anywhere in either tree to exercise the divergent path.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s51-f8

### S52-F2 — WAIVED (not a defect)
- Refuting lens said: "For a getter-backed read-only property the ParamSpec default is never applied to any instance and every g_object_get returns the getter's value, so no input/state yields a different result ... Not an observable functional difference."
- Verified: read oracle `src/apprt/gtk/class/inspector_window.zig:47-64` (`debug` property: `.default = build_config.is_debug`, getter-only accessor unconditionally returning `build_config.is_debug`) against rustty `crates/rustty-apprt/src/gtk/class/inspector_window.rs:58-62,164-168` (`#[property(name = "debug", get = Self::debug_prop)]` with `debug_prop` returning `cfg!(debug_assertions)` — identical value to the getter on both sides). Read-only, getter-backed GObject properties are always resolved via the getter on `g_object_get`; the pspec default is only consulted by introspection tooling that neither side's template (`inspector-window.blp:29` binds the getter's live value) touches.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s52-f2

### S52-F4 — WAIVED (not a defect)
- Refuting lens said: "The null branch requires the two decimal renderings plus '/' to exceed 31 bytes, i.e. counts in the 10^15+ range for search match indices ... unreachable with any real state ... (the finder themselves note 'unreachable with real match counts')."
- Verified: read oracle `src/apprt/gtk/class/search_overlay.zig:303-318` (`var buf: [32]u8 = undefined; ... catch return null;` around `bufPrintZ("{}/{}", .{selected+1, total})`) against rustty `crates/rustty-apprt/src/gtk/class/search_overlay.rs:359-372` (`format!("{sel}/{total}")`, always succeeds). A 32-byte buffer overflow requires match counts in the quadrillions, which cannot occur for any real terminal search; the finding's own text concedes this is unreachable with real match counts.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s52-f4

### S55-F3 — WAIVED (not a defect)
- Refuting lens said: "initActionMap (:1874-1884) creates the action via `initStateful(..., glib.Variant.newBoolean(false))` and GSimpleAction enforces the state's variant type on every set_state, so no code path can install a non-boolean state. No input produces a different observable result."
- Verified: read oracle `src/apprt/gtk/class/surface.zig:2651-2663` (`if (state.isOfType(bool_variant_type)==0) return;`) against rustty `crates/rustty-apprt/src/gtk/class/surface.rs:3173-3185` (`action.state().and_then(|s| s.get::<bool>()).unwrap_or(false)` then `set_state(!value)`). Confirmed both sides construct the `notify-on-next-command-finish` action stateful-boolean at init (oracle `surface.zig:1879-1883` `initStateful(..., newBoolean(false))`; rustty `surface.rs:1339-1349` `init_stateful(..., false.to_variant())`), and `GSimpleAction`/`gio::SimpleAction` enforces the initial state's `GVariantType` on every subsequent `set_state`/`change_state` call — a non-boolean state can never be installed on either side, so the divergent branch is unreachable.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s55-f3

### S55-F6 — WAIVED (not a defect)
- Refuting lens said: "I read all three rustty arms — noop.rs:158-163, wayland.rs:736-742, x11.rs:551-558 — each returns Ok(()) with no fallible operation ... matching the oracle arms whose only error source is Zig allocator/bufPrint failure that cannot trigger here. So no input or state produces a different result today."
- Verified: read oracle `src/apprt/gtk/winproto/noop.zig:81` (`pub fn addSubprocessEnv(_: *Window, _: *Environ.Map) !void {}`), `wayland.zig:256-258` (no-op), and `x11.zig:331-338` (`bufPrint("{}", .{xid})` into a 64-byte buffer, which cannot fail for any `u32`/`u64` XID, then `env.put`). Read rustty `crates/rustty-apprt/src/gtk/winproto/wayland.rs:737-742` (`Ok(())`), `noop.rs:158-163` (`Ok(())`), and `x11.rs:551-558` (`env.put(...); Ok(())`). All three implementations return `Ok(())` unconditionally on both sides — the propagate-vs-log divergence at `surface.rs:2564-2577` is genuinely unreachable given the current (and structurally fixed) set of winproto arms.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s55-f6

### S56-F2 — WAIVED (not a defect)
- Refuting lens said: "The panic arm is dead code: surface.zig:4247-4250 constructs the dialog with `can-remember = false` for exactly those four variants ... and nothing ever activates [the switch row] ... so `remember` is always false for those requests. The difference is confined to unreachable code."
- Verified: read oracle `src/apprt/gtk/class/surface.zig:4240-4325` (`ClipboardConfirmationDialog` constructed with `.@"can-remember" = switch (req) { .osc_52_read, .osc_52_write => true, .paste,.list,.kitty_read,.kitty_write => false }`, and `clipboardConfirmationDeny`'s `@panic` arm covers exactly those four `false` variants) and `src/apprt/gtk/class/clipboard_confirmation_dialog.zig:16-17,127-135,235-251` plus `ui/1.4/clipboard-confirmation-dialog.blp:91` (`Adw.SwitchRow remember_choice { visible: bind template.can-remember; }` — invisible, hence never toggled true, exactly when `can-remember` is false). Read rustty `crates/rustty-apprt/src/gtk/class/surface.rs:2483-2492` (`eprintln!(...)` instead of `@panic`). The panic arm is structurally unreachable on both sides: the UI can never produce `remember=true` for a request whose `can-remember` is false.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s56-f2

### S56-F5 — WAIVED (not a defect)
- Refuting lens said: "the sole caller in the whole workspace is surface.rs:1606 `.set_child_exited_data(Some(value))` (grepped crates/), mirroring the oracle's single non-null caller. Difference confined to unreachable code."
- Verified: read oracle `src/apprt/gtk/class/surface_child_exited.zig:76-91` (`setData`'s null-payload arm sets `priv.data = null` and returns before `notifyByPspec`) against rustty `crates/rustty-apprt/src/gtk/class/surface_child_exited.rs:174-181` (`set_child_exited_data` delegates to the `#[property(get, set, nullable)]`-generated setter, which always notifies). `grep -rn "set_child_exited_data" crates/rustty-apprt/src` returns exactly one call site, `surface.rs:1606`, passing `Some(value)` — matching the oracle's sole caller (`surface.zig:1545`), which likewise always passes non-null. The null-payload divergence is unreachable on both sides.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s56-f5

### S58-F2 — WAIVED (not a defect)
- Refuting lens said: "adw::AboutDialog is v1_5-gated and the crate compiles against a pinned adw >= 1.5 floor (gtk.rs:54,90-99), so the legacy gtk.showAboutDialog arm is unreachable in any supported build ... The finder itself states no fix is required."
- Verified: read oracle `src/apprt/gtk/class/window.zig:1965-2004` (`if (adw_version.supportsDialogs()) adw.showAboutDialog(...) else gtk.showAboutDialog(...)`) against rustty `crates/rustty-apprt/src/gtk/class/window.rs:2172-2209` (`action_about` unconditionally builds and presents an `adw::AboutDialog`, with an explicit "AD-3: `adw_version.supportsDialogs()` collapses to the modern `adw` arm" comment). This is the same ratified AD-3 `adw >= 1.5` pinned-floor decision documented and cross-referenced at S47-F2/S47-F3; the legacy branch is unreachable in the crate's one supported build configuration.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s58-f2

### S58-F5 — WAIVED (not a defect)
- Refuting lens said: "The only scenario in which this differs is one where the oracle's pending timer fires against a finalized window — undefined behaviour in the oracle, and no defined-behaviour input produces a different observable result ... the finder itself recommends no change."
- Verified: read oracle `src/apprt/gtk/class/window.zig:1397-1414` (`dispose` removes only `handle_active_state_source`, leaving any pending 500ms `tab_overview_focus_timer` to fire after finalization — a latent UAF) against rustty `crates/rustty-apprt/src/gtk/class/window.rs:317-341` (`dispose` additionally does `remove_source_id(self.tab_overview_focus_timer.take(), ...)`, with an inline PORT NOTE documenting this as invariant 11's sound re-expression). The extra removal only changes behaviour inside the oracle's own UB window (a timer firing against a dead object); in every defined-behaviour scenario nothing observable differs, and rustty's version is strictly safer.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s58-f5

### S59-F4 — WAIVED (not a defect)
- Refuting lens said: "Because the timer is restarted by the newest entry's start(), every surviving entry's age at fire time is >= 31s (glib timeouts fire no earlier than requested) ... trunc(age) > 30 and age > 30s therefore agree on every reachable timing; no entry can be in the (30.0s, 31.0s) band when the callback runs."
- Verified: read oracle `src/apprt/gtk/portal/OpenURI.zig:505-561` (`startCleanupTimer` always `stopCleanupTimer()` then re-arms at `cleanup_timeout + 1` = 31s; `cleanup` is one-shot, returns `SOURCE_REMOVE`, never re-arms) against rustty `crates/rustty-apprt/src/gtk/portal/open_uri.rs:520-580` (`start_cleanup_timer` stops+restarts at `CLEANUP_TIMEOUT_SECS + 1` via `timeout_add_seconds_local_once`; `cleanup` clears `cleanup_timer` and does not re-arm — same one-shot shape). Because the single shared timer is always freshly re-armed to fire 31s from the newest `start()` call (not from each entry's own start), every entry alive when it fires is at least 31s old regardless of when it individually started — the claimed (30s, 31s) truncation-vs-`Duration` gap cannot be reached by any entry.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s59-f4

### S60-F2 — WAIVED (not a defect)
- Refuting lens said: "The only protocol delta is one extra xdg_activation_token_v1.destroy request on an already-done token, which is a legal destructor request with no compositor-visible effect, and take() makes double-destroy impossible. No input/state yields a different result."
- Verified: read oracle `src/apprt/gtk/winproto/wayland.zig:450-472` (`onActivationTokenEvent`'s `.done` arm: `activation.activate(...); token.destroy(); self.activation_token = null;`) against rustty `crates/rustty-apprt/src/gtk/winproto/wayland/globals.rs:529-568` (the `Dispatch` handler activates but leaves the token proxy and `Window.activation_token` alone, per its PORT NOTE deferring destruction to the single owner). Read both sides' `setUrgent`/`set_urgent` (oracle `wayland.zig:261-273`; rustty `wayland.rs:745-762`): **both unconditionally destroy any existing `activation_token` before creating a new one**, regardless of whether `done` already fired — so the functional outcome (surface gets activated once per urgent-transition, stale tokens never double-activate) is identical; the only difference is exactly when the redundant `destroy` request is sent.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s60-f2

### S60-F3 — WAIVED (not a defect)
- Refuting lens said: "The extra destroy is a destructor request on a proxy the client is discarding at window teardown (the oracle leaks it), and the flush only pushes already-queued requests earlier. Nothing observable to the user or to window/terminal behaviour changes; this is cleanup, not a functional difference."
- Verified: read oracle `src/apprt/gtk/winproto/wayland.zig:216-221` (`deinit`: frees `blur_region`, destroys `bg_effect`, releases `decoration`/`slide` — no `activation_token` handling, a documented leak) against rustty `crates/rustty-apprt/src/gtk/winproto/wayland.rs:664-686` (`deinit` does the same four operations plus `if let Some(token) = self.activation_token.take() { token.destroy(); } let _ = self.conn.flush();`, with an inline comment noting the reference leaves the token to `done`/next-`setUrgent`). Since the window and its surface are being torn down in both cases, no subsequent code on either side observes whether the token proxy was explicitly destroyed or left to the compositor/library to reap — purely a resource-cleanup improvement with no reachable behavioural difference.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s60-f3

### S60-F4 — WAIVED (not a defect)
- Refuting lens said: "the oracle's noop.zig updateConfigEvent is an empty body and, as the finder itself states, has no dispatcher in winproto.zig and no wayland/x11 implementation — it is unreachable dead code on the oracle side ... omitting an empty never-called function changes nothing."
- Verified: `grep -rn "updateConfigEvent" /Users/gael/projects/ghostty/src/apprt/gtk/winproto/*.zig /Users/gael/projects/ghostty/src/apprt/gtk/*.zig` returns exactly one hit — the empty definition itself at `noop.zig:64-67` — confirming `winproto.zig`'s `Window` union type has no dispatcher for it and neither `wayland.zig` nor `x11.zig` implements a matching method. `grep -rn "update_config_event"` across `crates/rustty-apprt/src` returns nothing, consistent with omitting genuinely dead, uncalled API surface on both sides.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s60-f4

### S91-F3 — WAIVED (not a defect)
- Refuting lens said: "The finding's own suggested fix is 'document it as an intentional hardening divergence' — which is already done, explicitly, at web_canvas.rs:44-58 ... The guard exists only because the port adds a modern-surface `Face::init(lib, source, opts)` shim ... The reference's initNamed is unreachable with non-CSS bytes by construction (its only caller is Wasm.face_new_, web_canvas.zig:497-503, which is fed a CSS string by the JS glue)."
- Verified: read oracle `src/font/face/web_canvas.zig:50-77` (`initNamed` copies `raw` with no validation) and confirmed via `grep -n initNamed` that its only caller in the oracle tree is `Wasm.face_new_` at `web_canvas.zig:497-503` (`export fn face_new(ptr, len, pts, p)` from JS). Read rustty `crates/rustty-font-shaper/src/face/web_canvas.rs:260-296` — the added `validate_css_font_str(raw)?` is stricter than the oracle only for non-CSS byte inputs (font-file magic bytes, non-UTF8, control chars), none of which the sole real caller (browser JS glue passing a CSS `font` string) can produce. This is opt-in hardening on a browser-only, non-macOS-first target, not a fidelity loss on any reachable input.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s91-f3

### S94-F1 — UPHELD (real defect)
- Refuting lens said: "The observable behaviour the oracle's CFReleaseThread provides ... IS present ... I also confirmed the real port exists and is complete at crates/rustty-os/src/cf_release_thread.rs:8 ('PORT STATUS: ported') ... the coretext.rs:41-53 note claiming the xev seam is 'all unimplemented!()' is stale documentation only (rustty-global has no seam.rs any more), which is at most a low/doc-level issue, not the medium functional finding described."
- Why that is wrong: the refutation itself concedes the two things that matter — (1) the PORT NOTE at `crates/rustty-font-shaper/src/shaper/coretext.rs:36-45` is stale: I grepped the whole workspace for `seam.rs` and found no `rustty_global::seam::Xev` (only an unrelated `crates/rustty-apprt-api/src/seam.rs`), and (2) `crates/rustty-os/src/cf_release_thread.rs:1-20` is a genuinely complete, tested-by-inventory port ("PORT STATUS: ported ... every line, comment, and test{} ported ... PARITY (SEAM SATISFIED, R4): ... backend is now the real rustty-xev kqueue Loop/Async") sitting directly below rustty-font-shaper in the dependency graph. The lens tries to reclassify this down to "at most a low/doc-level issue" but that is exactly the concedes-and-relocates pattern this batch is instructed to uphold on: a real, already-ported implementation is sitting unused while the shaper crate ships a hand-rolled stand-in under a doc comment that is factually false about why. No other ledger row (checked `S94-F4`, `S111-F4` — both about dropped log calls only) covers the stale-doc/unused-real-port gap itself.
- Disposition: state=OPEN, owner= (unchanged), flows into its severity phase

### S31-F2 — WAIVED (not a defect)
- Refuting lens said: "This is a compiler-diagnostic flag: it changes whether a warning becomes an error, not generated code or runtime behaviour. Moreover the oracle itself already passes -Wno-implicit-function-declaration globally ... No input/state yields a different observable result; the finder's own text concedes 'no runtime behaviour changes'."
- Verified: read oracle `pkg/fontconfig/build.zig:122-124` (global `-Wno-implicit-function-declaration`) and `:215-228` (Windows+libxml2 arm adds `-Werror=implicit-function-declaration`) against rustty `crates-sys/fontconfig-sys/build.rs:227-234` (defines added, flag omitted). Windows is not a rustty target (macOS-first locked fork); the flag only affects whether an implicit-function-declaration diagnostic is promoted to a hard error during compilation, never generated code.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s31-f2

### S31-F3 — WAIVED (not a defect)
- Refuting lens said: "grep -rn ImplOSX over /Users/gael/projects/ghostty/src and /pkg returns zero hits, so no oracle use-site ever calls these symbols ... A never-called extern declaration plus an uncompiled backend .mm produces no observable behavioural difference."
- Verified: `grep -rn "ImplOSX" /Users/gael/projects/ghostty` returns only the three declarations at `pkg/dcimgui/main.zig:30-32` themselves — no caller anywhere in `src/`. Rustty `crates-sys/dcimgui-sys/src/lib.rs:154-330` correctly omits them; the macOS inspector path goes through the Metal backend that IS present.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s31-f3

### S31-F4 — WAIVED (not a defect)
- Refuting lens said: "Grepping the oracle src/ for DockBuilder ... shows exactly those 6 are ever called; the other 10 are declaration-only in the oracle. Missing unused extern declarations in a bindings shim change no runtime behaviour at any use-site."
- Verified: `grep -n DockBuilder src/inspector/widgets.zig src/inspector/widgets/surface.zig src/inspector/widgets/screen.zig` in the oracle shows only `SplitNode`, `DockWindow`, `Finish`, `GetNode`, `AddNodeEx`, `SetNodeSize` are ever invoked — exactly the 6 rustty declares at `crates-sys/dcimgui-sys/src/lib.rs:110-148`. The other 10 are unused in both trees.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s31-f4

### S31-F5 — WAIVED (not a defect)
- Refuting lens said: "Oracle grep shows only ImGuiDockNodeFlagsPrivate.DockSpace is referenced (widgets/surface.zig:165, widgets/screen.zig:140). Unused constant declarations produce no observable difference."
- Verified: `grep -n "DockNodeFlagsPrivate" src/inspector/widgets/surface.zig src/inspector/widgets/screen.zig` confirms only `.DockSpace` is used anywhere in the oracle tree; rustty `crates-sys/dcimgui-sys/src/lib.rs:164-172` defines exactly that one constant with the matching bit value `1 << 10`.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s31-f5

### S33-F2 — WAIVED (not a defect)
- Refuting lens said: "the finder itself concedes the compiled behaviour matches the oracle's `enable-coretext = hasCoretext()`. This is an internal doc-comment inaccuracy in rustty, not a divergence from the oracle."
- Verified: read `crates-sys/harfbuzz-sys/src/lib.rs:1-11` (stale "CoreText is off" claim) against `crates-sys/harfbuzz-sys/build.rs:87-91` (`build.define("HAVE_CORETEXT", Some("1"))` on macOS) and `wrapper.h:14-21` (correctly includes `hb-coretext.h` under `__APPLE__`, with an accurate comment right next to the stale one). The compiled artifact matches the oracle; only the top-of-file doc comment lies about it.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s33-f2

### S34-F7 — WAIVED (not a defect)
- Refuting lens said: "The oracle's loongarch.detect() is unreachable in this package's actual build ... LoongArch is therefore never a runtime-dispatch arch, so `ghostty_hwy_detect_targets` is not called there and DetectTargets takes the `bits |= HWY_ENABLED_BASELINE` arm identically in both trees (rustty's cpp/targets.cpp is byte-identical to the oracle's apart from the symbol rename — verified by diff)."
- Verified: independently diffed `pkg/highway/src/cpp/targets.cpp` against `crates-sys/highway-sys/cpp/targets.cpp` — identical apart from the `ghostty_hwy_detect_targets`→`rustty_hwy_detect_targets` rename. Read `hwy/detect_targets.h:760-767` (extracted from the oracle's zig-cache) confirming `HWY_HAVE_RUNTIME_DISPATCH_LINUX` is defined `1` only for `HWY_ARCH_ARM || HWY_ARCH_PPC || HWY_ARCH_S390X`, never LoongArch, and `HWY_HAVE_RUNTIME_DISPATCH` (targets.cpp:28-30) gates the call on `HWY_ARCH_X86 || ... || HWY_HAVE_RUNTIME_DISPATCH_LINUX` — LoongArch never reaches `ghostty_hwy_detect_targets`/`rustty_hwy_detect_targets` in either tree. Confirmed dead code on both sides.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s34-f7

### S35-F1 — UPHELD (real defect, narrowed scope)
- Refuting lens said: "the wrapper types have no in-tree counterpart and tests of a third-party crate's own API are not required parity artifacts ... The only genuinely in-tree re-implementation traced from these files is string_get_surrogate_pair_for_long_character ... which is a 6-line pure-Rust decomposition with no test ... That single low-value smoke assertion against a replaced binding layer does not clear the bar for a defensible finding."
- Why that is wrong: the lens's own text concedes a real, narrower gap survives inside the exact files already cited — `string_get_surrogate_pair_for_long_character` is hand-ported (not a third-party-crate API) in four places I verified by grep: `crates/rustty-font-shaper/src/deferred_face.rs:191`, `face/coretext.rs:94`, `shaper/coretext.rs:1273`, `discovery.rs:1025`, and none of the four copies carries a `#[test]`, while the oracle's `string.zig` "unichar" test (`!stringGetSurrogatePairForLongCharacter('A')`) exercises exactly this logic. Per the batch's binding precedent, a refutation that narrows-but-concedes a residual gap in the same cited region is UPHELD rather than waived, letting the fix task rescope from "16 test blocks" down to "one surrogate-pair helper test" (15 of the 16 oracle test blocks genuinely test a replaced third-party wrapper and are correctly not-applicable).
- Disposition: state=OPEN, owner= (unchanged), flows into its severity phase

### S36-F4 — WAIVED (not a defect)
- Refuting lens said: "os_log_create is documented never to return NULL ... in ReleaseFast the Zig `.?` is itself unreachable/UB rather than a checked error. The difference is therefore confined to unreachable code with no input producing a different observable result."
- Verified: read oracle `pkg/macos/os/log.zig:7-15` (`@ptrCast(c.os_log_create(...).?)`) against rustty `crates/rustty-os/src/macos.rs:296-302` (stores the raw pointer, SAFETY comment: "os_log_create never returns NULL (the reference unwraps with `.?`)"). rustty reproduces the reference's own assumption (an unchecked unwrap) rather than adding a defensive check the reference lacks; both sides trust the same documented Apple contract.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s36-f4

### S37-F1 — WAIVED (not a defect)
- Refuting lens said: "Program.zig:90-125 setUniform is dead code in the oracle: `grep -rn setUniform` over /Users/gael/projects/ghostty/src and /pkg returns only its own definition ... No input or state can reach the missing dispatch."
- Verified: `grep -rn "setUniform" src pkg` in the oracle checkout returns only the definition at `pkg/opengl/Program.zig:91` — zero callers anywhere. rustty `crates/rustty-renderer/src/opengl/gl/program.rs:12-28` correctly has no `set_uniform`; the live uniform path is the UBO one.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s37-f1

### S37-F2 — WAIVED (not a defect)
- Refuting lens said: "grep for PixelFormat across ghostty/src and ghostty/macos finds no use of macos.video.PixelFormat ... It is an unused constant table in a pkg/* Apple binding layer ... every use-site behaviour the oracle relies on (CVDisplayLink create/start/stop/...) is present in cvdisplaylink.rs."
- Verified: `grep -rn "PixelFormat" src` in the oracle shows only unrelated renderer enums (`OpenGL.zig`, `Metal.zig`, `image.zig`'s own `PixelFormat`, `metal/api.zig`'s `MTLPixelFormat`) — no use of `macos.video.PixelFormat`/`CVPixelFormatType`. Rustty's `crates/rustty-renderer/src/metal/cvdisplaylink.rs:1-87` covers every actually-used CVDisplayLink function; the 83-member enum is genuinely unreferenced.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s37-f2

### S38-F1 — WAIVED (not a defect)
- Refuting lens said: "the test behaviour is equivalent: png.zig:148-151 / jpeg.zig:148-151 assert only `expectError(error.Overflow)`, and png.rs:256-261 / jpeg.rs:250-254 assert `Err(Error::Overflow)` on inputs whose IHDR/SOF0 declare the same 32769x32768 dimensions ... this is a comment/provenance inaccuracy only."
- Verified: `ls -la pkg/wuffs/src/too_big.png too_big.jpg` in the oracle confirms both blobs exist (1.0MB / 12.5MB), contradicting rustty's PORT NOTE at `crates/rustty-wuffs/src/test_fixtures.rs:3-4` ("ABSENT from the ghostty checkout"). Read oracle `pkg/wuffs/src/png.zig:148-151` and rustty `crates/rustty-wuffs/src/png.rs:245-261` — both assert only `Overflow` on an input whose declared dimensions (32769×32768) hit the same size guard; the synthetic minimal fixture discriminates the guard exactly as well as the real blob. Doc-only inaccuracy, not a behavioral gap.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s38-f1

### S38-F6 — WAIVED (not a defect)
- Refuting lens said: "`b.systemIntegrationOption` is a zig-build packaging switch with no Cargo analogue, and the finding itself concedes it can 'never' change observable behaviour for the same library version ... Every use-site behaviour the oracle relied on (the spirv_cross_* C entry points) is present."
- Verified: `grep -rn "system_integration\|systemIntegration\|spirv-cross-c-shared" crates-sys/spirv-cross-sys xtask` returns nothing, confirming the absence; `crates-sys/spirv-cross-sys/build.rs:57-98` compiles the vendored sources providing the identical `spirv_cross_*` C API the oracle's dynamic-link arm would also expose. Packaging-only choice.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s38-f6

### S85-F5 — WAIVED (not a defect)
- Refuting lens said: "The control flow is identical (success value vs a failure return); only the error atom's identity differs, and no caller inspects it ... There is no input for which the two trees produce a different observable result."
- Verified: read oracle `src/font/DeferredFace.zig:150-153` (`return (try fc.pattern.get(.family, 0)).string;`) and its caller `src/font/CodepointResolver.zig:205` (`deferred_face.name(&buf) catch "<error>"`) against rustty `crates/rustty-font-shaper/src/deferred_face.rs:643-647` (collapses to `FaceError::OutOfMemory`) whose current callers likewise discard the error value. No caller on either side inspects the specific error atom.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s85-f5

### S85-F7 — WAIVED (not a defect)
- Refuting lens said: "the actual enum discriminants match the oracle exactly for all 17 keys. The only defect is the PORT NOTE text ... a rustty-internal comment with no oracle counterpart and no observable behaviour."
- Verified: read `crates/rustty-font/src/metrics.rs:887` (comment says index 11) against `:899-900` (`CursorThickness = 10, CursorHeight = 11`, i.e. discriminants ARE correct at index 10). The oracle's own field order (`Metrics.zig` 11th field, 0-based index 10) matches rustty's discriminant, not its comment. Comment-only defect with zero behavioral effect.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s85-f7

### S87-F1 — WAIVED (not a defect)
- Refuting lens said: "Those three CoreText symbols are annotated `nonnull` in Apple's headers (that's why objc2 generates non-optional returns), and the oracle's own comment ... reflects the same contract; the Zig wrapper's NULL->error.OutOfMemory arm is purely defensive."
- Verified: read objc2-core-text 0.3.2 `CTFont.rs:273-289` (`with_font_descriptor`) — the generated extern already declares `Option<NonNull<CTFont>>` and does `.expect("function was marked as returning non-null, but actually returned NULL")`, with the Apple doc comment directly above it stating "A best match font will always be returned". Grepped oracle `src/font/discovery.zig` and confirmed the `catch return self` arm at line 700 (`Score.score`) and the plain `try` at line 244 (`FontDescriptor.createWithAttributes`) are exactly the defensive arms described; both rely on the same underlying Apple contract that objc2's codegen also trusts. No reachable input produces NULL.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s87-f1

### S88-F1 — WAIVED (not a defect)
- Refuting lens said: "Every decision arm is preserved with identical pass/fail outcome ... The only delta is on-disk debugging artifacts ... plus log text. No input/state yields a different test result."
- Verified: read rustty `crates/rustty-font/src/glyf_rasterize/glyf_rasterize_png_test.rs:404-452` — missing reference panics (matches oracle's `return true` failure), `BytesDifferPixelsMatch` panics, and `PixelsDiffer` runs a per-pixel `assert_eq!` loop ending in a fail; all four oracle decision arms (byte-equal, missing-ref, bytes-differ-pixels-match, pixel-diff) map to an equivalent pass/fail outcome. Only the red/green diff PNG and the copied test PNG are not written.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s88-f1

### S88-F2 — WAIVED (not a defect)
- Refuting lens said: "with the var unset (the state of every CI/default run) the flow is byte-for-byte the oracle's ... The added arm cannot loosen any assertion on the default path and, being gated on decoded-pixel equality, cannot mask a visual change."
- Verified: read `crates/rustty-font/src/glyf_rasterize/glyf_rasterize_png_test.rs:409-431` — the regen arm requires BOTH `RUSTTY_REGEN_FONT_KAT_PNGS` set AND `outcome == AtlasDiff::BytesDifferPixelsMatch` (decoded pixels already match); `AtlasDiff::PixelsDiffer` always falls through to the failure arm at `:432-452` regardless of the env var. Cannot mask a real visual regression.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s88-f2

### S88-F3 — WAIVED (not a defect)
- Refuting lens said: "the oracle behaviour IS ported; the only defect is a stale sentence in a rustty-internal module doc that has no oracle counterpart text. Nothing in the ported code, tests, or observable behaviour differs from the oracle."
- Verified: read `crates/rustty-font-shaper/src/lib.rs:11-21` (claims `face/web_canvas.zig` "unported") against the filesystem: `crates/rustty-font-shaper/src/face/web_canvas.rs` and `crates/rustty-font-shaper/src/shaper/web_canvas.rs` both exist and are non-empty ports. Doc-only staleness.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s88-f3

### S90-F4 — WAIVED (not a defect)
- Refuting lens said: "The difference only materializes for a glyph id >= 2^16, and every producer of these ids is glyph_index() ... which returns a CGGlyph-derived u16 ... So the divergence is confined to an unreachable/invalid-input path."
- Verified: read `crates/rustty-font-shaper/src/face/coretext.rs:483-498` (`glyph_index` returns `Option<u32>` populated from a `[CGGlyph; 2]` array, i.e. always ≤ `u16::MAX`) and its two call sites feeding `render_glyph`/the ic_width block (`:1099` and `codepoint_resolver.rs:552-563`, whose own doc says the glyph index must come from `glyphIndex`/HarfBuzz-over-CTFont, both u16-bounded on this backend). The `as CGGlyph` truncation at `:517`/`:1106` cannot be reached with an out-of-range value from any real producer.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s90-f4

### S91-F6 — WAIVED (not a defect)
- Refuting lens said: "The reference behaviour for presentation > 1 is not defined behaviour to match ... a safety-check panic in Debug/ReleaseSafe and illegal behaviour in the ReleaseFast/ReleaseSmall builds the wasm artifact uses ... The only caller is the JS glue, which passes 0/1."
- Verified: read oracle `src/font/face/web_canvas.zig:494-511` (`Wasm.face_new_`, the sole caller of `initNamed`/this mapping) and confirmed (same grep as S91-F3) it is the only production caller, fed by JS glue with a 0/1 presentation value. rustty `crates/rustty-font-shaper/src/face/web_canvas.rs:961-980` matches the oracle for both defined inputs; the divergence only exists for an input the reference itself cannot safely accept (a safety panic / UB).
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s91-f6

### S92-F3 — WAIVED (not a defect)
- Refuting lens said: "the oracle behaviour being replaced is a Zig safety panic / undefined behaviour, not defined observable behaviour. Parity cannot require reproducing a panic-or-UB, no caller can distinguish it from an error result in any defined way, and the finder itself states 'no change required'."
- Verified (revised — see correction note below): read oracle `src/font/opentype/svg.zig:38-44`. `records: [*]const [12]u8 = @ptrCast(data[reader.seek..])` is a **many-item pointer with no length attached**, and the very next line, `const start_range = try glyphRange(&records[0])` (svg.zig:44), reads `records[0]` **unconditionally** — before the `len == 0` / `len - 1` site is ever reached, and regardless of whether `data[reader.seek..]` actually has 12 bytes behind it. With `numEntries == 0` there is no guarantee 12 bytes remain in the table, so `records[0]` is already an unchecked out-of-bounds read on the oracle side; in a ReleaseFast build the subsequent `len - 1` additionally wraps a `u16` `0` to `65535`, and `end_range`'s `glyphRange(&records[65535])` indexes roughly 786 KiB past the buffer. This is a genuine memory-safety bug in the oracle (an OOB read, not merely a checked trap), not observable behaviour that parity could sanely require rustty to reproduce. Rustty's `crates/rustty-font/src/opentype/svg.rs:75-77` (`if len == 0 { return Err(InitError::EndOfStream) }`) short-circuits before any such read and produces no wrong pixels on any input. Waived on this ground, not on "a panic is not behaviour to match" — that premise is retracted here because it collides with the doctrine this same ledger adopts at S209-F1 (safety-checked Debug/ReleaseSafe traps ARE first-class, defined, observable aborts); this row does not need that premise anyway, since the oracle's actual failure mode at `len == 0` is an unchecked OOB read, not a safety-checked trap.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s92-f3

  **Correction (post-review):** the original text above waived on "the oracle has no defined behavior [i.e., a panic] for rustty to match", which contradicts S209-F1's adopted doctrine that safety-checked traps are defined, observable behaviour Ghostty ships with. The ruling (WAIVED) is unchanged and independently re-derived by review; only the reasoning was rewritten to rest on the `records[0]` many-pointer OOB read at `svg.zig:44`, which is a real memory-safety bug rather than a defined trap, so no premise clash with S209-F1 remains. `S91-F6` is unaffected by this correction: its waiver rests on a different, unrelated premise (its only caller is JS glue passing a validated 0/1 presentation value) and survives independently.

### S92-F4 — WAIVED (not a defect)
- Refuting lens said: "`grep -rn \"OffsetSubtable|TableRecord\" /Users/gael/projects/ghostty/src` outside sfnt.zig returns nothing, and the FixedPoint use sites ... only convert values, never format them ... The difference is therefore confined to code no caller reaches, and is purely cosmetic string formatting."
- Verified: read oracle `src/font/opentype/sfnt.zig:105-115, 175-196, 209-227` (the three custom `format` methods) and confirmed via grep that `OffsetSubtable`/`TableRecord` are private nested types with no external caller, and `FixedPoint`'s use sites (`face/coretext.zig:97`, `face/freetype.zig:567-568,795`) only read the numeric value. rustty `crates/rustty-font/src/opentype/sfnt.rs:102,329,350` derives `Debug` only; no `Display` use site exists to diverge from.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s92-f4

### S98-F1 — WAIVED (not a defect)
- Refuting lens said: "box.rs: 2519 lines, single dispatch entry `pub fn draw2500_257F` ... matching oracle box.zig:48 ... no unimplemented!/todo! anywhere in the file ... So the comment is factually stale but purely doc-level."
- Verified: `wc -l crates/rustty-font/src/sprite/draw/box.rs` = 2519 lines; `grep -c "unimplemented!\|todo!"` = 0. `crates/rustty-font/src/sprite/draw/mod.rs:8-11` still claims `box` "currently carries only the diagonal helpers ... plus arc" and lists the symbols modules as deferred, but `mod.rs:21-23` already declares `symbols_for_legacy_computing`/`symbols_for_legacy_computing_supplement` as present modules. Doc-only staleness.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s98-f1

### S100-F2 — WAIVED (not a defect)
- Refuting lens said: "the sole consumer in the oracle GTK tree is command_palette.zig:54-64 ... grep for GhosttyBindingAction across src/apprt/gtk returns only the defineBoxed site — nothing ... looks the type up by name ... rustty covers this at crates/rustty-apprt/src/gtk/class/command_palette.rs ... This is the expected R4 relocation ... The only residue is the GType's registered name string, which nothing reads."
- Verified: `grep -rn "GhosttyBindingAction" /Users/gael/projects/ghostty/src/apprt/gtk` returns nothing (only the `defineBoxed` declaration site itself, per the oracle's own `Binding.zig:998-1005`), confirming the name string is never looked up by any `.blp` template or GObject property spec. rustty `crates/rustty-apprt/src/gtk/class/command_palette.rs:71-72,137,161,351` carries the equivalent boxed-type crossing (`ActionBoxed`/`GhosttyCommandPaletteAction`) at the real (and only) use site, satisfying the R4 relocation (rustty-input cannot depend on glib). Unlike `S87-F4` (a GObject *property* type whose registered name IS introspectable via the property spec), this GType's name has no consumer anywhere.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s100-f2

## Batch 2d — src/cli, src/build, include, src/os, src/lib, src/inspector, src/input, src/datastruct, src/config, src, macos, src/renderer (28 rows)

### S4-F1 — WAIVED (not a defect)
- Refuting lens said: "Not a functional gap — a naming transform explicitly mandated by the port's own parity contract, and the upstream alias layer no longer exists ... DEFINITION_OF_PARITY.md:402 defines R2-ABI-shape as 'renamed ghostty_*->rustty_* ... zero residual ghostty_* symbols' ... DEFERRALS.md:7870 (vet row W7) records that upstream DELETED the ghostty_* C-ABI forwarder layer at e4cc852 ..."
- Verified: read `.port/phase-0/plan/DEFINITION_OF_PARITY.md:398-422` directly — the table explicitly DROPS the "ghostty.h ABI byte-match" constitution gate (because "header is `rustty.h`, symbols are `rustty_*`") and replaces it with the binding **R2-ABI-shape** gate: "rustty exports the same set of functions with the same signatures, renamed `ghostty_*`→`rustty_*` ... **zero** residual `ghostty_*` symbols." Also read `.port/phase-2/DEFERRALS.md:7911-7912` (W7): upstream deleted the `ghostty_*` C-ABI forwarder layer at `e4cc852`; the ROADMAP's "aliases stay" constraint is recorded FALSE. Adding a `ghostty_*` alias layer, as the finding's suggested fix asks, would violate the project's own binding R2 gate, not satisfy it.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s4-f1

### S4-F3 — UPHELD (real defect)
- Refuting lens said: "The only divergence is the OOM arm, which is unobservable by construction in Rust's allocation model (the process aborts before any return value can be produced) ... same root cause as S4-F2 ... Consistent with the existing grid_ref_track precedent elsewhere in the same crate."
- Why that is wrong: read `include/rustty/vt/search.h` / `include/ghostty/vt/search.h:340-362` — the header documents `GHOSTTY_OUT_OF_MEMORY if allocation fails` as part of `ghostty_search_new`'s public C-ABI contract, a promise an embedder is entitled to rely on. Read `crates/rustty-vt-c/src/search.rs:387-405`: the reference's `catch { alloc.destroy(wrapper); return .out_of_memory; }` arm is carried only as a comment; the live code is `let inserted = unsafe { (*terminal_wrapper).searches.insert(wrapper) }; debug_assert!(inserted);` over a `std::collections::HashSet`, which aborts the whole process on allocation failure instead of returning the documented error code. This is not "unobservable" — it is the difference between a documented graceful error return (an embedder can catch and handle it) and an undocumented process abort (an embedder cannot). This is the same shape of divergence the batch's own S209-F1 doctrine addresses (a defined/graceful oracle path vs. an unrecoverable Rust-side outcome), and unlike S209-F1's malformed-input trigger, this one needs no adversarial input at all — ordinary memory pressure is enough. The "grid_ref_track precedent" cited by the refutation is a different situation (no documented external ABI promise on that path); here there is an explicit header-documented contract that the port breaks.
- Disposition: state=OPEN, flows into its severity phase (low)

### S4-F4 — WAIVED (not a defect)
- Refuting lens said: "the divergence is confined to a cell holding 0xD800-0xDFFF, which no write path can produce: cell codepoints arrive only through the stream's UTF-8 decoder (surrogates rejected there) and there is no C API that writes an arbitrary codepoint into a cell, so the state is unreachable ... the finder itself grades it 'effectively unreachable'."
- Verified: read `crates/rustty-vt-c/src/lib.rs` for any exported `rustty_*cell*` write API — only `rustty_cell_get`/`rustty_cell_get_multi` (read-only) exist; no C entry point writes a raw codepoint into a `Cell`. Cell codepoints are populated exclusively via VT-stream UTF-8 decoding, which cannot produce surrogate code points (invalid UTF-8 per RFC 3629, rejected at the decode boundary on both oracle and rustty). Both the finder and the refuting lens independently reach "effectively unreachable"; my own check of the C surface confirms no write path exists to construct the divergent state.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s4-f4

### S30-F2 — WAIVED (not a defect)
- Refuting lens said: "pkg/android-ndk/build.zig is Zig build plumbing ... explicitly in the 'replaced' class: cargo/cc-rs consume the standard CC_/AR_/CARGO_TARGET_*_LINKER env instead of an in-build NDK finder ... What remains is a CI-coverage gap ... not an observable functional difference in the port."
- Verified: read `.port/tools/gates/check_cross_targets.sh:51-98` — the `aarch64-linux-android` leg is present and reports `skipped` when the CXX target can't compile, rather than being silently absent. Cross-referenced this project's own locked-fork doctrine (memory: rustty-completion-plan records "macOS-first" as one of two locked forks) — Android/iOS build-artifact completeness was never a committed goal for this phase. The gap is CI/toolchain coverage, not a functional divergence in any code path that runs today.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s30-f2

### S30-F3 — WAIVED (not a defect)
- Refuting lens said: "The finder's own load-bearing claim ('no DEFERRALS.md row mentions apple-sdk/native_link/iOS') is false: `.port/phase-3/NON_PORTS.md:515-536` is a dedicated row 'G7 ... N/A, WITH A PRECONDITION' ... recording the iOS tier as an unscheduled precondition ... Documented scope of the locked macOS-first fork, not an accidental omission."
- Verified: read `.port/phase-3/NON_PORTS.md:515-536` directly — G7 explicitly records that rustty ships no iOS dylib, that the Zig-linker defect motivating `native_link.zig` doesn't apply to rustc's `ld64` path, and carries a **precondition to carry forward** ("if rustty ever ships an iOS framework ... those four assertions become mandatory ... record it as a precondition on the (unscheduled) iOS tier, not as a dismissal"). This is a deliberately tracked, scoped deferral under the locked macOS-first fork, not an accidental gap.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s30-f3

### S44-F4 — WAIVED (not a defect)
- Refuting lens said: "the ordering difference is real in the source ... but it is confined to a path no caller reaches: grep for `Level::Debug`/`LogLevel::Debug` across crates finds only the enum definitions ... whose `LogLevel::Debug` is never constructed at a call site (the only sites use Warn/Info/Err) ..."
- Verified: `grep -rn "Level::Debug\|LogLevel::Debug" crates/` — every hit is either an enum-variant definition, a mapping arm, or `crates/rustty/src/main_rustty.rs:496-505`'s `const LOG_LEVEL`, which is itself marked `#[allow(dead_code)]` and consumed nowhere. No call site in the workspace constructs a `Level::Debug`/`LogLevel::Debug` value to pass into `log_with`, so the mis-ordered gate in `crates/rustty-core/src/logging.rs:55-85` cannot currently produce a different observable result than the reference.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s44-f4

### S44-F6 — WAIVED (not a defect)
- Refuting lens said: "(a) Rust exposes no ReleaseSmall cfg ... (b) the reference's own sink (os/wasm/log.zig:25-53) does no level check either ... the only callers of rustty_os::wasm::log::freestanding::log in the main_wasm-linked tree are ... 'error' and 'warning'; both levels pass every reference filter ... Latent/inconclusive."
- Verified: `grep -rn "freestanding::log(" crates/` finds exactly two call sites — `crates/rustty-config/src/wasm.rs:52` (`"error"`) and `crates/rustty-font-shaper/src/face/web_canvas.rs:863` (`"warning"`) — both of which pass every level the reference's `std_options.log_level` (debug/info/warn) would allow through in any build mode, so the missing enforcement of `LOG_LEVEL` cannot currently suppress or leak a message differently than the oracle.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s44-f6

### S65-F2 — WAIVED (not a defect)
- Refuting lens said: "SharedDeps.zig:995-999 adds ... two flags ... only for `abi == .msvc`, i.e. when Zig's bundled clang cross-compiles for the MSVC ABI ... rustty-simd/build.rs:160 compiles the same four .cpp files through cc-rs, which on an x86_64-pc-windows-msvc host target invokes MSVC cl.exe — no clang autolink pragmas are emitted ... and `-fno-autolink` is not even a cl.exe flag."
- Verified: read `crates/rustty-simd/build.rs:90-165` — it builds via `cc::Build` (cc-rs), which dispatches to the platform's native toolchain (`cl.exe` on a real `-pc-windows-msvc` host, not Zig's bundled clang cross-compiler). The two oracle flags are specifically clang-autolink/UCRT-alternatename workarounds needed only when Zig's clang cross-compiles for the MSVC ABI; they don't apply to, and `-fno-autolink` isn't even valid for, cl.exe. This is a toolchain-artifact difference in legitimately replaced build plumbing, not a functional gap.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s65-f2

### S65-F5 — WAIVED (not a defect)
- Refuting lens said: "On macOS rustty ports the step (xtask/src/main.rs:592-615 + archives.rs) ... Off macOS, crates/rustty-simd/build.rs:160 uses `cc::Build::compile`, which emits `cargo:rustc-link-lib=static=... +bundle`, so the objects are already inside the rlib ... The finding's cited evidence (CMakeLists.txt:231-247 libintl/gettext) is about an external SYSTEM library, which the oracle's combine step would not fold either."
- Verified: read `CMakeLists.txt:231-247` directly — the gettext/libintl reference is indeed an external system library lookup (`find_library(... HINTS /opt/homebrew/opt/gettext/lib ...)`), not something an MRI archive-combine step would fold in either tree. Read `xtask/src/archives.rs:55-78` — `resolve_under_with` deliberately REJECTS any archive resolved outside the workspace `target/` directory specifically to prevent folding a system archive like Homebrew's libintl. cc-rs's default static-lib bundling already makes rustty's SIMD kernels self-contained inside the rlib without a separate MRI step. No substantiated gap.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s65-f5

### S65-F9 — WAIVED (not a defect)
- Refuting lens said: "grepped ghostty/src, ghostty/macos and ghostty/include for `ImGui_ImplOSX`/`imgui_impl_osx`/`ImplOSX`: zero hits — no oracle use-site exists ... The delta is confined to unreferenced symbols in a static archive with no observable behaviour."
- Verified: `grep -rni "ImplOSX\|imgui_impl_osx" /Users/gael/projects/ghostty/src /Users/gael/projects/ghostty/macos /Users/gael/projects/ghostty/include` returns nothing — the oracle itself never calls into the OSX imgui backend it compiles. `crates-sys/dcimgui-sys/build.rs:165-216` does compile the Metal backend, the one actually used. A dead, unreferenced backend TU is not a functional gap.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s65-f9

### S66-F2 — WAIVED (not a defect)
- Refuting lens said: "The deletion of the two prose lines ... leaves the line list '...for.', '', '', '' — which joins to exactly 'for.\n\n\n', i.e. rustty's CONFIG_HEADER tail. So the 'extra blank line' is not an independent error but an artifact of which blank separator you consider deleted alongside the paragraph ... No functional difference in the generated mdx (a blank line in markdown before a '## ' heading is inert)."
- Verified: read `ghostty/src/build/webgen/main_config.zig:14-31` and independently re-derived the multiline-string join myself (Zig multiline string literals join lines with `\n` and add no trailing `\n` after the last line): the oracle's 16-line literal, with the two "In the future..." lines deleted but both surrounding blank separators kept, joins to exactly `"for.\n\n\n"` — byte-identical to rustty's `CONFIG_HEADER` at `crates/rustty-extra/src/webgen.rs:44`. The "correct" 2-newline reading requires ALSO deleting one of the two blank separators, which is an equally defensible but not uniquely-correct choice. Markdown collapses blank lines before a heading regardless, so no rendered output differs either way.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s66-f2

### S67-F8 — WAIVED (not a defect)
- Refuting lens said: "The inline closure at tests.rs:605-609 ... is byte-for-byte the semantics of compatibility_renamed's body ... and the deviation is documented at tests.rs:589-594. The shipped helper is exercised in production by config_struct.rs:9554/9560/9567."
- Verified: read `crates/rustty-args/src/args/tests.rs:560-624` and `crates/rustty-args/src/args.rs:742-766` — the inline test closure is indeed line-for-line the same logic as `compatibility_renamed` (documented as a necessary workaround since Rust can't bind a comptime `to` into a bare fn pointer). More importantly, `compatibility_renamed` IS exercised by a real, non-inline test: `crates/rustty-config/src/config_struct.rs:12601-12619` (`compatibility_scrollback_limit_renamed_to_bytes`) drives `cfg.load_iter()` on `--scrollback-limit=1234`, which dispatches through `config_struct.rs:9567`'s `Some(rustty_args::args::compatibility_renamed(...))` — the actual shipped function, not a stand-in. The claimed "no unit test in this crate" is true only for `rustty-args` in isolation; workspace-wide, the function is genuinely covered end-to-end.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s67-f8

### S69-F7 — UPHELD (real defect)
- Refuting lens said: "there is no well-defined observable behaviour to diverge from, so there is nothing to be non-parity with ... The finding's own text says 'No change required' and 'the reference behaviour is not well-defined.'"
- Why that is wrong: this batch's own S209-F1 ruling (above) establishes the binding doctrine that "ghostty's safety-checked builds (Debug and ReleaseSafe) are first-class supported configurations, so a Zig safety trap is a defined, observable behaviour — NOT an excuse to waive." Read `ghostty/src/cli/list_themes.zig:484-490` directly: pressing `'c'` with an empty `filtered` list indexes `self.themes[self.filtered.items[self.current]]` where `filtered.items` is empty — a genuine, ordinary-use-reachable (type a filter matching nothing, press `c`) Debug-mode bounds panic that crashes the interactive theme browser. That crash is exactly the kind of "defined, observable behaviour" S209-F1 says must not be waived away by pointing at ReleaseFast UB. `crates/rustty-cli/src/list_themes.rs:960-976` instead adds an empty-filter guard and continues silently, diverging from the oracle's Debug-mode crash. Ruling WAIVED here (as the refuting lens's "not well-defined" premise argues) would directly contradict the S209-F1 doctrine already recorded in this same file — the two cannot both stand, and S209-F1 is the earlier-established, more thoroughly-argued precedent.
- Disposition: state=OPEN, flows into its severity phase (low)

### S71-F5 — WAIVED (not a defect)
- Refuting lens said: "The only assertion the port drops is `expectEqual(error.OutOfMemory, err)` on the induced-failure arm, plus testing.allocator's leak check. Rust's global allocator is infallible (abort on OOM), so that arm is unreachable in the port — the difference is confined to code that cannot exist."
- Verified: read `crates/rustty-cli/src/ssh_cache/disk_cache.rs:1174-1261` — this is a purely internal SSH-cache implementation detail with no documented C-ABI/public contract promising graceful OOM handling (unlike S4-F3 above, where `ghostty_search_new`'s header explicitly documents `GHOSTTY_OUT_OF_MEMORY`). The port's `Vec`/global-allocator abort-on-OOM here matches the workspace-wide accepted convention already ruled on elsewhere in this file (e.g. the earlier `page.rs`/`PayloadDecodeError::OutOfMemory` entry above: "confirm the allocations are infallible Rust Vecs (abort on OOM), matching the crate-wide convention ... only this specific allocator-exhaustion arm is unreachable"). No external contract is broken; this is a test-harness-only coverage nuance, not a production divergence.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s71-f5

### S71-F7 — WAIVED (not a defect)
- Refuting lens said: "The warn path IS present and faithful ... routed through the workspace's documented log_warn helper ... The only delta is the '[warn] ' prefix and Rust's ParseIntError Display vs Zig's error name — pure log text ... the finding's second half is not substantiable in this area: neither CApi.zig nor c_api.rs exists ... fabricated for shard 71."
- Verified: read `crates/rustty-cli/src/ssh_cache/entry.rs:57-70,163-168` — control flow, message content, and the `None` return are identical to `Entry.zig:22-25`; only the stderr prefix/error-Display formatting differs (cosmetic log text), the same class of finding already waived elsewhere in this file for log-text-only deltas (e.g. `S88-F1` above: "The only delta is on-disk debugging artifacts ... plus log text. No input/state yields a different test result"). `ls` of both `ghostty/src/cli/ssh-cache/` and `crates/rustty-cli/src/ssh_cache/` confirms neither has a `CApi.zig`/`c_api.rs`, so half the original finding's evidence is unsupported.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s71-f7

### S74-F7 — UPHELD (real defect)
- Refuting lens said: "the residual wasm path is a provable no-op: `isFlatpak()` is a compile-time `false` off Linux ... `probable_cli_environment()` on wasm returns false ... `self.command` therefore stays `None` ... No input/state on the wasm target yields a different value ... the difference is confined to code that cannot take effect."
- Why that is wrong: the refutation's "no-op" argument assumes the captured environ is always empty on wasm, but `crates/rustty-config/src/config_struct.rs:5537-5556`'s own PORT NOTE documents that a C/wasm embedder can install an arbitrary captured-environ block via `rustty_global::environ::set_environ` (verified: `crates/rustty-global/src/environ.rs:308` `pub fn set_environ` has no `cfg(not(wasm))`/similar gate — it is reachable on every target, exactly as the PORT NOTE says "libghostty embedders rely on the heuristic seeing THAT block"). An embedder that installs a block containing a non-empty `TERM_PROGRAM` (making `probable_cli_environment()` return true) and a `SHELL` value would make rustty's wasm build set `self.command = Shell(SHELL)`, whereas the oracle's `if (!isWasm and !is_test)` guard (`Config.zig:4671-4674`) unconditionally skips the whole block on wasm regardless of any embedder-supplied environment. This is a real, documented-mechanism-reachable divergence, not merely theoretical unreachable code.
- Disposition: state=OPEN, flows into its severity phase (low)

### S78-F8 — WAIVED (not a defect)
- Refuting lens said: "std.fs.path.dirnamePosix returns \"/\" for input \"/\" (it strips trailing slashes and returns \"/\" when end_index reaches 0), which is exactly what edit.rs:246-255 returns ... The only true divergence is trailing-separator input ... and the finder itself concedes it is unreachable ..."
- Verified, with a correction: I read the ACTUAL pinned Zig stdlib (`/opt/homebrew/Cellar/zig/0.16.0_1/lib/zig/std/fs/path.zig:1330-1353`, matching `ghostty/build.zig.zon`'s `minimum_zig_version = "0.16.0"`) and the refuting lens's specific factual claim is WRONG: `dirnamePosix("/")` returns `null` (the stdlib's own inline test asserts `testDirnamePosix("/", null)`), not `"/"` as claimed. So `edit.rs`'s `idx == 0 => Some(&path[..1])` does diverge from the oracle for a literal root path, contra the refutation's reasoning. However, the disposition is unaffected: I traced `config_path_candidates()` (`crates/rustty-config/src/edit.rs:218-240`) — every candidate (`default_xdg_path`/`legacy_default_xdg_path`/`default_app_support_path` on macOS) joins a base directory with `"<CONFIG_DIR>/config"` or similar, always producing at least two path separators with no bare single-slash-prefix form and no trailing separator. `idx == 0` (the diverging case) requires a path of the exact shape `"/x"` (one separator, at position 0), which no real candidate can produce; the presence lens's own counter-note independently reaches "unreachable from the actual configPath candidates" for the sibling trailing-slash case. Both known divergent shapes are unreachable via any live call site, even though the refuting lens's specific "/" reasoning was itself incorrect.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s78-f8

### S83-F1 — WAIVED (not a defect)
- Refuting lens said: "This is a Vim comment line in a generated file; no parser, ftdetect rule, or config behaviour depends on it ... It is a deliberate product-identity URL for the actual port repo, not a mechanical-rename miss with observable effect."
- Verified: read `ghostty/src/extra/vim.zig:1-25` and `crates/rustty-extra/src/vim.rs:1-40` — the divergent line (`" Maintainer: ...`) is a bare Vim comment inside a static generated byte string; the functional line immediately below it (`au BufRead,BufNewFile */rustty/config,...setf rustty`) IS correctly per-token-renamed. No code path reads or parses the Maintainer comment.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s83-f1

### S83-F3 — UPHELD (real defect)
- Refuting lens said: "The reference's own default test lane (`zig build test`, Debug) enables tripwire via `builtin.is_test`, and the port's default `cargo test` (debug_assertions on) runs them identically; only a non-default `--release` test invocation skips them, and a `tripwire-test` feature ... exists to re-enable. Harness gating, not an observable behaviour or assertion difference."
- Why that is wrong: read `crates/rustty-font/src/atlas.rs:1644-1720` — the three tests carry `#[cfg_attr(not(debug_assertions), ignore)]`, so a `cargo test --release` run skips them entirely (not merely disables the fail points; the test function itself never executes). The oracle's `Atlas.zig:826-889` tests run in every `zig build test` invocation at any optimize level via `builtin.is_test`, with no such extra restriction, so `zig build test -Doptimize=ReleaseFast` still exercises all three fail points while `cargo test --release -p rustty-font` skips them outright — a real coverage gap. **Correction (post-review): an earlier draft of this entry additionally argued that `cfg!(test)` inside `Module::enabled()` (`crates/rustty-util/src/tripwire.rs:198-200`) would make the tripwire mechanism itself fire correctly in a `--release` test binary, making the `ignore` attribute "stricter than the mechanism it gates." That argument is wrong and is struck: `cfg(test)` is per-compilation-unit, and `enabled()` compiles inside `rustty-util`, a plain (non-dev) dependency of `rustty-font` — under `cargo test --release -p rustty-font`, `rustty-util`'s own `test` cfg is false, `debug_assertions` is false, and `tripwire-test` is off, so `enabled()` returns `false` and `check()` (`tripwire.rs:214-216`) would return `Ok`, making the three tests genuinely FAIL if the `ignore` attribute were removed without also wiring `tripwire-test`. `tripwire.rs:185-197`'s own PORT NOTE says exactly this ("a dependency compiled for a consumer's test binary does NOT see it"). The `#[cfg_attr(not(debug_assertions), ignore)]` gate is therefore exactly as strict as the mechanism it gates, not stricter.** The finding survives on its other leg regardless: the refutation concedes the coverage gap exists ("only a non-default `--release` test invocation skips them") and argues only that it's non-default and fixable via the `tripwire-test` feature — but `crates/rustty-font/Cargo.toml` has no `tripwire-test` dev-feature wiring (confirmed by grep), so that escape hatch isn't actually wired up, and "the reference runs these in every optimize level, the port genuinely does not" stands as a real, low-cost-to-fix, conceded gap.
- Disposition: state=OPEN, flows into its severity phase (low)

### S105-F2 — WAIVED (not a defect)
- Refuting lens said: "`_padding` is an explicitly reserved-zero field ... no consumer ever reads it. The only divergent input is a host passing undefined high bits through `ghostty_input_mods_e` ... where the oracle would merely echo the garbage back and rustty zeroes it; the C header defines only bits 0..9, so this is out-of-contract input, not an observable behavioural difference."
- Verified: read `ghostty/include/ghostty.h:180` (`ghostty_input_mods_e` — an enum defining only bits 0..9, no documented padding semantics) and confirmed no `rustty/include` header documents any contract for bits 10..15 either. Unlike S4-F3/S74-F7 above, there is no documented ABI promise here that reserved bits round-trip; this is a case of genuinely undefined/reserved input where the oracle happens to echo garbage and rustty happens to zero it — neither behavior is "more correct" per any documented contract, and the divergence requires an out-of-contract caller to begin with (not a defined-trap-vs-silent-wraparound case like S209-F1; both sides here silently handle undefined bits, just differently).
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s105-f2

### S106-F1 — WAIVED (not a defect)
- Refuting lens said: "The seam note is indeed stale ... but no observable behaviour differs: the sole caller `crates/rustty-core/src/surface.rs:8140-8144` passes exactly `self.io.terminal.flags.mouse_event/mouse_format` and `self.size` ... Same values, same result, same defaults ... API-shape/tech-debt nit only."
- Verified: read `crates/rustty-vt/src/input/mouse_encode.rs:90-112` and `crates/rustty-core/src/surface.rs:8140-8144` — the caller reads the identical two `Terminal` flags and threads them in by value; the resulting `Options` struct is byte-identical to what `from_terminal(&Terminal, size)` would produce. This is a pure API-shape/technical-debt nit (a stale PORT NOTE about a seam that's since been closed elsewhere), not a functional divergence.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s106-f1

### S109-F1 — WAIVED (not a defect)
- Refuting lens said: "The function is unreachable dead code in the oracle, so no input or state can produce a different observable result ... a difference confined to unreachable code is refuted."
- Verified: read `ghostty/src/inspector/widgets/termio.zig:110-121` — the sole call site is `// eventTable(events);`, commented out in the oracle itself. Read `crates/rustty-inspector/src/widgets/termio.rs:295-310` — the port faithfully reproduces the same commented-out call. `eventTable` is genuinely dead code on the oracle side; the port's failure to also define the never-called function has no observable consequence.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s109-f1

### S109-F3 — WAIVED (not a defect)
- Refuting lens said: "The functionality IS present and is the faithful analogue of an inherently non-portable Zig construct ... `@typeName(Value)` emits a Zig-compiler-internal type-name string that has no Rust equivalent and is not part of any protocol or assertion ... with an explicit PORT NOTE ... documenting the substitution."
- Verified: read `crates/rustty-inspector/src/widgets/termio.rs:1550-1564` — the substitution (`core::any::type_name::<T>()` for Zig's `@typeName(Value)`) is explicitly documented in a PORT NOTE at the same slot/kind of value; both are compiler-generated type-name strings with no defined protocol contract, and Zig's `@typeName` spelling is fundamentally not reproducible from Rust (different compilers, different name-mangling schemes). The only consequence (a user's `passFilter` string might match in one tree and not the other) is an unavoidable side effect of a documented, non-fixable substitution — comparable to the already-accepted `S83-F5` "zig-js glue is legitimately replaced, no Rust analog" pattern in this same audit corpus.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s109-f3

### S110-F5 — UPHELD (real defect)
- Refuting lens said: "These are comptime-only guards over hand-written descriptors: they only fire for a malformed descriptor, and Rust const context has no type reflection to express `Layout.Zig != @FieldType(...)`. Every descriptor in the tree ... is well-formed, so no runtime input or state produces a different value, error, or ordering ... refuted."
- Why that is wrong: read `ghostty/src/lib/packed.zig:62-73,164-178` — the two dropped `@compileError` checks (`Layout.Owner != T`/`Layout.Union != field.type`/`Layout.union_field != @field(...)` and the arm-vs-source-type-identity check) are a compile-time safety net specifically for the packed-tagged-union descriptors that back C-ABI structs (consumed by `crates/rustty-vt-c/src/types.rs:2953-2962`). The refutation's "every descriptor today is well-formed" argument only shows the check is currently *vacuously* satisfied, not that dropping it is safe — it removes a static guard against a future malformed descriptor silently corrupting an ABI-facing packed layout, which given this project's documented emphasis on ABI byte-match rigor (embed-abi-byvalue-sret-gate memory) is exactly the class of gap worth keeping open. Unlike the genuinely non-portable `@typeName`/zig-js cases waived elsewhere in this batch, the suggested fix (carry a `source_bit_size`/owner-id per arm and check equality at `validate()`) is concretely implementable in Rust — this is a real, fixable safety-net gap, not an inherent language limitation.
- Disposition: state=OPEN, flows into its severity phase (low)

### S111-F10 — UPHELD (real defect)
- Refuting lens said: "The reference test cannot be expressed at all in Rust (there is no comptime call path to test), so this is not-applicable rather than a weakened assertion ... Inconclusive-at-best, so refuted per the default."
- Why that is wrong: read `crates/rustty-os/src/i18n.rs:110-147` and `:410-428` — the comptime-arm collapse itself is correctly documented and genuinely not portable (agreed). But the finding's real substance survives independently of that comptime-specific framing: the msgid-round-trip *property* the oracle test asserts (`gettext_(msgid) == msgid` when unbound) is exercised by exactly one rustty test, `gettext_resolves_from_vendored_libintl` (`i18n.rs:420-428`), which begins with `if !cfg!(target_os = "macos") { return; }` — so on Linux/other targets, nothing in the crate asserts this property at all. The refutation only addresses why the *literal* comptime test can't be ported; it doesn't address the cross-platform coverage gap for the underlying property, which the suggested fix (a trivial, cheap, cross-platform `underscore_returns_msgid` test) would close.
- Disposition: state=OPEN, flows into its severity phase (low)

### S112-F10 — WAIVED (not a defect)
- Refuting lens said: "No observable difference. The omitted items ... are pure re-exports of std.os.windows type aliases and one extern with no call site ... Declaration surface with no caller cannot change behaviour ... Whole module is #![cfg(windows)] on a macOS-first fork."
- Verified: read `crates/rustty-os/src/windows.rs:15-22,214-220,349-353` — the omissions are exactly the declaration-only aliases/externs the refutation describes; every genuine use site the finding itself lists (`WaitForSingleObject`/`GetExitCodeProcess` in `rustty-termio/src/command.rs:1697-1698`, `OVERLAPPED` in `rustty-xev/src/windows.rs:297`, the `NtCreateFile` path replaced by `CreateFileW("NUL")`) is already covered by a relocated/substituted equivalent. `ProcThreadAttributeNumber`'s only real-world constructed value (`ProcThreadAttributePseudoConsole = 22`) is representable by the closed Rust enum. Consistent with the locked macOS-first fork (`windows.rs` is `#[cfg(windows)]`, not exercised in the project's current CI matrix).
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s112-f10

### S112-F12 — WAIVED (not a defect)
- Refuting lens said: "In release the Rust add wraps, the mask leaves a value below addr, and align_pointer returns None — exactly the Zig result. Only a debug build differs (overflow panic), and only when mmap ... returns a base address within `alignment` bytes of usize::MAX, which the kernel never does. No reachable input produces a different observable result."
- Verified: read `crates/rustty-os/src/mach.rs:255-269` and the oracle's `std.mem.alignPointer` (`@addWithOverflow`-based, graceful null on overflow in every build mode). Unlike `S69-F7`/`S4-F3` above (where the trigger is ordinary/adversarial-but-realistic input — a malformed tmux control string, or real memory pressure), reaching this Debug-panic path requires the OS's `mmap` to return a virtual address within single-digit-to-low-double-digit bytes of `usize::MAX` (2^64-1) — something no real kernel's virtual-memory allocator does on any current hardware (virtual address spaces are always far below the full 64-bit range, ASLR included). This is a physical/OS-level impossibility, not merely rare-but-possible adversarial input, so it does not fall under the S209-F1 doctrine the way S69-F7 does.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s112-f12

### S119-F3 — WAIVED (not a defect)
- Refuting lens said: "The extra gate cannot fire in any oracle-supported configuration. `owns_context` has no counterpart in the oracle at all ... on the windowed GDK path it is false, so linux_impl.rs:812-817 calls present_to_default exactly like Frame.zig:63-67 ... The remaining delta is the swallowed `log.err`, which is the same crate-wide missing-facade class already reported as S119-F1 ... duplicative rather than a separate branch difference."
- Verified: `grep -rn "ownsContext\|owns_context" ghostty/src/renderer/OpenGL.zig ghostty/src/renderer/opengl` returns nothing — `owns_context` is a rustty-only addition for its offscreen/golden-image test harness, which the oracle doesn't have; on the real windowed GDK path the gate evaluates false and `crates/rustty-renderer/src/generic/linux_impl.rs:812-822` calls present exactly like the oracle. The only surviving delta (the swallowed `log.err`) is the same workspace-wide "no logging facade" gap already CONFIRMED (not disputed) as `S119-F1` elsewhere in this same audit file — this row is duplicative of an already-tracked finding, not an independent branch-behavior difference.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s119-f3

## Batch 14c-1 — src/renderer `LOG` partial waivers (task-14c-1, 2026-09-09)

Two rows whose diagnostics were restored ALMOST completely. Neither is "not a
defect" — the bulk of each row was a real gap and is now fixed — but each retains
one cited oracle anchor whose emission has no reachable `catch` arm in rustty, so
per `PROCEDURE.md` ("a site inside the cited anchors still forces `WAIVED`, no
matter how well documented") they are `WAIVED`, not `CLOSED`.

### S115-F4 — PARTIALLY RESTORED, WAIVED (one cited anchor unreachable)
- **BOTH TWINS NOW LIVE (task-14c-5, N9).** `generic.zig` has two Rust twins —
  `generic.rs`'s `#[cfg(target_os = "macos")] mod macos_impl` (from
  `generic.rs:1095`) and `generic/linux_impl.rs`, mutually exclusive per build.
  task-14c-1 restored the macOS twin; task-14c-3 fix round 1 NARROWED this entry
  because the Linux twin was still logging-dead; **task-14c-5 has now restored the
  Linux twin too**, so the counts below hold on both platforms with ONE structural
  difference: six of this row's fifteen anchors — `generic.zig:991`
  (`displayLinkCallback`) and `:1169/:1177/:1183/:1188/:1190` (`syncDisplayLink`)
  — are guarded by `DisplayLink != void`, which is `void` off macOS, so they have
  no Linux counterpart to restore and are not "missing" there. The other EIGHT
  restorable anchors (`:852, 1320, 1436, 1472, 1506, 1517, 1533, 1595`) are live in
  `generic/linux_impl.rs`, and the residual below (`:1558`) is a residual on BOTH
  arms for the same reason. `generic/linux_impl.rs` now carries
  `const LOG_SCOPE: &str = "generic_renderer";` at file scope.
- 14 of the row's 15 cited oracle anchors are now live `log::` emissions with the
  `generic_renderer` target, **on both twins** (macOS via `generic.rs`, Linux via
  `generic/linux_impl.rs` since task-14c-5; the six `DisplayLink`-gated anchors have no
  Linux counterpart to carry at all): `generic.zig:852, 991, 1169, 1177, 1183, 1188, 1190,
  1320, 1436, 1472, 1506, 1517, 1533, 1595`. (`:991`'s emission lives in
  `crates/rustty-renderer/src/metal/cvdisplaylink.rs` because the port inlines
  `displayLinkCallback`'s body into the CoreVideo `extern "C"` trampoline — a
  `Self` method cannot be one. It keeps the `generic_renderer` scope of the
  function it ports.)
- **Residual:** `generic.zig:1558` `log.warn("error rebuilding GPU cells err={}",
  .{err})`. Rustty's `rebuild_cells` is INFALLIBLE — its only OOM source aborts
  through the `Global` allocator — so the reference's `catch` arm does not exist
  and emitting the warn unconditionally would put a diagnostic on the SUCCESS
  path. PORT NOTE at `crates/rustty-renderer/src/generic.rs` AND (task-14c-5) at
  `crates/rustty-renderer/src/generic/linux_impl.rs` (search
  `error rebuilding GPU cells`). Same shape as the `graphics_storage` OOM waiver
  (`#s190-f2`).
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s115-f4

### S116-F2 — PARTIALLY RESTORED, WAIVED (two cited anchors unreachable)
- **BOTH TWINS NOW LIVE (task-14c-5, N9)** — same story as `#s115-f4` above. All
  eighteen of this row's anchors DO exist in `generic/linux_impl.rs` (none of them
  is display-link code), so the split there is the same as on macOS: **sixteen
  live, two residual** (`:2912`/`:3181`). The Linux twin's `prep_background_image`
  also had the same three `eprintln!` stand-ins and the same collapsed
  `std::fs::read`; both are fixed there now, exactly as task-14c-1 fixed them in
  the Metal twin, so the two open/read messages are verbatim on both arms.
- 16 of the row's 18 cited oracle anchors are now live `log::` emissions with the
  `generic_renderer` target, **on both twins** (Linux restored by task-14c-5):
  `generic.zig:1960, 1974, 1995, 2002, 2137, 2627,
  2777, 3143, 3150, 3222, 3237, 3461, 3476, 3481, 3514, 3518`. This includes the
  three `eprintln!` stand-ins on the `prepBackgroundImage` path, and the row's own
  "open/read warnings merged" complaint: the port had collapsed the reference's
  two distinct messages into one invented string over a single `std::fs::read`;
  that is now `File::open` + `read_to_end` with the oracle's two messages verbatim.
- **Residual:** `generic.zig:2912` and `:3181`, both
  `log.warn("error caching font shaping results err={}", .{err})`. Rustty's
  `ShaperCache::put` (`crates/rustty-font-shaper/src/shaper/cache.rs:136`) is
  INFALLIBLE — it allocates through `Box`/`Global` — so, again, there is no
  `catch` arm. Both sites now carry the reference's `log.warn` as an R1 comment
  plus a PORT NOTE (search `error caching font shaping results` in
  `crates/rustty-renderer/src/generic.rs`); before this stage neither site had so
  much as a comment marking the dropped diagnostic. task-14c-5 did the same for the
  two corresponding sites in `crates/rustty-renderer/src/generic/linux_impl.rs`,
  which likewise had no comment at all.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s116-f2

## Batch 14c-2 — renderer macOS-backend `LOG` waivers (task-14c-2, 2026-09-10)

### S117-F3 — PARTIALLY RESTORED, WAIVED (two cited anchors unreachable)
- **BOTH TWINS NOW LIVE (task-14c-5, N9).** `image.zig` has two Rust twins,
  `image/macos_impl.rs` and `image/linux_impl.rs`, mutually exclusive per build.
  task-14c-3 fix round 1 NARROWED this entry because `image/linux_impl.rs` had no
  `log::` emission at all; **task-14c-5 has now restored it**, with the SAME
  11-of-13 split and the SAME two residuals (`:365`, `:447`, infallible on both
  arms). `image/linux_impl.rs` now carries
  `const LOG_SCOPE: &str = "renderer_image";`. (`shadertoy.rs` is platform-neutral,
  so the three `shadertoy.zig` sites named below were already live on every
  platform.)
- 11 of the row's 13 cited `image.zig` anchors are now live `log::` emissions with
  the `renderer_image` target, **on both twins** (Linux restored by task-14c-5, the same
  11-of-13 split): `image.zig:85, 122, 132, 165, 378, 396, 423, 486,
  646, 660, 747`. The row's title also covers `shadertoy.zig`, and ALL of its live
  sites are restored: `:71` (`loaded custom shader path={s}`), `:117` (the
  spirv-error warn, whose arguments the port's comment had elided behind `...`) and
  `:298` (the spirv-cross error callback, which no row's oracle column claims —
  restored per PROCEDURE.md blind spot 3). `shadertoy.zig:424` stays a comment
  because it is a comment in the REFERENCE too.
- **Residual 1:** `image.zig:365`,
  `log.warn("error deferring relative placement err={}", .{err})`. The reference's
  `pending_relative.append(alloc, …)` takes an allocator and can fail; rustty's
  `Vec::push` cannot (OOM aborts through `Global`). There is no `catch` arm, and
  hoisting the warn out of it would fire it on every SUCCESSFUL deferral.
- **Residual 2:** `image.zig:447`,
  `log.warn("error tracking virtual origin err={}", .{err})`. Same shape:
  `virtual_origins.getOrPut(alloc, …)` is fallible, `HashMap::entry` is not, and
  the reference's `break :fold` on that arm has no counterpart to reach either.
- Both sites now carry the reference's `log.warn` as an R1 comment plus a PORT
  NOTE, in BOTH twins (search `error deferring relative placement` and `error
  tracking virtual origin` in `crates/rustty-renderer/src/image/macos_impl.rs` and
  `crates/rustty-renderer/src/image/linux_impl.rs`).
- Same shape as `#s115-f4` / `#s116-f2` / `#s190-f2`.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s117-f3

### buffer-zig-contents-null — two unrestorable sites that NO row claims
- `src/renderer/metal/buffer.zig:98` and `:155` each carry
  `log.warn("buffer contents ptr is null", .{})` inside an `orelse` guard on
  `msgSend(?[*]u8, objc.sel("contents"))`. objc2's `MTLBuffer::contents()` returns
  `NonNull<c_void>`, so the guard is structurally unreachable in rustty and the
  warn has no failure arm.
- **No ledger row cites `buffer.zig` at all** — `S119-F1` covers `shaders.zig`
  only — so no row's state is affected by this and no row is being `CLOSED` over a
  hole it claimed. It is recorded here, at both code sites, and in
  `NEW-FINDINGS.md` as [[N7]], so the finale re-audit meets it as a tracked item.
  (It was written as N6 and renumbered to N7: another worker committed a different
  N6 into `NEW-FINDINGS.md` in the same minute.)
- Consequence: `crates/rustty-renderer/src/metal/buffer.rs` deliberately has NO
  `LOG_SCOPE` constant, because it has no live emission and an unused constant is a
  `-D warnings` failure.

## Batch 14c-4 — src/font `LOG` waivers and narrowed residuals (task-14c-4, 2026-09-10)

The eleven `class=LOG` rows in `src/font` (`S84-F1`, `S86-F2`, `S90-F2`,
`S91-F3`, `S91-F7`, `S91-F8`, `S91-F10`, `S94-F1`, `S94-F4`, `S95-F1`,
`S97-F2`). Six closed outright; `S91-F3` was already waived; the four below are
the ones that needed a written disposition.

Per-file inventory (reference live `log.*` call sites : live `log::` emissions
after this stage). A reference site that is COMMENTED OUT UPSTREAM is not a live
site and is excluded — the counts below say so where it matters.

| reference file | live sites | emissions after | note |
|---|---|---|---|
| `CodepointResolver.zig` | 7 | 7 | 8 grep hits; `:347` is a comment upstream |
| `Collection.zig` | 13 | 13 | |
| `SharedGridSet.zig` | 6 | 6 | |
| `face/coretext.zig` | 8 | 8 | 9 grep hits; `:195` is a comment upstream |
| `face/freetype.zig` | 5 | 5 | |
| `face/web_canvas.zig` | 5 | 7 | 5 unique; 2 are emitted twice because the port has both a `rustty_wc_*` and a `ghostty-wasm-abi` unprefixed export of the same reference `export fn` |
| `shaper/coretext.zig` | 4 | 2 + 2 waived | 8 grep hits; 4 are inside `debugPositions`, whose only call site is commented out upstream |
| `shaper/feature.zig` | 1 | 1 | |
| `shaper/web_canvas.zig` | 2 | 2 | |
| `shaper/harfbuzz.zig` | 0 | 0 | 7 grep hits, all inside the never-called `debugPositions` |
| `DeferredFace.zig` | 3 | 2 + 1 waived | **no ledger row names this file** |

### S94-F4 — PARTIALLY RESTORED, WAIVED (two cited anchors unreachable)
- 2 of the row's 4 cited oracle anchors are now live `log::` emissions with the
  `font_shaper` target: `shaper/coretext.zig:254` (`BUG: CFRelease pool was not
  empty`, under `#[cfg(not(test))]` — the faithful analogue of the reference's
  `comptime !builtin.is_test`, which means this site is deliberately not
  observable from this crate's own test binary, exactly as it is not observable
  from the reference's) and `:264` (`error notifying cf release thread to stop`).
- **Residual 1:** `:277` `log.warn("error converting release pool to owned
  slice, slow release err={}")`. It sits in the `catch` arm of `toOwnedSlice`,
  which REALLOCATES and can fail. The port uses `std::mem::take`, which moves
  the existing `Vec` out and leaves an empty one: it allocates nothing and
  returns no `Result`. There is no failure arm; emitting would fire on the
  success path.
- **Residual 2:** `:291` `log.warn("error notifying cf release thread to wake
  up, may stall err={}")`. The reference performs a SEPARATE `wakeup.notify()`
  after a successful mailbox push. An mpsc `send` IS the wakeup — the channel
  unparks the receiver as part of the same infallible operation — so the port's
  `Ok(())` arm has no second fallible step. Emitting would fire on every
  successful frame end.
- Both carry the reference's `log.warn` as an R1 comment plus a PORT NOTE at the
  code site (search `WAIVED site 1 of 2` / `WAIVED site 2 of 2` in
  `crates/rustty-font-shaper/src/shaper/coretext.rs`).
- Same shape as `#s115-f4` / `#s116-f2` / `#s190-f2`.
- Disposition: state=WAIVED, owner=.port/parity-closure/ADJUDICATION.md#s94-f4

### N4 — `deferred_face_new`'s dropped warn (no ledger row; WAIVED)
- `DeferredFace.zig` has THREE live `log.warn` sites and NOT ONE of the four
  `src/font` rows that name the file (`S85-F2/F3/F5/F6`) is `class=LOG`. This is
  the under-count shape PROCEDURE.md blind spot 3 was written for: the sites were
  found by enumerating the reference's own calls per file, not by trusting the
  rows' cited anchors.
- `:403` (`failed to init face for codepoint check`) and `:463` (`error loading
  deferred face`) are now live emissions with the `deferred_face` target.
- **Waived:** `:430` `log.warn("error creating deferred face err={}")` in
  `deferred_face_new`. The reference's only failure mode there is OOM from
  `alloc.dupeZ`/`alloc.create`; Rust's `Vec`/`Box` ABORT on allocation failure
  rather than returning an error, so the port has no reachable failure arm and no
  `null` return. The emission could only sit on the success path.
- PORT NOTE at `crates/rustty-font-shaper/src/deferred_face.rs` (search
  `WAIVED site`).

### S97-F2 — LOG HALF CLOSED, ROW STAYS OPEN (narrowed residual)
- The row makes TWO claims. The logging one is now closed: `shaper/web_canvas.zig
  :269` (`error during shaper test`) and `:296` (the per-run `y=.. run=..
  shape=.. idx=..` info) are live emissions with the `font_shaper` target. Doing
  so surfaced a second defect: `shaper_test_` returned `Result<(), ()>`, so the
  restored message would have read `err=()` — invisible while the log was a
  comment. Replaced with a `ShaperTestError` enum whose `Display` renders the
  reference's own `error.Name` at each site.
- **Residual, unfixed:** the row's other claim — the port "shapes one run per row
  instead of iterating RunIterator runs" — is real and remains. Reaching the
  modern `RunIterator` needs a `shape::RunOptions` carrying a
  `rustty_vt::render::CellSlice`, i.e. a reconstructed RenderState, which the
  wasm harness does not build. This is unported work, NOT an unreachable arm, so
  the row is NOT waived: it stays OPEN with a narrowed `owner`.
- Its old `owner` (`NOT-CLOSABLE YET: ... Needs a rustty_wc_* install site
  first`) is now false and has been replaced — that install site landed in this
  stage.

### S94-F1 — STALE CLAIM CORRECTED, ROW STAYS OPEN
- The row's complaint is that `shaper/coretext.rs` uses an `mpsc` stand-in for
  `os.CFReleaseThread` "under a stale 'deferred seam' PORT NOTE while rustty-os
  ships the real port". The stale claim is now corrected in place: the note used
  to assert the libxev backend is "a DEFERRED SEAM ... all `unimplemented!()`",
  which is FALSE on macOS (the only target this file compiles for) —
  `rustty-xev`'s no-backend seam is `cfg`-gated to freestanding wasm and
  `compile_error!`s if it is ever active for a non-wasm target, and
  `rustty_os::cf_release_thread::Thread::{init,deinit,threadMain}` are real,
  inhabited ports against the real kqueue backend.
- **The swap itself was NOT done** — it is a runtime refactor, not a logging
  change, and out of this stage's scope. The row stays OPEN with an `owner`
  recording that the blocker it rested on does not exist.

### N6 — `discovery.zig`'s two table-parse warns (no ledger row; RESTORED)
- Added in task-14c-4 fix round 1 (review item F4). `src/font/discovery.zig` has
  exactly two live `log.*` calls — `:762` (`error parsing head table`) and `:779`
  (`error parsing OS/2 table`) — and NO ledger row names either. The port
  discarded both failures with `.ok()`, keeping the diagnostics as PORT NOTE
  comments; the file had no `LOG_SCOPE` at all.
- **Not waivable:** `Head::init` and `OS2::init` both return `Result`, so these
  are genuine reachable failure arms, unlike this batch's `S94-F4` and `N4`
  residuals. Both are now live on the reference's `discovery` scope.
- The `catch` arms were extracted into `parse_head_table` / `parse_os2_table`
  free fns so the failure arm is reachable from a test at all; four tests cover
  them (malformed warns exactly once and still degrades to `None`; well-formed
  emits nothing).
- Full entry: `.port/parity-closure/NEW-FINDINGS.md`, section `N6`.
- Disposition: no row to close; tracked as finding N6.

### Inventory addendum (fix round 1)
`src/font/discovery.zig` was not in this batch's original per-file table because
no row named it. Its line: **2 live sites : 2 emissions after.** With it, every
file under `src/font` that contains a live reference `log.*` call has now been
enumerated and reconciled against the port.

## task 14d-1 — `rustty-core`'s dropped diagnostics

### S41-F1 — WAIVED (not CLOSED): two sites in range have no failure arm
- Every other site inside the row's cited span (`Surface.zig:2193-4345`) is live:
  28 live reference `log.*` calls in that span, 26 of them now emitting.
- The two that do not, and cannot:
  - `Surface.zig:3074` — `ev.binding = self.alloc.dupe(input.Binding.Action,
    actions) catch |err| binding: { log.warn("error allocating binding action for
    inspector err={}", .{err}); break :binding &.{}; };`
  - `Surface.zig:3258` — `const copy = self.alloc.alloc(u8, slice.len) catch |err|
    { log.warn("error allocating pty data for inspector err={}", .{err}); break
    :pty; };`
- Both are OOM-only `catch` arms over an allocation. The port's counterparts are
  `actions.to_vec()` and `slice.to_vec()`: Rust's global allocator ABORTS on
  allocation failure rather than returning an error, so there is no `Result`, no
  branch, and nothing to log from. This is the workspace-wide allocator doctrine,
  not an oversight in this file, and it predates this stage — the PORT NOTE at
  each site already says so ("the OOM-only catch collapses — infallible global
  allocator; the `&.{}` fallback arm is unreachable").
- Emitting anything at either site would put a diagnostic on a path that only
  ever succeeds — the bug class this plan exists to catch — so the sites stay as
  they are and the ROW is `WAIVED` rather than `CLOSED`, per the stage brief's
  rule ("Before restoring, confirm a real failure arm exists. If not: WAIVED").
- `owner` records the two file:line pairs. The row's 26 restorable sites ARE
  restored; the waiver is about the two that are not defects.

### S41-F1 — three cited anchors contain no `log.*` call at all
- `Surface.zig:2854`, `:3671` and `:3989` are cited by the row and, at pin
  `20abdb50a`, hold `fn maybeHandleBinding(`'s signature, `fn isMouseReporting`'s
  body, and a `break :click;` respectively. There is no `log.` within 40 lines of
  any of them.
- The row cites 30 anchors for 28 real sites in its span. Recorded because a
  reader reconciling anchor-count against site-count will otherwise conclude two
  sites were missed.

### Sites restored that NO row claims
- Both `Surface.zig` rows start at `:2193` (S41-F1) and `:4424` (S42-F2). The
  file's first 2192 lines contain **37 live reference `log.*` calls** — the whole
  of `init`, `deinit`, `handleMessage`, `updateConfig`, the clipboard writers and
  the renderer-health/scrollbar notifiers. No row claims any of them. All 37 are
  restored, under PROCEDURE.md blind spot 3 ("a site you find that no row claims
  still gets restored").
- One of those unclaimed sites is NOT restored, for the same infallible-arm
  reason as the two above: `Surface.zig:767` (`error copying command for title,
  title will not be set err={}`), whose `Command::string()` is infallible in the
  port. It belongs to no row, so it changes no row's state; recorded here so it
  is not mistaken for an oversight.

---

## task 14d-3 — `rustty-apprt` and the GTK tree

### S46-F5 — WAIVED (not CLOSED): 8 of `embedded.zig`'s 54 sites have no arm

46 of 54 are live. The 8 that are not:

| reference site | why there is no arm |
|---|---|
| `:251` | the port initializes `App.keymap` EAGERLY; upstream is now LAZY, and the warning lives in `keyboardLayout`'s lazy-init `catch`. Structural drift — `NEW-FINDINGS.md` **N18**. |
| `:353` | `clone_config()` is infallible (no allocator). |
| `:540` | `openDirAbsolute` + `dir.stat()` collapse into one `std::fs::metadata`, so the separate stat-error arm cannot occur. `:531`'s arm covers both. |
| `:559` | `WorkingDirectory::finalize()` returns `()`. |
| `:828 :883 :899` | `stackFallback` conversion buffers are owned `Vec`s; Rust allocation is infallible. |
| `:1983` | `dupeZ` -> `to_vec()`; same. |

PROCEDURE.md blind spot 1 is explicit that a row with a known unrestored site is
`WAIVED` with the reason in `owner`, never `CLOSED`. The carve-out does not apply:
this row's title claims the WHOLE FILE ("all backend `log.*` sites in
`embedded.rs` are elided"), so every one of the 54 is inside its claim. Understating
completion costs nothing.

### `embedded.zig`'s CAPI block ports to a DIFFERENT CRATE, and the portmap did not say so

`LOG-PORTMAP.tsv` mapped `src/apprt/embedded.zig` to
`crates/rustty-apprt/src/embedded.rs` alone. 23 of the file's 54 sites are inside the
reference's `CAPI` block, which ports to `crates/rustty-apprt-c/src/lib.rs` — a
different crate. Those 23 could never have been credited, and 22 of them were sitting
there as bare `eprintln!`s, invisible to BOTH ratchets (a bare `eprintln!` carries no
`warn(scope):` prefix, so `G-LOGFILE` counts it as neither an emission nor a bypass).
The arm now sums both files. This is the same class as the portmap header's own
"filename guessing gets at least five of these wrong" list; it is now six.

### The arm's green is an OVER-CREDIT, stated so nobody reads it as completeness

`src/apprt/embedded.zig` scores 54/54 with 8 sites genuinely unrestored. It reaches 54
because the shared arm also carries `src/main_c.zig`'s 5 emissions (a declared overlap)
and 3 port-only D11 C-boundary range guards. 46 real + 8 over-credit = 54. The ratchet
FORCES `allowance=0` once measured deficit is 0, so the honest number cannot live in the
allowance; it lives in the `reason` cell and here.

### S50-F4 — the inverse defect, plus the gate change that had to accompany it

`globalShortcutTrigger` emitted `global shortcut action performed ok` on its SUCCESS
path. `application.zig:1741-1749` is a bare `catch` with no success arm. Removed.

`.port/tools/gates/ws15_globalshortcuts_portal_gate.sh` asserted that line, so its grep
is deleted. **R5 argument that this is not a weakening:** the gate's very next assertion
counts a new toplevel window on the session bus, and the bound action is `new_window`. A
window appearing ENTAILS `performAllAction` having been reached (through the `trigger`
emission and an installed thunk) and having returned Ok. The retained assertion strictly
implies the removed one; only the granularity of the failure MESSAGE changes, and that
message now names all four causes.

### Two more inverse defects found, and three rustty-only lines KEPT

Found while working, under their own heading in the report:

- `rustty_app_set_color_scheme` and `rustty_surface_set_color_scheme` each printed
  `invalid color scheme ... value={}` on an out-of-range enum. At the pin BOTH oracle
  guards are `std.enums.fromInt(...) orelse return;` — silent (`embedded.zig:1788`,
  `:1992`). The R1 comments reproduce a SUPERSEDED revision. **Removed**, matching the
  precedent already in that same file: `rustty_surface_mouse_pressure` had an identical
  stale-revision emission removed earlier for exactly this reason.
- **KEPT**: three rustty-only instrumentation lines in `global_shortcuts.rs` that the
  WS15 live portal gate genuinely depends on and that have no substitute (the
  reference's own `activated={s}` fires BEFORE the map lookup, so it cannot separate
  "signal arrived" from "signal resolved"). They are moved off the reference scope onto
  `target: "rustty_gtk_instrumentation"` so they cannot be mistaken for restored
  reference diagnostics. Listed as candidates for a later stage.

### A FIFTH emission shape exists, and it must never become the facade

`gtk/pre_exec.rs` runs between `fork()` and `exec()`. Its two `log.err` sites were
already live as a raw async-signal-safe `write(2)` of a `&'static [u8]` — `log::error!`
formats through `core::fmt`, allocates in `AppLogger::log`, and takes the process
`Stderr` lock, all forbidden there. `G-LOGFILE` saw neither. Rather than teach the
scanner a new shape (R5: net-new coverage plus a self-test arm), the helper is renamed
`write_stderr` -> `log_err_raw` so it presents as the fn-facade shape the gate already
knows. Bytes, fd and async-signal-safety untouched.

### `logVersion` is a scanner blind spot the reference's own naming creates

`adw_version.rs` / `gtk_version.rs` keep an allowance of 1 each, and it is NOT a dropped
diagnostic. `discover_emitters`'s `FN_DEF` is `\bfn\s+(log_[A-Za-z0-9_]*)`; the reference
names the function `logVersion`, the port spells it `log_version`, so `log_version` is
itself classified as a FACADE and `rust_emissions`'s `inside()` excludes every primitive
within its own span. The emission is credited only at `log_version`'s CALL sites, which
are in `application.rs`. Recorded in both `reason` cells.

### Folds a count ratchet cannot see

Three files measure short with every reference site covered, because several reference
sites route through ONE shared helper whose single `log::warn!` reproduces each caller's
own message verbatim from its `what` argument:

| reference file | reference sites | port emissions | fold |
|---|---|---|---|
| `resize_overlay.zig` | 4 | 1 | 4 `unable to remove ...` -> `remove_source_id` |
| `split_tree.zig` | 15 | 11 | 5 -> `remove_source_id` |
| `surface.zig` | 63 | 62 | 4 -> `remove_source_id` |

The inverse also happens: `wayland/Globals.zig` measures 10 against 6 because
`Globals.zig:150`'s single `log.debug("matched {}")` sits inside an `inline for` over the
tag list, which Rust's if-else dispatch turns into seven static sites with identical
runtime behaviour.

### Message texts that had drifted (none of them in a row)

Found by comparing every reference message against its port counterpart, not by the
rows: the `Fatal: ` and `X11: ` prefixes in `x11.rs`; `gtk-x11` for the reference's
`gtk_x11` scope; `after surfaces initialized` for `after surfaces have been initialized`;
`unable to remove {what} source={}` where no pinned reference site prints `source=`
(a superseded-revision shape, found in THREE files); `common cause: <url>` for
`this is a common cause of this issue: <url>`; six `open uri ` prefixes and six
`global shortcuts ` prefixes the reference does not use; and a `token={x}` suffix on four
OpenURI messages. All restored verbatim.

## task 14e-1 — `rustty-termio` (and `rustty-inspector`'s termio widget)

Seven reference files, 129 live sites, both crates starting from **no `log` dependency at
all**. 120 sites restored, 9 not — the 9 are enumerated below, each with a PORT NOTE at its
own code site and an `LOG-PORTMAP.tsv` allowance ratcheted to exactly that count.

**One audit row (`S212-F5`, `mailbox.zig`, 2 sites) — CLOSED.** The other six files are
pure `G-LOGFILE` findings: no row names them, because the portmap was rebuilt from the
REFERENCE tree (N15) rather than from the audit, and the audit never looked at them. The
brief said "seven reference files map into `rustty-termio`"; six do. The seventh,
`src/inspector/widgets/termio.zig`, maps into **`rustty-inspector`**, which is not in
`DECISION-logging.md` §6r IMPORTANT 5's crate table at all — it gained its `log`
dependency here.

### The 9 sites NOT restored — all one shape, all WAIVED for the same reason

Every one of them is a reference diagnostic whose failure the PORT CANNOT PRODUCE. There
is no arm to hang it on, and emitting it unconditionally would put a diagnostic on a
SUCCESS path — the bug this project shipped once (a warning outside its `catch` firing on
every successful scroll). Understating completion costs nothing; a false record does not.

| reference site | message | why there is no arm |
|---|---|---|
| `Exec.zig:731` | `error building XDG_DATA_DIRS; err={}` | hangs off `std.fmt.bufPrint`'s `NoSpaceLeft` into a fixed `[max_path_bytes]u8`; the port formats into a growable `Vec` |
| `Exec.zig:748` | `error building MANPATH; man pages may not be available; err={}` | same |
| `Exec.zig:840` | `failed to allocate space for command args, falling back to basic shell` | the `error.OutOfMemory` arm; Rust allocation here is infallible (OOM aborts) |
| `Exec.zig:1078` | `error killing command during cleanup err={}` | lives inside an `errdefer` with no reachable error return between its registration and the success return |
| `Exec.zig:1469` | `read thread exiting, failed to spawn gather thread err={}` | `std::thread::scope` has no fallible spawn |
| `Termio.zig:705` | `error recording pty read in inspector err={}` | `Inspector::recordPtyRead` is infallible in the port (Task 16 — the OSC parser allocator collapses under Rust's global allocator) |
| `stream_handler.zig:1547` | `error notifying surface of pwd change err={}` | `WriteReq::init` is infallible; a live call fires on every SUCCESSFUL pwd change |
| `widgets/termio.zig:358` | `failed to format terminal IO events for copy err={}` | `Stream::formatEvents` is infallible in the port |
| `widgets/termio.zig:378` | `failed to format terminal IO events for export err={}` | same |

None of these falls inside `S212-F5`'s cited anchors, so no row's state rests on them.

### Three EMPTY `log_*` stub helpers removed

`mailbox.rs` carried `log_warn_notify_failed` / `log_warn_wake_failed` and
`termio_impl.rs` carried `log_warn_input_open_failed`, each an empty-bodied function
holding a commented reference call. G-LOGFILE lists exactly this shape under its own
`NOTE:` line: an empty `log_*` helper is a DROPPED diagnostic, not a facade. All three are
gone and the calls are back inline where the reference has them. (11 such stubs remain in
`rustty-os`; out of scope here.)

### Zig constructs that needed a named shim, not an approximation

- **`@tagName(message)`** (`Thread.zig:313`) → `Message::tagName()`. The Rust variants are
  PascalCase (R2 naming), so `{:?}` would have put the wrong bytes on the wire.
- **`@tagName(dynamic)`** (six `stream_handler.zig` color sites) → `color::Dynamic::tag_name`
  in `rustty-vt`, same reason.
- **`@tagName(posix.errno(...))`** (`Exec.zig:213, :1759, :1770`, all `err=E{s}`) →
  `errno_tag_name`. Its test asserts the transcribed numbers against the HOST's own
  `strerror` rather than trusting them, per the `rustty-cli` `errno_suffix` precedent — and
  that test earned its keep immediately, catching two wrong `strerror` fragments on Darwin.
- **`{x}` on a `[]const u8`** (two response sites) → a `HexBytes` Display wrapper; Rust's
  `{:x}` has no slice impl.
- **`defer log.debug(...)`** (`Thread.zig:229, :238, :278`) → three DIFFERENT things, and
  conflating them would have been wrong. `:238` must fire on `threadEnter(...)?`'s early
  return, so it is a `Drop` guard. `:278` is registered LAST, so Zig's LIFO order fires it
  FIRST on exit, before the teardown — written inline right after `loop.run` returns.
  `:229` scopes an `if` block with no early exit — its last statement.

### `stream_handler.zig:186` — the `vt` funnel had dropped half its message

The reference's `vt` takes a COMPTIME `Stream.Action.Tag` and logs
`action={} err={}`. The port collapses that dispatcher into one trait method per action,
so the tag is not a value at the site, and the port had dropped `action=` entirely. Each
of the 38 callers now passes its own reference `Action.Tag` field name; all 38 were checked
against the oracle's own `Action` tag list. The type-name prefix Zig's `{}` puts on an
exhaustive enum is not reproduced — N11's class.

### Two stale pre-resync Zig comments found, and deliberately NOT emitted

Both reproduce an OLDER upstream shape that no longer exists at pin `20abdb50a`. Emitting
either would be an emission the reference does not make — the inverse defect, caught before
it was introduced rather than after.

- `stream_handler.rs`'s OSC 7 `UriHostTooLong` arm carries
  `failed to get full hostname for OSC 7 validation: {}`. At the pin, `uri.getHost`'s error
  set is `error{UriMissingHost}` ONLY — there is no such site. The arm is left alone
  (removing it is a behavioural change, out of scope) with a note; no live call added.
- `exec.rs`'s `setNonblock` carried `read thread failed to get flags err={}` plus a second
  line, `this isn't a fatal error, but may cause performance issues`. At the pin the
  message renders the ERRNO NAME (`err=E{s}`) and there is no second line. The live calls
  follow the pin, and the comments were corrected to the pin.

A third, `termio_impl.rs`'s `failed to get current time err={}`, is stale the same way
(`Termio.zig` has no such site at the pin) and likewise got no emission. It is left in
place as a comment; see `NEW-FINDINGS.md` N20.

### The count is +122 for 120 sites, and that is expected

`G-LOGFILE` counts, it does not attribute. `exec.rs` carries several reference sites twice
under mutually-exclusive `cfg` arms (`started subcommand path=…` on the posix and windows
arms; the flatpak spawn under `#[cfg(feature = "flatpak")]`), so the textual scan credits
more emissions than there are reference sites. The per-file allowances are set from the
SITE inventory above, not from the delta.

### task 14e-1 fix round 1 — three corrections

**1. `Exec.zig:1469` was waived on a false claim, and the ledger already refuted it.**
The waiver said `std::thread::scope` has no fallible spawn. `Builder::spawn_scoped` returns a
`Result`; the site discarded it with `.expect("spawn io-gather")`. **There was an arm — it
panicked.** And `S211-F6` is a CONFIRMED, OPEN row on this same file titled "gather-thread spawn
failure panics instead of logging and returning", which had recorded exactly this independently.
So the first round waived as unrestorable a diagnostic the repository's own ledger records as
missing *because the port panics instead of logging*.

The waiver is reverted and both halves are restored: the `error` emission, and the reference's
**log-and-return** semantics (the `return` leaves the `scope` closure, which is the last statement
of `threadMainPosix`, so the read thread exits exactly as the reference's does; nothing was
spawned, so there is nothing to join). `Exec.zig`'s waived count drops 5 → 4 and its allowance
3 → 2. `S211-F6` stays **OPEN** with a narrowed `owner`: the code half is done, but no test drives
it — forcing a real thread-spawn failure needs resource exhaustion, and a flaky exhaustion harness
would be worse than an untested fix. That row's port line range was also stale (`4232-4245` against
code near `4611`) and is corrected.

**The lesson generalises.** A waiver asserting "the port cannot produce this failure" is a claim
about the port's control flow, and it must be checked against the code AND against the ledger's
other classes — this one was contradicted by an `INDIV`/`missing-error-path` row that a `LOG`-class
sweep never looks at. Grep the ledger for the reference file before waiving anything in it.

**2. The forked-child site used the facade, and the reasoning was backwards.**
`Exec.zig:1045` runs between `fork(2)` and `execve(2)`. The first round kept `log::error!` there,
arguing that `rustty-apprt`'s `gtk/pre_exec.rs` raw-`write` shape applies where the reference is
async-signal-safe and this site is not. That is backwards: `pre_exec.zig:39` and `:58` **also call
`log.err`**, through the same global `logFn`. The two are the *identical* reference construct, and
that port's own note calls the call "the reference's one liberty".

The hazard is concrete, not theoretical. `rustty-core`'s `AppLogger` — the backend the real app
installs — both allocates (`logging.rs:61,76,97`) and takes the process-wide stderr lock
(`logging.rs:99`). The parent always has renderer, io and gather threads running, so either lock
can be held by a thread that did not survive the fork, and the child then hangs with **no
diagnostic at all** — strictly worse than the failure the diagnostic exists to report.

Fidelity here is owed to the reference's observable OUTPUT, not its call mechanism, and the
raw-write shape loses nothing because it hand-renders the same bytes. The site now calls
`log_err_child_raw`, which formats into a 256-byte **stack** buffer through `core::fmt` (no
allocation; the only argument is a derived `Debug`) and then issues one `write(2)`. `G-LOGFILE`
counts it: the `log_` prefix plus the `writeln!` inside makes `discover_emitters` classify it as
the fn-facade shape (shape 3), so the call site is credited rather than read as dropped, and the
N14 bypass count is unchanged at 60.

**The bytes were verified, not asserted.** `log_err_child_raw_writes_exactly_what_logfn_would`
redirects fd 2 through a pipe and reads back what was actually written:

```
error(io_exec): error initializing child: Error\n
```

The level token is `error`, not `err` — Zig's `Level.asText` maps `.err` to `"error"`
(`lib/std/log.zig:45`). That is the third time this rendering has been at risk on this plan, so
the test asserts the token separately from the whole line, and a control that changes `error(` to
`err(` fails it. A second, structural test fails if `log::error!`/`eprintln!`/`format!` ever
reappears inside the helper.

**3. The dropped-space class was reintroduced in the very commit that discovered it.**
Four sites — `stream_handler.rs:3524, :3544, :3557, :3570` — emitted `mode:Flags { … }` where the
reference emits `mode: <value>`. Cause: a scripted edit built each format string as `msg[:-3]`,
stripping the `{}` **and the space before it**. Same partial-message class as the `action=`
truncation the same commit found and fixed.

Fixed, and then **all 120 restored messages were swept for the class**: a checker normalises every
restored `log::` format string and the oracle's own `log.*` format strings by replacing every
placeholder with a sentinel, and requires an exact match. (Substring matching does *not* catch
this — `"pushing kitty keyboard mode:"` is a substring of the reference's
`"pushing kitty keyboard mode: {}"`. The first version of the checker was written that way, scored
zero, and was self-tested by reintroducing the bug — which it missed.) The sweep reports **6**
mismatches, all intended and individually verified: five R2 identity substitutions
(`Ghostty`/`ghostty` → `rustty_identity::APP_NAME*`) and `DeferLog`'s `"{}"`, which carries its
message as an argument. The coordinator is scheduling the machine version as gate work.

**Also corrected:** the first round's report attributed the second cfg-duplicated emission pair to
the flatpak spawn. Wrong — that occurs once, at `exec.rs:3258`. The two duplicated pairs are
`cannot access cwd, ignoring` (`:2929`/`:3202`) and `started subcommand path={s} pid={?}`
(`:2985`/`:3361`), both being the posix and windows `start` arms. The count of two was right; the
attribution was not. Fixed in the portmap `reason` cell.
