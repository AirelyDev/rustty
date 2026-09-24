# Parity audit — src

Audited the 28 oracle units directly under `ghostty/src` (30 shard rows, `src/Surface.zig` split into three ranges), pin `20abdb50a`, across 5 shards. **11,039 lines read** against **11,039 lines in scope** — every unit was read in full; there are no partially-read units. Unit status: **23 ported**, **4 partial** (`global.zig`, `main_ghostty.zig`, `main_wasm.zig`, `pty.zig`), **2 replaced** (`apprt.zig`, `helpgen.zig`), **1 not-applicable** (`quirks_memset.zig`), 0 missing. Findings: **17 CONFIRMED**, **2 DISPUTED**, **0 REFUTED**, **0 UNVERIFIED** — by corrected severity: **1 high**, **7 medium**, **11 low**.

The single most important gap is **S43-F1**: `global.init` never re-syncs the captured environment after `ensureLocale()`, so the LANG/LC_* corrections the reference re-captures are invisible to every `environ_map()` consumer (child-process environments, config/path lookup). A cluster of process-lifecycle gaps sits just behind it (`global.deinit`/sentry flush never runs on GUI exit, `xev.detect()` never runs on Linux, the bench binary skips global init entirely).

## Coverage

| Oracle unit:lines | Rustty counterpart(s) | Status | Note |
| --- | --- | --- | --- |
| `src/Surface.zig:1-2182` | `crates/rustty-core/src/surface.rs:1-5470`, `:14160-14225`, `:14428-14490` | ported | Line-for-line with Zig interleaved as comments: module consts, all 32 Surface fields, `InputEffect`, `Search`, `Mouse`/`Keyboard`, all 46 `DerivedConfig` fields + `scaledPadding`, `init`/`deinit`/`close`, `queueIo` readonly interception, inspector activate/deactivate, `needsConfirmQuit`, all 24 `handleMessage` arms, `childExited(Abnormally)`, `searchCallback`, `mouseRefreshLinks`, `updateConfig`, `dumpText`, `resolvePathForOpening`, `clipboardWrite`. Rust-only additions (`attachHostView`, post-spawn `initErrdefers`) are reference-invisible. No `test` blocks in range. |
| `src/Surface.zig:2183-4364` | `crates/rustty-core/src/surface.rs:5395-9665`, `:1581-1655`, `:13961-13975`, `:14056-14115`, `:14142-14160`, `:14224-14230`, `:14516-14530` | ported | Every declaration has a counterpart with identical branches, constants, side-effect ordering (mutex/queueIo/performAction), saturating arithmetic, and per-site error handling; `runtime_safety`→`cfg!(debug_assertions)`, `isDarwin`→`cfg!(target_vendor="apple")`. No `test` blocks in range. |
| `src/Surface.zig:4365-6545` | `crates/rustty-core/src/surface.rs:9640-14160`, `:14915-14961` | ported | `processLinks`, `openUrl`, `osc8URI`, `cursorPosCallback`, `performBindingAction` (all arms incl. live search), `writeScreenFile`, the whole clipboard/kitty-clipboard complex, `showDesktopNotification`, `crashThreadState`, `presentSurface`, `getProcessInfo` (split accessors); the `queueIo frees allocated writes in readonly mode` test is ported. |
| `src/apprt.zig:1-69` | `crates/rustty-apprt/src/lib.rs:25-57`, `crates/rustty-apprt-api/src/lib.rs:49-76`, `crates/rustty-apprt/src/gtk/app.rs:319-323`, `crates/rustty-apprt/tests/gtk_seam.rs:25-27` | replaced | Comptime `runtime` switch → generic `apprt_api::App` trait parameter + cfg gate for gtk; all struct/action/ipc re-exports live in `rustty-apprt-api`; `must_draw_from_app_thread` → `App::MUST_DRAW_FROM_APP_THREAD`. The anonymous `refAll` test has no assertions. |
| `src/build_config.zig:1-105` | `crates/rustty-build-config/src/lib.rs:1-391`, `crates/rustty-version/src/lib.rs` (the git-derived version, split out by task T-B2 so a commit rebuilds only its consumers) | ported | version/version_string, release_channel, artifact detection, exe_entrypoint/flatpak/snap/app_runtime/font_backend/renderer/i18n, bundle_id (R2 rename), SLOW_RUNTIME_SAFETY/IS_DEBUG. `mode_string` collapsed to 2 of 4 modes (S42-F3). |
| `src/cli.zig:1-14` | `crates/rustty-cli/src/lib.rs:37-137` | ported | args/action/rustty modules + CompatibilityHandler, compatibility_renamed, DiagnosticList, Diagnostic, Location re-exported from `rustty-args`; `refAllDecls` → `ref_all_decls`. |
| `src/config.zig:1-57` | `crates/rustty-config/src/lib.rs:1-126`, `crates/rustty-cfg-fmt/src/formatter.rs` | ported | All 30 field-type aliases + Config/conditional/io/string/edit/url/ConditionalState/FileFormatter/RepeatableStringMap/CApi/Wasm re-exported in reference order; `entryFormatter`/`formatEntry` relocated to `rustty-cfg-fmt` (R4), `preferredDefaultFilePath` in `file_load.rs`. |
| `src/crc32c.zig` | `crates/rustty-lib/src/crc32c.rs` | ported | init/update/final/hash, hardware 8/4/1-byte batching, slicing-by-16 software tables, threshold 4096 + `(len/3) & ~15` split, fold/zero_shift/mat_times_vec + matrices; all 5 tests with equal assertion counts. |
| `src/fastmem.zig` | `crates/rustty-util/src/fastmem.rs` | ported | move/copy/rotateOnce/rotateOnceR/rotateIn/rotateInR with identical element movement. |
| `src/fastprint.zig` | `crates/rustty-vt/src/fastprint.rs` | ported | `printDecimal` u8 and u21 paths identical; both test case lists ported with equal assertions. |
| `src/file_type.zig` | `crates/rustty-util/src/file_type.rs` | ported | Full signature table (5 JPEG forms incl. wildcards, PNG, GIF87a/89a, BM, qoif, RIFF????WEBP), `detect` first-match-wins, ASCII-insensitive `guessFromExtension`. |
| `src/global.zig` | `crates/rustty-global/src/{lib.rs,environ.rs}`, `crates/rustty-apprt/src/lib.rs`, `crates/rustty/src/main_rustty.rs`, `crates/rustty-apprt-c/src/lib.rs`, `crates/rustty-termio/src/command.rs` | partial | R4 split: leaf state in `rustty-global`, side-effecting init body in `rustty_apprt::init_with_action`. Gaps: `syncEnviron()` after `ensureLocale` (S43-F1), `xev.detect()` (S43-F2), the `InitOpts.tool` arm (S43-F3), `deinit()`'s crash flush (S43-F4). |
| `src/helpgen.zig` | `crates/rustty-helpgen/src/lib.rs`, `crates/rustty-{config,input,cli}/build.rs` | replaced | Zig-AST program → three `build.rs` codegen steps over shared `find_common_prefix`/`extract_doc_block`/`doc_contents_above`; `_`-prefixed skip and the `doc comment must be present…` + exit(1) preserved as a build panic with identical text. |
| `src/input.zig` | `crates/rustty-input/src/lib.rs`, `crates/rustty-vt/src/input/mod.rs` | ported | Re-export hub split across `rustty-vt::input` and `rustty-input`; `Keymap` = darwin/noop by cfg; `ctrlOrSuper` present. One misquoted comment (S43-F7). |
| `src/lib_vt.zig` | `crates/rustty-vt-c/src/{lib.rs,sys.rs}`, `crates/rustty-vt/src/lib.rs`, `crates/rustty-lib/src/allocator` | ported | All 203 `@export` symbols have `rustty_<name>` counterparts with matching feature gates (verified 203/203, 0 gate mismatches) plus the 5 wasm allocator exports. Zig-toolchain-only pieces (TinyIo, `_fltused`, quirks_memset force-reference) N/A; the release `tinyPanicImpl` handler is not ported (S43-F6). |
| `src/main.zig` | `crates/rustty/src/{main.rs,main_rustty.rs}`, `crates/rustty-extra/src/{mdgen.rs,webgen.rs}`, `crates/rustty-config/build.rs` | ported | Entry selector arms mapped; `std_options` forwarded via main_rustty log setup; quirks_memset force-reference is a documented non-port. |
| `src/main_bench.zig` | `crates/rustty-bench/src/{main_bench.rs,cli.rs}` | ported | Thin entry → `rustty_bench::cli::main`, error → stderr + FAILURE. The missing `global.init(.tool)` is reported under `global.zig` (S43-F3). |
| `src/main_build_data.zig` | `xtask/src/{main_build_data.rs,res_misc.rs}` | ported | 9-arm Action enum with identical CLI spellings, stdout emit switch over bash/fish/zsh/sublime/vim + terminfo, NoAction error. Action detection diverges (S43-F5). |
| `src/main_c.zig` | `crates/rustty-apprt-c/src/lib.rs`, `include/rustty-apprt.h` | ported | `rustty_init`, `rustty_cli_try_action`, `Info`/`BuildMode` (repr(C), 24 bytes, frozen ordinals), `rustty_translate`, `String` + `rustty_string_free`; 3 string tests ported with equal runtime assertions. DllMain MSVC bootstrap N/A. |
| `src/main_gen.zig` | `crates/rustty-synthetic/src/{main_gen.rs,cli.rs}` | ported | `main` → `rustty_synthetic::cli::main` returning Result; reference `synthetic.cli.main` does not call `global.init`, so no tool-arm gap here. |
| `src/main_ghostty.zig:1-266` | `crates/rustty/src/main_rustty.rs`, `crates/rustty-core/src/logging.rs`, `crates/rustty-apprt/src/lib.rs` | partial | main dispatch and `logFn` (macOS unified-log + stderr sinks, level mapping, scope prefixes) ported. Gaps: no `defer global.deinit()` counterpart on the GUI path (S44-F2), `unimplemented!()` GUI leaf (S44-F5), debug-level filter ordering in `log_with` (S44-F4). |
| `src/main_wasm.zig:1-26` | `crates/rustty-wasm/src/lib.rs` | partial | Comptime retention modelled by crate edges + `#[used]` statics; `logFn` retained as `rustty_os::wasm::log::freestanding::log`. `std_options.log_level` diverges and is unconsumed (S44-F6). |
| `src/math.zig:1-15` | `crates/rustty-util/src/math.rs` | ported | Mat/F32x4/ortho2d identical values (`[f32;4]` for `@Vector(4,f32)`). |
| `src/pty.zig:1-519` | `crates/rustty-os/src/pty.rs` | partial | winsize/Mode defaults, ProcessInfo, NullPty, full PosixPty (open/CLOEXEC/IUTF8/childPreExec/getProcessInfo), WindowsPty and the `test {}` ported (+ an extra IUTF8 test). Gaps: WindowsPty open errdefers (S44-F1), log routing (S44-F3). |
| `src/quirks.zig:1-57` | `crates/rustty-util/src/quirks.rs` | ported | `disableDefaultFontFeatures` via trait seam; `inlineAssert` = `debug_assert!`, matching the builtin.mode switch. |
| `src/quirks_memset.zig:1-338` | `crates/rustty-bench/src/main_bench.rs`, `.port/phase-3/NON_PORTS.md` §E34 | not-applicable | Zig-toolchain workaround for Zig 0.16 losing loop auto-vectorization; rustc lowers to the platform libc memset (already SIMD/`dc zva`), so there is no slow symbol to override. Its 3 tests have no subject in Rust. |
| `src/renderer.zig:1-66` | `crates/rustty-renderer/src/lib.rs` | ported | Hub re-exports; comptime `Renderer` switch → per-target cfg (metal/opengl/webgl); `Health` `enum(c_int)` → `#[repr(i32)]` with the header-parity test. |
| `src/surface_mouse.zig:1-336` | `crates/rustty-core/src/surface_mouse.rs` | ported | Fields, `keyToMouseShape` state machine, `eligibleMouseShapeKeyEvent`, `isMouseModeOverrideState`, `isRectangleSelectState`; the `keyToMouseShape` test with all 13 sub-blocks. |
| `src/termio.zig:1-39` | `crates/rustty-termio/src/lib.rs` | ported | All re-exports plus relocated `Command`; `refAllDecls` → `ref_all_decls`; `shell_integration` present with its own tests. |
| `src/tripwire.zig:1-288` | `crates/rustty-util/src/tripwire.rs` | ported | `module(P,E)` → `Module<P,E>` instance; check/checkConstrained, errorAlways/errorAfter, `end(reset|retain)` with untripped warning + UntrippedError, reset; all 4 tests with equal assertions. |

