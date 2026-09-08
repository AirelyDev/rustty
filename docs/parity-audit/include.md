# Parity audit — include

Audited the whole `include/` tree of the oracle (Ghostty @ `20abdb50a`) across 8 shards: 44 oracle units fully read, 14,640 lines read of 14,640 lines in scope (100%; no unit left partial). 19 candidate findings were raised and double-verified: **9 CONFIRMED, 3 DISPUTED, 7 REFUTED, 0 UNVERIFIED**. By corrected severity over the surviving 12: **2 high, 4 medium, 6 low**.

The single most important gap is **S1-F1**: `rustty_command_s` values handed to the macOS host are a non-`repr(C)` Rust struct of four `Vec<u8>` (96 B) where the header promises four `const char*` (32 B) — the command palette reads misaligned/garbage pointers. A second ABI-width defect of the same class is **S3-F1** (1-byte Rust enum written into a 4-byte C enum out-parameter for Kitty image format/compression).

## Coverage

| Oracle unit (path:lines) | Rustty counterpart(s) | Status | Note |
| --- | --- | --- | --- |
| `include/ghostty.h:1-1278` | `include/rustty-embed.h`, `.port/tools/gates/check_rustty_h_abi.sh`, `.port/tools/gates/check_embed_h_abi.sh`, `crates/rustty-apprt-c/tests/abi_layout.rs` | ported | Token-transformed header differs only by a 4-line provenance banner and one trailing space. All 91 `RUSTTY_API` prototypes resolve to exactly one `pub extern "C" fn` (78 in `rustty-apprt-c/src/lib.rs`, 13 in `rustty-config/src/c_api.rs`, 1 in `rustty-bench/src/capi.rs`); ordinals, bit values, negative enumerators, the `chostty_ipc_target_s` typo and `#ifdef __APPLE__` blocks preserved. No `unimplemented!()`/`todo!()`. |
| `include/ghostty.h` enums (platform/clipboard/mouse/mods/key/action/point/target/build_mode/config/ipc) | `rustty-apprt/src/embedded.rs`, `rustty-apprt-api/src/{structs,action,ipc,surface}.rs`, `rustty-vt/src/input/{mouse,key,key_mods}.rs`, `rustty-input/src/binding.rs`, `rustty-apprt-c/src/{capi,lib}.rs`, `rustty-config/src/{c_api,value_types,path}.rs`, `rustty-vt/src/{mouse,osc/mod,pagelist/grow}.rs`, `rustty-renderer/src/lib.rs` | ported | `ghostty_input_key_e` 176/176 and `ghostty_action_tag_e` 69/69 in order; struct field order/width matches (KeySequenceC 16B, KeyTableC 24B, OpenUrlC 24B). Rust ipc `ActionCValue` has a `new_tab` arm the upstream header lacks — matches the oracle's comptime-generated union (`src/apprt/ipc.zig:187-209`), i.e. the header is the stale side. |
| `include/ghostty/vt.h` | `include/rustty/vt.h` (+ 2 dist copies) | ported | Umbrella; doc-only additions (`@ref color_scheme`, `c-vt-color-scheme` example) and `rustty.sh` URL. All 30 includes + guard 1:1. |
| `include/ghostty/vt/allocator.h` | `include/rustty/vt/allocator.h`, `rustty-vt-c/src/allocator.rs`, `.../lib.rs` | ported | Header byte-identical post-transform; vtable layouts pinned; `free(NULL)` no-op, NULL allocator → default; all 6 reference tests present. |
| `include/ghostty/vt/build_info.h` | `include/rustty/vt/build_info.h`, `rustty-vt-c/src/build_info.rs`, `.../lib.rs` | ported | Enumerators 0..3 / 0..10 and per-variant out types match; `OPTIMIZE` derived from cargo profile instead of `builtin.mode` (build-system replacement). |
| `include/ghostty/vt/color.h` | `include/rustty/vt/color.h`, `rustty-vt-c/src/color.rs`, `.../lib.rs` | ported | 479 lines byte-identical; all 11 functions exported under `feature = "color"`, mirroring `lib_vt.zig` `if (features.color)`. |
| `include/ghostty/vt/color_scheme.h` | `include/rustty/vt/color_scheme.h`, `rustty-vt-c/src/color_scheme.rs`, `.../lib.rs` | ported | Byte-identical; OUT_OF_SPACE + required size + fitting prefix match; out-of-range int → INVALID_VALUE (unreachable for conforming callers). |
| `include/ghostty/vt/device.h` | `include/rustty/vt/device.h`, `rustty-vt-c/src/terminal.rs`, `rustty-vt/src/device_status.rs` | ported | 18 conformance / 16 feature / 11 device-type defines, DA1/DA2/DA3 struct widths and field order match. |
| `include/ghostty/vt/focus.h` | `include/rustty/vt/focus.h`, `rustty-vt-c/src/focus.rs`, `.../lib.rs` | ported | Byte-identical; export gated on `input-encode` mirroring `features.input_encode`; all 4 reference tests present (+1 extra). |
| `include/ghostty/vt/formatter.h` | `include/rustty/vt/formatter.h`, `rustty-vt-c/src/formatter.rs`, `.../lib.rs` | ported | Byte-identical; 3 options structs field-order match; 5 functions gated on `formatter`; error contracts match. |
| `include/ghostty/vt/grid_ref.h` | `include/rustty/vt/grid_ref.h`, `rustty-vt-c/src/grid_ref.rs`, `.../lib.rs` | ported | Byte-identical; `CGridRef{size,node,x,y}`; 5 functions under `grid-introspection`; NULL/OUT_OF_SPACE contracts match. |
| `include/ghostty/vt/grid_ref_tracked.h` | `include/rustty/vt/grid_ref_tracked.h`, `rustty-vt-c/src/grid_ref_tracked.rs`, `.../lib.rs` | ported | Byte-identical; feature gates exactly as `lib_vt.zig:409-415`; NO_VALUE/OOM paths match. |
| `include/ghostty/vt/io.h` | `include/rustty/vt/io.h`, `rustty-vt-c/src/io.rs` | ported | Typedefs only; Reader/Writer/MimeReader signatures match. |
| `include/ghostty/vt/key.h` | `include/rustty/vt/key.h` (+ `key/event.h`, `key/encoder.h`) | ported | Doc + two includes; both sub-headers present. |
| `include/ghostty/vt/kitty_graphics.h` | `include/rustty/vt/kitty_graphics.h` (+2 dist), `rustty-vt-c/src/{lib,kitty_graphics}.rs`, `example/c-vt-kitty-graphics` | ported | 871/871 byte-identical; 16 functions under `kitty-graphics`; enum values and `PlacementRenderInfo` order match. One ABI-width defect in the image-data out path (S3-F1). |
| `include/ghostty/vt/modes.h` | `include/rustty/vt/modes.h`, `rustty-vt-c/src/{lib,modes}.rs`, `rustty-vt/src/modes.rs`, `example/c-vt-modes` | ported | 42 mode defines, 3 inline helpers with the `0x7FFF`/`<<15` packing, ReportState 0-4 match. |
| `include/ghostty/vt/mouse.h` | `include/rustty/vt/mouse.h` (+ `mouse/event.h`, `mouse/encoder.h`), `example/c-vt-encode-mouse` | ported | Umbrella identical; both sub-headers same line counts as oracle. |
| `include/ghostty/vt/osc.h` | `include/rustty/vt/osc.h`, `rustty-vt-c/src/{lib,osc}.rs`, `rustty-vt/src/osc/mod.rs` | ported | CommandType 0-26 and CommandData 0/1 match; 7 functions with matching signatures. |
| `include/ghostty/vt/paste.h` | `include/rustty/vt/paste.h`, `rustty-vt-c/src/{lib,paste}.rs`, `example/c-vt-paste` | ported | Source 0/1 and `GhosttyPaste` layout match; 3 functions match; all snippets present. |
| `include/ghostty/vt/point.h` | `include/rustty/vt/point.h`, `rustty-vt-c/src/point.rs`, `rustty-vt/src/point.rs` | ported | Coordinate `{u16,u32}`, Tag 0-3, union `_padding[2]` const-asserted 16/8. |
| `include/ghostty/vt/render.h` | `include/rustty/vt/render.h` (+ dist), `rustty-vt-c/src/render.rs`, `.../lib.rs:2281-2677`, `render/tests.rs`, `example/c-vt-render` | ported | 890/890 byte-identical; 22 functions under `render-state`; all enum discriminants and `repr(C)` structs match; 46 oracle tests have same-named counterparts (59 total). |
| `include/ghostty/vt/screen.h` | `include/rustty/vt/screen.h`, `rustty-vt-c/src/{cell,row}.rs`, `.../lib.rs:1255-1335` | ported | 420/420; Cell/Row = u64, CellsView `{ptr,len}`, all enums match; 8+6 oracle tests ported. |
| `include/ghostty/vt/search.h` | `include/rustty/vt/search.h` (+ dist), `rustty-vt-c/src/search.rs`, `.../lib.rs:2897-3034`, `example/c-vt-search` | ported | 501/501; 4 enums and 8 functions match; all 17 oracle tests ported. |
| `include/ghostty/vt/selection.h` | `include/rustty/vt/selection.h`, `rustty-vt-c/src/{selection,selection_gesture}.rs`, `.../lib.rs` | ported | 1084 lines byte-identical; 6 sized structs, 7 enums, 20 functions match. |
| `include/ghostty/vt/sgr.h` | `include/rustty/vt/sgr.h`, `rustty-vt-c/src/sgr.rs`, `.../lib.rs:1397-1510` | ported | Tag 0-30, Underline 0-5, CValue union with pinned `_padding:[u64;8]`; 9 functions match. |
| `include/ghostty/vt/size_report.h` | `include/rustty/vt/size_report.h`, `rustty-vt-c/src/size_report.rs`, `.../lib.rs` | ported | Style 0-3 and Size struct match; NULL buf / OUT_OF_SPACE required-size behaviour matches `c/size_report.zig:20-31`. |
| `include/ghostty/vt/snapshot.h` | `include/rustty/vt/snapshot.h`, `rustty-vt-c/src/snapshot.rs`, `.../lib.rs:1701-1905` | ported | Only diffs are a retargeted `@see` URL and one continuation indent; 12 functions, defaults (65 MiB, retain=false) and get_multi error indexing match. |
| `include/ghostty/vt/style.h` | `include/rustty/vt/style.h`, `rustty-vt-c/src/style.rs`, `.../lib.rs` | ported | ColorTag 0-2, union, Style struct (3 colors, 8 bools, c_int underline) match. |
| `include/ghostty/vt/sys.h` | `include/rustty/vt/sys.h`, `rustty-vt-c/src/sys.rs`, `.../lib.rs:429-480` | ported | Image/LogLevel/callback typedefs/Option 0-3 match; NULL clears callbacks and restores platform random. Two log-path nuances (S5-F1, S5-F2). |
| `include/ghostty/vt/terminal.h` | `include/rustty/vt/terminal.h` (+2 dist), `rustty-vt-c/src/{terminal,lib}.rs` | ported | Byte-identical modulo whitespace; 18 functions, 12 enums and every struct layout verified against `c/terminal.zig:683-1868`. Two artifact-default divergences (S6-F1, S6-F2). |
| `include/ghostty/vt/types.h` | `include/rustty/vt/types.h`, `rustty-vt-c/src/{result,selection_gesture,lib}.rs`, `rustty-lib/src/types.rs` | ported | Byte-identical; `RusttyResult` 0,-1..-7; 15 opaque handles; String/Buffer/SurfacePosition/Codepoints match. |
| `include/ghostty/vt/unicode.h` | `include/rustty/vt/unicode.h`, `rustty-vt-c/src/unicode.rs`, `.../lib.rs` | ported | Identical modulo a 2-space continuation indent; documented contracts (>U+10FFFF → 1, len==0 → 0) match; 4 oracle tests ported. |
| `include/ghostty/vt/wasm.h` | `include/rustty/vt/wasm.h`, `rustty-vt-c/src/lib.rs:1942-1963`, `rustty-lib/src/allocator/wasm.rs` | ported | Identical; 5 exports under `cfg(target_arch="wasm32")`; allocator semantics line-for-line; both oracle tests ported. |
| `include/ghostty/vt/key/encoder.h` | `include/rustty/vt/key/encoder.h`, `rustty-vt-c/src/key_encode.rs`, `rustty-vt/src/kitty/key.rs`, `rustty-vt/src/input/config.rs` | ported | Kitty flag bits, OptionAsAlt 0..3, EncoderOption 0..7 match; 5 exports under `input-encode`; OUT_OF_SPACE counting writer matches `key_encode.zig:136-170`. |
| `include/ghostty/vt/key/event.h` | `include/rustty/vt/key/event.h`, `rustty-vt-c/src/key_event.rs`, `rustty-vt/src/input/{key,key_mods}.rs` | ported | Action 0/1/2; 176 contiguous Key values in identical order with a `KEY_KEY_COUNT=176` guard; Mods u16 bit layout matches; 16 exports. |
| `include/ghostty/vt/mouse/encoder.h` | `include/rustty/vt/mouse/encoder.h`, `rustty-vt-c/src/{lib,mouse_encode}.rs` | ported | Byte-identical (dist copies too); 6 functions; TrackingMode/Format/Option discriminants and the 9-field `repr(C)` Size match. |
| `include/ghostty/vt/mouse/event.h` | `include/rustty/vt/mouse/event.h`, `rustty-vt-c/src/{lib,mouse_event}.rs`, `rustty-vt/src/input/mouse.rs` | ported | Byte-identical; 11 functions; Action 0-2, Button 0-11, Mods u16, `Pos{f32,f32}` by value. |
| `macos/GhosttyUITests/AppKitExtensions.swift` | `macos/RusttyUITests/AppKitExtensions.swift` | ported | Identical modulo header-comment rename; luminance weights and `colorAt` bodies preserved. |
| `macos/GhosttyUITests/GhosttyCommandPaletteTests.swift` | `macos/RusttyUITests/RusttyCommandPaletteTests.swift` | ported | Same predicates, chords and timeouts; `Command Palette` menu item present in `MainMenu.xib:337`. |
| `macos/GhosttyUITests/GhosttyCustomConfigCase.swift` | `macos/RusttyUITests/RusttyCustomConfigCase.swift` (+ `AppDelegate.swift:177`, `UserDefaults+Extension.swift:6`) | ported | Env vars renamed and consumed at the same use-sites. |
| `macos/GhosttyUITests/GhosttyMouseStateTests.swift` | `macos/RusttyUITests/RusttyMouseStateTests.swift` (+ `TerminalSplitTreeView.swift:129`) | ported | Upstream PR URL correctly kept verbatim; both tests preserved. |
| `macos/GhosttyUITests/GhosttyThemeTests.swift` | `macos/RusttyUITests/RusttyThemeTests.swift` (+ `TerminalView.swift:174`, `xtask/src/res_themes.rs`) | ported | All 9 tests, helper and luminance asserts preserved; theme corpus emitted by xtask. |
| `macos/GhosttyUITests/GhosttyTitleUITests.swift` | `macos/RusttyUITests/RusttyTitleUITests.swift` | ported | Identical modulo rename. |
| `macos/GhosttyUITests/GhosttyTitlebarTabsUITests.swift` | `macos/RusttyUITests/RusttyTitlebarTabsUITests.swift` (+ `TerminalWindow.swift:384`) | ported | 5 tests + geometry helper with the same accuracies. |
| `macos/GhosttyUITests/GhosttyWindowPositionUITests.swift` | `macos/RusttyUITests/RusttyWindowPositionUITests.swift` (+ `AppDelegate.swift:192`, `SplitView.swift:179`) | ported | All 9 tests and accuracies preserved; Xcode target present with the same reference count. |

