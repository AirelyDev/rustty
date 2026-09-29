# Phase 7 delta review — R3: lib / C API / include / input / config / termio / os / shell-integration / bench / embedded

Reviewer: R3. Date: 2026-09-28. READ-ONLY review (no Rustty code changed, no builds, oracle
not checked out).

Range: `20abdb50a` (pin, `.port/tools/gates/ORACLE_PIN`) → `12752b2ac` (`origin/main` in
`../ghostty`). Upstream reads: `git -C ../ghostty diff 20abdb50a origin/main -- <path>`,
`git log --no-merges 20abdb50a..origin/main -- <path>`, `git show <sha>`. "End state" below
means `origin/main`. Rustty line numbers are at `079cc21e`.

Area: `src/lib/**` (not `parse_int.zig`), `include/**`, `src/input/**`, `src/config/**`,
`src/termio/**`, `src/os/**`, `src/shell-integration/**`, `src/benchmark/**`,
`src/apprt/embedded.zig`, `example/**`, plus the libghostty-vt C API files
(`src/terminal/c/*.zig`, `src/lib_vt.zig`) whose headers live in `include/`.

---

## 1. Commit census

`git log --no-merges 20abdb50a..origin/main -- src/lib include src/input src/config src/termio
src/os src/shell-integration src/benchmark src/apprt/embedded.zig example ':!src/lib/parse_int.zig'`
lists **32** commits. Two more touch only `src/terminal/c/terminal.zig` (the C API, no header
change): `9dc0d974e` and `105d0a545`. **34 commits in total.** `example/**` has no commits in
the range.

Nine of the 34 are cross-area: the part in my area is small and depends on another reviewer's
half. They are marked "(X)" and the owner of the other half is named.

Area diff (these paths plus `src/terminal/c`, `src/lib_vt.zig`): 42 files, +4,672 / −1,667.
Of that, `src/lib/TinyIo.zig` + `src/lib/tinyio/*` is +2,189 / −1,370, and it is a NON-PORT (§3.1).

## 2. Summary table

Observable: **Y** = PTY bytes, C-ABI results, config/CLI output, or shell behaviour change;
**N** = internal (comments, memory ownership, build). Status is Rustty at `079cc21e`.