**Partially-read units: none.** Every unit listed above was read end-to-end in its shard; the four rows marked `partial` are *port*-status partial (documented gaps captured as findings), not coverage-partial.

## Findings

### High

#### S43-F1 — global.init does not re-sync the captured environment after `ensureLocale()`

- **Status:** ✅ CONFIRMED (both lenses failed to refute)
- **Kind:** missing-branch · **Severity:** high
- **Oracle:** `src/global.zig:204-209, 307-322`
- **Rustty:** `crates/rustty-apprt/src/lib.rs:329-337`

The reference calls `try internal_os.ensureLocale(); syncEnviron();` so the LANG/LC_* mutations `ensureLocale` performs via setenv/unsetenv are re-captured into `state.environ` (and the Io environ), which feeds `environMap()` and child-process environments. rustty's `init_with_action` calls `rustty_os::ensureLocale()` and never calls `rustty_global::sync_environ()`; the only caller of `sync_environ()` in the workspace is the GTK application class. The process-global `ENVIRON` was already lazily captured earlier in the same init (tmp-dir discovery, RUSTTY_LOG read) so it holds the pre-locale block, and `environ_map()` consumers (termio command environment, crash env snapshot, config/path lookup) never see the corrected locale.

**Evidence:** ghostty `src/global.zig:208-209` `try internal_os.ensureLocale(); syncEnviron();` (preceded at :206 by the comment "We need to re-sync the environment after this completes"). rustty `crates/rustty-apprt/src/lib.rs:333` `rustty_os::ensureLocale().map_err(InitError::Locale)?;` followed directly by glslang init at :337. `grep -rn 'sync_environ()' crates` → only `crates/rustty-apprt/src/gtk/class/application.rs:6072`. `crates/rustty-os/src/locale.rs:171-173` even documents "the app initializer calls it right after ensure_locale()" — no such call exists. rustty's `ENVIRON` is an owned copy (`crates/rustty-global/src/environ.rs:129-138`), so live-process mutations are not observed.

