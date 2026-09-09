# Parity closure checklist — include

12 open finding(s): 2 high, 4 medium, 6 low. Full evidence per row is in [`docs/parity-audit/include.md`](../../docs/parity-audit/include.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## High (2)

- [ ] **S1-F1** · CONFIRMED · `ABI` · behavioral-difference
  - **What:** rustty_command_s elements handed to C are not the header's 4x const char* layout
  - **Oracle:** `include/ghostty.h:421-426, 508-512` → **Rustty:** `crates/rustty-input/src/command.rs:46-57 (and crates/rustty-config/src/value_types.rs:5943-5973, crates/rustty-config/src/c_get.rs:190-198,306)`
  - **Audit's suggested fix:** Make `rustty_input::command::C` `#[repr(C)]` with four `*const c_char` fields (NUL-terminated), keeping the owned NUL-terminated byte buffers alive alongside (e.g. `RepeatableCommand` owns a `Vec<CommandCStorage>` of the four CStrings and `value_c: Vec<C>` holds the pointers, invalidated/rebuilt together as the reference's arena-owned `[*:0]` strings are). Add `// @embed-abi rustty_command_s 32` + const size/offset asserts, pin it in abi_layout.rs, and extend the value_types.rs cval test to read the fields through `CStr::from_ptr` so the C view is what is asserted.
- [ ] **S3-F1** · CONFIRMED · `ABI` · behavioral-difference
  - **What:** IMAGE_DATA_FORMAT/COMPRESSION write a 1-byte Rust enum into the caller's 4-byte C enum
  - **Oracle:** `include/ghostty/vt/kitty_graphics.h:312-336, 407-411, 422-428 (GhosttyKittyImageFormat / GhosttyKittyImageCompression are `enum GHOSTTY_ENUM_TYPED` = C int; GHOSTTY_KITTY_IMAGE_DATA_FORMAT / _COMPRESSION 'Output type: GhosttyKittyImageFormat *' / 'GhosttyKittyImageCompression *')` → **Rustty:** `crates/rustty-vt-c/src/kitty_graphics.rs:338-344 (type aliases), 540-542 (writes), plus crates/rustty-vt/src/kitty/graphics_command.rs:863-873 and 916-920`
  - **Audit's suggested fix:** Either give `Format`/`Compression` `#[repr(C)]` (they are C-target enums in the oracle) or, if rustty-vt must stay repr-free, convert at the boundary: `*(out as *mut c_int) = image.format as c_int` (and the same for compression) in image_get_typed, and add a test that writes into a `c_int` initialised to -1 and asserts the full value, plus `const _: () = assert!(size_of::<ImageFormat>() == size_of::<c_int>())` if the repr route is taken.

## Medium (4)

- [ ] **S4-F1** · DISPUTED · `INDIV` · other
  - **What:** ghostty_* C symbol identity absent: headers and exports declare only rustty_* names, no alias layer
  - **Oracle:** `include/ghostty/vt/render.h:394-889 (also screen.h 340-419, search.h 340-500)` → **Rustty:** `include/rustty/vt/render.h:394-889 (screen.h 340-419, search.h 340-500); crates/rustty-vt-c/src/lib.rs:2296-2677,1262-1335,2913-3034`
  - **Audit's suggested fix:** Add the planned generated alias layer: either `#[export_name = "ghostty_..."]` twin exports (or a C shim/`.def`/linker alias file) for every rustty_* symbol, plus `#define ghostty_X rustty_X` / `typedef` aliases in the headers so both identities resolve. Workspace-wide item; this shard covers 34 of the symbols.
- [ ] **S4-F2** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** Caller-supplied GhosttyAllocator is not used for render-state internals or the search needle/scrollback state
  - **Oracle:** `include/ghostty/vt/render.h:384-395, 410-463 (allocator param contract for ghostty_render_state_new + update/begin_update); search.h 340-362, 425-445` → **Rustty:** `crates/rustty-vt-c/src/render.rs:631-720 (free/update/begin_update use allocator_api2::alloc::Global); crates/rustty-vt-c/src/search.rs:685-690 (TerminalSearch::init(Global, ..))`
  - **Audit's suggested fix:** Thread an allocator_api2::Allocator adapter over the C ZigAllocator through RenderState::update/begin_update/deinit and TerminalSearch::init (the fix plan already recorded in DEFERRALS.md F-008/W12-01 Tier 2).
- [ ] **S6-F1** · CONFIRMED · `INDIV` · different-constant
  - **What:** librustty-vt terminal_new defaults Kitty image storage to 320MB instead of libghostty-vt's 10MB
  - **Oracle:** `src/terminal/Terminal.zig:289-298 (Options.kitty_image_storage_limit, artifact-selected: .lib => 10*1000*1000); used by src/terminal/c/terminal.zig:864-902 new_ via Options defaults; src/build_config.zig:92 (.Lib => .lib)` → **Rustty:** `crates/rustty-vt-c/src/terminal.rs:1538-1562 (new_ hardcodes kitty_image_storage_limit: 320 * 1000 * 1000); crates/rustty-vt/src/options.rs:32 (ARTIFACT = Artifact::Ghostty unconditionally)`
  - **Audit's suggested fix:** Give rustty-vt-c (the lib artifact) the .lib default: select 10*1000*1000 in new_ (or thread an Artifact::Lib through a rustty-vt feature/const so options::ARTIFACT reflects the lib build), and add the get(KITTY_IMAGE_STORAGE_LIMIT)-after-new default test.
