# Parity audit — src/lib

Area `src/lib` was audited across two shards (110: `src/lib/*`; 111: `src/lib/allocator`, `src/lib/compat/*` plus the `src/os/*` units those compat shims feed). 29 oracle units were fully read — 3,584 lines read, which is the whole of the area in scope for these shards; no unit was left partial. 18 findings were raised: 13 CONFIRMED, 2 DISPUTED, 3 REFUTED, 0 UNVERIFIED. By corrected severity: 0 high, 3 medium, 12 low (refuted candidates excluded). The single most important gap is **S110-F1** — `GlobalZigAllocator::alloc`/`remap` build their `Layout` with `.unwrap()`, so a caller-controlled over-large `len` panics inside an `extern "C"` callback (an abort) where the reference `std.heap.c_allocator` returns null and the C API reports `OUT_OF_MEMORY`.

## Coverage

| Oracle unit | Rustty counterpart(s) | Status | Note |
| --- | --- | --- | --- |
| `src/lib/allocator.zig` | `crates/rustty-lib/src/allocator.rs`, `crates/rustty-vt-c/src/test_util.rs` | ported | VTable/Allocator (`#[repr(C)]`), `from_zig`/`zig` round-trip, the four Zig→C forwarding callbacks (`ZIG_ALLOCATOR_VTABLE`) and C→Zig forwarding (`CAllocatorAsZig`) match arg order and alignment u8 encoding. `default()` comptime arms (libc/wasm/smp) collapse to `GlobalZigAllocator` (Rust global allocator = malloc on macOS/Linux); freestanding→`FailingZigAllocator` kept. `c_allocator` const → `c_allocator()` fn over a static. `test_allocator` intentionally omitted (test-only const; replaced by `rustty-vt-c::test_util::with_test_allocator` + Miri leak checking). Tests: `fba allocator` ported with same assertion; `c allocator` ported but does not call `c_allocator()` (see S110-F3). |
| `src/lib/c_abi.zig` | `crates/rustty-lib/src/c_abi.rs` | ported | Comptime fold over `std.Target.CType` alignments replaced by a per-target constant table (aarch64-apple → 8, else 16); consumers (`allocator/wasm.rs` `WASM_ALIGNMENT`, `vt-c types.rs type_json`) use `MAX_ALIGNMENT`. Table only covers the documented targets (see S110-F4, refuted). |
| `src/lib/enum.zig` | `crates/rustty-lib/src/enum.rs` | ported | `Enum()` modeled as runtime `EnumType` descriptor (tag_type + (name,value) with holes preserved, `IntFittingRange(0, keys.len-1)`); `checkGhosttyHEnum` → `check_ghostty_henum` with caller-supplied decl lookup; used by `rustty-vt/mouse.rs`, `rustty-renderer`, `rustty-apprt-api` tests. All 5 tests ported with identical assertions. |
| `src/lib/main.zig` | `crates/rustty-lib/src/lib.rs` | ported | Re-export hub; `refAllDecls` test → `main_tests::test_anon_0`. `pub const TinyIo` has no counterpart because `src/lib/TinyIo.zig` is a confirmed NON-PORT (`.port/phase-4/NON_PORTS.md`: Zig `std.Io` VTable shim; rustty streaming C APIs use `std::io` / `rustty_vt::snapshot::io` traits via `rustty-vt-c/src/io.rs`; `randomSecure` ported in `rustty-os/src/file.rs:316`). |
| `src/lib/packed.zig` | `crates/rustty-lib/src/packed.rs`, `crates/rustty-vt-c/src/types.rs`, `crates/rustty-vt/src/page.rs` | ported | `Packed`/`PackedTaggedUnion` comptime factories modeled as `'static PackedLayout` / `PackedTaggedUnionLayout` descriptors with const-fn `validate()` panicking at const-eval (compile-error analogue); `fieldOptions`/`fieldName`/`bitOffset`/`bitWidth`/`arm` ported. Test ported with the same assertions (plus an extra `assert_ne`). Two comptime type-identity checks not representable in the model (see S110-F5). |
| `src/lib/struct.zig` | `crates/rustty-lib/src/struct.rs`, `crates/rustty-vt-c/src/render.rs` | ported | `Struct(.c)` → `struct_type` resolving alignment 0 to natural; `sizedFieldFits` → `sized_field_fits` over a `StructType` descriptor (used by vt-c `render.rs` cursor/colors size-gates). All 3 tests ported with identical assertions (offsets computed by the same extern-layout math). |
| `src/lib/target.zig` | `crates/rustty-lib/src/target.rs` | ported | Payload-less `union(enum)` → plain enum `{C, Zig}`. |
| `src/lib/types.zig` | `crates/rustty-lib/src/types.rs` | ported | `String{ptr,len}` + `init(&[u8])`; `Buffer{ptr: null, cap: 0, len: 0}` defaults via `Default` impl; both `#[repr(C)]`. |
| `src/lib/union.zig` | `crates/rustty-lib/src/union.rs` | ported | `TaggedUnion` modeled as descriptor: C-target size check (panic == `@compileError`), `cvalue_size` (extern-union max size/align incl. `_padding`), `c_field_rename`, `has_cvalue_field` (`@hasField`), `cval`, `Value` → `value_size`. Not used outside `rustty-lib` (C unions are hand-written `repr(C)` at use-sites). All 4 tests ported with identical assertions. Nested `Type.C` / `v.cval()` resolution is pre-resolved by the caller (see S110-F6, refuted). |
| `src/lib/allocator/wasm.zig` | `crates/rustty-lib/src/allocator/wasm.rs` | ported | `allocBytes`/`freeBytes`/`allocOpaque`/`freeOpaque`/`takeOpaque` all present with identical null/zero-len branches; both tests ported with the same assertions (alignment check, write/read 42, `allocBytes(0)==null`, `freeBytes(null,0)`; take-null/take-value/take-null again). |
| `src/lib/compat/fd.zig` | `crates/rustty-os/src/pipe.rs`, `crates/rustty-termio/src/exec.rs` | replaced | Zig-0.15.2 std backport (close/pipe/pipe2 with CLOEXEC-via-`F_SETFD` fallback). Use-sites (`os/pipe.zig`, `os/passwd.zig`, `termio/Exec.zig`) are covered by direct `libc::pipe2` on Linux and `libc::pipe` + `fcntl(F_SETFD, FD_CLOEXEC)` on macOS (`pipe.rs:26-65`, `exec.rs:4178-4193`) — the same syscall sequence `pipe2()` performs. |
| `src/lib/compat/file.zig` | `crates/rustty-bench/src/page_compression.rs`, `crates/rustty-termio/src/termio_impl.rs`, `crates/rustty-renderer/src/shadertoy.rs` | replaced | Zig std backport of `File.readToEndAlloc(max_bytes)`. Its one test is ported in `page_compression.rs:847-870` against `read_to_end_limited` with the same two assertions (exact-limit success, limit−1 fails); other use-sites express the byte cap per call (`termio_impl.rs:1970` via `take(max+1)`; `shadertoy.rs:283` 4MB cap). |
| `src/lib/compat/reader.zig` | `crates/rustty-font/src/opentype/{head,hhea,post,glyf,os2}.rs` | replaced | Old-stdlib reader helpers (`readStructEndian`/`readerInt`/`readByte`/`readSkipBytes`…). No tests. Use-sites ported as explicit big-endian field reads (PORT NOTEs at `hhea.rs:98`, `head.rs:162`, `glyf.rs:412`, `post.rs:73`, `os2.rs:18`) with identical byte decoding. |
| `src/lib/compat/testing.zig` | `crates/rustty-vt/src/mouse.rs`, `crates/rustty-apprt-api/src/action_tests.rs` | replaced | `refAllDeclsRecursive` is a comptime Zig-only semantic-analysis helper (no runtime behaviour). Its two use-sites have `ref_all_decls` stand-ins at `mouse.rs:463-469` and `action_tests.rs:279`. |
| `src/lib/compat/thread.zig` | `crates/rustty-datastruct/src/blocking_queue.rs`, `crates/rustty-zigstd/src/thread.rs` | replaced | Backport of `std.Io.Condition.waitTimeout` (futex loop, signal consumption, `Timeout` error). Sole use-site `datastruct/blocking_queue.zig` is covered by `std::sync::Condvar::wait_timeout` at `blocking_queue.rs:173` with the same observable outcome. |
| `src/os/TempDir.zig` | `crates/rustty-os/src/tempdir.rs` | ported | `init` loop on `PathAlreadyExists`, `name()`, `deinit`, `CloseMode{delete,retain}`, `close()` all present. Both tests ported with the same assertions. |
| `src/os/cf_release_thread.zig` | `crates/rustty-os/src/cf_release_thread.rs` | ported | `Message`/`Mailbox(64)`/`init`/`deinit`/`threadMain` (drain-on-abnormal-exit)/`threadMain_`/`drainMailbox`/`wakeupCallback`/`stopCallback` all present against real `rustty-xev`. See S111-F4 on the empty log helpers. |
| `src/os/cgroup.zig` | `crates/rustty-os/src/cgroup.rs` | ported | `current(buf,pid)`: `/proc/<pid>/cgroup` open, read into buf, `lastIndexOf ':'`, `trimEnd " \r\n"`. See S111-F8 on single `read()` vs `readSliceShort`. |
| `src/os/dbus.zig` | `crates/rustty-os/src/dbus.rs` | ported | `launchedByDbusActivation`: linux → both `DBUS_STARTER_ADDRESS` and `DBUS_STARTER_BUS_TYPE` present; else false. Identical. |
| `src/os/desktop.zig` | `crates/rustty-os/src/desktop.rs` | ported | `launchedFromDesktop` and `desktopEnvironment(environ_map)` identical; test "desktop environment" ported with all 6 linux cases + macos/windows cases. |
| `src/os/edit.zig` | `crates/rustty-os/src/edit.rs` | ported | `Options.default_editor {failure,vi,nano}=vi`, `Error.NoEditorConfigured`, `VISUAL` then `EDITOR` via `containsUnempty`+`getAlloc`, three warn messages, editor + ' ' + shell-escaped path, NUL-terminated result. Oracle has no tests; rustty adds 7. |
| `src/os/file.zig` | `crates/rustty-os/src/file.rs`, `crates/rustty-global/src/environ.rs` | ported | rlimit alias, `fixMaxFiles` (binary search, `RLIM_INFINITY` short-circuit, logs), `restoreMaxFiles`, `allocTmpDir`/`freeTmpDir`, `random_basename_len=calcSize(16)` url-safe no-pad base64, `randomBasename(BufferTooSmall)`, `randomTmpPath`, test `randomBasename` with same 3 assertion groups. See S111-F1 and S111-F12. |
| `src/os/flatpak.zig` | `crates/rustty-os/src/flatpak.rs` | ported | `isFlatpak`; `FlatpakHostCommand` (Flags packed `watch_bus`, State init/err/started/exited, Completion, Error set, spawn/wait/waitXev/signal/threadMain/start/updateState/onExit/noopCallback, timer callback via `container_of` on `c_xev`) ported under the `flatpak` feature with identical GIO call sequence and cleanup order. See S111-F2/F3/F9. |
| `src/os/homedir.zig` | `crates/rustty-os/src/homedir.rs` | ported | `home()`/`homeUnix` (HOME map lookup, macOS `NSFileManager.homeDirectoryForCurrentUser.path`, passwd, `/bin/sh -c 'cd && pwd'`), `homeWindows`, `expandHome`/`expandHomeUnix`; both tests ported with the same assertions. See S111-F11. |
| `src/os/hostname.zig` | `crates/rustty-os/src/hostname.rs` | ported | `LocalHostnameValidationError{PermissionDenied,Unexpected}`, `isLocal` (localhost shortcut, `GetComputerNameA` on windows, `gethostname` elsewhere); all 3 tests ported with identical assertions. rustty additionally carries an `isValid()` + test that the pinned oracle no longer has (additive, not a gap). |
| `src/os/i18n.zig` | `crates/rustty-os/src/i18n.rs`, `crates-sys/gettext-sys/src/lib.rs` | ported | `locales_map` membership, `InitError`, `init`, `initGlobalDomain`, `_` → `gettext_` (dgettext), `N_`, `canonicalizeLocale`, `fixZhLocale` (all 5 zh cases), `staticLocale` (panic instead of compileError), extern decls; test "canonicalizeLocale darwin" ported with all 9 `expectEqualStrings`. See S111-F10 (disputed). |
| `src/os/i18n_locales.zig` | `crates/rustty-os/src/i18n_locales.rs` | ported | 34-entry `locales` list identical in content and order (`zh_CN` … `sr@latin`). |
| `src/os/kernel_info.zig` | `crates/rustty-os/src/kernel_info.rs` | ported | `getKernelInfo` reads `/proc/sys/kernel/osrelease`, trims ASCII whitespace, non-linux → `None`; test ported with the same 2 assertions. See S111-F7 on >128-byte handling. |
| `src/os/locale.zig` | `crates/rustty-os/src/locale.rs` | ported | `ensureLocale`, `setLangFromCocoa`, `preferredLanguageFromCocoa`, locale-c aliases, `setenv`/`unsetenv` externs. Oracle has no tests. See S111-F5 and S111-F6. |

