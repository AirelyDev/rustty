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
