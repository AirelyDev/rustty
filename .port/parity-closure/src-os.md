# Parity closure checklist — src/os

17 open finding(s): 0 high, 2 medium, 15 low. Full evidence per row is in [`docs/parity-audit/src-os.md`](../../docs/parity-audit/src-os.md).

A row is CLOSED only when its fix has a test that fails without it (or a written, reviewed justification for why no test is possible).


## Medium (2)

- [ ] **S112-F1** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** GHOSTTY_RESOURCES_DIR read as UTF-8 String; non-UTF-8 value becomes an error instead of the raw path
  - **Oracle:** `src/os/resourcesdir.zig:51-58, 113-120` → **Rustty:** `crates/rustty-os/src/resourcesdir.rs:95-110, 169-184, 288-291`
  - **Audit's suggested fix:** Read the variable as bytes (`std::env::var_os` / rustty_global environ map), make `app_path`/`host_path` `Vec<u8>`/`OsString`, treat NotPresent as missing and never fail on non-UTF-8.
- [x] **S113-F1** · CONFIRMED · `LOG` · behavioral-difference
  - **What:** displayRealized: prepareContext failure aborts re-realize instead of warn-and-continue
  - **Oracle:** `src/renderer/OpenGL.zig:237-250` → **Rustty:** `crates/rustty-renderer/src/generic/linux_impl.rs:1725-1748`
  - **Audit's suggested fix:** Match the oracle: on the windowed path, call `prepare_context_windowed()`, log a warning on `Err` and fall through to the draw_mutex lock + rebuild instead of returning `Err(GlFailed)`.
  - **Disposition (task-14c-3):** CLOSED — the reference body now lives where the reference puts it, as `OpenGL::display_realized(&self) -> ()` (`opengl/opengl_impl.rs`), warning on `target: LOG_SCOPE` (`"opengl"`, OpenGL.zig:34) and CONTINUING; `generic/linux_impl.rs::display_realized` calls it unconditionally and no longer returns `Err(GlFailed)`. The ledger's oracle anchor `OpenGL.zig:237-250` is exact at pin `20abdb50a`. `{err:?}` renders the payload, per this crate's convention for the reference's `{}`-on-an-error. Tests: `opengl_impl::tests::display_realized_warns_and_continues_when_context_prep_fails` (the warn + the `()` return) and `display_realized_is_silent_on_the_offscreen_path` (no diagnostic on the success path). Verified on the Fedora aarch64 VM under `RUSTTY_REQUIRE_GL=1`; negative-controlled.

## Low (15)

- [ ] **S112-F10** · DISPUTED · `INDIV` · missing-function
  - **What:** Several windows.zig declarations are omitted or relocated out of the bridge module
  - **Oracle:** `src/os/windows.zig:12-15, 23-36, 44-55, 80, 105-109, 216-223, 240-254, 257-260` → **Rustty:** `crates/rustty-os/src/windows.rs:15-22, 183-189, 214-220, 349-353`
  - **Audit's suggested fix:** Transcribe the remaining declarations (or re-export the relocated ones) so rustty-os::windows carries the full reference surface, and make ProcThreadAttributeNumber an open newtype.
- [ ] **S112-F11** · CONFIRMED · `INDIV` · missing-error-path
  - **What:** homedir.home errors collapsed into NoHomeDir
  - **Oracle:** `src/os/xdg.zig:90-100` → **Rustty:** `crates/rustty-os/src/xdg.rs:155-169`
  - **Audit's suggested fix:** Add a `Home(homedir::Error)` variant to DirError and propagate it.
- [ ] **S112-F12** · DISPUTED · `ABI` · behavioral-difference
  - **What:** align_pointer can overflow before its overflow check
  - **Oracle:** `src/os/mach.zig:107` → **Rustty:** `crates/rustty-os/src/mach.rs:255-269`
  - **Audit's suggested fix:** Use `addr.checked_add(align - 1)` and return None on overflow.