**Partial units:** none — every unit listed above was read in full (`units_partial` was empty for both shards).

## Findings

### High severity

None.

### Medium severity

#### S110-F1 — GlobalZigAllocator::alloc panics (aborts across the C ABI) on an over-large len instead of returning null

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** missing-error-path
- **Oracle:** `src/lib/allocator.zig:235-236` (std.heap.c_allocator backend; malloc failure → null → OutOfMemory)
- **Rustty:** `crates/rustty-lib/src/allocator.rs:510-528` (corrected location: `Self::layout` unwrap at 510-514, called from 524-527 and 540)

**Description.** The reference default backend (`std.heap.c_allocator` / `smp_allocator`) returns null for any allocation it cannot satisfy, which the C API surfaces as `OUT_OF_MEMORY`. rustty's replacement backend builds the `Layout` with `Layout::from_size_align(len, align).unwrap()`; `from_size_align` returns `Err` when `len` rounded up to `align` exceeds `isize::MAX`, so a C caller (via the exported vtable in `c_allocator()` / `default(None)`) asking for e.g. `SIZE_MAX` bytes triggers a panic inside an `extern "C"` callback — an abort — rather than a null/OOM result.

**Evidence.** `allocator.rs:511-513` `std::alloc::Layout::from_size_align(len, alignment.to_byte_units()).unwrap()` called from `alloc` (526) and `remap` (540); oracle `allocator.zig:236` `c_allocator = .fromZig(&std.heap.c_allocator)` whose alloc returns `?[*]u8`, null on malloc failure. The presence lens additionally grepped the workspace: no `from_size_align(..).ok()`/checked path exists near this backend, and `GlobalZigAllocator` is the production backend used by `rustty-vt-c/src/osc.rs:34`, `rustty-inspector/src/widgets/termio.rs:86` and `rustty-bench`, so the abort-vs-null divergence is not confined to tests.