**Suggested fix:** Add `rustty_global::sync_environ();` immediately after the `ensureLocale()` call in `init_with_action`.

*Note: the presence lens confirmed but proposed medium (crash/sentry env snapshot is taken before `ensureLocale` upstream too, so only captured-environ consumers diverge); the semantic lens held high. Graded high.*

### Medium

#### S42-F1 — completeClipboardPasteEvent queues with `MutexState::Locked` after already unlocking the renderer mutex

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/Surface.zig:6222-6247`
- **Rustty:** `crates/rustty-core/src/surface.rs:13235-13251`

The oracle holds `renderer_state.mutex` (defer unlock at 6223) for the whole tail of the function, so `self.queueIo(.{ .write_alloc = ... }, .locked)` at 6243-6246 runs with the mutex held; `.locked` tells `Mailbox.send` it may unlock/relock that mutex on the full-queue slow path. The port explicitly unlocks at `surface.rs:13235` and *then* calls `queueIo(..., MutexState::Locked)` at 13245-13251. On the mailbox-full path (`termio/mailbox.rs:165-179`) the port therefore calls `unlock()` on an unheld zigstd Mutex (debug_assert panic "unlock of an unlocked zigstd thread::Mutex" in checked builds, `rustty-zigstd thread.rs:108`) and then relocks it, returning with the mutex **locked** while the caller believes it unlocked → the next lock deadlocks.

**Evidence:** Oracle 6222-6223 `self.renderer_state.mutex.lockUncancelable(...); defer ... unlock(...)` then 6243 `self.queueIo(.{ .write_alloc = ... }, .locked)`. Rustty 13235 `unsafe { (*self.renderer_state.mutex).unlock() };` followed by 13245-13251 `self.queueIo(termio::Message::WriteAlloc(...), termio::termio_impl::MutexState::Locked)`. `termio_impl.rs:1011-1017` maps `Locked` to the renderer mutex pointer; `mailbox.rs:165-179` unlocks/relocks it when the instant push fails. No intervening re-lock exists in `surface.rs:13130-13252`.

**Suggested fix:** Move the explicit unlock to after the `queueIo` call (keeping `MutexState::Locked`, matching the oracle's scope) — or pass `MutexState::Unlocked`; the oracle-faithful option is the former.

#### S43-F2 — `xev.detect()` is never run and the libxev backend log line is hardcoded

- **Status:** ✅ CONFIRMED
- **Kind:** missing-branch · **Severity:** medium
- **Oracle:** `src/global.zig:160-164, 178`
- **Rustty:** `crates/rustty-apprt/src/lib.rs:229-233, 292-302`

The reference runs `if (comptime xev.dynamic) xev.detect() catch |err| log.warn(...)` and then logs `libxev default backend={t}` from `xev.backend`. On Linux the reference `xev` is `Xev(&.{.io_uring, .epoll})` (dynamic), so `detect()` picks epoll when io_uring is unavailable. rustty-xev ports `dynamic::detect()`/`backend()` with `DYNAMIC = true` (`crates/rustty-xev/src/dynamic.rs:109,202`) and re-exports them (`lib.rs:529`), but `init_with_action` comments the call out claiming `xev.dynamic` is comptime-false on every target, so `BACKEND` stays at its default and the log line prints a constant string instead of the detected backend.

**Evidence:** ghostty `src/global.zig:161-164` and `:178`. rustty `crates/rustty-apprt/src/lib.rs:229-233` (comment-only PORT NOTE), `:293-302` `let xev_backend = if cfg!(target_os = "macos") { "kqueue" } else { "io_uring" }`. `grep -rn '::detect()' crates` shows `rustty_xev::detect` called only from tests (`rustty-xev/tests/surface_parity.rs:250`, `rustty-termio/tests/live_shell.rs:289`); the GTK path calls `prefer()` only. The presence lens additionally notes `BACKEND_DEFAULT` is the *last* candidate (Epoll, `dynamic.rs:149`), so the hardcoded "io_uring" log string is wrong about the actual default as well.

**Suggested fix:** On targets where `rustty_xev::DYNAMIC` is true (Linux/FreeBSD), call `rustty_xev::detect()` and warn with the reference text on failure; log `rustty_xev::backend()` in the "libxev default backend" line.

#### S43-F3 — `InitOpts.tool` arm not ported: the bench binary skips global init entirely

- **Status:** ✅ CONFIRMED
- **Kind:** missing-branch · **Severity:** medium
- **Oracle:** `src/global.zig:37-52, 129-136` (and `src/benchmark/cli.zig:54-55`)
- **Rustty:** `crates/rustty-global/src/lib.rs:421-431`, `crates/rustty-bench/src/cli.rs:185-207`

The reference `benchmark/cli.zig` main (reached via `main_bench.zig`) calls `global.init(.{ .tool = minimal })`, which performs everything in init except app-action detection: tmp-dir discovery, GHOSTTY_LOG logging config, SIGPIPE ignore, version/backend log lines, rlimit raising (`fixMaxFiles`), sentry init, `ensureLocale`, glslang and oniguruma init, resources-dir discovery and i18n. rustty-global's PORT NOTE declares the tool arm N/A on the grounds that rustty-bench never initializes global state — but that is precisely the divergence: `rustty_bench::cli::main` runs benchmarks with none of those process-level side effects (no raised NOFILE limit, no SIGPIPE ignore, no locale/oniguruma init, no RUSTTY_LOG-driven logging).

**Evidence:** ghostty `src/global.zig:37-52` (`tool` arm), `:129-136` (`.tool => null`); `src/benchmark/cli.zig:54-55`. rustty `crates/rustty-bench/src/cli.rs:186-207` reproduces the pre-`tool`-arm reference body (its own comment quotes the older `pub fn main() !void`); `crates/rustty-global/src/lib.rs:421-431` "the InitOpts::tool arm is N/A … deliberately NOT ported". Grep for `init_with_action`/`rustty_global`/`ensure_locale` in `rustty-bench`: none.

**Suggested fix:** Have rustty-bench's `cli::main` run the shared bootstrap with action detection skipped (e.g. `rustty_apprt::init_with_action(false)` or a tool-arm entry performing the same side effects), matching `benchmark/cli.zig:55`.

#### S43-F4 — `global.deinit()`'s `crash.deinit()` flush is never executed

- **Status:** ✅ CONFIRMED
- **Kind:** missing-function · **Severity:** medium
- **Oracle:** `src/global.zig:232-251` (and `src/main_ghostty.zig:58`)
- **Rustty:** `crates/rustty-global/src/lib.rs:478-506`

The reference `deinit()` frees resources_dir, calls `crash.deinit()` (joins the sentry init thread and `sentry_close()`s so pending crash data is flushed), frees the tmp dir path, deinits the Io and the GPA. It runs via `defer global.deinit()` in `main_ghostty.zig` whenever main returns normally (the GTK app path). rustty's leaf `GlobalState::deinit` is an empty body, `rustty_crash::deinit` (`crates/rustty-crash/src/sentry.rs:504`) has zero callers in the workspace, and the bin's GTK path (`run_gtk`) diverges through `std::process::exit` after terminate/destroy, so on a normal GTK exit sentry is never closed/flushed.

**Evidence:** ghostty `src/global.zig:237-238` `crash.deinit();`; `src/main_ghostty.zig:58` `defer global.deinit();`. rustty `crates/rustty-global/src/lib.rs:506` `pub fn deinit(&mut self) {}`; `grep -rn 'crash::deinit\|rustty_crash::deinit' crates` → no callers; `crates/rustty/src/main_rustty.rs:171-175` documents skipping `state.deinit()` on every path, `:294-301` `run_gtk` returns `!`.

**Suggested fix:** Call `rustty_crash::deinit()` (and the rest of the reference deinit sequence) on the GTK normal-exit path before the process exits, mirroring `defer global.deinit()`.

*(Overlaps with S44-F2, which reports the same missing `defer` from the `main_ghostty.zig` side.)*

#### S44-F1 — `WindowsPty::open` leaks pipe handles on every failure path (reference errdefers dropped)

- **Status:** ✅ CONFIRMED
- **Kind:** missing-error-path · **Severity:** medium
- **Oracle:** `src/pty.zig:383-450`
- **Rustty:** `crates/rustty-os/src/pty.rs:603-688`

The reference registers `errdefer CloseHandle(pty.in_pipe)` (`pty.zig:386`), `errdefer CloseHandle(pty.in_pipe_pty)` (`:401`) and `errdefer { CloseHandle(out_pipe); CloseHandle(out_pipe_pty); }` (`:421-424`) so that a failure in `CreateFileW`, `CreatePipe`, any of the four `SetHandleInformation` calls, or `CreatePseudoConsole` closes the handles created so far. The Rust port has no guard object and no explicit `CloseHandle` on any early `return Err(...)`/`?` (`pty.rs:616-619, 635-638, 655-664, 666-669, 681-683`), so the already-created named pipe / file / anonymous pipe handles are leaked on those paths. The PosixPty half *does* carry an `OpenErrdefer` guard (`pty.rs:247-251, 496-515`), so the omission is specific to the Windows arm. Observable as leaked kernel handles and a live named-pipe instance blocking `FILE_FLAG_FIRST_PIPE_INSTANCE` reuse.

**Evidence:** `pty.zig:386` / `:401` / `:421-424` vs `pty.rs:575-688`, whose only `CloseHandle` calls are in `deinit` (`pty.rs:690-699`), never reached because the Pty is never returned. `grep 'impl Drop'` in `pty.rs` returns only the POSIX `OpenErrdefer` at :506.

**Suggested fix:** Add a Windows `OpenErrdefer`-style guard tracking `in_pipe` / `in_pipe_pty` / `out_pipe` / `out_pipe_pty` that closes those created so far when dropped armed; disarm before `Ok(pty)`.

#### S44-F2 — `defer global.deinit()` has no counterpart on the GUI (`run_gtk`) path

- **Status:** ✅ CONFIRMED
- **Kind:** missing-branch · **Severity:** medium
- **Oracle:** `src/main_ghostty.zig:58, 99-115`
- **Rustty:** `crates/rustty/src/main_rustty.rs:171-175, 389-411`

In the reference, after `app_runtime.run()` returns normally the deferred calls run LIFO: `app_runtime.terminate()`, `app.destroy()`, then `global.deinit()` (`global.zig:232-251`: resources_dir.deinit, `crash.deinit()` which flushes crash logs via `sentry_close()`, freeTmpDir, io deinit, GPA deinit with leak diagnostics). The port's `run_gtk` performs terminate+destroy (`main_rustty.rs:395-397`) then `std::process::exit(0)` (`:401`) with no global deinit. The PORT NOTE at `:171-175` claims every path exits via `process::exit` like the reference's `posix.exit`, but the GUI path is precisely the one where the reference *does* reach the defer. `rustty_global::GlobalState::deinit` is an empty body (`rustty-global/src/lib.rs:501`) documenting the crash/resources work as a "deferred seam executed by rustty-core", and no caller performs that seam.

**Evidence:** `main_ghostty.zig:58` + `:114 try app_runtime.run();` vs `main_rustty.rs:390-411` (`let result = app_runtime.run(); app_runtime.terminate(); core_app.destroy(); match result { Ok(()) => std::process::exit(0), … }`); `rustty-global/src/lib.rs:501 pub fn deinit(&mut self) {}`; crash init *is* wired at `crates/rustty-apprt/src/lib.rs:314-322` while `rustty_crash::sentry::deinit` has no workspace caller.

**Suggested fix:** Add a `rustty_apprt::deinit()` (or core-side equivalent) performing the `global.zig` deinit body (crash flush, resources_dir deinit, tmp-dir release) and call it in `run_gtk` after `core_app.destroy()` and before the exit on both the Ok and Err arms (the reference `try` still runs the defer on error).

#### S44-F3 — pty log lines bypass the `logFn` sinks (stderr gate + macOS unified logging) and use a different format

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/pty.zig:7, 158, 168, 258, 307`
- **Rustty:** `crates/rustty-os/src/pty.rs:26-37, 258-271, 402-405, 466-469`