**Partial units:** none. Every unit listed in scope was read end to end (`units_partial` was empty in all 8 shards).

## Findings

### High

#### S1-F1 — rustty_command_s elements handed to C are not the header's 4x const char* layout

- **Status:** ✅ CONFIRMED (both lenses failed to refute)
- **Kind:** behavioral-difference · **Severity:** high
- **Oracle:** `include/ghostty.h:421-426, 508-512` (and `src/input/command.zig:25-30`)
- **Rustty:** `crates/rustty-input/src/command.rs:46-57` (and `crates/rustty-config/src/value_types.rs:5943-5973`, `crates/rustty-config/src/c_get.rs:190-198,306`)

**Description.** The header declares `ghostty_command_s` as four NUL-terminated `const char*` (32 bytes) and `ghostty_config_command_list_s` as `{const ghostty_command_s* commands; size_t len}`. rustty's `RepeatableCommandC.commands` (`value_types.rs:5953-5956`) points at `Vec<rustty_input::command::C>`, but `command::C` (`command.rs:52-57`) is a plain Rust struct of four `Vec<u8>` with **no `#[repr(C)]`** — 96 bytes per element (each field a `{ptr,cap,len}` Vec header, field order unspecified). `rustty_config_get(cfg, &list, "command-palette-entry", ..)` (`c_get.rs:306` via `g_cval!`) writes this pointer to the host, and the macOS app then iterates `UnsafeBufferPointer<rustty_command_s>(start: v.commands, count: v.len)` with a 32-byte stride and calls `String(cString:)` on each field (`macos/Sources/RusttyKit/Rustty.Config.swift:714-722`, `Rustty.Command.swift:33-38`). The oracle's `Command.C` is an `extern struct` of `[*:0]const u8`. Result: misaligned/garbage pointers for every entry after the first field — the command palette (`TerminalCommandPalette.swift:130`) and the App Intents `CommandEntity` read corrupt data or crash. Neither `abi_layout.rs` nor `check_embed_h_abi.sh` gates this type (pointer-passed, listed "unfrozen"), and the only test (`value_types.rs:9575-9615`) reads the Rust struct back through the Rust type, so it cannot detect the ABI break.