- [ ] **S112-F2** · CONFIRMED · `INDIV` · missing-error-path
  - **What:** maybeDir drops the bufPrint NoSpaceLeft error path
  - **Oracle:** `src/os/resourcesdir.zig:131-148` → **Rustty:** `crates/rustty-os/src/resourcesdir.rs:200-224`
  - **Audit's suggested fix:** Return an error (add a NoSpaceLeft variant to ResourcesDirError) when `bytes.len() > buf.len()`.
- [ ] **S112-F3** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** Exe-path walk uses lossy to_str() and Path::parent instead of byte dirname
  - **Oracle:** `src/os/resourcesdir.zig:69-109` → **Rustty:** `crates/rustty-os/src/resourcesdir.rs:117-165, 305-307`
  - **Audit's suggested fix:** Operate on `OsStr` bytes (`as_bytes()` on unix) for maybeDir's base, and stop when the parent is empty.
- [ ] **S112-F4** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** expand takes `&str` cmd (reference: `[]const u8`) and the Windows OsStr shim is lossy
  - **Oracle:** `src/os/path.zig:9-19` → **Rustty:** `crates/rustty-os/src/path.rs:33-41, 208-211`
  - **Audit's suggested fix:** Take `cmd: &[u8]`; on Windows convert bytes via WTF-8/`OsString::from_wide` equivalents rather than defaulting to an empty path.
- [x] **S112-F5** · CONFIRMED · `ALLOC` · missing-branch
  - **What:** Flatpak host-command block additionally gated on cargo feature `flatpak`; error mapping and NUL handling differ
  - **Oracle:** `src/os/passwd.zig:58-124` → **Rustty:** `crates/rustty-os/src/passwd.rs:84-162`
  - **Audit's suggested fix:** Tie `build_config::flatpak()` to the same cargo feature (or drop the extra cfg), carry real OS errors in `Error::Os`, and truncate/handle interior NUL instead of unwrap.
  - **Disposition (task L-4):** CLOSED — the host-command block run inside a real Flatpak sandbox (`flatpak build`, rvm): `passwd::tests::flatpak_get_reads_the_host_entry` and `passwd::tests::flatpak_spawn_failure_returns_the_host_command_error_and_closes_the_pty`, with negative controls; the slave-fd leak it found is UD320 (R13). Evidence in the LEDGER owner cell.
- [ ] **S112-F6** · CONFIRMED · `INDIV` · missing-error-path
  - **What:** macOS pipe fallback leaks both fds when fcntl(F_SETFD) fails
  - **Oracle:** `src/os/pipe.zig:9-11` → **Rustty:** `crates/rustty-os/src/pipe.rs:55-69`
  - **Audit's suggested fix:** Close both fds before returning the error in the fcntl failure arms.
- [ ] **S112-F7** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** openThread stderr logging is unbounded per line and thread-spawn failure panics
  - **Oracle:** `src/os/open.zig:66-67, 79-101` → **Rustty:** `crates/rustty-os/src/open.rs:166-167, 196-221`
  - **Audit's suggested fix:** Use `std::thread::Builder::new().spawn()` and map the error into `OpenError`; optionally cap line reads at 256 bytes to match chunking.
- [ ] **S112-F8** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** connect() addrlen and comm-read error arm differ from the reference
  - **Oracle:** `src/os/systemd.zig:37-53, 136-149` → **Rustty:** `crates/rustty-os/src/systemd.rs:51-57, 167-185`
  - **Audit's suggested fix:** Pass `size_of::<sockaddr_un>()` (offset + sun_path.len()) as addrlen and keep the two log arms if exact parity is wanted.
- [ ] **S112-F9** · CONFIRMED · `INDIV` · behavioral-difference
  - **What:** Port/percent number parsing does not accept Zig parseInt's '_' digit separators
  - **Oracle:** `src/os/uri.zig:21-91` → **Rustty:** `crates/rustty-os/src/uri.rs:321-336`
  - **Audit's suggested fix:** Implement Zig's parseWithSign semantics (skip '_' after the first digit, '+' prefix, '-' rejected for unsigned) in `parse_int_u16` / `parse_int_u8_hex`.