The reference logs via `std.log.scoped(.pty)`, which routes through `main_ghostty.zig` `logFn`: output is suppressed when `state.logging.stderr` is false (the default in lib mode — `global.zig:398` `stderr = build_config.app_runtime != .none`, i.e. the macOS app), is additionally sent to macOS unified logging under the `pty` category when `logging.macos` is set, and is formatted `warning(pty): …` / `error(pty): …`. The port's `log_warn`/`log_err` helpers unconditionally `eprintln!("[warn] [pty] …")` / `"[err] [pty] …"`, so (a) the CLOEXEC / TIOCSCTTY / TIOCPTYGNAME diagnostics appear on stderr in configurations where the reference is silent, (b) they never reach the unified log, and (c) the text differs. `rustty_core::logging::log_with` (the `logFn` port, which does apply both gates) exists but is not reachable from rustty-os (R4 tier), so a crate-local shim was used.

**Evidence:** `pty.zig:7` `const log = std.log.scoped(.pty);` with `log.warn`/`log.err` at `:158/:168/:258/:307` vs `pty.rs:30-37` `fn log_warn(msg) { eprintln!("[warn] [{LOG_SCOPE}] {msg}") }` used at `:258, :267, :402, :466`; `rustty-global/src/lib.rs:550-551` confirms `stderr` defaults to false when `app_runtime == None`.