**Suggested fix.** Use `Layout::from_size_align(..).ok()` and return `null_mut()` (alloc/remap) or `false` (resize) on `Err`.

#### S111-F1 — allocTmpDir treats an empty TMPDIR/TMP as unset (oracle uses the empty value)

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/file.zig:85-86`
- **Rustty:** `crates/rustty-os/src/file.rs:138-147` and `crates/rustty-global/src/environ.rs:405-411`

**Description.** The oracle does `environ.getPosix("TMPDIR") orelse environ.getPosix("TMP") orelse return "/tmp"` and then trims trailing separators — a present-but-empty `TMPDIR` yields `""` and does NOT fall through to `TMP`. rustty adds `.filter(|v| !v.is_empty())` after the `or_else` chain, so `TMPDIR=""` returns `/tmp` instead of `""`, and `TMPDIR=""` with `TMP=/x` also returns `/tmp` instead of `""`. rustty-global's duplicated fallback `default_tmp_dir_path` carries the same extra filter, so the pinning test `alloc_tmp_dir_agrees_with_global_fallback` passes while both diverge from the oracle.

**Evidence.** `file.zig:85` `const tmpdir = environ.getPosix("TMPDIR") orelse environ.getPosix("TMP") orelse return "/tmp";` vs `file.rs:138-142` `.get_posix(b"TMPDIR").or_else(|| environ.get_posix(b"TMP")).filter(|v| !v.is_empty())`. Zig std `getPosix` (zig 0.16 `std/process/Environ.zig:606-619`) returns a non-null empty slice for `TMPDIR=""`; rustty's `get_posix` likewise returns `Some(&b"")` (its own test at `environ.rs:443`). Neither lens found a documented justification for the extra filter.

**Suggested fix.** Drop the `.filter(|v| !v.is_empty())` in `file.rs:141` and `environ.rs:410` so an empty `TMPDIR`/`TMP` is returned (trimmed) exactly as the oracle does, or document the deviation and update both copies together.

#### S111-F3 — WaitError drops the xev.Timer.RunError half; a timer failure is reported to the callback as FlatpakSpawnFail

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/flatpak.zig:110, 220-228, 462-470`
- **Rustty:** `crates/rustty-os/src/flatpak.rs:348-349, 869-873`