| # | commit | theme | observable? | Rustty status | size | risk |
|---|---|---|---|---|---|---|
| 1 | `4406cea3e` | input: ctrl keys with MOK2 (CSI 27 before the C0 conversion) | Y (PTY bytes) | absent | S | low; ordering inside `legacy()` |
| 2 | `636a2f35b` | input: numeric keypad keeps its text under MOK2 (`kpKeys(suffix, normal)`) | Y | absent | S | low |
| 3 | `e7bdda991` | input: F13–F25 legacy/PC-style entries | Y | absent (keys exist in the enum) | S | low |
| 4 | `37e3cdd2d` | input: alt+escape with MOK2 (`\x1b[27;3;27~`) | Y | absent | S | low |
| 5 | `cc3fd8a77` | input: Help / ContextMenu keys (`CSI 28~` / `CSI 29~`) | Y | absent (keys exist) | S | low |
| 6 | `587e08f3f` | input: non-ASCII alt prefix as full UTF-8 | Y | absent; **contradicted by a Rustty-only test** | S–M | a Rustty-only guard test asserts the old raw byte |
| 7 | `3b8141fbd` | os/open: consume newline when draining opener stderr | Y (log, CPU, zombie) | **present as R13 fix UD200, but a different fix** | S | re-alignment decision (Q1) |
| 8 | `5909690de` | config: rebuild RepeatableCommand C mirror on clone | Y (embedder UAF) | behaviour present (ABI fix S1-F1/S103-F1); comments + test absent | S | PORT NOTE rationale is now wrong |
| 9 | `2a987d59d` (X) | config: move `vt-window-resize-allowed` next to `vt-kam-allowed` | Y (docs/show-config order) | absent (the option does not exist) | S | depends on #10 |
| 10 | `714fe9b90` (X) | CSI 8 t window resize: config option, `include/ghostty.h` action, termio dispatch | Y | absent | S (my half) | embed.h A3/A4; core + stream owners |
| 11 | `219cca932` (X) | gtk CSI 8 t; Config doc text for the option | Y (docs) | absent | S (my half) | GTK half is R2 |
| 12 | `1a8f331f1` (X) | macOS move_tab_to_new_window; `Binding.zig` doc lines removed | Y (help text) | absent | S | Swift half is the macOS reviewer's |
| 13 | `fe9cf6a26` (X) | build: `@cImport` → `@import("posix_c")` in Config/os/desktop/os/passwd/termio/Exec | N | N/A; commented reference lines to refresh | S | none |
| 14 | `f6cb8312b` | tinyio: Windows arm; the C API uses TinyIo on every target | N (binary size) | NON-PORT (`.port/phase-4/NON_PORTS.md:8`); ledger + comment refresh | S | audit gate sees 23 unexplained tests unless the NON_PORTS row names the new files |
| 15 | `60b43068c` (X) | os/windows: `DiscardVirtualMemory` + `ERROR_SUCCESS` for page reclaim | Y on Windows (RSS) | absent | S (my half) | Windows VM; caller is `terminal/mem.zig` (vt owner) |
| 16 | `dd2edd760` | libvt: NULL / real-storage pointers for empty output | Y (C ABI results) | absent; **2 Rustty-only tests assert the old non-NULL sentinel** | M | ABI result change; Miri; Rust empty slices are dangling too |
| 17 | `b1c264163` | vt: reject search ticks after the terminal is freed | Y (UAF → INVALID_VALUE) | absent; Rustty carries the same UAF by reading | S | Miri; header doc change |
| 18 | `105d0a545` | C terminal wrapper: kitty temp dir path heap-allocated only when set | N | structurally equivalent (Rustty owns the bytes); comment refresh | S | none |
| 19 | `78494bad8` | C API: `GHOSTTY_TERMINAL_DATA_MOUSE_SHAPE = 41` + `GhosttyMouseShape` | Y (new C data key) | absent (Terminal field present) | S | A5 arm (c) registry; enum MAX sentinel |
| 20 | `67fb0c940` | headers: mouse shapes moved from `mouse/shape.h` into `mouse.h` | Y (header layout) | absent (land together with #19) | S | net: no `mouse/shape.h` at end state |
| 21 | `e50779498` (X) | C API: render hold effect (mode 2026) callback, OPT 41 | Y | absent | M | depends on `stream_terminal.zig` +160 (vt owner); trampoline seam D3 |
| 22 | `c55f213aa` (X) | C API: `GHOSTTY_TERMINAL_OPT_RESIZE_PULL_SCROLLBACK = 40` | Y | absent | S (my half) | depends on PageList/Screen/Terminal flag (vt owner) |
| 23 | `ca4f719f7` (X) | RenderState overscan (Zig API) + ScreenClone bench modes | Y (bench CLI modes) | absent | M (bench half) | depends on `render.zig` +710 (vt owner) |
| 24 | `2ea1eeae2` (X) | C API: RenderState overscan (option, 2 data keys, 2 row-data keys, 2 structs) | Y | absent | L | depends on #23; +524 in `c/render.zig`, 10 tests |
| 25 | `d1cd56a56` (X) | dynamic palette shares the built-in default; termio + C API callers | Y (OOM result for `color_palette`) | absent | S (my half) | depends on `color.zig` (vt owner) |
| 26 | `3beb6d717` (X) | DECRQM: don't truncate 16-bit mode requests (termio + `c/modes.zig`) | Y (DECRPM reply) | absent (`& 0x7FFF` still there) | S (my half) | depends on `modes.zig` (vt owner) |
| 27 | `9dc0d974e` (X) | DECRQM ANSI form; C test renamed and extended | Y | absent | S (my half: 1 test) | depends on `stream.zig` (vt owner) |
| 28 | `500040d3b` (X) | `parseInt` moved to `src/lib`, re-exported from `lib/main.zig` | N | N/A in my crates; owner of `parse_int` decides | S | R4 layering question (Q5) |
| 29 | `a1bf5b5e1` | embedded: split/split_focus/request_close go through `performBindingAction` | N (same apprt action reached); log text fixes half of UD31 | absent | S | UD31 status update |
| 30 | `5d6615fc4` | bash-preexec 0.7.0 | Y (bash < 4.4) | absent | S | keep the local HISTCONTROL modification |
| 31 | `4dfa44ecd` | bash: precmd uses the passed exit status | Y (OSC 133 exit code) | absent | S | none |
| 32 | `ed7f046ee` | bash: recognise `declare -ax` PROMPT_COMMAND arrays | Y | absent | S | none |
| 33 | `591ccacbd` | bash: preserve exit status across scalar prompt commands | Y | absent | S | rename-transform new identifiers |
| 34 | `a4031c8e3` | bash: detect our hook inside PROMPT_COMMAND arrays | Y | absent | S | none |

---

## 3. Per-theme sections

### 3.1 `src/lib/**`: TinyIo Windows arm (`f6cb8312b`), `lib/types.zig`, `lib/main.zig`

**What changed.** `src/lib/TinyIo.zig` (1,522 at the pin) is split: the POSIX syscalls move to
`src/lib/tinyio/posix.zig` (608, NEW), a new Windows arm `src/lib/tinyio/windows.zig` (745,
NEW, ntdll/kernel32 calls, 2 tests) is added, and the shared tests move to
`src/lib/tinyio/test.zig` (836, NEW, 21 tests; `TinyIo.zig` goes from 21 tests to 0). The C
API's `terminal_c.Io` stops choosing `*std.Io.Threaded` on Windows and becomes a stateless
`lib.TinyIo` on every target (`src/terminal/c/terminal.zig:59` at end state), so `Io.init` is
infallible, `Io.deinit` is gone, and the decode/`fromDecoded`/`new_`/`free` error paths lose
their `io.deinit` calls (`src/terminal/c/snapshot.zig` `decoderReadyTerminal`). `src/os/windows.zig`
gains the constants and `ntdll`/`kernel32` externs TinyIo uses. `lib_vt.zig` only changes a
doc sentence ("110KB of binary size on macOS and 370KB on Windows"). Why: binary size (the
commit measures −382 KB on the ReleaseFast DLL) and ~1 KB per terminal.

**Rustty.** TinyIo is a CONFIRMED NON-PORT: `.port/phase-4/NON_PORTS.md:8`. Its reopen trigger
("a future upstream consumer threads `std.Io` into an API whose Rust port genuinely needs
pluggable blocking IO") does **not** fire: the commit changes which `Io` the existing
consumers get, and adds no consumer. The C tier has no `Io` type
(`crates/rustty-vt-c/src/terminal.rs:119-166` carries the reference `Io` struct as commented
text and a PORT NOTE; `crates/rustty-vt-c/src/io.rs:18-20`; `crates/rustty-vt-c/src/snapshot.rs:8`).

**Status: N/A in code; bookkeeping required.**
- `.port/tools/audit_test_coverage.py:302-316` builds its non-port set from
  `` `src/[\w/]+\.zig` `` spans in every `NON_PORTS.md`. The row names only
  `src/lib/TinyIo.zig`, so after the advance `src/lib/tinyio/test.zig` (21) and
  `src/lib/tinyio/windows.zig` (2) are 23 unexplained test blocks. Extend the row to name
  `src/lib/tinyio/posix.zig`, `windows.zig` and `test.zig`, with a line recording that the
  reopen trigger was re-read for `f6cb8312b` and did not fire.
- Refresh the commented reference `Io` block in `terminal.rs:119-166` and its PORT NOTE
  ("TinyIo on POSIX, `std.Io.Threaded` on Windows" is no longer the reference), the
  `lib_vt.zig` doc in `crates/rustty-vt/src/lib.rs:58-71`, and the snapshot PORT NOTE.
- `src/os/windows.zig`'s additions for TinyIo (`DELETE`, `FILE_NAME_NORMALIZED`, `FILE_OPEN`,
  `FILE_OPEN_REPARSE_POINT`, `FILE_SHARE_*`, `FILE_SYNCHRONOUS_IO_NONALERT`, `SYNCHRONIZE`,
  `VOLUME_NAME_DOS`, `IOCTL_KSEC_GEN_RANDOM`, the `FILE_*_INFORMATION` types, `GetFinalPathNameByHandleW`,
  `NtOpenFile`/`NtClose`/`NtReadFile`/`NtQueryInformationFile`/`NtSetInformationFile`/
  `NtDeviceIoControlFile`/`RtlGetFullPathName_U`/`RtlWaitOnAddress`/`RtlWakeAddress*`) have no
  Rustty caller. `crates/rustty-os/src/windows.rs:14-22` already has a carve-out for
  "constants and externs upstream added for symbols no rustty code calls". Their new doc
  comments (e.g. "IOCTL that fills the output buffer with bytes from the kernel CSPRNG…",
  "The futex primitives that kernel32's WaitOnAddress…") will count as G3b missing unless
  declared in `.port/parity-closure/COMMENT-NOT-CARRIED.tsv`.
- No Windows VM work: nothing is ported.

**`src/lib/types.zig`** (`dd2edd760`): see §3.5. **`src/lib/main.zig`** (`500040d3b`): one
line, `pub const parseInt = @import("parse_int.zig").parseInt;`. Rustty's parser lives in
`crates/rustty-vt/src/protocol_int.rs`; whether it moves to `rustty-lib` is the parse_int
owner's call (Q5).

### 3.2 `src/input/**`: legacy encoder series (`4406cea3e`, `636a2f35b`, `e7bdda991`, `37e3cdd2d`, `cc3fd8a77`, `587e08f3f`) and `1a8f331f1`

All six change PTY bytes for real key presses, and through `crates/rustty-vt-c/src/key_encode.rs`
also the libghostty-vt C/wasm key encoder. Rustty home: `crates/rustty-vt/src/input/key_encode.rs`
(4,095 lines, 96 tests) and `crates/rustty-vt/src/input/function_keys.rs`. None of the nine new
test names exists anywhere in `crates/` (grep, 0 hits each).

1. **`4406cea3e` ctrl + MOK2.** The `ctrlSeq` C0 block and the `utf8.len == 0` alt-prefix
   block move *after* the `modify_other_keys_state_2` block (`src/input/key_encode.zig:383`), so
   ctrl+p under MOK2 sends `\x1b[27;5;112~` instead of `\x10`. Tests "legacy: ctrl+char with
   modify other state 2", "…without modify other state 2". Rustty's MOK2 block is at
   `key_encode.rs:497`.
2. **`636a2f35b` keypad under MOK2.** `kpKeys(suffix, normal)` appends an
   `Entry{ .keypad = .normal, .sequence = normal }` per key (`function_keys.zig:276`), so keypad
   1 sends `1` rather than falling into the CSI 27 path. Rustty `kp_keys(suffix)` at
   `function_keys.rs:851` takes one argument. Test "legacy: keypad 1 with modify other state 2".
3. **`e7bdda991` F13–F25.** Thirteen `pcStyle` rows (`function_keys.zig:107-119`: `CSI 25~`,
   26, 28, 29, 31–34, 42–46) and the TODO comment changes to "f26-f35". `Key::F13..F25` exist
   (`crates/rustty-vt/src/input/key.rs:336-348`). Test "legacy: f13 through f25".
4. **`37e3cdd2d` alt+escape under MOK2.** The escape row splits into `.set` → `\x1b\x1b` and
   `.set_other` → `\x1b[27;3;27~` (`function_keys.zig:242-243`). Test "legacy: alt+escape with
   modify other state 2".
5. **`cc3fd8a77` Help/ContextMenu.** `help` → `CSI 28~`, `context_menu` → `CSI 29~`
   (`function_keys.zig:91-92`). Keys exist (`key.rs:242,259`). Test "legacy: help and context menu".
6. **`587e08f3f` non-ASCII alt prefix.** `legacyAltPrefix` now takes the writer, returns
   `bool`, writes `ESC` + the whole UTF-8 text (or the UTF-8 encoding of `unshifted_codepoint`,
   which on macOS takes precedence when non-zero) instead of truncating to one byte
   (`key_encode.zig:552`). A surrogate/invalid codepoint makes `utf8Encode` fail →
   `return false` (safe; in Rust, `char::from_u32` → `None`). Three tests: "legacy: alt+unicode",
   "…only unshifted", "…without unshifted".
   **Conflict:** the Rustty-only test `legacy_alt_prefix_high_byte_is_written_raw`
   (`key_encode.rs:3040-3080`, guarding port bug S104-F1) asserts that an alt press with no
   UTF-8 and `unshifted_codepoint = 0xE9` writes `\x1B\xE9`. At end state the reference writes
   `\x1B\xC3\xA9`. Its first half must be rewritten to the new behaviour; the second half
   (single byte `utf8 = [0xE9]` written raw) still holds (`utf8.len == 1` branch writes `utf8`
   verbatim).
7. **`1a8f331f1`** deletes the two doc lines "Only implemented on Linux, but there's a native
   tab menu provided by macOS." from `move_tab_to_new_window`. Rustty still has them at
   `crates/rustty-input/src/binding.rs:981-985` (the only copy in the tree). The Swift handler
   is the macOS reviewer's.

Ordering: apply 1→6 in upstream order. Each moves code the next one edits. Size: **M** for
the series (≈+360/−90 reference lines, 9 tests). Risks: the Rust `function_keys` table is built
at runtime (`Vec<Entry>`), so the new `normal` entries need the same match-order semantics as
the Zig comptime table (R8). Watch the macOS `cfg!` arm in `legacy_alt_prefix`.

### 3.3 `src/config/**`

- **`5909690de` RepeatableCommand clone.** Upstream found that `clone` copied `value_c`
  (C structs whose string pointers referenced the source config), a use-after-free for an
  embedder that frees the old config after `ghostty_config_clone`. It now rebuilds `value_c`
  via `cval` (`src/config/Config.zig:8936`) and adds test "RepeatableCommand clone rebuilds the C
  mirror" (`:9037`). **Rustty already rebuilds** (`crates/rustty-config/src/value_types.rs:6024-6047`,
  `clone_in`, ABI FIX S1-F1/S103-F1), so there is no behaviour change. Still to do: port the
  four new comment lines and the test (the test must assert that the clone's `title` pointer
  differs from the source's). The existing PORT NOTE claims the reference was "safe there
  because Zig's `Command.C` strings are independently heap-owned" (`value_types.rs:6025-6027`).
  The upstream commit says otherwise, so the note must be corrected. No UD/N entry exists for
  this (grep of both ledgers: 0 hits), so there is no UD to close. Size S.
- **`vt-window-resize-allowed`** (`714fe9b90` adds it, `219cca932` edits its doc, `2a987d59d`
  moves it). End state: the field and its 16-line doc sit right after `vt-kam-allowed`
  (`Config.zig:3055-3071`). Rustty has neither the key nor the field (grep of `crates/`: 0).
  Field order is observable (`+show-config --docs`, docs generation). Rustty's
  `vt-kam-allowed` rows are at `crates/rustty-config/src/config_struct.rs:538,4040,8636`.
  G9 `check_parity_surface.py` reads `.port/phase-0/inventory/parity-surface.json`, so the
  inventory must be regenerated at the new pin to see the new key. Size S, **but** the option
  is dead without the CSI 8 t core/stream/apprt work (§3.8).
- **`fe9cf6a26`**: `const c = @import("posix_c")` replaces `@cImport(unistd.h)` in the
  reference. Rustty mirrors: `config_struct.rs:107` (commented), `rustty-os/src/desktop.rs:19`,
  `passwd.rs:29`, `rustty-termio/src/exec.rs:2086-2089`. Refresh only. S.

### 3.4 `src/os/**`, `src/termio/**`

- **`3b8141fbd` os/open (UD200 fixed upstream).** Upstream switches to
  `takeDelimiterInclusive('\n')` and trims the `\n` for the log (`src/os/open.zig:93`).
  Rustty fixed the same hang under R13 (`crates/rustty-os/src/open.rs:266-305`
  `log_stderr_lines`; UD200 at `.port/parity-closure/UPSTREAM-DEFECTS.md:1918`;
  `docs/ghostty-bug-fixes.md:38`). **The two fixes differ at end of stream:** zig 0.16
  `peekDelimiterInclusive` (`/opt/homebrew/Cellar/zig/0.16.0_1/lib/zig/std/Io/Reader.zig:819-851`)
  propagates `error.EndOfStream` from `fillMore`, and the loop `break`s, so an unterminated last
  line is **dropped** upstream. Rustty's loop logs the pending bytes at EOF
  (`open.rs:279-283`, modelled on `takeDelimiterExclusive`). By reading, the 256-byte
  `StreamTooLong` piece behaviour matches in both. Decision needed (Q1). In both cases, update
  UD200's `upstream status` to "fixed upstream 3b8141fbd", and either delete the
  `ghostty-bug-fixes.md` row (Rustty now matches) or reword it for the remaining difference.
  Tests `open_thread_tests::*` stay. S.
- **`60b43068c` os/windows (X).** `ERROR_SUCCESS` and `kernel32.DiscardVirtualMemory`
  (`src/os/windows.zig:243`), used by `terminal/mem.zig` and `PageList.zig` for Windows page
  reclaim. When the vt owner ports `mem.zig`, the externs become live and must be added to
  `crates/rustty-os/src/windows.rs` (the WI-5 "live caller" precedent at `:24-31`). Needs the
  Windows VM (`ssh rwin`). S.
- **`d1cd56a56` termio + C (X).** `Termio.init` sets `.palette = .default` and then calls
  `try term.colors.palette.changeDefault(alloc, opts.config.palette)` (`src/termio/Termio.zig:283`).
  `changeConfig` logs and falls back to `resetDefault` on OOM. The C API `color_palette` set
  returns `.out_of_memory` on failure, and `color_palette_default` reads `original` through a
  pointer. All of this depends on the new `DynamicPalette` API in `src/terminal/color.zig`
  (vt owner). S.
- **`3beb6d717` termio + C (X).** `requestModeUnknown` passes the full `u16`
  (`src/termio/stream_handler.zig:586`). Rustty still masks: `rustty-termio/src/stream_handler.rs:896-901`
  (`mode_raw & 0x7FFF // @truncate`). `c/modes.zig` `report_encode` builds the new report tag
  from `value`/`ansi` (Rustty `crates/rustty-vt-c/src/modes.rs:62-69`). Depends on the vt
  owner's `modes.zig` `Report.tag` type change. S.
- **`714fe9b90` termio (X)**: `.resize_window => self.surfaceMessageWriter(.{ .resize_window = value })`
  (`stream_handler.zig:307`) and the `apprt/surface.zig` `Message.resize_window`. Depends on
  the stream owner's `StreamAction.ResizeWindow`. S.
- **`fe9cf6a26`**: `termio/Exec.zig` and `os/desktop.zig`/`os/passwd.zig` cImport lines, as
  in §3.3.

### 3.5 libghostty-vt C API, independent of vt-core (`dd2edd760`, `b1c264163`, `105d0a545`, `78494bad8` + `67fb0c940`)

- **`dd2edd760` safe pointers for empty output (M).** Reason: Zig empty slices can have
  sentinel addresses (0x1). Go's stack scanner rejects them even at length 0. Changes:
  `lib.String.init` maps an empty slice to `""` (real storage, `src/lib/types.zig:11`, +test
  "String.init empty output"); `ghostty_alloc(…, 0)` returns NULL
  (`src/terminal/c/allocator.zig:16`; test "alloc zero length" gains `expectEqual(null, ptr)`);
  `format_alloc`, `selection_format_alloc` and `continuation_alloc` return NULL/0 for empty
  output (+3 tests: "format_alloc empty output", "selection_format_alloc empty output", the
  extended continuation test); the OSC 5522 clipboard mime/data, desktop notification
  title/body, APC content and `kitty_image_medium_temp_file` get go through `String.init`
  (+test "kitty_image_medium_temp_file empty output"); `sys.zig` `decodePngWrapper` passes
  `lib.String.init(data).ptr`. Headers: `allocator.h`, `formatter.h`, `selection.h`,
  `terminal.h` (continuation doc) and `types.h` ("Empty strings produced by the library have a
  non-NULL pointer to valid storage").
  **Rustty has the same hazard.** `rustty_lib::String::init` is `ptr: zig.as_ptr()`
  (`crates/rustty-lib/src/types.rs:18-23`): an empty `Vec`/slice is `NonNull::dangling()`.
  `allocator::alloc` returns "the non-null aligned sentinel" for 0 (`crates/rustty-vt-c/src/allocator.rs:49-64`).
  **Two Rustty-only tests assert the old contract and must be inverted:**
  `allocator.rs:165-180` `custom_allocator_zero_length_short_circuit` (`assert!(!ptr.is_null())`
  for `alloc(custom, 0)`) and `formatter.rs:985-1022`
  `format_alloc_empty_output_never_reaches_the_c_allocator` (`assert!(!out_ptr.is_null())` at
  `:1011`). Their "never reaches the C allocator" halves stay valid.
  Risk: in Rust, `"".as_ptr()` is **not** guaranteed to be real storage (a zero-length
  constant can be the dangling address). Use a 1-byte `static`, and have the test assert
  equality with that static's address, which is what the reference's `expectEqual("", str.ptr)`
  means. Needs Miri (`cargo +nightly miri test -p rustty-vt-c`) and the A5 layout arm.
- **`b1c264163` search tick after terminal free (S).** Adds
  `_ = wrapper.zigTerminal() orelse return .invalid_value;` to `tick`
  (`src/terminal/c/search.zig:175`), an assertion in "search free after terminal free", a new
  test "search tick after terminal free with history", and the `search.h` doc. Rustty `tick`
  (`crates/rustty-vt-c/src/search.rs:508-525`) has no terminal check, so by reading it carries
  the reference's use-after-free (a history tick reads a pin tracked in the freed PageList). No
  UD/N records it (ledger grep: 0). Port it, run the new test under Miri, and record in the
  record that upstream found and fixed the defect in-range (it is not an R13 divergence at the
  new pin).
- **`105d0a545` kitty temp dir (S, N).** `tmp_dir_path` becomes `?[]u8` allocated on set and
  freed on clear/free (`src/terminal/c/terminal.zig:81`). Rustty's limits own their bytes
  (`crates/rustty-vt-c/src/terminal.rs:91-110`, `:3155-3190`), so this is structurally
  equivalent. Refresh the commented reference and PORT NOTE. Keep the `MAX_PATH_BYTES`
  `out_of_memory` check, which is still in the reference.
- **`78494bad8` + `67fb0c940` mouse shape (S).** End state: `GhosttyMouseShape` (34 values
  plus `GHOSTTY_MOUSE_SHAPE_MAX_VALUE = GHOSTTY_ENUM_MAX_VALUE`) lives in
  `include/ghostty/vt/mouse.h:64-112`, which now `#include`s `types.h`. The intermediate
  `mouse/shape.h` is created and then deleted, so do not add it. `GHOSTTY_TERMINAL_DATA_MOUSE_SHAPE = 41`
  (`terminal.h:2120`; `c/terminal.zig:1583`) reads `t.mouse_shape`. Registry row
  `.initEnum("GhosttyMouseShape", …)` (`c/types.zig:274`). Tests "get mouse_shape" and
  manifest assertions (`TEXT`=8, `ZOOM_OUT`=33, `MOUSE_SHAPE`=41). Rustty has `Terminal.mouse_shape`
  (`crates/rustty-vt/src/terminal.rs:817`) and `mouse::Shape` (`crates/rustty-vt/src/mouse.rs:70`),
  but no C surface (`include/rustty/vt/mouse.h` has no shape enum; `terminal.h:1953` ends at
  `DATA_…= 40`). The renamed `RUSTTY_MOUSE_SHAPE_MAX_VALUE = RUSTTY_ENUM_MAX_VALUE` satisfies
  the CLAUDE.md sentinel rule as transcribed. Check that the `Shape` discriminants match 0..33
  in declaration order.

### 3.6 libghostty-vt C API, dependent on vt-core (`e50779498`, `c55f213aa`, `2ea1eeae2`, `d1cd56a56`, `3beb6d717`, `9dc0d974e`)

- **`e50779498` render hold (M).** `GhosttyTerminalRenderHoldFn(terminal, userdata, bool held)`,
  `GHOSTTY_TERMINAL_OPT_RENDER_HOLD = 41` (`terminal.h:1705`), a ~120-line doc block, plus
  `render.h` +21. `Effects.render_hold` + `renderHoldTrampoline` (`c/terminal.zig:596`) go
  through `TerminalWrapper.fromHandler`. In Rustty that is the D3 `offset_of!` container_of
  seam (memory `vtc-effects-trampoline-seam-d3`): recover through a raw `*mut TerminalWrapper`
  and do not form a `&mut TerminalWrapper` while the handler borrow is live. `reset` fires
  `held=false` if sync output was on. Test "set render_hold callback". **Depends on**
  `stream_terminal.zig` +160 (the handler-side hold tracking; vt owner). Miri required (FFI
  callback + raw-pointer recovery).
- **`c55f213aa` resize pull scrollback (S my half).** `GHOSTTY_TERMINAL_OPT_RESIZE_PULL_SCROLLBACK = 40`,
  `?*const bool`, NULL → `true` (`c/terminal.zig:1189`, header doc 25 lines). Test "set
  resize pull scrollback". **Depends on** `Terminal.flags.resize_pull_scrollback` and the
  PageList/Screen resize paths (vt owner). Note OPT 40 and 41 both arrive in this range, so
  both must land before either is exposed, or the enum has a hole.
- **`2ea1eeae2` RenderState overscan C API (L).** New `GhosttyRenderStateOverscan {u16 above,
  below}` (size 4) and `GhosttyRenderStateRowId {u64 bits[2]}` (size 16),
  `GHOSTTY_RENDER_STATE_DATA_OVERSCAN = 20`, `…_OVERSCAN_REQUEST = 21`,
  `GHOSTTY_RENDER_STATE_OPTION_OVERSCAN = 1`, `ROW_DATA_VIEWPORT_Y = 6` (i32),
  `ROW_DATA_ID = 7`. The row iterator is re-based on `rowDataRange()` with `pins`/`serials`
  slices and `viewport_y_base = -overscan.above`. `render.h` +207, `c/render.zig` +524, 10
  tests ("render: overscan …", "render: row ids stable across scroll", …), plus
  `c/types.zig` registry rows and the manifest test "manifest describes render state overscan
  types". Rustty home: `crates/rustty-vt-c/src/render.rs` (2,622) + `render/tests.rs`
  (3,194). **Depends on** `ca4f719f7` (`src/terminal/render.zig` +710, vt owner). Both structs
  are passed by out-pointer, not returned by value, so there is no sret hazard. Their layout
  is under A5 `rustty_type_json()`.
- **`d1cd56a56`, `3beb6d717`, `9dc0d974e`**: C-side hunks as in §3.4. `9dc0d974e` renames
  test "set write_pty callback" → "set write_pty callback DECRQM" (Rustty
  `crates/rustty-vt-c/src/terminal.rs:7756-7758`), adds `calls` counting and an ANSI `CSI 4 $ p`
  query, and extends "set write_pty without callback ignores queries". S each.

### 3.7 `src/benchmark/**` (`ca4f719f7`)

`ScreenClone.zig` +161: six new `Mode`s (`render-scroll`, `render-scroll-page`,
`render-output`, `render-{scroll,output,clean}-overscan`; `:69-94`), `bench_overscan =
{above 4, below 1}`, 4,000 scrollback setup lines, and direction reversal intervals. Rustty
`crates/rustty-bench/src/screen_clone.rs`. The mode names are bench-CLI visible. There are no
test changes. G-BENCH (`check_bench_tests.sh:49-57`) reads `origin/main:src/benchmark/cli.zig`,
which is unchanged in range, so its action arm stays GREEN. **Depends on** vt `RenderState`
overscan. Size M.

### 3.8 Embedding C API and `include/ghostty.h` (`a1bf5b5e1`, `714fe9b90` header half)

- **`a1bf5b5e1`.** `ghostty_surface_request_close` now tries
  `performBindingAction(.close_surface)` and falls back to `close()`; `ghostty_surface_split`
  and `…_split_focus` build a `Binding.Action` and call `performBindingAction`
  (`src/apprt/embedded.zig:2166-2215`). By reading, `Surface.performBindingAction` reaches the
  same `rt_app.performAction(.new_split/.goto_split)` and `self.close()`
  (`origin/main:src/Surface.zig:5435-5460, 5598`), so the observable effect is the log text.
  The split_focus message is now "error focusing split err={}", which fixes the embedded half
  of **UD31** (`UPSTREAM-DEFECTS.md:552-568`). Rustty: `crates/rustty-apprt-c/src/lib.rs:2212-2277`,
  with its PORT NOTE preserving the copy-paste. Its `performBindingAction` is
  `crates/rustty-core/src/surface.rs:11163`. Port, then set UD31's embedded sub-site to "fixed
  upstream a1bf5b5e1" (the GTK `setWindowTitle` sub-site is R2's, still open). S.
- **`include/ghostty.h` +8 (`714fe9b90`).** `ghostty_action_resize_window_s {u32 width,
  height}` (`:801-806`), `GHOSTTY_ACTION_RESIZE_WINDOW` appended after
  `GHOSTTY_ACTION_MOVE_TAB_TO_NEW_WINDOW` (`:1018`; Rustty `include/rustty-embed.h:1015`), and
  a `resize_window` union member. This must land in the same change as the apprt-api
  `Action::ResizeWindow` + `Key::ResizeWindow` (the "ghostty.h Action.Key" enum-check test)
  and the core `Surface` CSI 8 t handler (R2/core owner). `rustty_action_s` is passed **by
  value** to the runtime action callback; its freezes are `// @embed-abi rustty_action_u 24` /
  `rustty_action_s 32` (`crates/rustty-apprt-c/tests/abi_layout.rs:209-211`). An 8-byte arm
  does not change the union size, but A4 must still pass with a new `@embed-abi` row for
  `rustty_action_resize_window_s`.

### 3.9 Shell integration (`5d6615fc4`, `4dfa44ecd`, `ed7f046ee`, `591ccacbd`, `a4031c8e3`)

At the pin, Rustty's `crates/rustty-extra/shell-integration/bash/bash-preexec.sh` is
byte-identical to the reference, and `rustty.bash` equals `ghostty.bash` under the
`ghostty→rustty` token rename (verified with `diff`). End-state changes:
- bash-preexec 0.7.0 (+266/−76). Upstream keeps its local `__bp_adjust_histcontrol`
  modification (#2269) with a rewritten comment.
- `ghostty.bash`: `__ghostty_precmd` takes `ret="${1:-$?}"`; new
  `__ghostty_restore_status() { builtin return "$1"; }` (`:228`); `__ghostty_hook` consumes
  `__ghostty_status`; scalar `PROMPT_COMMAND` is prefixed with a status-capture/restore
  snippet; the hook-present guard becomes a substring match; `declare -a` matching allows
  attributes (`-ax`).
Transplant the end-state files through the rename transform. The new identifiers
(`__ghostty_status`, `__ghostty_restore_status`) must become `__rustty_*`, keeping casing per
token (memory `rename-casing-fidelity`). No test impact in Rustty (the reference has no bash
unit tests in range). S total.

### 3.10 Header-parity gates, by reading

- **A5 (`check_rustty_vt_headers.sh`, PROGRESS, ci.sh:1122).** Arm (d) reads exports from
  `origin/main:src/lib_vt.zig` (`:17-19`); `lib_vt.zig` gained **no** exports in range, so arm
  (d) and the `VT_BOTH_FLOOR=203` intersection (`:278`) are unaffected. Every `ghostty_*` token
  added to header prose is a real export (checked: 14 tokens, all present), so the
  "6 non-export tokens" identity is unaffected. **Arm (c)** (`.port/tools/check-vt-struct-layouts.mjs:188-240`)
  reads `origin/main:src/terminal/c/types.zig` and ignores the pin. It should therefore
  **already be RED** now that `origin/main` is at `12752b2ac`: 3 oracle registry keys
  (`RenderStateOverscan`, `RenderStateRowId`, `MouseShape`) are missing on the Rustty side.
  Not run (no builds). It turns green only with §3.5 + §3.6.
- **A3 (`check_rustty_h_abi.sh`)** compares `GHOSTTY_API` function sets of the oracle
  working-tree `include/ghostty.h` against `include/rustty-embed.h`. The range adds no
  function to `ghostty.h`, so A3 is unaffected by function set. The token-renamed header must
  still be updated for byte parity (the enum value + struct + union arm).
- **A4 (`check_embed_h_abi.sh`)**: one new aggregate (`rustty_action_resize_window_s`) and the
  union.
- **(a3) `include/rustty.h` regen**: every new `#[repr(C)]`/enum in rustty-vt-c must be
  regenerated with `cargo run -p xtask -- header-gen`.

### 3.11 Test-count impact (my area)

New reference tests to port by name: input 9; `lib/types.zig` 1; `Config.zig` 1;
`c/render.zig` 10; `c/terminal.zig` 4 new + 1 renamed ("set write_pty callback DECRQM");
`c/types.zig` 1; `c/search.zig` 1; `c/formatter.zig` 1; `c/selection.zig` 1. **30 new + 1
renamed.** Existing tests extended with assertions: "alloc zero length", "search free after
terminal free", the continuation-alloc test, "set write_pty without callback ignores queries",
"manifest uses public enum names". Rustty-only tests to invert or rewrite: 3
(`key_encode.rs:3040-3080`, `allocator.rs:165-180`, `formatter.rs:985-1022`). Non-port tests:
23 (`tinyio/test.zig` 21, `tinyio/windows.zig` 2). G1b's `vt_c` row (globbing `c/*.zig`)
stayed GREEN by slack in Phase 6 (433/487). It will not see these, so the per-test
`audit_test_coverage.py` is the honest meter.

---

## 4. Proposed workstreams

| WS | content | commits | depends on | size |
|---|---|---|---|---|
| R3-A input encoder | the six legacy-encoder commits in order, the `binding.rs` doc lines, rewrite of the S104-F1 guard test | 1–6, 12 | none | M, ~1 day |
| R3-B C API independent | safe pointers (+ invert 2 tests, `static` empty storage, Miri), search tick UAF (Miri), kitty temp dir comments, mouse shape + `mouse.h` + registry, TinyIo comment refresh | 14, 16–20 | none | M, ~1–1.5 days |
| R3-C C API dependent | render hold (D3 seam, Miri), resize-pull option, overscan C API (L), palette/DECRQM C hunks + tests | 21, 22, 24–27 | vt owner: stream_terminal hold, PageList flag, `render.zig` overscan, `color.zig` DynamicPalette, `modes.zig` tag, `stream.zig` DECRQM ANSI | L, ~2.5–3 days |
| R3-D config/os/termio/shell | RepeatableCommand comments + test, cImport comment refresh, os/open re-alignment per Q1, 5 bash commits, termio palette/DECRQM/resize_window dispatch, `DiscardVirtualMemory` externs | 7, 8, 13, 15, 25, 26, 30–34 | termio hunks wait on vt; windows externs wait on vt `mem.zig` | S–M, ~1 day |
| R3-E embedding | `a1bf5b5e1`; `rustty-embed.h` resize_window + A4 row; `vt-window-resize-allowed` field in final position | 9, 10, 11, 29 | core/stream owner: CSI 8 t (`stream.zig`, `Surface.zig`, `apprt/action.zig`) | S, ~0.5 day |
| R3-F bench | ScreenClone overscan/scroll modes | 23 | vt `RenderState` overscan | M, ~0.5 day |
| R3-G ledgers/gates | NON_PORTS TinyIo row → new file names; COMMENT-NOT-CARRIED rows for `os/windows.zig` TinyIo externs; UD200 + UD31 status; `ghostty-bug-fixes.md` UD200 row; parity-surface inventory regen for the new config key; A5 arm (c) back to GREEN | — | after A–F | S, ~0.5 day |

Ordering: A, B and D (except the termio hunks) can start as soon as the pin advances. C and F
follow the vt owner's `render.zig` / `stream_terminal.zig` / `PageList` / `color.zig` /
`modes.zig` ports. E follows the core CSI 8 t port. G closes. Land OPT 40 and 41 (C) together.
Land 19 and 20 together. Run the Linux lane early (Phase-6 lesson). The Windows VM is needed
only for 15 (with the vt owner's `mem.zig`). **Total: about 7–8 agent-days** (A 1, B 1.5, C 3,
D 1, E 0.5, F 0.5, G 0.5).

## 5. Open questions for the maintainer

1. **UD200 re-alignment.** Upstream's fix drops an unterminated final stderr line
   (`takeDelimiterInclusive` → `EndOfStream` → `break`), while Rustty's R13 fix logs it. Should
   Rustty follow the reference's end state exactly (drop the line; retire the
   `ghostty-bug-fixes.md` row), or keep logging the tail as a remaining R13 divergence? If
   kept, is dropping the tail a "non-crash bug that breaks evident intent" (the code's intent
   is to log opener stderr)?
2. **Rustty-only guard tests that contradict the new reference** (`key_encode.rs:3040-3080`,
   `allocator.rs:165-180`, `formatter.rs:985-1022`): rewrite them to the new behaviour, or
   delete and replace them with the reference's new tests? Each guarded a real port bug
   (S104-F1, W36-B06, S171-F1) whose "never call the C allocator for zero bytes" half is
   still valid.
3. **Empty-string storage in Rust.** `lib.String.init` now promises "a non-NULL pointer to
   valid storage" (`types.h`). Is a crate-level `static EMPTY: [u8; 1]` in `rustty-lib`
   acceptable as the R1 rendering of Zig's `""`, given that Rust does not guarantee `"".as_ptr()`
   is real storage?
4. **A5 arm (c) reads `origin/main`**, so it went RED (by reading) the moment `12752b2ac` was
   fetched, before any advance. Is a known-RED A5 acceptable until R3-B/C land, or should the
   pin advance be sequenced first so that the RED is attributed?
5. **`parseInt` in `src/lib`** (`500040d3b`): should Rustty move `protocol_int.rs` from
   `rustty-vt` to `rustty-lib` to mirror the reference (R4 allows it, since lib is below vt),
   or keep it in place with a PORT NOTE? (The parse_int owner decides; noted because it
   touches `lib/main.zig`.)
6. **`b1c264163`** fixes a use-after-free that Rustty carries by reading but never recorded.
   Should it get a retroactive NEW-FINDINGS row, or only a note in the Phase-7 record that it
   was absorbed from upstream?