**Suggested fix:** Route rustty-os logging through a tier-appropriate sink hook (a function-pointer/trait seam set by rustty-core at init) that applies the `Logging.stderr`/`macos` gates and the `level(scope): ` format, or at minimum consult the shared logging config.

### Low

#### S40-F1 — `mouse_interval` arithmetic width differs (u32 in Zig vs u64 in Rust)

- **Status:** ✅ CONFIRMED
- **Kind:** other · **Severity:** low
- **Oracle:** `src/Surface.zig:395` (finding cited 385)
- **Rustty:** `crates/rustty-core/src/surface.rs:955-956` (finding cited 928-929)

The reference computes `config.@"click-repeat-interval" * 1_000_000` in u32 (`click-repeat-interval` is `u32`, `Config.zig:2578`; peer type of `u32 * comptime_int` is u32) and only then coerces to the u64 field, so any configured interval > 4294 ms overflows (safety panic in Debug/ReleaseSafe, wrap in ReleaseFast). The port widens first (`u64::from(config.click_repeat_interval) * 1_000_000`) and yields the mathematically intended value. Reachable from a plausible user config (neither side clamps), but observable only for pathological configs; the Rust behaviour is a strict improvement, reported for completeness.

**Evidence:** `Surface.zig:395 .mouse_interval = config.@"click-repeat-interval" * 1_000_000, // 500ms` vs `surface.rs:956 mouse_interval: u64::from(config.click_repeat_interval) * 1_000_000`. `click-repeat-interval=10000` yields 10_000_000_000 ns in rustty vs 1_410_065_408 ns (wrap) or a panic in ghostty; consumed identically at `surface.rs:8418`/`:8782` vs `Surface.zig:3840`/`:4000`.

**Suggested fix:** None required functionally; optionally document the deliberate widening in the PORT NOTE.

#### S41-F1 — All `log.info/warn/err/debug` sites in `Surface.zig:2183-4364` are preserved only as comments

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/Surface.zig:2193, 2205, 2219, 2312, 2352, 2426, 2470-2477, 2698, 2854, 2996-2999, 3019-3022, 3033, 3061, 3084-3087, 3113-3116, 3139-3142, 3200, 3350, 3395, 3671, 3773, 3855, 3862, 3919, 3925, 3936, 3989, 4231-4234, 4283, 4342-4345`
- **Rustty:** `crates/rustty-core/src/surface.rs:251-254` (convention), 5398, 5415-5418, 5445-5453, 5690-5697, 5741, 5826-5830, 5876, 5989-5997, 6289, 6461, 6753-6757, 6787-6791, 6935-6938, 7009, 7108, 7233-7239, 7384, 7681, 8004, 8206, 8412, 8471-8475, 8586-8589, 8617-8620, 9314, 9366, 9424, 9483

The reference emits diagnostics via `std.log` (scoped `.surface`) at ~30 sites in this range, several user-facing rather than developer noise: "application attempted to write clipboard, but 'clipboard-write' is set to deny" (2193), "application sent invalid base64 data for OSC 52" (2205), "WARNING: very small terminal grid detected with padding set…" (2470-2477, twice), "key binding action failed action=… err=…" (3019), "unable to recompute initial window size" (2426), "error setting clipboard string" (2219, 2352), "failed to encode mouse event" (3773), "cursor is at prompt but no prompt found" (4231). The port declares a file-wide convention (`surface.rs:251-254`, "no Rust logging shim") and every site is a `// log.warn(...)` comment inside an otherwise-empty arm, so control flow is identical but no output is emitted. Both lenses stress this is a consistently applied *workspace-wide* convention (there is no `log`/`tracing` dependency anywhere in rustty), best tracked once project-wide.

**Evidence:** `surface.rs:5398 // log.info("application attempted to write clipboard, …")` with `return Ok(())` vs `Surface.zig:2193`; `surface.rs:5989-5997` both padding warnings are empty `if` bodies vs `Surface.zig:2470-2477`; `surface.rs:6753-6757 Err(_err) => { // log.info("key binding action failed …") }` vs `Surface.zig:3019-3022`. `grep -cE 'log::(warn|info|debug|error)' surface.rs` == 0; workspace-wide grep for any `log|tracing` macro returns nothing.

**Suggested fix:** Add a thin scoped logger and re-enable the commented calls verbatim (same level, same format string), or document the omission as an accepted project-wide deviation.

#### S42-F2 — Every `log.warn/info/debug/err` in `Surface.zig:4365-6545` is commented out

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/Surface.zig:4424, 4448, 4767, 4779, 4848, 4859-4862, 4874, 4891, 5027, 5030, 5055, 5063, 5075, 5105, 5120, 5131, 5142, 5465, 5521, 5531, 5538-5541, 5557-5560, 5563, 5586-5589, 5612, 5883, 5986, 6029, 6044, 6056, 6094-6097, 6155, 6162, 6305, 6354, 6473, 6490`
- **Rustty:** `crates/rustty-core/src/surface.rs:9640-14060` (every `// log.*` line)

All ~40 log sites in this range ("apprt did not handle open URL action, falling back to default opener", "apprt failed to set mouse visibility", "error scrolling to bottom", "error parsing text binding", "failed to get URI for OSC8 hyperlink", "application attempted to read clipboard, but clipboard-read is set to deny", "potentially unsafe paste detected", "rate limiting desktop notifications", "suppressing identical desktop notification", the key-table debug/info lines, "mode 5522 paste event was not written", "error setting clipboard") survive only as `// log.*` comments, so operator-visible diagnostics differ from the oracle. Note a logging facade *does* exist (`crates/rustty-core/src/logging.rs:37 log_with`) but only `crates/rustty/src/main_rustty.rs` wires it; the omission is a tracked tree-wide deferral (`.port/phase-2/DEFERRALS.md:2238`).