**Description.** The oracle defines `WaitError = xev.Timer.RunError || Error` and the timer-run callback forwards a timer error verbatim (`const result = if (r) |_| c_outer.result.? else |err| err;`). rustty collapses `WaitError` to `Error` (`pub type WaitError = Error;`) and maps any timer error to `Err(Error::FlatpakSpawnFail)`, so a consumer (termio Exec's flatpak wait callback) cannot distinguish a timer/loop failure from a spawn failure.

**Evidence.** `flatpak.zig:110` `pub const WaitError = xev.Timer.RunError || Error;` and `:226` `else |err| err` vs `flatpak.rs:349` `pub type WaitError = Error;` and `:870-873` `Err(_) => Err(Error::FlatpakSpawnFail)`. The timer error type is real and populated (`crates/rustty-xev/src/dynamic.rs:1239` `TimerRunError`, re-exported `lib.rs:410`), so the callback receives a different error value than the reference on a timer/loop failure.

**Suggested fix.** Make `WaitError` an enum carrying both `Error` and `xev::TimerRunError` variants and forward the timer error unchanged in `xev_timer_callback`.

### Low severity

#### S110-F3 — Test `c allocator` does not exercise `c_allocator()`; the public fn is untested

- **Status:** CONFIRMED (severity corrected medium → low by both lenses)
- **Kind:** weakened-assertion
- **Oracle:** `src/lib/allocator.zig:246-253`
- **Rustty:** `crates/rustty-lib/src/allocator.rs:665-683` (test) vs the untested `c_allocator()` at `:645`

**Description.** The oracle test does `const alloc = c_allocator.zig();` — it round-trips the module-level `c_allocator` value. The rustty test named `c_allocator` instead constructs a fresh `GlobalZigAllocator`/`ZigAllocatorRef`/`Allocator::from_zig` on the stack and never calls `c_allocator()`, so the static-backed `C_ALLOCATOR_REF`/`SyncAllocatorRef` path is not covered by any test in the crate.

**Evidence.** oracle `allocator.zig:249` `const alloc = c_allocator.zig();` vs `allocator.rs:671-675` `let backend = GlobalZigAllocator; let backend_ref = ZigAllocatorRef::new(&backend); let c_alloc = Allocator::from_zig(&backend_ref); let alloc = c_alloc.zig();`. The test fn name even shadows the glob-imported fn. Workspace grep for `c_allocator` finds only an unrelated `TestAllocatorStats::c_allocator` in `rustty-vt-c/src/test_util.rs:64` — no caller of `rustty_lib::allocator::c_allocator` exists. Assertion count is not reduced, so this is a test-subject gap rather than a weakened assertion.

**Suggested fix.** `let c_alloc = c_allocator(); let alloc = c_alloc.zig();` then the same alloc/`len==10`/free assertions.

#### S110-F5 — PackedTaggedUnionLayout::validate drops the arm type-identity and Owner-identity compile-time checks

- **Status:** DISPUTED (semantic lens refuted; presence lens confirmed)
- **Kind:** missing-branch
- **Oracle:** `src/lib/packed.zig:165-178` (and `68-73`)
- **Rustty:** `crates/rustty-lib/src/packed.rs:272-291` (and `154-163`); corrected: `283-291` (validate) and `159-165` (TaggedUnion field check)

**Description.** The oracle's comptime block rejects an arm whose layout `Zig` type differs from the union's source field type ("packed union arm layout has the wrong Zig type for tag") in addition to the width bound, and `Packed`'s tagged_union arm also checks `Layout.Owner != T`. The Rust model only enforces `layout.bit_size > union_bit_size` and matches `union_field` name + bit width; a same-width but wrong-source-type arm (or a layout built for a different owner) passes const validation. These are compile-time-only guards (no runtime observable), used by vt-c `types.rs` / vt `page.rs` descriptors.

**Evidence.** `packed.zig:171-174` two `@compileError` checks; `packed.rs:285-287` single width check with a PORT NOTE admitting the modeling; `packed.zig:69` `Layout.Owner != T` vs `packed.rs:159-161` name+width only. The presence lens also checked the only consumer, `rustty-vt-c/src/types.rs:2953-2962` (`writePackedTaggedUnion`) — it performs no compensating identity check.

**Refuting lens (semantic), verbatim:** "Read packed.zig:66-73 (nested/tagged-union field checks incl. `Layout.Owner != T`) and :164-178 (arm `Layout.Zig != @FieldType(UnionT, source_name)` plus width bound) against packed.rs:144-168 and :270-291. The rustty checks are present but model type identity by bit width, with PORT NOTEs saying so. These are comptime-only guards over hand-written descriptors: they only fire for a malformed descriptor, and Rust const context has no type reflection to express `Layout.Zig != @FieldType(...)`. Every descriptor in the tree (vt-c types.rs:2953, vt page descriptors) is well-formed, so no runtime input or state produces a different value, error, or ordering. Difference confined to code unreachable for any valid program -> refuted."

**Suggested fix.** Carry a `source_bit_size`/owner id per arm (or per-field zig type label) in the descriptor and check equality in `validate()`.

#### S111-F2 — gerror_message() is a stub returning the literal "GError" — GLib error text is never logged

- **Status:** CONFIRMED (severity corrected medium → low by both lenses: log text only)
- **Kind:** stub-or-unimplemented
- **Oracle:** `src/os/flatpak.zig:254, 274, 297, 304, 326-337, 398`
- **Rustty:** `crates/rustty-os/src/flatpak.rs:891-895` (used at `:510, 532, 568, 622, 627, 632, 737`)

**Description.** Every warn site in the oracle prints `g_err.?.*.message` (the GError message string). The rustty helper `gerror_message` ignores its argument and returns `"GError"`, so all seven diagnostics (signal bus error, signal send error, spawn bus error, three fd-append errors, HostCommand RPC failure) lose the actual failure reason. GError is a public struct (`domain: GQuark`, `code: gint`, `message: gchar*`); the message can be read via a `repr(C)` struct.

**Evidence.** `flatpak.zig:297` `log.warn("spawn error getting bus: {s}", .{g_err.?.*.message});` vs `flatpak.rs:891-895` `unsafe fn gerror_message(_err: *mut c::GError) -> String { "GError".to_string() }` (with a PORT NOTE claiming the message text is logged). No other GError-message reader exists in the file.

**Suggested fix.** Declare `#[repr(C)] struct GError { domain: u32, code: c_int, message: *const c_char }` and read `(*err).message` (null-checked) into the log string.

#### S111-F4 — All cf_release_thread log calls are empty no-op helpers (warn/err/debug never emitted)

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/cf_release_thread.zig:105, 112-113, 116, 122, 137-138, 169, 178`
- **Rustty:** `crates/rustty-os/src/cf_release_thread.rs:293-328`

**Description.** The oracle logs warn on thread error, warn on abrupt exit, err on drain-loop failure, err on wakeup error, err on drain-mailbox error and several debug lines. rustty keeps named helpers but every body is a comment only, so none of these diagnostics reach stderr/the log facade. Other modules in the same crate (`file.rs`, `edit.rs`, `flatpak.rs`, `tempdir.rs`) do emit via `eprintln!`. Also `defer log.debug("cf release thread exited")` (line 122) has no call site at all.

**Evidence.** `cf_release_thread.zig:105` `log.warn("error in cf release thread err={}", .{err});` vs `cf_release_thread.rs:298-300` `fn log_warn_thread_err(_err: &xev::Error) { /* log.warn(...) */ }` (empty body); same for the seven sibling helpers at 301-328 (`log_warn_abrupt_exit`, `log_debug_fully_exiting`, `log_err_drain_failed`, `log_debug_starting`, `log_debug_shutdown`, `log_err_wakeup`, `log_err_drain_mailbox`). Grep of the file for `eprintln!`/`log::` finds none.

**Suggested fix.** Emit the messages through the crate's `eprintln!`-based scoped log convention (`[warn] [cf_release_thread] ...`) as `file.rs`/`edit.rs` do, and add the `cf release thread exited` debug line at the end of `threadMain_`.

#### S111-F5 — All locale.zig log.info/log.warn/log.debug sites are empty helpers or comments — nothing is emitted

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/locale.zig:31, 45, 60, 71, 74, 76, 87, 98, 114, 117, 121, 134, 139-142, 188`
- **Rustty:** `crates/rustty-os/src/locale.rs:143-161, 242, 261, 286, 290, 294, 306, 315, 360-361, 364`

**Description.** `ensureLocale`'s five info/warn lines and `setLangFromCocoa`/`preferredLanguageFromCocoa`'s seven warn/info/debug lines are preserved only as comments or empty functions in rustty; the user never sees "setlocale failed … Falling back to en_US.UTF-8", "error setting locale env var", "failed to convert a preferred language to UTF-8", etc. `i18n.rs:102-105` `log_debug_binding` is the same pattern for `i18n.zig:47`.

**Evidence.** `locale.zig:71` `log.warn("setlocale failed with LANG and system default. Falling back to en_US.UTF-8", .{});` vs `locale.rs:149-151` `fn log_warn_fallback() { /* log.warn(...) */ }`. `log_info_setlocale_env`, `log_info_setlocale_unset`, `log_info_setlocale_default`, `log_warn_fallback_failed`, `log_info_setlocale_windows_todo` are likewise comment-only; the Cocoa path's UTF-8 warn survives only as a comment at `locale.rs:360-361`. No `eprintln!`/`log::` emitters in the file.

**Suggested fix.** Route these through the crate's `eprintln!`-based scoped log helpers (`[warn] [os_locale] ...`).

#### S111-F6 — preferredLanguageFromCocoa no longer bounds the joined LANGUAGE string by the 1024-byte buffer

- **Status:** CONFIRMED
- **Kind:** missing-error-path
- **Oracle:** `src/os/locale.zig:183-189, 193-196, 208, 215`
- **Rustty:** `crates/rustty-os/src/locale.rs:352-392` (corrected: `370-401`)

**Description.** The oracle writes ':' separators, each canonicalized locale, '.UTF-8' and the trailing NUL into a fixed 1024-byte writer and returns `error.NoSpaceLeft` (caller logs and skips setting LANGUAGE) when any write overflows; it also drops any single preferred language whose UTF-8 form exceeds 255 bytes (`str.cstring(&str_buf[255:0], .utf8) orelse continue`). rustty accumulates into an unbounded `Vec` (only the canonicalize sub-slice is bounded by `buf[pos..]`), so an over-long list produces a LANGUAGE value the oracle would have refused; it also maps a non-UTF-8 language string to `NoSpaceLeft` (`c_str.to_str().map_err(|_| NoSpaceLeft)?`) where the oracle passes raw bytes to `canonicalizeLocale`.

**Evidence.** `locale.zig:194` `_ = writer.writeByte(':') catch return error.NoSpaceLeft;` / `:208` `_ = writer.writeAll(".UTF-8") catch return error.NoSpaceLeft;` / `:215` trailing NUL vs `locale.rs:370-372` `out.push(b':')` and `:391` `out.extend_from_slice(b".UTF-8")` with no bound check, and the NUL deliberately not written (`:398-401`). Both lenses note the residual slack is ~7 bytes per entry, so the divergence is real but narrow.

**Suggested fix.** Track the logical write position against `buf.len()` (including the trailing NUL) and return `NoSpaceLeft` on overflow; pass bytes to `canonicalizeLocale` instead of requiring `&str`.

#### S111-F7 — getKernelInfo returns None when osrelease exceeds 128 bytes; oracle truncates to 128

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/kernel_info.zig:12-18`
- **Rustty:** `crates/rustty-os/src/kernel_info.rs:22-28`

**Description.** At the pin the oracle reads with `file.readPositionalAll(io, &kernel_info_buf[128], 0)`, which returns up to 128 bytes (a longer file is silently truncated) and then trims. rustty reads the whole file and returns `None` if `len > 128` (a PORT NOTE still cites the older `readToEndAlloc(alloc, 128)` semantics).

**Evidence.** `kernel_info.zig:13-17` `kernel_info_buf[0 .. file.readPositionalAll(global.io(), &kernel_info_buf, 0) catch return null]` vs `kernel_info.rs:26-28` `if kernel_info.len() > 128 { return None; }`; stale PORT NOTE at `kernel_info.rs:23-25`. Practically unreachable on real kernels.

**Suggested fix.** Truncate to the first 128 bytes before trimming instead of returning `None`.

#### S111-F8 — cgroup::current does a single read() where the oracle's readSliceShort fills buf until EOF

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/cgroup.zig:18-22`
- **Rustty:** `crates/rustty-os/src/cgroup.rs:30-34`

**Description.** `reader.readSliceShort(buf)` returns fewer than `buf.len` bytes only at end-of-stream; rustty performs one `Read::read(buf)`, which may legitimately return a short count before EOF. On a short read rustty sees truncated contents and can return a truncated path or `None` where the oracle returns the full first-line path (the parse takes the LAST ':' in the contents — `cgroup.zig:25` vs `cgroup.rs:36`). For `/proc/<pid>/cgroup` a single read normally returns the whole content, so this is an edge case, but the PORT NOTE's claim that the oracle does a single short read is inaccurate.

**Evidence.** `cgroup.zig:21` `const len = reader.readSliceShort(buf) catch return null;` vs `cgroup.rs:33` `let len = file.read(buf).ok()?;`; inaccurate PORT NOTE at `cgroup.rs:29-32`.

**Suggested fix.** Loop on `read()` until 0 or buf is full (or use a `read_exact`-style fill that stops at EOF).

#### S111-F9 — FlatpakHostCommand.argv is Vec<String> instead of byte slices

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/flatpak.zig:50, 343-348`
- **Rustty:** `crates/rustty-os/src/flatpak.rs:261, 667-671`

**Description.** The oracle's argv is `[]const []const u8` (arbitrary bytes, `dupeZ`'d into the `^aay` variant, where an interior NUL merely truncates the C string). rustty models it as `Vec<String>`, which forces UTF-8 at the seam (Exec's argv is bytes), contrary to the project's byte-slice convention, and `CString::new(...).unwrap()` panics on an interior NUL where the oracle would just pass the truncated C string.

**Evidence.** `flatpak.zig:50` `argv: []const []const u8,` vs `flatpak.rs:261` `pub argv: Vec<String>,` and `flatpak.rs:667-668` `CString::new(arg.as_str()).unwrap()`. The same file already uses the byte-slice form for env keys/values (`flatpak.rs:650-652` `CString::new(k.as_slice())`), so this is an internal inconsistency rather than a Rust-idiom equivalence.

**Suggested fix.** Use `Vec<Vec<u8>>` and `CString::new(arg.as_slice())` as is already done for env keys/values.

#### S111-F10 — test "_ returns msgid at comptime" has no direct counterpart

- **Status:** DISPUTED (presence lens refuted; semantic lens confirmed)
- **Kind:** missing-test
- **Oracle:** `src/os/i18n.zig:205-210`
- **Rustty:** `crates/rustty-os/src/i18n.rs:420-428` (and the collapse note at `130-141`)

**Description.** The oracle asserts `comptime _("Ghostty")` returns `"Ghostty"`. Rust has no comptime evaluation of `gettext_`, so the test is not ported by name; the nearest stand-in is `gettext_resolves_from_vendored_libintl` (macOS-only) which asserts `gettext_(msgid) == msgid` at runtime. The assertion meaning (msgid round-trips) is covered only on macOS and not on other targets; the `gettext_` `!i18n()` arm (`i18n.rs:143-147`) is untested.

**Evidence.** `i18n.zig:205-210` vs `i18n.rs:420-428` (macOS-gated, different name).

**Refuting lens (presence), verbatim:** "The oracle test src/os/i18n.zig:205-210 asserts a property of a code arm that does not exist in the Rust port: `if (@inComptime()) return msgid;`. rustty documents that arm's collapse explicitly at crates/rustty-os/src/i18n.rs:130-141 ('Rust has no comptime evaluation of this function (it is not const fn; FFI cannot run at const-eval), so that arm collapses with no behavior here'), and the runtime msgid round-trip it implies IS asserted at crates/rustty-os/src/i18n.rs:420-428 (`assert_eq!(gettext_(msgid), msgid)`) — one assertion on each side. The reference test cannot be expressed at all in Rust (there is no comptime call path to test), so this is not-applicable rather than a weakened assertion; the macOS gate on the stand-in is a link-environment gate for the vendored libintl, not a coverage narrowing of the comptime property. Inconclusive-at-best, so refuted per the default."

**Suggested fix.** Add a cross-platform test `underscore_returns_msgid` asserting `gettext_(c"Rustty") == c"Rustty"` when no catalog is bound (or with i18n disabled).

#### S111-F11 — Shell-fallback home lookup drops the 512-byte stdout/stderr limits

- **Status:** CONFIRMED
- **Kind:** missing-error-path
- **Oracle:** `src/os/homedir.zig:62-67`
- **Rustty:** `crates/rustty-os/src/homedir.rs:138-159`

**Description.** The oracle runs `/bin/sh -c 'cd && pwd'` with `.stdout_limit = .limited(512)` / `.stderr_limit = .limited(512)` (`fba.buffer.len/2`) and a `try`, so both a spawn failure and output exceeding 512 bytes surface as an error from `home()`. rustty uses `Command::output()` with no limit and swallows any spawn error (`if let Ok(run)`), returning the (trimmed) output or `Ok(None)`; an over-512-byte home path is returned successfully.

**Evidence.** `homedir.zig:65-66` `.stdout_limit = .limited(fba.buffer.len / 2), .stderr_limit = .limited(fba.buffer.len / 2),` vs `homedir.rs:137-147` `.output()` unbounded and `:149` `if let Ok(run) = run` falling through to `Ok(None)` at `:159`.

**Suggested fix.** Cap captured stdout/stderr at 512 bytes and treat overflow as an error, mirroring `std.process.run`'s `StdoutStreamTooLong`.

#### S111-F12 — allocTmpDir converts TMPDIR through String::from_utf8_lossy

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/file.zig:85-86`
- **Rustty:** `crates/rustty-os/src/file.rs:146-147`

**Description.** A non-UTF-8 `TMPDIR` (legal on POSIX) is altered with U+FFFD replacement before being used as the temp-dir path, so the resulting path no longer names the real directory, whereas the oracle returns the raw byte slice. Per the project's Zig-byte-slice → `Vec<u8>` convention this should stay bytes (the same crate's `environ.get_posix` already yields bytes).