- [x] **S113-F2** · CONFIRMED · `LOG` · missing-branch
  - **What:** Metal log.debug/log.warn call sites preserved only as comments
  - **Oracle:** `src/renderer/Metal.zig:85-88,174,443` → **Rustty:** `crates/rustty-renderer/src/metal/metal_impl.rs:250-251,844-846,923-926`
  - **Audit's suggested fix:** Route the three sites through the crate's log macros (or the rustty log facade) with the oracle's message text.
  - **Disposition (task-14c-2):** CLOSED — all 3 of `Metal.zig`'s live `log.*` calls (`:85` debug, `:174` warn, `:443` warn) are live in `metal/metal_impl.rs` under `LOG_SCOPE = "metal"`. `:174`'s emission lives in `display_callback`, the port's `extern "C"` trampoline, not in the `loopEnter` block comment that also quotes it (the double-count trap). `:85`'s `default_storage_mode` renders `Shared`/`Managed` via a derived `Debug` where Zig's `{}` renders the fully-qualified enum name — the known/accepted rendering difference.
- [x] **S113-F3** · CONFIRMED · `LOG` · missing-branch
  - **What:** Overlay rect-draw error warnings are swallowed silently
  - **Oracle:** `src/renderer/Overlay.zig:195-197,245-247,291-293` → **Rustty:** `crates/rustty-renderer/src/overlay.rs:329-341,461-470,509-518`
  - **Audit's suggested fix:** Emit the three warnings through the crate's log macros with the oracle text and the `OverlayError` Display.
  - **Disposition (task-14c-2):** CLOSED — all three cited anchors are live in `overlay.rs`. The ledger is RIGHT and this task's brief was wrong to say there were two: `Overlay.zig:196` is a `std.log.warn`, i.e. the `.default` scope, so it emits with `target: ""` per DECISION-logging.md §6r while `:246`/`:292` use `renderer_overlay`. `OverlayError` has no `Display` impl (the audit's suggestion assumed one); the payloads render through the derived `Debug`, and `#[allow(dead_code)]` stays because rustc does not count a `Debug` field read as a use.
- [x] **S113-F4** · CONFIRMED · `LOG` · missing-branch
  - **What:** present: re-enable GL_FRAMEBUFFER_SRGB failure is swallowed without the oracle's log.err
  - **Oracle:** `src/renderer/OpenGL.zig:307-309` → **Rustty:** `crates/rustty-renderer/src/opengl/opengl_impl.rs:406-410,499-501`
  - **Audit's suggested fix:** Call `log_err!("Error re-enabling GL_FRAMEBUFFER_SRGB, err={err}")` in the `is_err()` arm.
  - **Disposition (task-14c-3):** CLOSED — both cited rustty anchors are live. The audit's suggested `log_err!` was the eprintln stand-in; this stage routed those macros through the `log` facade first, and these two sites emit `log::error!(target: LOG_SCOPE, ...)` directly. Both arms had discarded the payload with `.is_err()`; they bind it now. Test: `opengl_impl::tests::present_logs_when_the_srgb_re_enable_fails`, whose fixture is real GL — a blit into a surfaceless context's incomplete FBO 0 raises GL_INVALID_FRAMEBUFFER_OPERATION, which the very next `draw::enable` collects. NOTE for the re-audit: `present` returns `Ok` in that case, because `glBlitFramebuffer` is unchecked on BOTH sides (OpenGL.zig:316-327 has no `try`) — the restored `log.err` is the only report the failure ever gets.
- [x] **S113-F5** · CONFIRMED · `ALLOC` · behavioral-difference
  - **What:** wasm log: allocation-failure path returns silently in Zig but the Rust port always heap-formats
  - **Oracle:** `src/os/wasm/log.zig:32-50` → **Rustty:** `crates/rustty-os/src/wasm/log.rs:58-87`
  - **Audit's suggested fix:** Format with `core::fmt::Write` into the stack buffer first (e.g. a fixed-capacity writer) and only fall back to a `try_reserve`-guarded String, returning on failure, to preserve the OOM-tolerant path.