**Evidence:** oracle `4448 log.warn("apprt did not handle open URL action, falling back to default opener", .{});` vs rustty `9791 // log.warn(...)`; oracle `6155 log.info("potentially unsafe paste detected, rejecting until confirmation", .{});` vs rustty ~13641-13645 (only the unlock + `return Err(Error::UnsafePaste)`). `grep -c '^\s*// log\.'` → 103 commented sites; grep for any live logging call → 0.

**Suggested fix:** Route the commented sites through `rustty-core`'s `logging::log_with` at the same levels with the same message text.

#### S42-F3 — `mode_string` only ever reports "Debug" or "ReleaseFast"

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/build_config.zig:23-28`
- **Rustty:** `crates/rustty-build-config/src/lib.rs:123-129`

The oracle derives `mode_string` from `@tagName(builtin.mode)`, yielding Debug/ReleaseSafe/ReleaseFast/ReleaseSmall. The port collapses it to `cfg!(debug_assertions) ? "Debug" : "ReleaseFast"`. The value is observable: the sentry `build-mode` tag (`rustty-crash/src/sentry.rs:418`), the startup `build optimize=` log (`rustty-apprt/src/lib.rs:246`) and `+version` output (`rustty-cli/src/version.rs:171`). Reachable, not hypothetical: `Cargo.toml:411-414` defines a `[profile.release-safe]` with `debug-assertions = true`, which reports "Debug" where the oracle reports "ReleaseSafe"; the other two tags are unreachable.

**Evidence:** Oracle 24-28 `const m = @tagName(builtin.mode); if (lastIndexOfScalar(u8, m, '.')) |i| break :mode m[i..]; break :mode m;` vs rustty 123-129 `if cfg!(debug_assertions) { "Debug" } else { "ReleaseFast" }`.

**Suggested fix:** Distinguish optimized+debug_assertions ("ReleaseSafe") from unoptimized ("Debug") via a build.rs-captured `OPT_LEVEL`, and map opt-level `s`/`z` to "ReleaseSmall".

#### S42-F4 — `goto_tab` index conversion wraps instead of trapping above `i32::MAX`

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/Surface.zig:5317-5331` (conversion at 5328)
- **Rustty:** `crates/rustty-core/src/surface.rs:11722-11731` (finding cited 11673-11688)

The oracle converts the binding's `goto_tab: usize` payload with `@enumFromInt(v)` into the non-exhaustive `enum(c_int)` `GotoTab`; in safety-checked builds a value that does not fit `c_int` traps. The port does `apprt::action::GotoTab(v as i32)`, which silently wraps (e.g. `goto_tab=4294967295` becomes `-1` == PREVIOUS, `4294967294` becomes NEXT), so an out-of-range keybind value changes meaning instead of failing. No clamp exists at parse time in either tree.

**Evidence:** Oracle 5328 `.goto_tab => @enumFromInt(v),` with `Binding.zig:575 goto_tab: usize` and `action.zig:569 GotoTab = enum(c_int) { previous = -1, next = -2, last = -3, _ }`; rustty `surface.rs:11729 binding::ScopedSurface::GotoTab(v) => apprt::action::GotoTab(v as i32)` with `binding.rs:966 GotoTab(usize)` and `action.rs:904 pub struct GotoTab(pub i32)`.

**Suggested fix:** Use `i32::try_from(v).expect(...)` (checked, matching the safe-build trap) or clamp/reject at binding parse time.

#### S43-F5 — Action detection accepts bare names and never reports InvalidAction/MultipleActions

- **Status:** ✅ CONFIRMED
- **Kind:** behavioral-difference · **Severity:** low (both lenses corrected medium → low: build-data generator, no artifact bytes differ)
- **Oracle:** `src/main_build_data.zig:30-33`
- **Rustty:** `xtask/src/main_build_data.rs:93-104, 141-148`

The reference uses `cli.action.detectArgs(Action, ...)`, which only recognises arguments starting with `+`, returns `error.InvalidAction` for an unknown `+name`, and `error.MultipleActions` when two `+` arguments are present; a missing action is `error.NoAction`. The port's `Action::detect` strips an optional `+` (so a bare `bash` is accepted), and `run` does `args.filter_map(detect).next()`, so an unknown `+foo` is silently skipped and `+bash +zsh` silently runs the first instead of erroring.

**Evidence:** ghostty `main_build_data.zig:32-33` `const action_ = try cli.action.detectArgs(Action, alloc, init.minimal.args); const action = action_ orelse return error.NoAction;` (`src/cli/action.zig`: `if (arg.len == 0 or arg[0] != '+') continue; if (pending != null) return DetectError.MultipleActions; pending = stringToEnum(...) orelse return DetectError.InvalidAction`). rustty `xtask/src/main_build_data.rs:98-104 let name = arg.strip_prefix('+').unwrap_or(arg);` and `:142 args.filter_map(|a| Action::detect(&a)).next()`.

**Suggested fix:** Mirror `detectArgs` (only `+`-prefixed args, error on unknown name, error on a second `+` arg) — the same local-replica pattern already used at `rustty-bench/src/cli.rs:210-241`.

#### S43-F6 — Release-build `tinyPanicImpl` panic handler not ported

- **Status:** ✅ CONFIRMED
- **Kind:** missing-function · **Severity:** low
- **Oracle:** `src/lib_vt.zig:460-471, 500-531` (handler selection at :493-503 per the presence lens)
- **Rustty:** `crates/rustty-vt-c/src/lib.rs:25-77`

For ReleaseFast/ReleaseSmall libghostty-vt builds the reference installs `std.debug.FullPanic(tinyPanicImpl)`, which writes exactly `panic: <msg>\n` to stderr with a single raw write (no locks, no allocation) and then `@trap()`s. rustty-vt-c ports `NATIVE_FREESTANDING`/`DEBUG_MACHINERY` as consts but documents that "today the std panic handler is what runs", so a panic in a release rustty-vt-c library prints Rust's default `thread '…' panicked at …` message (taking the stderr lock, possibly allocating) rather than the reference line.

**Evidence:** ghostty `lib_vt.zig:466-471` and `:500-531`; rustty `crates/rustty-vt-c/src/lib.rs:50-56, 71-77` — consts defined, no panic hook; `grep -rn 'tiny_panic\|panic: {s}' crates` → no hits.

**Suggested fix:** Install a `std::panic::set_hook` in the C-ABI crate when `!DEBUG_MACHINERY` that writes `panic: <msg>\n` via the raw stderr writer and aborts.

#### S43-F7 — Quoted reference comment for the Keymap selector is misquoted