**Evidence.** `file.zig:86` `return std.mem.trimEnd(u8, tmpdir, &.{std.fs.path.sep});` vs `file.rs:146` `let tmpdir = String::from_utf8_lossy(tmpdir); trim_end_str(&tmpdir, PATH_SEP).to_string()`.

**Suggested fix.** Return `Vec<u8>` (or `OsString`) trimmed of trailing `/` bytes.

## Performance-only differences (not findings)

- `allocator.rs:530-534` `GlobalZigAllocator::resize` only succeeds when shrinking (`new_len <= len`) whereas Zig's `CAllocator` can also grow in place up to `malloc_usable_size`; both are valid results under the allocator contract (callers fall back to remap/alloc+copy).
- `allocator.rs:219` `default()` returns `Box<dyn ZigAllocator>` (heap-boxed trait object) where the reference returns a two-word `std.mem.Allocator` by value; behaviour identical, one extra allocation per C-API entry.
- `enum.rs` / `struct.rs` / `union.rs` / `packed.rs` model Zig comptime type factories as runtime/const descriptors (Vec-backed `EnumType`/`StructType`, `'static PackedLayout` tables); the value/layout algorithms match assertion-for-assertion, only the point of evaluation differs.
- `c_abi.rs` `MAX_ALIGNMENT` is a cfg-selected constant rather than a comptime fold; identical values on the supported targets.
- `i18n.locales_map`: the oracle's comptime `StaticStringMap(void)` is a linear `locales.contains(&name)` in rustty (identical membership result).
- `TempDir`: `Dir` handles are modelled as `PathBuf`s instead of fds (`Dir.close` is a no-op); all filesystem operations go through `std::fs` with the same observable results, and the fd-BADF probes were deleted upstream too.
- `cgroup`/`kernel_info`/`tempdir`/`edit`: fixed stack buffers (`max_path_bytes`, `FixedBufferAllocator`, `Writer.Allocating`) replaced by heap `String`/`Vec` with identical output bytes.
- `flatpak.start`: the `ArenaAllocator` for `dupeZ`'d env/argv strings is replaced by a `keep_alive: Vec<CString>` that lives to end of scope.
- `file.rs randomBasename` uses `arc4random_buf`/`getentropy` directly rather than the oracle's `global.io().random`; both are CSPRNG-backed with an identical output alphabet/length.
- `cf_release_thread`: Rust ownership subsumes the explicit `alloc` field; `Mailbox::create()`/`destroy()` and `Vec` drop replace `alloc.free` of the refs slice.

