# Parity closure checklist — src

19 open finding(s): 1 high, 7 medium, 11 low. Full evidence per row is in [`docs/parity-audit/src.md`](../../docs/parity-audit/src.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## High (1)

- [ ] **S43-F1** · CONFIRMED · `INDIV` · missing-branch
  - **What:** global.init does not re-sync the captured environment after ensureLocale()
  - **Oracle:** `src/global.zig:204-209, 307-322` → **Rustty:** `crates/rustty-apprt/src/lib.rs:329-337`
  - **Audit's suggested fix:** Add `rustty_global::sync_environ();` immediately after the `ensureLocale()` call in `init_with_action`.

## Medium (7)

- [ ] **S42-F1** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** completeClipboardPasteEvent queues with MutexState::Locked after already unlocking the renderer mutex
  - **Oracle:** `src/Surface.zig:6222-6247` → **Rustty:** `crates/rustty-core/src/surface.rs:13235-13251`
  - **Audit's suggested fix:** Either move the explicit unlock to after the queueIo call (keeping MutexState::Locked, matching the oracle's scope), or pass MutexState::Unlocked when calling after the unlock; the oracle-faithful option is the former.
- [ ] **S43-F2** · CONFIRMED · `LOG` · missing-branch
  - **What:** xev.detect() is never run and the libxev backend log line is hardcoded
  - **Oracle:** `src/global.zig:160-164, 178` → **Rustty:** `crates/rustty-apprt/src/lib.rs:229-233, 292-302`
  - **Audit's suggested fix:** On targets where `rustty_xev::DYNAMIC` is true (Linux/FreeBSD), call `rustty_xev::detect()` and warn with the reference text on failure; log `rustty_xev::backend()` in the 'libxev default backend' line.
- [ ] **S43-F3** · CONFIRMED · `INDIV` · missing-branch
  - **What:** InitOpts.tool arm not ported: the bench binary skips global init entirely
  - **Oracle:** `src/global.zig:37-52, 129-136` → **Rustty:** `crates/rustty-global/src/lib.rs:421-431`
  - **Audit's suggested fix:** Have rustty-bench's cli::main run the shared bootstrap with action detection skipped (e.g. `rustty_apprt::init_with_action(false)` or a tool-arm entry that performs the same side effects), matching benchmark/cli.zig:55.
- [ ] **S43-F4** · CONFIRMED · `INDIV` · missing-function
  - **What:** global.deinit()'s crash.deinit() flush is never executed
  - **Oracle:** `src/global.zig:232-251` → **Rustty:** `crates/rustty-global/src/lib.rs:478-506`
  - **Audit's suggested fix:** Call `rustty_crash::deinit()` (and the rest of the reference deinit sequence) on the GTK normal-exit path before the process exits, mirroring `defer global.deinit()`.
- [ ] **S44-F1** · CONFIRMED · `INDIV` · missing-error-path
  - **What:** WindowsPty::open leaks pipe handles on every failure path (reference errdefers dropped)
  - **Oracle:** `src/pty.zig:383-450` → **Rustty:** `crates/rustty-os/src/pty.rs:603-688`
  - **Audit's suggested fix:** Add a Windows `OpenErrdefer`-style guard that tracks in_pipe / in_pipe_pty / out_pipe / out_pipe_pty and closes the ones created so far when dropped armed; disarm before `Ok(pty)`.
- [ ] **S44-F2** · CONFIRMED · `INDIV` · missing-branch
  - **What:** `defer global.deinit()` has no counterpart on the GUI (run_gtk) path — crash-log flush / resources_dir deinit never run
  - **Oracle:** `src/main_ghostty.zig:58,99-115` → **Rustty:** `crates/rustty/src/main_rustty.rs:171-175,389-411`
  - **Audit's suggested fix:** Add a `rustty_apprt::deinit()` (or core-side equivalent) that performs the global.zig deinit body (crash flush, resources_dir deinit, tmp-dir release) and call it in `run_gtk` after `core_app.destroy()` and before the exit on both the Ok and Err arms (the reference `try` still runs the defer on error).
- [ ] **S44-F3** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** pty log lines bypass the logFn sinks (stderr gate + macOS unified logging) and use a different format
  - **Oracle:** `src/pty.zig:7,158,168,258,307` → **Rustty:** `crates/rustty-os/src/pty.rs:26-37,258-271,402-405,466-469`
  - **Audit's suggested fix:** Route rustty-os logging through a tier-appropriate sink hook (e.g. a function-pointer/trait seam set by rustty-core at init) that applies the Logging.stderr/macos gates and the `level(scope): ` format, or at minimum consult the shared logging config.

## Low (11)

- [ ] **S40-F1** · CONFIRMED · `INDIV` · other
  - **What:** mouse_interval arithmetic width differs (u32 in Zig vs u64 in Rust)
  - **Oracle:** `src/Surface.zig:385` → **Rustty:** `crates/rustty-core/src/surface.rs:928-929`
  - **Audit's suggested fix:** None required functionally; optionally document the deliberate widening in the PORT NOTE.
- [ ] **S41-F1** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** All log.info/warn/err/debug sites in the range are preserved only as comments; no log output is emitted
  - **Oracle:** `src/Surface.zig:2193, 2205, 2219, 2312, 2352, 2426, 2470-2477, 2698, 2854, 2996-2999, 3019-3022, 3033, 3061, 3084-3087, 3113-3116, 3139-3142, 3200, 3350, 3395, 3671, 3773, 3855, 3862, 3919, 3925, 3936, 3989, 4231-4234, 4283, 4342-4345` → **Rustty:** `crates/rustty-core/src/surface.rs:251-254 (convention), 5398, 5415-5418, 5445-5453, 5690-5697, 5741, 5826-5830, 5876, 5989-5997, 6289, 6461, 6753-6757, 6787-6791, 6935-6938, 7009, 7108, 7233-7239, 7384, 7681, 8004, 8206, 8412, 8471-8475, 8586-8589, 8617-8620, 9314, 9366, 9424, 9483`
  - **Audit's suggested fix:** Add a thin `log` (or tracing) scoped logger for the surface module and re-enable the commented log calls verbatim (same level, same format string), or document the omission as an accepted deviation if the port deliberately ships without a logging backend.