- **Status:** ✅ CONFIRMED
- **Kind:** other · **Severity:** low
- **Oracle:** `src/input.zig:39-40`
- **Rustty:** `crates/rustty-input/src/lib.rs:41-43`

The port's PORT NOTE quotes the reference comment as "We could implement it in theory for XKB too on Linux but we don't as of writing this." whereas the reference at the pin reads "…but we don't need it right now." Comment-only drift; the cfg-gated `Keymap` selection is behaviourally identical.

**Evidence:** ghostty `src/input.zig:39-40` vs rustty `crates/rustty-input/src/lib.rs:42-43`; `grep 'as of writing this'` over `ghostty/src` returns no hits, so the phrase is not from another oracle location.

**Suggested fix:** Update the quoted comment text to match the pinned reference.

#### S44-F4 — `log_with` applies the release-mode debug-level filter only to the stderr sink

- **Status:** ⚠️ DISPUTED (semantic lens refuted; presence lens confirmed)
- **Kind:** missing-branch · **Severity:** low
- **Oracle:** `src/main_ghostty.zig:129-167, 208-211`
- **Rustty:** `crates/rustty-core/src/logging.rs:55-90`

In the reference, `std_options.log_level = .info` in non-Debug builds means `std.log.debug` calls are filtered by std.log *before* `logFn` is invoked, so in release builds a debug message reaches neither the macOS unified-log sink nor stderr. The port's `log_with` executes the macOS unified-logging block first (`logging.rs:55-78`) and only afterwards checks `!cfg!(debug_assertions) && level == Debug` (`:83-85`) for the stderr block. A `Level::Debug` call in a release build would therefore emit to unified logging where the reference emits nothing.

**Evidence:** `main_ghostty.zig:208-211 .log_level = switch (builtin.mode) { .Debug => .debug, else => .info }` (global filter) vs `logging.rs:55-78` (macOS block, unconditional on level) followed by `:83-85` (debug gate only before the stderr block).

**Suggested fix:** Move the `!cfg!(debug_assertions) && level == Debug` early-return to the top of `log_with`, before the macOS block.

**Refuting lens — semantic (verbatim):** "The ordering difference is real in the source (logging.rs:55-78 macOS block precedes the :83-85 debug gate, whereas the reference's std.log filters debug before logFn in non-Debug builds per main_ghostty.zig:208-211), but it is confined to a path no caller reaches: grep for `Level::Debug` / `LogLevel::Debug` across crates finds only the enum definitions, the as_text arm (logging.rs:26), the LogType mapping (logging.rs:65), the gate itself (:83), a test (:135), and the bin's level mapping in main_rustty.rs:449 whose LogLevel::Debug is never constructed at a call site (the only sites use Warn/Info/Err); rustty-vt-c and rustty-wasm have their own sinks and never call rustty_core::logging. Per the lens rule (difference confined to unreachable code, latent only), refuted."

#### S44-F5 — `unimplemented!()` GUI-launch leaf remains in production `main` for a gtk-feature-less Linux/FreeBSD build

- **Status:** ✅ CONFIRMED
- **Kind:** stub-or-unimplemented · **Severity:** low
- **Oracle:** `src/main_ghostty.zig:77-115`
- **Rustty:** `crates/rustty/src/main_rustty.rs:303-313`

After the `.none` usage-banner exit and the cfg-gated `run_gtk` arm, `main` falls through to `unimplemented!("GUI launch has no compiled backend in this build …")`. The reference has no such state: `apprt.Runtime` is exactly `{none, gtk}` (`apprt/runtime.zig:5-25`) and both arms are handled. The stub is reachable only when rustty is built on Linux/FreeBSD with `--no-default-features` (gtk off) — a configuration with no oracle equivalent — but it is a production-code `unimplemented!()` per the audit rules.

**Evidence:** `main_rustty.rs:308-313 unimplemented!("GUI launch has no compiled backend in this build (app_runtime={:?}) …")` vs `main_ghostty.zig:99-115`, which always constructs `apprt.App` for any non-`.none` runtime.

**Suggested fix:** Make the gtk feature mandatory on Linux/FreeBSD (a build-time `compile_error!` guard) so the runtime stub cannot exist, or exit with a diagnostic instead of panicking.

#### S44-F6 — wasm `std_options.log_level`: the `.ReleaseSmall => .warn` arm is collapsed and `LOG_LEVEL` is never enforced

- **Status:** ⚠️ DISPUTED (semantic lens refuted; presence lens confirmed with low confidence)
- **Kind:** different-constant · **Severity:** low
- **Oracle:** `src/main_wasm.zig:14-26`
- **Rustty:** `crates/rustty-wasm/src/lib.rs:260-300`

The reference's `std_options.log_level` (Debug⇒debug, ReleaseSmall⇒warn, else⇒info) is a filter std.log applies before calling `logFn`. The port defines `LOG_LEVEL` (Debug under `debug_assertions`, else Info) but (a) drops the `.warn` level for size-optimized builds and (b) nothing consumes `LOG_LEVEL`: the crate is `#![allow(dead_code)]` and the freestanding sink `rustty_os::wasm::log::freestanding::log` (`rustty-os/src/wasm/log.rs:58-73`) takes pre-formatted text with no level comparison, so no std.log-style level filtering happens in the wasm artifact at all.

**Evidence:** `main_wasm.zig:18-22` vs `crates/rustty-wasm/src/lib.rs:276-284` (`pub const LOG_LEVEL`) with no consumer anywhere in `crates/rustty-wasm` or `crates/rustty-os/src/wasm` (grep returns only the definition).

**Suggested fix:** Have the wasm log entry point compare the message level against `LOG_LEVEL` before formatting/emitting, and add a `release-small`-style cfg mapping to Warn if a ReleaseSmall wasm build is intended.

**Refuting lens — semantic (verbatim):** "Both halves fail to show an observable difference. (a) The `.ReleaseSmall => .warn` arm: Rust exposes no ReleaseSmall cfg (opt-level=\"z\" is a profile, not a cfg predicate), and Zig-build-mode mapping is the 'replaced' build-plumbing area; rustty-wasm/src/lib.rs:252-282 documents the collapse and matches the existing build_config mode precedent. (b) The missing level filter: the reference's own sink (os/wasm/log.zig:25-53, Freestanding.log) does no level check either — filtering lives in std.log, which Rust lacks — and the only callers of rustty_os::wasm::log::freestanding::log in the main_wasm-linked tree are crates/rustty-config/src/wasm.rs:52 (\"error\") and crates/rustty-font-shaper/src/face/web_canvas.rs:863 (\"warning\"); both levels pass every reference filter (.debug/.info/.warn), so no message that the reference would suppress is emitted. Latent/inconclusive -> refuted."

## Performance-only differences (not findings)