**Evidence.** `ghostty.h:421-426` `typedef struct { const char* action_key; const char* action; const char* title; const char* description; } ghostty_command_s;` vs `command.rs:51-57` `#[derive(Clone, Debug, PartialEq)] pub struct C { pub action_key: Vec<u8>, pub action: Vec<u8>, pub title: Vec<u8>, pub description: Vec<u8> }` (no `repr(C)`); `value_types.rs:5953-5956` `#[repr(C)] pub struct RepeatableCommandC { pub commands: *const rustty_input::command::C, pub len: usize }`; `Rustty.Config.swift:719-721` `let buffer = UnsafeBufferPointer(start: v.commands, count: v.len); return buffer.map { Rustty.Command(cValue: $0) }`.

**Suggested fix.** Make `rustty_input::command::C` `#[repr(C)]` with four `*const c_char` (NUL-terminated) fields, keeping the owned NUL-terminated byte buffers alive alongside (e.g. `RepeatableCommand` owns a `Vec<CommandCStorage>` of the four CStrings while `value_c: Vec<C>` holds the pointers, rebuilt together as the reference's arena-owned `[*:0]` strings are). Add `// @embed-abi rustty_command_s 32` plus const size/offset asserts, pin it in `abi_layout.rs`, and extend the `value_types.rs` cval test to read fields through `CStr::from_ptr` so the C view is what is asserted.

#### S3-F1 — IMAGE_DATA_FORMAT/COMPRESSION write a 1-byte Rust enum into the caller's 4-byte C enum

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** high
- **Oracle:** `include/ghostty/vt/kitty_graphics.h:312-336, 407-411, 422-428` (with `src/terminal/kitty/graphics_command.zig:518-539`, `src/lib/enum.zig:38-41`)
- **Rustty:** `crates/rustty-vt-c/src/kitty_graphics.rs:338-344, 540-542`; `crates/rustty-vt/src/kitty/graphics_command.rs:863-873, 916-920`

**Description.** The header contract (identical in oracle and rustty) says `rustty_kitty_graphics_image_get(..., RUSTTY_KITTY_IMAGE_DATA_FORMAT, out)` stores a `RusttyKittyImageFormat` (an `enum : int`, 4 bytes), likewise `RusttyKittyImageCompression`. In the oracle these are `lib.Enum(lib.target, ...)` = `enum(c_int)` for the C build, so `out.* = image.format` writes a full `c_int`. In rustty, `ImageFormat`/`ImageCompression` alias `rustty_vt::kitty::graphics_command::Format`/`Compression`, which are plain Rust enums with only `#[derive(...)]` and no `#[repr(C)]`/`#[repr(i32)]` — 1 byte. `image_get_typed` then does `*(out as *mut ImageFormat) = image.format` (`:540`) / `*(out as *mut ImageCompression) = image.compression` (`:542`), writing only the low byte of the caller's int and leaving the other three untouched. A C/FFI caller that does not pre-zero its variable (the rustty example happens to write `format = 0` first, `main.c:177`, masking it) reads garbage in the high bytes; on big-endian targets the value is wrong even when pre-zeroed. The same path is reached via `rustty_kitty_graphics_image_get_multi`. The type JSON (`types.rs:3074-3076`) still advertises these enums as size 4 / underlying i32, so the published ABI description and the actual write width disagree. Unit tests declare the out variable as the Rust `ImageFormat` type (`kitty_graphics.rs:2064`), not `c_int`, so they cannot catch it. Note the sibling C enums in the same file (`PlacementIteratorOption`, `ImageData`) *are* `#[repr(C)]`, and `selection.rs:191` carries exactly the size guard these types lack.

**Evidence.** Oracle `graphics_command.zig:518-527,536-539` `pub const Format = lib.Enum(lib.target, &.{...})` with `enum.zig:38 .c => c_int`; header `312-328`. Rustty `graphics_command.rs:863` `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Format { Rgb, Rgba, Png, GrayAlpha, Gray }` (no repr); `kitty_graphics.rs:540` `ImageData::Format => unsafe { *(out as *mut ImageFormat) = image.format }`; `types.rs:3075` `write_size_align(out, len, size_of::<c_int>(), align_of::<c_int>())`.

**Suggested fix.** Either give `Format`/`Compression` `#[repr(C)]` (they are C-target enums in the oracle) or convert at the boundary: `*(out as *mut c_int) = image.format as c_int` (same for compression) in `image_get_typed`. Add a test writing into a `c_int` initialised to `-1` and asserting the full value, plus `const _: () = assert!(size_of::<ImageFormat>() == size_of::<c_int>())` if the repr route is taken.

### Medium

#### S4-F1 — ghostty_* C symbol identity absent: headers and exports declare only rustty_* names, no alias layer

- **Status:** ⚠️ DISPUTED (one lens refuted)
- **Kind:** other · **Severity:** medium (corrected: medium)
- **Oracle:** `include/ghostty/vt/render.h:394-889` (also `screen.h:340-419`, `search.h:340-500`)
- **Rustty:** `include/rustty/vt/render.h:394-889`; `crates/rustty-vt-c/src/lib.rs:2296-2677,1262-1335,2913-3034`

**Description.** The oracle headers declare 34 C functions under `ghostty_*` names (22 `render_state_*`, `cell_get`/`cell_get_multi`/`row_get`/`row_get_multi`, 8 `search_*`). The rustty headers declare them only as `rustty_*` and the crate exports only `rustty_*` symbols. The parity naming rule as understood by the shard states `ghostty_*` symbols are KEPT and aliased as `rustty_*`; the project's completion plan records the alias export layer as locked-but-not-yet-done. A grep of `include/`, `crates/`, `xtask/`, `macos/`, `crates-sys/`, `example/` for `ghostty_render_state_new`, `ghostty_terminal_new`, `export_name`, `link_name` finds no alias file, `#define`, or `export_name` attribute anywhere (the only hit is comment text). A C/Swift consumer written against the upstream header cannot compile or link against this header/library.

**Evidence.** Oracle `render.h:394` `GHOSTTY_API GhosttyResult ghostty_render_state_new(...)` vs rustty `render.h:394` `RUSTTY_API RusttyResult rustty_render_state_new(...)`; `lib.rs:2296` `pub unsafe extern "C" fn rustty_render_state_new`; `grep -rl "ghostty_terminal_new\|ghostty_render_state_new" crates include xtask macos crates-sys example` → only `crates/rustty-vt-c/src/lib.rs` (comment text).

**Suggested fix.** Add the planned generated alias layer: `#[export_name = "ghostty_..."]` twin exports (or a C shim / `.def` / linker alias file) for every `rustty_*` symbol, plus `#define ghostty_X rustty_X` / typedef aliases in the headers so both identities resolve. Workspace-wide item; this shard covers 34 symbols.

> **Refuting lens: presence** (confidence high) — verbatim:
> "Not a functional gap — a naming transform explicitly mandated by the port's own parity contract, and the upstream alias layer no longer exists. (1) Symbol-set parity is exact: I extracted every `ghostty_*(` from /Users/gael/projects/ghostty/include/ghostty/vt/{render,search,screen}.h (40 symbols) and every `rustty_*(` from /Users/gael/projects/rustty/include/rustty/vt/{render,search,screen}.h, applied the ghostty_->rustty_ rename, and `diff` reported IDENTICAL_SYMBOL_SETS — nothing is missing, only prefixed differently. (2) The direction the finding asks for is FORBIDDEN by the project gate: DEFINITION_OF_PARITY.md:402 defines R2-ABI-shape as 'renamed ghostty_*->rustty_* ... zero residual ghostty_* symbols', and :421-422 makes 'zero ghostty_*/_ghostty_* symbols; zero literal ghostty/Ghostty in published headers' a hard R2 gate. (3) The finding's premise that the alias layer is 'locked-but-not-yet-done' is stale: DEFERRALS.md:7870 (vet row W7) records that upstream DELETED the ghostty_* C-ABI forwarder layer at e4cc852 and that the ROADMAP constraint 'ghostty_* C-ABI aliases stay' is FALSE (:7911-7912), and crates/rustty-apprt-c/Cargo.toml states the crate exports ONLY rustty_* in every profile. The 'reused Swift app hard-codes ghostty_' argument is also wrong for this tree — the macOS side consumes RusttyKit (DEFERRALS.md:4734). Observable C-ABI behaviour per function is unchanged."

*(The semantic lens did not refute, noting the gap is real and acknowledged in `rustty-completion-plan.md:27,50`. Related, separately-refuted duplicates of this same theme appear as S2-F1, S3-F2, S5-F3 and S6-F3 in the Refuted section.)*

#### S4-F2 — Caller-supplied GhosttyAllocator is not used for render-state internals or the search needle/scrollback state

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `include/ghostty/vt/render.h:384-395, 410-463`; `search.h:340-362, 425-445` (with `src/terminal/c/render.zig:210,222,233`, `c/search.zig:239`)
- **Rustty:** `crates/rustty-vt-c/src/render.rs:631-720`; `crates/rustty-vt-c/src/search.rs:685-690`

**Description.** The header documents `allocator: Pointer to allocator, or NULL to use the default allocator` for `ghostty_render_state_new` and `ghostty_search_new`. In the oracle the allocator handed to `new()` is stored in the wrapper and threaded into every growth path (`state.state.update(state.alloc, t)`, `beginUpdate(state.alloc, t)`, `deinit(alloc)`, `TerminalSearch.init(wrapper.alloc, bytes)`). In rustty only the wrapper struct itself goes through the caller allocator; `update`/`begin_update`/`deinit` pass `allocator_api2::alloc::Global` (`render.rs:648,678,712`) and `TerminalSearch::init(allocator_api2::alloc::Global, bytes)` (`search.rs:686`). A custom/tracking/wasm allocator therefore never observes row-data, cell, style, grapheme or search-buffer allocations, and an allocator that rejects cannot make `update`/`begin_update` return `GHOSTTY_OUT_OF_MEMORY` as documented. This is the acknowledged OPEN "Tier 2" item in `.port/phase-2/DEFERRALS.md:8069-8101` (F-008/W12-01).

**Evidence.** `ghostty src/terminal/c/render.zig:222` `state.state.update(state.alloc, t) catch return .out_of_memory;` vs rustty `render.rs:677-681` `let gpa = allocator_api2::alloc::Global; ... state.state.update(gpa, t_ref)`; `ghostty c/search.zig:239-242` `TerminalSearch.init(wrapper.alloc, bytes)` vs rustty `search.rs:685` `TerminalSearch::init(allocator_api2::alloc::Global, bytes)`; `DEFERRALS.md:8085` "Tier 2 REMAINS OPEN ... `update`/`begin_update` bypass the caller allocator (oracle: `state.alloc`)".

**Suggested fix.** Thread an `allocator_api2::Allocator` adapter over the C `ZigAllocator` through `RenderState::update`/`begin_update`/`deinit` and `TerminalSearch::init` (the fix plan already recorded in DEFERRALS.md F-008/W12-01 Tier 2).

#### S6-F1 — librustty-vt terminal_new defaults Kitty image storage to 320MB instead of libghostty-vt's 10MB

- **Status:** ✅ CONFIRMED
- **Kind:** different-constant · **Severity:** medium
- **Oracle:** `src/terminal/Terminal.zig:289-298` (used by `src/terminal/c/terminal.zig:864-902`; `src/build_config.zig:92` `.Lib => .lib`)
- **Rustty:** `crates/rustty-vt-c/src/terminal.rs:1538-1562`; `crates/rustty-vt/src/options.rs:32`

**Description.** `ghostty_terminal_new` relies on `Terminal.Options` defaults, which select the Kitty image storage limit by build artifact: 320MB for the ghostty app, 10MB for the lib artifact (libghostty-vt is built as `.Lib` → `.lib`). rustty-vt-c's `new_` hard-codes the app value `320*1000*1000` because rustty-vt's `ARTIFACT` constant is always `Ghostty` (its PORT NOTE says the lib artifact is "selected by a later build seam" that does not exist). Observable through `GHOSTTY_TERMINAL_DATA_KITTY_IMAGE_STORAGE_LIMIT` (header 1770-1778) immediately after `terminal_new`, and through Kitty graphics eviction: a librustty-vt embedder retains up to 320MB of images where libghostty-vt evicts past 10MB.

**Evidence.** Oracle `Terminal.zig:291-298` `kitty_image_storage_limit: usize = switch (build_options.artifact) { .ghostty => 320 * 1000 * 1000, .lib => 10 * 1000 * 1000 }` and `c/terminal.zig:884-891` (no override). Rustty `terminal.rs:1560` `kitty_image_storage_limit: 320 * 1000 * 1000,`; `options.rs:32` `pub const ARTIFACT: Artifact = Artifact::Ghostty;`.

**Suggested fix.** Give rustty-vt-c (the lib artifact) the `.lib` default: select `10*1000*1000` in `new_` (or thread an `Artifact::Lib` through a rustty-vt feature/const so `options::ARTIFACT` reflects the lib build), and add a `get(KITTY_IMAGE_STORAGE_LIMIT)`-after-`new` default test.

#### S6-F2 — Mode 5522 (Kitty paste events) is disabled in librustty-vt on non-macOS targets, unlike libghostty-vt

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/terminal/modes.zig:334-342`; `include/ghostty/vt/terminal.h:804-807, 1512-1520`
- **Rustty:** `crates/rustty-vt/src/modes.rs:619, 629-632`; `crates/rustty-vt/src/options.rs:32`

**Description.** In the oracle the lib artifact never disables mode 5522 (the predicate is false whenever `artifact == .lib`), so libghostty-vt on Linux/Windows accepts `CSI ? 5522 h/l`, `GHOSTTY_TERMINAL_OPT_MODE`/`MODE_DEFAULT` with mode 5522, and the paste-event flow documented for `GHOSTTY_TERMINAL_OPT_CLIPBOARD_READ`. rustty-vt's `ARTIFACT` is fixed to `Ghostty`, so the ported predicate collapses to `!cfg!(target_os = "macos")`: on Linux/Windows builds of rustty-vt-c, `mode_from_int` returns `None` for 5522 (`modes.rs:386` `!entry.disabled && ...`), the pty cannot enable the mode, `terminal_set(OPT_MODE, {5522,true})` returns INVALID_VALUE, `terminal_get(DATA_MODE, {5522})` returns INVALID_VALUE, and `terminal_paste` never emits the OSC 5522 paste event the header promises. On macOS the two agree.

**Evidence.** Oracle `modes.zig:341` `.disabled = build_options.artifact != .lib and builtin.os.tag != .macos,`. Rustty `modes.rs:629-632` `const KITTY_PASTE_EVENTS_DISABLED: bool = !matches!(crate::options::ARTIFACT, crate::build_options::Artifact::Lib) && !cfg!(target_os = "macos");` with `options.rs:32` `ARTIFACT = Artifact::Ghostty` for every build including rustty-vt-c.

**Suggested fix.** Make the lib artifact real for rustty-vt-c builds (feature-selected `options::ARTIFACT = Lib`) so the predicate evaluates to false, and add a Linux-target test asserting mode 5522 is settable through `OPT_MODE` / `CSI ? 5522 h`.

### Low

#### S2-F3 — Comment cites RUSTTY_FOCUS_EVENT_MAX_VALUE but the header enumerator is RUSTTY_FOCUS_MAX_VALUE

- **Status:** ✅ CONFIRMED
- **Kind:** other · **Severity:** low
- **Oracle:** `include/ghostty/vt/focus.h:43`
- **Rustty:** `crates/rustty-vt-c/src/focus.rs:48-49`

**Description.** Comment-only. `focus.rs` doc comment says "`include/rustty/vt/focus.h` itself declares `RUSTTY_FOCUS_EVENT_MAX_VALUE`"; the published header (and the oracle, `GHOSTTY_FOCUS_MAX_VALUE` at `focus.h:43`) names it `RUSTTY_FOCUS_MAX_VALUE`, and the cited identifier exists nowhere in the tree. The comment is load-bearing for the D11 rationale (why the C parameter is `c_int`). No behavioural effect.

**Evidence.** `include/rustty/vt/focus.h:43` `RUSTTY_FOCUS_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE,` vs `crates/rustty-vt-c/src/focus.rs:48-49` "itself declares `RUSTTY_FOCUS_EVENT_MAX_VALUE`".

**Suggested fix.** Fix the comment to `RUSTTY_FOCUS_MAX_VALUE`.

#### S4-F3 — search_new terminal-registry insertion has no OUT_OF_MEMORY error path

- **Status:** ⚠️ DISPUTED
- **Kind:** missing-error-path · **Severity:** low
- **Oracle:** `include/ghostty/vt/search.h:340-362` (`ghostty_search_new`: "GHOSTTY_OUT_OF_MEMORY if allocation fails"), `src/terminal/c/search.zig:129-136`
- **Rustty:** `crates/rustty-vt-c/src/search.rs:374-379`

**Description.** The oracle registers the search in the terminal's search set with `terminal_wrapper.searches.putNoClobber(gpa, wrapper, {}) catch { alloc.destroy(wrapper); return .out_of_memory; }`, so a failed registry allocation frees the wrapper and returns `GHOSTTY_OUT_OF_MEMORY` as documented. rustty does `let inserted = (*terminal_wrapper).searches.insert(wrapper); debug_assert!(inserted);` — a std `HashSet` insert has no fallible path (allocation failure aborts the process), so that documented error return cannot occur and the wrapper is not released on that failure.

**Evidence.** `ghostty src/terminal/c/search.zig:129-136` vs rustty `crates/rustty-vt-c/src/search.rs:374-376` `let inserted = unsafe { (*terminal_wrapper).searches.insert(wrapper) }; debug_assert!(inserted);`.

**Suggested fix.** Use a fallible insert (`try_reserve(1)` before insert, or an `allocator_api2`-backed set) and on failure destroy the wrapper through the caller allocator and return `Result::OutOfMemory`.

> **Refuting lens: presence** (confidence medium) — verbatim:
> "Read both sides. Oracle c/search.zig:140-149 `terminal_wrapper.searches.putNoClobber(terminal_wrapper.terminal.gpa(), wrapper, {}) catch { alloc.destroy(wrapper); return .out_of_memory; }`. rustty search.rs:387-405 carries the oracle text commented out plus an explicit PORT NOTE: 'putNoClobber asserts the key is absent (a fresh heap ptr is always unique); HashSet::insert returns true for new keys. The reference catch arm is the map's own allocation failing — a Rust HashSet grows through the global allocator and aborts on OOM, so that arm has no reachable analog (same as tracked_grid_refs in terminal_c::grid_ref_track).' The no-clobber assertion IS preserved (`debug_assert!(inserted)`). The only divergence is the OOM arm, which is unobservable by construction in Rust's allocation model (the process aborts before any return value can be produced), so no C caller can distinguish the two implementations; it is also the same root cause as S4-F2 (registry allocation not on the C allocator) rather than an independent defect. The claim 'the wrapper is not released on that failure' is moot for the same reason. Consistent with the existing grid_ref_track precedent elsewhere in the same crate. Note the container swap ArrayHashMap->HashSet is also unobservable: the only iteration site (terminal.rs:4457) sets `search.terminal = null` for each entry, which is order-independent."

#### S4-F4 — GRAPHEMES_UTF8 error ordering differs for surrogate codepoints (out.len / OUT_OF_SPACE vs INVALID_VALUE)

- **Status:** ⚠️ DISPUTED
- **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `include/ghostty/vt/render.h:718-730`; `src/terminal/c/render.zig:823-870`
- **Rustty:** `crates/rustty-vt-c/src/render.rs:2132-2190`

**Description.** Oracle `rowCellsGetGraphemesUtf8` computes `needed` with `std.unicode.utf8CodepointSequenceLength`, which accepts surrogates (0xD800-0xDFFF) and only rejects >0x10FFFF; it then sets `out.len = needed`, returns OUT_OF_SPACE if `ptr` is NULL or `cap < needed`, and only fails with INVALID_VALUE when `utf8Encode` rejects the surrogate. rustty uses `char::from_u32` in the length pass, which rejects surrogates immediately, returning INVALID_VALUE with `out.len = 0` before any OUT_OF_SPACE / required-size reporting. For a cell holding a surrogate codepoint (not producible by the UTF-8 stream decoder, so effectively unreachable) the result code and reported len differ.

**Evidence.** `ghostty src/terminal/c/render.zig:846-856` `needed = utf8CodepointSequenceLength(first) catch return .invalid_value; ... out.len = needed; if (out.ptr == null or out.cap < needed) return .out_of_space;` then `utf8Encode(...) catch return .invalid_value` vs rustty `render.rs:2158-2167` `let cp0 = match char::from_u32(first) { Some(c) => c, None => return Result::InvalidValue };` before `out.len = needed`.

**Suggested fix.** Compute `needed` from the codepoint range only (1/2/3/4 bytes by value, error only for > 0x10FFFF), set `out.len` and do the OUT_OF_SPACE check first, and let the encode step reject surrogates — matching the Zig ordering.

> **Refuting lens: semantic** (confidence medium) — verbatim:
> "The mechanical difference is described correctly — oracle src/terminal/c/render.zig:851-860 computes `needed` with std.unicode.utf8CodepointSequenceLength (which only errors on CodepointTooLarge, not on surrogates), sets `out.len = needed`, does the OUT_OF_SPACE check, and only hits .invalid_value at utf8Encode; rustty crates/rustty-vt-c/src/render.rs:2173-2187 uses `char::from_u32` first, so a surrogate returns InvalidValue with out.len still 0. But the divergence is confined to a cell holding 0xD800-0xDFFF, which no write path can produce: cell codepoints arrive only through the stream's UTF-8 decoder (surrogates rejected there) and there is no C API that writes an arbitrary codepoint into a cell, so the state is unreachable. For >0x10FFFF the two agree (both InvalidValue with out.len = 0, since the oracle zeroes out.len before the length pass). Per the lens rule (difference confined to unreachable code) this is refuted; the finder itself grades it 'effectively unreachable'."

#### S5-F1 — sgr_set_params over-length-separators warning bypasses the sys log callback (unconditional eprintln!)

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `include/ghostty/vt/sys.h:175-193, 213-233`; `src/terminal/c/sgr.zig:9,69`
- **Rustty:** `crates/rustty-vt-c/src/sgr.rs:580-592`

**Description.** `sys.h` documents that internal library log messages are delivered to the `GHOSTTY_SYS_OPT_LOG` callback when set and are silently discarded when cleared. The oracle's `ghostty_sgr_set_params` emits its "separators length {} exceeds max supported length {}" warning via `log.warn` on the scoped `.sgr` sink, routed through that callback (`lib_vt.zig:459` sets `std_options.logFn`). The rustty port emits it with `eprintln!`, so the line goes straight to stderr regardless of whether a log callback is installed (or cleared), never reaches an installed callback, and carries no `sgr` scope. Every other invalid-argument warning in these modules (selection adjust/ordered, gesture event_set/get) correctly uses `sys::log_fn`.

**Evidence.** `ghostty src/terminal/c/sgr.zig:9` `const log = std.log.scoped(.sgr);` and `:69` `log.warn("ghostty_sgr_set_params: separators length {} exceeds max supported length {}", ...)` vs rustty `crates/rustty-vt-c/src/sgr.rs:586` `eprintln!("rustty_sgr_set_params: separators length {len} exceeds max supported length {SEP_BIT_LENGTH}")`. Contrast `lib.rs:3452-3456` which uses `sys::log_fn(sys::StdLevel::Warn, selection::LOG_SCOPE, ...)`.

**Suggested fix.** Replace the `eprintln!` with `crate::sys::log_fn(sys::StdLevel::Warn, b"sgr", format_args!(...))` (define an `sgr::LOG_SCOPE` like the selection modules).

#### S5-F2 — rustty_sys_log_stderr lossily re-encodes non-UTF-8 scope/message bytes

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `include/ghostty/vt/sys.h:213-233`; `src/terminal/c/sys.zig:263-266`
- **Rustty:** `crates/rustty-vt-c/src/sys.rs:660-682`

**Description.** The oracle `logStderr` writes the scope and message byte slices verbatim into the `[level](scope): message\n` line. The port formats them through `String::from_utf8_lossy`, so any non-UTF-8 byte sequence in scope or message is replaced by U+FFFD before reaching stderr. For valid UTF-8 (the documented case) output is byte-identical; for an embedder that passes raw non-UTF-8 bytes to the built-in callback the stderr bytes differ.

**Evidence.** `ghostty src/terminal/c/sys.zig:263-266` `std.fmt.bufPrint(&buffer, "[{s}]({s}): {s}\n", .{ level_text, scope, message })` (raw bytes) vs rustty `crates/rustty-vt-c/src/sys.rs:664-681` `writeln!(lock, "[{}]({}): {}", level_text, String::from_utf8_lossy(scope), String::from_utf8_lossy(message))`.

**Suggested fix.** Write the pieces as raw bytes (`write_all(b"[")`, level_text, `b"]("`, scope, `b"): "`, message, `b"\n"`, or build a `Vec<u8>` and `write_all` once) instead of `from_utf8_lossy`.

#### S7-F1 — key_encoder_setopt invalid-option/OptionAsAlt warnings bypass the C-ABI log sink

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/terminal/c/key_encode.zig:77-125` (use-site: `include/ghostty/vt/key/encoder.h`)
- **Rustty:** `crates/rustty-vt-c/src/key_encode.rs:164-245` (specifically `:173` and `:241`)

**Description.** The oracle's `setopt`/`setoptTyped` guards emit `log.warn("setopt invalid option value={d}")` and `log.warn("setopt invalid OptionAsAlt value={d}")` through `std.log`, which `lib_vt.zig` routes to `terminal/c/sys.zig logFn` (level/scope-prefixed, chunked C-ABI dispatcher; a no-op on non-debug wasm). The rustty port of the same two guards uses bare `eprintln!`, whereas the sibling key_event guards in `lib.rs:632-636` and `:666-670` correctly go through `sys::log_fn(StdLevel::Warn, ...)`. The observable difference is the log destination/format (no `warning(key_encode):` prefix; message lost to an installed callback; still printed on release wasm where the reference is silent). Primary ownership is the `key_encode.zig` shard; the same pattern exists outside this shard in `mouse_encode.rs:320-406`, `osc.rs:235`, `sgr.rs:586`.

**Evidence.** Oracle `key_encode.zig:77-96` `log.warn("setopt invalid option value={d}", ...)` routed via `lib_vt` `std_options.logFn` → `sys.zig:203 logFn`; rustty `key_encode.rs:171-174` `eprintln!("setopt invalid option value={option}")` and `:239-242` `eprintln!("setopt invalid OptionAsAlt value={raw}")`; contrast `lib.rs:632-636` using `sys::log_fn(sys::StdLevel::Warn, "key_event", ...)`.

**Suggested fix.** Replace the two `eprintln!` calls with `sys::log_fn(sys::StdLevel::Warn, "key_encode", format_args!(...))` as `lib.rs` does for key_event.

## Performance-only differences (not findings)

- `include/ghostty.h` is declarations only — no algorithmic or data-structure differences apply. Rust-idiom equivalents observed and not reported: nullable `close_surface` as `Option<extern fn>`; non-exhaustive `enum(c_int)` `GotoTab`/`ColorKind` as `#[repr(transparent)]` i32 newtypes with named consts; `ClipboardRequestType`/`ColorScheme`/`Clipboard` as `#[repr(u8)]` mirroring the Zig `enum(u8)`/`enum(u2)` backings; const-ness differences on pointer fields (`char *klass` vs `*const u8`).
- `rustty_color_scheme_report_encode` (`color_scheme.rs:25-53`) encodes into a temporary `Vec` sized `MAX_COLOR_SCHEME_REPORT_ENCODE_SIZE` and memcpy's into the caller buffer instead of writing through a fixed writer; bytes written, OUT_OF_SPACE semantics and required-size reporting are identical.
- `rustty_build_info(OPTIMIZE)` derives `OptimizeMode` from cargo profile properties (`build_info.rs:149-197`) rather than `builtin.mode`; same four enumerators with the same meanings — a build-system replacement.
- `rustty_focus_encode` / `rustty_color_scheme_report_encode` / `rustty_tracked_grid_ref_point` narrow the inbound `c_int` enum and return INVALID_VALUE for out-of-range values where Zig's non-exhaustive `enum(c_int)` would pass it through; identical for every declared enumerator.
- `rustty_kitty_graphics_get` / `_image_get` / `_placement_get` / `_placement_iterator_set` narrow the selector via `from_c_int` before dispatch; observable INVALID_VALUE behaviour is the same and it only avoids Rust UB on an invalid discriminant.
- Kitty placement iterator, image and terminal handles are opaque pointers in both trees; rustty wraps them in `PlacementIteratorWrapper`/`OscParserWrapper` with identical external semantics.
- `render.rs` `RowIteratorWrapper` stores one `*mut [RenderRow]` slice instead of the oracle's four MultiArrayList field slices; `RowCellsWrapper` stores a `*const CellList`. Same observable behaviour, different memory layout.
- Rust enum range checks (`Data`/`RowData`/`RowCellsData`/`SearchData`/`SearchOption::from_c_int`, `lib.rs:832-852`, `key_encode.rs:164-175`, `lib.rs:3099-3120`) run in every build mode where the oracle guards only under `comptime std.debug.runtime_safety`; same INVALID_VALUE result, strictly more defensive in release.
- `render_state_set` with an out-of-range Dirty payload returns INVALID_VALUE where the oracle assigns the raw enum unchecked (UB in ReleaseFast); defensive, not observable for valid inputs.
- `rustty_size_report_encode` computes the required size on OUT_OF_SPACE with a Rust `CountingWriter` rather than `std.Io.Writer.Discarding`; same result.
- Snapshot/gesture `get_multi` loops read keys as raw `c_int` and range-check via `from_c_int` before dispatching to the single `get` (same guard placement as the oracle's guarded `get`).
- `snapshot.h` doc-only differences in the rustty header: `@see` link retargeted to `crates/rustty-vt/src/snapshot/mod.rs` and a 3-space indentation change on the `rustty_snapshot_encode` continuation line; no declaration changes.
- `rustty_sys_log_stderr` holds the stderr lock and writes with `writeln!` instead of the oracle's fixed stack buffer + piecewise-write fallback; same no-interleaving observable and same bytes for UTF-8 input.
- `terminal.rs:2247-2270` `continuation_alloc` builds the continuation in a `Vec` then copies through the caller's C allocator (extra copy); identical ownership/return contract.
- `terminal.rs:4310-4340` `grid_ref_track` and the clipboard trampolines (`985-1010`, `1098-1110`, `1170-1195`) use the Rust global allocator / plain `Vec`s, so the oracle's alloc-failure arms become unreachable under abort-on-OOM.
- `terminal.rs:3476-3500` `scroll_viewport`: an undeclared tag value returns early (debug_assert in debug) where the oracle's safe builds panic and ReleaseFast is UB; no defined-input divergence.
- `terminal.rs:1500-1530` `new_`: the Zig `std.Io` owner has no Rust analog and is omitted; terminal internals allocate via Global while wrapper/terminal storage and OSC buffers go through the C-resolved allocator (documented F-008 Tier-2 residual).
- `rustty_wasm_*` use the Rust global allocator with a `Layout` of alignment `c_abi::MAX_ALIGNMENT` instead of Zig's `rawAlloc` with the same alignment; alignment guarantee and NULL semantics identical.
- The XTVERSION default string is `librustty` vs `libghostty` (rename rule, not a finding).

## Refuted candidates

Seven candidates were raised and cleared by both verification lenses:

- **S1-F2 — Stale doc claim that action-union interior parity is deferred.** The comment accurately describes which asserts the gate holds; every union arm is present and `ActionCValue`/`ActionC` sizes are pinned. Doc-only, no divergence.
- **S1-F3 — Local `dist/xcframework-headers/rustty.h` is a stale pre-pin embedding header.** The file is gitignored (`.gitignore:89`) and untracked — a local build artifact, not part of the port; `dist/xcframework-headers-RusttyKit/rustty.h` is byte-identical to `include/rustty-embed.h`.
- **S2-F1 — No `ghostty_*` alias symbols for the vt C ABI.** The `ghostty_*`→`rustty_*` rename is the sanctioned R2 transform applied uniformly; the upstream alias layer was itself deleted (`e4cc852`) and the plan says not to add forwarders. Behaviour identical per function.
- **S2-F2 — vt.h umbrella doc drift (extra `@ref`/`@example`, `rustty.sh` URL).** All differences are additive doc comments describing an example that exists in both trees; nothing from the oracle is missing, and `rustty.sh` is the project's own identity.
- **S3-F2 — No `ghostty_*` symbol/macro aliases for the vt C API (kitty_graphics/modes/mouse/osc/paste/point).** Same known, locked project-wide fork as S2-F1; the six headers are byte-identical to the oracle under the rename.
- **S5-F3 — No `ghostty_*` names/aliases for the libvt C API (selection/sgr/size_report/snapshot/style/sys).** Same locked R2 fork, documented at `rustty-vt-c/src/lib.rs:3-19`; every oracle export is mapped 1:1 in PORT NOTEs.
- **S6-F3 — No `ghostty_terminal_*` alias symbols/macros.** `PORT_RULES.md:33-46` (R2) explicitly makes the published artifacts a clean standalone rather than a ghostty drop-in and drops the identity-equivalence gates; all 18 functions exist under the transformed name.

*(The same theme survives once, as DISPUTED, under S4-F1 — where the semantic lens judged the missing alias layer a real, acknowledged symbol-identity gap.)*