- [ ] **S6-F2** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** Mode 5522 (Kitty paste events) is disabled in librustty-vt on non-macOS targets, unlike libghostty-vt
  - **Oracle:** `src/terminal/modes.zig:334-342 (kitty_paste_events .disabled = build_options.artifact != .lib and builtin.os.tag != .macos); include/ghostty/vt/terminal.h:804-807 and 1512-1520 (clipboard_read enables Kitty paste events / mode 5522)` → **Rustty:** `crates/rustty-vt/src/modes.rs:619, 629-632 (KITTY_PASTE_EVENTS_DISABLED = !matches!(ARTIFACT, Lib) && !cfg!(target_os = "macos")); crates/rustty-vt/src/options.rs:32 (ARTIFACT always Ghostty)`
  - **Audit's suggested fix:** Make the lib artifact real for rustty-vt-c builds (feature-selected options::ARTIFACT = Lib) so the predicate evaluates to false, and add a Linux-target test asserting mode 5522 is settable through OPT_MODE / CSI ? 5522 h.

## Low (6)

- [ ] **S2-F3** · CONFIRMED · `INDIV` · other
  - **What:** Comment cites RUSTTY_FOCUS_EVENT_MAX_VALUE but the header enumerator is RUSTTY_FOCUS_MAX_VALUE
  - **Oracle:** `include/ghostty/vt/focus.h:43` → **Rustty:** `crates/rustty-vt-c/src/focus.rs:48-49`
  - **Audit's suggested fix:** Fix the comment to RUSTTY_FOCUS_MAX_VALUE.
- [ ] **S4-F3** · DISPUTED · `ALLOC` · missing-error-path
  - **What:** search_new terminal-registry insertion has no OUT_OF_MEMORY error path
  - **Oracle:** `include/ghostty/vt/search.h:340-362 (ghostty_search_new: 'GHOSTTY_OUT_OF_MEMORY if allocation fails')` → **Rustty:** `crates/rustty-vt-c/src/search.rs:374-379`
  - **Audit's suggested fix:** Use a fallible insert (try_reserve(1) before insert, or an allocator_api2-backed set) and on failure destroy the wrapper through the caller allocator and return Result::OutOfMemory.
- [ ] **S4-F4** · DISPUTED · `INDIV` · behavioral-difference
  - **What:** GRAPHEMES_UTF8 error ordering differs for surrogate codepoints (out.len / OUT_OF_SPACE vs INVALID_VALUE)
  - **Oracle:** `include/ghostty/vt/render.h:718-730 (GHOSTTY_RENDER_STATE_ROW_CELLS_DATA_GRAPHEMES_UTF8 contract)` → **Rustty:** `crates/rustty-vt-c/src/render.rs:2132-2190`
  - **Audit's suggested fix:** Compute `needed` from the codepoint range only (1/2/3/4 bytes by value, error only for > 0x10FFFF), set out.len and do the OUT_OF_SPACE check first, and let the encode step reject surrogates — matching the Zig ordering.
- [ ] **S5-F1** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** sgr_set_params over-length-separators warning bypasses the sys log callback (unconditional eprintln!)
  - **Oracle:** `include/ghostty/vt/sys.h:175-193, 213-233` → **Rustty:** `crates/rustty-vt-c/src/sgr.rs:580-592`
  - **Audit's suggested fix:** Replace the eprintln! with `crate::sys::log_fn(sys::StdLevel::Warn, b"sgr", format_args!("rustty_sgr_set_params: separators length {len} exceeds max supported length {SEP_BIT_LENGTH}"))` (define an `sgr::LOG_SCOPE` like the selection modules).
- [ ] **S5-F2** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** rustty_sys_log_stderr lossily re-encodes non-UTF-8 scope/message bytes
  - **Oracle:** `include/ghostty/vt/sys.h:213-233` → **Rustty:** `crates/rustty-vt-c/src/sys.rs:660-682`
  - **Audit's suggested fix:** Write the pieces as raw bytes: `lock.write_all(b"[")`, level_text, `b"]("`, scope, `b"): "`, message, `b"\n"` (or build a Vec<u8> and write_all once) instead of from_utf8_lossy.
- [ ] **S7-F1** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** key_encoder_setopt invalid-option/OptionAsAlt warnings bypass the C-ABI log sink (eprintln! instead of sys::log_fn)
  - **Oracle:** `src/terminal/c/key_encode.zig:77-125` → **Rustty:** `crates/rustty-vt-c/src/key_encode.rs:164-245`
  - **Audit's suggested fix:** Replace the two eprintln! calls in key_encode.rs setopt/setopt_typed with `sys::log_fn(sys::StdLevel::Warn, "key_encode", format_args!(...))` as lib.rs does for key_event.