- [ ] **S42-F2** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** Every log.warn/info/debug/err in the range is commented out; surface.rs emits no logging at all
  - **Oracle:** `src/Surface.zig:4424, 4448, 4767, 4779, 4848, 4859-4862, 4874, 4891, 5027, 5030, 5055, 5063, 5075, 5105, 5120, 5131, 5142, 5465, 5521, 5531, 5538-5541, 5557-5560, 5563, 5586-5589, 5612, 5883, 5986, 6029, 6044, 6056, 6094-6097, 6155, 6162, 6305, 6354, 6473, 6490` → **Rustty:** `crates/rustty-core/src/surface.rs:9640-14060 (every `// log.*` line)`
  - **Audit's suggested fix:** Route the commented log sites through rustty-core's logging module (logging.rs log_with) at the same levels and with the same message text.
- [ ] **S42-F3** · CONFIRMED · `DEBUGONLY` · behavioral-difference
  - **What:** mode_string only ever reports "Debug" or "ReleaseFast" (never ReleaseSafe/ReleaseSmall)
  - **Oracle:** `src/build_config.zig:24-28` → **Rustty:** `crates/rustty-build-config/src/lib.rs:123-129`
  - **Audit's suggested fix:** Distinguish at least optimized+debug_assertions ("ReleaseSafe") from unoptimized ("Debug") using cfg!(opt_level) / a build.rs-captured OPT_LEVEL env, and map opt-level 's'/'z' to "ReleaseSmall".
- [ ] **S42-F4** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** goto_tab index conversion wraps instead of trapping for values above i32::MAX
  - **Oracle:** `src/Surface.zig:5317-5331` → **Rustty:** `crates/rustty-core/src/surface.rs:11673-11688`
  - **Audit's suggested fix:** Use `i32::try_from(v).expect(...)` (checked, matching the safe-build trap) or clamp/reject at binding parse time.
- [ ] **S43-F5** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** Action detection accepts bare names and never reports InvalidAction/MultipleActions
  - **Oracle:** `src/main_build_data.zig:30-33` → **Rustty:** `xtask/src/main_build_data.rs:93-104, 141-148`
  - **Audit's suggested fix:** Mirror detectArgs: only consider `+`-prefixed args, error on an unknown `+name`, error on a second `+` argument (the same local-replica pattern rustty-bench/src/cli.rs:210-241 already uses).
- [ ] **S43-F6** · CONFIRMED · `INDIV` · missing-function
  - **What:** Release-build tinyPanicImpl panic handler not ported (different panic output)
  - **Oracle:** `src/lib_vt.zig:460-471, 500-531` → **Rustty:** `crates/rustty-vt-c/src/lib.rs:25-77`
  - **Audit's suggested fix:** Install a `std::panic::set_hook` (or panic=abort hook) in the C-ABI crate when `!DEBUG_MACHINERY` that writes `panic: <msg>\n` via the raw stderr writer and aborts.
- [ ] **S43-F7** · CONFIRMED · `INDIV` · other
  - **What:** Quoted reference comment for the Keymap selector is misquoted
  - **Oracle:** `src/input.zig:39-40` → **Rustty:** `crates/rustty-input/src/lib.rs:41-43`
  - **Audit's suggested fix:** Update the quoted comment text to match the pinned reference.
- [ ] **S44-F4** · DISPUTED · `LOG` · missing-branch
  - **What:** log_with applies the release-mode debug-level filter only to the stderr sink; macOS sink sees Debug messages
  - **Oracle:** `src/main_ghostty.zig:129-167,208-211` → **Rustty:** `crates/rustty-core/src/logging.rs:55-90`
  - **Audit's suggested fix:** Move the `!cfg!(debug_assertions) && level == Debug` early-return to the top of `log_with`, before the macOS block, so the std.log-level filter applies to both sinks.
- [ ] **S44-F5** · CONFIRMED · `INDIV` · stub-or-unimplemented
  - **What:** `unimplemented!()` GUI-launch leaf remains in production main for a gtk-feature-less Linux/FreeBSD build
  - **Oracle:** `src/main_ghostty.zig:77-115` → **Rustty:** `crates/rustty/src/main_rustty.rs:303-313`
  - **Audit's suggested fix:** Make the gtk feature mandatory on Linux/FreeBSD (compile_error!-style `#[cfg]` guard at build time) so the runtime stub cannot exist, or exit with a diagnostic instead of panicking.
- [ ] **S44-F6** · DISPUTED · `DEBUGONLY` · different-constant
  - **What:** wasm std_options.log_level: `.ReleaseSmall => .warn` arm collapsed and LOG_LEVEL is never enforced by the log sink
  - **Oracle:** `src/main_wasm.zig:14-26` → **Rustty:** `crates/rustty-wasm/src/lib.rs:260-300`
  - **Audit's suggested fix:** Have the wasm log entry point compare the message level against LOG_LEVEL before formatting/emitting, and add a `release-small`-style cfg (e.g. `opt-level = "z"` profile feature) mapping to Warn if a ReleaseSmall wasm build is intended.
