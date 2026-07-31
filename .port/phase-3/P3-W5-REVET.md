# `P3-W5` re-vet verdict — 8 of 9 plan tasks were ALREADY SHIPPED

`P3-W5` ("`page::Mask` → printSlice → render → stream fast paths → benches") was
written before the **WS10 perf series** landed. WS10
(`docs/superpowers/plans/2026-07-23-ws10-perf-series.md`) owns the same BL-4
commits and merged them in `d7c3afa`, `1073c9d` and `18fc255`. Phase 3's binding
rule is *re-vet, do not assume*, and a verdict of "already ported" is only
acceptable with the rustty symbol, its file:line, and a test or gate that would
fail if it regressed. That evidence is below.

**Only Task 3 (`77190bd02`, `Screen::cursorScrollRegionUp`) was genuinely open**,
and WS10's own handoff note said so: `4e556ac`'s PERF_LOG section records T18 as
"still unported and NOT in this lane's scope … re-owned to Phase 3". That is
also Phase 3 re-vet row **P3-RV3**. It is ported by commit
`vt/screen: fast in-page scroll-region rotate …  (P3-W5 T3)`.

| Plan task | Oracle | Verdict | Evidence (symbol / file:line) | Guard that would fail on regression |
|---|---|---|---|---|
| T1 `page::Mask` | `446f80f4e` page.zig | **SHIPPED** (WS10 T19) | `page::Mask<T, MASK, GROUP_LEN>` `crates/rustty-vt/src/page.rs:1527`; `pattern:1562`, `strip:1570`, `match_scalar:1592`, `eql:1606`, `eql_scalar:1619`, `eql_prefix:1628` (raw-base), `eql_exact:1670`; `cell_field_mask`/`row_field_mask` | `page_tests.rs:2927` Mask unit + differential |
| T2 `print_slice` family | `47e26df60` `cb2d78587` `446f80f4e` `f2a7652ab` | **SHIPPED** (WS10 T16) | `Terminal::print_slice` `terminal.rs:1177`, `print_slice_fast:1208`, `print_slice_fill:1319` | `terminal_print_slice_differential_fuzz_vs_print` `terminal.rs:16604` |
| T2 Step 5 `b287f6d1a` test half | `b287f6d1a` | **SHIPPED** (`P3-W1 T4`) | PORT NOTE `terminal.rs:16456` records the `eraseDisplay` workaround removal | same differential |
| T3 `cursor_scroll_region_up` | `77190bd02` (+`0bad91adb` hunk) | **PORTED HERE** | `Screen::cursor_scroll_region_up` `screen.rs` | 7 tests incl. the 168-case fast-vs-slow differential; Miri harness `.port/phase-3/mirihar-scroll-region-up` |
| T4 `RenderState` two-phase | `446f80f4e` render.zig | **SHIPPED** (WS10 T19) | `pending_styles` `render.rs:129`, `StyleRun:138`, `begin_update:621`, `end_update:577`, `SCAN_GROUP_LEN = 8` `:1388`, `RowDirtyMask:1393`, `CellSpecialMask:1399`; `last_dirty_page` deleted (0 hits repo-wide) | `render::tests::incremental_updates_match_full_rebuild` `render.rs:2710` |
| T5 renderer consumer + link | `446f80f4e` generic.zig, `9c9cf3e82` | **SHIPPED** (WS10 T19 / T9) | `generic.rs:4571` `begin_update` inside the mutex, `:4668` `end_update` outside; `Link::active` `link.rs:92` | `link.rs:488` counting `#[global_allocator]` zero-allocation assertion (its PORT NOTE explicitly records why `result.count() == 0` alone would be vacuous) |
| T6 stream fast paths | `253e4f9c3` `1a88f3622` `300b42f42a` `f6f79acce` `8c523ed03` `47e26df60` `083d9709b` `b5053153f` | **SHIPPED** (WS10 T7/T13/T14/T15/T16/T17) | `HAS_VT_RAW` `stream.rs:289`, `consume_csi_params:1186`, `csi_entry_byte:1115`, `csi_dispatch_final`, `consume_apc_string:1045`, `apc_put_slice:605`, `log_unsupported_once:1619`; `WidenAsciiStore` / `DecodeNonAsciiUntilControlSeq` / `HWY_TARGET == HWY_SCALAR` arm `crates/rustty-simd/cbits/simd/vt.cpp:204,232,208` | `stream_apc_vector_invalid_branch_matches_scalar_path` `stream.rs:4406`; the `next_slice` fast-vs-general differentials at `stream.rs:4070+` |
| T7 two C-API exports | `446f80f4e` render.h | **SHIPPED** (WS10 T19) | `rustty_render_state_begin_update` / `_end_update` `crates/rustty-vt-c/src/render.rs:448,478`; `include/rustty/vt/render.h:42` "## Two-Phase Updates", `:392,:396` | **A5 arm (d)**: `bash .port/tools/gates/check_rustty_vt_headers.sh` → `OK (185 oracle exports checked; 0 documented residual(s))`. The arm's anti-vacuity check FAILS if the two are ported but left on the residual list, so the list cannot rot. Closes **P3-RV5**. |
| T8 `ScreenClone` / `TerminalStream` benches | `446f80f4e` `4c5b1d5d5` `258de36d1` | **SHIPPED** (WS10 T10/T12) | `Mode::RenderLocked/RenderClean/RenderPartial` + `Options.loops` `screen_clone.rs:45,105-117,215-217`; `terminal_stream.rs:249` `[0u8; 64 * 1024]` | `terminal_stream_uses_the_full_terminal_handler` |
| T9 `HyperlinkMap` / `ApcParser` benches | `7e14347c1` `f6f79acce` | **SHIPPED** (WS10 T11 / T17) | `crates/rustty-bench/src/hyperlink_map.rs`, `apc_parser.rs`; `Action` has 11 variants `cli.rs:22-45` | `bench_every_action_round_trips_through_from_name` + `bench_registers_compression_actions` (18 `rustty-bench` tests green) |

## What this lane adds beyond T3

`fix(vt): repair cargo test -p rustty-vt --release` — the release lane has been
**unbuildable on `main`** since `P3-W4 T5` widened the `set_string` tripwire gate
to `cfg(debug_assertions)` while its four tests were only marked
`#[cfg_attr(not(debug_assertions), ignore)]`. `ignore` skips execution, not
compilation. Verified pre-existing by stashing this lane's changes at `a76aff3`.

## Not done by this lane

The plan's finale asks for a perf re-measure against `perf-baseline-rust-vs-zig`.
**Not run.** This box was running several concurrent build lanes throughout the
session (including a 2-hour Miri compile), so any absolute timing taken here
would be noise reported as signal. `77190bd02` is a scroll-region optimization
whose upstream claim is 1.05×–1.49× on region workloads; measuring it needs a
quiet box and a paired same-session A/B. Filed as verification debt for the
Phase 3 measurement campaign (V17), which already owns the oracle-anchored
re-measure.
