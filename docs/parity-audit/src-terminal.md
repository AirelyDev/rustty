# Parity audit — src/terminal

Exhaustive functional-parity audit of the `src/terminal` area of the Ghostty oracle (pin `20abdb50a`) against rustty. 88 shards audited **162 oracle units** (every `.zig` file under `src/terminal`, including the `c/`, `apc/`, `compress/`, `kitty/`, `osc/`, `search/`, `snapshot/` and `tmux/` subtrees), reading **173,002 lines** — the whole of the area in scope, with the large files (`PageList.zig`, `Terminal.zig`, `Screen.zig`, `stream.zig`, `stream_terminal.zig`, `formatter.zig`, `page.zig`, `graphics_storage.zig`, `c/terminal.zig`, `graphics_exec.zig`) split across sequential parts so no line was skipped.

134 candidate findings were double-verified by two independent lenses (presence/relocation and semantic-equivalence): **90 CONFIRMED**, **20 DISPUTED** (one lens refuted), **24 REFUTED** (both lenses refuted), **0 UNVERIFIED** (the verification cap was never hit). By corrected severity across the 110 kept findings: **2 high, 36 medium, 72 low**. The overwhelming majority of the low tier is one systemic, deliberate class — rustty-vt wires no logging facade, so every `log.warn`/`log.err`/`log.info` diagnostic in the area is carried as a comment.

The single most important gap is **S151-F1**: `Options.codepoint_map` is discarded at every formatter delegation hop (`copy_options` hard-codes an empty map), so the user-facing `clipboard-codepoint-map` config option is silently inert in rustty — the map is set on the top-level `ScreenFormatter` by `Surface.copySelectionToClipboards` and then thrown away before it can reach `PageFormatter`, where the reference applies it.

## Coverage

One row per oracle unit (multi-part files aggregated across their shards).

| Oracle unit | Rustty counterpart(s) | Status | Note |
| --- | --- | --- | --- |
| `src/terminal/PageList.zig:1-20404` (10 parts) | `crates/rustty-vt/src/pagelist/{mod,access,resize,scroll,grow,erase,compress,pin,iter,builder,tests}.rs` | ported | Every decl, branch, errdefer/defer ordering, tripwire fail point and all ~330 test blocks have counterparts; raw-pointer accessors for SB discipline. |
| `src/terminal/Parser.zig:1-1116` | `crates/rustty-vt/src/parser.rs` | ported | State/TransitionAction/Action, SepList bitset, all 25 tests; C-ABI `Action.C` projection kept as comment. |
| `src/terminal/Screen.zig:1-12351` (6 parts) | `crates/rustty-vt/src/screen.rs` | ported | All cursor/scroll/select/resize/hyperlink/style paths + all tests; `io: std.Io` field is a documented NON_PORT. |
| `src/terminal/ScreenSet.zig:1-154` | `crates/rustty-vt/src/screen_set.rs` | ported | EnumMap → fixed arrays; both tests with identical assertions. |
| `src/terminal/Selection.zig` | `crates/rustty-vt/src/selection.rs` | ported | All 20 tests, assertion-for-assertion. |
| `src/terminal/SelectionGesture.zig` | `crates/rustty-vt/src/selection_gesture.rs` | ported | All 35 tests; `pixelFromFloat` replaced by Rust's saturating float→int cast (identical semantics). |
| `src/terminal/StringMap.zig` | `crates/rustty-vt/src/string_map.rs` | ported | Oniguruma-gated search iterator; url regex inlined (vt cannot depend on rustty-config). |
| `src/terminal/Tabstops.zig` | `crates/rustty-vt/src/tabstops.rs` | ported | All 7 tests; resize OOM path aborts instead of returning (S141-F3). |
| `src/terminal/Terminal.zig:1-16505` (8 parts) | `crates/rustty-vt/src/terminal.rs` | ported | Full struct/flags/modes/print/erase/resize/DECCOLM/kitty paths + all ~500 tests. |
| `src/terminal/UTF8Decoder.zig` | `crates/rustty-vt/src/utf8_decoder.rs` | ported | Tables verbatim; branchless reshape machine-checked against reference control flow. |
| `src/terminal/ansi.zig` | `crates/rustty-vt/src/ansi.rs` | ported | All enums and discriminants. |
| `src/terminal/apc.zig` | `crates/rustty-vt/src/apc/mod.rs` | partial | All 23 tests ported; the `build_options.glyph_protocol` comptime gate is missing from the handler (S149-F1). |
| `src/terminal/bitmap_allocator.zig` | `crates/rustty-vt/src/bitmap_allocator.rs` | ported | All 25 tests; private `dumpBitmaps` omitted (S149-F3). |
| `src/terminal/build_options.zig` | `crates/rustty-vt/src/{build_options,options}.rs` | ported | Features parse + `add()` resolution; generated module is `options.rs`. |
| `src/terminal/charsets.zig` | `crates/rustty-vt/src/charsets.rs` | ported | All tables verified element-wise. |
| `src/terminal/clipboard.zig` | `crates/rustty-vt/src/clipboard.rs` | ported | `Location` closed-enum divergence (S150-F2). |
| `src/terminal/color.zig` | `crates/rustty-vt/src/color.rs`, `crates/rustty-vt-c/src/color.rs` | ported | 33/34 tests equal; `RGB.parse` weakened (S150-F1). |
| `src/terminal/compress.zig` | `crates/rustty-vt/src/compress.rs` | ported | Module hub. |
| `src/terminal/csi.zig` | `crates/rustty-vt/src/csi.rs` | ported | — |
| `src/terminal/cursor.zig` | `crates/rustty-vt/src/cursor.rs` | ported | — |
| `src/terminal/dcs.zig` | `crates/rustty-vt/src/dcs.rs` | ported | All 10 tests; tmux gate mirrors `build_options`. |
| `src/terminal/device_attributes.zig` | `crates/rustty-vt/src/device_attributes.rs` | ported | All 7 tests. |
| `src/terminal/device_status.zig` | `crates/rustty-vt/src/device_status.rs` | ported | Encode sizes frozen and asserted. |
| `src/terminal/focus.zig` | `crates/rustty-vt/src/focus.rs` | ported | — |
| `src/terminal/formatter.zig:1-7030` (4 parts) | `crates/rustty-vt/src/formatter.rs` | partial | All formatters + all tests; `codepoint_map` dropped at delegation (S151-F1). |
| `src/terminal/fraction.zig` | `crates/rustty-vt/src/fraction.rs` | ported | All 43 assertions. |
| `src/terminal/hash_map.zig` | `crates/rustty-vt/src/hash_map.rs` | ported | 27 tests + 2 additive; `KeyIterator`/`ValueIterator` absent (S155-F2). |
| `src/terminal/highlight.zig` | `crates/rustty-vt/src/highlight.rs` | ported | OOM propagation folded (S155-F6). |
| `src/terminal/hyperlink.zig` | `crates/rustty-vt/src/hyperlink.rs` | ported | `PageEntry.hash` uses SipHash, not seed-0 Wyhash (S155-F1). |
| `src/terminal/kitty.zig` | `crates/rustty-vt/src/kitty/mod.rs` | ported | Submodule hub. |
| `src/terminal/lib.zig` | `crates/rustty-lib/*`, `crates/rustty-vt-c/src/io.rs` | replaced | C-ABI plumbing hub; verbatim comment block retained (S155-F7 covers the stale text). |
| `src/terminal/main.zig` | `crates/rustty-vt/src/lib.rs` | ported | Every import/re-export mapped. |
| `src/terminal/mem.zig` | `crates/rustty-vt/src/mem.rs` | ported | All 3 tests; madvise warnings dropped (S156-F3). |
| `src/terminal/modes.zig` | `crates/rustty-vt/src/modes.rs` | ported | 43-row table, all 14 tests. |
| `src/terminal/mouse.zig` | `crates/rustty-vt/src/mouse.rs`, `crates/rustty-apprt/src/gtk/class/surface.rs` | partial | `Shape.getGObjectType` stripped, GTK property untyped (S156-F2). |
| `src/terminal/osc.zig` | `crates/rustty-vt/src/osc/mod.rs` | ported | Parser/Capture/Terminator + 5 capture tests; header check tautological (S156-F1). |
| `src/terminal/page.zig:1-4462` (3 parts) | `crates/rustty-vt/src/{page,page_impl,page_tests}.rs` | ported | Page/Row/Cell/Mask/Capacity + all 34 tests. |
| `src/terminal/parse_table.zig` | `crates/rustty-vt/src/parse_table.rs` | ported | Every transition verified. |
| `src/terminal/paste.zig` | `crates/rustty-vt/src/paste.rs` | ported | OTP/grant/encode precedence identical. |
| `src/terminal/point.zig` | `crates/rustty-vt/src/point.rs`, `crates/rustty-vt-c/src/point.rs` | ported | — |
| `src/terminal/ref_counted_set.zig` | `crates/rustty-vt/src/ref_counted_set.rs` | ported | Full Robin-Hood/PSL logic + both tests. |
| `src/terminal/render.zig` | `crates/rustty-vt/src/render.rs` | ported | All 20 tests; two low deltas (S161-F1/F2). |
| `src/terminal/search.zig` | `crates/rustty-vt/src/search/mod.rs` | ported | Re-export hub. |
| `src/terminal/selection_codepoints.zig` | `crates/rustty-vt/src/selection_codepoints.rs` | ported | Tables identical. |
| `src/terminal/sgr.zig` | `crates/rustty-vt/src/sgr.rs`, `crates/rustty-vt-c/src/sgr.rs` | ported | All 33 tests. |
| `src/terminal/size.zig` | `crates/rustty-render-types/src/term_size.rs` | ported | Hoisted (A1) and re-exported as `rustty_vt::size`. |
| `src/terminal/size_report.zig` | `crates/rustty-vt/src/size_report.rs` | ported | All 5 tests byte-identical. |
| `src/terminal/stream.zig:1-5352` (3 parts) | `crates/rustty-vt/src/stream.rs` | ported | Comptime `vt()` union → `Handler` trait; all CSI/OSC/ESC arms and all tests. |
| `src/terminal/stream_continuation.zig` | `crates/rustty-vt/src/stream_continuation.rs` | ported | All 8 tests; FailingAllocator → tripwire. |
| `src/terminal/stream_terminal.zig:1-6538` (3 parts) | `crates/rustty-vt/src/stream_terminal.rs` | ported | All handler arms + all 63+8 tests. |
| `src/terminal/style.zig` | `crates/rustty-vt/src/style.rs` | ported | All 38 tests; only debug `format` text differs (refuted). |
| `src/terminal/sys.zig` | `crates/rustty-vt/src/sys.rs`, `crates/rustty-os/src/file.rs`, `crates/rustty-vt-c/src/sys.rs` | ported | decode_png/random_secure seams. |
| `src/terminal/tmux.zig` | `crates/rustty-vt/src/tmux/mod.rs` | ported | Submodule hub. |
| `src/terminal/x11_color.zig` | `crates/rustty-vt/src/x11_color.rs` + `res/rgb.txt` | ported | rgb.txt byte-identical. |
| `src/terminal/apc/glyph.zig` | `crates/rustty-vt/src/apc/glyph/mod.rs` | ported | — |
| `src/terminal/apc/glyph/Glossary.zig` | `.../glyph/glossary.rs` | ported | All 11 tests. |
| `src/terminal/apc/glyph/execute.zig` | `.../glyph/execute.rs` | ported | All 12 tests. |
| `src/terminal/apc/glyph/request.zig` | `.../glyph/request.rs` | ported | All 19 tests; `_` separator divergence (S170-F1). |
| `src/terminal/apc/glyph/response.zig` | `.../glyph/response.rs` | ported | All 12 tests. |
| `src/terminal/c/allocator.zig` | `crates/rustty-vt-c/src/allocator.rs` | ported | All 6 tests + 1 extra. |
| `src/terminal/c/build_info.zig` | `crates/rustty-vt-c/src/build_info.rs` | ported | All 11 tests. |
| `src/terminal/c/cell.zig` | `crates/rustty-vt-c/src/cell.rs` | ported | All 8 tests. |
| `src/terminal/c/color.zig` | `crates/rustty-vt-c/src/color.rs` | ported | All 22 tests. |
| `src/terminal/c/color_scheme.zig` | `crates/rustty-vt-c/src/color_scheme.rs` | ported | All 5 tests. |
| `src/terminal/c/focus.zig` | `crates/rustty-vt-c/src/focus.rs` | ported | All 4 tests. |
| `src/terminal/c/formatter.zig` | `crates/rustty-vt-c/src/formatter.rs` | ported | All 13 tests; zero-length alloc path differs (S171-F1). |
| `src/terminal/c/grid_ref.zig` | `crates/rustty-vt-c/src/grid_ref.rs` | ported | All 11 tests. |
| `src/terminal/c/grid_ref_tracked.zig` | `crates/rustty-vt-c/src/grid_ref_tracked.rs` | ported | All 4 tests. |
| `src/terminal/c/io.zig` | `crates/rustty-vt-c/src/io.rs`, `crates/rustty-vt/src/snapshot/io.rs` | ported | All 11 tests; zero-length `stream_to` guard absent (refuted). |
| `src/terminal/c/key_encode.zig` | `crates/rustty-vt-c/src/key_encode.rs` | ported | All 8 tests; warnings bypass log sink (S172-F1). |
| `src/terminal/c/key_event.zig` | `crates/rustty-vt-c/src/key_event.rs` | ported | All 3 tests. |
| `src/terminal/c/kitty_graphics.zig` | `crates/rustty-vt-c/src/kitty_graphics.rs` | ported | All 30 tests + 2 D11 tests. |
| `src/terminal/c/main.zig` | `crates/rustty-vt-c/src/lib.rs` | ported | All 198 exports set-differenced with no gaps. |
| `src/terminal/c/modes.zig` | `crates/rustty-vt-c/src/modes.rs` | ported | All 7 tests. |
| `src/terminal/c/mouse_encode.zig` | `crates/rustty-vt-c/src/mouse_encode.rs` | ported | All 7 tests; warnings bypass log sink (S174-F1). |
| `src/terminal/c/mouse_event.zig` | `crates/rustty-vt-c/src/mouse_event.rs` | ported | All 3 tests. |
| `src/terminal/c/osc.zig` | `crates/rustty-vt-c/src/osc.rs` | partial | Parser backed by the global allocator (S174-F3) + stderr warn (S174-F2). |
| `src/terminal/c/paste.zig` | `crates/rustty-vt-c/src/paste.rs` | partial | Out-of-range clipboard location rejected (S174-F4); mimes OOM path dropped (S174-F5). |
| `src/terminal/c/render.zig` | `crates/rustty-vt-c/src/render.rs` | ported | All 46 tests + 12 extra; caller allocator not threaded (S175-F1). |
| `src/terminal/c/result.zig` | `crates/rustty-vt-c/src/result.rs` | ported | All 8 discriminants. |
| `src/terminal/c/row.zig` | `crates/rustty-vt-c/src/row.rs` | ported | All 5 tests + 1 extra. |
| `src/terminal/c/search.zig` | `crates/rustty-vt-c/src/search.rs` | partial | All 17 tests; engine internals on the global allocator (S176-F1). |
| `src/terminal/c/selection.zig` | `crates/rustty-vt-c/src/selection.rs` | ported | All 3 tests. |
| `src/terminal/c/selection_gesture.zig` | `crates/rustty-vt-c/src/selection_gesture.rs` | ported | All 16 tests + 6 extra; `instant_from_ns` totality (S177-F2). |
| `src/terminal/c/sgr.zig` | `crates/rustty-vt-c/src/sgr.rs` | ported | Both tests; overflow warn to stderr (S177-F1). |
| `src/terminal/c/size_report.zig` | `crates/rustty-vt-c/src/size_report.rs` | ported | All 4 tests + 1 extra. |
| `src/terminal/c/snapshot.zig` | `crates/rustty-vt-c/src/snapshot.rs` | ported | All 5 tests. |
| `src/terminal/c/style.zig` | `crates/rustty-vt-c/src/style.rs` | ported | Both comptime layout blocks reproduced as const asserts. |
| `src/terminal/c/sys.zig` | `crates/rustty-vt-c/src/sys.rs` | ported | All 8 tests; `log_stderr` differences (S178-F1/F2). |
| `src/terminal/c/terminal.zig:1-6247` (3 parts) | `crates/rustty-vt-c/src/terminal.rs` | ported | All decls + all 115 tests; allocator/limit deltas (S179-F1/F2/F3). |
| `src/terminal/c/types.zig` | `crates/rustty-vt-c/src/types.rs` | ported | 159 manifest rows; 4 stale enum-value rows (S182-F1..F4). |
| `src/terminal/c/unicode.zig` | `crates/rustty-vt-c/src/unicode.rs` | ported | All 8 tests. |
| `src/terminal/compress/Page.zig` | `crates/rustty-vt/src/compress/page.rs` | ported | All 4 tests + 1 extra. |
| `src/terminal/compress/lz4.zig` | `crates/rustty-vt/src/compress/lz4.rs` | ported | Byte-for-byte codec; all 10 tests. |
| `src/terminal/compress/lz4_differential.zig` | `crates/rustty-vt/src/compress/lz4_differential.rs` | ported | Both suites, identical budgets. |
| `src/terminal/kitty/clipboard.zig` | `crates/rustty-vt/src/kitty/clipboard.rs` | ported | Re-export hub + concrete ref-all-decls test. |
| `src/terminal/kitty/clipboard_command.zig` | `.../clipboard_command.rs` | ported | All 19 tests. |
| `src/terminal/kitty/clipboard_grants.zig` | `.../clipboard_grants.rs` | ported | All 8 tests. |
| `src/terminal/kitty/clipboard_response.zig` | `.../clipboard_response.rs` | ported | All 15 tests, byte-identical strings. |
| `src/terminal/kitty/clipboard_write.zig` | `.../clipboard_write.rs` | ported | All 20 tests. |
| `src/terminal/kitty/color.zig` | `crates/rustty-vt/src/kitty/color.rs` | ported | — |
| `src/terminal/kitty/dnd.zig` | `crates/rustty-vt/src/kitty/dnd.rs` | ported | Re-export hub. |
| `src/terminal/kitty/dnd_command.zig` | `.../dnd_command.rs` | ported | All 11 tests. |
| `src/terminal/kitty/dnd_drop.zig` | `.../dnd_drop.rs` | ported | All 13 event arms. |
| `src/terminal/kitty/dnd_response.zig` | `.../dnd_response.rs` | ported | All 6 tests. |
| `src/terminal/kitty/dnd_test.zig` | `.../dnd_test.rs` | ported | All 21 harness tests. |
| `src/terminal/kitty/graphics.zig` | `crates/rustty-vt/src/kitty/graphics.rs` | ported | Stale deferral comment (S185-F1). |
| `src/terminal/kitty/graphics_animation.zig` | `.../graphics_animation.rs` | ported | — |
| `src/terminal/kitty/graphics_command.zig` | `.../graphics_command.rs` | ported | All 38 tests; `_` separator divergence (S186-F1). |
| `src/terminal/kitty/graphics_exec.zig` (2 parts) | `.../graphics_exec.rs` | ported | All 68 tests. |
| `src/terminal/kitty/graphics_image.zig` | `.../graphics_image.rs` | ported | 28/29 tests (S189-F1); readFile truncation (S189-F2). |
| `src/terminal/kitty/graphics_pixel.zig` | `.../graphics_pixel.rs` | ported | All 6 tests + differential KAT. |
| `src/terminal/kitty/graphics_render.zig` | `.../graphics_render.rs` | ported | — |
| `src/terminal/kitty/graphics_storage.zig` (3 parts) | `.../graphics_storage.rs` | ported | All decls + all 71 tests. |
| `src/terminal/kitty/graphics_unicode.zig` | `.../graphics_unicode.rs` | ported | 295-entry diacritics table verified. |
| `src/terminal/kitty/key.zig` | `crates/rustty-vt/src/kitty/key.rs` | ported | All 5 tests. |
| `src/terminal/osc/encoding.zig` | `crates/rustty-vt/src/osc/encoding.rs` | ported | Test input weakened (S194-F4). |
| `src/terminal/osc/kitty_metadata.zig` | `.../osc/kitty_metadata.rs` | ported | Both tests. |
| `src/terminal/osc/parsers.zig` | `.../osc/parsers/mod.rs` | ported | 17 submodules. |
| `src/terminal/osc/parsers/change_window_icon.zig` | `.../change_window_icon.rs` | ported | — |
| `src/terminal/osc/parsers/change_window_title.zig` | `.../change_window_title.rs` | ported | All 6 tests. |
| `src/terminal/osc/parsers/clipboard_operation.zig` | `.../clipboard_operation.rs` | ported | Allocator test weakened (S194-F2). |
| `src/terminal/osc/parsers/color.zig` | `.../color.rs` | ported | All 11 tests + 3 extra; `_` separator (S194-F3). |
| `src/terminal/osc/parsers/context_signal.zig` | `.../context_signal.rs` | ported | All 18 tests. |
| `src/terminal/osc/parsers/hyperlink.zig` | `.../hyperlink.rs` | ported | All 8 tests. |
| `src/terminal/osc/parsers/iterm2.zig` | `.../iterm2.rs` | ported | All 18 tests. |
| `src/terminal/osc/parsers/kitty_clipboard_protocol.zig` | `.../kitty_clipboard_protocol.rs` | partial | All 27 tests; `Option_::read` hand-rolled scan (S195-F1). |
| `src/terminal/osc/parsers/kitty_color.zig` | `.../kitty_color.rs` | ported | All 5 tests + 2 extra. |
| `src/terminal/osc/parsers/kitty_desktop_notification.zig` | `.../kitty_desktop_notification.rs` | ported | All 57 tests. |
| `src/terminal/osc/parsers/kitty_dnd_protocol.zig` | `.../kitty_dnd_protocol.rs` | ported | All 5 tests. |
| `src/terminal/osc/parsers/kitty_text_sizing.zig` | `.../kitty_text_sizing.rs` | partial | All 8 tests; `_` separator rejected (S195-F2). |
| `src/terminal/osc/parsers/mouse_shape.zig` | `.../mouse_shape.rs` | ported | — |
| `src/terminal/osc/parsers/osc9.zig` | `.../osc9.rs` | ported | All 61 tests. |
| `src/terminal/osc/parsers/report_pwd.zig` | `.../report_pwd.rs` | ported | Both tests. |
| `src/terminal/osc/parsers/rxvt_extension.zig` | `.../rxvt_extension.rs` | ported | Warnings dropped (S196-F1). |
| `src/terminal/osc/parsers/semantic_prompt.zig` | `.../semantic_prompt.rs` | ported | All 64 tests; exit-code `_` separator (S197-F1). |
| `src/terminal/search/Thread.zig` | `crates/rustty-vt/src/search/thread.rs` | ported | xev seam timer re-arm defect (S198-F1). |
| `src/terminal/search/active.zig` | `crates/rustty-vt/src/search/active.rs` | ported | Both tests + 1 extra. |
| `src/terminal/search/pagelist.zig` | `crates/rustty-vt/src/search/pagelist.rs` | ported | All 9 tests. |
| `src/terminal/search/screen.zig` | `crates/rustty-vt/src/search/screen.rs` | ported | All 23 tests + 1 extra. |
| `src/terminal/search/sliding_window.zig` | `crates/rustty-vt/src/search/sliding_window.rs` | ported | All 26 tests + 2 extra. |
| `src/terminal/search/terminal.zig` | `crates/rustty-vt/src/search/terminal.rs` | ported | All 6 tests. |
| `src/terminal/search/viewport.zig` | `crates/rustty-vt/src/search/viewport.rs` (+ `tests/viewport_fingerprint_alloc.rs`) | ported | All 5 tests + the FailingAllocator test as an integration test. |
| `src/terminal/snapshot/checkpoint.zig` | `crates/rustty-vt/src/snapshot/checkpoint.rs` | ported | All 4 tests; golden fixture byte-identical. |
| `src/terminal/snapshot/continuation.zig` | `.../continuation.rs` | ported | All 5 tests; 7 goldens byte-identical. |
| `src/terminal/snapshot/envelope.zig` | `.../envelope.rs` | ported | All 3 tests. |
| `src/terminal/snapshot/fixture.zig` | `.../fixture.rs` | ported | Grammar + formatting identical. |
| `src/terminal/snapshot/grid.zig` | `.../grid.rs` | ported | All 13 tests; grid-v1.hex byte-identical. |
| `src/terminal/snapshot/history.zig` | `.../history.rs` | ported | All 4 tests. |
| `src/terminal/snapshot/hyperlink.zig` | `.../hyperlink.rs` | ported | All 7 tests. |
| `src/terminal/snapshot/io.zig` | `.../io.rs` | ported | Round-trip test + std.Io stand-in traits. |
| `src/terminal/snapshot/main.zig` | `.../mod.rs` | ported | All 11 submodules + re-exports. |
| `src/terminal/snapshot/page.zig` | `.../page.rs` | ported | All 19 tests; 3 goldens byte-identical; decode OOM path (S204-F1). |
| `src/terminal/snapshot/record.zig` | `.../record.rs` | ported | All 8 tests. |
| `src/terminal/snapshot/screen.zig` | `.../screen.rs` | ported | All 22 tests + 2 comptime-assert stand-ins. |
| `src/terminal/snapshot/snapshot.zig` | `.../codec.rs`, `.../mod.rs` | ported | All 15 tests; OOM sweep weakened (S206-F1). |
| `src/terminal/snapshot/style.zig` | `.../style.rs` | ported | All 6 tests. |
| `src/terminal/snapshot/terminal.zig` | `.../terminal.rs` | ported | All 12 tests + comptime-len test; alt-screen kitty limit (S207-F1). |
| `src/terminal/tmux/control.zig` | `crates/rustty-vt/src/tmux/control.rs` | ported | All 18 tests (8 oniguruma-gated, mirroring `build_options`). |
| `src/terminal/tmux/layout.zig` | `crates/rustty-vt/src/tmux/layout.rs` | ported | All 40 tests. |
| `src/terminal/tmux/output.zig` | `crates/rustty-vt/src/tmux/output.rs` | ported | All 47 tests; delimiter byte-vs-char (S208-F2). |
| `src/terminal/tmux/viewer.zig` | `crates/rustty-vt/src/tmux/viewer.rs` | ported | All 9 tests; dead `nextIdle` intentionally dropped. |

**Partially-read units: none.** Every shard reported an empty `units_partial` list; each oracle file was read end-to-end (multi-part files were split on line boundaries and the straddling declaration/test was always read past the boundary to completion). The `partial` rows above are *counterpart* statuses — the unit was fully read and fully compared, but at least one declaration or test in it diverges (each such row cites the finding id).

## Findings

Status badges: **CONFIRMED** = both verifier lenses failed to refute; **DISPUTED** = one lens refuted (its verbatim reason is quoted). Severities are the corrected severities agreed by the verifiers.

### High

#### S151-F1 — codepoint_map is discarded when Terminal/Screen/PageList formatters delegate downward (clipboard-codepoint-map has no effect)
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** high
- **Oracle:** `src/terminal/formatter.zig:486` (TerminalFormatter → ScreenFormatter.init(..., self.opts)), `:691-698` (ScreenFormatter → PageListFormatter.init(&self.screen.pages, self.opts)), `:899` (PageListFormatter → PageFormatter.init(chunk.node.page(), self.opts))
- **Rustty:** `crates/rustty-vt/src/formatter.rs:766-767, 1000-1001, 1310`, helper `copy_options` at `1240-1250`
- **Description:** In the oracle every formatter passes `self.opts` by value to the child formatter, so `Options.codepoint_map` reaches `PageFormatter` where it is applied (`cp_map_empty` / `writeCodepoint` replacement). In rustty each hop constructs the child with `copy_options(&self.opts)`, and `copy_options` hard-codes `codepoint_map: Some(Vec::new())`. Consequently a `ScreenFormatter` (or `TerminalFormatter`/`PageListFormatter`) built with a non-empty codepoint map formats content with NO replacements, and the fast run path is taken instead. This is exactly the production path: `Surface.copySelectionToClipboards` (oracle `src/Surface.zig:2241`; rustty `crates/rustty-core/src/surface.rs:5500`) builds a `ScreenFormatter` with `codepoint_map = config.clipboard_codepoint_map`, so the user-facing `clipboard-codepoint-map` config option is silently ignored in rustty. The oracle test suite only exercises the map at Page level, so no ported test catches it.
- **Evidence:** `formatter.rs:1240-1250` `fn copy_options(opts)` returns `codepoint_map: Some(Vec::new())` with the comment "codepoint_map does not affect extras emission" — but it is also used at the content hops: `formatter.rs:767` `ScreenFormatter::init(terminal.screens.active, copy_options(&self.opts))`, `:1001` `PageListFormatter::init(&screen.pages, copy_options(&self.opts))`, `:1310` `PageFormatter::init(page, copy_options(&self.opts))`. rustty-core `surface.rs:14512-14523` even has its own `copy_options` that KEEPS the map, noting the vt one is "extras-only" — yet the vt one is used on the content path.
- **Suggested fix:** At the three content-delegation sites pass an Options copy that clones `codepoint_map` (e.g. `clone_options`), keeping the map-less copy only for the discarding extras passes; add a Screen/Terminal-level test asserting replacement happens through `ScreenFormatter`.

#### S198-F1 — Search refresh timer is one-shot in rustty: the callback's re-arm is immediately cancelled by apply_action(Disarm)
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** high
- **Oracle:** `src/terminal/search/Thread.zig:468-499` (`refreshCallback`), `:415-433` (`startRefreshTimer`)
- **Rustty:** `crates/rustty-vt/src/search/thread.rs:499-503, 946-989, 1386-1406`
- **Description:** The oracle `refreshCallback` re-runs the timer with `self.refresh.run(&self.loop, &self.refresh_c, REFRESH_INTERVAL, ...)` and then returns `.disarm`. In libxev `.disarm` only means "the loop does not itself re-add this completion"; the explicit `timer.run` inside the callback has already re-submitted it, so the refresh timer loops forever at REFRESH_INTERVAL (24 ms / 40 FPS). In the rustty xev seam the dispatcher does `let action = self.refresh_callback(); self.loop_.apply_action(&mut self.refresh_c, action);`, and `apply_action(Disarm)` for a completion whose `callback == Some(Callback::Refresh)` sets `c.active = false` AND `shared.timer_armed.store(false)` — undoing the re-arm that just happened. `start_refresh_timer` is only called once (`thread.rs:349`), so the refresh timer fires exactly once, ~24 ms after thread start, and never again.
- **Evidence:** `thread.rs:946-989` re-arms via `self.refresh.run(&self.loop_, &mut self.refresh_c, REFRESH_INTERVAL, xev::Callback::Refresh)` then returns `Disarm`; `thread.rs:499-503` applies that action; `thread.rs:1398-1402` `Some(Callback::Refresh) => self.shared.timer_armed.store(false, Ordering::SeqCst)`. Net effect: the periodic `feed_locked` that re-scans the active area / viewport stops running, so an active search's match count, viewport matches and selected match go stale as the terminal content scrolls, whereas the oracle re-feeds every 24 ms.
- **Suggested fix:** Make `apply_action` not clobber a re-arm performed inside the callback — snapshot the completion's armed state before invoking the callback and only clear it on Disarm if the callback did not re-submit (mirroring libxev), or have `Timer::run` mark the completion as "re-submitted this dispatch".

### Medium

#### S127-F1 — clone test compares only line-limit fields instead of the whole Limits struct
- **Status:** CONFIRMED · **Kind:** weakened-assertion · **Severity:** medium
- **Oracle:** `src/terminal/PageList.zig:11390-11395`
- **Rustty:** `crates/rustty-vt/src/pagelist/tests.rs:6969-6978`
- **Description:** The oracle asserts `expectEqual(s.limits, cloned.limits)`, i.e. equality of ALL four fields (bytes.explicit, bytes.min, lines.explicit, lines.min). The rustty counterpart asserts only `lines.explicit`, `lines.min` and `max(Key::Lines)`; it never checks that `bytes.explicit`/`bytes.min` survive `clone`. `Limits` derives `PartialEq`/`Eq` (`pagelist/mod.rs:1474-1478`), so a whole-struct assert is directly expressible. A regression dropping or recomputing the byte limit in `PageList::clone` would pass rustty and fail the oracle.
- **Evidence:** oracle `:11395` `try testing.expectEqual(s.limits, cloned.limits);` vs `tests.rs:6976-6978` three line-only asserts, no `bytes.*` comparison.
- **Suggested fix:** Replace the three asserts with `assert_eq!(s.limits, cloned.limits);`.

#### S131-F1 — reset-invalidates-stale-refs test drops the node-address-reuse loop and both nodeIsValid assertions
- **Status:** CONFIRMED · **Kind:** weakened-assertion · **Severity:** medium
- **Oracle:** `src/terminal/PageList.zig:18921-18967`
- **Rustty:** `crates/rustty-vt/src/pagelist/tests.rs:1693-1719`
- **Description:** The oracle repeatedly records (first node ptr, serial) and calls `reset()` until `s.pages.first` comes back at a previously recorded address (bounded by `page_preheat*4`), then asserts: reuse happened, `old_node == new_node`, `old_serial < page_serial_epoch`, `!s.nodeIsValid(old_node, old_serial)`, `s.nodeIsValid(new_node, new_serial)` and the new-serial range. The rustty port performs a single `reset()` and asserts only the serial-range facts. It never drives address reuse, never asserts pointer identity, and never calls `node_is_valid` — so the property the test is named for (a stale serial is rejected even when the exact node address is live again) is not exercised. `PageList::node_is_valid` (`pagelist/mod.rs:2878`) and `PAGE_PREHEAT` (`mod.rs:128`) both exist, so the assertions are portable as-is.
- **Evidence:** oracle `:18928-18951` loop + `:18953/18961/18963/18964` asserts vs rustty `1699-1719` (single `reset()` at 1706; only five serial-range asserts; grep for `node_is_valid` in the region returns nothing).
- **Suggested fix:** Port the loop faithfully with up to `PAGE_PREHEAT*4` (node ptr, serial) pairs and all six assertions.

#### S133-F1 — Screen.clearCells does not clear the row's kitty_virtual_placeholder flag
- **Status:** CONFIRMED · **Kind:** missing-branch · **Severity:** medium
- **Oracle:** `src/terminal/Screen.zig:1818-1828`
- **Rustty:** `crates/rustty-vt/src/screen.rs:2739-2742`
- **Description:** The oracle's `Screen.clearCells`, when kitty graphics is enabled and a whole row (`cells.len == page.size.cols`) is cleared on a row with `kitty_virtual_placeholder` set, scans the cells for the kitty placeholder codepoint and, if none is found, clears the row flag. The Rust `Screen::clear_cells` replaces that block with a PORT NOTE saying the logic "lands with the kitty co-port (vt-12)", but it was never added: the row flag is left true after a full-row clear. The equivalent block DOES exist in `Page::clear_cells` (`page_impl.rs:1975-1990`), but `Screen::clear_cells` is a separate function and is what `cursorDownScroll` / `cursorScrollAbove` / `cursorScrollRegionUp` / `clearRows` / `eraseDisplay` call. The flag is observable through the C API (`rustty-vt-c/src/row.rs:257`) and gates whether the renderer scans a row for Unicode placements (`kitty/graphics_unicode.rs:51`).
- **Evidence:** oracle `:1818-1828` `if (comptime build_options.kitty_graphics) { if (row.kitty_virtual_placeholder and cells.len == page.size.cols) { ... } }` vs `screen.rs:2739-2742` — only the PORT NOTE comment, then `let blank = self.blank_cell();`; grep for `kitty_virtual_placeholder` in `screen.rs` hits only that comment.
- **Suggested fix:** Add the flag-clearing block after the styled block in `Screen::clear_cells` (mirror `page_impl.rs:1975-1990`) and delete the stale deferral note.

#### S133-F2 — Screen.Options.default uses kitty_image_storage_limit = 0 instead of 320MB (ghostty) / 10MB (lib)
- **Status:** CONFIRMED · **Kind:** different-constant · **Severity:** medium
- **Oracle:** `src/terminal/Screen.zig:273-293`
- **Rustty:** `crates/rustty-vt/src/screen.rs:400-423`
- **Description:** In the oracle `Options.default` overrides only cols/rows/max_scrollback_bytes, so `kitty_image_storage_limit` takes its struct default (`320 * 1000 * 1000` for `.ghostty`, `10 * 1000 * 1000` for `.lib`). The Rust `Options::default()` hard-codes `kitty_image_storage_limit: 0`, which `ImageStorage::set_limit` treats as a zero-byte image budget (every kitty image load is rejected/evicted). The PORT NOTE justifying it ("kitty graphics is DEFERRED to vt-12") is stale: kitty graphics is fully ported and `Screen::init` passes the value to `images.set_limit`. Any caller using `screen::Options::default()` (e.g. `screen_set.rs:181-227`) gets a screen that cannot store kitty images.
- **Evidence:** oracle `:275-278` artifact switch and `:289-293` `pub const default` vs `screen.rs:415-422` `kitty_image_storage_limit: 0` with the stale note at `:410-414`; `graphics_storage.rs:348-360` special-cases limit 0 as "disable by deleting all".
- **Suggested fix:** Set the artifact default (320 MB) in `Options::default()` and remove the stale vt-12 note.

#### S133-F3 — cursorCopy does not restore the old cursor when manualStyleUpdate fails; error set collapsed to OutOfMemory
- **Status:** CONFIRMED · **Kind:** missing-error-path · **Severity:** medium
- **Oracle:** `src/terminal/Screen.zig:1430-1451`
- **Rustty:** `crates/rustty-vt/src/screen.rs:2341-2380`
- **Description:** The oracle does `const old = self.cursor; self.cursor = other; errdefer self.cursor = old;` then `try self.manualStyleUpdate();`. If `manualStyleUpdate` fails (error set `IncreaseCapacityError = OutOfMemory || OutOfSpace`), the errdefer restores the ENTIRE old cursor and the caller receives the original error. The Rust port overwrites `self.cursor = new_cursor` and then `self.manual_style_update().map_err(|_| OutOfMemory)?` — on failure `self.cursor` is left as a half-applied copy of `other` (other's style/pending_wrap/protected/semantic_content with the old style_id/pin/x/y), and OutOfSpace is reported as OutOfMemory. The PORT NOTE claiming the errdefer is "not reachable" is wrong: `manual_style_update` returns `pagelist::grow::IncreaseCapacityError` with both variants.
- **Evidence:** oracle `:1430-1432` + `:1451` vs `screen.rs:2376` `self.cursor = new_cursor;` then `:2380` `self.manual_style_update().map_err(|_| OutOfMemory)?;`.
- **Suggested fix:** Capture `let old = self.cursor;` (Cursor is Copy), restore it on `Err(e)` and return `e`; widen the return type to `IncreaseCapacityError`.

#### S141-F1 — libghostty-vt default kitty_image_storage_limit is 320MB instead of the .lib artifact's 10MB
- **Status:** CONFIRMED · **Kind:** different-constant · **Severity:** medium
- **Oracle:** `src/terminal/Terminal.zig:289-297` (default consumed by `src/terminal/c/terminal.zig:880-886`)
- **Rustty:** `crates/rustty-vt-c/src/terminal.rs:1547-1560`
- **Description:** `Terminal.Options.kitty_image_storage_limit` defaults to 320 MB for `.ghostty` but 10 MB for `.lib` (libghostty-vt). The oracle C API calls `.init(io, alloc, .{ .cols, .rows })` and relies on that default, so embedders get 10 MB. rustty-vt has no `Default` for `Options`, so rustty-vt-c spells the defaults out by hand and picks the ghostty-artifact value. `ghostty_terminal_new` in rustty therefore allows 32× more kitty image storage than the reference library.
- **Evidence:** oracle `Terminal.zig:290-297` artifact switch; `c/terminal.zig` passes only cols/rows; `rustty-vt-c/src/terminal.rs:1560` `kitty_image_storage_limit: 320 * 1000 * 1000,` with the PORT NOTE at `1552-1555`. A workspace grep for `10 * 1000 * 1000` finds no lib-artifact constant anywhere.
- **Suggested fix:** Add an `Options` constructor in rustty-vt carrying the reference field defaults and use `10 * 1000 * 1000` for the lib artifact in rustty-vt-c.

#### S143-F1 — resize failure-path test compares only subsets of flags, tabstops and cursor
- **Status:** CONFIRMED · **Kind:** weakened-assertion · **Severity:** medium
- **Oracle:** `src/terminal/Terminal.zig:4528-4569`
- **Rustty:** `crates/rustty-vt/src/terminal.rs:19136-19306`
- **Description:** The oracle's restoration arms assert whole-struct equality: `expectEqual(before.flags, t.flags)` (every Flags field), `std.meta.eql(before.tabstops, t.tabstops)` (the entire tabstop table) and `std.meta.eql(before_primary.cursor, primary.cursor)` / `before_alternate.cursor` (the full Cursor including pending_wrap, style, hyperlink state, page_pin). The rustty port snapshots and compares only `flags.dirty.clear`, the two tabstop bits at columns 8 and 3, and the cursor `(x, y)` pair. A rollback regression clearing another Flags field, disturbing another tabstop, or altering `cursor.pending_wrap`/style would pass rustty and fail the oracle. (The port's PORT NOTE claiming it snapshots "exactly the fields the reference compares" is inaccurate.)
- **Evidence:** oracle `:4538/4539/4551/4564` whole-struct asserts vs rustty `19149-19151`, `19161`, and the assertions at `19272-19274`, `19282-19285`, `19296-19299`.
- **Suggested fix:** Snapshot `t.flags` (derive PartialEq), the full tabstop table (or all `get(i)`), and the whole `Cursor` value before/after the failed resize.

#### S149-F1 — apc Handler does not gate the glyph protocol on build_options.glyph_protocol
- **Status:** CONFIRMED · **Kind:** missing-branch · **Severity:** medium
- **Oracle:** `src/terminal/apc.zig:84-97, 134-140, 200-207, 237-246, 280-283, 294-297, 661, 727`
- **Rustty:** `crates/rustty-vt/src/apc/mod.rs:174-191, 255-261, 355-362, 427-433, 477, 502, 1069-1086, 1152-1163`
- **Description:** The oracle wraps every glyph arm in `if (comptime build_options.glyph_protocol)`: with the feature off the `;`-identify `25a1` path goes to `.ignore` (no glyph CommandParser allocated, no bytes buffered up to the 1 MiB glyph limit, `end()` returns null), and the `State.glyph`/`Command.glyph` payloads collapse to `void` with `unreachable` guards. The rustty port has no `options::GLYPH_PROTOCOL` check anywhere in `apc/mod.rs`: with `--no-default-features` it still instantiates `glyph::CommandParser`, buffers up to `max_bytes[Glyph]`, parses, and `Handler::end` returns `Some(Command::Glyph(..))`; only `stream_terminal.rs:2760` then discards it. The kitty arms in the same file DO carry the equivalent `KITTY_GRAPHICS` gate, so this is an asymmetry within one file. The two tests the oracle guards with `SkipZigTest` (`apc.zig:661`, `:727`) also lack the corresponding `if !GLYPH_PROTOCOL { return; }` skip that the kitty tests were given.
- **Evidence:** oracle `:84-97` vs `apc/mod.rs:178-185` (no GLYPH_PROTOCOL test); `grep -n GLYPH_PROTOCOL crates/rustty-vt/src/apc/mod.rs` returns nothing; the const exists (`options.rs:163`, a real optional feature) and is honoured elsewhere.
- **Suggested fix:** Mirror the kitty gating in feed / feed_slice / end / `State::deinit` / `Command::deinit`, and add the two test skips.

#### S150-F1 — test "RGB.parse" port drops 6 of the reference assertions
- **Status:** CONFIRMED · **Kind:** weakened-assertion · **Severity:** medium
- **Oracle:** `src/terminal/color.zig:927-976`
- **Rustty:** `crates/rustty-vt/src/color.rs:1930-2044` (+ `1798-1839`)
- **Description:** The reference asserts 38 cases; the rustty `rgb_parse` (plus the split-off `rgb_parse_bare_hex_and_trim`) omits six: the tab-trimmed X11 name `"\tForestGreen\t"` and the InvalidFormat cases `""`, `"  "`, `"#12345"`, `"12345"`, `"nosuchcolor"`. These exercise the post-trim empty early return, the 6-char and 5-char length arms and the final `rgb` prefix rejection — all now untested at the `RGB::parse` level (the vt-c layer covers `""`/`"#12345"` indirectly; `"\tForestGreen\t"`, `"  "` and `"12345"` are covered nowhere).
- **Evidence:** oracle `:953` and `:956-957`, `:973-975`; rustty `color.rs:1930-2044` has none of the six; `grep nosuchcolor crates/rustty-vt/src/color.rs` → no match.
- **Suggested fix:** Add the six assertions to `rgb_parse`.

#### S150-F2 — clipboard.Location is a closed enum; reference is non-exhaustive enum(c_int){_}
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/clipboard.zig:4-9`
- **Rustty:** `crates/rustty-vt/src/clipboard.rs:14-23`
- **Description:** The reference `Location = enum(c_int) { standard, selection, primary, _ }` is open, so an out-of-range `c_int` from an embedder is representable and passes through (paste events report an unknown location as non-standard). rustty's `#[repr(i32)] enum Location` has no open arm, so C-boundary code must narrow: `rustty-vt-c/src/paste.rs:167-172` returns `Result::InvalidValue` for anything other than 0/1/2 where the oracle would accept and forward it. The vt-c PORT NOTE records this as the crate-wide D11 closed-enum policy, but the sibling `Write.Status`/`Read.Status` types in the same file WERE ported as open `#[repr(transparent)]` i32 newtypes, so the treatment within the file is inconsistent.
- **Evidence:** oracle `:4-9` vs `clipboard.rs:17-23` (no `Other`), `paste.rs:169-172` `_ => return Result::InvalidValue`; `clipboard.rs:187-201`/`312-323` are open newtypes.
- **Suggested fix:** Model `Location` like `WriteStatus`/`ReadStatus` (a transparent i32 newtype with associated consts), or accept and document the D11 divergence next to the type.

#### S156-F1 — ghostty.h ProgressReport.State check never reads the C header (tautological lookup)
- **Status:** CONFIRMED · **Kind:** weakened-assertion · **Severity:** medium
- **Oracle:** `src/terminal/osc.zig:249-253`
- **Rustty:** `crates/rustty-vt/src/osc/mod.rs:406-434`
- **Description:** The oracle test calls `lib.checkGhosttyHEnum(CState, "GHOSTTY_PROGRESS_STATE_")`, which reflects over the REAL generated `ghostty.h` and fails if any declaration is missing, renamed, revalued or reordered. The rustty counterpart passes `check_ghostty_henum` a lookup closure that resolves each `RUSTTY_PROGRESS_STATE_<KEY>` from the descriptor's OWN `t.fields`, so the header (`include/rustty-embed.h:899-905`) is never consulted and the check can never fail. Contrast `mouse.rs:277-297`, which ports the same idiom by parsing the header text and even has a probe-can-fail test.
- **Evidence:** `osc/mod.rs:421-433` builds the expected values from `state_enum_descriptor().fields`; `rustty-lib/src/enum.rs:124-164` only compares `c(expected_name)` against the descriptor value, so descriptor-vs-descriptor is always equal.
- **Suggested fix:** Reuse `mouse.rs`'s `parse_header_enum` (hoist it into rustty-lib) against `include/rustty-embed.h`, plus a probe-can-fail test.

#### S156-F2 — Shape.getGObjectType (GhosttyMouseShape GObject enum) stripped; GTK mouse-shape property is a plain int
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/mouse.zig:77-92` (+ `apprt/gtk/class/surface.zig:222-241`)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:247-257`
- **Description:** The oracle registers `Shape` as a GObject enum type named `GhosttyMouseShape` when the GTK runtime is selected, and declares the `mouse-shape` property with that GType (typed getter/setter, default `.text`). rustty strips `getGObjectType` from `mouse.rs` (PORT NOTE R3, "will be seamed in by the apprt crate") but the apprt crate does not seam it in: `surface.rs:255-257` declares `mouse-shape` as `Cell<i32>` with a PORT NOTE (concern) admitting the GType registration is not done. Observable through GObject introspection / blueprint bindings / `g_object_get`, and the property default differs too.
- **Evidence:** oracle `mouse.zig:78-92` `gobject.ext.defineEnum(Shape, .{ .name = "GhosttyMouseShape" })`; `grep -rn GhosttyMouseShape|RusttyMouseShape crates` finds only the PORT NOTE. The `glib::Enum` idiom is used elsewhere in the same crate (`gtk/class/gtypes.rs:38`), so this is a genuine gap.
- **Suggested fix:** Register a `RusttyMouseShape` glib enum type and type the `mouse-shape` property with it, matching `surface.zig:222-241`.

#### S170-F1 — Integer control values containing '_' digit separators are rejected where the reference accepts them
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/apc/glyph/request.zig:106, 217, 220-222, 372`
- **Rustty:** `crates/rustty-vt/src/apc/glyph/request.rs:437-499` (helpers at `452-465`, `480-499`)
- **Description:** `Parser.finishValue` delegates to `std.fmt.parseInt(i32|u32, buf, 10)`, which skips interior `_` separators (rejecting only a leading/trailing `_`), so the reference parses `s=1_0` as width 10 and `z=-1_0` as -10. The port's `parse_int_u32`/`parse_int_i32` delegate to Rust's `str::parse`, which reports `InvalidDigit` for any `_`, mapped to `Error::InvalidFormat`; since `finish_value` propagates that out of `Parser::complete`, the entire glyph command is dropped rather than parsed. The port's own doc comment acknowledges the deferral, and the negative-magnitude path WAS hand-ported to honour `_`, making the port internally inconsistent.
- **Evidence:** oracle `:321-322`; rustty `request.rs:433-441`, `452-465`, `480-499`; PORT NOTE at `517-521`; the negative-path test table pins `(b"-0_0", Ok(0))` but there is no non-negative `_` row.
- **Suggested fix:** Port Zig's `_` handling to the non-negative branches (skip interior `_`, reject leading/trailing) and add the table rows.

#### S171-F1 — format_alloc with empty output invokes a zero-length raw alloc callback and can return out_of_memory
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/c/formatter.zig:220-240` (with `std/Io/Writer.zig:2646-2665`)
- **Rustty:** `crates/rustty-vt-c/src/formatter.rs:578-626`
- **Description:** The oracle formats into `std.Io.Writer.Allocating` and returns `aw.toOwnedSlice()`. For empty output (reachable: a plain, trimmed format of a blank screen) `toOwnedSlice` returns `a.writer.buffer[0..0]` WITHOUT calling any allocator callback, so the C caller gets `.success` with `out_len = 0`. The rustty port formats into a Vec and then calls `allocator.alloc(len, ...)` — the RAW vtable alloc — even when `len == 0`. With a caller-supplied C allocator this (a) invokes the C alloc callback with size 0, which the oracle never does, and (b) if that callback returns NULL for a zero-size request (legal), maps to `Result::OutOfMemory` where the oracle returns success. The crate's own `alloc_bytes` helper (`rustty-lib/src/allocator.rs:88-101`) documents exactly this zero-length short-circuit and is used by the sibling port.
- **Evidence:** oracle `:236` `const buf = aw.toOwnedSlice() catch return .out_of_memory;`; rustty `formatter.rs:611-615` `let dst = allocator.alloc(len, Alignment::from_byte_units(1), 0); if dst.is_null() { return Result::OutOfMemory; }`.
- **Suggested fix:** Skip the allocator entirely for empty output (non-null dangling ptr, len 0, Success), or at minimum use `allocator.alloc_bytes(len)`.

#### S174-F1 — mouse_encode setopt warnings bypass the C API log sink (eprintln! instead of sys::log_fn)
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/c/mouse_encode.zig:110-171` (five `log.warn` sites)
- **Rustty:** `crates/rustty-vt-c/src/mouse_encode.rs:320, 355, 374, 395, 406`
- **Description:** Same class as S172-F1: the oracle's five setopt diagnostics use the `.mouse_encode` scoped logger, routed by `src/lib_vt.zig:456-460` into the embedder-registered log callback. `mouse_encode.rs` instead calls `eprintln!`, which unconditionally writes to process stderr and never reaches the embedder callback. `mouse_encode.rs` is the only file in the crate with zero `sys::log_fn` uses.
- **Evidence:** oracle `:112-115`, `:137-140`, `:148-151`, `:159-163`, `:168`; rustty `mouse_encode.rs:320` etc.; contrast `lib.rs:1013-1018` (`sys::log_fn(..., "mouse_event", ...)`) and `sys.rs:558-570` (`log_fn_c_abi` early-returns without a callback).
- **Suggested fix:** Route the five warnings through `sys::log_fn(sys::StdLevel::Warn, "mouse_encode", ...)` with identical text.

#### S174-F2 — osc commandData warning bypasses the C API log sink
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/c/osc.zig:72-78`
- **Rustty:** `crates/rustty-vt-c/src/osc.rs:235`
- **Description:** The oracle's `log.warn("commandData invalid data value={d}")` under `std.log.scoped(.osc)` is dispatched to the embedder's registered C log callback; `osc.rs` writes to stderr with `eprintln!` instead, so the message never reaches the embedder and is emitted even when no log callback is installed.
- **Evidence:** oracle `:73-77`; rustty `osc.rs:230-237` `_ => { eprintln!("commandData invalid data value={data}"); return false; }`; `grep log_fn osc.rs` → nothing.
- **Suggested fix:** Route through `sys::log_fn(sys::StdLevel::Warn, "osc", ...)`.

#### S174-F3 — C-created OSC parser does not use the caller-supplied allocator for its dynamic payloads
- **Status:** DISPUTED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/c/osc.zig:18-36`
- **Rustty:** `crates/rustty-vt-c/src/osc.rs:34, 66-89`
- **Description:** The oracle's `osc.new` resolves the caller allocator (`lib.alloc.default(alloc_)`) and initializes the parser with it, so all dynamic OSC growth (payloads longer than MAX_BUF, e.g. OSC 52 clipboard data) is allocated through the embedder's allocator, and `free` releases it via `parser.alloc.?`. rustty's `osc::new` initializes the parser with a process-static `GlobalZigAllocator` and only uses the C allocator for the wrapper struct itself, so an embedder supplying a custom allocator never sees the parser's dynamic allocations. The module PORT NOTE justifies this by citing `terminal::new_`, but that site was since fixed (F-008), so the justification is stale.
- **Evidence:** oracle `:19-27`, `:30-35`; rustty `osc.rs:34` `static GLOBAL_ZIG_ALLOCATOR`, `:76` `Parser::init(Some(ZigAllocatorRef::new(&GLOBAL_ZIG_ALLOCATOR)))`.
- **Refuting lens (presence):** "The claimed observable difference does not follow from the cited line. I read crates/rustty-vt/src/osc/mod.rs:891-953 and 700-780: the `alloc: Option<ZigAllocatorRef>` slot is used ONLY as a presence gate (`ensure_allocator` at :896, `if self.alloc.is_none()` at :935, the kitty-color deinit gate at :834-840); the allocating capture's backing is `Vec::with_capacity(...)` and its growth at :709-715 / :771-777 is `reserve_exact` on that Vec, i.e. the Rust GLOBAL heap. So a caller-supplied CAllocator would not receive the dynamic OSC payload allocations even after the suggested fix — and the terminal.rs F-008 path (terminal.rs:84-91) has exactly the same residual. Both `Some(global)` and `Some(caller)` are observably identical at the C boundary here, and the free path already destroys the wrapper through the caller's allocator (osc.rs:100-115), matching the oracle's `parser.alloc.?`. Any genuine gap lives in rustty-vt's Vec-backed capture, not in osc.rs, and is not what this finding describes."
- **Suggested fix:** Apply the terminal.rs F-008 pattern (heap-hold the resolved C allocator inside `OscParserWrapper`, build the parser's `ZigAllocatorRef` from it, reclaim it last in `free`) — and, per the refutation, make the vt-side allocating capture actually allocate through that slot.

#### S174-F4 — terminal_paste rejects an out-of-range clipboard location that the oracle accepts
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/c/paste.zig:70-73` (with `src/terminal/clipboard.zig:4-9`)
- **Rustty:** `crates/rustty-vt-c/src/paste.rs:159-175`
- **Description:** `clipboard.Location` is a NON-exhaustive `enum(c_int)` in the oracle, so a Request carrying any other location value is forwarded to the paste handler: the paste proceeds and the paste-event path reports the unknown location. rustty narrows `req.location` to a closed enum and returns `Result::InvalidValue` for anything outside 0..=2, so an embedder using a tolerated location value gets no paste at all. (The sibling narrowing of `req.source` is NOT a difference: `Source` is exhaustive in the oracle.)
- **Evidence:** oracle `clipboard.zig:4-9` + `paste.zig:70-73` (location forwarded unchecked) vs `paste.rs:160-170` `_ => return Result::InvalidValue`; documented at `paste.rs:150-158` (D11).
- **Suggested fix:** Model the C-boundary location as an open value (c_int newtype or `Unknown(c_int)`) threaded to the paste path.

#### S175-F1 — RenderState contents (update/begin_update/deinit) bypass the caller-supplied C allocator
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/c/render.zig:194-227`
- **Rustty:** `crates/rustty-vt-c/src/render.rs:630-720`
- **Description:** The oracle threads the C-resolved allocator (`state.alloc`) into every RenderState growth/teardown path: `free` → `state.state.deinit(alloc)`, `update` → `state.state.update(state.alloc, t)`, `begin_update` → `state.state.beginUpdate(state.alloc, t)`. rustty routes only the wrapper storage through the caller allocator and constructs/updates the RenderState with `allocator_api2::alloc::Global` (root cause: `crates/rustty-vt/src/render.rs:47` aliases `Allocator = Global`, so the parameter is a ZST). An embedder supplying a custom allocator observes almost none of the terminal's render allocations, and a rejecting allocator that should surface `RUSTTY_OUT_OF_MEMORY` for row growth cannot. Documented as an open deferral ("F-008/W12-01 Tier 2").
- **Evidence:** oracle `:197`, `:212`, `:223`; rustty `render.rs:643-646`, `671-672`, `704-705` (`let gpa = allocator_api2::alloc::Global;`) with the PORT NOTE at `640-643`.
- **Suggested fix:** Complete F-008 Tier 2 — parameterize `rustty_vt::render::RenderState` over the allocator so the three sites can pass `&*state.alloc`; extend the `CountingCAllocator` tests to assert caller alloc calls > 0 on update and live_bytes 0 after free.

#### S176-F1 — Search engine internals are allocated through the Rust global allocator instead of the embedder-supplied C allocator
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/c/search.zig:253-257`
- **Rustty:** `crates/rustty-vt-c/src/search.rs:682-685`
- **Description:** The oracle builds the `TerminalSearch` on the allocator resolved from the C caller (`wrapper.alloc`), so every allocation the search engine makes (match lists, viewport highlight cache, needle copy) goes through the embedder's callbacks and a failure is reported as `.out_of_memory`. The rustty port hard-codes `allocator_api2::alloc::Global` (and `rustty-vt/src/search/terminal.rs:31` aliases `Allocator = Global`, so it structurally cannot be threaded today), so the embedder's allocator never sees those allocations and OOM aborts instead of returning `RUSTTY_OUT_OF_MEMORY`. Only the wrapper block still goes through the C allocator. Tracked as the F-008/W12-01 Tier-2 residual.
- **Evidence:** oracle `:253-257`; rustty `search.rs:682` `TerminalSearch::init(allocator_api2::alloc::Global, bytes)`; the port's own comment at `search.rs:70-76` concedes "the reference passes this allocator to `TerminalSearch.init` as well".
- **Suggested fix:** Thread the wrapper's resolved C allocator into `TerminalSearch::init` via an allocator_api2 adapter over `ZigAllocator`.

#### S177-F1 — sgr set_params separator-overflow warning writes to stderr instead of the C log callback
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/c/sgr.zig:9, 68-74`
- **Rustty:** `crates/rustty-vt-c/src/sgr.rs:33, 584-591`
- **Description:** The reference declares `const log = std.log.scoped(.sgr)` and emits `log.warn("ghostty_sgr_set_params: separators length {} exceeds max supported length {}")` when `len > SepList.bit_length`; in the C library that reaches the host-installed log callback and is SILENT when none is installed. The rustty port calls `eprintln!` directly, so the host callback never receives the message and stderr is written unconditionally. `selection_gesture.rs` in the same shard does this correctly via `crate::sys::log_fn`, and `lib.rs:3116` states the port doctrine ("it never writes to stderr; `sys::log_fn` is that sink").
- **Evidence:** oracle `:9`, `:69-72`; rustty `sgr.rs:33` (logger only as a comment), `:586-588` `eprintln!(...)`; grep for the message text finds only that one call.
- **Suggested fix:** Replace with `crate::sys::log_fn(StdLevel::Warn, "sgr", format_args!(...))`, add a module `LOG_SCOPE`, and extend the lib.rs log-scope test with an `sgr` row.

#### S177-F2 — instant_from_ns can panic on a large time_ns value where the reference cannot
- **Status:** DISPUTED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/c/selection_gesture.zig:739-743`
- **Rustty:** `crates/rustty-vt-c/src/selection_gesture.rs:1758-1764`
- **Description:** The reference `instantFromNs(ns: u64) std.Io.Timestamp` is a pure value conversion (`.fromNanoseconds(ns)`) and is total over the whole u64 domain, so a C caller setting `..._EVENT_OPTION_TIME_NS` to any value simply produces some timestamp. The rustty port anchors a process-lifetime `std::time::Instant` base and computes `base + Duration::from_nanos(ns)`; `Instant`'s `Add<Duration>` panics ("overflow when adding duration to instant") when the result is not representable, so a near-`u64::MAX` `time_ns` through `rustty_selection_gesture_event_set` aborts the library instead of returning `.success`.
- **Evidence:** oracle `:741-743`; rustty `selection_gesture.rs:1758-1764`; reached from the `EventOption::TimeNs` arm (`selection_gesture.rs:1329-1336`) driven by the exported setter (`lib.rs:2846-2862`).
- **Refuting lens (presence):** "The claimed panic cannot occur. rustty crates/rustty-vt-c/src/selection_gesture.rs:1758-1764 computes `base + Duration::from_nanos(ns)`; `Duration::from_nanos(u64::MAX)` is ~18,446,744,073 seconds (~1.8e10), and on unix `Instant`'s add goes through `Timespec::checked_add_duration`, which checked-adds that into an i64 `tv_sec` whose base value is seconds-since-boot — i64::MAX is ~9.2e18, so no representable u64 `time_ns` can overflow it and the `Add` impl never reaches its panic path (rustty is macOS/Linux-first; there is no freestanding-wasm arm here). So `instant_from_ns` is total over the whole u64 domain, exactly like oracle selection_gesture.zig:741-743 `fn instantFromNs(ns: u64) std.Io.Timestamp { return .fromNanoseconds(ns); }`, and `rustty_selection_gesture_event_set` still returns success for any `time_ns`. No observable difference; the only real (doc-only, non-finding) discrepancy is that the PORT NOTE at selection_gesture.rs:1745-1757 quotes a pre-`std.Io.Timestamp` version of the reference body."
- **Suggested fix:** Use `base.checked_add(Duration::from_nanos(ns)).unwrap_or(base)` so the conversion is provably total; refresh the stale PORT NOTE quote.

#### S179-F1 — C-API terminal_new uses the 320MB app kitty-image storage limit instead of the 10MB lib default
- **Status:** CONFIRMED · **Kind:** different-constant · **Severity:** medium
- **Oracle:** `src/terminal/Terminal.zig:289-298` (default consumed by `src/terminal/c/terminal.zig:875-884`)
- **Rustty:** `crates/rustty-vt-c/src/terminal.rs:1548-1562`
- **Description:** `new_` in the oracle relies on `Terminal.Options` defaults, and `src/terminal/c/terminal.zig` is the `.lib` artifact, so a C-created terminal starts with a 10 MB kitty image storage limit. rustty-vt's `Options` has no defaults and vt-c writes `kitty_image_storage_limit: 320 * 1000 * 1000` unconditionally (`rustty_vt::options::ARTIFACT` is pinned to `Artifact::Ghostty` with a note saying the lib artifact is "selected by a later build seam"). The value is directly observable via `rustty_terminal_get(RUSTTY_TERMINAL_DATA_KITTY_IMAGE_STORAGE_LIMIT)` and changes when image eviction begins.
- **Evidence:** oracle `Terminal.zig:291-298`, `c/terminal.zig:875-884`; rustty `terminal.rs:1560`; workspace grep finds no `10 * 1000 * 1000` anywhere.
- **Suggested fix:** Pass the `.lib` default (10 MB) in vt-c `new_`, or derive it from an artifact constant.

#### S179-F2 — Terminal internals are not allocated through the caller-supplied C allocator (F-008 Tier-2 residual)
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/c/terminal.zig:875-884`
- **Rustty:** `crates/rustty-vt-c/src/terminal.rs:1526-1546`
- **Description:** The oracle threads the C-resolved allocator into terminal construction (`t.* = try .init(io.io(), alloc, .{...})`), so every internal allocation (pages, styles, screens, hyperlinks) goes through the embedder's `GhosttyAllocator` and a rejecting allocator surfaces `.out_of_memory`. rustty routes only the wrapper/terminal STORAGE through the caller allocator and constructs the terminal itself with `allocator_api2::alloc::Global` (documented as a remaining Tier-2 residual). An embedder supplying an arena/accounting/sandboxed heap observes almost none of the terminal's allocations, and OOM there aborts instead of returning `RUSTTY_OUT_OF_MEMORY`. (Checked: there is no `#[global_allocator]` shim redirecting Global to the C allocator in shipping code.)
- **Evidence:** oracle `:878-884`; rustty `terminal.rs:1526` `let gpa = allocator_api2::alloc::Global;` and `1538-1541` `ZigTerminal::init(gpa, ...)` with the PORT NOTE at `1500-1505`.
- **Suggested fix:** Thread the resolved C allocator into `ZigTerminal::init` (make rustty-vt's Terminal generic over the allocator, as its `deinit(alloc)` signature already suggests), or record the divergence as a header-documented limitation.

#### S182-F1 — ABI manifest omits RUSTTY_REJECTED (-7) from the RusttyResult enum values
- **Status:** CONFIRMED · **Kind:** missing-branch · **Severity:** medium
- **Oracle:** `src/terminal/c/types.zig:244` (row) / `src/terminal/c/result.zig:2-11`
- **Rustty:** `crates/rustty-vt-c/src/types.rs:1418-1435`
- **Description:** The oracle emits every field of `result.Result` into the manifest by reflection, so `GhosttyResult` carries `REJECTED: -7`. The rustty row is a hand-written `enum_values!` list of only 7 of the 8 variants, so `rustty_type_json` reports no name for -7 even though the Rust enum (`result.rs:23`) and the C header (`include/rustty/vt/types.h:108`) both have it. Manifest-only, but it breaks header-less FFI/WASM consumers decoding a rejected result.
- **Evidence:** oracle `result.zig:3-10` + `types.zig:244` reflection; rustty `types.rs:1424-1433` ends at `("limit_exceeded", LimitExceeded)`; workspace grep for the literal `"rejected"` finds nothing.
- **Suggested fix:** Add `("rejected", Rejected),` to the row.

#### S182-F2 — ABI manifest omits RUSTTY_SYS_OPT_RANDOM_SECURE (3) from RusttySysOption
- **Status:** CONFIRMED · **Kind:** missing-branch · **Severity:** medium
- **Oracle:** `src/terminal/c/types.zig:310` (row) / `src/terminal/c/sys.zig:61-66`
- **Rustty:** `crates/rustty-vt-c/src/types.rs:2338-2350`
- **Description:** `sys.Option` has four fields in the oracle (userdata, decode_png, log, random_secure=3), all reflected into the manifest. The rustty row lists only the first three, so the manifest never mentions RANDOM_SECURE even though `sys::Option_::RandomSecure = 3` exists (`sys.rs:119`, dispatched at `lib.rs:475`) and the header declares `RUSTTY_SYS_OPT_RANDOM_SECURE = 3`.
- **Evidence:** oracle `sys.zig:65`; rustty `types.rs:2342-2349`; grep for the manifest literal `random_secure` finds no row anywhere.
- **Suggested fix:** Add `("random_secure", RandomSecure),`.

#### S182-F3 — ABI manifest omits CLIPBOARD_READ (38) and CLIPBOARD_WRITE_MAX_BYTES (39) from RusttyTerminalOption
- **Status:** CONFIRMED · **Kind:** missing-branch · **Severity:** medium
- **Oracle:** `src/terminal/c/types.zig:315` (row) / `src/terminal/c/terminal.zig:1222-1223`
- **Rustty:** `crates/rustty-vt-c/src/types.rs:2437-2484`
- **Description:** The oracle's `terminal.Option` ends with `clipboard_read = 38` and `clipboard_write_max_bytes = 39`; both are reflected. The rustty row stops at `terminfo_name`, so two settable terminal options are invisible to manifest-driven (WASM) embedders even though both variants exist in the Rust enum (`terminal.rs:2484`, `:2486`) and are handled (`terminal.rs:2661`, `:3240`).
- **Evidence:** oracle `:1222-1223`; rustty `types.rs:2482` last row is `("terminfo_name", TerminfoName)`.
- **Suggested fix:** Append both rows.

#### S182-F4 — ABI manifest omits CLIPBOARD_WRITE_MAX_BYTES (40) from RusttyTerminalData
- **Status:** CONFIRMED · **Kind:** missing-branch · **Severity:** medium
- **Oracle:** `src/terminal/c/types.zig:314` (row) / `src/terminal/c/terminal.zig:1595`
- **Rustty:** `crates/rustty-vt-c/src/types.rs:2387-2436`
- **Description:** The oracle's `terminal.TerminalData` ends with `clipboard_write_max_bytes = 40` and reflects it. The rustty row stops at `cursor_at_prompt`, so the queryable data key is absent from the manifest even though the Rust enum has it (`terminal.rs:3663`) and the generated header declares it.
- **Evidence:** oracle `:1595`; rustty `types.rs:2433` last row `("cursor_at_prompt", CursorAtPrompt)`.
- **Suggested fix:** Append `("clipboard_write_max_bytes", ClipboardWriteMaxBytes),`.

#### S186-F1 — Integer control values containing '_' digit separators are rejected where the reference accepts them (kitty graphics)
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/kitty/graphics_command.zig:318-324`
- **Rustty:** `crates/rustty-vt/src/kitty/graphics_command.rs:437-499`
- **Description:** `Parser.finishValue` delegates every kitty graphics control value to `std.fmt.parseInt(i32|u32, buf, 10)`, which skips interior `_` (rejecting only leading/trailing), so `s=1_0` parses as 10 and `z=-1_0` as -10; `accumulateValue` applies no character filter, so `_` really does reach parseInt. The Rust `parse_int_u32`/`parse_int_i32` delegate to `str::parse`, whose `InvalidDigit` is mapped to `Error::InvalidFormat` and propagated out of `finish_value`, aborting the whole command. The port's PORT NOTE (`graphics_command.rs:517-521`) acknowledges the divergence, and the negative-magnitude path already honours `_`, so the port is internally inconsistent.
- **Evidence:** oracle `:321-322`; rustty `graphics_command.rs:433-441`, `452-465`, `480-499`.
- **Suggested fix:** Port Zig's `_` handling to the non-negative branches and add table rows (`"1_0" -> Ok(10)`, `"-1_0" -> Ok(-10)`, `"_10"/"10_" -> InvalidFormat`).

#### S189-F1 — Missing test "image load: rgb, not compressed, relative regular file"
- **Status:** CONFIRMED · **Kind:** missing-test · **Severity:** medium
- **Oracle:** `src/terminal/kitty/graphics_image.zig:1446-1485`
- **Rustty:** `crates/rustty-vt/src/kitty/graphics_image.rs:1474-2601` (mod tests)
- **Description:** The oracle has a dedicated test that transmits a CWD-RELATIVE path (`.zig-cache/tmp/{sub_path}/image.data`) with `medium=file` and asserts the load completes with `compression == .none`. It is the only test exercising a non-absolute path through the file medium (and transitively that handle-derived realpath validation works for a relative submitted path). Listing all 29 oracle tests against the 29 Rust tests shows this is the only one without a counterpart; every other file-medium test uses absolute paths.
- **Evidence:** oracle `:1446`; `grep -rn "relative regular" crates/` returns nothing in rustty-vt.
- **Suggested fix:** Add `image_load_rgb_not_compressed_relative_regular_file` writing the fixture into a scratch dir under the crate CWD and passing a relative path as `Command.data`.

#### S189-F2 — readFile with no `s=` size silently TRUNCATES at max_size instead of failing
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/kitty/graphics_image.zig:440-443`
- **Rustty:** `crates/rustty-vt/src/kitty/graphics_image.rs:680-687`
- **Description:** For `t.size == 0` the oracle calls `reader.appendRemaining(alloc, &managed, .limited(max_size))`, and Zig's `appendRemainingAligned` returns `error.StreamTooLong` as soon as the limit is reached; the oracle catches it and returns `error.InvalidData`. The rustty port uses `(&mut file).take(MAX_SIZE).read_to_end(&mut managed)`, which stops silently at MAX_SIZE with no error, so the load proceeds with a truncated 400 MiB payload. Observable: (a) for an animation-frame load `complete()` accepts `actual_len >= expected_len` and truncates, so a >= 400 MiB file the oracle rejects is ACCEPTED and composed from truncated bytes; (b) for a zlib payload the truncated stream fails later as `DecompressionFailed` where the oracle produced `InvalidData` — a different kitty error response.
- **Evidence:** oracle `:440-443` with `std/Io/Reader.zig:356-386`; rustty `graphics_image.rs:681-687`; frame acceptance at `graphics_image.rs:766-779`.
- **Suggested fix:** Read `MAX_SIZE + 1` (or check `managed.len() >= MAX_SIZE` after the take) and return `Error::InvalidData`, mirroring `error.StreamTooLong`.

#### S194-F2 — test "OSC 52: get/set clipboard with allocator" ported without an allocator, making it a duplicate
- **Status:** CONFIRMED · **Kind:** weakened-assertion · **Severity:** medium
- **Oracle:** `src/terminal/osc/parsers/clipboard_operation.zig:94-107`
- **Rustty:** `crates/rustty-vt/src/osc/parsers/clipboard_operation.rs:130-152`
- **Description:** The reference test exists solely to exercise OSC 52 capture through the ALLOCATING backing (`Parser.init(testing.allocator)` + `deinit()`). OSC 52 really does request `captureTrailing(.allocating)` on both sides (`osc.zig:805`, `osc/mod.rs:1216-1218`), and the port's `capture_trailing` falls back to Fixed when `self.alloc.is_none()` — so the port's `Parser::init(None)` makes this test byte-for-byte the same scenario as `osc_52_get_set_clipboard` above it and covers none of the allocator path the test name names. Threading an allocator into tests is available and used elsewhere (`kitty_color.rs:154`, `osc/mod.rs:1403`).
- **Evidence:** oracle `:97-98`; rustty `clipboard_operation.rs:132-136` PORT NOTE + `Parser::init(None)`.
- **Suggested fix:** Construct the parser with a `ZigAllocatorRef` in this test.

#### S194-F3 — parse_int_u9 rejects the '_' digit separators that Zig's std.fmt.parseInt accepts (OSC 4/5/104/105)
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/osc/parsers/color.zig:177-181, 250-254`
- **Rustty:** `crates/rustty-vt/src/osc/parsers/color.rs:482-488`
- **Description:** The reference parses the OSC 4/5/104/105 color index with `std.fmt.parseInt(u9, color_str, 10)`, which accepts `_` between digits. The port's `parse_int_u9` uses `str::parse::<u16>()`, which rejects any `_`. Observable: `OSC 4;1_0;red` sets palette index 10 in the reference, while the port's parse failure returns the results accumulated so far (an empty list for a leading pair) — the set is silently dropped; same for the `continue` path in `parse_reset_ansi_color`.
- **Evidence:** oracle `:177-181` (`catch return result`) and `:250-254` (`catch continue`); rustty `color.rs:482-488`.
- **Suggested fix:** Mirror `std.fmt.parseInt`'s separator handling (strip interior `_`, reject leading/trailing/doubled) before the numeric parse.

#### S195-F1 — OSC 5522 Option_::read hand-rolls prefix key matching instead of using the ported ValueIterator
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/osc/parsers/kitty_clipboard_protocol.zig:90-115` + `src/terminal/osc/kitty_metadata.zig:39-72`
- **Rustty:** `crates/rustty-vt/src/osc/parsers/kitty_clipboard_protocol.rs:225-292`
- **Description:** The oracle's `Option.read` delegates to `kitty_metadata.ValueIterator(@tagName(key), null)`, which splits the metadata on `:`, requires each field to contain `=`, trims the key with `std.ascii.whitespace` and requires it to be EXACTLY the option name, and `continue`s on any mismatch or malformed field. The rustty port does not call the (ported, faithful) `kitty_metadata::ValueIterator`; it keeps an older hand-written scan that (a) matches with `metadata[pos..].starts_with(key)` — a PREFIX match — and (b) `return self.null_value()` when the byte after that prefix is not `=`, aborting the whole search instead of skipping the field. So a field whose key is a strict extension of the searched key hides all later matching fields, and a field lacking `=` aborts the search (e.g. `pwx=1:pw=B` → oracle "B", rustty null; `typeX=1:type=read` → oracle read, rustty null; `id:id=5` → oracle "5", rustty null). The oracle's own kitty_metadata test (`kitty_metadata.zig:76-84`) locks exactly this behaviour. Secondary nit in the same path: the local `trim_whitespace` (`kitty_clipboard_protocol.rs:329-341`) uses `u8::is_ascii_whitespace`, which excludes VT (0x0B) that `std.ascii.whitespace` includes (the sibling `kitty_desktop_notification.rs:559-572` spells the Zig set out).
- **Evidence:** oracle `kitty_metadata.zig:52-59`; rustty `kitty_clipboard_protocol.rs:242-251` (`starts_with`) and `:262-265` (`return self.null_value()`).
- **Suggested fix:** Replace the hand-rolled scan with `kitty_metadata::ValueIterator::init(self.tag_name(), None, metadata).next()` (as `kitty_desktop_notification.rs:376-395` does), delete the local `trim_whitespace`, and add the prefix-key regression test.

#### S195-F2 — OSC 66 update() rejects '_' digit separators that Zig's std.fmt.parseInt accepts
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/osc/parsers/kitty_text_sizing.zig:49-54`
- **Rustty:** `crates/rustty-vt/src/osc/parsers/kitty_text_sizing.rs:95-105`
- **Description:** The oracle parses every OSC 66 option value with `std.fmt.parseInt(u4, value, 10)`, which accepts `_` digit separators (the sibling parser `kitty_desktop_notification.zig` filters `_` explicitly precisely because parseInt otherwise accepts it; `kitty_text_sizing.zig` does NOT filter). The rustty port uses `str::parse::<u8>()`, which rejects `_`, so such values take the `InvalidValue` path and the caller (`kitty_text_sizing.zig:135-141`) leaves the field at its default with a warning — e.g. `66;n=1_2;x` gives numerator 12 in the oracle vs 0 in rustty.
- **Evidence:** oracle `:50-54`; rustty `kitty_text_sizing.rs:102-105`; contrast the ported `_` rejection in `kitty_desktop_notification.rs:424-427` (which the oracle spells out there).
- **Suggested fix:** Strip/accept `_` the way Zig's parseInt does before `parse::<u8>()`, ideally via a shared `parse_int_zig` helper.

#### S206-F1 — Allocation-failure transactionality test sweeps 8 tripwire indices instead of every decode allocation, and drops the allocation_count > 0 assertion
- **Status:** CONFIRMED · **Kind:** weakened-assertion · **Severity:** medium
- **Oracle:** `src/terminal/snapshot/snapshot.zig:1636-1694`
- **Rustty:** `crates/rustty-vt/src/snapshot/codec.rs:2582-2637`
- **Description:** The oracle test first decodes with a non-failing `testing.FailingAllocator` to learn `allocation_count` (asserting it > 0), then re-runs the decode once per allocation index `0..allocation_count`, requiring each run to either succeed or return exactly `error.OutOfMemory`, with `testing.allocator` proving leak-freedom on every path. The port keeps the baseline decode but discards its result (no count, no `> 0` assertion) and hardcodes `for fail_index in 0..8usize` over the tripwire seam. Only three production sites carry `tw::check(SnapshotFp::Alloc)` (`snapshot/terminal.rs:919`, `continuation.rs:201`, `hyperlink.rs:193`), so indices 3..7 are no-ops and screen/page/pagelist/grapheme decode allocations — which the oracle sweep covers through the two-screen + history golden — are never failure-injected. The test is additionally `#[cfg(debug_assertions)]`-gated, so it does not run in release test builds.
- **Evidence:** oracle `:1637-1652`; rustty `codec.rs:2596-2618`; `grep -rn "SnapshotFp::Alloc" crates/rustty-vt/src` → three sites.
- **Suggested fix:** Instrument the remaining decode allocation sites, derive the sweep bound from a baseline that counts tripwire hits, and assert that count is > 0.


### Low

The 71 low-severity findings below survived verification (53 CONFIRMED, 18 DISPUTED). The dominant class is the deliberate one named in the summary: rustty-vt wires no logging facade, so reference `log.*` diagnostics are carried as comments rather than emitted.

#### S122-F1 — All log.err/log.warn diagnostics in this range are commented out (rustty-vt has no logging facility)
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/PageList.zig:186, 278, 820-823, 831-834, 845-848, 871-874, 880-883, 891-894, 1828`
- **Rustty:** `crates/rustty-vt/src/pagelist/mod.rs:572, 625, 1985-1988, 1996-1999, 2010-2013, 2040-2043, 2049-2052, 2060-2063; crates/rustty-vt/src/pagelist/resize.rs:1587`
- **Description:** The oracle emits scoped `page_list` log lines before the integrity-check panics (with cached/actual values), before the restore/clone panics (with the decode error), and a `log.warn("reflowRow OutOfSpace on first row, discarding cell managed memory")` on the reflow degradation path. In rustty every one of these is a `// log.…` comment; rustty-vt declares no `log`/`tracing` dependency (checked Cargo.toml) so nothing is emitted. Control flow and panic behaviour are identical; only the diagnostic text is lost (the integrity panic message does include the `IntegrityError` variant via `{_err:?}` but not the cached/actual numbers).
- **Evidence:** grep of crates/rustty-vt/src/pagelist/{mod,resize,access}.rs finds 21 `// log.` commented sites and zero live log calls; oracle lines cited above each contain a live `log.warn`/`log.err`.
- **Suggested fix:** Route these through the crate's chosen diagnostics seam (e.g. a `log`/`tracing` facade) so the same messages are observable under a `page_list` scope.

#### S123-F1 — All log.err/log.warn diagnostics in this range are commented out (rustty-vt has no logging facility)
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/PageList.zig:2282, 2311, 2352, 2373, 2401, 2481, 2555, 3111, 3665, 3742`
- **Rustty:** `crates/rustty-vt/src/pagelist/resize.rs:2065, 2105, 2158, 2186-2189, 2210-2213, 2291-2294, 2380-2383, 1060-1068; grow.rs 117-120, 873`
- **Description:** The oracle emits scoped `page_list` log messages on the degraded-recovery paths (setGraphemes/link dupe/addWithIdContext/setHyperlink/addWithId failures after capacity increase, moveLastRowToNewPage clone failure, unexpected cloneRowFrom failure in resizeWithoutReflowGrowCols, compact clone failure, split clone failure). The Rust port keeps the message text as `// log.err(...)` comments and emits nothing; rustty-vt has no `log`/`tracing` dependency at all (Cargo.toml has none; no `log::` use anywhere in the crate). Control flow on every one of these paths is otherwise identical (unreachable!() under debug_assertions, graceful degradation in release).
- **Evidence:** resize.rs:2065 `// log.err("setGraphemes failed after capacity increase err={}", .{err});` vs PageList.zig:2282 `log.err("setGraphemes failed after capacity increase err={}", .{err});`; resize.rs:1065-1068 PORT NOTE: "the scoped logger is not yet wired in the Rust port". `grep -rn 'log::' crates/rustty-vt/src` returns nothing.
- **Suggested fix:** Wire a `log`/`tracing` facade into rustty-vt (feature-gated if desired) and replace the commented-out lines with `log::error!`/`log::warn!` calls carrying the same text.

#### S123-F2 — `defer assert(self.total_rows == old_total_rows)` not evaluated on the error exit of the cols branch
- **Status:** CONFIRMED · **Kind:** missing-branch · **Severity:** low
- **Oracle:** `src/terminal/PageList.zig:2802-2803`
- **Rustty:** `crates/rustty-vt/src/pagelist/resize.rs:596-597, 674-675`
- **Description:** The oracle's `defer assert(...)` in the `opts.cols` block runs on every exit including the `try self.resizeWithoutReflowGrowCols` error propagation (a safety-build panic if the invariant is broken during error unwinding). The Rust port only asserts after the match on the success path; on the `Err` return at resize.rs:663-667 the assertion is skipped. Debug-assertion-only; no release-mode behavioural change.
- **Evidence:** resize.rs:663-667 returns `Err(e)` directly; the `assert!(self.total_rows == old_total_rows)` is at resize.rs:675 after the match, unreachable on that path. PageList.zig:2803 `defer assert(self.total_rows == old_total_rows);`.
- **Suggested fix:** Also assert (or debug_assert) `self.total_rows == old_total_rows` before the early `return Err(e)` at resize.rs:663-667, or wrap the block in a scope guard.

#### S124-F1 — log.info/log.err calls in increaseCapacity and eraseRows are elided (commented out)
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/PageList.zig:4317, 4336, 5580`
- **Rustty:** `crates/rustty-vt/src/pagelist/grow.rs:767, 784; crates/rustty-vt/src/pagelist/erase.rs:616`
- **Description:** The oracle emits `log.info("adjusting page capacity={}", .{cap})` before recloning in increaseCapacity, `log.err("increaseCapacity clone failed err={}")` before the panic, and `log.err("failed to regrow active area after erase err={}")` on the eraseRows regrow failure. The Rust port keeps these as `// log.info(...)` / `// log.err(...)` comments; no log facade is invoked (grep for `log::`/`tracing::` in crates/rustty-vt/src/pagelist finds none). Control flow is identical; only the diagnostic output is missing.
- **Evidence:** PageList.zig:4317 `log.info("adjusting page capacity={}", .{cap});` vs grow.rs:767 `// log.info("adjusting page capacity={}", .{cap});`; PageList.zig:5580 `log.err("failed to regrow active area after erase err={}", .{err});` vs erase.rs:612-617 PORT NOTE + commented call then `return;`.
- **Suggested fix:** Wire the crate's logging facade (whatever rustty-vt uses for scoped `terminal` logs) and emit the three messages at the same points.

#### S124-F2 — diagram() panics on an unencodable codepoint where Zig `{u}` writes U+FFFD
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/PageList.zig:5926-5931`
- **Rustty:** `crates/rustty-vt/src/pagelist/access.rs:199-207`
- **Description:** The oracle prints cell/grapheme codepoints with `writer.print("{u}", ...)`, which (std.Io.Writer.printUnicodeCodepoint) substitutes the replacement character for surrogate halves / out-of-range u21 values. The Rust port uses `char::from_u32(cell.codepoint()).unwrap()` and `char::from_u32(*cp).unwrap()`, which panic on such values. Cell codepoints come from the UTF-8 decoder and are valid scalars in practice, so this is an edge case in a debug-only diagram routine.
- **Evidence:** PageList.zig:5926 `try writer.print("{u}", .{cell.codepoint()});` / 5930 `try writer.print("{u}", .{cp});` vs access.rs:199 `write!(writer, "{}", char::from_u32(cell.codepoint()).unwrap())?;` and access.rs:206 `write!(writer, "{}", char::from_u32(*cp).unwrap())?;`
- **Suggested fix:** Use `char::from_u32(cp).unwrap_or(char::REPLACEMENT_CHARACTER)` at both sites.

#### S126-F1 — cold-compression-after-incompressible-page test drops the decommitted_raw_bytes == second page len assertion
- **Status:** CONFIRMED · **Kind:** weakened-assertion · **Severity:** low
- **Oracle:** `src/terminal/PageList.zig:9224-9227`
- **Rustty:** `crates/rustty-vt/src/pagelist/tests.rs:2600-2632`
- **Description:** The oracle asserts `second.metadata().memory.len == memory.decommitted_raw_bytes` after the full pass (proving exactly one page's mapping was decommitted and the failed first page was not). The Rust port asserts compressed_pages==1, first not compressed, second compressed, encoded<decommitted, page_size unchanged, and stats unchanged on a second pass, but never checks decommitted_raw_bytes against the second page's raw length. The port also constructs the list with `max_size: Some(10 MiB)` instead of the oracle's default options.
- **Evidence:** Oracle 9224-9227: `try testing.expectEqual(second.metadata().memory.len, memory.decommitted_raw_bytes);`. Rust 2613-2624 has no assertion mentioning decommitted_raw_bytes except the `<` comparison against encoded_bytes.
- **Suggested fix:** Add `assert_eq!(unsafe { (*super::Node::metadata(second)).memory_len() }, memory.decommitted_raw_bytes);` after the compressed_pages assertion and use `Options::new(80, 24)` to match the oracle setup.

#### S132-F1 — Parser log.warn diagnostics dropped (invalid intermediates count; CSI colon/mixed separator on non-'m')
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/Parser.zig:319-328, 412-424`
- **Rustty:** `crates/rustty-vt/src/parser.rs:707-717, 820-838`
- **Description:** The oracle emits `log.warn("invalid intermediates count")` when a 5th intermediate is collected (Parser.zig:322) and `warnCsiSepMismatch` logs `"CSI colon or mixed separators only allowed for 'm' command, got: {f}"` (Parser.zig:418-424) before dropping the CSI. The Rust port keeps the identical control flow (early return / return None) but the warnings are ported only as comments (parser.rs:711, 830-835) because rustty-vt has no logging facade (crate-wide convention stated at parser.rs:27-29; no log::/tracing:: usage anywhere in crates/rustty-vt/src). Parsing behaviour and returned actions are identical; only the diagnostic side effect is absent.
- **Evidence:** Parser.zig:322 `log.warn("invalid intermediates count", .{});` and :420-423 log.warn with the CSI formatter vs parser.rs:711 `// log.warn("invalid intermediates count", .{});` and parser.rs:832-835 commented `log.warn(...)`; the `return`/`return None` that follow match the oracle's `return`/`break :csi_dispatch null`.
- **Suggested fix:** If/when a log facade is wired into rustty-vt, emit the two warnings at the same points (cold/noinline helper for the CSI one, as the oracle does).

#### S133-F4 — Screen.clone maps PageList CloneFrom errors to OutOfMemory
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/Screen.zig:480-498`
- **Rustty:** `crates/rustty-vt/src/screen.rs:994-998`
- **Description:** Oracle `clone` returns `!Screen` with the inferred error set of `PageList.clone`, which includes `Page.CloneFromError` (StyleSetError || HyperlinkError || GraphemeError, e.g. NeedsRehash / OutOfMemory sub-variants) from `try dst_page.cloneFrom(...)` (PageList.zig:1173). The Rust port does `self.pages.clone(&mut opts).map_err(|_| OutOfMemory)?`, so a CloneFrom failure surfaces to callers as OutOfMemory. The PORT NOTE claims only allocator failures are reachable, but `CloneError::CloneFrom` exists precisely because `clone_from` can fail with set/rehash errors. Only the error variant differs; success behaviour is identical.
- **Evidence:** Oracle Screen.zig:493-497 `var pages = try self.pages.clone(alloc, .{...});` with PageList.zig:1173 `try dst_page.cloneFrom(...)` (CloneFromError = StyleSetError || HyperlinkError || GraphemeError, page.zig:694-697). Rust screen.rs:998 `let mut pages = self.pages.clone(&mut opts).map_err(|_| OutOfMemory)?;` and pagelist/access.rs:772-775 `pub enum CloneError { OutOfMemory, CloneFrom(CloneFromError) }`.
- **Suggested fix:** Return `pagelist::access::CloneError` (or an enum wrapping it) from `Screen::clone` and propagate with `?` instead of collapsing.

#### S133-F5 — log.warn / log.err calls in increaseCapacity, cursorChangePin and cursorCopy are elided
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/Screen.zig:649-652, 671-674, 1473, 1541, 1562`
- **Rustty:** `crates/rustty-vt/src/screen.rs:1258-1260, 1279-1281, 2295, 2318, 2410`
- **Description:** The oracle emits `log.warn("(Screen.increaseCapacity) Failed to add cursor style back to page, err={}")`, `log.warn("(Screen.increaseCapacity) Failed to add cursor hyperlink back to page, err={}")`, `log.err("failed to update style on cursor change err={}")` and `log.err("failed to update hyperlink on cursor change err={}")` (twice) on the degrade paths. The Rust port keeps the graceful-degrade behaviour identical (reset style / drop hyperlink) but the log lines are present only as commented-out Zig; rustty-vt has no logging facade wired in, so nothing is emitted. Cosmetic/diagnostic only.
- **Evidence:** Oracle Screen.zig:649-652, 671-674, 1473, 1541, 1562 (`log.warn(...)` / `log.err(...)`). Rust screen.rs:1259-1260, 1280-1281, 2295, 2318, 2410 contain only `// log.warn(...)` / `// log.err(...)` comments; `grep -rn 'log::\|tracing::' crates/rustty-vt/src/*.rs` returns nothing.
- **Suggested fix:** Route these through whatever logging seam rustty-vt adopts (e.g. the `log` crate scoped `screen`) so the warnings are observable as in the oracle.

#### S134-F1 — Seven log.err/log.warn diagnostics in the resize/style/hyperlink degrade paths are commented out, not emitted
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/Screen.zig:2133, 2172, 2264, 2298, 2529, 2542, 2650`
- **Rustty:** `crates/rustty-vt/src/screen.rs:3159, 3210, 3323, 3366, 3639, 3647, 3790`
- **Description:** The oracle logs when it degrades gracefully: `failed to update style on cursor reload err={}` and `failed to update hyperlink on resize err={}` (resize), `cursor on prompt line but promptIterator found no prompt` (clearPromptForRedraw), `setAttribute error restoring old style after failure err={}` (setAttribute), `style addition failed after capacity increase` / `style rehash resulted in needs rehash` (manualStyleUpdate), `grapheme append failed after capacity increase` (appendGrapheme). The Rust port carries each as a `// log.err(...)` comment and emits nothing; rustty-vt has no logging facility at all (no `log`/`tracing` dependency), so this is a crate-wide omission rather than a per-site slip. Control flow at each site is identical; only the diagnostic output is missing.
- **Evidence:** Screen.zig:2133 `log.err("failed to update style on cursor reload err={}", .{err});` vs screen.rs:3159 `// log.err("failed to update style on cursor reload err={}", .{err});`. `grep -rln 'log::warn!|log::error!|tracing::warn!' crates/rustty-vt/src` returns nothing; Cargo.toml has no log/tracing dep.
- **Suggested fix:** Add a logging seam to rustty-vt (e.g. the `log` facade) and emit the seven messages at the marked sites with the same text.

#### S134-F2 — testWriteString flattens cursorSetHyperlink's OutOfSpace into OutOfMemory
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/Screen.zig:3796, 3813, 3826, 3836`
- **Rustty:** `crates/rustty-vt/src/screen.rs:5221, 5238, 5259, 5274`
- **Description:** The oracle's `testWriteString` returns an inferred error set, so `try self.cursorSetHyperlink()` propagates `PageList.IncreaseCapacityError` (OutOfMemory | OutOfSpace) unchanged. The port's `PrintStringError` (terminal.rs:680-687) has only OutOfMemory | InvalidUtf8, so every `cursor_set_hyperlink()` failure is `.map_err(|_| OutOfMemory)`; the same flattening applies to `cursor_down_or_scroll` and the zero-width `append_grapheme` (those are `Allocator.Error`-only in the oracle so no loss there). Only the hyperlink OutOfSpace case is a distinguishable error lost. This is a test-only helper ("jank version of Terminal.printString"), so it is not user-visible.
- **Evidence:** Screen.zig:3796 `if (self.cursor.hyperlink_id > 0) try self.cursorSetHyperlink();` (error set IncreaseCapacityError) vs screen.rs:5221 `self.cursor_set_hyperlink().map_err(|_| OutOfMemory)?;`
- **Suggested fix:** Add an `OutOfSpace` variant to `PrintStringError` (or a dedicated `TestWriteStringError`) and propagate `IncreaseCapacityError` unchanged.

#### S139-F1 — Stale header comment says '18 Selection tests' but 20 are ported
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/Selection.zig:513-1676`
- **Rustty:** `crates/rustty-vt/src/selection.rs:7-8`
- **Description:** The module header PORT STATUS claims 'the 18 Selection tests'. The oracle at pin 20abdb50a has 20 test blocks (the two mixed-width clamp tests 'rectangle corners clamp across mixed-width pages' and 'containedRow clamps mixed-width pages' were added upstream) and the Rust file does contain all 20. Doc-only inaccuracy; no functional impact.
- **Evidence:** Oracle test blocks at Selection.zig:513,580,629,678,725,772,800,828,856,926,994,1059,1085,1199,1262,1325,1406,1452,1515,1635 (20). Rust #[test] fns at selection.rs:693..1474 (20). Header selection.rs:7-8 says 18.
- **Suggested fix:** Update the header comment to say 20 tests (or drop the count).

#### S140-F1 — StringMap tests build the line selection with non-default SelectLine options
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/StringMap.zig:142-147, 199-204, 264-266`
- **Rustty:** `crates/rustty-vt/src/string_map.rs:472-481, 563-572, 661-667`
- **Description:** The oracle tests call `s.selectLine(.{ .pin = ... })`, which uses the Zig SelectLine field defaults `whitespace = &selection_codepoints.default_line_whitespace` and `semantic_prompt_boundary = true` (Screen.zig:2994-3006). The Rust ports of all three tests pass `whitespace: None, semantic_prompt_boundary: false`, so the fixture selection is not trimmed of trailing whitespace (the URL-test lines span to x=39 instead of x=35). The assertions (match start/end points, `next()==None`, `found_url`) are the same count and still hold because the regex match offsets are unaffected, but the fixture differs from the reference. Note the same file's production code (selection_gesture.rs press_selection/drag_selection_line) correctly spells out the defaults, so the test-side choice is inconsistent.
- **Evidence:** Oracle StringMap.zig:142-147 `s.selectLine(.{ .pin = s.pages.pin(...).? })` (defaults apply); rustty string_map.rs:472-481 `s.select_line(SelectLine { pin: ..., whitespace: None, semantic_prompt_boundary: false })`; same pattern at 563-572 and 661-667.
- **Suggested fix:** Use `whitespace: Some(&crate::selection_codepoints::DEFAULT_LINE_WHITESPACE), semantic_prompt_boundary: true` in the three test fixtures (and the additive test) to match the Zig defaults.

#### S140-F2 — Alternate-screen test fixtures use max_scrollback_bytes=10_000 instead of the Screen.Options default 0
- **Status:** DISPUTED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/SelectionGesture.zig:1635-1638, 1769-1772, 1826-1829, 2065-2068, 2084-2087`
- **Rustty:** `crates/rustty-vt/src/selection_gesture.rs:1916-1919, 2094-2097, 2174-2177, 2493-2496, 2522-2525`
- **Description:** The oracle tests create the alternate screen with `t.screens.getInit(io, alloc, .alternate, .{ .cols = t.cols, .rows = t.rows })`; `getInit` takes `Screen.Options` (ScreenSet.zig:75-81) whose `max_scrollback_bytes` defaults to 0 (Screen.zig:266). The Rust tests pass `screen_opts(cols, rows, 10_000)`, i.e. `max_scrollback_bytes: Some(10_000)`. No assertion in these five tests depends on the alternate screen's scrollback, so the outcome is identical, but the fixture value differs from the reference.
- **Evidence:** Oracle SelectionGesture.zig:1635-1638 `.{ .cols = t.cols, .rows = t.rows }` with Screen.Options default `max_scrollback_bytes: ?usize = 0`; rustty selection_gesture.rs:1916-1919 `.get_init(Allocator, Key::Alternate, screen_opts(cols, rows, 10_000))` where screen_opts sets `max_scrollback_bytes: Some(max_scrollback_bytes)` (lines 1113-1126).
- **Suggested fix:** Pass `screen_opts(cols, rows, 0)` in the five get_init call sites to mirror the Zig Screen.Options default.
- **Disputed by:** semantic lens (confidence high) — "The fixture value does differ (oracle SelectionGesture.zig:1635-1638/1769/1826/2065/2084 pass only .cols/.rows so Screen.Options.max_scrollback_bytes defaults to 0 per Screen.zig:266; rustty selection_gesture.rs:1918/2096/2176/2495/2524 pass screen_opts(cols, rows, 10_000), helper at 1113-1126 setting Some(10_000)), but it is functionally inert here: max_scrollback_bytes only affects pruning when rows scroll off the active area, and none of these five tests writes or scrolls on the alternate screen — they press/release, switch screens, remove the screen, and assert left_click_count, left_click_screen, left_drag_autoscroll, and countTrackedPins (e.g. oracle 2053-2074 vs rustty 2479-2510). No input to the code under test and no asserted value can differ, so this is a cosmetic fixture mismatch with no observable behaviour difference."

#### S141-F3 — Tabstops.resize aborts on a real allocation failure instead of returning OutOfMemory and preserving cols
- **Status:** CONFIRMED · **Kind:** missing-error-path · **Severity:** low
- **Oracle:** `src/terminal/Tabstops.zig:151-155`
- **Rustty:** `crates/rustty-vt/src/tabstops.rs:283-291`
- **Description:** The reference `alloc.realloc` propagates error.OutOfMemory and leaves `self.cols` unchanged (the test at 281-297 asserts this). The port only returns Err through the cfg(test) tripwire; the actual growth uses `allocator_api2::vec::Vec::resize`, which calls handle_alloc_error (process abort) on allocator failure, so in production a real OOM never reaches the `Result` and Terminal::init/resize cannot observe it. Also, `core::mem::replace` moves the old box out before the growth, so an abort happens mid-state (moot since the process dies).
- **Evidence:** Oracle Tabstops.zig:152-155 `self.dynamic_stops = try alloc.realloc(...)`; rustty tabstops.rs:283-289 `new_vec.resize(dynamic_count, 0)` with no try_reserve; the only Err path is `#[cfg(test)] resize_tw_check(...)?` at 271-272.
- **Suggested fix:** Use `try_reserve_exact(dynamic_count - old_len).map_err(|_| OutOfMemory)?` before `resize`, so the error propagates and `cols` stays unchanged.

#### S141-F4 — log.warn diagnostics in print (zero-width), printCell (hyperlink realloc) and restoreCursor are dropped
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/Terminal.zig:1467, 1480, 1958, 1975`
- **Rustty:** `crates/rustty-vt/src/terminal.rs:2452, 2468, 2722, 3199`
- **Description:** The reference emits `log.warn("zero-width character with no prior character, ignoring")` (twice), `log.warn("error reallocating for more hyperlink space, ignoring hyperlink err={}")` and `log.warn("restoreCursor error updating style err={}")`. The port keeps them only as commented-out lines; rustty-vt has no logging call at all in terminal.rs (grep for log::warn/tracing::warn returns nothing). Control flow is identical; only the diagnostic output differs.
- **Evidence:** Oracle Terminal.zig:1467,1480,1958,1975 vs rustty terminal.rs:2452,2468,2722,3199 (all `// log.warn(...)` comments).
- **Suggested fix:** Wire the crate's log facility (or `log` crate) and emit the same warnings.

#### S142-F1 — All log.warn/log.err diagnostics in this range are dropped (commented out) in the port
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/Terminal.zig:2288, 3012-3013, 3232-3233, 3524, 3598, 4086, 4149-4152`
- **Rustty:** `crates/rustty-vt/src/terminal.rs:3572-3573, 4591-4593, 4847-4849, 5348-5349, 5431, 6192, 6247-6248`
- **Description:** The oracle emits diagnostics at seven sites in this range: tabClear unknown-setting warn, insertLines/deleteLines trackPin OOM err (before @panic), eraseLine unimplemented-mode err, eraseDisplay scroll-clear-failed warn, resize alternate-screen-resize-failed warn and alternate-screen-replacement-failed warn. rustty keeps each as a `// log.warn(...)` comment and emits nothing (rustty-vt has no logging facility at all: grep for log::/warn!/error! across crates/rustty-vt/src returns nothing). Control flow after each site is identical (return / panic / fallback), so this is log-text-only.
- **Evidence:** Terminal.zig:2288 `else => log.warn("invalid or unknown tab clear setting: {}", .{cmd})` vs terminal.rs:3572-3573 `// log.warn(...)` then `csi::TabClear::Other(_) => {}`; Terminal.zig:3524 `log.err("unimplemented erase line mode: {}", .{mode}); return;` vs terminal.rs:5347-5350 `_ => { // log.err(...) return; }`; Terminal.zig:4086 `log.warn("alternate screen resize failed, replacing it err={}", .{err});` vs terminal.rs:6192 comment only; Terminal.zig:4149-4152 replacement-failed warn vs terminal.rs:6247-6248 comment only.
- **Suggested fix:** If/when rustty-vt adopts a logging facade (e.g. the `log` crate), restore these seven warn/err calls with the oracle's message text; otherwise document the crate-wide no-logging policy once so these are not re-audited.

#### S143-F2 — resize() drops the two log.warn diagnostics on alternate-screen failure
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/Terminal.zig:4108, 4149-4152`
- **Rustty:** `crates/rustty-vt/src/terminal.rs:6192, 6246-6250`
- **Description:** The oracle logs `alternate screen resize failed, replacing it err=...` and `alternate screen replacement failed, falling back to primary err=...`; rustty keeps both only as comments (the crate has no logging facility, a crate-wide convention also applied to the cursor-copy warnings in switch_screen_mode). Behaviour is otherwise identical; only the diagnostic output is missing.
- **Evidence:** Oracle 4108: `log.warn("alternate screen resize failed, replacing it err={}", .{err});` and 4149-4152 the replacement-failed warn. Rustty 6192: `// log.warn("alternate screen resize failed, replacing it err={}", .{err});` and 6247-6248 the second as a comment; the bound error is named `_err`/`_init_err` and never emitted.
- **Suggested fix:** If/when rustty-vt gains a log facade, emit these two warnings at the same points.

#### S145-F1 — hyperlink-dense insertLines test initialised with max_scrollback_bytes=10_000 instead of 1024
- **Status:** DISPUTED · **Kind:** different-constant · **Severity:** low
- **Oracle:** `src/terminal/Terminal.zig:8442-8444`
- **Rustty:** `crates/rustty-vt/src/terminal.rs:16124`
- **Description:** The oracle constructs the terminal with `.{ .rows = 5, .cols = 10, .max_scrollback_bytes = 1024 }`; the rustty port uses `opts(10, 5)`, whose helper sets `max_scrollback_bytes: Some(10_000)`. The sibling test 'insertLines across page boundary marks all shifted rows dirty' correctly uses `opts_sb(10, 5, 1024)`. Every assertion in the body is otherwise identical, so this is a test-precondition drift rather than a missing assertion; the scrollback cap affects how PageList prunes/keeps pages around the boundary the test is exercising.
- **Evidence:** Terminal.zig:8444 `var t = try init(io_impl, alloc, .{ .rows = 5, .cols = 10, .max_scrollback_bytes = 1024 });` vs terminal.rs:16124 `let mut t = Terminal::init(Global, opts(10, 5)).unwrap();` with terminal.rs:7201 `max_scrollback_bytes: Some(10_000)`.
- **Suggested fix:** Use `opts_sb(10, 5, 1024)` at terminal.rs:16124 (and likewise check the deleteLines twin at 16238 which is outside this shard).
- **Disputed by:** semantic lens (confidence high) — "The constant does differ (Terminal.zig:8444 `.max_scrollback_bytes = 1024` vs terminal.rs:16124 `opts(10, 5)` with `max_scrollback_bytes: Some(10_000)` at terminal.rs:7201), but it is unobservable. Both values are clamped away by the same effective-minimum logic: PageList.zig:6940-6946 `pub fn max()` returns `@max(self.bytes.explicit, self.bytes.min)` and `bytes.min = minMaxSize(cols, rows)` (PageList.zig:6907-6912, 7022-7062) = `PagePool.item_size * (pages_exact + 1)` >= 2 standard page allocations (hundreds of KB). Both 1024 and 10_000 are orders of magnitude below that floor, so `limits.max(.bytes)` is byte-identical in the two runs and no pruning/enforcement decision (PageList.zig:4027, `exceeded`) can differ. rustty ports the identical clamp: crates/rustty-vt/src/pagelist/mod.rs:1509 (`min: Limits::min_max_size(cols, rows)`), :1542 (`core::cmp::max(self.bytes.explicit, self.bytes.min)`), :1570 (`fn min_max_size`). The test body and all assertions are otherwise identical, so no input/state produces a different result."

#### S146-F1 — deleteLines hyperlink-dense test inits with 10_000 scrollback bytes instead of oracle's 1024
- **Status:** DISPUTED · **Kind:** different-constant · **Severity:** low
- **Oracle:** `src/terminal/Terminal.zig:11463-11466`
- **Rustty:** `crates/rustty-vt/src/terminal.rs:16248`
- **Description:** Oracle test "Terminal: deleteLines hyperlink-dense row crosses page boundary" constructs the terminal with `.{ .rows = 5, .cols = 10, .max_scrollback_bytes = 1024 }`. The rustty counterpart uses `opts(10, 5)`, whose `max_scrollback_bytes` is `Some(10_000)` (terminal.rs:7201); the `opts_sb(10, 5, 1024)` helper that exists for exactly this purpose (terminal.rs:7219) is not used. The sibling 'across page boundary marks all shifted rows dirty' test does use `opts_sb(10, 5, 1024)` (terminal.rs:15396), so this one is inconsistent. All assertions of the test body are otherwise identical, so the page-boundary/hyperlink-capacity path is still exercised, but the scrollback-limit configuration under which it is exercised differs from the oracle (the same omission exists in the insertLines sibling at terminal.rs:16124 vs oracle Terminal.zig:8443, outside this shard's range).
- **Evidence:** Oracle Terminal.zig:11465 `var t = try init(io_impl, alloc, .{ .rows = 5, .cols = 10, .max_scrollback_bytes = 1024 });` vs rustty terminal.rs:16248 `let mut t = Terminal::init(Global, opts(10, 5)).unwrap();` with opts() setting `max_scrollback_bytes: Some(10_000)` at terminal.rs:7201.
- **Suggested fix:** Change terminal.rs:16248 to `Terminal::init(Global, opts_sb(10, 5, 1024))` (and likewise terminal.rs:16124 for the insertLines sibling).
- **Disputed by:** semantic lens (confidence high) — "Verified oracle ghostty/src/terminal/Terminal.zig:11465 uses max_scrollback_bytes=1024 and rustty crates/rustty-vt/src/terminal.rs:16248 uses opts(10,5) with max_scrollback_bytes=Some(10_000) (terminal.rs:7201). But that value is only an *explicit* byte limit which is floored by the effective minimum: PageList.zig:6942-6947 `max(.bytes) = @max(explicit, min)` with `min = minMaxSize(cols, rows)` (PageList.zig:6910, 7022-7062), and minMaxSize returns `PagePool.item_size * (pages_exact+1)` i.e. at least two standard pages (hundreds of KiB). Both 1024 and 10_000 are far below that floor, so the effective byte limit — and hence every pruning decision the test could observe — is byte-for-byte identical in the two configurations. No input/state produces a different result; test assertions are otherwise identical (finder concedes this). Cosmetic inconsistency with the sibling test at terminal.rs:15396 only."

#### S148-F2 — printSlice differential fuzz uses a 64-bit LCG instead of std.Random.DefaultPrng (xoshiro256++), so the 1700 exercised op sequences differ from the oracle's
- **Status:** DISPUTED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/Terminal.zig:14489-14501`
- **Rustty:** `crates/rustty-vt/src/terminal.rs:17462-17742`
- **Description:** The oracle seeds `std.Random.DefaultPrng.init(0xC0FFEE)` and draws via `intRangeAtMost`/`intRangeLessThan`/`boolean`/`int(u8)`. The port substitutes a private `TestRand` (Knuth MMIX LCG, `>> 17`, modulo-reduced ranges) with the same seed. The differential invariant, op alphabet, op weights, geometries (80x24, 10x4, 5x2, 2x2) and op counts (500,500,500,200) are identical, and the per-op assertions (cursor x/y/pending_wrap, full-screen dump equality, final verify_integrity on both pages) match one-to-one; but the concrete random sequence — and therefore which wrap/scroll/margin/grapheme edge cases are hit — differs from upstream, so a regression that upstream's exact sequence catches may not be reproduced here (and vice versa). The port documents this in a PORT NOTE.
- **Evidence:** Terminal.zig:14495-14496 `var prng = std.Random.DefaultPrng.init(0xC0FFEE); const rand = prng.random();` vs terminal.rs:17692-17721 `struct TestRand(u64)` with `wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407); self.0 >> 17` and `lo + (self.next() as usize) % (hi - lo + 1)`.
- **Suggested fix:** Port xoshiro256++ (std.Random.Xoshiro256 with SplitMix64 seeding) and Zig's intRangeAtMost/intRangeLessThan (Lemire-style `uintLessThan`) so the same seed yields the oracle's exact op sequence.
- **Disputed by:** semantic lens (confidence medium) — "Read Terminal.zig:14489-14501 and terminal.rs:17689-17742 plus the shared helper. Op alphabet, weights, geometries (80x24, 10x4, 5x2, 2x2), op counts (500/500/500/200) and every per-op assertion (cursor x/y, pending_wrap, full-screen dump equality, two verify_integrity walks) are identical; only the deterministic generator differs (xoshiro256++ vs an MMIX LCG, same 0xC0FFEE seed). The test asserts a differential invariant (print_slice must equal print) that holds for ANY input sequence, so the port's assertions are equal in meaning and no product behaviour differs — this is a test-input-generator substitution, not an observable functional difference or a weakened assertion."

#### S149-F2 — The six `log.warn("kitty graphics protocol error" / "glyph protocol error")` sites are dropped
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/apc.zig:8, 128, 136, 193, 202, 230, 241`
- **Rustty:** `crates/rustty-vt/src/apc/mod.rs:23, 238, 257, 344, 357, 414, 430`
- **Description:** The oracle logs a scoped (.terminal_apc) warning with the error value whenever the kitty or glyph parser fails in feed, feedSlice, or end. The port keeps the state transitions (ignore / None) but leaves the log calls as comments; no logging is emitted. Control flow and return values are otherwise identical.
- **Evidence:** Oracle apc.zig:127-131 `p.feed(byte) catch |err| { log.warn("kitty graphics protocol error: {}", .{err}); p.deinit(); self.state = .ignore; };` vs rustty apc/mod.rs:237-244 `if p.feed(byte).is_err() { // log.warn(...) ... self.state = State::Ignore; }`. Same shape at 257, 344, 357, 414, 430. `grep -rln 'warn!(' crates/rustty-vt/src` finds no logging in the crate.
- **Suggested fix:** Route the warnings through whatever logging seam rustty-vt adopts (or at least a `#[cfg(feature = "log")]`-gated `log::warn!`) so the diagnostic text is preserved.

#### S149-F3 — Private debug helper dumpBitmaps is omitted
- **Status:** DISPUTED · **Kind:** missing-function · **Severity:** low
- **Oracle:** `src/terminal/bitmap_allocator.zig:211-217`
- **Rustty:** `crates/rustty-vt/src/bitmap_allocator.rs:322-333`
- **Description:** `fn dumpBitmaps(self: *Self, base: anytype) void` (logs `bm={b} idx={}` per bitmap word via std.log.warn) has no Rust counterpart; the port preserves the body only as a comment. The function is private, never called by production or test code, so there is no observable behavioural effect, but it is a declaration with no counterpart.
- **Evidence:** Oracle bitmap_allocator.zig:211-217 defines `fn dumpBitmaps`; rustty bitmap_allocator.rs:322-333 is a PORT NOTE saying it 'is omitted to avoid pulling in a logging dependency'.
- **Suggested fix:** Add a `#[allow(dead_code)] fn dump_bitmaps(&self, base)` that formats each word as binary with its index (eprintln! or the crate's log seam).
- **Disputed by:** semantic lens (confidence medium) — "Oracle bitmap_allocator.zig:211-217 `fn dumpBitmaps` is private (`fn`, not `pub fn`) and, grepping the ghostty tree, is never called by production or test code; it only emits std.log.warn output. rustty bitmap_allocator.rs:321-333 preserves the body verbatim as a PORT NOTE comment. No input or state produces a different result, no caller exists, and the finder concedes 'no observable behavioural effect' — a difference confined to dead code, so refuted under the semantic-equivalence lens."

#### S155-F1 — PageEntry.hash uses std SipHash DefaultHasher instead of seed-0 Wyhash autoHash/autoHashStrat
- **Status:** DISPUTED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/hyperlink.zig:143-160`
- **Rustty:** `crates/rustty-vt/src/hyperlink.rs:355-377`
- **Description:** The oracle hashes a hyperlink PageEntry with `Wyhash.init(0)` + `autoHash(activeTag)` + `autoHash(implicit)` / `autoHashStrat(explicit slice, .Deep)` + `autoHashStrat(uri, .Deep)`. The port uses `std::collections::hash_map::DefaultHasher` (SipHash-1-3) with `write_u8(tag)` / `write_u32` / `write(bytes)`. The PORT NOTE asserts the hash is 'never oracle-compared / serialized' so any self-consistent hash is parity-preserving, but the hash decides the slot/PSL of every entry in the page's `hyperlink_set` RefCountedSet, and RefCountedSet's `add`/`addWithId` return `OutOfMemory` vs `NeedsRehash` vs success based on the PSL distribution (`psl_stats[31] > 0`, `psl >= max_psl` checks at ref_counted_set.zig:263-289, mirrored in ref_counted_set.rs:587-613). So the point at which `Screen.startHyperlink` must grow page capacity / rehash differs from the oracle for the same input stream, and the hyperlink_set item/table bytes inside the page mapping (which compress/page.rs LZ4-compresses) differ. The port's own hash_map.rs (lines 116-124) argues the opposite policy — that the AutoContext hash must be bit-exact because it determines raw page bytes — and style.rs:600-607 reproduces `std.hash.int` bit-exactly for the style set; the hyperlink set is the one page hash left non-faithful.
- **Evidence:** hyperlink.zig:144-159 `var hasher = Wyhash.init(0); autoHash(&hasher, std.meta.activeTag(self.id)); ... autoHashStrat(&hasher, self.uri.slice(base), .Deep); return hasher.final();` vs hyperlink.rs:361-376 `let mut hasher = std::collections::hash_map::DefaultHasher::new(); ... hasher.write_u8(1); hasher.write_u32(...)` / `hasher.write(s)`. Consumers: ref_counted_set.zig:263-289 OutOfMemory/NeedsRehash decisions depend on PSL.
- **Suggested fix:** Use `rustty_zigstd::wyhash::Wyhash::init(0)` and feed exactly what Zig autoHash/autoHashStrat(.Deep) write: the tag as its enum integer (u1/u8 per Zig's autoHash of an enum: it hashes @intFromEnum as the tag's backing int), the implicit u32 bytes, the explicit slice bytes with the slice length prefix that autoHashStrat(.Deep) on a []const u8 emits, then the uri likewise — and pin it with a golden test like HashMap_auto_context_matches_reference_wyhash_layout.
- **Disputed by:** semantic lens (confidence medium) — "Read ghostty/src/terminal/hyperlink.zig:143-160 (Wyhash.init(0) + autoHash/autoHashStrat) vs rustty crates/rustty-vt/src/hyperlink.rs:355-377 (DefaultHasher, write_u8 tag / write_u32 / write bytes). The difference is real but not observable: PageEntry.hash is consumed only by the page-local RefCountedSet (ref_counted_set.zig:250-300) to pick home slots and PSLs. Ids are handed out from next_id in insertion order, count()/refCount() (the only things Screen.zig tests assert, e.g. Screen.zig:4128,4321,11231) are hash-independent, and the set's OOM/NeedsRehash arms are either id-exhaustion-driven (next_id >= cap) or the statistically-unreachable psl_stats[31]>0 cold path. The raw page bytes that compress/page.rs LZ4s are never oracle-compared or persisted across versions, so a self-consistent hash preserves every observable result. Inconsistency with hash_map.rs's bit-exact policy is a style/doctrine issue, not a functional parity defect."

#### S155-F2 — KeyIterator/ValueIterator (FieldIterator) and keyIterator()/valueIterator() not ported
- **Status:** CONFIRMED · **Kind:** missing-function · **Severity:** low
- **Oracle:** `src/terminal/hash_map.zig:313-336, 415-445`
- **Rustty:** `crates/rustty-vt/src/hash_map.rs:642-644`
- **Description:** The oracle declares `pub const KeyIterator = FieldIterator(K)`, `pub const ValueIterator = FieldIterator(V)`, the `FieldIterator(T)` struct with its `next()` (len/metadata/items walk returning `?*T`), and the `keyIterator()`/`valueIterator()` constructors (with the metadata==null -> len 0 branch). None exist in hash_map.rs; only `iterator()` (Entry iterator) is ported. No oracle caller uses these on the offset map (the `keyIterator` in kitty/graphics_storage.zig:783 is on a std.AutoHashMapUnmanaged), so no product behaviour is affected — public API surface only.
- **Evidence:** grep for key_iterator/value_iterator/KeyIterator/FieldIterator in crates/rustty-vt/src/hash_map.rs returns nothing; oracle hash_map.zig:313-336 and 415-445 define them.
- **Suggested fix:** Add `FieldIterator<T>` with `next(&mut self) -> Option<*mut T>` plus `key_iterator()`/`value_iterator()` on HashMapUnmanaged mirroring lines 316-336 and 415-445.

#### S155-F3 — removeByPtr zero-sized-key fallback scans for the used slot instead of using index 0 (deliberate divergence)
- **Status:** DISPUTED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/hash_map.zig:901-912`
- **Rustty:** `crates/rustty-vt/src/hash_map.rs:1119-1160`
- **Description:** For `@sizeOf(K) == 0` the oracle removes index 0 unconditionally; the port scans metadata for the single used slot and removes that (and `expect`s one exists). The port documents this as a deliberate fix (W24-B03-ZST-01: with seed-0 Wyhash the ZST key's home slot is 9 at capacity 16, so the oracle would corrupt `size`). No product caller uses a ZST key (both page maps key on Offset(Cell)), so it is unobservable in the product, but it is a documented functional divergence with an additive test (`HashMap_removeByPtr_true_zero_sized_key`) pinning the divergent behaviour.
- **Evidence:** hash_map.zig:906-909 `const idx = if (@sizeOf(K) > 0) (...) / @sizeOf(K) else 0;` vs hash_map.rs:1142-1157 scanning `for i in 0..capacity { if is_used() { found = Some(i); break } }` + `found.expect(...)`.
- **Suggested fix:** Keep as documented divergence (it is a strict correctness improvement on an unreachable path) or restore `0` for strict fidelity; either way record it in the deferral/divergence ledger.
- **Disputed by:** semantic lens (confidence medium) — "Both sides read: hash_map.zig:901-912 uses idx=0 for @sizeOf(K)==0; hash_map.rs:1119-1160 scans metadata for the single used slot with an expect(). The divergence exists only on the size_of::<K>()==0 branch, and every product/test caller keys on Offset(Cell)/u32 (the oracle's own 'removeByPtr 0 sized key' test uses i32), so the branch is unreachable in both trees. Per the lens, a difference confined to unreachable code is refuted; it is documented in-place (W24-B03-ZST-01) and is a strict correctness improvement."

#### S155-F4 — expectCanonical ported as cfg(debug_assertions)-gated debug_assert_canonical, a no-op in release test runs
- **Status:** CONFIRMED · **Kind:** weakened-assertion · **Severity:** low
- **Oracle:** `src/terminal/hash_map.zig:1028-1046, 1476, 1527, 1564, 1616`
- **Rustty:** `crates/rustty-vt/src/hash_map.rs:1187-1217, 1951, 1999, 2039, 2104`
- **Description:** The oracle's four tests call the test-only `expectCanonical(&map, ctx)` which always asserts (via `expect`) the canonical-placement invariant. The port routes those calls to `debug_assert_canonical`, which returns immediately when `!cfg!(debug_assertions)` and uses `debug_assert!` inside. Under `cargo test` (debug assertions on) the assertions hold; under `cargo test --release` (or any profile with debug-assertions off) the four tests silently lose that assertion, unlike the oracle where the check is unconditional in tests.
- **Evidence:** hash_map.rs:1188-1190 `if !cfg!(debug_assertions) { return; }` and 1208/1216 `debug_assert!`/`debug_assert_eq!`; oracle hash_map.zig:1031-1046 uses `try expect(...)`/`try expectEqual(...)`.
- **Suggested fix:** Add a test-module `expect_canonical` helper that always asserts (plain assert!/assert_eq!) and call it from the four tests, keeping debug_assert_canonical for the production invariant.

#### S155-F5 — Randomized tests use different PRNGs (LCG / splitmix64) than std.Random.DefaultPrng (xoshiro256++), and the PORT NOTE misdescribes the replacement
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/hash_map.zig:1280-1316, 1567-1617`
- **Rustty:** `crates/rustty-vt/src/hash_map.rs:1718-1729, 2044-2070`
- **Description:** `HashMap put and remove loop in random order` uses an LCG Fisher-Yates shuffle instead of DefaultPrng.init(0)+random.shuffle, and `HashMap random operations against an oracle` uses splitmix64 seeded 0xdeadbeef while its PORT NOTE claims 'an inline xoshiro256++ with the same seeding discipline'. The assertion set and count are identical and the tests are differential, so this is not a weakened assertion, but the exercised operation sequence differs from the oracle's and the note is inaccurate.
- **Evidence:** hash_map.rs:2044-2048 PORT NOTE 'inline xoshiro256++' vs 2062-2070 body implementing splitmix64 (`0x9E37_79B9_7F4A_7C15`, `0xBF58_476D_1CE4_E5B9`); oracle 1300-1301 / 1580-1581 `std.Random.DefaultPrng.init(...)`.
- **Suggested fix:** Either port xoshiro256++ with Zig's DefaultPrng seeding (splitmix64 seed expansion) so the sequence matches, or correct the PORT NOTE to say splitmix64.

#### S155-F6 — Flattened.init/clone return Result<_, OutOfMemory> but can never yield Err (Vec::push/with_capacity abort on OOM)
- **Status:** CONFIRMED · **Kind:** missing-error-path · **Severity:** low
- **Oracle:** `src/terminal/highlight.zig:141-172`
- **Rustty:** `crates/rustty-vt/src/highlight.rs:168-214`
- **Description:** The oracle's `Flattened.init` (`try result.append(alloc, ...)` with errdefer deinit) and `clone` (`try self.chunks.clone(alloc)`) propagate `Allocator.Error.OutOfMemory` to callers (search/thread.zig, search/terminal.zig collect these via `try`). The port keeps the `Result<Flattened, OutOfMemory>` signature but uses `Vec::push` / `Vec::with_capacity`, which abort the process on allocation failure, so the Err arm is unreachable. hyperlink.rs:98-130 in this same shard treats the identical pattern (`Hyperlink.dupe`) as a contract violation and fixed it with `try_reserve_exact`; Flattened was not given the same treatment.
- **Evidence:** highlight.rs:172-181 `let mut result: Vec<Chunk> = Vec::new(); ... result.push(Chunk{...})` and 200-208 `Vec::with_capacity(...)`/`push`, always `Ok(...)`; oracle highlight.zig:146-149 `errdefer result.deinit(alloc); ... try result.append(alloc, ...)` and 168 `try self.chunks.clone(alloc)`.
- **Suggested fix:** Use `try_reserve`/`try_reserve_exact` before pushing (mapping TryReserveError to OutOfMemory) as hyperlink.rs's try_dupe_bytes does.

#### S155-F7 — Verbatim comment blocks for main.zig's root test and lib.zig are stale (omit compress/fraction/mem and TinyIo lines)
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/main.zig:93-104`
- **Rustty:** `crates/rustty-vt/src/lib.rs:330-341, 288-296`
- **Description:** lib.rs carries main.zig's root `test {}` block as a verbatim comment listing only bitmap_allocator, hash_map, ref_counted_set, size; the oracle at pin 20abdb50a also references compress.zig, fraction.zig and mem.zig (all three modules ARE ported and declared as pub mod, so this is doc-only). Likewise the lib.zig verbatim block at lib.rs:288-296 omits `pub const TinyIo = lib.TinyIo;` (oracle lib.zig:17; the TinyIo shim itself exists at rustty-vt-c/src/io.rs).
- **Evidence:** lib.rs:330-338 comment vs main.zig:93-104; lib.rs:288-296 vs lib.zig:15-24.
- **Suggested fix:** Refresh the two verbatim comment blocks to the pinned oracle text.

#### S156-F3 — madvise failure warnings not logged
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/mem.zig:109,129,169`
- **Rustty:** `crates/rustty-vt/src/mem.rs:189,214,262`
- **Description:** The oracle logs `madvise(DONTNEED) failed err={}`, `madvise(FREE_REUSABLE) failed err={}` and `madvise(FREE_REUSE) failed err={}` under the `terminal_mem` scope on the respective failure paths. rustty keeps these as commented-out lines; the control flow (return false / fall through to memset) is identical, only the diagnostic is lost.
- **Evidence:** mem.zig:109/129/169 `log.warn(...)`; mem.rs:189, 214, 262 `// log.warn(...)` comments with no logging call; `grep -rln 'log::warn|tracing::' crates/rustty-vt/src` is empty.
- **Suggested fix:** Wire a logging facade (e.g. the `log` crate) into rustty-vt and emit the three warnings, including the errno.

#### S156-F4 — ensureAllocator warning not logged
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/osc.zig:439-444`
- **Rustty:** `crates/rustty-vt/src/osc/mod.rs:895-906`
- **Description:** Oracle `ensureAllocator` logs `An allocator is required to process OSC {t} but none was provided.` (scoped `.osc`) before setting the state invalid. rustty's `ensure_allocator` sets the state invalid identically but the log call is a comment.
- **Evidence:** osc.zig:442 `log.warn(...)`; osc/mod.rs:903 `// log.warn(...)`.
- **Suggested fix:** Emit the warning through a logging facade once one is wired into rustty-vt.

#### S157-F1 — verifyIntegrity log.warn diagnostics (with y/x/id/expected/actual details) are dropped
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/page.zig:405-611`
- **Rustty:** `crates/rustty-vt/src/page_impl.rs:586-746`
- **Description:** The reference emits a `log.warn("page integrity violation ...")` line naming the row/column (and for ref-count mismatches the id/expected/actual counts) before returning each IntegrityError; assertIntegrity additionally `log.err`s before panicking. The port returns the same error variants but every log call is reduced to a comment, so the diagnostic output (which is the only user-visible trace when the error is returned rather than panicked, e.g. from deserialization callers) is absent. The panic message in assert_integrity does include the error variant, so only the warn-level detail is lost.
- **Evidence:** page.zig:405-406 `log.warn("page integrity violation zero row count", .{}); return IntegrityError.ZeroRowCount;` and :433-437 `log.warn("page integrity violation y={} x={} grapheme data missing", .{ y, x })` vs page_impl.rs:586-589 `// log.warn(...)` + `return Err(IntegrityError::ZeroRowCount)` and :617-620 where the y index is dropped entirely (`for row in rows.iter()`, PORT NOTE at :606-607). Same for the ref-count mismatch warnings at page.zig:582-604 vs page_impl.rs:730-747.
- **Suggested fix:** Emit the equivalent `log::warn!` lines (with y, x, id, expected, actual) before each `return Err(...)` and a `log::error!` in assert_integrity before the panic.

#### S158-F1 — Cell doc comment states a stale bit layout that contradicts the constants
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/page.zig:2087-2130`
- **Rustty:** `crates/rustty-vt/src/page.rs:574-586`
- **Description:** The Cell struct doc comment says style_id is bits 23..38, wide 39..40, protected 41, hyperlink 42, semantic_content 43..44, padding 45..60, but the actual constants (and the Zig packing with a u24 content union) put style_id at 26..41, wide 42..43, protected 44, hyperlink 45, semantic_content 46..47, padding 48..63. The code is correct; only the comment is wrong. Doc-only.
- **Evidence:** page.rs:574-586 doc lists 'bits 23..38 style_id ... bits 45..60 _padding'; page.rs:592-600 define CELL_STYLE_ID_SHIFT = 26, CELL_WIDE_SHIFT = 42, CELL_PROTECTED_SHIFT = 44, CELL_HYPERLINK_SHIFT = 45, CELL_SEMANTIC_CONTENT_SHIFT = 46, CELL_PADDING_SHIFT = 48, matching page.zig:2087-2130 (content_tag u2, content u24, style_id u16, wide u2, protected, hyperlink, semantic_content u2, _padding u16).
- **Suggested fix:** Rewrite the doc block to the 26/42/44/45/46/48 offsets (or delete the duplicated table and point at the CELL_* constants and cell_field_bit_offsets test).

#### S161-F1 — Pin/serial skip test omits the last-row serial comparison
- **Status:** CONFIRMED · **Kind:** missing-branch · **Severity:** low
- **Oracle:** `src/terminal/render.zig:599-615`
- **Rustty:** `crates/rustty-vt/src/render.rs:1037-1055`
- **Description:** The oracle rewrites the chunk's pins+serials if ANY of seven conditions fails, including `row_serials[y + take - 1] != node_serial` (line 605). rustty's condition (lines 1038-1044) checks redraw, first-row node/y/serial and last-row node/y, but not the last-row serial. If the trailing rows of a chunk hold a stale serial for the same node pointer (node address reuse after page free/realloc where only the head rows were refreshed), rustty skips the rewrite and leaves stale serials, so `update_highlights_flattened` (which gates on `chunk.serial != row_serial`) would reject valid highlights for those rows where the oracle would accept them.
- **Evidence:** render.zig:599-605 lists `row_serials[y + take - 1] != node_serial` as the 7th disjunct; render.rs:1038-1044 has only 6 disjuncts and never reads `self.row_data[y + take - 1].serial`.
- **Suggested fix:** Add `|| self.row_data[y + take - 1].serial != node_serial` to the condition at render.rs:1038-1044.

#### S161-F2 — string() writes surrogate codepoints as WTF-8 bytes instead of U+FFFD
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/render.zig:991-999`
- **Rustty:** `crates/rustty-vt/src/render.rs:1524-1548`
- **Description:** The oracle formats each codepoint with `writer.print("{u}", ...)`, which in the pinned Zig 0.16 std (`Io/Writer.printUnicodeCodepoint`) replaces `Utf8CannotEncodeSurrogateHalf`/`CodepointTooLarge` with the 3-byte replacement character EF BF BD. rustty's `write_codepoint` instead emits a raw 3-byte generalized-UTF-8 encoding of the surrogate (e.g. 0xD800 -> ED A0 80). Byte count is the same (so the StringMap offsets agree) but the emitted bytes differ and the rustty output is invalid UTF-8.
- **Evidence:** /opt/homebrew/Cellar/zig/0.16.0_1/lib/zig/std/Io/Writer.zig:1573-1582 substitutes `std.unicode.replacement_character_utf8` on surrogate error; render.rs:1531-1545 pushes `0xE0|..`, `0x80|..`, `0x80|..` from the surrogate value. The rustty comment at 1520-1523 asserts Zig `{u}` is 'permissive', which the std source contradicts.
- **Suggested fix:** In `write_codepoint`'s `None` arm, emit `[0xEF, 0xBF, 0xBD]` when `utf8_codepoint_sequence_length(cp)` is Some(3) (surrogate), keeping the Err for out-of-range.

#### S163-F1 — vtRaw error arm (`catch` -> warn + skip) has no channel in the Rust `vt_raw` hook
- **Status:** DISPUTED · **Kind:** missing-error-path · **Severity:** low
- **Oracle:** `src/terminal/stream.zig:1289-1301`
- **Rustty:** `crates/rustty-vt/src/stream.rs:1978-1989`
- **Description:** The oracle's `vtRaw` returns `!bool`; on error the stream logs `error handling action manually err=... action=...` and forces skip=true so the action is NOT processed by the stream. The Rust `Handler::vt_raw` returns a plain `bool`, so an implementer cannot signal failure. The only in-tree implementer (rustty-inspector VTHandler) swallows its resize/append errors with `let _ =` and returns `false` (crates/rustty-inspector/src/widgets/termio.rs:1826-1846), so after an allocation failure the stream would process the action (oracle: skip). Practically unreachable because Rust allocation failure aborts, hence low.
- **Evidence:** stream.zig:1290-1298 `const skip = self.handler.vtRaw(action) catch |err| err: { log.warn(...); break :err true; };` vs stream.rs:1987 `if self.handler.vt_raw(&action) { continue; }` with trait signature `fn vt_raw(&mut self, _action: &parser::Action) -> bool` (stream.rs:325).
- **Suggested fix:** Either make `vt_raw` return `Result<bool, E>` and map `Err` to skip=true (matching the oracle), or document the folded semantic on the trait and have the inspector return `true` on its internal failure paths.
- **Disputed by:** semantic lens (confidence medium) — "Read both sides. Oracle stream.zig:1289-1301 folds any vtRaw error into skip=true; rustty stream.rs:1978-1989 calls the infallible trait method (stream.rs:325-327 default false) with an explicit PORT NOTE recording the folded semantic. The only fallible paths in the sole implementer are allocation ones: oracle inspector/widgets/termio.zig:828-880 can only fail via `try VTEvent.init(alloc,...)` / `vt_events.resize`/`append` OOM; in the Rust port (rustty-inspector/src/widgets/termio.rs:1816-1845) Rust's global allocator aborts on OOM, so the differing arm is unreachable rather than a different result. No non-OOM input produces a different outcome, so the behaviours are equivalent in reachable code."

#### S163-F2 — All `log.warn`/`log.info` diagnostics in the stream fast paths and CSI dispatch are dropped (no log facade in rustty-vt)
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/stream.zig:1352-1356, 1372, 1435, 1441-1444, 1456, 1462, 1478, 1484, 1499, 1505, 1520, 1528, 1543, 1551, 1566, 1572, 1590, 1597, 1610, 1615, 1630, 1641, 1665, 1677, 1688, 1700, 1705, 1718, 1723, 1735, 1740, 1753, 1758, 1770, 1775, 1807, 1816`
- **Rustty:** `crates/rustty-vt/src/stream.rs:2072, 2093, 2173-2620`
- **Description:** Every oracle warning in this range (invalid cursor/CUP/HPA/IL/DL/DCH/SU/SD/CHT/erase commands, 'ignoring unimplemented CSI x with intermediates', 'CSI colon or mixed separators only allowed for m', 'executing C1 0x.. as ESC ..', 'invalid cursor tabulation control') is present only as a commented-out line in the port; no message is emitted. `grep -c 'log::|tracing::' stream.rs` = 0. Control flow (early returns, ignored arms) is identical, so this is diagnostic-output only. The `logUnsupportedOnce` sites keep their dedup table but likewise emit nothing.
- **Evidence:** e.g. stream.zig:1352-1356 `log.warn("CSI colon or mixed separators only allowed for 'm' command, got: {f}", ...)` vs stream.rs:2071-2074 `if c != b'm' && p.params_sep.count() > 0 { // log.warn(...); return; }`; stream.zig:1372 `log.info("executing C1 0x{x} as ESC {c}", ...)` vs stream.rs:2092-2098 (no log).
- **Suggested fix:** Wire a `log` facade (e.g. the `log` crate scoped `stream`) and emit the same messages at the same points; make `log_unsupported_once` callers emit under the returned bool.

#### S163-F3 — Action C-ABI projections (Tag/Value/C/CValue/cval and per-payload `.C`/`cval`) have no Rust counterpart
- **Status:** DISPUTED · **Kind:** missing-function · **Severity:** low
- **Oracle:** `src/terminal/stream.zig:237-249, 252-262, 272-283, 285-296, 322-330, 332-340, 352-360, 362-370, 372-380, 382-397, 399-414, 416-432, 439-449`
- **Rustty:** `crates/rustty-vt/src/stream.rs:102-128`
- **Description:** The oracle derives a C tagged union for `Action` (`lib.TaggedUnion` with `[16]u64` padding) and per-payload extern projections (`Print.C{cp:u32}`, `PrintSlice.C{cps,len}`, `ApcPutSlice.C{bytes,len}`, `DeviceStatus.C=u16` via @bitCast, `Mode.C=u16`, `KittyKeyboardFlags.C=u8`, `WindowTitle.C=lib.String`, `ShowDesktopNotification.C`, `StartHyperlink.C` with `id orelse ""`, `ClipboardContents.C{kind,data}` dropping terminator, `ColorOperation.C=void`). rustty's PORT NOTE says the mirror 'lives in rustty-vt-c', but rustty-vt-c only mirrors the `UnknownSequence` union (terminal.rs:577-611), not the stream `Action`. At pin 20abdb50a no oracle use-site consumes `stream.Action.C`/`cval` (only `.Tag`/`.Value` are used, for comptime dispatch), so nothing observable is missing today; recorded so the PORT NOTE's claim is not taken as covered.
- **Evidence:** stream.zig:238-249 `const c_union = lib.TaggedUnion(lib.target, @This(), .{ .padding = [16]u64 }); pub const Tag/Value/C/CValue/cval` vs stream.rs:120-127 PORT NOTE 'The C-ABI mirror lives in the rustty-vt-c crate' — `grep -rn 'PrintSlice\|ApcPutSlice' crates/rustty-vt-c` returns nothing.
- **Suggested fix:** Either add the `Action` C projection to rustty-vt-c when a C consumer appears, or correct the PORT NOTE to state that the projection is intentionally not ported because no use-site exists.
- **Disputed by:** semantic lens (confidence high) — "Confirmed the code facts (oracle stream.zig:238-296 c_union Tag/Value/C/CValue/cval plus per-payload `.C`/`cval`; rustty stream.rs:104-128 keeps them only as commented reference with the 'C-ABI mirror lives in rustty-vt-c' note; `grep -rn 'PrintSlice|ApcPutSlice|cps' crates/rustty-vt-c/src` finds no Action mirror). But the projection is dead in the oracle at pin 20abdb50a: `grep -rn 'Stream.Action|stream.Action' ghostty/src` yields only `.Tag`/`.Value` comptime-dispatch uses (benchmark/ApcParser.zig:48-49, inspector/widgets/termio.zig:820-821, termio/stream_handler.zig:182-193) and no header in include/ghostty/vt exposes a stream Action C type. No input or C consumer can observe a difference; the residue is an inaccurate PORT NOTE sentence, i.e. doc-only. Difference confined to unreachable/unused code -> refuted."

#### S164-F1 — All log.warn/log.err/log.debug emissions in csi/osc/esc dispatch and logUnsupportedOnce are dropped (no logging facade)
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/stream.zig:1785-3079`
- **Rustty:** `crates/rustty-vt/src/stream.rs:2577-4242`
- **Description:** Every diagnostic the oracle emits in this range (e.g. 'invalid cursor tabulation control', 'invalid DECSTBM command', 'ignoring CSI t with no parameters' at log.err level, 'unimplemented OSC callback' at debug, 'invalid OSC, should never happen', and the tail `log.warn(format, args)` of logUnsupportedOnce at stream.zig:3078) is preserved only as a `//` comment in the port. `log_unsupported_once` (stream.rs:4187) returns a bool that all 25 call sites discard, so the dedup table is maintained but nothing is ever logged. Handler dispatch and control flow are otherwise identical, so this is cosmetic/observability only. Documented as a workspace-wide decision in the PORT NOTE at stream.rs:4093-4103.
- **Evidence:** stream.zig:3078 `log.warn(format, args);` vs stream.rs:4214 `Ok(_) => return true` with e.g. stream.rs:2832 `log_unsupported_once(&SEEN_UNIMPLEMENTED_MODE, mode_int);` (result unused); stream.zig:2439-2442 `log.err("ignoring CSI t with no parameters: {f}", ...)` vs stream.rs:3377 comment only.
- **Suggested fix:** Wire a logging facade (e.g. the `log` crate) into rustty-vt and emit the preserved messages at the same levels; emit under the boolean returned by log_unsupported_once.

#### S165-F1 — 'stream: continuation lifecycle' is wholly #[ignore]d in release builds, not just its FailingAllocator arm
- **Status:** DISPUTED · **Kind:** weakened-assertion · **Severity:** low
- **Oracle:** `src/terminal/stream.zig:4850-4906`
- **Rustty:** `crates/rustty-vt/src/stream.rs:6845-6932`
- **Description:** The oracle test always runs all five arms (ContinuationDisabled for no-allocator/zero-capacity/no-max, empty continuation after ground input, WriteFailed on a 1-byte writer, ContinuationUnavailable under a failing allocator). The rustty port marks the ENTIRE test `#[cfg_attr(not(debug_assertions), ignore)]` because only the last arm needs the debug-only TRACKER_TW tripwire; the four allocator-independent arms are therefore skipped in a `cargo test --release` run. Same pattern for 'stream: continuation allocation failure recovers' (7350-7386), which is entirely tripwire-dependent and so is a legitimate whole-test gate.
- **Evidence:** stream.rs:6846-6850 `#[cfg_attr(not(debug_assertions), ignore = "the FailingAllocator arm needs the TRACKER_TW tripwire...")]` on fn stream_continuation_lifecycle, whereas only the block at 6905-6927 is `#[cfg(debug_assertions)]`. Oracle stream.zig:4850-4906 has no such gate.
- **Suggested fix:** Drop the cfg_attr ignore and keep only the `#[cfg(debug_assertions)]` block guard on the failing-allocator arm so the other four assertions run in every configuration.
- **Disputed by:** semantic lens (confidence medium) — "Read both regions: ghostty/src/terminal/stream.zig:4830-4910 (five arms) vs rustty crates/rustty-vt/src/stream.rs:6838-6928. Every oracle assertion has an exact counterpart with identical meaning: ContinuationDisabled x3 (6851-6878), empty continuation with end==0 (6888-6892), WriteFailed on the 1-byte writer (6894-6900), ContinuationUnavailable under the tripwire-driven alloc failure (6904-6926). In the parity test configuration (cargo test, debug — the analogue of zig build test's default Debug, and the configuration in which the TRACKER_TW tripwire exists at all, mirroring Zig's builtin.is_test/FailingAllocator gating) all five arms run with the same assertions. The cfg_attr only affects a non-default `--release` run; it is test-harness hygiene, not an observable functional difference nor a weakened/looser assertion. Splitting the gate would be a nicety, not a parity defect."

#### S166-F1 — Clipboard read/write reply side effects are applied after the effect callback returns instead of inside `reply`
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/stream_terminal.zig:738-760, 927-960, 1206-1234`
- **Rustty:** `crates/rustty-vt/src/stream_terminal.rs:907-951, 1083-1118, 1261-1339, 2922-3184`
- **Description:** In the oracle, ClipboardReadState.reply / KittyClipboardReadState.reply write the pty response (and record the session grant) synchronously inside the embedder's `reply` call, and KittyClipboardWriteReplyState.reply records the write grant inside `reply`. The port's reply functions only encode into the stack state (`response: Vec<u8>`, `remember: bool`) and the handler writes the pty / records the grant after the effect callback returns (SB-discipline PORT NOTE). The emitted bytes and grant contents are identical, but an embedder callback that, after calling `reply`, inspects `handler.kitty_clipboard_grants` or emits its own pty data through its write_pty function sees a different ordering (grant not yet recorded; its own bytes precede the protocol response instead of following it).
- **Evidence:** Oracle stream_terminal.zig:753-759 `self.respond(data) catch ...; self.replied = true;` and :945-956 `grants.grant(...)` then `respondSuccess` execute during the callback; rustty stream_terminal.rs:938-950 and :1110-1118 apply `grant` and `write_pty` only after `func(self, ...)` returns; :1325-1339 likewise for the write grant.
- **Suggested fix:** Document the deferral as an intentional contract in the Effects doc comment, or apply the grant/pty write from inside `reply` via a raw handler pointer in the state (as the oracle does) if the SB/Miri harness allows it.

#### S166-F2 — Two oracle tests only run in debug builds (ignored / cfg'd out in release)
- **Status:** CONFIRMED · **Kind:** missing-test · **Severity:** low
- **Oracle:** `src/terminal/stream_terminal.zig:2075-2109, 2178-2209`
- **Rustty:** `crates/rustty-vt/src/stream_terminal.rs:4153-4155, 4279-4286`
- **Description:** `resize failure preserves terminal state and does not write` is `#[cfg_attr(not(debug_assertions), ignore)]` and `semantic failure is sticky while processing continues` is `#[cfg(debug_assertions)]` because the tripwire fail points that stand in for `testing.FailingAllocator` are debug-only. The oracle tests run in every test build. Assertions are otherwise identical.
- **Evidence:** rustty stream_terminal.rs:4154 `#[cfg_attr(not(debug_assertions), ignore)]`, :4285 `#[cfg(debug_assertions)]`; oracle tests at stream_terminal.zig:2075 and :2178 carry no such gate.
- **Suggested fix:** Make the tripwire modules available under a `cfg(test)` (not `debug_assertions`) gate so both tests run under `cargo test --release` too.

#### S166-F3 — All `log.warn` diagnostics in the handler are dropped (commented out)
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/stream_terminal.zig:344-347, 470-477, 506, 512, 675, 726, 741, 756-757, 930, 952, 957, 1209, 1228, 1445, 1471, 1489, 1530`
- **Rustty:** `crates/rustty-vt/src/stream_terminal.rs:674-680, 879, 2508, 2530, 2564, 2642, 2931, 3030, 3164, 3844, 3887, 3899`
- **Description:** Every warning the oracle logs (VT action error with action/err, OSC color / Kitty color / clipboard / kitty dnd errors, invalid OSC 52 base64, clipboard read/write replied more than once, grant recording errors, size-report / mode-report encode errors, title and pwd truncation) is present only as a `// log.warn(...)` comment in the port; nothing is emitted through the `log` facade. Control flow is identical.
- **Evidence:** Oracle :346 `log.warn("error handling VT action action={} err={}", ...)` vs rustty :677-678 sets `semantic_failure` with the warn commented out; oracle :1471 `log.warn("title length {d} exceeds max length {d}, truncating", ...)` vs rustty :2530 comment only; same pattern at each cited line.
- **Suggested fix:** Emit the warnings via the workspace `log` crate (`log::warn!`) at each site with the same message text.

#### S167-F1 — DCS command memory-release test has no leak check outside Miri
- **Status:** CONFIRMED · **Kind:** weakened-assertion · **Severity:** low
- **Oracle:** `src/terminal/stream_terminal.zig:2607-2625`
- **Rustty:** `crates/rustty-vt/src/stream_terminal.rs:4763-4791`
- **Description:** The oracle test 'DCS command memory is released' has no explicit expect calls; its assertion is testing.allocator's automatic leak detection on deinit (for both the completed and the incomplete DCS command). The rustty counterpart replays the same two sequences but, per its own PORT NOTE, has no leak-detecting allocator, so in a plain cargo test run the test cannot fail on the leak the oracle guards against; the check only exists under Miri or an external leak checker.
- **Evidence:** Oracle 2607-2625: `s.nextSlice("\x1BP+q536D756C78\x1B\\"); s.nextSlice("\x1BP+q536D756C78"); s.deinit();` under testing.allocator. rustty 4763-4769 PORT NOTE: 'rustty has no leak-detecting test allocator here ... observable under Miri / a leak checker'; the test body (4771-4791) contains no assertion.
- **Suggested fix:** Wrap the handler's DCS command buffer allocations in the crate's tripwire/alloc-counting seam (as done for CLIPBOARD_CONTENTS_TW) and assert the live-allocation count returns to zero after both the completed command and Handler::deinit, so the leak is caught in the normal test lane.

#### S170-F2 — format_wire panics on non-UTF-8 `.other`/clear reason bytes instead of writing them through
- **Status:** DISPUTED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/apc/glyph/response.zig:124, 199-211`
- **Rustty:** `crates/rustty-vt/src/apc/glyph/response.rs:174, 184-186, 319-321`
- **Description:** The oracle `formatWire` writes the `Reason.other` and `Clear.reason` byte slices verbatim via `writer.writeAll`. The port stores them as Vec<u8> (faithful) but the writer is `std::fmt::Write`, so both paths go through `std::str::from_utf8(..).expect(..)`, which panics on non-UTF-8 bytes rather than emitting them. Not reachable from the executor (it only constructs ASCII reasons), but the public Response API differs for caller-constructed values.
- **Evidence:** Oracle response.zig:203 `try writer.writeAll(reason.name());` / :210 `try writer.writeAll(reason);` (byte slices). Port response.rs:320 `std::str::from_utf8(value).expect("reason name is valid UTF-8")` and :185 `std::str::from_utf8(reason).expect("clear reason is valid UTF-8")`.
- **Suggested fix:** Make format_wire write to a byte sink (e.g. `&mut impl std::io::Write` or `&mut Vec<u8>`) and emit the reason bytes directly, or fall back to a lossy write instead of expect().
- **Disputed by:** semantic lens (confidence medium) — "Read oracle src/terminal/apc/glyph/response.zig:203 (`writer.writeAll(reason.name())`) and :210 (`writer.writeAll(reason)`) against port crates/rustty-vt/src/apc/glyph/response.rs:174/184-186 and :319-321. The expect()-on-from_utf8 panic the finder describes is real in the code, but it is confined to unreachable state: every construction site in the workspace produces ASCII. Grepped `ResponseRegisterReason::Other` across crates — only response.rs:319 (the accessor itself), response.rs:547 (test, b"future_reason") and execute.rs:102 (b"out_of_memory"); ResponseClear.reason is likewise only set from ASCII by the executor, exactly as in the oracle, which never synthesizes non-ASCII reasons either. No terminal input or protocol path can produce non-UTF-8 reason bytes, so no input/state yields a different observable result; the difference exists only for a hypothetical external constructor of the Rust type, which is not a Ghostty parity path (unreachable-code criterion)."

#### S172-F1 — key_encode setopt invalid-value warnings bypass the C log callback and print unconditionally to stderr
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/c/key_encode.zig:82-87, 115-120`
- **Rustty:** `crates/rustty-vt-c/src/key_encode.rs:171-175, 239-243`
- **Description:** The reference emits `log.warn("setopt invalid option value={d}")` / `"setopt invalid OptionAsAlt value={d}"` through `std.log`, which for the C library is routed by `std_options` to `terminal/c/sys.zig logFn`: nothing is emitted when no log callback is installed, otherwise the message is delivered to the embedder's callback with level warn and scope `key_encode`. rustty uses `eprintln!`, so the message always goes to stderr and never reaches the installed `rustty_sys_set(LOG)` callback. The sibling key_event guards in `lib.rs:627-635/661-669` correctly use `sys::log_fn`, so the funnel exists and this is an inconsistency. (The oracle warns additionally sit behind `std.debug.runtime_safety`, so a release build emits nothing at all.)
- **Evidence:** oracle `:84`, `:117`; rustty `key_encode.rs:173`, `:241`; `sys.zig:209` `if (global.log == null) return;`.
- **Suggested fix:** Replace both `eprintln!` calls with `crate::sys::log_fn(sys::StdLevel::Warn, "key_encode", ...)`.

#### S174-F5 — terminal_paste mimes-array allocation failure aborts instead of returning out_of_memory
- **Status:** CONFIRMED · **Kind:** missing-error-path · **Severity:** low
- **Oracle:** `src/terminal/c/paste.zig:62-67`
- **Rustty:** `crates/rustty-vt-c/src/paste.rs:138-147`
- **Description:** The oracle allocates the mimes slice via a stack-fallback allocator over the terminal gpa and maps allocation failure to `return .out_of_memory`, so a C caller can observe RUSTTY_OUT_OF_MEMORY from a large mimes list under memory pressure. rustty collects into a `Vec<&[u8]>`, whose allocation failure aborts the process (documented DG5), so that error return is unreachable.
- **Evidence:** oracle src/terminal/c/paste.zig:63-66 `var sfa = std.heap.stackFallback(256, wrapper.terminal.gpa()); const alloc = sfa.get(); const mimes = alloc.alloc([]const u8, c_mimes.len) catch return .out_of_memory;` vs crates/rustty-vt-c/src/paste.rs:138-147 (PERF(port) note + `let mimes: Vec<&[u8]> = c_mimes.iter().map(...).collect();`). The paste call's own OutOfMemory arm (paste.rs:207) is carried, only the array allocation's arm is dropped.
- **Suggested fix:** Use a fallible allocation (try_reserve / Vec::try_with_capacity equivalent) and return Result::OutOfMemory on failure, or record the DG5 exemption explicitly on this line.

#### S176-F2 — `search::new` drops the registry-insert out-of-memory path
- **Status:** DISPUTED · **Kind:** missing-error-path · **Severity:** low
- **Oracle:** `src/terminal/c/search.zig:135-145`
- **Rustty:** `crates/rustty-vt-c/src/search.rs:389-400`
- **Description:** The oracle registers the new search in `terminal_wrapper.searches` with `putNoClobber(terminal_wrapper.terminal.gpa(), wrapper, {})` and, on allocation failure, destroys the freshly created wrapper and returns `.out_of_memory` (leaving `out.*` NULL as set at the top). The rustty port uses `HashSet::insert`, whose growth failure aborts the process rather than returning; the `.out_of_memory` result and the compensating `destroy` are therefore unreachable/absent.
- **Evidence:** oracle src/terminal/c/search.zig:137-145 (`... catch { alloc.destroy(wrapper); return .out_of_memory; }`) vs rustty crates/rustty-vt-c/src/search.rs:396-399 `let inserted = unsafe { (*terminal_wrapper).searches.insert(wrapper) }; debug_assert!(inserted);` with the PORT NOTE stating the catch arm "has no reachable analog".
- **Suggested fix:** Use a fallible insert (e.g. `try_reserve` before `insert`, or the allocator-aware hashbrown API) so a registry allocation failure destroys the wrapper and returns `Result::OutOfMemory` as the oracle does.
- **Disputed by:** semantic lens (confidence medium) — "Read oracle search.zig:137-145 and rustty search.rs:387-400. The `catch` arm the finder cites is only the ArrayHashMap's own growth allocation failing. In rustty the registry is a plain `HashSet<*mut SearchWrapper>` (terminal.rs:237, constructed at :1709) backed by the Rust global allocator, which aborts on allocation failure by language convention — a Rust-idiom equivalence for which no caller-observable alternative behavior exists at this call site, and the `putNoClobber` uniqueness assert is preserved as `debug_assert!(inserted)`. To the extent the oracle's `terminal.gpa()` is the embedder allocator, that is the very same F-008 Tier-2 allocator-threading residual already captured by S176-F1, not an independent defect; on its own this reduces to an unreachable error path, so it does not survive as a separate finding."

#### S178-F1 — log_stderr renders scope/message through String::from_utf8_lossy instead of raw bytes
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/c/sys.zig:249-285`
- **Rustty:** `crates/rustty-vt-c/src/sys.rs:600-660`
- **Description:** The reference `logStderr` formats the scope and message byte slices with Zig's `{s}` verb, which emits the bytes VERBATIM (`std.fmt.bufPrint(&buffer, "[{s}]({s}): {s}\n", .{ level_text, scope, message })`, and the piecewise fallback `stderr.write(scope)` / `stderr.write(message)`). The rustty port converts both byte slices with `String::from_utf8_lossy(...)` before printing. Any embedder-supplied scope or message containing non-UTF-8 bytes is therefore rewritten: each invalid byte/sequence becomes U+FFFD (EF BF BD), so the emitted stderr bytes and the line length differ from the reference. Log messages from `log_fn` are chunked at a fixed 2048-byte boundary, which can split a multi-byte UTF-8 scalar across two callback invocations — so the mangling is reachable from the port's own log path, not only from a direct embedder call. This also contradicts the project's byte-slice fidelity rule (Zig byte slices must not round-trip through a lossy String).
- **Evidence:** Oracle src/terminal/c/sys.zig:262-266 `std.fmt.bufPrint(&buffer, "[{s}]({s}): {s}\n", .{ level_text, scope, message })` and :283-284 `stderr.write(scope); ... stderr.write(message);` — raw bytes. rustty crates/rustty-vt-c/src/sys.rs:643-651 `writeln!(lock, "[{}]({}): {}", level_text, String::from_utf8_lossy(scope), String::from_utf8_lossy(message))` and :654-659 the no-scope arm, both lossy.
- **Suggested fix:** Write the raw bytes: `lock.write_all(b"[")`, `write_all(level_text.as_bytes())`, `write_all(b"](")`, `write_all(scope)`, `write_all(b"): ")`, `write_all(message)`, `write_all(b"\n")` — or build the line into a stack `[u8; 2048+128]` buffer and issue one `write_all`, matching the reference's single-write path exactly.

#### S178-F2 — log_stderr no longer emits each line with a single write; the doc-comment guarantee is unbacked
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/c/sys.zig:219-285`
- **Rustty:** `crates/rustty-vt-c/src/sys.rs:586-660`
- **Description:** The reference deliberately formats the whole line into a stack buffer and emits it with ONE raw `stderr.write(v)` call, and its doc comment states the reason: "Each log line is emitted with a single raw write to stderr, which keeps concurrent log lines from interleaving." rustty keeps that doc comment verbatim but implements it as `writeln!` over a held `std::io::stderr().lock()`. Rust's `Stderr` is unbuffered, so `write_fmt` issues one `write(2)` per format fragment (prefix, level, separator, scope, separator, message, newline). The process-internal lock prevents interleaving between Rust callers, but the reference's stronger property — one atomic write per line, which is what survives interleaving with other writers on the same fd (the embedder's own C logging, a forked child, another process sharing stderr) — is lost. The reference's explicit "didn't fit" piecewise fallback exists precisely because it is a best-effort degradation of that guarantee; the port takes that degraded path unconditionally.
- **Evidence:** Oracle src/terminal/c/sys.zig:260-269: `var buffer: [LogEmitter.buffer_size + 128]u8 = undefined; const line = ...bufPrint(...) catch null; if (line) |v| { stderr.write(v); return; }` — one write; :270-285 is the documented best-effort multi-write fallback. rustty crates/rustty-vt-c/src/sys.rs:601-606 repeats the single-raw-write doc comment, while :640-660 always uses `writeln!(lock, "[{}]({}): {}", ...)` with no line buffer (the PORT NOTE at :628-639 acknowledges "with no fixed buffer there is no didn't-fit fallback path to reproduce").
- **Suggested fix:** Format into a `[u8; LogEmitter::buffer_size + 128]` stack buffer and emit it with a single `write_all`, falling back to piecewise writes when the line does not fit — restoring both the single-write property and the reference's two-path structure. (Fixing this and the lossy-UTF-8 finding together is one change.)

#### S179-F3 — grid_ref_track allocates the TrackedGridRef on the Rust global heap instead of the caller's allocator
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/c/terminal.zig:1809-1834`
- **Rustty:** `crates/rustty-vt-c/src/terminal.rs:4306-4340`
- **Description:** The oracle allocates the `TrackedGridRef` and the tracked-ref registry through `alloc = t.gpa()` (which is `screens.active.alloc`, i.e. the caller-supplied C allocator for a C terminal) and returns `.out_of_memory` on failure, untracking the pin first. rustty uses `Box::into_raw(Box::new(...))` (global heap, aborts on OOM) and a `HashSet` that also allocates globally; the `ref.alloc` field it stores is the terminal-internals `Global`. Consequence: an embedder's allocator never sees tracked-grid-ref allocations, and the documented `RUSTTY_OUT_OF_MEMORY` return for this call is unreachable. Same root cause as the Tier-2 residual above, but a distinct site.
- **Evidence:** ghostty/src/terminal/c/terminal.zig:1809-1833 `const alloc = t.gpa(); const ref = alloc.create(grid_ref_tracked_c.TrackedGridRef) catch { list.untrackPin(tracked_pin); return .out_of_memory; }; ... wrapper.tracked_grid_refs.putNoClobber(alloc, ref, {}) catch {...}` (Terminal.zig:455 `gpa() => self.screens.active.alloc`). rustty/crates/rustty-vt-c/src/terminal.rs:4318-4331 `let alloc = unsafe { (*active).alloc }; ... Box::into_raw(Box::new(TrackedGridRef{...}))` plus `tracked_grid_refs.insert(ref_ptr)`.
- **Suggested fix:** Allocate the TrackedGridRef through the wrapper's stored C allocator (`rustty_lib::allocator::create_raw`) and keep the `untrack_pin` + `RUSTTY_OUT_OF_MEMORY` unwind, matching the oracle's cleanup order.

#### S180-F1 — `scroll_viewport null` drops the `.row` tag call
- **Status:** CONFIRMED · **Kind:** weakened-assertion · **Severity:** low
- **Oracle:** `src/terminal/c/terminal.zig:2394-2397`
- **Rustty:** `crates/rustty-vt-c/src/terminal.rs:5218-5232`
- **Description:** The oracle's `test "scroll_viewport null"` makes TWO calls on a null terminal handle: one with `.tag = .top, .value = undefined` and one with `.tag = .row, .value = .{ .row = 1 }`. The rustty counterpart `scroll_viewport_null` only performs the `Top` call; the `Row`-tag null call is absent, so the null guard is no longer exercised for a tag whose union payload is read.
- **Evidence:** Oracle 2394-2397:
```
test "scroll_viewport null" {
    scroll_viewport(null, .{ .tag = .top, .value = undefined });
    scroll_viewport(null, .{ .tag = .row, .value = .{ .row = 1 } });
}
```
rustty terminal.rs:5218-5232 `fn scroll_viewport_null()` contains a single `scroll_viewport(null_mut(), ScrollViewportC { tag: ScrollViewportTag::Top as c_int, value: ScrollViewportCValue { _padding: [0; 2] } })` and nothing else. (The implementation at terminal.rs:3477 returns on null before decoding the tag, so behaviour is equivalent — only the test coverage is weaker.)
- **Suggested fix:** Add the second call: `scroll_viewport(std::ptr::null_mut(), ScrollViewportC { tag: ScrollViewportTag::Row as c_int, value: ScrollViewportCValue { row: 1 } })`.

#### S180-F2 — `get vt_processing_error` counterpart is compiled out of release builds
- **Status:** DISPUTED · **Kind:** missing-test · **Severity:** low
- **Oracle:** `src/terminal/c/terminal.zig:3035-3073`
- **Rustty:** `crates/rustty-vt-c/src/terminal.rs:6252-6316`
- **Description:** The oracle test runs in every Zig test build (it forces the failure with `testing.failing_allocator`, available in all optimization modes). The rustty counterpart `get_vt_processing_error` is gated `#[cfg(debug_assertions)]` because it drives the `set_string_tw_*` tripwire, which only exists under `debug_assertions`. Under `cargo test --release` the test does not exist at all, so the `vt_processing_error` reporting path (including the "reset does not clear it" assertion) is unverified in the release lane. Assertions are otherwise identical in debug.
- **Evidence:** Oracle 3035-3073 swaps `t.?.terminal.screens.active.alloc` for `testing.failing_allocator` around `vt_write(t, "\x1B]2;unavailable\x1B\\")` and then asserts `processing_error` is true both immediately and after `reset(t)`. rustty terminal.rs:6262 carries `#[cfg(debug_assertions)]` directly above `fn get_vt_processing_error()`, with a comment stating the fail point "never trips" without `debug_assertions`.
- **Suggested fix:** Either expose a release-visible fail point (or a failing-allocator equivalent) for the terminal-owned string update path so the test can be unconditional, or accept the gap explicitly in the census ledger.
- **Disputed by:** presence lens (confidence medium) — "The counterpart test IS present, with assertion-for-assertion parity with oracle terminal.zig:3037-3086: initial Success+`!processing_error`, forced terminal-owned update failure through the public VT write path (the port arms `set_string_tw_error_always(SetStringFp::Reserve)` where the oracle swaps in `testing.failing_allocator`), Success+`processing_error`, then `reset(t)` and Success+`processing_error` again — 8 assertions on each side. It runs and passes in the default `cargo test` lane. The only delta is the `#[cfg(debug_assertions)]` gate at terminal.rs:6262, an artifact of the port's fail-injection mechanism (the rustty-vt tripwire fail point itself only exists under debug_assertions — the in-file PORT NOTE at 6254-6266 records that `cfg` rather than `ignore` is required because the `SetStringFp` symbols do not compile under --release), not a weakened or missing assertion. Zig has no release-mode test lane equivalent that this actually loses; production behaviour of the vt_processing_error path is unaffected. Not a functional-parity finding." Corrected location: `crates/rustty-vt-c/src/terminal.rs:6252-6316 (fn get_vt_processing_error)`.

#### S185-F1 — Stale "DEFERRED to vt-12 / compiling stub" PORT NOTE on graphics_storage re-export
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/kitty/graphics.zig:18-27`
- **Rustty:** `crates/rustty-vt/src/kitty/graphics.rs:27-30`
- **Description:** The PORT NOTE above the `graphics_storage` import still claims the module is deferred and a compiling stub whose `ImageStorage` will be re-exported "once ported". `ImageStorage` is in fact re-exported 18 lines below (graphics.rs:48), `graphics_storage.rs` defines a real `pub struct ImageStorage` (line 99) and `pub fn next_generation` (line 81), and the ref_all_decls test already names `ImageStorage`. Comment-only; no behavioural difference.
- **Evidence:** ghostty graphics.zig:18 `const storage = @import("graphics_storage.zig");` / :27 `pub const ImageStorage = storage.ImageStorage;` vs rustty graphics.rs:27-30 ("PORT NOTE: DEFERRED to vt-12 ... is a compiling stub; once ported it will re-export `ImageStorage` below") while graphics.rs:48 already has `pub use storage::ImageStorage;` and graphics_storage.rs:99 defines it.
- **Suggested fix:** Delete or reword the stale deferral note in crates/rustty-vt/src/kitty/graphics.rs:27-30.

#### S189-F3 — `LoadingImage.debugDump` not ported (carried as a comment)
- **Status:** CONFIRMED · **Kind:** missing-function · **Severity:** low
- **Oracle:** `src/terminal/kitty/graphics_image.zig:563-586`
- **Rustty:** `crates/rustty-vt/src/kitty/graphics_image.rs:854-880`
- **Description:** The oracle's `pub fn debugDump(io, self) !void` — a Debug-build-only helper that writes the loading buffer to `image-{format}-{compression}-{w}x{h}-{id}.data` for capturing test fixtures — has no Rust counterpart; it is reproduced verbatim as a comment block. No test or production caller uses it in either tree, so this is cosmetic/tooling only.
- **Evidence:** Oracle graphics_image.zig:563 `pub fn debugDump(io: std.Io, self: LoadingImage) !void` with `if (comptime builtin.mode != .Debug) @compileError("debugDump in non-debug");`. rustty graphics_image.rs:854-880 is the same body inside `//` comments prefixed by `PORT NOTE: debugDump is a Debug-only helper ... It is reproduced as a comment to preserve the reference verbatim.`
- **Suggested fix:** Optionally add a `#[cfg(debug_assertions)] pub fn debug_dump(&self) -> std::io::Result<()>` with the same filename format, or leave as-is and record it as an accepted cosmetic gap.

#### S189-F4 — Windows has no handle-based realpath, so the kitty file/temporary-file media fail closed there
- **Status:** CONFIRMED · **Kind:** stub-or-unimplemented · **Severity:** low
- **Oracle:** `src/terminal/kitty/graphics_image.zig:451-465`
- **Rustty:** `crates/rustty-vt/src/kitty/graphics_image.rs:1407-1420`
- **Description:** `validatedFilePath` derives the canonical path from the OPEN handle (`file.realPath`), which Zig implements on Windows via the NT object path. The rustty `realpath_of_handle` only implements macOS (`fcntl(F_GETPATH)`) and Linux/Android (`/proc/self/fd/N`); every other target returns `ErrorKind::Unsupported`, which `validatedFilePath` maps to `Error::InvalidData`. On Windows the oracle successfully loads images via `t=f`/`t=t`, whereas rustty rejects every such transmission with InvalidData. This is a registered deferral, not an unnoticed gap, but it is an observable behavioral difference in production code.
- **Evidence:** Oracle graphics_image.zig:451-453 `fn validatedFilePath(io, file, buf) ![]const u8 { const path = buf[0..try file.realPath(io, buf)]; ...}` (std File.realPath supports Windows). rustty graphics_image.rs:1407-1420: `#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "android")))] fn realpath_of_handle(_file: &std::fs::File) -> std::io::Result<Vec<u8>> { Err(std::io::Error::from(std::io::ErrorKind::Unsupported)) }` with a DEFERRAL comment; the ledger row is /Users/gael/projects/rustty/.port/phase-2/DEFERRALS.md:8284 "kitty file-medium handle-path on Windows".
- **Suggested fix:** Add a Windows arm using `GetFinalPathNameByHandleW` (stripping the `\\?\` prefix) so `validatedFilePath` can apply the blocklist to the opened handle, then close the deferral row.

#### S190-F1 — All log.info/log.warn/log.debug diagnostics in ImageStorage are carried as comments, never emitted
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/kitty/graphics_storage.zig:224-226, 302-311, 369, 576, 1025-1033, 1252, 1271, 1294, 1312, 1495-1505`
- **Rustty:** `crates/rustty-vt/src/kitty/graphics_storage.rs:363-367, 452-458, 468, 530, 810-816, 1315-1322, 1607, 1631, 1655, 1682, 1935, 1944`
- **Description:** Every logging call in the oracle's ImageStorage (eviction info/warn in setLimit, addImage, reserveAnimationBytes, evictImageExcept; addImage/addPlacement debug; the scroll OOM warn; the four `delete ... must be at least 1 / greater than zero` warns; the two deleteAnimationFrame warns) is reproduced in the port only as a commented-out source line. rustty-vt has no logging facility or `log` dependency at all, so these diagnostics are unobservable. No control flow differs — this is diagnostics-only and a port-wide convention rather than a per-file omission.
- **Evidence:** Oracle graphics_storage.zig:224 `log.info("evicting images to lower limit, evicting={}", .{req_bytes});` vs rustty graphics_storage.rs:364 `// log.info("evicting images to lower limit, evicting={}", .{req_bytes});`. `grep -rn 'log::warn|log::info|log::debug' crates/rustty-vt/src/` returns nothing, and crates/rustty-vt/Cargo.toml declares no logging crate.
- **Suggested fix:** Optional: none required for functional parity; if diagnostics are wanted later, introduce a vt-level logging shim and restore these call sites verbatim.

#### S190-F2 — Two documented allocation-failure degradation paths cannot be reached in the port (abort instead)
- **Status:** DISPUTED · **Kind:** missing-error-path · **Severity:** low
- **Oracle:** `src/terminal/kitty/graphics_storage.zig:570-579, 977-988`
- **Rustty:** `crates/rustty-vt/src/kitty/graphics_storage.rs:806-822, 1268-1300`
- **Description:** scrollMarginsBegin's `result.restores.append(...) catch { log.warn("OOM adjusting image placement for scroll"); continue; }` degrades gracefully on allocation failure (the placement is left to generic pin tracking). The port uses `Vec::push`, which aborts the process on allocation failure, so the degraded-but-safe arm is unreachable. Likewise `convertImageToRgba` is `Allocator.Error!void` in the oracle (propagating a failure from `pixel.rgbaFromFormat`) while the port's `convert_image_to_rgba` is infallible because `rgba_from_format` aborts on OOM. Both differences are explicitly documented as PORT NOTEs and are only observable under memory exhaustion, which the whole Rust port treats as abort.
- **Evidence:** Oracle 570-579 (`catch { ... continue; }`) vs rustty 806-816 PORT NOTE `Rust's Vec::push aborts on OOM, so the reference's degraded-but-safe fallback arm is unreachable in this port.` Oracle 977 `pub fn convertImageToRgba(...) Allocator.Error!void` vs rustty 1268 `pub fn convert_image_to_rgba(&mut self, alloc: Allocator, image_id: u32)` (no Result).
- **Suggested fix:** None practical; the port-wide OOM=abort policy makes these arms dead code. If a fallible allocator is ever adopted for vt, restore `try_reserve`/`push` fallbacks at these two sites.
- **Disputed by:** semantic lens (confidence high) — "Read both sides. Oracle graphics_storage.zig:569-579 `result.restores.append(...) catch { log.warn("OOM adjusting image placement for scroll"); continue; }` — the degraded arm is reachable only on allocation failure. rustty graphics_storage.rs:813-822 pushes into a Vec with the reference arm carried as a comment plus a PORT NOTE; Vec::push aborts on OOM, so under the port-wide OOM=abort policy that arm is unreachable code, and no input/state produces a different observable result. Same for oracle convertImageToRgba (`Allocator.Error!void`, line 984 `try pixel.rgbaFromFormat`) vs rustty convert_image_to_rgba (rs:1268, infallible, calling pixel::rgba_from_format which aborts on OOM): the only removed error is the allocation failure that aborts anyway, and the byte accounting / generation stamping after it is identical. Per the ground rules, a difference confined to unreachable code is refuted."

#### S194-F4 — isSafeUtf8 test's 8-bit-escape case changed from a raw 0x9F byte (invalid UTF-8) to a valid U+009F, so the invalid-UTF-8 rejection branch is no longer covered
- **Status:** CONFIRMED · **Kind:** weakened-assertion · **Severity:** low
- **Oracle:** `src/terminal/osc/encoding.zig:37`
- **Rustty:** `crates/rustty-vt/src/osc/encoding.rs:49-51`
- **Description:** The reference asserts `!isSafeUtf8("\x9f8-bit escapes are clever, but no")`; in Zig `\x9f` is the single raw byte 0x9F, which makes the slice invalid UTF-8 and exercises the `Utf8View.init` failure branch. The port writes `"\u{9f}8-bit escapes are clever, but no"`, which encodes as the two valid bytes C2 9F, so the assertion now exercises only the C1-codepoint branch (0x80..=0x9f) and the `str::from_utf8` error branch of `is_safe_utf8` is untested.
- **Evidence:** Oracle encoding.zig:37 `try testing.expect(!isSafeUtf8("\x9f8-bit escapes are clever, but no"));` (byte 0x9F). rustty encoding.rs:49-51 `assert!(!is_safe_utf8("\u{9f}8-bit escapes are clever, but no".as_bytes()));` (bytes C2 9F).
- **Suggested fix:** Use a raw byte literal, e.g. `assert!(!is_safe_utf8(b"\x9f8-bit escapes are clever, but no"));`, to restore the invalid-UTF-8 input, and optionally keep the U+009F case as an extra assertion.

#### S196-F1 — rxvt_extension scoped logger and its two warn() calls are commented out
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/osc/parsers/rxvt_extension.zig:6,27-32`
- **Rustty:** `crates/rustty-vt/src/osc/parsers/rxvt_extension.rs:7-11,34,41`
- **Description:** The oracle declares `const log = std.log.scoped(.osc_rxvt_extension);` and emits `log.warn("unknown rxvt extension: {s}", .{ext})` on a non-`notify` extension and `log.warn("rxvt notify extension is missing the title", .{})` when the second ';' is absent. The Rust port keeps both call sites as comments with a PORT NOTE saying the crate has no logging facade. Control flow (parser.state = Invalid; return None) is identical, so only the diagnostic output differs. This matches the same documented convention used for `osc.zig`'s own scoped logger (crates/rustty-vt/src/osc/mod.rs:899-903), so it is a consistent, deliberate deviation, not an isolated omission.
- **Evidence:** ghostty src/terminal/osc/parsers/rxvt_extension.zig:6 `const log = std.log.scoped(.osc_rxvt_extension);`, :27 `log.warn("unknown rxvt extension: {s}", .{ext});`, :31 `log.warn("rxvt notify extension is missing the title", .{});` vs rustty crates/rustty-vt/src/osc/parsers/rxvt_extension.rs:11 `// const log = std.log.scoped(.osc_rxvt_extension);`, :34 `// log.warn("unknown rxvt extension: {s}", .{ext});`, :41 `// log.warn("rxvt notify extension is missing the title", .{});`
- **Suggested fix:** If/when a logging facade is wired into rustty-vt, restore both warnings with the same message text and the `osc_rxvt_extension` scope.

#### S197-F1 — exit_code parsing: Zig std.fmt.parseInt accepts '_' digit separators, Rust str::parse does not
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/osc/parsers/semantic_prompt.zig:160-166`
- **Rustty:** `crates/rustty-vt/src/osc/parsers/semantic_prompt.rs:323-333`
- **Description:** Oracle decodes the OSC 133 `D` exit code with `std.fmt.parseInt(i32, full, 10) catch null`. Zig's parseInt skips interior `_` digit separators (rejecting only leading/trailing `_`), so `133;D;1_2` yields exit_code 12. The Rust port decodes with `core::str::from_utf8(full).ok()?.parse::<i32>().ok()`, and Rust's integer FromStr rejects any `_`, so the same input yields None (no exit code recorded). All other accepted forms (leading `+`/`-`, overflow rejection, non-digit rejection, no base-prefix detection at base 10) match.
- **Evidence:** ghostty src/terminal/osc/parsers/semantic_prompt.zig:160-166 `if (comptime self == .exit_code) { return std.fmt.parseInt(i32, full, 10) catch null; }` vs rustty crates/rustty-vt/src/osc/parsers/semantic_prompt.rs:323-333 `decode: |full| { core::str::from_utf8(full).ok()?.parse::<i32>().ok() }`. Input `1_2` -> Zig Some(12), Rust None.
- **Suggested fix:** Strip interior `_` separators (rejecting leading/trailing `_` and an all-`_`/empty remainder) before calling `parse::<i32>()`, mirroring Zig's parseIntWithSign, and add a regression assertion to `option_read_exit_code`.

#### S204-F1 — Decode-time temporary allocations abort instead of returning OutOfMemory
- **Status:** DISPUTED · **Kind:** missing-error-path · **Severity:** low
- **Oracle:** `src/terminal/snapshot/page.zig:252-253, 373-381`
- **Rustty:** `crates/rustty-vt/src/snapshot/page.rs:457, 561-573`
- **Description:** The reference returns `error.OutOfMemory` (a declared member of `PayloadDecodeError`/`DecodeError`) when the staged-payload buffer (`alloc.alloc(u8, remaining)`, up to 8 MiB) or the style/hyperlink remap tables (`grid.StyleRemap.init(alloc) catch return error.OutOfMemory`) cannot be allocated. The port allocates these with the global allocator (`vec![0u8; remaining]`, `grid::StyleRemap::init()` returning a value, not a Result), so an allocation failure aborts the process rather than surfacing `PayloadDecodeError::OutOfMemory`. The variant itself is preserved and is still produced for `TerminalPage::init` failure, so the only unreachable path is allocator failure on these temporaries. No reference test exercises it (no failing-allocator test in this file), and it matches the crate-wide global-allocator convention, hence low.
- **Evidence:** ghostty src/terminal/snapshot/page.zig:252 `const staged = try alloc.alloc(u8, remaining);` and :374 `grid.StyleRemap.init(alloc) catch return error.OutOfMemory` vs rustty crates/rustty-vt/src/snapshot/page.rs:457 `let mut staged = vec![0u8; remaining];` and :565 `grid::StyleRemap::init()`.
- **Suggested fix:** If the port wants full parity on this error path, use a fallible allocation (e.g. `Vec::try_reserve`) for the staged payload and remap tables and map failure to `PayloadDecodeError::OutOfMemory`; otherwise record the deviation in a PORT NOTE next to the staged allocation.
- **Disputed by:** semantic lens (confidence high) — "Read both regions. Oracle ghostty src/terminal/snapshot/page.zig:252 (`const staged = try alloc.alloc(u8, remaining)`) and :373-381 (`grid.StyleRemap.init(alloc) catch return error.OutOfMemory`, same for HyperlinkRemap) vs rustty crates/rustty-vt/src/snapshot/page.rs:457 (`let mut staged = vec![0u8; remaining]`) and :567-573 (`grid::StyleRemap::init()` / `HyperlinkRemap::init()` returning values). The description is factually accurate, but the only divergence is behaviour under global-allocator exhaustion, which in Rust aborts rather than unwinding an error — the crate-wide infallible-allocation convention, not a snapshot-specific deviation. The staged buffer is bounded by Self::MAX_STAGED_PAYLOAD (the `else if remaining <= Self::MAX_STAGED_PAYLOAD` guard at page.rs:450 mirrors the oracle's `max_staged_payload` branch), so there is no unbounded/hostile-allocation hazard difference. PayloadDecodeError::OutOfMemory is preserved (page.rs:303) and still produced on TerminalPage::init failure (page.rs:369, :536), matching the oracle's other OOM producer. No oracle test injects a failing allocator in this file, so no test asserts less. Difference confined to a path unreachable outside OS allocator failure => refuted per the lens."

#### S205-F1 — Two decode tests use unlimited scrollback where the reference uses the Options default of zero
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/snapshot/screen.zig:2371-2377, 2444-2450`
- **Rustty:** `crates/rustty-vt/src/snapshot/screen.rs:2971, 3050`
- **Description:** In `test "SCREEN decode ignores an invalid cursor hyperlink"` and `test "SCREEN decode ignores a PAGE with an empty hyperlink URI"` the reference calls `decode(..., .{ .cols = 1, .rows = 1 })`, and `Screen.Options.max_scrollback_bytes` defaults to `0` (src/terminal/Screen.zig:266), so both tests decode with scrollback DISABLED (`no_scrollback == true`, PageList builder `max_size = 0`). The Rust counterparts pass `screen_options(1, 1, None)`, i.e. `max_scrollback_bytes = None` = UNLIMITED scrollback, so `no_scrollback` is false and the builder gets no size cap. The second test then resizes to cols=2 under a different scrollback policy than the reference. The asserted outcomes (cursor hyperlink absent / cell hyperlink flag clear) still hold, but the tests no longer exercise the reference's configuration.
- **Evidence:** Oracle screen.zig:2371-2377 `var decoded = try decode(&source, std.testing.io, std.testing.allocator, .{ .cols = 1, .rows = 1 });` with Screen.zig:266 `max_scrollback_bytes: ?usize = 0,`. Rustty screen.rs:2971 `let mut decoded = decode(&mut source, Allocator, screen_options(1, 1, None)).unwrap();` and screen.rs:3050 identical, where `screen_options` (screen.rs:1679-1687) assigns the third argument straight into `max_scrollback_bytes`. Every other test in the file passes `Some(0)`, matching the reference's explicit `.max_scrollback_bytes = 0`.
- **Suggested fix:** Change both call sites to `screen_options(1, 1, Some(0))` so the decode options match the reference's `Options` default.

#### S207-F1 — Restored alternate screen gets kitty_image_storage_limit 0 instead of the Screen.Options default (320MB)
- **Status:** DISPUTED · **Kind:** different-constant · **Severity:** low
- **Oracle:** `src/terminal/snapshot/terminal.zig:992-999`
- **Rustty:** `crates/rustty-vt/src/snapshot/terminal.rs:1236-1251`
- **Description:** In `decode`, the oracle creates the optional alternate screen with `screens.getInit(io_, alloc, .alternate, .{ .cols, .rows, .max_scrollback_bytes = 0 })`, letting every other `Screen.Options` field take its struct default — notably `kitty_image_storage_limit = 320 * 1000 * 1000` for the `.ghostty` artifact (src/terminal/Screen.zig:275-278). The port spreads `..crate::screen::Options::default()`, but that helper is the port of the reference's *`Options.default` constant* (cols 80 / rows 24 / scrollback 0) and sets `kitty_image_storage_limit: 0` (crates/rustty-vt/src/screen.rs:415-422). The restored alternate screen therefore carries a Kitty image storage limit of 0 rather than 320MB, while the primary screen created a few lines earlier in the same function is given 320MB explicitly (crates/rustty-vt/src/snapshot/terminal.rs:1219). Any Kitty image transmitted on the alternate screen after a snapshot restore would be rejected/evicted where the reference accepts it.
- **Evidence:** oracle src/terminal/snapshot/terminal.zig:992-999 `_ = try result.screens.getInit(io_, alloc, .alternate, .{ .cols = header.columns, .rows = header.rows, .max_scrollback_bytes = 0 });` + src/terminal/Screen.zig:275-278 default `kitty_image_storage_limit = 320 * 1000 * 1000` (.ghostty). rustty crates/rustty-vt/src/snapshot/terminal.rs:1236-1245 `crate::screen::Options { cols, rows, max_scrollback_bytes: Some(0), ..crate::screen::Options::default() }` and crates/rustty-vt/src/screen.rs:420 `kitty_image_storage_limit: 0`.
- **Suggested fix:** Spell out `kitty_image_storage_limit: 320 * 1000 * 1000` and `kitty_image_loading_limits: Limits::DIRECT` (the artifact defaults, as done for the primary Terminal::init options at line 1219) instead of spreading `Options::default()`, or add an `Options::field_defaults()` helper that mirrors the reference struct-field defaults rather than the `Options.default` constant.
- **Disputed by:** semantic lens (confidence high) — "Traced the whole decode pipeline in both trees. The alternate screen created by `getInit` in `terminal.decode` is an explicitly documented placeholder: oracle src/terminal/snapshot/terminal.zig:943-948 ("contains empty primary and optional alternate screens with the declared routing. Later SCREEN records replace those screens in place") and terminal.zig:995-996. In snapshot.zig:313-350 every slot is `slot.deinit(); slot.* = decoded.screen;` where `decoded.screen` comes from screen.decode(..., screen_options), and screen.zig:500-510 applies `options.kitty_image_storage_limit` / `kitty_image_loading_limits` to the FINAL screen. The port does the identical replacement at crates/rustty-vt/src/snapshot/codec.rs:888-921 and 924-960, and its `screen_options` closure spells out `kitty_image_storage_limit: 320 * 1000 * 1000` and `Limits::DIRECT` (codec.rs:918-919), matching the oracle struct default for the .ghostty artifact. So the restored primary AND alternate screens both end with total_limit 320MB in both trees; the 0 at snapshot/terminal.rs:1240 lives only on an object that is deinit'd and overwritten before any kitty command can reach it (the loop errors out on Duplicate/UnexpectedScreenKey, never leaving a slot unreplaced). The only way to observe it is a direct call to the low-level record decoder `snapshot.terminal.decode`, whose contract states the screens are empty placeholders. No input produces a different kitty-storage behaviour; refuted as a dead-value/style inconsistency (worth a cosmetic cleanup at most)."

#### S207-F2 — PORT NOTE states ModePacked::BITS is 42; it is 43
- **Status:** DISPUTED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/snapshot/terminal.zig:247-249`
- **Rustty:** `crates/rustty-vt/src/snapshot/terminal.rs:285-291`
- **Description:** The port note above `MODE_BITS_MASK` says the low `ModePacked::BITS` (42) bits are the packed registry. `ModePacked::BITS = ENTRIES.len()` is actually 43 (crates/rustty-vt/src/modes.rs:240, ENTRIES at modes.rs:560-620), which is what the oracle's `test "TERMINAL mode bit layout"` asserts and what the port's own `terminal_mode_bit_layout` test asserts. Comment-only; the mask value itself is correct.
- **Evidence:** rustty crates/rustty-vt/src/snapshot/terminal.rs:288 `// `ModePacked::BITS` (42) bits` vs crates/rustty-vt/src/snapshot/terminal.rs:1597 `assert_eq!(43usize, terminal_modes::ModePacked::BITS);` and oracle src/terminal/snapshot/terminal.zig:1204-1207 `expectEqual(@as(usize, 43), @bitSizeOf(ModePacked))`.
- **Suggested fix:** Change the parenthetical to 43.
- **Disputed by:** semantic lens (confidence high) — "The factual claim checks out — crates/rustty-vt/src/snapshot/terminal.rs:290 says "`ModePacked::BITS` (42) bits" while modes.rs:240 defines `BITS = ENTRIES.len()` and both the oracle test (terminal.zig:1204-1207, expects 43) and the port test (snapshot/terminal.rs:1597 `assert_eq!(43usize, ModePacked::BITS)`) say 43. But MODE_BITS_MASK is computed from the symbol `terminal_modes::ModePacked::BITS`, not from the literal, so the mask is `(1<<43)-1` and every decode/truncate path behaves identically to the oracle. A stale parenthetical inside a PORT NOTE produces no observable functional difference; refuted under the semantic-equivalence lens (valid as a comment nit only)."

#### S208-F1 — Notification format/Display output text differs from the reference's comptime-reflected format
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/tmux/control.zig:568-590`
- **Rustty:** `crates/rustty-vt/src/tmux/control.rs:878-975`
- **Description:** The reference `Notification.format` prints `@typeName(T)` (i.e. `terminal.tmux.control.Notification`), then `{ .<tag> = `, then the payload: `[]const u8` payloads via `"{s}"` on the trimmed value and EVERY other payload (including the void tags and the anonymous struct payloads) via `{any}`. Rustty's hand-written `Display` prints the shorter type name `control.Notification`, renders the void tags (`enter`, `exit`, `sessions_changed`) as `.<tag> =  }` where Zig's `{any}` prints `void`, and renders the struct payloads with a bespoke `output{ .pane_id = 42, .data = "foo" }` shape whose `[]const u8` members are quoted strings where Zig's `{any}` would print them as byte arrays. Log-text only: the reference's single consumer is `log.info("tmux control mode event cmd={f}", .{tmux})` in src/termio/stream_handler.zig:405, whose Rustty counterpart (crates/rustty-termio/src/stream_handler.rs:492) is a commented-out log line.
- **Evidence:** oracle control.zig:571-573 `try writer.writeAll(@typeName(T)); ... try writer.writeAll(" = ");` and :580-583 `switch (u_field.type) { []const u8 => try writer.print("\"{s}\"", ...), else => try writer.print("{any}", .{value}) }`. rustty control.rs:889 `writer.write_str("control.Notification")?;`, :893 `Notification::Enter => write!(writer, "enter =  }}")`, :899-903 `Notification::Output {..} => write!(writer, "output = output{{ .pane_id = {pane_id}, .data = \"{}\" }} }}", trim_display(data))`.
- **Suggested fix:** Emit `terminal.tmux.control.Notification` as the type name, print `void` for the payload-less tags, and render the struct payloads the way Zig's `{any}` does (byte-array members), or accept the divergence explicitly in the PORT NOTE since the only consumer is a log line.

#### S208-F2 — tmux output delimiter is treated as a Unicode char rather than a byte
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/tmux/output.zig:17-36, 57-66`
- **Rustty:** `crates/rustty-vt/src/tmux/output.rs:104-119, 383-405`
- **Description:** `parseFormatStruct`/`format` take `delimiter: u8` and the reference splits/writes it as a single BYTE (`std.mem.splitScalar(u8, str, delimiter)`, `writer.writeByte(delimiter)`). Rustty converts it with `delimiter as char`, so for any delimiter >= 0x80 the split matches the two-byte UTF-8 encoding of that code point (which never occurs as a standalone byte the reference would split on) and `format` writes two bytes instead of one. Both entry points are `pub`. All in-tree call sites use ASCII delimiters (' ', ',', '\t'), so this is unobservable today.
- **Evidence:** oracle output.zig:22 `var it = std.mem.splitScalar(u8, str, delimiter);` and :63 `if (i != 0) try writer.writeByte(delimiter);`. rustty output.rs:395 `inner: str.split(delimiter as char)` and :112 `out.push(delimiter as char);`.
- **Suggested fix:** Split/emit on the raw byte (operate on `&[u8]`, or assert/restrict the delimiter to ASCII) so a >=0x80 delimiter behaves as in the reference.

#### S208-F3 — Stale PORT NOTE claims the oniguruma binding crate does not exist
- **Status:** CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/terminal/tmux/control.zig:1-10`
- **Rustty:** `crates/rustty-vt/src/tmux/control.rs:11-22`
- **Description:** control.rs's header PORT NOTE justifies the `#[cfg(feature = "oniguruma")]` gate on the eight regex-backed notification arms with "The `oni` binding crate does not exist yet, so the gated code is NOT compiled in the default build". That is stale: crates/rustty-oniguruma exists and is a non-optional dependency of rustty-input, and rustty-core enables `rustty-vt = { features = ["oniguruma", ...] }`, so the app-tier build compiles all eight arms. Functional parity itself is intact — `options::TMUX_CONTROL_MODE = ONIGURUMA` (crates/rustty-vt/src/options.rs:177) exactly mirrors `opts.addOption(bool, "tmux_control_mode", self.oniguruma)` (src/terminal/build_options.zig:296) and dcs.rs guards every tmux prong on it, so a build without oniguruma disables tmux control mode wholesale exactly as the reference does. Only the note's rationale is wrong, and it hides the fact that a bare `cargo test -p rustty-vt` (no feature unification) skips the eight regex tests.
- **Evidence:** rustty control.rs:17-19 "The `oni` binding crate does not exist yet, so the gated code is NOT compiled in the default build" vs crates/rustty-oniguruma/Cargo.toml (exists) and crates/rustty-core/Cargo.toml:37 `rustty-vt = { workspace = true, features = ["oniguruma", "simd", "wuffs"] }`.
- **Suggested fix:** Rewrite the note to state the real (and correct) rationale — the feature mirrors the reference `oniguruma` build option, is enabled by rustty-core for the app tier, and `cargo test -p rustty-vt --features oniguruma` is required to run the eight gated tests.

#### S209-F1 — Pane terminal dimension overflow: Zig @intCast traps, Rust `as` silently truncates
- **Status:** DISPUTED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/tmux/viewer.zig:1131-1140`
- **Rustty:** `crates/rustty-vt/src/tmux/viewer.rs:935-950`
- **Description:** In initLayout the reference constructs the pane Terminal with `.cols = @intCast(layout.width), .rows = @intCast(layout.height)` from `usize` layout dimensions, which panics (illegal behavior detected) in safe builds when a layout string reports a width/height > 65535. The port uses `d.width as size::CellCountInt` / `d.height as size::CellCountInt`, which silently wraps modulo 2^16 and creates a terminal with a wrong (possibly 0) size instead of trapping. Both trees acknowledge the cast is unsafe in the same TODO comment, but the observable outcome on a malformed/hostile layout string differs (crash vs. silently mis-sized pane, and cols=0 can produce further downstream misbehavior).
- **Evidence:** oracle viewer.zig:1131-1140 `var t: Terminal = try .init(io, gpa_alloc, .{ .cols = @intCast(layout.width), .rows = @intCast(layout.height) });` vs rustty viewer.rs:938-944 `Terminal::init(self.alloc, terminal_opts(d.width as size::CellCountInt, d.height as size::CellCountInt))`. Layout width/height are `usize` in both (ghostty layout.zig; rustty tmux/layout.rs:33-34).
- **Suggested fix:** Use a checked conversion that reproduces the trap (e.g. `size::CellCountInt::try_from(d.width).expect(...)`, matching the @intCast safety check) or explicitly document/handle the overflow as the TODO suggests.
- **Disputed by:** presence lens (confidence medium) — "Read oracle src/terminal/tmux/viewer.zig:1131-1148 and rustty crates/rustty-vt/src/tmux/viewer.rs:938-950: the port carries the oracle's own TODO verbatim ("We need to gracefully handle overflow of our max cols/width here. In practice we shouldn't hit this so we cast but its not safe") and performs the same unchecked narrowing. The only delta is Zig's safe-build @intCast check, which is a build-mode safety check, not defined behaviour: in ReleaseFast (Ghostty's release mode) @intCast on out-of-range input is UB, so there is no reference-defined trap to port. Both sides parse layout width/height as usize (ghostty layout.zig; rustty tmux/layout.rs:33-34) and both construct the Terminal with the narrowed value; the ported control flow, error path and observable result on all in-range inputs are identical. This is the generic Zig-safety-check-vs-Rust-`as` class, not a functional-parity finding." Corrected location: `crates/rustty-vt/src/tmux/viewer.rs:935-950`.

#### S209-F2 — All log.info/log.warn diagnostics in the viewer are dropped in the port
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/tmux/viewer.zig:16, 305, 397, 419-421, 461-472, 501, 505, 566, 654, 828, 855, 878, 900, 1027, 1049, 1063, 1075`
- **Rustty:** `crates/rustty-vt/src/tmux/viewer.rs:66-69, 455-460, 621-627, 749-753, 1140-1146, 1600-1607`
- **Description:** The reference emits scoped log messages at every ignored/failed path (`received notification in defunct state, ignoring`, `failed to queue command, becoming defunct`, `failed to process output for pane id={}: {}`, `failed to handle session change/layout change/window add, becoming defunct`, `layout change for unknown window id={}`, `unexpected block output err={}`, `failed to parse tmux version/list-windows line/list-panes line`, `failed to parse window layout id={} layout={s}`, `received pane state/history/visible/output for untracked pane id={}`, and `testViewer step failed i={} step={}`). The port preserves each site as a comment but emits nothing; the crate does not thread a logging facade through this seam (documented PORT NOTE at viewer.rs:66-69). Observable only as missing operator-facing diagnostics; control flow is identical.
- **Evidence:** e.g. oracle viewer.zig:855 `log.info("received pane state for untracked pane id={}", .{data.pane_id});` vs rustty viewer.rs:1143 `// log.info("received pane state for untracked pane id={}", .{data.pane_id});` with only `continue;`. Same pattern at all 17 log sites.
- **Suggested fix:** None required for functional parity; if/when a logging seam exists in rustty-vt, re-instate these messages verbatim.

#### S209-F3 — Action formatting text differs slightly from the reference's reflection-based format
- **Status:** CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/tmux/viewer.zig:207-229`
- **Rustty:** `crates/rustty-vt/src/tmux/viewer.rs:311-333`
- **Description:** The reference `Action.format` prints `@typeName(Action){ .<tag> = <value> }`, where the void `exit` payload is rendered through `{any}` (printing `void`) and `windows` through `{any}` over the slice. The port's hand-written Display prints the type name as the literal `viewer.Viewer.Action`, renders `exit` as `{ .exit =  }` (empty payload, double space) and `windows` via Rust `{:?}` debug formatting. Diagnostics-only (the reference uses it in testViewer's failure log, which the port also drops); no assertion depends on it.
- **Evidence:** oracle viewer.zig:207-229 (`try writer.writeAll(@typeName(T)); ... try writer.print("{any}", .{value});`) vs rustty viewer.rs:319-332 (`writer.write_str("viewer.Viewer.Action")`, `Action::Exit => write!(writer, "exit =  }}")`, `Action::Windows(v) => write!(writer, "windows = {v:?} }}")`).
- **Suggested fix:** Cosmetic; align the exit/windows rendering text if exact diagnostic parity is desired.


## Performance-only differences (not findings)

385 observations recorded by the finders as performance- or idiom-only: identical observable behaviour, different implementation. They are listed per shard with the oracle unit(s) that shard covered.


**Shard 122** — PageList.zig

- NodeData is #[repr(C, u8)] so tag/payload are projected by raw pointer arithmetic (Stacked Borrows discipline); loses Zig niche packing, no observable difference.
- std_size()/std_capacity() are #[inline] fns constant-folded at call sites instead of comptime consts.
- PagePool standard buffers are lazily OS-zeroed (mmap) instead of eagerly memset under runtime safety; handed-out pages are observably zero in both (reused slots are zeroed by destroyNodeExt / zero_retained_arena).
- TrackedPinsRemap uses a cheap splitmix-style PtrHasher instead of SipHash (pointer-identity keys, same semantics).
- clone() builds a second PageIterator for counting rather than copying `it` (iterator is a pure function of unmodified state).
- reflowRow trims trailing blanks with `iter().rev().take_while(is_empty).count()`; bulkRunLength uses `as_chunks::<BULK_GROUP_LEN>()` for the vectorized group loop — both yield identical cols_len / run lengths.
- MemoryPool::init takes no allocator handles (Global + process page allocator) — the only oracle behaviour not compiled is the native-freestanding embedder-allocator arm, which this crate does not target.


**Shard 123** — PageList.zig

- copy_run uses core::ptr::copy_nonoverlapping instead of @memcpy (identical result; aliasing proven on a Miri harness per the PORT NOTE at resize.rs:1846-1852).
- grow's `break :prune` fall-through is factored into a `grow_alloc` helper (grow.rs:484-506) so every prune-exit can return to the identical allocation tail; not a separate function in the oracle.
- scroll is split into `scroll`/`scroll_impl` (scroll.rs:57-93) solely to reproduce the two `defer` blocks (assertIntegrity + page_compression reset/markActivity) on every exit path, including through the .row -> .delta_row recursion.
- compact/split re-derive `*mut Page` via raw projection after list splices (Stacked-Borrows hygiene, grow.rs:75, 906); no observable difference.
- Scrollbar.C is hoisted to a sibling `ScrollbarC` type with identical repr(C) layout (grow.rs:1021-1033).


**Shard 124** — PageList.zig

- createPageExt (mod.rs:2424-2440): the oracle's `@memset(page_buf, 0)` under runtime-safety/freestanding is skipped because Rust allocators do not 0xAA-fill; pooled buffers are zeroed by destroyNodeExt/decommit and fresh mmap pages are OS-zeroed, so handed-out buffers are observably identical (all zero).
- trackedPins (pin.rs:113-115) returns a freshly collected Vec<*mut Pin> instead of borrowing the hashmap's contiguous key array; same elements in the same insertion order.
- increaseCapacity's `inline else` over Capacity fields is expanded via a macro into four explicit arms with the same per-field integer types (StyleCountInt/GraphemeBytesInt/HyperlinkCountInt/StringBytesInt), matching page.zig:1857-1869.
- activity_serial u48 is modelled as a u64 masked to 48 bits (compress.rs:162,179); wrap-around behaviour is identical.
- compress_page: `compress::CompressedPage::init` takes no allocator argument (Rust global allocator) — same OOM->false handling.


**Shard 125** — PageList.zig

- `Limits.enforce(self: *const Limits, pagelist)` is relocated as `PageList::enforce_limit(&mut self, key)` operating on a `Copy` of `self.limits`; Limits is never mutated during the loop so behaviour is identical (Stacked-Borrows-driven signature change).
- `trackedPins()` returns an owned `Vec<*mut Pin>` snapshot instead of a borrowed keys slice; same insertion order, same 'valid until modified' contract.
- Chunk::rows returns `*mut [Row]` (raw fat pointer) instead of a `[]Row` slice; `Pin::cells` likewise returns `*mut [Cell]`.
- Test-only FailingAllocator injection is replaced by `rustty_util::tripwire` fail points (FINISH_TW / ALLOCATE_PAGE_TW) placed at the same allocation sites; the two affected tests are ignored in release builds where tripwire is compiled out.
- PageIterator::Limit::Row stores the pin by value and the port copies node/y out before matching to avoid holding a borrow across `self.row` writes — identical result.


**Shard 126** — PageList.zig

- Incompressible-page fixtures use a xorshift64* stream (tests.rs:2276-2295) instead of std.Random.DefaultPrng; only incompressibility is asserted, not specific bytes.
- Rust compression tests reach compressed encoded bytes through `compressed_encoded_for_test` / `compressed_encoded_fill_for_test` raw-projection helpers rather than direct `node.data.compressed.encoded` field access, to avoid forming a `&NodeData`; observable behaviour is identical.
- Several counterparts carry `#[cfg_attr(miri, ignore)]` / G6-lane ignores for LZ4 volume; they still run natively and under ASan.


**Shard 127** — PageList.zig

- Several counterparts (grow allocate, grow prune scrollback ×2, eraseRows/eraseRow/eraseRowBounded ×5, row erasure renews generations, increaseCapacity multi-page) carry `#[cfg_attr(all(miri, debug_assertions), ignore = ...)]`; this only gates the Miri debug-assertions lane for time budget and does not change native test behaviour.
- rustty `pagelist_grow_prune_scrollback` keeps the oracle's explicit `if pin_y >= rows_pruned { .. } else { 0 }` rather than `saturating_sub` (allow(clippy::implicit_saturating_sub)); identical result.


**Shard 129** — PageList.zig

- Several tests carry `#[cfg_attr(all(miri, debug_assertions), ignore = ...)]` (tests.rs:10553, 11023, 11371, 11443, 11878) — Miri-budget lane gating only; the tests run unconditionally under plain cargo test and in the G6c Miri lane, so assertions are not weakened.


**Shard 131** — PageList.zig

- Miri-lane #[cfg_attr(all(miri, debug_assertions), ignore)] attributes on several ported tests (grapheme map capacity exceeded, backfill pins, grow non-standard x2, compact oversized) only affect the Miri G6 lane; the tests run normally under cargo test.
- "destroyed pool page reuse is zeroed": Zig checks the contiguous cells slab as u64 words via std.mem.allEqual; rustty checks each cell's bytes through get_row_and_cell — same zero-ness check, different traversal.
- PagePool.item_size in the oracle grow tests is spelled std_size() in rustty; the pool item size equals std_size so the loop conditions are identical.


**Shard 132** — Parser.zig

- Action dispatch payloads (CSI/ESC/DCS/osc::Command) are Box'd and CSI/ESC/DCS carry fixed-capacity InlineVec<u8,4>/InlineVec<u16,24> copies of the parser buffers instead of the oracle's slices into self.intermediates/self.params ('valid until next call'); repr(align(8)). Observable contents (prefix slices, final, params_sep) identical, lifetime strictly stronger.
- Parser::next has a state==next_state fast path (parser.rs:612-620) returning [None, do_action, None] with Print/Execute duplicated as separate return sites and the redundant self.state store skipped; identical to the oracle's guarded exit/entry arms which are null when state == next_state.
- self.state hoisted into a local before the table lookup (parser.rs:569); collect/do_action/clear marked #[inline(always)] mirroring Zig `inline fn`; next marked #[inline].
- SepList is a hand-rolled u32 bitset (StaticBitSet(24) equivalent); is_set returns false for idx >= 24 where the Zig bitset would assert — unreachable in practice (params_idx is bounded by MAX_PARAMS).
- Parser::init zero-fills intermediates/params instead of leaving them undefined (Valgrind re-init branch is a no-op); never read past tracked indices.


**Shard 133** — Screen.zig

- screen.rs:986-991 `TrackedPinsRemap::default()` uses a cheap pointer hasher instead of RandomState for the clone pin remap — identical mapping results.
- screen.rs:1201-1226 `clone_cursor` helper factors the two identical inline Cursor literals of the oracle's `clone`; same field values.
- screen.rs:2053-2061 zero-fill of the scroll-region top row is `ptr::write_bytes` over `#[repr(transparent)] Cell(u64)` instead of a `[]u64` memset — same bytes written.
- screen.rs:1978-1992, 2969-2976, 3539-3548 wrap bodies in `_inner` fns so `assert_integrity` runs on error exits like Zig `defer`; debug-only assertion placement, no observable change.
- screen.rs:2966-2968 `#[inline]` on `resize` mirrors the oracle's `pub inline fn` for the rows benchmark.
- Dirty is a plain struct of bools rather than a packed struct; Cursor derives Copy to mirror Zig by-value copies; raw `*mut` pointers replace Zig pointers throughout (Stacked-Borrows discipline) with identical observable behaviour.


**Shard 134** — Screen.zig

- set_attribute: the errdefer is realised as a closure invoked only on the single fallible call (manual_style_update); identical observable behaviour.
- manual_style_update / append_grapheme: Zig `defer assertIntegrity()` is realised as an outer wrapper around an `_inner` fn so the integrity gate runs on error paths too; no behavioural change.
- start_hyperlink: URI/ID bytes are copied into owned Vecs BEFORE the implicit-id counter is bumped (Zig dupes after the bump under an errdefer); observable state on failure (counter unchanged) is identical. The `start_hyperlink_raw` raw-pointer variant exists only to permit the oracle's 'source slices may alias the current hyperlink' contract under Stacked Borrows.
- resize_tracked: the surviving-node style/hyperlink release derefs the live pin's node pointer rather than the saved `cursor_node` pointer (same address, live provenance) for SB soundness; identical effect.
- select_line / prompt_click_line: cells slices are borrowed as `&[Cell]` from the raw page pointer instead of Zig's `[]Cell`; same iteration and comparisons.
- selection_string / dump_string_alloc return an unsentineled Vec<u8> (length == Zig sentinel-exclusive len) instead of `[:0]const u8`; documented that no consumer reads the NUL.


**Shard 135** — Screen.zig

- Two rustty tests in this range ('Screen: cursorAbsolute to page with insufficient capacity' at screen.rs:5989 and 'Screen: scrolling when viewport is pruned' at screen.rs:13017) carry #[cfg_attr(all(miri, debug_assertions), ignore)] so they skip only in the Miri+debug-assertions lane for budget reasons; they run under plain cargo test with full assertions, so this is a tooling/perf-lane annotation, not a weakened test.
- rustty's 'Screen: cursorCellEndOfPrev across mixed-width pages' (screen.rs:11573) wraps the oracle's direct `first.page().size.cols = 2` mutation in pause_integrity_checks(true/false) and restores cols=4 before deinit; the assertions are identical (cursor node == second, expected cell ptr == cursor_cell_end_of_prev()), the extra scaffolding only keeps Rust's debug integrity asserts from firing on the intentionally inconsistent modelled state.


**Shard 136** — Screen.zig

- screen.rs:7629 'resize errors preserve state' compares selection via is_none()==is_none() instead of a full std.meta.eql; equivalent here because the fixture never sets a selection (both sides None) and any spurious selection after a failed resize would still be caught.
- Rust resize tests pass an explicit `reflow: true, prompt_redraw: Redraw::False` where Zig relies on struct defaults (`reflow = true`, `prompt_redraw = .false`, Screen.zig:2010/2016) — identical inputs.
- Rust tests use helper fns mkscreen/clone_clear_init/dump/vp/scr/act wrapping init+dumpStringAlloc; observable assertions unchanged.


**Shard 138** — Screen.zig, ScreenSet.zig

- ScreenSet: std.EnumMap(Key, *Screen) / EnumMap(Key, usize) are ported as fixed [Option<*mut Screen>; 2] / [usize; 2] arrays indexed by Key as usize — identical observable behaviour.
- ScreenSet.init/get_init: the oracle allocates uninitialized storage then inits in place (with errdefer destroy); the port constructs the Screen first and only Box-allocates on success — same success/failure outcomes, no leak on the init-error path.
- Screen tests: hyperlink URIs/IDs and selection strings are Vec<u8>/&[u8] instead of Zig []const u8 (byte-faithful); FailingAllocator replaced by the rustty_util tripwire fail-point seam at the Box::new(dupe) allocation.


**Shard 139** — Selection.zig

- None. The Rust port keeps the same pointFromPin-based ordering (including the NOTE(mitchellh) inefficiency comment) and the same cell/row iterator walks; no algorithmic substitutions observed.


**Shard 140** — SelectionGesture.zig, StringMap.zig

- string_map.rs Match.map: Zig copies the StringMap value (two non-owning slice headers) into each Match; Rust `shallow_copy` makes an owning deep clone of `string` and `map` per Match (extra allocation, read-only surface, identical observable results).
- selection_gesture.rs `Press` is made `Copy` so the by-value reuse after press_initial works; no observable difference.
- selection_gesture.rs `left_click_count` is u8 instead of Zig u3; value is clamped to 3 by @min in both, so widths are unobservable.
- selection_gesture.rs time_since uses `as_nanos() as u64` (truncating u128->u64) where Zig does `@min(delta, maxInt(u64))`; unreachable difference for std::time::Instant durations.


**Shard 141** — Tabstops.zig, Terminal.zig

- printSliceStoreRun / the wide-pair store / the simple-cell and same-style scans: the reference uses explicit @Vector ops (splat, intCast widen, shift, std.simd.interlace, @reduce/@ctz); the port writes fixed-`lanes`-width scalar windows (terminal.rs:202-235, 271-284, 1523-1543, 1714-1732, 1814-1828) relying on LLVM auto-vectorization. Results are bit-identical.
- print's three cold arms (grapheme clustering, zero-width attach, wide char) are outlined into #[cold] #[inline(never)] helpers (terminal.rs:2098-2540) purely for frame-size reasons; bodies moved verbatim.
- printSliceFill is split into print_slice_fill / print_slice_fill_inner and print into print / print_inner solely to model Zig `defer screen.assertIntegrity()`; no behavioural change.
- Tabstops carries the allocator as a generic type parameter + Box<[u8]> instead of threading `alloc` into set/get; identical observable behaviour (allocation happens at the same call, same rounding via div_ceil, tail-only zeroing).
- The reference's `resize to existing capacity does not allocate` test proves no second allocation via a FixedBufferAllocator; the port proves the same property via the pre-existing tripwire fail point (asserts resize succeeds AND the fail point was never reached) — equivalent, arguably stronger.


**Shard 142** — Terminal.zig

- indexScrollWithImages: Zig `comptime op` parameter is a plain runtime enum parameter in Rust (terminal.rs:3762-3772); identical branch selection, only codegen differs.
- Zig `defer`/`errdefer` blocks in index, scrollDown, scrollUp, insertLines, deleteLines, decaln and resize are reproduced via RAII guards (SemanticIndexGuard, CursorRestoreGuard, ShiftLinesEndGuard, TrackedPinGuard) or explicit early-return rollback (resize_rollback_geometry); drop/execution order matches Zig's LIFO defer order (verified for insertLines/deleteLines untrackPin-before-cursorAbsolute and scrollDown/scrollUp kitty end()-before-cursor-restore).
- insertBlanks backwards pointer loop: Rust breaks when `x == left` instead of computing a one-before-`left` pointer (terminal.rs:5136-5146); same swaps in the same order.
- insertLines/deleteLines row swap uses `core::ptr::swap` instead of the three-assignment swap; same memory effect.
- printAttributes formats each piece through `format!` into a scratch String then copies into the caller buffer (write_print! macro, terminal.rs:5824-5833); byte output and NoSpaceLeft-on-overflow semantics are identical to the Zig fixed writer.
- KittyScrollMargins disabled-build stub type is not needed in Rust: the real ScrollMargins is always compiled and the call sites are gated on the `options::KITTY_GRAPHICS` const (terminal.rs:3800-3814).
- compressionActivity: the oracle's u48 serial is stored as a u64 pre-masked to 48 bits in pagelist/compress.rs:179, so the read is a plain load with identical values.


**Shard 143** — Terminal.zig

- Tripwire fail-point checks in resize() are `#[cfg(test)]` (compiled out of non-test builds) versus Zig's `tripwire.module` that is inert unless `builtin.is_test`; observable behaviour is identical.
- `resize_rollback_geometry` re-sets synchronized_output on the error path even when it was already false (bumping an internal debug counter); no observable difference from the Zig errdefer.


**Shard 144** — Terminal.zig

- Test-only: rustty tests dereference raw `*mut Cell`/`*mut Row`/`*mut Page` via accessor methods (content_codepoint(), wide(), has_grapheme(), memory_ptr()) instead of Zig field access; observable assertions identical.
- Test-only: rustty `terminal_scroll_clear_across_pages_keeps_cursor_hyperlink_refs_page_local` uses `vec![0u32; cap]` in place of `alloc.alloc(u32, cap)` + `@memset`; same holder-count logic.


**Shard 146** — Terminal.zig

- terminal_delete_lines_across_page_boundary_marks_all_shifted_rows_dirty carries a `#[cfg_attr(all(miri, debug_assertions), ignore = ...)]` Miri-budget exception (terminal.rs:15390-15393); it runs normally under cargo test and in the debug-assertions-off Miri lane, so no assertion is lost.
- Rust tests access cells/rows via raw pointers and accessor methods (`(*cell).content_tag()`, `(*row).hyperlink()`, `Node::page(node)`) in place of Zig field access; observable assertions identical.
- Rust test fn `terminal_deleteLines_hyperlink_dense_row_crosses_page_boundary` keeps camelCase `deleteLines` in its identifier (cosmetic naming only; the `// test "..."` anchor comment is correct).


**Shard 147** — Terminal.zig

- The differential fuzz test carries `#[cfg_attr(miri, ignore)]` (terminal.rs:17733-17739) — a Miri-runtime-only exclusion documented in .port/phase-2/MIRI_EXCEPTIONS.md; the test still runs under normal `cargo test`, so no functional/test-coverage difference.
- `deleteChars with a spacer head at the end` reads the cleared cell via `Cell::codepoint()` (tag-aware, returns 0 for bg-color tags) where the oracle reads the raw union field `content.codepoint.data`; for the blank cell asserted (default pen -> codepoint tag) both yield 0, so the assertion is equivalent.
- Test scaffolding differences that are pure Rust idiom: explicit `t.deinit(Global)` instead of `defer`, `opts(cols, rows)` helper, `active_pt`/`screen_pt` point constructors, `unsafe { (*t.screens.active)... }` raw-pointer reads of the active screen, and `String::from_utf8_lossy` only in the diagnostic assert message of the differential helper.


**Shard 148** — Terminal.zig, UTF8Decoder.zig, ansi.zig

- utf8_decoder.rs:44-66: TRANSITIONS is padded from 108 to 512 entries (pad = REJECT) so `state + char_class` indexing is provably in-bounds; entries 0..108 are verbatim and guarded by `transitions_table_matches_reference_with_reject_padding`. Reachable indices max out at 107, so no behaviour change.
- utf8_decoder.rs:129-175: `next()` computes both accumulator candidates unconditionally and selects, loads the transition into a local, and stores `self.state` per-arm (accept arm stores the literal ACCEPT_STATE). Equivalence to the branchy oracle body is machine-checked exhaustively by `reshape_matches_reference_control_flow` (all reachable states x 4 accumulator patterns x 256 bytes, all 2-byte prefixes, 1M-byte xorshift stream).
- utf8_decoder.rs:71: accumulator is u32 instead of u21; the DFA never lets more than 21 bits accumulate from a valid lead byte, and the first-byte mask `& 0x1F_FFFF` is a no-op, so emitted codepoints are identical.
- terminal.rs:17727-17734: the differential fuzz test is `#[cfg_attr(miri, ignore)]` (documented in MIRI_EXCEPTIONS.md); it still runs under normal cargo test.
- terminal.rs:19525-19528: `resize with high unique style per cell with wrapping` is ignored only under `all(miri, debug_assertions)`; runs everywhere else.


**Shard 149** — apc.zig, bitmap_allocator.zig, build_options.zig, charsets.zig

- bitmap_allocator.rs: `find_free_chunks` iterates by index (`for idx in 0..bitmaps.len()`) instead of the oracle's `for (bitmaps, 0..) |*bitmap, idx|`; identical results and mutation order.
- bitmap_allocator.rs: `div_exact` is a `debug_assert!`-checked `/` (traps on inexact input in assert-bearing builds like Zig Debug/ReleaseSafe; floors in release where the oracle is UB) - no observable difference on valid inputs.
- apc/mod.rs: `UnknownBuilder.append` uses `Vec::reserve_exact(capacity - len)` for the oracle's `ensureTotalCapacityPrecise(capacity)`; the retained-byte count and truncated flag are computed identically. The OOM arms (`catch { self.truncated = true; return; }`, `toOwnedSlice` failure) have no analog because the global allocator aborts on OOM.
- apc/mod.rs: `Identify.len` is u8 instead of u3 and the identify buffer starts zeroed instead of `undefined`; only `buf[0..len]` is ever read.
- build_options.rs: `Options::add` returns a `Resolved` value instead of writing into a `std.Build.Module`; the option keys, values, and emission order are the same; `Features` is a plain struct instead of a packed struct (the backing integer only served the test's `@bitCast(0)`, ported as `Features::NONE`).


**Shard 150** — clipboard.zig, color.zig, compress.zig, csi.zig, cursor.zig, dcs.zig, device_attributes.zig

- color.zig:139-184 paletteCvalSlice: the LE @Vector(16,u8) 4-to-3 byte shuffle fast path is replaced by a plain element copy in color.rs:469-478 because rustty's RGB is already a 3-byte #[repr(C)] struct identical to RGB.C; identical output, the every-length test sweep is retained (color.rs:2591-2638).
- color.zig:637-640 `@sizeOf(RGB) == 4` (packed struct(u24) stored in u32) vs rustty `size_of::<RGB>() == 3` (color.rs:1107-1113, test rgb_layout 1842-1855). Memory footprint of Palette differs (1024 vs 768 bytes); no observable behavioural effect, and the C ABI type is 3 bytes in both.
- color.zig:10-47 comptime default palette is a `const fn build_default()` evaluated into a `static DEFAULT` (color.rs:36-90); same values, same construction loop.
- color.zig:194 PaletteMask = std.StaticBitSet(256) is a hand-rolled StaticBitSet256 over [u64;4] (color.rs:485-532) providing initEmpty/set/unset/isSet/count/iterator; `iter_set` scans 0..256 with is_set (O(256)) instead of a word-wise bit iterator — identical ascending order.
- dcs.zig:326/229 std.Io.Writer.Allocating replaced by an allocator_api2 Vec-backed `Allocating<A>` (dcs.rs:600-640) using try_reserve to keep the fallible OutOfMemory path.
- dcs.zig:8/33/37/117 scoped `log.info/debug` calls are no-op comments in dcs.rs (crate has no logging facade); log text only.
- color.zig:1327-1357 paletteCvalSlice test poisons whole RGB elements with 0xAA rather than raw bytes (byte-equivalent since size_of::<RGB>()==3 is asserted).


**Shard 151** — device_status.zig, focus.zig, formatter.zig

- formatter.rs: `BufOut` 4096-byte buffered adapter wrapped around the io::Write sink for the duration of one PageFormatter format (stand-in for Zig's buffered std.Io.Writer); same bytes, same order.
- formatter.rs: `write_cell_run` / `encode_codepoint` monomorphized via const-generic EMIT (mirrors the Zig comptime `emit` param); `run_buf`/`style_buf` scratch hoisted to the caller frame to avoid zeroing memsets.
- formatter.rs: HTML/VT header assembled in a `Vec<u8>` instead of a fixed 1024/512-byte stack buffer (oracle would return WriteFailed on overflow, which cannot happen for the fixed-size header content).
- formatter.rs: PinMapMap `set_node`/`append` are infallible (Vec growth aborts on OOM) where the oracle maps Allocator.Error to WriteFailed.
- device_status.rs: MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE frozen to 9 instead of computed at comptime by dry-running the encoder; test asserts 9 and the encoder outputs are 9 bytes.


**Shard 152** — formatter.zig

- writeGraphemeCps: Zig comptime `emit` is a runtime `match emit` dispatching to the const-generic `encode_codepoint::<EMIT>` (formatter.rs:2356-2360); output identical.
- encodeCodepoint: `len` passed by value and returned rather than `*usize` (formatter.rs:2674) to drop a bounds check; `#[inline(always)]` mirrors Zig `inline fn`.
- formatStyleOpen (vt): uses `Style::format_into(&mut [u8;128])` and one `write_all` instead of `formatter.format(writer)` through a fmt adapter (formatter.rs:2574-2575); the byte sequence is the same SGR emission (style.rs:670-739).
- appendBlankPoints / formatStyleClose / formatHyperlinkClose: Zig `ensureUnusedCapacity ... catch return error.WriteFailed` becomes `Vec::reserve`/`push` (allocation failure aborts) — standard Rust allocation idiom used throughout the port, not an observable difference under normal operation.
- write_codepoint_utf8 (formatter.rs:163-166) PORT NOTE still cites `writer.print("{u}")` while the oracle now calls `printUnicodeCodepoint`; behaviour (U+FFFD fallback) is identical, comment-only staleness.


**Shard 153** — formatter.zig

- Rust tests use Vec<u8> as the writer and `trim_start(full_output, b"\n")` / `b"\r\n"` helper in place of std.Io.Writer.Allocating + std.mem.trimStart; identical observable assertions.
- Rust multi-page PageList tests re-derive the `pages` raw pointer via a closure after each vt_stream() write (Stacked-Borrows hygiene, PORT NOTE WS19 T6) instead of caching `&t.screens.active.pages` once as the Zig test does; test-only, no behavioural difference.


**Shard 154** — formatter.zig, fraction.zig

- formatter.rs tests build output into a Vec<u8> and clone it instead of std.Io.Writer.Allocating buffered() slices; identical observable bytes.
- fraction.rs uses u8::is_ascii_digit() and a RangeInclusive::contains for the [0,1] check instead of explicit comparisons; semantics (including NaN rejection and -0 acceptance) are identical.


**Shard 155** — hash_map.zig, highlight.zig, hyperlink.zig, kitty.zig, lib.zig, main.zig

- hash_map.rs: `#[inline]` on get_index/get_entry_adapted/get_context/get_adapted and the one-shot `Wyhash::hash` small-key path via `SmallKeyBytes` (bit-identical digest to the iterative route) — performance only.
- hash_map.rs: `remove_by_index_context` runs `debug_assert_canonical` after every removal in debug builds (an O(capacity) check the oracle only does in tests); compiled away in release, so not observable.
- highlight.rs: `Flattened.chunks` is a `Vec<Chunk>` (AoS) instead of `std.MultiArrayList(Chunk)` (SoA); identical observable content/order.
- hyperlink.rs: `SetContext` stores page pointers as exposed-provenance `usize` addresses instead of raw pointers (Stacked Borrows soundness); identical behaviour.
- hyperlink.rs: `PageEntryId` is a byte-complete `#[repr(C)]` tagged struct with zeroed inactive payload instead of a Rust enum; same 24-byte size/alignment, avoids uninit-byte reads by the page compressor; value semantics unchanged.


**Shard 156** — mem.zig, modes.zig, mouse.zig, osc.zig

- modes.rs: ModePacked is a u64 bitset indexed by ENTRIES position and Mode->index lookup is a linear scan of MODE_BY_INDEX instead of comptime field resolution; all set/get/save/restore/reset results are identical (8-byte size asserted).
- modes.rs: debug_assertions-only thread-local counter of synchronized_output clears (sync_output_clear_count) — instrumentation with no release-observable effect.
- mouse.rs: StaticStringMap replaced by a OnceLock-built HashMap<Vec<u8>, Shape> over the same 56 pairs; exact byte-key lookups are identical.
- osc/mod.rs: Capture fixed backing stores a running length into Parser.buffer instead of a std.Io.Writer over it; allocating backing is a Vec grown with reserve_exact to the same bounded capacities (tests assert capacity == oracle buffer.len). Terminator::cval rebuilds the C EnumType descriptor on each call.
- osc/mod.rs: Parser::next is #[inline(always)] to mirror the oracle's @call(.always_inline) at its call sites; the comptime @sizeOf(Command) == 64 assert is not mirrored (Rust enum layout, not observable).
- mem.rs: madvise is declared via a hand-rolled extern "C" instead of std.posix; cfg!(miri) and a `test-decommit-sim` feature join cfg!(test) on the simulated-reclaim arm.


**Shard 157** — page.zig

- clonePartialRowFrom different-page managed path (page.zig:895-984) is reshaped in page_impl.rs:1197-1346 around maximal runs of style-only cells sharing one style_id: first cell goes through the verbatim body (add_with_id_ptr), run-mates are collapsed into one `use_multiple(id, k-1)` only when `add_with_id_is_pure_ref_bump` proves the reference's per-call path would be the plain `ref += 1` arm (ref_counted_set.zig:347-355), then bulk-copied; kitty placeholder row flag accumulated and applied after. End state and error prefix state are identical.
- Grapheme/hyperlink arms of the managed-copy loop are outlined into #[cold] helpers `clone_cell_graphemes_from` / `clone_cell_hyperlink_from` (page_impl.rs:1543-1681); same calls, same order, same error mapping. The grapheme arm snapshots the codepoints into a Vec before set_graphemes (extra allocation, identical result).
- Managed-copy loop reads the source cell once after the dst reset store (`copy_with_managed_reset` = single masked store for `dst.*=src.*` + the three resets) instead of the reference's per-branch reloads; documented as observably identical including the aliasing same-page case.
- Styles passed by pointer into the other page (`style_get_ptr_of` + `add_with_id_ptr`) instead of a 28-byte copy; hyperlink add likewise by pointer.
- exactRowCapacity's `std.StaticBitSet(65536)` on the stack is a boxed [u64;1024] with a cached count (page_impl.rs:2696-2724); identical set semantics.
- verifyIntegrity uses std HashMap for styles_seen/hyperlinks_seen instead of an arena-backed AutoHashMap; the `runningOnValgrind()` early return has no Rust analogue and is omitted (not observable).
- get_row_and_cell / lookup_hyperlink / layout carry #[inline] to compensate for the reference's single-compilation-unit inlining.
- Cell/Row are #[repr(transparent)] u64 wrappers with bit-field accessors reproducing the packed struct layout (verified by layout tests); `Cell::is_style_run_cell` is a bit-test shortcut for `!hasGrapheme && !hyperlink`.


**Shard 158** — page.zig

- Mask group ops use fixed-size [Backing; GROUP_LEN] loops instead of @Vector/@reduce; identical predicates (page.rs:1962-2176 vs page.zig:2417-2560). Extra eql_group / eql_any_group (&[T; N] variants, XOR/min reductions) are additions with no oracle counterpart, purely for bounds-check elision.
- Page::layout is #[inline(always)] and Capacity::adjust/available_bits_for_grid are #[inline] to mirror Zig's `pub inline fn`; no behavioural effect.
- Cell::copy_with_managed_reset / is_style_run_cell (page.rs:889-930) are masked-store fusions of the sequential resets in clonePartialRowFrom; machine-checked equivalent by cell_copy_with_managed_reset_equivalence / cell_style_run_facts tests.
- Row::reset uses debug_assert! where move_hyperlink/set_graphemes use assert!; both are at least as strict as the corresponding Zig std.debug.assert (compiled out in ReleaseFast) / slow_runtime_safety checks.


**Shard 159** — page.zig, parse_table.zig, paste.zig, point.zig

- parse_table.rs builds the transition table in a `const fn` into a `static TABLE` instead of Zig comptime `genTable()`; the accumulated OptionalTable + `orelse transition(state, .none)` fill is reproduced and the resulting table is byte-identical.
- page_tests.rs carries three rustty-only regression tests (page_cloneFrom_style_run_refcounts_{batched,conflicting_dst_id,dead_dst_id}) guarding a PERF reshape of the style-run batch in clone_partial_row_from_inner; they pin the reference's per-cell refcount/id-mapping results, so they are additive coverage, not a behavioural divergence.
- paste.rs `Context` drops the `alloc` field (global allocator, infallible `grants.grant`); observable behaviour identical apart from OOM on grant being unrepresentable.


**Shard 160** — ref_counted_set.zig

- ref_counted_set.rs:544-732 — public by-value add/add_with_id/lookup are #[inline] shims over *const T bodies (add_context/add_with_id_ptr/lookup_context/insert) to avoid marshalling large T per call; single copy of T happens in insert exactly as the reference's by-value semantics.
- ref_counted_set.rs:751-766 — extra add_with_id_is_pure_ref_bump predicate used by page_impl.rs:1298 to batch k identical add_with_id calls into one use_multiple; behaviour-preserving only because the style context's `deleted` is the trait default no-op (style.rs:1057-1063 defines only hash/eql, matching style.zig:686-691 which has no deleted).
- ref_counted_set.rs:841-862 — get split into items_ptr + get_at (hoisted base resolution for render loops); get(base,id) == get_at(items_ptr(base),id) with identical asserts.
- ref_counted_set.rs:456-508 and 1154-1164, 1238-1242 — item array zeroed byte-wise and Item written field-wise (instead of typed whole-struct writes) so padding bytes stay initialized for the page compressor under Miri; value bytes identical.
- ref_counted_set.rs:1370-1381 — ref_from_id converts the `n: Id` of release_multiple to RefCount by repeated addition (O(n)) instead of an integer cast; same value.


**Shard 161** — render.zig, search.zig, selection_codepoints.zig

- render.rs: `row_data` is `Vec<Row>` (AoS) instead of `std.MultiArrayList(Row)`; per-row cells are a hand-rolled SoA `CellList` (raw/style/grapheme_spans + shared `grapheme_buf`) replacing `MultiArrayList(Cell)` + per-row ArenaAllocator; identical observable contents.
- render.rs `fill_styles` uses `<[T]>::fill` instead of the oracle's manual two-overlapping-16-byte vector stores; the `16 <= sizeof(Style) <= 32` comptime assert is preserved as a const assert.
- render.rs `row_builder` hoists the style-set base pointer once per row and `rebuild_raw` only resizes style/span columns when the column count changes (stale-but-unread slots, matching the oracle's 'undefined unless flagged' contract).
- render.rs `RowDirtyMask`/`CellSpecialMask` are const-generic `page::Mask` instantiations (match_group/match_scalar/eql/eql_exact/bits/pattern) mirroring `page.Mask(...)`.
- render.rs `link_cells` uses rustty_zigstd AutoArrayHashMapUnmanaged for CellSet; `string()` writes into `Vec<u8>` instead of `*std.Io.Writer` (only WriteFailed can arise).


**Shard 162** — sgr.zig, size.zig, size_report.zig

- sgr.zig @branchHint(.likely/.unlikely/.cold) annotations and @call(.always_inline, SepList.isSet) have no Rust counterpart beyond #[inline] on is_colon; behaviour identical.
- rustty's SepList::is_set (parser.rs:338) returns false for idx >= MAX_PARAMS where Zig's StaticBitSet.isSet would safety-assert; this only makes rustty more lenient on the (unreachable for valid input) out-of-range probe in the colon-consumption loop and produces no different output. SepList itself belongs to Parser.zig (other shard).
- OffsetSlice<T> carries an explicit `_padding: u32` (64-bit only) so the page mapping has no uninitialized bytes; same size/align (16/8) as the oracle's auto-layout struct.


**Shard 163** — stream.zig

- stream.rs:1180-1193 printable-run scan: oracle uses a `@Vector(lanes,u32)` compare + `@reduce(.Or)` + `@ctz`; port spells it as a scalar bitmask accumulate over a `[u32; lanes]` window with `trailing_zeros` — identical run boundaries (lanes = rustty_simd::lanes(4) = 2x native, same as simd.lanes(u32)).
- stream.rs:1470-1485 / 1544-1555 APC/OSC prescans: oracle vector compare + reduce; port is a per-lane scalar OR over the same native-width window (APC_SCAN_LANES/OSC_SCAN_LANES = native bits/8, matching std.simd.suggestVectorLength(u8)); stop positions identical.
- stream.rs:1998-2000 `*csi_action`/`*esc`/`*cmd`: rustty boxes pointer-sized parser Action payloads and unboxes here (PERF(port) note); handler signatures/values unchanged.
- stream.rs:2055-2060 csi_dispatch_final builds `parser::CSI` with owned `InlineVec::from_buf` copies instead of the oracle's slices into parser buffers; same content and the same construction the scalar parser path uses.
- stream.rs:823-833 Tracker allocates on the global allocator; `Options.allocator` is used only as the presence gate (oracle passes the allocator into Tracker.init). Observable gating (allocator present AND max_bytes>0) is identical.
- Rust `debug_assert!` used where the oracle uses `quirks.inlineAssert` (which is also compiled out in Release modes) for state preconditions in csi_entry_byte/consume_csi_params/next_utf8/next_non_utf8; `assert!` kept where the oracle asserts in next_slice_capped/consume_apc_string/consume_osc_string.


**Shard 164** — stream.zig

- Zig @branchHint(.likely/.unlikely/.cold) annotations throughout csiDispatch/oscDispatch/escDispatch are preserved as comments only; no behavioural effect.
- SGR arm (stream.zig:2059-2066 vs stream.rs:2885-2895) copies the parser SepList bit-by-bit into sgr::SepList because the two are distinct Rust types; the separator set observed by the SGR parser is identical.
- Kitty '=' arm folds the oracle's two-step (select Action.Tag, then switch on it) into a single match on `number` (stream.rs:3447-3467); dispatch targets and the invalid-number early exit are identical.
- Oracle's inline `fn` dispatchers are plain methods in Rust (inlining left to the optimizer).


**Shard 165** — stream.zig, stream_continuation.zig

- stream_continuation.rs find_vt_replay_start: the oracle's @Vector/@reduce/@clz backward ESC scan (unroll 4) is emulated with scalar loops over the same lane/unroll geometry (ESC_SCAN_LANES from rustty_simd::native_vector_bits); identical result (index of the last ESC or None), just no SIMD.
- Tracker allocation: oracle stores an explicit Allocator and ArrayList; rustty uses a global-allocator Vec<u8> whose real OOM aborts, with the `catch { markBroken }` arms preserved only through the test-only TRACKER_TW tripwire seam (documented PORT NOTE). Observable behaviour under non-OOM conditions is identical.
- Rustty apc bulk tests drive `consume_until_ground` directly (drive_bulk/drive_bulk_into) rather than `next_slice`, because both trees' `nextSlice` degrade to per-byte in debug builds; this exercises the bulk path in every configuration and asserts a superset of the oracle's SIMD-guarded assertions (rustty-side strengthening, not a gap).
- Tracker::deinit assigns `Vec::new()` instead of an explicit free; equivalent lifecycle.


**Shard 166** — stream_terminal.zig

- Handler::paste: the oracle PtyWriter drains 4096-byte chunks to write_pty during the paste; the port's PtyChunker queues the identical chunk sequence and delivers it (also on error) immediately after `paste_pkg::paste` returns (stream_terminal.rs:733-796). Paste takes `&Terminal` (oracle `*const Terminal`), so callback-observable state is the same; only timing differs.
- std.heap.stackFallback + Allocating writers (kittyClipboardRespond, kittyDnd, reportDeviceAttributes, reportSize, colorOperation, kittyColorOperation, ClipboardReadState.respond, respondSuccess) become owned Vec/String accumulators; NUL sentinels for `[:0]const u8` callback slices are omitted since write_pty takes `&[u8]` (capacity budgets kept: 127/288/MAX_TN_RESPONSE_BYTES/DECRQSS max+1).
- XTGETTCAP map built once via OnceLock at first use instead of comptime.
- vtFallible's single comptime switch is distributed over `impl stream::Handler` trait methods (monomorphized per action); the shared catch is `vt_catch`.
- Kitty clipboard WriteState is `Option<Box<_>>` taken out of the slot during data/alias/commit and put back on success instead of a raw `?*` pointer; ReadState id/pw/mimes are owned copies instead of borrowed slices.


**Shard 167** — stream_terminal.zig

- Test-harness only: Zig local-struct `var` statics captured by callbacks become `thread_local!` cells (CAPTURE/COUNTER, DN_*, PR_*, CLIP_*, CR_*, KC) because Rust effect callbacks are bare fn pointers; the oracle's fixed capture arrays ([1024]u8 responses, [8][64]u8 mimes, [8][256]u8 data) become growable Vecs, which never truncates captured data (the oracle silently caps at 8 contents); no observable assertion differs.
- Test-harness only: comptime `++`/`bytesToHex` query construction in the XTGETTCAP tests is replaced by runtime test_hex/test_concat helpers producing byte-identical sequences.
- Test-harness only: `clipboard_write allocation failure is ignored` injects the allocation failure through the CLIPBOARD_CONTENTS_TW tripwire fail point instead of swapping `t.screens.active.alloc` for testing.failing_allocator (rustty's allocator is the zero-sized Global); the observable assertions (count 0, no semantic_failure) are kept and extended.


**Shard 168** — stream_terminal.zig

- Test capture state: the reference uses per-test local `S` structs with `var` statics / fixed [64]u8 or [128]u8 buffers; rustty routes capture through `thread_local!` cells with owned Vec<u8> (cap_write_pty/count_write_pty/kc/pc/dnd helpers). Observable assertions are identical.
- `kitty clipboard allocation failure is ignored`: the reference swaps `t.screens.active.alloc` to `testing.failing_allocator`; rustty uses the KITTY_CLIPBOARD_TW tripwire fail point (`KittyClipboardFp::CreateWriteState`) at the same `create(WriteState)` site — same observable outcome (no transaction, no semantic_failure, no responses).
- `paste: mode 5522 without entropy`: `std.Io.failing` maps to `t.io = sys::io_failing` plus a process-wide `random_secure_test_lock`; three 5522/reader tests carry `#[cfg_attr(miri, ignore)]` only under Miri (real OS RNG), unaffected under normal cargo test.
- `std.base64.standard.Encoder.calcSize(otp_len)` is computed inline as `otp_len.div_ceil(3) * 4` (same value).


**Shard 169** — style.zig, sys.zig, tmux.zig, x11_color.zig

- style.rs Flags::eq compares the 9 raw bytes via transmute instead of repacking to u16 (exhaustively proven equivalent by `flags_eq_matches_packed_eq`).
- style.rs Color is a byte-complete repr(C) {tag, [u8;3]} instead of a Rust enum; Color::eql compares a u32 LE key (proven equivalent by `color_eql_matches_reference`). `Style::default_style` delegates to `eql(&Style::default())` rather than baking the packed default on wasm.
- style.rs VtFormatter splits a `format_into(&mut [u8;128])` byte core out of `format` so hot callers can write bytes directly; append_color/append_color_rgb take whole-buffer+cursor and are #[inline(always)].
- sys.rs decode_png_wuffs copies the decoded pixel buffer into a global-allocator Vec (`result.bytes().to_vec()`) instead of the oracle's zero-copy handoff; the decode itself still runs through the caller's (limited) allocator.
- x11_color.rs builds the case-insensitive map, the original-case key list and the entries table lazily via OnceLock at first access instead of comptime; rgb.txt is byte-identical and has no case-insensitive key conflicts so HashMap last-wins vs StaticStringMap is unobservable.


**Shard 170** — glyph.zig, Glossary.zig, execute.zig, request.zig, response.zig

- request.rs decode_glyf_payload uses a heap Vec scratch buffer instead of the oracle's stackFallback(64KiB) allocator; size is capped identically so behaviour is unchanged.
- request.rs expands the comptime-generic Register.get(option)/Option.read/default into per-option typed accessors (get_cp, get_fmt, ...) and default_* fns; same branch logic per option.
- request.rs inlines a verbatim port of std.base64.standard.Decoder (fast 16/4-byte paths + tail) rather than depending on a base64 crate; identical accept/reject semantics.
- glossary.rs register() uses contains()+ordered_remove+put instead of getOrPut/putAssumeCapacity; same FIFO ordering and eviction observable behaviour.
- decode_limited threads the per-allocation limit explicitly (each Vec allocation site checks its request size) instead of a LimitedAllocator vtable; real OOM aborts so the oracle's OutOfMemory-vs-PayloadTooLarge discriminant collapses to PayloadTooLarge with identical observable results.


**Shard 171** — allocator.zig, build_info.zig, cell.zig, color.zig, color_scheme.zig, focus.zig, formatter.zig, grid_ref.zig, grid_ref_tracked.zig

- color.rs x11_entries(): the oracle builds the null-terminated X11Entry table at comptime; rustty builds it once at runtime in a OnceLock and leaks CString names. Same contents, same pointer stability (x11_names static test present).
- color_scheme.rs report_encode / focus.rs encode: encode via Vec/FixedWriter+CountingWriter instead of std.Io.Writer.fixed + Discarding re-run; outputs, out_written and Result values identical (prefix-fill semantics reproduced).
- formatter.rs: FormatterWrapper stores the construction inputs and rebuilds a TerminalFormatter per format/format_buf/format_alloc call instead of holding a live formatter; the oracle formatter is stateless across calls so output is identical.
- formatter.rs format_alloc (non-empty path): formats into a Vec and performs a single alloc+memcpy through the resolved C allocator, whereas the oracle's Allocating writer issues grow/remap/free callbacks as it fills; the C caller receives the same bytes/len and frees with the same allocator (the empty-output case is reported as a finding).
- build_info.rs: builtin.mode is derived from cfg!(debug_assertions)/OPT_LEVEL cfgs (four-way mapping) rather than a single compiler-provided mode value; profile-pinned tests assert the ordinals.


**Shard 172** — io.zig, key_encode.zig, key_event.zig

- io.rs ReaderAdapter drops the reference's 4 KiB `transfer_buffer` (used only when the destination std.Io.Writer is unbuffered); the Rust trait pumps through the caller's destination slice instead, so the callback still receives the destination's exact capacity. Observable bytes/offsets identical; only the intermediate copy differs.
- io.rs WriterAdapter embeds a fixed 4096-byte buffer (`RECOMMENDED_BUFFER_LEN`) instead of taking a caller-owned buffer in `initBuffered`; every reference call site sizes the caller buffer at `recommended_buffer_len`, so the callback batching is the same. The buffered-splat test's lengths are scaled accordingly and keep the same boundary assertions.
- io.rs ReaderAdapter observables (offset/callback_failed/invalid_read/eof) are held in a separately boxed `ReaderState` cell accessed via raw pointers (Stacked-Borrows reason) instead of adapter fields; values and transitions are identical.
- key_event.rs keeps an owned Vec<u8> cache refreshed from the borrowed utf8 ptr/len on every use (`sync_utf8`) so the rustty-vt KeyEvent (which owns its utf8) sees the caller's live bytes; extra copy only, observable pointer identity and mutation visibility match the reference.
- key_encode.zig's setopt/OptionAsAlt range guards and key_event.zig's set_action/set_key guards are compiled only under `std.debug.runtime_safety`; rustty always guards (safe superset).


**Shard 173** — kitty_graphics.zig, main.zig

- kitty_graphics.rs:109-114 stores the map entry as a raw (key_ptr, value_ptr) tuple and the iterator as `Option<hash_map::Iter<'static,..>>` transmuted from a borrowed iterator, instead of Zig's `PlacementMap.Entry`/unmanaged `Iterator`. Same observable behaviour (the reference's unmanaged iterator is equally untracked); documented in a PORT NOTE.
- compute_viewport_pos widens the visibility sums to i64 (`vp_row as i64 + grid_size.rows as i64`) exactly as the oracle does at kitty_graphics.zig:2062-2065 — no divergence; noted only because the Rust code carries an R7 comment about the older i32 cast.
- Enum selectors are narrowed from a plain `c_int` via `from_c_int` in the extern wrappers (lib.rs:1983-2139) rather than being received as `enum(c_int)` parameters. This is a documented deliberate divergence (D11) covering UB the Zig oracle has no defined behaviour for; the in-range dispositions (including `invalid = 0` -> `.invalid_value`) are identical, and two extra tests (kitty_graphics.rs:3975, 4001) pin it.


**Shard 174** — modes.zig, mouse_encode.zig, mouse_event.zig, osc.zig, paste.zig

- mouse_encode.rs flattens the reference's stored `input_mouse_encode.Options` into scalar wrapper fields and rebuilds the Options (with the last_cell borrow) at each encode call; observable behaviour, including the last_cell reset semantics on event/format/size changes, is identical.
- paste.rs replaces the oracle's `std.heap.stackFallback(256, ...)` stack-first allocation for the mimes array with a plain Vec (allocation strategy only; the error-path consequence is filed separately as a finding).
- modes.rs / mouse_encode.rs replace `std.Io.Writer.fixed` + `std.Io.Writer.Discarding` with the crate's FixedWriter / CountingWriter; byte output, written counts and the out_of_space partial-prefix behaviour match (proved by the two extra Rust-only prefix tests).
- paste.rs tests use a Vec-backed capture buffer instead of the oracle's fixed [1024]u8 + len pair, and take a process-wide mutex because Rust runs tests in parallel; assertions are unchanged.
- osc.rs appends a NUL sentinel in place to the ChangeWindowTitle Vec before handing the pointer to C, reproducing the oracle's `[:0]const u8` `.ptr`; the extra byte is not observable through any other C surface.


**Shard 175** — render.zig

- Data-structure reshape (documented, behaviour-identical): the oracle's `RowIteratorWrapper` caches four parallel `std.MultiArrayList` column slices (`raws`/`cells`/`selection`/`dirty`); rustty caches one `*mut [RenderRow]` and projects `row_data[y].raw/.cells/.selection/.dirty`. Same for `RowCellsWrapper` (three SoA columns -> one `*const CellList`). All reads/writes, including `row_set(.dirty)` writing through the slice into the render state, are equivalent.
- rustty adds raw-`c_int` entry points (`get_checked`, `row_get_checked`, `row_cells_get_checked`) plus per-element range checks in the `*_get_multi` loops to carry the oracle's `std.enums.fromInt` runtime-safety guard, which the oracle keeps inside its `*Dispatch` inlines. Same log scope (`render_state_c`), same `log.warn` text, same `.invalid_value` disposition, same `out_written = i` on the failing element.
- `Dirty::from_c_int` / `set_typed` range-check the inbound `set(.dirty)` payload and return `.invalid_value` for an undeclared value. The oracle loads `value.*` directly (safety-checked illegal behavior in Debug, UB in ReleaseFast), so no defined oracle behaviour is changed; for every declared value the two are identical. Backed by the extra rustty tests `render_dirty_payload_is_int_sized`, `render_get_dirty_fills_the_whole_int_slot`, `render_set_dirty_reads_the_whole_int_slot`.
- Uninitialized-field substitution: the oracle's `row_iterator_new`/`row_cells_new` leave every field `undefined`; rustty writes sentinels (`y/x = POSITION_NONE`, empty rows slice / shared empty `CellList`, null palette). Both are only observable after `get(.row_iterator)` / `row_get(.cells)` install real values, and the sentinel makes the pre-install `next` calls return false rather than reading uninitialized memory.


**Shard 176** — result.zig, row.zig, search.zig, selection.zig

- `terminal_c.searches` is a Zig `AutoArrayHashMapUnmanaged` (insertion-ordered) vs rustty `HashSet<*mut SearchWrapper>` (unordered). The set is only iterated in `terminal::free` to write `search.terminal = null` on every member, so iteration order is unobservable.
- `SearchWrapper.alloc` is a `Box<dyn ZigAllocator>` (one extra Rust-global heap block per search) where Zig stores the fat `std.mem.Allocator` value inline. No observable behaviour change.
- rustty's `format_alloc` accumulates into a `Vec<u8>` and then copies into a C-allocator block, where the oracle's `std.Io.Writer.Allocating` grows the C-allocator block directly. Same returned (ptr, len) ownership contract; only allocation traffic differs. (The extra non-null-sentinel branch for a 0-length result matches Zig's `toOwnedSlice` non-null empty slice.)
- `codepoint_slice` is the identity reinterpret in rustty (`page::u21 == u32`) vs Zig's `@ptrCast` of `[*]const u32` to `[*]const u21`; the size/align comptime asserts are preserved and hold trivially.
- rustty applies the `std.enums.fromInt` range guards unconditionally (`get_checked`/`set_checked`) where the oracle gates them on `comptime std.debug.runtime_safety`; in Rust materializing an out-of-range `#[repr(C)]` discriminant is UB in every mode, so the always-on guard is strictly a superset with identical observable results in safety-enabled builds.


**Shard 177** — selection_gesture.zig, sgr.zig, size_report.zig

- selection_gesture: the reference's `union(EventType)` payloads store the borrowed `word_boundary_codepoints`/`behaviors` slices inline; rustty stores only the non-borrowing fields (PressFields/DragFields/AutoscrollTickFields/DeepPressFields) and rebuilds the borrowing `sg::Press`/`Drag`/`AutoscrollTick`/`DeepPress` at dispatch from the event-owned storage. Observably identical (set fields then dispatch).
- selection_gesture: `packed struct { bool x5 }` validation sidecar modeled as a plain `#[derive(Default)] struct` of bools; bit packing is unobservable across the C boundary (the struct is never exposed).
- selection_gesture: `?[]u21` backing storage becomes `Option<Vec<u32>>`; `&.{}` (empty) is `Some(Vec::new())` and `null` is `None` falling back to `DEFAULT_WORD_BOUNDARIES`, matching the reference's target-slice semantics exactly.
- sgr: `alloc.dupe(u16, params[0..0])` in the reference still enters the allocator vtable for a zero-length allocation, whereas rustty's `set_params` short-circuits `len == 0` to `(null, 0)` with no allocator call. No observable result difference (the reference's zero-length slice is likewise never freed, since `free`/`set_params` guard on `params.len > 0`); only the caller-allocator call count differs.
- sgr: rustty operates through raw pointers + `addr_of_mut!` rather than a retained `&mut *parser_` across `free_params`, to keep Stacked Borrows happy. Same field writes, same order.


**Shard 178** — snapshot.zig, style.zig, sys.zig

- style.rs `Style::write`: the reference uses a Zig `@Vector(8, u8)` select to spread the 8 flag bits into 8 bytes; rustty uses a const-foldable scalar `while i < 8` loop producing the same `u64::from_ne_bytes` value, then the same unaligned 8-byte store at `&out.bold`. Identical bytes, identical field offsets (const-asserted at style.rs:246-257).
- snapshot.rs `encode_alloc`: the reference grows a `std.Io.Writer.Allocating` directly on the caller's C allocator and hands over `toOwnedSlice`; rustty encodes into a Global-backed `AllocatingWriter` and then copies once into a C-allocator buffer. Same result code on OOM, same ownership contract (caller frees with rustty_free + the same allocator), one extra memcpy.
- snapshot.rs: the core decoder's internal allocator parameter is `Global` (`const GPA`) rather than the caller's C allocator (crate-wide rustty-vt treatment of `std.mem.Allocator`); the terminal handed back to the caller is still built through the caller's C allocator via `from_decoded`, so all C-visible ownership is unchanged.
- snapshot.rs `DecoderWrapper`: the reference stores the `Source` inline in the wrapper allocation; rustty puts the `Source` and a shared `ReaderState` cell in separate Box allocations so the core decoder's `&'static mut` borrow never overlaps wrapper field access (Stacked-Borrows discipline). Two extra small allocations per decoder, no observable difference.
- sys.rs: the reference's module-level `var global` becomes a `Mutex<Global>`; every read takes and immediately drops the lock. Same startup-only mutation contract, no observable difference.
- sys.rs `LogEmitter`: the reference implements a `std.Io.Writer` vtable `drain`; rustty implements `core::fmt::Write` with an explicit fill/flush loop. Chunk boundaries and callback counts are identical for the 2048/3000/4096-byte cases the reference tests pin (verified by hand against both algorithms).


**Shard 179** — terminal.zig

- clipboard_write_trampoline / clipboard_read_trampoline / clipboard_read_reply_trampoline: the oracle's `std.heap.stackFallback(128/256, gpa)` and the `>4 contents` heap path map to plain `Vec`s in rustty (the stack-first 4-element array is preserved for the write path). The only behavioural consequence is that the oracle's OOM arms (`write.reply(.io_error)`, `log.warn` + unanswered request) are unreachable under Rust's abort-on-OOM policy (DG5) — documented in-file.
- `std.Io.Writer.Allocating` / `Discarding` / `.fixed` map to `Vec<u8>` / `CountingWriter` / `FixedWriter` with identical observable results (`out_of_space` + required length, exact prefix, `end` accounting).
- `wrapper_from_handler` collapses the oracle's two nested `@fieldParentPtr` steps into one offset subtraction plus an exposed-provenance pointer reconstitution — same recovered address, needed for Stacked-Borrows soundness rather than speed.
- `AutoArrayHashMapUnmanaged(*T, void)` -> `HashSet<*mut T>` for `tracked_grid_refs`/`searches`: only iteration order of the detach loops differs, and the loops are order-independent (each entry gets `terminal = null`).
- `apc_handler.max_bytes` EnumMap -> always-full `[usize; Protocol::COUNT]` array with `default_max_bytes` re-seeding on `remove`/`else` — identical lookup results.
- ScrollViewport/UnknownSequence `lib.TaggedUnion` factory -> hand-authored `#[repr(C)]` struct+union with build-time size/alignment asserts; same ABI.


**Shard 180** — terminal.zig

- `compression activity and incremental scheduling`: the oracle allocates `line.len * repeat` bytes and `@memcpy`s each slot; rustty uses `Vec::with_capacity` + `extend_from_slice` in a loop. Identical resulting bytes.
- `get mouse_tracking`: rustty factors the repeated `set(t, .mode, &config)` / `get(t, .mouse_tracking, ...)` calls into two local closures (`set_mode`, `get_tracking`). Same call sequence and same assertions (modes 9, 1000, 1002, 1003 enable/disable, then all-off).
- Callback capture scaffolding differs idiomatically: the oracle uses a per-test `const S = struct { var last_data: ?[]u8 ... }` with `testing.allocator.dupe`; rustty uses per-test `static mut` plus `extern "C" fn` capturing into an owned `Vec<u8>`. Observable assertions identical.
- `continuation internal allocation and restoration`: the oracle's `continuationAllocIo` returns an allocator-owned slice freed by the test; rustty's returns a `Vec<u8>`. Same exported bytes and same `error.InvalidContinuation` / `RestoreContinuationError::InvalidContinuation` failure for input "A".


**Shard 181** — terminal.zig

- rustty test harness replaces the oracle's fixed `[64]u8`/`[512]u8` capture buffers with `Vec<u8>`/`Vec<Vec<u8>>` statics (crates/rustty-vt-c/src/terminal.rs:8648-8660, 8387-8400); assertion content and count are identical.
- `crate::color::PaletteC` is a type alias for `[rustty_vt::color::RGB; 256]` and `palette_cval` is a plain array copy (crates/rustty-vt-c/src/color.rs:26-51) because rustty's `RGB` is already `#[repr(C)] {r,g,b:u8}`; the oracle's per-element `.cval()` loop is an identity map, so `color.default[0].cval()` and `rustty_vt::color::DEFAULT[0]` compare the same bytes.
- `set_glyph_protocol_...` uses an array index (`enabled[Protocol::Glyph as usize]`) where the oracle uses an EnumSet `.contains(.glyph)` — identical observable predicate.


**Shard 182** — types.zig, unicode.zig

- types.rs replaces Zig comptime type reflection with explicit const rows plus a const-fn JSON writer; the output document is byte-identical modulo the R2 Ghostty->Rustty renames. Not a functional difference in itself (the drift risk it introduces is captured as the findings above).
- The oracle's `GhosttyKey` values are reflected from `input_key.Key` (176 fields); rustty iterates the existing `rustty_vt::input::key::TAG_NAMES` table instead of duplicating the 176 rows — same names, same values.
- unicode.rs merges the oracle's four `codepoint_width *` test blocks into one Rust test containing all five assertions; no assertion is lost.
- types.rs adds a Rust-only test `json_is_comptime_static_data` (no oracle counterpart) guarding that the manifest is static comptime data rather than a lazily allocated cache; extra coverage, not a divergence.


**Shard 183** — Page.zig, lz4.zig, lz4_differential.zig

- compress/Page.zig:76 vs page.rs:139-140 — `representation_overhead` is `@sizeOf(Page) - @sizeOf(TerminalPage)` in both, but the value differs by 8 bytes on 64-bit (Zig: []u8 16 + Allocator 16 = 32; Rust: Vec<u8> = 24) because Rust's Vec carries its Global allocator implicitly, so the oracle's `alloc: Allocator` field has no counterpart (documented PORT NOTE at page.rs:38-41). The formula, the `<=` early return, and the `-1` strictness reserve are identical; only the accounting of the port's own representation size differs, which is inherent to the representation and self-consistent (the rustty test at page.rs:464-469 asserts the same formula the oracle asserts at Page.zig:310-313). Not a behavioural divergence in the ported code paths.
- compress/Page.zig:115-116 and :221-225 — the oracle's `std.valgrind.memcheck.makeMemDefined` calls (guarded by `runningOnValgrind()`) have no rustty counterpart. These are Memcheck annotations only: they suppress false undefined-padding reports under Valgrind and change no program-visible behaviour or assertion outcome.
- lz4.zig:449-490 vs lz4.rs:695-734 — `std.mem.readInt`/`writeInt`/`copyIntAt` on fixed-size subslices are expressed as safe `from_le_bytes`/`to_le_bytes` + `copy_from_slice` (and `copy_within` for the >=64-byte non-overlapping match memcpy). Same bytes moved, same order; the rustty module contains no `unsafe` where the oracle relies on Zig's unaligned loads.
- lz4.zig:853-862 / lz4_differential.zig — the oracle drives its round-trip and generator suites with `std.Random.DefaultPrng`; lz4.rs:972-985 uses a xorshift64* and page.rs:509-515 an LCG, while lz4_differential.rs:65-134 reimplements xoshiro256++ with SplitMix64 seeding. The exact byte streams differ from the oracle's in the first two cases, but every assertion in those tests is structural (round-trip identity / incompressibility), so no assertion is weakened.


**Shard 184** — clipboard.zig, clipboard_command.zig, clipboard_grants.zig, clipboard_response.zig, clipboard_write.zig, color.zig, dnd.zig

- clipboard_command.rs: `Payload` stores an owned `Vec<u8>` plus a decoded length and exposes `data()` instead of the reference's two aliasing slices (`buf`/`data`); identical observable bytes.
- clipboard_command.rs / clipboard_grants.rs / clipboard_write.rs: the reference's packet-scoped `ArenaAllocator` and `dupe` calls become owned `Vec<u8>` fields freed on drop (Zig `deinit` methods have no counterpart). Same lifetimes and contents, different allocation strategy.
- clipboard_write.rs `WriteState::data`: the reference decodes into the spool's *uninitialized* unused capacity then bumps `items.len`; the port zero-extends the Vec (`resize(start+reserve, 0)`), feeds into the initialized tail, then truncates. One extra memset per chunk, byte-identical result (and identical spool length on the Invalid/TooLarge error paths, which both leave the spool at `start`).
- clipboard_write.rs `Alias.target`: the reference shares one arena-duped target slice across every alias registered in a call; the port clones the target bytes per alias. Same mapping, one small allocation per alias.
- clipboard_response.rs: `std.base64.standard.Encoder.encodeWriter` is reproduced inline as `b64_encode_writer` (scalar 3-byte grouping); same padded standard-alphabet output.
- clipboard_command.rs: `std.mem.TokenIterator(u8, .any)` over `std.ascii.whitespace` is reproduced as an explicit `MimeIterator` with the same delimiter set (including 0x0B, which Rust's `is_ascii_whitespace` omits) and the same `reset` rewind.


**Shard 185** — dnd_command.zig, dnd_drop.zig, dnd_response.zig, dnd_test.zig, graphics.zig, graphics_animation.zig

- dnd_command.rs Operations::protocol_value returns u8 assembled from bit positions instead of Zig's `@bitCast` of a packed struct(u2); identical wire value (0..3).
- dnd_drop.rs Offered stores byte ranges into its own `payload` Vec instead of Zig's self-referential `[]const []const u8` slices into the same allocation; same single allocation, identical eql/payload behaviour.
- dnd_drop.rs / dnd_response.rs use `format!` Strings where the reference uses fixed stack buffers with `catch unreachable` (header_buf [32]/[64]/[96], id_buf [16], payload_buf [256]); byte-identical output, no truncation on either side.
- dnd_response.rs ships a local RFC4648 base64 encoder (base64_standard_encode) because the workspace only has a SIMD decoder; identical output to std.base64.standard.Encoder.
- dnd_test.rs base64 round-trip decodes by splitting on ESC and stripping the "]72;"/"\\" remnants instead of splitting on the two-byte ST, and uses rustty_simd::base64::decode; same payload extraction and same final expectEqualSlices-equivalent assertion.
- graphics_animation.rs derives Clone/PartialEq/Eq and holds Animation as Option<Box<Animation>> in Image (deep clone) instead of Zig's shared `?*Animation`; observably identical for every read per the documented PORT NOTE.


**Shard 186** — graphics_command.zig

- decode_data: the reference decodes base64 in place, aliasing `self.data.items` as both source and destination; the Rust port clones the source first (`let src = self.data.clone()`) because of the borrow checker, then decodes into `self.data[0..max_len]` and truncates. Same decoded bytes, same length, same InvalidData error path — extra allocation only.
- Parser.deinit / Command.deinit are dropped in favour of Rust's Drop; Parser drops the `alloc: Allocator` field since Vec carries its own allocator.
- Transmission.Usage is `packed struct(u32) { transient: bool, _padding: u31 }` in the reference and is `@bitCast` from the raw N value; the Rust port models only `transient` (bit 0) and drops the padding bits. The padding is never read anywhere (graphics_image.zig:131 reads only `t.usage.transient`), so this is not observable; the port pins bit 0 with a dedicated KAT.
- KV.values is `undefined` in the reference and zero-initialized in Rust; `get` refuses to read a slot whose presence bit is clear, so the initial contents are unobservable.
- Parser.kv_temp_len is `u4` in the reference and `u8` in Rust; the value never exceeds 12 before being reset, so the width is unobservable.
- Format/Medium/Compression are `lib.Enum(lib.target, ...)` in the reference; the port uses plain Rust enums plus a rustty-only `descriptor()` helper (hardcoded `Target::Zig`). Other `lib.Enum(lib.target, ...)` sites in the port (point.rs, focus.rs, device_attributes.rs) model no descriptor at all, so this helper is an addition, not a regression; the reference reflects over these types only for C-ABI layout, which the vt-c layer handles separately.


**Shard 187** — graphics_exec.zig

- graphics_storage::image_by_id/image_by_number return a cloned `Image` (including its pixel Vec) where the reference returns a by-value struct copy that aliases the same data slice; `display` only reads id/width/height/format from it, so behaviour is identical but the port copies more bytes.
- complete_animation_frame / control_animation / compose_animation re-look the image up by id (`storage.images.get_mut`) around each mutation instead of holding the reference's `img: *Image` interior pointer (documented hazard-#3 PORT NOTE). The id is stable and reserve_animation_bytes never evicts this image, so results are identical; it is extra hash lookups only.
- compose_animation splits the same-frame case into `compose_canvas_rect_in_place` (per-row split borrows) instead of the reference's aliasing `composeCanvasRect(dst == src)`; the per-row math and composition kernel are byte-identical (graphics_pixel.rs:208-234).
- `delete` and the retransmit-delete branch of `load_and_add_image` `mem::take` the ImageStorage out of the active screen and move it back around the call to satisfy the borrow checker; the reference aliases the pointer directly. Same observable state on return.
- load_and_add_image returns `img.withoutData()` (metadata-only copy) instead of the reference's data-aliasing struct copy; the caller only reads `id` and `metadata.implicit_id`.


**Shard 188** — graphics_exec.zig

- rustty accesses the active screen via raw pointer (`unsafe { &(*t.screens.active).kitty_images }`) instead of Zig's `&t.screens.active.kitty_images`; identical observable behaviour.
- Where Zig asserts image bytes via `img.data.bytes().?`, rustty sometimes destructures `Data::Complete(bytes)` directly (e.g. graphics_exec.rs:3040, 3090); this is equal-or-stronger, same values compared.
- Zig's `resp.encode(&writer)` into a fixed 128-byte buffer is a `Vec<u8>` in rustty (graphics_exec.rs:3329-3332); the compared output string is identical ("\x1b_Gi=1,r=2;OK\x1b\\").


**Shard 189** — graphics_image.zig, graphics_pixel.zig, graphics_render.zig

- graphics_pixel.rs: with the `wuffs` feature off, the swizzles and the src-over composite are hand-written scalar mirrors of the vendored wuffs kernels instead of FFI calls; a differential KAT (`wuffs_and_scalar_src_over_agree`, graphics_pixel.rs:527) pins them byte-identical to the real kernel, and the `rgba conversion` vectors run in both cfgs. Identical observable output.
- graphics_pixel.rs:65-68: the wuffs arm copies the returned RawBuf into an owning `Vec<u8>` (an extra copy vs the reference's direct `[]u8` handoff). Self-documented as PERF(port).
- graphics_pixel.rs:179-234: the reference's single aliasing-tolerant `composeCanvasRect` is split into `compose_canvas_rect` (two canvases) and `compose_canvas_rect_in_place` (one canvas, split_at_mut) because Rust cannot express `&mut`/`&` aliasing of the same buffer. Same row order, same offsets, same math.
- graphics_image.rs:1226-1240: `Image::render_data` returns a borrowed `RenderData<'a>` mirror instead of the by-value `Data` union, avoiding a deep pixel copy; both arms and `bytes()`/`len()` behave identically.
- graphics_image.rs:1348-1356: `Image::withoutData` deep-clones the animation box where the oracle copies a shared pointer. Content is identical for the logging use; only allocation cost differs.
- graphics_image.rs:928-983 (`decodePng`): the port moves the decoder seam's already-global-allocated `Vec` in rather than re-allocating and `appendSlice`-ing (one copy saved). Same size check against MAX_SIZE and the same dimension/format updates.
- graphics_image.rs:900-922: zlib inflate uses the vendored C zlib (`zlib_sys::inflate_zlib`) rather than Zig's `std.compress.flate`; both cap at max_size and both surface a cap breach or malformed stream as a failure mapped to DecompressionFailed.
- graphics_image.rs:1085-1108: `Metadata` is a plain struct rather than the oracle's `packed struct(u32)`; it is only read/written field-wise and `placement_count` is bounded by `PLACEMENT_COUNT_MAX` (maxInt(u30)) at the single increment site.


**Shard 190** — graphics_storage.zig

- Hash maps: oracle uses `std.AutoHashMapUnmanaged` with iterate-and-`removeByPtr`; the port uses `std::collections::HashMap` with a collect-keys-first-then-remove shape (graphics_storage.rs:161-164 and every removal loop). Both are unordered and each collected key is removed at most once, so membership/count results are identical.
- `GenerationCounter`'s comptime mutex fallback for 32-bit targets without 64-bit atomics (oracle 41-66) is not ported; every rustty target reports `target_has_atomic="64"`, so `AtomicU64` with `Ordering::Relaxed` (== Zig `.monotonic`) is used unconditionally (graphics_storage.rs:56-82). Documented PORT NOTE; identical observable stamps.
- `Candidate.priority: u2` is carried as `u8` in the port (graphics_storage.rs:2318-2335); the value is only compared, never bit-packed, and `init` can only produce 0..=3.
- `PlacementId` is a Zig `packed struct { tag: enum(u1), id: u32 }` and a plain `#[derive(Hash, Eq)]` struct in the port; it is only hashed/compared field-wise, never bit-cast.
- `imagePtrByIdOrNumber` returns `?*Image` upstream and `Option<&Image>` in the port; mutating callers resolve the id and re-look up via `images.get_mut` (documented hazard-#3 shape). Same image, same mutations.
- `markImageContentChanged` in `animationTick` is deferred to just after the per-image borrow ends in the port (graphics_storage.rs:1362-1367, 1500-1503); it only stamps dirty/generation, so the reordering within one iteration is unobservable.
- `ScrollMargins.end()` is a consuming no-op with the restore body in `Drop` (graphics_storage.rs:2261-2296), mirroring Zig's `defer state.end()` on early-return paths.
- Screen/Terminal parameters are raw `*mut` in the port rather than `&mut` to avoid aliasing the `&mut self` receiver (storage lives inside `Screen`); all dereferences are narrow, and the resulting behaviour is identical.


**Shard 191** — graphics_storage.zig

- deleteIntersecting and evictImageExcept iterate the placements map in place in Zig; the Rust port first collects `self.placements.keys().copied()` into a Vec to satisfy the borrow checker (extra allocation, identical set of removals).
- Zig `imageById` hands back a pointer into the images map; the Rust `image_by_id` returns an `Option<Image>` copy in `delete_intersecting` (Copy of the small Image header; same observable values).
- Candidate.priority is `u2` in Zig, `u8` in Rust (only ever compared, never bit-packed; values are 0..=3 either way).
- PlacementId is a `packed struct` in Zig and a plain `#[derive(Hash, Eq)]` struct in Rust; the value is only hashed/compared field-wise, never bit-cast.


**Shard 192** — graphics_storage.zig

- Test-structure-only differences (not functional): rustty extracts two helpers `external_placement_coord`/`external_placement` (graphics_storage.rs:6893/6930) in place of the Zig tests' inline `storage.placements.get(...)` + `pointFromPin` expressions; the assertions they feed are identical.
- rustty adds one EXTRA test with no oracle counterpart, `storage_eviction_order_is_transient_then_unused` (graphics_storage.rs:4841), documented in-file as a deliberate negative control pinning the full 4-block eviction ordering. Additional coverage, not a parity gap.
- Zig `pending.complete(&s, io, data)` takes an allocator-owned slice that the caller frees on `false`; the Rust `complete(&mut s, Vec<u8>)` consumes the Vec and drops it internally on `false`. Ownership idiom only - the returned bool and the stored bytes are identical in every arm the tests assert.


**Shard 193** — graphics_unicode.zig, key.zig

- graphics_unicode getIndex: oracle uses std.sort.binarySearch with an explicit comparator; rustty uses slice::binary_search on the same DIACRITICS table (295 entries, byte-identical values and order). Identical results.
- graphics_unicode PlacementIterator::next: oracle iterates `for (cells, row.x..) |*cell, x|`; rustty indexes a raw base pointer over the same `.right` cell subset with a captured x_start. Same traversal, same mutation of row.x per iteration.
- key.rs models Zig's `u3` idx with a masked newtype (wrapping_add/wrapping_sub with MASK 0b111) and `Flags` packed-struct bit layout with explicit shifts in int()/from_int(). Bit-for-bit identical to @bitCast on packed struct(u5).
- graphics_unicode Placement fields/Pin are copied field-by-field (Pin is deliberately non-Copy in the port) where the oracle does `row.*` / `.pin = ...` value copies. Same values.


**Shard 194** — encoding.zig, kitty_metadata.zig, parsers.zig, change_window_icon.zig, change_window_title.zig, clipboard_operation.zig, color.zig, context_signal.zig, hyperlink.zig, iterm2.zig

- color.zig's `SegmentedList(Request, 2)` (prealloc 2, copy-free growth) is modeled as `Vec<Request>` in color.rs:163; element sequence and observable results are identical, only allocation strategy differs.
- kitty_metadata.zig's comptime-specialized `ValueIterator(key, valid_value_characters)` type-constructor is modeled as a struct carrying both as `'static` runtime fields (kitty_metadata.rs:62-90); comptime vs runtime dispatch only.
- iterm2.zig's `std.StaticStringMapWithEql` (length-bucketed comptime map) is modeled as a linear scan over a `&[(&str, Key)]` table with `eq_ignore_ascii_case` (iterm2.rs:59-107); lookup results are identical.
- `@branchHint(.cold)` markers in encoding.zig are dropped in encoding.rs (no observable behavior).
- `std.mem.tokenizeScalar` is reimplemented as the local `TokenizeScalar` struct (color.rs:191-220) with identical skip-empty-runs semantics.
- The unused `simd` import in iterm2.zig (line 4) has no counterpart; it is not referenced anywhere in the audited range.


**Shard 195** — kitty_clipboard_protocol.zig, kitty_color.zig, kitty_desktop_notification.zig, kitty_dnd_protocol.zig, kitty_text_sizing.zig, mouse_shape.zig

- kitty_color.rs copies `cap.trailing()` into an owned Vec before reborrowing `parser.command` (rs:40-45); the oracle keeps a borrowed slice. Same observable result, extra copy for Rust aliasing rules.
- kitty_clipboard_protocol.rs / kitty_dnd_protocol.rs / kitty_desktop_notification.rs store `metadata`/`payload` as owned `Vec<u8>` instead of the oracle's borrowed `[]const u8` slices into the capture buffer; byte-identical content.
- kitty_color.rs uses `kv.splitn(2, '=')` where the oracle uses `splitScalar` + `it.rest()`; identical remainder semantics.
- Zig's comptime-dependent `key.Type()` returns are modeled as runtime-tagged `OptionValue` enums with `as_*` accessors in both kitty OSC parsers; same values, runtime dispatch instead of comptime.
- VALID_METADATA_VALUE_CHARACTERS is built by a const-eval concatenation loop rather than Zig's `++`; identical byte content.


**Shard 196** — osc9.zig, report_pwd.zig, rxvt_extension.zig

- osc9.rs / report_pwd.rs / rxvt_extension.rs materialize payloads as owned Vec<u8> (`.to_vec()`) where the oracle returns zero-copy `[:0]const u8` sentinel slices into the capture buffer. Byte content is identical (trailing NUL excluded exactly as in Zig's `data[a .. len-1 :0]`); only an allocation/copy is added.
- osc9.rs re-acquires `parser.capture.as_ref().unwrap().trailing(&parser.buffer)` after each `write_byte` instead of holding a long-lived `cap` binding, to satisfy Rust borrowck. Same sequence of side effects and same re-read of the (possibly reallocated) trailing slice as the Zig `data = cap.trailing();` re-fetch.
- osc9.rs reads back the just-set state via `match &parser.command { Command::ConemuProgressReport(r) => r.state, _ => unreachable!() }` where Zig switches directly on `parser.command.conemu_progress_report.state`; identical value and branch selection.
- Zig tests for OSC 9;6 through 9;12 construct the parser with `testing.allocator` while the Rust ports use `Parser::init(None)`. Verified non-observable: osc.zig:855-857 requests `captureTrailing(.fixed)` for state `.@"9"`, so the allocator is never consulted on the OSC 9 path and both trees exercise the fixed-buffer capture.


**Shard 197** — semantic_prompt.zig

- Zig `options_unvalidated: []const u8` borrows the parser capture buffer; the Rust `Command` owns a `Vec<u8>` copy set via `set_options` (semantic_prompt.rs:575-582). Allocation/copy difference only; observable option-reading behaviour is identical.
- Zig's comptime-generic `Option` enum with per-tag `Type()`/`key()`/`read()` is flattened in Rust into typed descriptor constants (`option::AID`, `option::CL`, ...) plus a `ReadableOption` trait and a shared `scan_value` helper. Same key strings, same decoders, same scanning loop semantics (including the bare-`break` 'Not found' fallthrough).
- Zig `lib.Enum(lib.target, ...)` gives `Click`/`ClickEvents` a c_int backing tag under the C target; the Rust port uses plain `#[derive]` enums with the same variant order (line/multiple/conservative_vertical/smart_vertical; absolute/relative). No C header currently exposes these types (include/ghostty/vt/osc.h only defines GHOSTTY_OSC_COMMAND_SEMANTIC_PROMPT = 3), so the tag width is not observable.
- Zig `Redraw = enum(u2)` vs Rust plain enum with the same variant order (True, False, Last); discriminant width is not observable through any API in this file (noted in the port's own PORT NOTE at semantic_prompt.rs:415-419).


**Shard 198** — Thread.zig, active.zig, pagelist.zig

- crates/rustty-vt/src/search/pagelist.rs:871-917 (`feed_keeps_its_tracked_pin_within_a_shorter_page`) uses 80 cols instead of the oracle's 10 and wraps the fill loops in `pause_integrity_checks(true/false)`; the asserted property (split successor shorter than the second page; feed's pin lands on `shorter` and stays valid) is identical, and the integrity checks are re-enabled + `assert_integrity()` is run before the assertions. Documented PORT NOTE, runtime-only.
- crates/rustty-vt/src/search/thread.rs:1240-1565: the in-crate `xev` seam (Loop/Async/Timer backed by std Mutex/Condvar/atomics) replaces libxev; apart from the timer re-arm defect reported as a finding, its observable semantics (level-triggered async waits, one-shot timer, run(.once)/run(.no_wait), stop/stopped) match. Callback dispatch is deferred to `run_loop` instead of inline to avoid an aliasing `*mut Thread`; ordering of side effects is preserved.
- crates/rustty-vt/src/search/thread.rs:664-700: `notify` re-fetches `s.active_screen_search()` after the viewport-matches block instead of holding one `*ScreenSearch` across it (borrowck); `viewport_matches` does not touch `screens`, so it is the same entry.
- Oracle Thread.zig log statements (std.log.scoped(.search_thread)) are mirrored with `eprintln!` carrying the same message text; diagnostics only.


**Shard 199** — screen.zig

- crates/rustty-vt/src/search/screen.rs:427-472 `matches()` and :1387-1415 `match_at()`/`selected_match()` return deep `clone()`s of `Flattened` where the Zig returns shallow aliasing copies (Rust `Flattened` owns a `Vec<Chunk>` and is not `Copy`). Observably identical — callers only read `untracked()`/`start_pin()` — extra allocation only.
- crates/rustty-vt/src/highlight.rs:134-146 `Flattened.chunks` is a `Vec<Chunk>` (AoS) where the oracle uses `std.MultiArrayList(Chunk)` (SoA). Layout-only; all `.items(.node)`/`.items(.serial)` projections map to field access with identical values.
- crates/rustty-vt/src/search/screen.rs:659-668 carries a PERF NOTE that `prune_history` is O(results * chunks * live_pages) per `select` via `node_is_valid`; this is oracle-identical (`PageList.zig` `nodeIsValid` walks the same way), documented rather than changed.
- Five tests (select_after_partial_history_erase_drops_a_pruned_selection, select_after_history_compaction_ignores_replaced_results, reload_defers_pruning_unselected_history_results, reload_after_partial_history_page_erase_drops_shifted_selection_first, select_after_partial_history_page_erase_ignores_shifted_results, select_after_history_page_split_ignores_moved_results) use 80 cols instead of the reference's 10, documented in-file: 10 cols yields ~35k-row pages whose row-by-row fill costs ~30s in a debug Rust build. Assertions are unchanged and column-count independent (they exercise history erase/split/compaction validity, not layout).
- crates/rustty-vt/src/search/screen.rs uses `eprintln!("info(search_screen): ...")` in place of the oracle's `log.info` on the failed-select-retry paths. This is the crate-wide convention (30 occurrences across rustty-vt); message text is mirrored.


**Shard 200** — sliding_window.zig, terminal.zig

- sliding_window.zig `chunk_buf: std.MultiArrayList(FlattenedHighlight.Chunk)` (SoA) -> `Vec<crate::highlight::Chunk>` (AoS). The reverse fixup's `std.mem.reverse` over four parallel arrays becomes one `self.chunk_buf.reverse()`; identical resulting chunk order and field values.
- `highlight()` ends with `result.chunks = self.chunk_buf` in Zig (the returned Flattened ALIASES the window's chunk storage, valid only until the next next()/append()). rustty clones the chunks into the result instead. Observable results are identical for the documented usage contract; rustty's copy is merely longer-lived (strictly safer, never fewer/different chunks).
- Zig `chunk_buf.ensureTotalCapacity(alloc, self.meta.capacity())` + `appendAssumeCapacity` -> rustty `Vec::reserve(...)` + `push`. rustty's reserve amount can be smaller than the oracle's target capacity, but `push` grows on demand so no behavioural difference; only allocation counts differ.
- Zig `std.ascii.indexOfIgnoreCase` (which may use vectorized `indexOfScalar`-style scanning) -> a scalar `ascii_index_of_ignore_case` loop in rustty with the same results, including `Some(0)` for an empty needle (pinned by a rustty-only regression test).
- `assertIntegrity` skips its checks under Valgrind in Zig (`std.valgrind.runningOnValgrind() > 0`); rustty always runs them under debug_assertions. Debug-only cost, no behavioural difference.
- `Meta::deinit` frees the ArrayList in Zig; rustty replaces the Vec with `Vec::new()` (same free), and `MetaBuf::delete_oldest`/`clear` additionally `take` the slots to drop non-Copy Meta rather than `@memset(undefined)`. Same observable state.


**Shard 201** — viewport.zig, checkpoint.zig, continuation.zig, envelope.zig, fixture.zig

- crates/rustty-vt/src/search/viewport.rs:290-320 — the reference binds `const entries = fingerprint.entries.items` once and iterates the slice; the port re-indexes `self.fingerprint.entries[i]` each iteration (Stacked-Borrows soundness, documented PORT NOTE). Identical iteration order, identical appends.
- crates/rustty-vt/src/search/viewport.rs:409-424 — `ensureTotalCapacity(alloc, pages.rows)` becomes `Vec::try_reserve(rows.saturating_sub(len))`, which yields the same capacity>=rows guarantee and the same 'reserve before any mutation so an OOM preserves the old fingerprint' ordering.
- crates/rustty-vt/src/snapshot/continuation.rs:199-209 — Zig's `testing.FailingAllocator` is replaced by the crate's `tw` tripwire seam plus `Vec::try_reserve_exact`; the 'limit check precedes allocation' property is asserted the same way (armed tripwire must end untripped).
- crates/rustty-vt/src/snapshot/fixture.rs:91-123 — the reference's two comptime passes (`decodedLen` then `parse`) collapse into a single run-time pass returning `Vec<u8>`; the accepted grammar, separator rule, and error messages are byte-for-byte the same.
- crates/rustty-vt/src/snapshot/record.rs:289-296 — `errdefer stream.cancel()` in checkpoint/continuation `encode` is subsumed by `Writer::finish`, which cancels on every exit path (plus an explicit cancel on the continuation payload-write failure path).


**Shard 202** — grid.zig

- Bulk row emit: the oracle (grid.zig:507-548) writes the row header + cells directly into the Zig writer's `unusedCapacitySlice()` and bails to the generic path when spare capacity is short. The port (grid.rs:632-664) assembles the row in a hoisted 4 KiB stack chunk (`EMIT_CHUNK`) and emits it with one `write_all`, falling back to the generic path when `needed > EMIT_CHUNK`. Both paths produce byte-identical output; only the fallback trigger differs (writer capacity vs. fixed 4 KiB).
- Vectorization: the oracle uses Zig `@Vector` shuffles/reduces in `scanRow`, `encodeNarrowInto`/`encodeNarrowStep` and `widenCells`/`widenStep` (grid.zig:591-767, 976-1064). The port keeps the identical window structure (4-cell encode steps, 16-transported-byte widen steps, overlapping final windows) as scalar `u64` loops that LLVM auto-vectorizes (grid.rs:724-908, 1228-1300), documented in the module's "PORT NOTE (vector paths)". Byte-for-byte identical, including the overlapping-store tails.
- Comptime -> const generics: the oracle's `comptime width: Cell.EncodedWidth` parameters become `const SIZE: usize` monomorphized `_sized` helpers (`truncate_sized`, `extend_sized`, `widen_value_sized`, `encode_narrow_cells_sized`, `encode_narrow_into_sized`, `widen_cells_sized`) with runtime-`width` wrappers preserving the reference call shape (grid.rs:803-908, 1206-1300).
- `widen_value_sized` uses a value-select instead of the oracle's early `return 0xFFFD` (grid.zig:1075-1088 vs grid.rs:1302-1326) to keep the widening loop vectorizable; same result for every input.
- `encode_inner` hoists the 4 KiB emit chunk out of the row loop (grid.rs:554-560) to avoid a per-row zero-fill; the oracle has no such buffer.
- `Remap` is one concrete `u16` type with two aliases instead of the oracle's `Remap(comptime Id)` generic (both `TerminalStyleId` and `TerminalHyperlinkId` are `u16`), and its `seen` bit set is a `Vec<u64>` stand-in for `std.DynamicBitSetUnmanaged`; `deinit` is replaced by `Drop`, allocation by the global allocator (grid.rs:1795-1876). Same capacity (65536), same put/contains/get semantics. `seen_iter()` is an added helper equivalent to `seen.iterator(.{})` used by sibling modules.
- `NATIVE_MATCHES_WIRE` is a pinned `const true` rather than a comptime-computed probe result, because the native cell accessors are not `const fn`; the oracle's three bit-pattern probes are carried verbatim in the `cell_wire_layout_registry` test (grid.rs:502-513, 1892-1975), so a native layout divergence still fails loudly.


**Shard 203** — history.zig, hyperlink.zig, io.zig, main.zig

- snapshot/io.rs replaces Zig's comptime-generic `writeInt`/`readInt` with a `LeInt` trait implemented by a macro for u16/u32/u64 (the only widths the subsystem uses); byte output and error behaviour are identical.
- snapshot/history.rs `decode_page` splits into `decode_page` + `decode_page_inner` and threads a `*mut TerminalScreen` instead of `&mut` purely to keep Stacked-Borrows validity while `PageAllocation` aliases the PageList; observable ordering (decode -> read rows/prompt -> finalize -> set semantic_prompt.seen) is unchanged.
- snapshot/hyperlink.rs replaces Zig `errdefer page.string_alloc.free(...)` with explicit `free_page_string` calls on every error/degrade path; the same allocations are released on the same paths.
- snapshot/hyperlink.rs `decode` drops the `alloc: Allocator` parameter and allocates through the global allocator + `try_reserve_exact`, with a `tw::SnapshotFp::Alloc` tripwire standing in for `std.testing.FailingAllocator`; the `OutOfMemory` error path and its per-allocation index semantics are preserved (error_after(min) == fail_index).


**Shard 204** — page.zig, record.zig

- record.rs Reader: the reference nests std.Io.Reader.Limited (1-byte buffer) inside std.Io.Reader.Hashed (256-byte buffer) requiring a stable self address; the port flattens both adapters into Reader (one 256-byte hashing_buffer + `remaining` limit + hasher updated as bytes enter the buffer). Payload limiting, EndOfStream on truncation, and finish()'s exhaustion/CRC checks are byte-identical; documented in an in-file PORT NOTE.
- record.rs Reader borrowed path: the reference stores a `std.Io.Reader.fixed` view aliasing the source buffer plus the source pointer; the port stores only a (len, seek) window and re-derives the byte view from source.buffered(). Same checksum-on-finish and same deferred `source.toss(payload_len)`.
- record.rs Writer: the reference takes an Allocator for the scratch Allocating writer; the port uses a StackFallbackWriter with a 512-byte stack region (RECORD_SCRATCH_STACK_BYTES), matching the reference's std.heap.stackFallback(512, alloc) at the snapshot.encode call site but applied to every Writer. Only where the first 512 payload bytes live differs.
- page.rs Header::LEN and record.rs Header::LEN are pinned constants (20 and 10) instead of the reference's comptime computeLen() + comptime assert; each file carries a `header_len_matches_encoder` test asserting LEN == compute_len() and the literal, preserving both the synchronization and the assert at test time.
- page.rs page_hyperlink copies the entry's byte strings into owned Vec<u8> (the crate's Hyperlink owns its strings) where the reference returns slices aliasing page memory; the encoded bytes are identical.
- page.rs Decoder::init / record.rs Reader::init are plain constructors instead of the reference's in-place `self: *T` initialization over undefined memory.


**Shard 205** — screen.zig

- Rust replaces the Zig comptime `Header.computeLen()`/`SavedCursor.computeLen()` (and their `comptime { assert(len == 53/23) }`) with pinned `LEN` consts plus two runtime tests that re-derive the length from the encoder and assert the same 53/23 values — same guarantee, moved from comptime to test time.
- Zig `packed struct(u8/u16)` + `@bitCast` are replaced by explicit-field Rust structs with hand-written `bits()`/`from_bits()` shift-and-mask helpers; the wire bytes are identical and the layout is pinned by the `cursor_kitty_and_saved_cursor_flag_bit_layouts` test.
- The generic Zig `enumFromInt(comptime T, raw)` becomes seven per-registry `*_from_int` helpers; each maps exactly the declaration indices the `native_enum_values_used_by_the_screen_format` test pins.
- `decode` drops the reference's `io_: std.Io` parameter because the ported `Screen` carries no `io` field (crate-wide decision, documented in a PORT NOTE above `decode`); no observable behaviour depends on it in this file.
- `screen_sequence_failure_preserves_preceding_bytes` and the OOM arm of `screen_restoration_rejects_invalid_and_incomplete_sequences` substitute the Zig `FailingAllocator{ .fail_index = 0 }` with, respectively, a full FixedWriter destination and the pagelist Builder finish tripwire; both are documented PORT NOTEs and assert the same errors (WriteFailed with the destination still exactly "prefix"; DecodeError::OutOfMemory with no leaked partial screen).


**Shard 206** — snapshot.zig, style.zig

- codec.rs `encode`/`decode`/`ready`/`next` drop the reference's `alloc: Allocator` and `io_: std.Io` parameters where the port's `record::Writer` and `history::decode_page` allocate through the global allocator; documented PORT NOTEs, no observable behaviour change.
- `std.EnumMap(TerminalScreenKey, u64)` / `std.EnumSet(TerminalScreenKey)` become fixed `[Option<u64>; COUNT]` / `[bool; COUNT]` arrays (HistoryRows, History.generations/routed) with a KEYS declaration-order table; identical iteration order and semantics.
- `nextPage` is a free function over split borrows (`source` + `&mut History`) rather than a `&mut self` method, a borrow-checker accommodation with identical control flow.
- Slot replacement in `ready_inner` and the test helpers adds `drop(core::ptr::read(slot))` before `ptr::write` because the port's PageList frees pool memory in Drop rather than in `deinit`; same net effect as the Zig `slot.deinit(); slot.* = replacement;`.
- style.rs represents the Zig `packed struct(u16)` Flags as explicit fields plus hand-written `bits`/`from_bits` shifts instead of a bit-cast; the flag_bit_layout test pins the identical bit positions.


**Shard 207** — terminal.zig

- Oracle `Header.len` is a comptime `computeLen()` running the encoder plus a `comptime` assert of 103; the port pins `Header::LEN = 103` and moves the encoder-derived computation into a `#[cfg(test)] compute_len()` covered by the `header_len_matches_encoder` test (both assertions preserved).
- Oracle's generic `enumFromInt(comptime T, raw)` is replaced by per-enum `*_from_int` helpers (status_display/screen_key/cursor_style/redraw/mouse_event/mouse_format/mouse_shape); all declaration-index mappings were checked against the oracle enum declarations (ansi.zig StatusDisplay, cursor.zig Style, osc/parsers/semantic_prompt.zig Redraw enum(u2), mouse.zig Event/Format/Shape, ScreenSet.Key) and match.
- Zig `errdefer`/`defer` cleanup is expanded into explicit per-error-arm `deinit` calls in `decode_payload` and `decode`; the cleanup set and ordering (result.deinit before payload.deinit) match the reference defer ordering.
- `testing.FailingAllocator{.fail_index = 0}` is replaced by the crate's snapshot tripwire seam (`tw::error_always(SnapshotFp::Alloc)`); the observable expectation (`error.OutOfMemory` from `decode`) is identical.
- `ModePacked` is a `u64` newtype with a bit mask instead of a comptime-generated packed struct; `@truncate` to the u43 backing integer maps to `raw & MODE_BITS_MASK` with the same result.
- `record.Writer.init` drops the allocator parameter (Rust writer owns its scratch buffer); no observable difference in the emitted bytes.


**Shard 208** — control.zig, layout.zig, output.zig

- control.rs returns owned `Vec<u8>` payloads in `Notification` where the reference borrows sub-slices of the parser buffer ("Important: do not clear buffer here since name points to it"); an extra copy, identical observable values.
- layout.rs builds the tree from owning `Vec<Layout>` children instead of arena-allocated `[]const Layout` slices, so `Allocator`/`ArenaAllocator` parameters and the `Allocator.Error` arm of `ParseError` disappear; parse results and `SyntaxError` behaviour are identical.
- output.rs `comptime_format` delegates to the runtime `format` instead of the reference's comptime `Discarding`-sizing + `.fixed` buffer two-pass; identical bytes, computed at runtime.
- output.rs replaces `FormatStruct(comptime vars) -> type` + `parseFormatStruct(comptime T, ...)` with a runtime `&[Variable]` field list returning `Vec<Value>` in field order (no Rust equivalent of Zig struct-type synthesis); the delimiter splitting, MissingEntry/ExtraEntry/FormatError paths and per-variable parse results are identical.
- control.rs `Parser::init` / `init_options` replace the reference's struct-literal field defaults (`state: .idle`, `max_bytes: 1024 * 1024`, caller-supplied `buffer: .init(alloc)` / `.initCapacity(alloc, 128)`); the DCS hook's fallible capacity reservation is preserved via `try_reserve_exact` -> `OutOfMemory`.


**Shard 209** — viewer.zig

- Command queue: reference `CircBuf(Command, undefined)` vs port `VecDeque<Command>` with `with_capacity(COMMAND_QUEUE_INITIAL=8)`. FIFO push_back/front/pop_front/len/is_empty/forward-iterate semantics are identical; `Command` owns a Vec in the `.user` arm which the Copy-bound CircBuf cannot hold.
- Action storage: reference accumulates actions in a reset-per-call ArenaAllocator plus a preallocated `action_single[1]`; the port keeps an owned `Vec<Action>` on the Viewer cleared at the start of each `next()` and returned as `&[Action]`. `.windows` therefore carries an owned clone of the window list rather than a borrow of `self.windows`/a local list (the reference's list-windows path actually hands back a slice of a locally-deinit'd ArrayList).
- Window layout ownership: reference keeps a per-window ArenaAllocator (`layout_arena`) solely to bulk-free the Layout tree plus an explicit `Window.deinit`; the port's `Layout` owns its subtree so dropping `Window` frees it — the arena field and deinit are dropped.
- initLayout split: reference walks the layout tree writing leaves directly into the new PanesMap and bit-copying carried-over Panes out of the old map (aliasing an owned Terminal); the port collects ordered leaf descriptors and then moves carried-over panes out of `self.panes` via `fetch_swap_remove`. Walk order, get-or-init dedup order, new-pane command queueing order and removed-pane pruning are identical.
- `self.windows.reserve(len.saturating_sub(current))` replaces `ensureTotalCapacity(windows.len)`; capacity-only.
- Format.comptimeFormat is a comptime string in Zig and a runtime `String` build in the port; the emitted command bytes are identical.
- `Format.Struct()` (comptime-synthesized parse struct) is replaced by parsing into a `&[Value]` slice decoded by `ListPanes::from_values` in the same variable order.


## Refuted candidates

24 candidate findings were refuted by BOTH verifier lenses and are therefore NOT parity gaps. They are listed so a reader can see what was checked and cleared.

- **S125-F1** (low) — Empty pageIterator carries caller direction instead of default .right_down
  - Refuted: Read both regions. Oracle PageList.zig:6663-6673 returns `.{ .row = null }` (direction defaults .right_down, field decl at src/terminal/PageList.zig:6470s); rustty crates/rustty-vt/src/pagelist/access.rs:677-684 returns PageIterator{row:None,limit:Limit::None,direction}. The direction field is only consumed by PageIterator.next (PageList.zig:6479 -> nextUp/nextDown, both of which `return null` immediately when self.row == null, mirrored by iter.rs:343-344), and by RowIterator.next (6409) / CellIterator.next (6345), which both short-circuit on a null chunk/cell before touching page_it.direction — and an empty page_it yields a null chunk in rowIterator (7176) and null cell in cellIterator (7196). No code in either tree reads .direction of an already-empty iterator otherwise. No input can produce a different observable result; a struct-default artifact of Zig only.
- **S126-F2** (low) — allocation-failure test does not assert the injected failure actually fired
  - Refuted: The finder missed the tripwire semantics. rustty-util/src/tripwire.rs:278-295 `end(ResetMode::Reset)` returns Err(UntrippedError) if any armed expectation was never tripped. tests.rs:4202 calls `INIT_TW.with(|tw| tw.borrow_mut().end(ResetMode::Reset).unwrap())` between the failed compress and the continuation compress — the `.unwrap()` panics unless the Encoded fail point actually fired, which is exactly `try testing.expect(failing.has_induced_failure)` at oracle 8455. The doc comment at tests.rs:4168-4170 states this explicitly. Assertion power is equal.
- **S126-F3** (low) — partial-erase-restarts-compression test uses a different history-growth setup than the oracle
  - Refuted: Assertions are identical on both sides (finder concedes this). Oracle 8631-8648 grows MAX_INSPECTED+1 cold pages; rustty tests.rs:2876-2880 uses fill_compressible_history(MAX+2) which grows until total_pages() >= MAX+2 (tests.rs:2209-2231), i.e. cold pages >= MAX+1 with a 80x24 active area occupying one page — the same fixture shape, plus a compressible 'a' marker per page. The marker walk (`for _ in 1..MAX_INSPECTED { marker = (*marker).next }`) would null-deref, not silently pass, if the list were short. No observable behavioural difference in what the test proves; a fixture-style preference, not a parity gap.
- **S126-F4** (low) — pointFromPin-overflow test models the overflow with a cyclic 3-node ring instead of a 65,539-node linear list
  - Refuted: Read oracle 9704-9739 and rustty tests.rs:16151-16210. In the oracle the accumulated Y overflows u32 long before the walk reaches the final node (65,539 x 65,535 rows), so the null is returned by the same checked-accumulate guard regardless of the target's list membership — the ring-plus-detached-target fixture drives the identical arithmetic and the identical `None` result, as the port's comment (tests.rs:16135-16150) documents including the pre-fix panic/infinite-loop observation. Assertion is equal (`== null` vs `assert_eq!(None, ...)`); the difference is test-fixture robustness under a hypothetical removed guard, not observable behaviour.
- **S126-F5** (low) — preserved-page test: no assertion that the borrowed arm did not allocate, and the OOM arm is debug-only
  - Refuted: Borrowed arm: tests.rs:3160-3168 arms the Buffer fail point with errorAlways and then calls page_preserving_state(node).expect("a resident page must not allocate") — if the fail point had been reached the call would return Err and the expect would panic, so this IS the `!failing.has_induced_failure` assertion (oracle 8928), just spelled structurally; the following reset() only clears the deliberately-untripped expectation. OOM arm: gated `#[cfg(debug_assertions)]` because tripwire::Module::enabled() is cfg!(debug_assertions) (tripwire.rs), and cargo test's default profile has debug_assertions on, so the four assertions do run in the standard test lane; the gate is a documented port-mechanism artifact (tests.rs:3196-3200), not an assertion the port never makes. Inconclusive as an observable parity gap, so refuted.
- **S127-F2** (low) — screenPoint long-scrollback test uses a different (smaller) fixture than the oracle
  - Refuted: Read oracle PageList.zig:10948-10987 and rustty tests.rs:16056-16130. Both exercise the identical code path (Cell::screen_point walking `prev` links summing size.rows into a u32) and both assert the same two things: expected_y > CellCountInt::MAX, and screen_point() == Point::Screen{x:0,y:expected_y}. The only difference is the fixture (307 metadata-only `undefined` nodes vs 3 real pages with two leading size.rows temporarily set to CellCountInt::MAX), which is a Rust-safety-driven equivalent documented in the PORT NOTE at tests.rs:16064-16070; assertions are equal in meaning and coverage of the overflow case. The finder itself labels it informational. Not an observable functional difference.
- **S138-F1** (low) — 'implicit hyperlink ID wraps' split into two tests; the failure half starts from a fresh screen instead of after a prior start/end
  - Refuted: Read oracle ghostty/src/terminal/Screen.zig:11010-11046 and rustty crates/rustty-vt/src/screen.rs:14592-14647. The port splits the single Zig test into screen_implicit_hyperlink_id_wraps (success half) and screen_implicit_hyperlink_id_wrap_rolls_back_on_allocation_failure (failure half), and all four oracle assertions are present with identical meaning: implicit_id==0 and cursor.hyperlink.id.implicit==OffsetInt::MAX in the success half (rs 14600-14608), Err(OutOfMemory) from start_hyperlink and implicit_id==MAX afterwards in the failure half (rs 14634-14644). Nothing asserted in the oracle is dropped or loosened, so this is not a weakened-assertion finding. The only delta is that the oracle's failure half runs on the same screen after startHyperlink+endHyperlink; the finder offers no evidence (and I found none) that the preceding insert/remove into page.hyperlink_set changes the code path under test — the assertion is purely about cursor.hyperlink_implicit_id rollback, and the counter state entering the failing call is identical (explicitly re-set to maxInt on both sides). The failure-injection mechanism differs (Zig FailingAllocator vs the tripwire fail point at the Box::new(source.dupe()) allocation), which is the documented port-idiom seam and produces the same OutOfMemory error at the same allocation. Test-setup detail with no observable behavioural difference; refuted.
- **S141-F2** (low) — Terminal.Options field defaults have no Rust counterpart (no Default/constructor), forcing every caller to re-spell them
  - Refuted: Absence of `impl Default`/constructor for Options (crates/rustty-vt/src/terminal.rs:477-523) is a code-organisation difference only: every field is public and each call site supplies an explicit value. I checked the call sites found by grep (rustty-bench/src/lib.rs:84-85, rustty-renderer/src/cell.rs:457, generic.rs:6787, overlay.rs:871/932, link.rs:328) — all pass the reference ghostty-artifact values, so observable behaviour matches. The only place where a wrong value is actually produced is rustty-vt-c, already captured as S141-F1; F2 is a duplicate of that with no independent observable effect.
- **S147-F1** (low) — printSlice differential fuzz uses a different PRNG (LCG) than Zig's DefaultPrng (xoshiro256++), so the exercised op sequence differs
  - Refuted: Read both regions in full. Oracle Terminal.zig:14327-14481 (helper) and 14486-14501 (test) vs rustty terminal.rs:17468-17681 (helper) and 17734-17747 (test). The op dispatch table (0..9 print run, 10 CR/LF, 11 setCursorPos, 12 SGR attrs incl. direct_color_fg with three random bytes and 8_fg red, 13 insert, 14 wraparound, 15 grapheme_cluster, 16/17 margins, 18/19 hyperlink, 20 charset G0), the alphabet (40 identical codepoints), cps_buf[64], the per-op assertions (cursor.x, cursor.y, pending_wrap, full dump_string_alloc equality with the same diagnostic dump of 2027/insert/wrap/sr.left/sr.right/cols), the final verifyIntegrity on both pages, and all four (ops,cols,rows) geometries — 500/80/24, 500/10/4, 500/5/2, 200/2/2 (oracle 14497-14500 vs rustty 17743-17746) — match exactly. The only difference is the deterministic generator backing the stream (xoshiro256++ vs a documented LCG, terminal.rs:17706-17712), which changes which random sequence is drawn but asserts nothing less: the test's invariant (print vs print_slice must agree after every op, for any deterministic sequence) is identical in strength, and no production code path differs. That is a test-fixture substitution with equal assertions, not an observable functional difference. The TestRand::range hi<=lo guard is likewise unreachable-equivalent: range(1, n-i) has n-i>=1, and range(1, cols/2)/range(cols/2, cols) with the smallest geometry cols=2 gives lo==hi==1 and lo=1,hi=2, matching Zig's intRangeAtMost(1,1) and (1,2) — no input in this test reaches hi<lo.
- **S148-F1** (low) — "fullReset tracked pins" test is ignored in release builds; oracle test is unconditional
  - Refuted: Finder's premise is wrong. Oracle PageList.zig:5683-5686 `pub fn pinIsValid` opens with `comptime assert(build_options.slow_runtime_safety);` — it is NOT ungated; the function fails to compile (and so the test at Terminal.zig:15743-15751 cannot exist) in a build without slow_runtime_safety. The port mirrors this: crates/rustty-vt/src/pagelist/mod.rs:2785-2799 keeps the same invariant as `assert!(cfg!(debug_assertions))`, so calling pin_is_valid in a release build would panic by design. `#[cfg_attr(not(debug_assertions), ignore)]` at terminal.rs:18742 is therefore the faithful Rust rendering of the oracle's build-option gating, not a weakened assertion; in the profile where the oracle test exists (debug/slow_runtime_safety) the port's test runs with identical assertions (track_pin of cursor pin, full_reset, pin_is_valid).
- **S148-F3** (low) — Semantic-prompt / index / OSC133C / multiple-newline tests read cells via Point::Screen where the oracle uses `.active` coordinates
  - Refuted: Verified both sides: oracle Terminal.zig:15071-15079, 15115-15123, 15154-15164, 15333-15375 use `.active` literals on 10x5 terminals; the ports at terminal.rs:20074-20078, 20208, 20268, 20313, 20350, 20402, 20458-20470 use screen_pt (terminal.rs:7232). In every one of these tests the terminal is freshly initialised 10x5 and at most 3 rows are written (prompt_start + two carriage_return/linefeed pairs), so nothing ever scrolls into scrollback and Point::Screen and Point::Active resolve to the same row for all asserted coordinates. The finder concedes the assertions currently evaluate identically; there is no input or state in these tests that yields a different result, so this is a fidelity nit in unreachable divergence, not an observable difference.
- **S149-F4** (low) — Tests 'kitty max bytes exceeded' / 'feedSlice kitty max bytes exceeded' build the Handler with glyph max_bytes = 0 instead of the default
  - Refuted: Read both. Oracle apc.zig:581 / 745 `var h: Handler = .{ .max_bytes = .init(.{ .kitty = 4 }) }` vs rustty apc/mod.rs:950-958 and 1174-1182 `[0usize; Protocol::COUNT]` with only the kitty slot set. The glyph slot value is only ever read on the identify `;`/glyph path (apc.zig:86-90 / mod.rs:180-183), and neither test feeds a glyph identifier — both feed 'Ga=t;...' which takes the immediate 'G' kitty branch. Every assertion in the rustty tests matches the oracle one-for-one (state != ignore, state != ignore, state == ignore). The fixture divergence is unreachable in these tests, so no observable difference; refuted.
- **S154-F1** (low) — Stale PORT NOTE says fraction call-site switch is 'a later wave' but it is already done
  - Refuted: Verified both sides: crates/rustty-vt/src/lib.rs:153-158 carries the stale sentence 'Ported here as a leaf; the call-site switch is a later wave', while color.rs:1212 and apc/glyph/request.rs:1002 both already call crate::fraction::parse, matching oracle color.zig/apc glyph request behaviour. The note is merely out-of-date prose; nothing in production code is deferred, stubbed, or behaviourally different, so there is no observable functional divergence. Comment-only staleness with the actual port complete is not a parity finding under the semantic-equivalence lens.
- **S158-F2** (low) — std_capacity test-build grapheme_bytes gate is crate-local (cfg!(test)) rather than build-wide (builtin.is_test)
  - Refuted: Refuted as speculative. Production behaviour is identical (8192 both sides), and every oracle test that depends on the 512 figure was ported as an in-crate unit test (crates/rustty-vt/src/page_tests.rs:411-620, pagelist/tests.rs, screen.rs, terminal.rs), where cfg!(test) is true and grapheme_bytes==512 exactly as builtin.is_test gives. I grepped all callers of std_capacity across crates/ (only rustty-vt internals plus crates/rustty-vt/tests/std_size_golden.rs): the sole out-of-crate consumer deliberately WANTS the production value — its module doc (tests/std_size_golden.rs:1-6) says 'This is an INTEGRATION test on purpose: the library is compiled WITHOUT cfg(test), so std_capacity() returns the PRODUCTION capacity (grapheme_bytes = 8192 ...)' and asserts cap.grapheme_bytes==8192 at line 82. No downstream test relies on 512, so the finder's 'could diverge' has no concrete instance; the difference is confined to a hypothetical. Also note the cited oracle line was wrong (1833, not 1770).
- **S161-F3** (low) — Fuzz test drives a different random operation sequence than the oracle
  - Refuted: Compared render.zig:1549-1600+ with render.rs:2864-3030: same seed constant 0xB0BA_CAFE, same 300 iterations, same 1..6 op batches, same 19 op kinds with identical escape sequences and inner counts, same terminal opts (20x8, 500 scrollback), and the same assertions via test_compare_states/testCompareStates. The only difference is how bounded draws are derived from the xoshiro256++ stream (Lemire/type-width in Zig vs modulo on a u64 in rustty), which changes which random op sequence a fixed seed produces but does not make the test assert less nor change any production behaviour. Both are equally strong randomized fuzz drivers, and the port documents the divergence at render.rs:2864-2866/2901. Not an observable functional difference.
- **S162-F1** (low) — OffsetBuf::member / get_offset use `as OffsetInt` where the oracle uses safety-checked @intCast
  - Refuted: Read ghostty/src/terminal/size.zig:121-127 (`member` -> `@intCast(self.offset + len)`) and :149-158 (`getOffset` -> `@intCast(offset)`) against rustty crates/rustty-render-types/src/term_size.rs:320-324 and :362-367 (`as OffsetInt`). size.zig:8-12 defines `max_page_size = maxInt(u32)` and `OffsetInt = IntFittingRange(0, max_page_size-1)`, i.e. u32, and term_size.rs:27-29 mirrors that exactly; every in-domain offset therefore fits and both sides produce identical values. The only cited difference is the out-of-range failure mode (safety-checked panic vs wrap), which is (a) unreachable for any valid page and (b) not even a panic in Ghostty's shipping ReleaseFast build, where @intCast on an out-of-range value is UB rather than a defined trap. No input/state yields a different observable result, so this is a defensive-check style difference in unreachable code, not a functional-parity finding.
- **S162-F2** (low) — 'sgr: Attribute C compat' test body is empty in rustty-vt
  - Refuted: Oracle sgr.zig:548-550 is `test "sgr: Attribute C compat" { _ = Attribute.C; }` — a pure compile-time force of the tagged-union C projection; it makes no runtime assertion, so the empty rustty-vt test at crates/rustty-vt/src/sgr.rs:609-614 asserts exactly as much at runtime (nothing). The compile-time guarantee the oracle buys is delivered, and strengthened, in rustty-vt-c: crates/rustty-vt-c/src/lib.rs:119 declares `pub mod sgr;` unconditionally (no cfg gate), and crates/rustty-vt-c/src/sgr.rs:242-244 carries build-failure gates `const _: () = assert!(size_of::<CValue>() == size_of::<[u64;8]>()); ... align_of == 8; size_of::<Tag>() == size_of::<c_int>()` plus the `AttributeC::cval` construction, all compiled on every build rather than only under `zig test`. Equivalent (in fact stronger) compile-time coverage in the crate where the C type must live under R4; at most a cosmetic test-placement/naming issue, not a weakened assertion.
- **S165-F2** (low) — ContinuationTestHandler's `else => self.committed += 1` catch-all is expanded to only bell/set_attribute/window_title
  - Refuted: Compared stream.zig:4666-4711 (`else => self.committed += 1`) with stream.rs:6605-6688 (explicit bell/set_attribute/window_title overrides plus print/print_slice/print_repeat/apc_end/dcs_unhook). I enumerated the actual continuation corpora in stream.rs (lines ~6705-6835, 6941-7400, 7398-7399): inputs are printable text, malformed UTF-8, CSI SGR (\x1b[0m, \x1b[38;5...), OSC titles (\x1b]2;/\x1b]0;), APC (\x1b_G..., \x1b_25a1;s), DCS (\x1bP$qm, \x1bP1q, \x1bP+q), truncated prefixes that never dispatch (\x1b(, \x1b[1$, \x1b[:, \x1bP:), and CAN/SUB (\x18/\x1A) which dispatch nothing in either tree. No corpus produces an action outside the overridden set, so every committed count and every `restored.handler.committed == 0` assertion evaluates identically in both trees. The finder itself concedes the expansion is complete for today's corpora; the concern is future-corpus robustness, which is not an observable functional difference. Also note Rust's trait has no expressible catch-all, making this the faithful idiom mapping.
- **S169-F1** (low) — Debug `Style.format` renders color and underline values differently from the oracle's `{any}` formatting
  - Refuted: Read both sides. Oracle style.zig:250-313 prints non-default fields with "{s}={any}"; oracle Color has its own custom format (style.zig:69-88) emitting exactly "Color.none" / "Color.palette{ N }" / "Color.rgb{ r, g, b }", which is what rustty style.rs:303-312 reproduces verbatim and calls from style.rs:505-506/513/519. The finder's claim rests entirely on a version-dependent Zig assertion that {any} bypasses the custom formatter; the oracle file still uses the transitional 4-arg format signature, so which renderer Zig picks here is not determinable by static reading, and the port matches the oracle's authored text. The only residual delta is the underline enum text ({:?} -> "Single" vs ".single"), which is debug-log-only: no production consumer and no test on either side asserts this string (grep of ghostty/src/terminal and rustty-vt found no assertions on "Style{ " or "fg_color=" other than the writers themselves). Inconclusive + cosmetic -> refuted.
- **S169-F2** (low) — decode_png default for the lib artifact depends on cargo feature unification rather than per-artifact comptime
  - Refuted: Compared sys.zig:18-21 with sys.rs:151-163. The port keeps the .lib arm (matches!(ARTIFACT, Artifact::Lib) -> None) and additionally makes the whole default None when the wuffs feature is off, which is exactly the configuration rustty-vt-c compiles under. The actual shipped lib artifact therefore has decode_png = None, identical to the oracle. The divergence exists only in a combined cargo selection that feature-unifies wuffs into a single rustty-vt build — a configuration the port explicitly excludes for published lib artifacts, documented at sys.rs:136-146 and enforced by an existing gate (verified present: /Users/gael/projects/rustty/.port/tools/gates/check_vtc_lib_isolation.sh). The finder itself concedes it is 'only observable in non-isolated builds'. No input/state in a produced artifact yields different behaviour, so this is a build-plumbing replacement detail, not a functional-parity finding.
- **S172-F2** (low) — ReaderAdapter::stream_to panics on a zero-length destination instead of returning 0 without invoking the callback
  - Refuted: The mechanical difference is real (io.zig:187-188 `if (limit == .nothing) return 0;` vs crates/rustty-vt-c/src/io.rs:343-352 which has no empty-dest guard and falls into read_into, whose io.rs:214 `assert!(!destination.is_empty())` would panic), but it is confined to code that cannot be reached. `stream_to` is not a C-API entry point: its only callers are snapshot.rs:161-164 (a pure dispatch) and the three rustty-vt sites, each of which computes a provably non-zero cap before calling — snapshot/io.rs:495-507 returns EndOfStream when the limit is 0 and otherwise passes `space.min(r>0)`; snapshot/record.rs:512-524 has `debug_assert!(self.hend < self.hashing_buffer.len())` so space>0 and returns EndOfStream when remaining==0; snapshot/page.rs:1826-1839 returns early when bytes_before_failure==0 && !failed, so cap>0. There is no public path by which a caller can hand an empty slice to the C adapter, so no input or state produces a different observable result. Per the ground rule that a difference confined to unreachable code is refuted, this is a defensive-contract nit rather than a functional-parity finding.
- **S194-F1** (medium) — All 18 OSC 1337 tests were ported with Parser::init(None) instead of the reference's allocating parser, so the allocating-capture path is never exercised
  - Refuted: OSC 1337 never uses the allocating capture: ghostty src/terminal/osc.zig:843-847 dispatches state .@"1337" with `';' => self.captureTrailing(.fixed)` (and iterm2.zig has no `alloc` use outside its test prologues — grep shows alloc only on the `.init(testing.allocator)` lines 203-426). The port matches with the same fixed capture. So passing `testing.allocator` in the reference tests is inert for these 18 cases; `Parser::init(None)` in crates/rustty-vt/src/osc/parsers/iterm2.rs:277-522 exercises the identical code path with identical assertions. No coverage of the Vec backing is lost here (the alloc.is_none() fallback at osc/mod.rs:934-938 is never reached from OSC 1337).
- **S200-F1** (low) — MetaBuf::resize implements the pre-20abdb50a CircBuf.resize head/tail/full fixup (shrink path diverges)
  - Refuted: Re-read ghostty src/datastruct/circ_buf.zig:159-179 and rustty crates/rustty-vt/src/search/sliding_window.rs:1338-1382. The grow path is state-for-state identical: after rotate_to_zero tail==0 always; if full, the port sets head=prev_len(=prev_cap) and clears full == oracle's head=retained_len/tail=0/full=false; if not full, head already equals prev_len and full is already false. The shrink branch is unreachable: MetaBuf::resize's only caller is ensure_unused_capacity (sliding_window.rs:1331-1336) which returns early unless new_cap > capacity(), so size > prev_cap always; grep of '.resize(' in the file shows the other hits are Vec::resize (lines 202, 2147, 2556), and no test calls MetaBuf::resize directly. Difference confined to dead code; the faithful generic port exists at crates/rustty-datastruct/src/circ_buf.rs:165-193. Latent-fidelity note, not an observable functional difference.
- **S200-F2** (low) — MetaBuf::init sets full=false where the oracle CircBuf.init sets full = (size == 0)
  - Refuted: Re-read circ_buf.zig:67-76 vs sliding_window.rs:1271-1284. Only construction site is MetaBuf::init(alloc, 0) at sliding_window.rs:223. With storage.len()==0, len() returns 0 under either flag (full branch returns storage.len()==0; non-full branch returns head-tail==0). ensure_unused_capacity->resize from cap 0 lands on head=0/tail=0/full=false in both. append_assume_capacity's assert!(!self.full) (line 1423) is always preceded by ensure_unused_capacity (lines 1094-1097). meta.full/empty() are never read externally — the one place that would care (assert_integrity, sliding_window.rs:1158-1166) deliberately uses len()==0 and documents the zero-cap full==true semantics. No input or state produces a different result.