## Refuted candidates

Checked and cleared — both verification lenses refuted these:

- **S110-F2 — GlobalZigAllocator::remap with new_len == 0 on a non-empty block calls std::alloc::realloc with size 0 (UB).** The oracle premise is wrong: Zig's raw vtable contract requires `new_len > 0` too (`std/mem/Allocator.zig:43,64`; `std.heap.CAllocator.remap` asserts it at `std/heap.zig:298`), and no call site in either tree reaches remap with 0 — rustty's typed helpers short-circuit zero lengths at `allocator.rs:93-96 / 108-112`.
- **S110-F4 — MAX_ALIGNMENT table only reproduces the comptime fold for 5 documented targets.** Values are equal on every target the project builds (PORT NOTE at `c_abi.rs:13-25` records the `zig build-obj` probe results); the finding itself concedes it is unreachable on the locked targets, so no observable divergence exists.
- **S110-F6 — TaggedUnion model does not perform the nested `Type.C` / `v.cval()` payload conversion.** The conversion is present, hand-authored at the use sites (`rustty-vt-c/src/sgr.rs:246-300` `AttributeC::cval` with `unknown_cval`/`underline_cval`; `terminal.rs:577-597, 1293, 3371-3410`), and the descriptor's fields already carry the `Type.C`-resolved size/alignment (`union.rs:133-140`), so `cvalue_size()` yields the same layout.