- `searchCallback_.viewport_matches`: the reference's shallow `alloc.dupe` + per-element in-place `clone` is collapsed into a single clone-into-Vec pass (`surface.rs` ~4160); identical elements and failure points.
- `handleMessage .pwd_change`: `std.heap.stackFallback(256, alloc)` + `dupeZ` reproduced as a 256-byte stack buffer with heap fallback (`surface.rs` ~3345); same bytes reach `performAction`.
- All `ArenaAllocator` uses (DerivedConfig, childExitedAbnormally, searchCallback_, mouseRefreshLinks) collapse into owned Vec/Box values freed on drop; allocation strategy only.
- Surface id: `std.Random.IoSource` u64 draw replaced by `rustty-os` `crypto_random_bytes` over 8 native-endian bytes; same non-zero-retry loop.
- Thread names set via `std::thread::Builder::name` before spawn instead of post-spawn `setName` (whose failure the reference ignores).
- `copySelectionToClipboards`: Zig arena + 2-slot stack `contents_buf` → per-buffer `Vec<u8>` and a `Vec<(mime, Vec<u8>)>`; same mime/data pairs, order, and `.mixed` HTML background/foreground nulling. `formatter.Options.codepoint_map` shallow MultiArrayList header copy → `copy_options` Vec clone.
- `endKeySequence`: queued `WriteReq` list is `mem::take`n for the flush/drop loop and restored before the `mem` switch (Rust `WriteReq` is move-only); `.retain` = `Vec::clear`, `.free` = `drop(take)`. Same observable `queueIo` sequence.
- `encodeKey`: fixed writer is a `std::io::Cursor` over the Small array (buffer-full == `error.WriteFailed`); allocating fallback uses `Vec::with_capacity(max(utf8.len*2, MAX*2))` + `into_boxed_slice` (== `toOwnedSlice` shrink).
- `keyCallback`/`maybeHandleBinding`: Zig `Binding.Set.Entry` key/value pointer pair → `(*const Set, index)` with `value_at`/`key_at`; same lookup order (sequence set → tables inner-most first → root).
- `scrollCallback`/`contentScaleCallback`: `@intFromFloat` → `isize_from_float` (debug_assert trap in checked builds, saturating `as` in release) and `as u16` for DPI; identical for in-range inputs.
- `mouseButtonCallback` extend_selection: `click_time.untilNow(...).toNanoseconds() <= mouse_interval` → `Instant::now().duration_since(click_time)`, which saturates at 0 where the Zig signed value would be negative — both take the `break :extend_selection` branch.
- `mouseReport`: `Options.fromTerminal` reads only `flags.mouse_event`/`flags.mouse_format`; the Rust `from_terminal(mouse_event, mouse_format, size)` takes those two reads directly.
- `linkAtPin`: Zig by-value `searchIterator(link.regex)` copies the StringMap header and oni handle; Rust moves the owned StringMap in and recovers it via `strmap = it.map`, copying the Regex handle struct.
- `preeditCallback`: `Utf8View.init` + `nextCodepoint` == `str::from_utf8` + `chars()`; codepoints ArrayList → Vec moved into `Preedit`.
- `performBindingAction .text`: `stackFallback(256)` parse buffer → always-heap `Vec` (`surface.rs:10690`); identical output.
- `performBindingAction .cursor_key`: reference queues `.write_stable` (aliases config storage); port copies the bytes via `Message::writeReq` because the Rust `CursorKey` owns its Vec — identical bytes reach the pty.
- `set_surface_title`/`set_tab_title`/`set_window_title` and `write_screen_file .copy`: `dupeZ` NUL-terminated copies collapse to borrowed `&[u8]` passed synchronously; same payload.
- `showDesktopNotification`: dedup digest uses std `DefaultHasher` (SipHash) instead of Wyhash(0); the digest is process-local and never exposed, so throttling is identical.
- `kittyClipboardStatus`/`completeKittyClipboardRead`/`completeClipboardPasteEvent`: `aw.toOwnedSlice()` → `aw.written().to_vec().into_boxed_slice()` (one extra copy).
- `completeClipboardReadOSC52`: the 7-byte OSC prefix is written byte-by-byte rather than via `bufPrint`; identical buffer contents.
- `getProcessInfo`'s comptime-typed return split into `getForegroundPid()`/`getTtyName()` accessors.
- crc32c: comptime backend selection → runtime `is_aarch64_feature_detected!("crc")` / `is_x86_feature_detected!("sse4.2")` with `core::arch` intrinsics instead of inline asm; identical results (same KAT/differential tests). Test PRNG is SplitMix64 instead of Zig DefaultPrng — both sides of each assertion see the same bytes.
- fastmem: `move` uses `core::ptr::copy` and the rotate helpers `slice::copy_within` instead of libc memmove.
- file_type: `inline for` over the comptime table → runtime loop over a const slice; same order and semantics.
- helpgen: doc extraction scans Rust source lines rather than a Zig AST token stream; the `[3..]` strip and common-prefix dedent are byte-identical.
- global: Zig GPA-vs-c_allocator selection and `std.Io.Threaded` setup collapse to Rust's process allocator/std (`seam::Allocator = ()`); no observable behaviour beyond debug leak diagnostics.
- `macosLogger` caches one `os_log` object per scope in an atomic for the process lifetime; `rustty-core/src/logging.rs:70-77` calls `Log::create`/`release` per log call. Identical unified-log output. **Superseded (parity tail S-8d, N280): the cache is now ported (`logging.rs` `macos_logger`).**
- `quirks_memset`: hand-vectorized strong `memset` export (aarch64 `dc zva` path) replaced by the platform libc memset rustc links; identical fill semantics.
- tripwire: type-static `std.EnumMap` per comptime module vs per-instance `HashMap`; only the order of the "untripped point=" warning lines in `end` can differ.
- math: `@Vector(4, f32)` vs `[f32; 4]` — same element values.
- pty: Linux `foreground_pid` uses the raw `linux.tcgetpgrp` syscall wrapper; `pty.rs:421-432` calls libc `tcgetpgrp` — same result.
- `SurfaceMouse` by-value self and pty `winsize`/`Mode` packed/extern structs → plain `Copy` structs with `#[repr(C)]` where ABI-visible.

## Refuted candidates

None — no finding in this area was refuted by both verifier lenses. Two findings were refuted by one lens each and are retained above as DISPUTED:

- **S44-F4 — `log_with` debug-filter ordering** — semantic lens: real source ordering difference, but no caller anywhere constructs `Level::Debug`, so it is unreachable/latent.
- **S44-F6 — wasm `LOG_LEVEL` unenforced / `.ReleaseSmall` arm dropped** — semantic lens: the reference's own wasm sink does no level check either (filtering lives in std.log), and the only two live callers pass levels that every reference filter admits, so no message the reference would suppress is emitted.

No findings exceeded the verification cap, so there are no UNVERIFIED entries.
