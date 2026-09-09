# Parity audit — src/os

Two shards (112 `src/os`, 113 `src/os/wasm` + the renderer units it carried) audited 24 oracle units against their rustty counterparts, reading 4,210 lines — the full text of every unit in scope (17 + 7 units fully read, none partial).

17 findings survived verification: **15 CONFIRMED**, **2 DISPUTED**, **0 REFUTED**, 0 UNVERIFIED. By corrected severity: **0 high, 2 medium, 15 low**.

The most important gap is **S113-F1** — `OpenGL.displayRealized` returns `Err(GlFailed)` on a `prepare_context_windowed()` failure and skips the swap-chain/shader rebuild, where the oracle logs a warning and always rebuilds; the port surfaces an error the reference cannot produce and leaves the renderer un-realized. Next is **S112-F1** (`GHOSTTY_RESOURCES_DIR` read as UTF-8 `String`, so a non-UTF-8 value hard-errors instead of being returned as raw bytes). Everything else is error-path collapse, dropped log sites, or unsupported-target surface.

## Coverage

| Oracle unit (path:lines) | rustty counterpart(s) | Status | Note |
| --- | --- | --- | --- |
| `src/os/mach.zig` | `crates/rustty-os/src/mach.rs`, `crates/rustty-os/src/page_allocator.rs` | ported | `VMTag` (open u8 newtype), `make()`, `taggedPageAllocator`, `TaggedPageAllocator` alloc/resize/remap/free/map/unmap/realloc and test `VMTag.make` all present with the same constants/branches; `next_mmap_addr_hint` omitted (perf-only). `page_allocator.rs:87` wires `application_specific_1` on darwin as `PageList.zig` does. |
| `src/os/macos.zig` | `crates/rustty-os/src/macos.rs` | ported | `isAtLeastVersion`, `appSupportDir`, `cacheDir`, `SetQosClassError`, `setQosClass` (0/EPERM/panic), `QosClass` values, `pthread_set_qos_class_self_np`/`pthread_setname_np` externs, `NSSearchPathDirectory`/`DomainMask` via objc2-foundation, `commonDir` (AppleAPIFailed on nil URL / nil path / nil UTF8String), test "cacheDir paths" with the same 5 asserts. File also hosts the `pkg/macos/os/log.zig`+`signpost.zig` ports (outside this shard). |
| `src/os/main.zig` | `crates/rustty-os/src/lib.rs` | ported | All namespaces are pub mods; every "Functions and types" re-export (CFReleaseThread, TempDir, launchedFromDesktop, launchedByDbusActivation, launchedBySystemd, desktopEnvironment, rlimit, fixMaxFiles, restoreMaxFiles, randomTmpPath, isFlatpak, FlatpakHostCommand, home, expandHome, ensureLocale, clickInterval, open, OpenType, pipe, resourcesDir, ResourcesDir, ShellEscapeWriter, getKernelInfo, getConfigEditCommand) present; the `test{}` aggregator is a no-op `#[test]` (Rust compiles submodule tests unconditionally). |
| `src/os/mouse.zig` | `crates/rustty-os/src/mouse.rs` | ported | `clickInterval`: macOS `NSEvent doubleClickInterval` via objc2 `msg_send`, `ceil(*1000) as u32`, `log.err` on missing class, `None` elsewhere. |
| `src/os/open.zig` | `crates/rustty-os/src/open.rs` | ported | `Kind` seam (parity image of `apprt.action.OpenUrl.Kind`), macOS osc8 → `UnsafeOSC8Link`, per-OS argv (xdg-open / rundll32 / open [-t] / ios Unimplemented / compile_error), stdout null + stderr piped, snap `LD_LIBRARY_PATH` removal, detached `openThread` that logs stderr lines and waits; test "macOS OSC 8 links have no generic opener fallback" ported with the same assertion (cfg macos). |
| `src/os/passwd.zig` | `crates/rustty-os/src/passwd.rs` | ported | `Entry` (shell/home/name), `get()`: `getpwuid_r` 1024-byte buf, `res!=0` warn+empty, null `pw_ptr` warn+empty, flatpak host-command block (getent via `/bin/sh -l -c` over a Pty, EIO=EOF, size cap, split-backwards `:` for shell/home), then pw_shell/pw_dir/pw_name dupes; `test{}` with the same two asserts. Windows `@compileError` mapped to item absence (E0425 at any call site). |
| `src/os/path.zig` | `crates/rustty-os/src/path.rs` | ported | `expand(environ_map, cmd)`: slash short-circuit, PATH from map, PATH_MAX buffer with `PathTooLong`, per-entry open (FileNotFound continue / AccessDenied accumulate / else return), stat `kind!=dir && mode&0o111`, `seen_eacces` → AccessDenied; `isExecutable` windows⇒true; 3 tests with the same assertions. |
| `src/os/pipe.zig` | `crates/rustty-os/src/pipe.rs` | ported | `pipe2(O_CLOEXEC)` on Linux/BSD; `pipe()`+`fcntl(F_SETFD, FD_CLOEXEC)` fallback on macOS (matches `compat/fd.zig` pipe2 fallback); Windows `CreatePipe` → `unexpectedError(GetLastError)`. |
| `src/os/resourcesdir.zig` | `crates/rustty-os/src/resourcesdir.rs` | ported | `ResourcesDir {app_path, host_path, deinit, app, host}`, `resourcesDir()`: release-mode env-var first (non-empty), per-OS sentinels (retargeted identity, hex-of-first-byte dir on macOS), walk exe dirnames probing `Contents/Resources` (darwin) and `share`\|`local/share`, debug-mode env fallback, `maybeDir` formats base/sub/suffix and returns base/sub. See S112-F1..F3 for env-var/UTF-8 and bufPrint error-path differences. |
| `src/os/shell.zig` | `crates/rustty-os/src/shell.rs` | ported | `ShellCommandBuilder` (init/deinit/appendArg/toOwnedSlice with NUL), `ShellEscapeWriter` escaping the same 11 bytes, test ShellCommandBuilder (5 blocks, 6 asserts) and shell escape 1-7 with identical expected strings. |
| `src/os/stderr.zig` | `crates/rustty-os/src/stderr.rs` | ported | POSIX arm: raw `write(2)` loop with EINTR retry, any other errno returns. Windows arm resolves the same stderr handle via `GetStdHandle(STD_ERROR_HANDLE)`+`WriteFile` instead of PEB+`NtWriteFile` (best-effort, same observable effect). freestanding/other no-op. `test write` smoke ported. |
| `src/os/string_encoding.zig` | `crates/rustty-os/src/string_encoding.rs` | ported | `printfQDecode` (`$''`/`''` stripping, escape table incl. e/n/r/t/v, DecodeError paths), `urlPercentDecode` (%XX with `src+2>=len` check), `hex()`, `isValidChar` (`' '`,`';'`,`'='` false else isPrint), `urlPercentEncode` via `Component::percentEncode`; tests printf_q 1-10, singles percent (0..255 both cases), percent 1-7 all with the same assertions. |
| `src/os/systemd.zig` | `crates/rustty-os/src/systemd.rs` | ported | `launchedBySystemd` (INVOCATION_ID+JOURNAL_STREAM, ppid==1, `/proc/<ppid>/comm` 16-byte compare trimmed of `\n`), `notify.send` (NOTIFY_SOCKET, empty/prefix/length checks, AF_UNIX SOCK_DGRAM\|CLOEXEC, connect, write w/ short-write warn), `ready()`, `reloading()` with CLOCK_MONOTONIC usec. See S112-F8 for the connect addrlen and collapsed log arm. |
| `src/os/uri.zig` | `crates/rustty-os/src/uri.rs` | ported | `ParseOptions` defaults, `ParseError` set, `parse()` with InvalidPort MAC re-parse, 14-char/4-colon/port<=99 repair, raw_path rewrite; `isValidMacAddress`; the `std.Uri` subset (parse, parseAfterScheme, Component raw/percent_encoded, isEmpty, toRaw, formatRaw, percentEncode) reimplemented; 3 tests with identical assertion sets (24 + 15 + 7). |
| `src/os/wasm.zig` | `crates/rustty-os/src/wasm.rs`, `crates/rustty-build-config/src/lib.rs` | replaced | comptime `isWasm` guard → `cfg(target_arch="wasm32")` module; `shared_mem = rustty_build_config::wasm_shared()`; `alloc = std::alloc::System` (Zig `wasm_allocator` / `testing.allocator`). Extra resurrected `host_owned` module is additive. |
| `src/os/windows.zig` | `crates/rustty-os/src/windows.rs`, `crates/rustty-termio/src/command.rs`, `crates/rustty-xev/src/windows.rs` | partial | Bridge/bindings file. Present: base types, COORD/SECURITY_ATTRIBUTES/STARTUPINFOW/PROCESS_INFORMATION/STARTUPINFOEX, all bit constants used by callers, ProcThreadAttributeValue/PSEUDOCONSOLE, `exp.kernel32` CreatePipe/CreatePseudoConsole/ResizePseudoConsole/ClosePseudoConsole/InitializeProcThreadAttributeList/UpdateProcThreadAttribute/PeekNamedPipe/CreateProcessW/GetComputerNameA/GetTempPathW/VirtualAlloc/VirtualFree/ReadFile/CancelIoEx/TerminateProcess, `unexpectedError`. Missing here (some relocated): OVERLAPPED (`rustty-xev/src/windows.rs`), WaitForSingleObject/GetExitCodeProcess (`rustty-termio/src/command.rs:1697-1698`), `ntdll.NtCreateFile` + ACCESS_MASK/IO_STATUS_BLOCK/NTSTATUS/OBJECT_ATTRIBUTES/UNICODE_STRING, `unexpectedStatus`, FILE_NON_DIRECTORY_FILE, HINSTANCE/LARGE_INTEGER/LPCWSTR/ULONG. See S112-F10. |
| `src/os/xdg.zig` | `crates/rustty-os/src/xdg.rs` | ported | `Options` (byte-typed), config/cache/state with the same env/windows_env/default_subdir triples, `dir()` (home short-circuit, env non-empty with/without subdir, homedir fallback, NoHomeDir), `parseTerminalExec`, faithful `joinSepMaybeZ` path_join; `test{}` + "cache directory paths" + both fallback tests + parseTerminalExec with the same assertions (plus two additive proving tests). |
| `src/os/wasm/log.zig:1-61` | `crates/rustty-os/src/wasm/log.rs`, `crates/rustty-vt-c/src/sys.rs:500-552` | ported | `Freestanding.log` → `freestanding::log(level_txt, scope, formatted)` (wasm32-only, `env.log` import with explicit wasm_import_module), `noop` carried; the `lib_vt.zig` `std_options` selection (Debug → log, else noop) lives at the head of rustty-vt-c `log_fn`. Level text and the `.default` → `': '` vs `'(scope): '` prefix match. Only divergence: S113-F5. |
| `src/os/wasm/target.zig:1-20` | `crates/rustty-os/src/wasm/target.rs`, `crates/rustty-build-config/src/os/wasm.rs`, `crates/rustty-build-config/src/lib.rs:91,253-255` | ported | `Target { browser }` + `target: ?Target` → `TARGET: Option<Target>` (None off-wasm, Some(Browser) on wasm32). The comptime tag-name assert has no runtime analogue; rustty hardcodes `Some(Target::Browser)` rather than reading `rustty_build_config::wasm_target()`, but build-config's `HOST_WASM_TARGET` is itself hardcoded `Browser` (mirroring `build/Config.zig:162`), so the value is identical. |
| `src/renderer/Metal.zig:1-496` | `crates/rustty-renderer/src/metal/metal_impl.rs`, `crates/rustty-renderer/src/metal.rs`, `crates/rustty-renderer/src/generic.rs:2116-2120,2447-2462`, `crates/rustty-core/src/app.rs:247` | ported | All decls present: consts (custom_shader_target=msl, y_is_down=true, swap_chain_count=3), fields, init (split into init_device + init/attach_view, identical attach order), deinit (Drop), loopEnter/displayCallback (container_of + draw_frame_raw(true)), drawFrameStart/End, initShaders/initTarget/textureOptions srgb selection, surfaceSize, present/presentLastTarget, the 6 bufferOptions aliases, samplerOptions, ImageTextureFormat.toPixelFormat, imageTextureOptions, initAtlasTexture, beginFrame, warmup, chooseDevice, queryMaxTextureSize. Only the log sites are dropped (S113-F2). |
| `src/renderer/OpenGL.zig:1-467` | `crates/rustty-renderer/src/opengl/opengl_impl.rs`, `crates/rustty-renderer/src/opengl/gl/sys.rs:42-47,326-581`, `crates/rustty-renderer/src/generic/linux_impl.rs:1193-1230,1725-1748` | ported | Consts, init (blending only), deinit, glDebugMessageCallback (same strings + severity mapping), prepareContext, surfaceInit (GTK arm), finalizeSurfaceInit/threadEnter/threadExit no-ops, drawFrameStart/End, initShaders, surfaceSize, initTarget, present (disable SRGB, blit NEAREST, always re-enable), presentLastTarget, gpuResourcesReleased, bufferOptions ×6, textureOptions, samplerOptions, ImageTextureFormat, imageTextureOptions, initAtlasTexture, beginFrame. See S113-F1 (medium) and S113-F4. |
| `src/renderer/Options.zig:1-24` | `crates/rustty-core/src/renderer_seam.rs:563-603`, `crates/rustty-renderer/src/generic.rs:1009-1070` | ported | All six fields (config, font_grid, size, surface_mailbox, rt_surface, thread) present on core's `Options<A>`; the apprt-free `rustty_renderer::Options` carries the first three (R4 relocation). |
| `src/renderer/Overlay.zig:1-416` | `crates/rustty-renderer/src/overlay.rs` | ported | Color enum + rgb values, rectFill alpha 96, rectBorder alpha 200, premultiplied alphaPixel; Feature; InitError; init; deinit; pendingImage; reset; applyFeatures dispatch; highlightHyperlinks / highlightSemanticPrompts; highlightGridRect / highlightPixelRect. Oracle has 0 tests. Only the three log.warn sites are dropped (S113-F3). |
| `src/renderer/State.zig:1-235` | `crates/rustty-renderer/src/state.rs` | ported | Fields (mutex, terminal, inspector, preedit, mouse, demand, handoff_gen), handoff_timeout 1ms, lockDemand/unlockDemand/yieldToDemand (relaxed atomics, same double-check + timed wait; futex replaced by Mutex+Condvar gate with equivalent observable behaviour), Mouse, Preedit (Codepoint, deinit, clone, width, range). Both oracle tests ported with identical assertions. |

**Partially-read units:** none. Both shards report `units_partial: []` — every unit listed above was read in full.

## Findings

### High

None.

### Medium

#### S113-F1 — displayRealized: prepareContext failure aborts re-realize instead of warn-and-continue

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** behavioral-difference
- **Oracle:** `src/renderer/OpenGL.zig:237-250`
- **rustty:** `crates/rustty-renderer/src/generic/linux_impl.rs:1725-1748` (presence lens corrected the anchor to `:1746-1748`)

The oracle's `OpenGL.displayRealized` returns void: on the GTK arm it does `prepareContext(null) catch |err| log.warn(...)` and continues, so `GenericRenderer.displayRealized` (`generic.zig:934-947`) always goes on to lock `draw_mutex` and rebuild the shaders/swap chain. The rustty port makes `prepare_context_windowed()` failure return `Err(Error::GlFailed)` before the `draw_mutex` lock, so the swap chain/shaders are NOT rebuilt after a realize on a context whose version gate or GL enable fails, `display_realized` stays false, and the caller sees an error the oracle never produces.

**Evidence:** `OpenGL.zig:240-246` `apprt.gtk => prepareContext(null) catch |err| { log.warn("Error preparing GL context in displayRealized, err={}", .{err}); }` (fn returns `void`); `generic.zig:937` `self.api.displayRealized();` (no `try`). `linux_impl.rs:1746-1748` `if !self.api.owns_context() && crate::opengl::gl::sys::prepare_context_windowed().is_err() { return Err(Error::GlFailed); }` followed by `self.draw_mutex.lock()` and the rebuild at 1750+. Caller: `rustty-core/src/renderer_seam.rs:525`.

**Suggested fix:** Match the oracle — on the windowed path call `prepare_context_windowed()`, log a warning on `Err`, and fall through to the `draw_mutex` lock + rebuild instead of returning `Err(GlFailed)`.

#### S112-F1 — GHOSTTY_RESOURCES_DIR read as UTF-8 String; non-UTF-8 value becomes an error instead of the raw path

- **Status:** CONFIRMED (both lenses failed to refute)
- **Kind:** behavioral-difference
- **Oracle:** `src/os/resourcesdir.zig:51-58, 113-120`
- **rustty:** `crates/rustty-os/src/resourcesdir.rs:95-110, 169-184, 288-291`

The reference reads the env var as raw bytes via `global.environ().getAlloc` and, on POSIX, only `EnvironmentVariableMissing` is special-cased (release) or `InvalidWtf8`/`EnvironmentVariableMissing` are swallowed (debug fallback); any byte value is accepted and returned as `app_path`. The port uses `std::env::var` (String), so a non-UTF-8 value yields `VarError::NotUnicode`, which the port propagates as `Err(ResourcesDirError::EnvError)` in BOTH modes. Release: Zig returns the raw dir, Rust errors. Debug: Zig swallows InvalidWtf8 and returns `.{}` (or the terminfo-detected dir), Rust errors. `app_path` is also `Option<String>` rather than bytes (lossy vs `[]const u8`).

**Evidence:** `resourcesdir.zig:52-57` `getAlloc(alloc, "GHOSTTY_RESOURCES_DIR") catch |err| switch (err) { error.EnvironmentVariableMissing => break :env, else => return err }` and `:114-119` `error.InvalidWtf8, error.EnvironmentVariableMissing => {}`; `resourcesdir.rs:96-109` and `:170-183` `match get_env_var_owned(..) { .. Err(err) => match err { NotPresent => {}, other => return Err(EnvError(other)) } }` with `:289-291` `std::env::var(key)`. A byte-typed environ map exists and is used elsewhere (`path.rs:47` over `rustty_global::EnvMap`), so this is not an unavailable-API constraint.

**Suggested fix:** Read the variable as bytes (`std::env::var_os` / rustty_global environ map), make `app_path`/`host_path` `Vec<u8>`/`OsString`, treat NotPresent as missing and never fail on non-UTF-8.

### Low

#### S112-F2 — maybeDir drops the bufPrint NoSpaceLeft error path

- **Status:** CONFIRMED
- **Kind:** missing-error-path
- **Oracle:** `src/os/resourcesdir.zig:131-148`
- **rustty:** `crates/rustty-os/src/resourcesdir.rs:200-224`

The reference formats `base/sub/suffix` into the caller's `max_path_bytes` buffer with `try std.fmt.bufPrint`, so an over-long candidate propagates `error.NoSpaceLeft` out of `resourcesDir`. The port formats into a heap `String`, only copies into `buf` when it fits, and otherwise silently proceeds to the existence check (which fails and returns `None`), so `resourcesDir` returns `Ok(default)` where the oracle returns `Err`.

**Evidence:** `resourcesdir.zig:137` `const path = try std.fmt.bufPrint(buf, "{s}/{s}/{s}", ...)` (called with `try` at `:83` and `:99`); `resourcesdir.rs:208-212` `let path = format!(..); if bytes.len() <= buf.len() { buf[..].copy_from_slice(bytes); }` then `:214` `Path::new(&path).exists()`. `ResourcesDirError` (`:70-74`) has only the `EnvError` variant.

**Suggested fix:** Return an error (add a `NoSpaceLeft` variant to `ResourcesDirError`) when `bytes.len() > buf.len()`.

#### S112-F3 — Exe-path walk uses lossy to_str() and Path::parent instead of byte dirname

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/resourcesdir.zig:69-109`
- **rustty:** `crates/rustty-os/src/resourcesdir.rs:117-165, 305-307`

`dir_str` maps a non-UTF-8 exe directory to `""` so every probe becomes `/Contents/Resources/...` or `/share/...` at the filesystem root instead of the real directory; the reference walks the raw bytes. Additionally `Path::parent` of a relative path yields `Some("")` (one extra iteration probing root-relative `/share/...`) where `std.fs.path.dirname` returns null; only reachable if `current_exe()` is relative.

**Evidence:** `resourcesdir.zig:79-80` `while (std.fs.path.dirname(exe)) |dir| { exe = dir; ...}`; `resourcesdir.rs:126-128` `while let Some(dir) = exe.parent()` and `:305-307` `dir.to_str().unwrap_or("")`.

**Suggested fix:** Operate on `OsStr` bytes (`as_bytes()` on unix) for maybeDir's base, and stop when the parent is empty.

#### S112-F4 — expand takes `&str` cmd (reference: `[]const u8`) and the Windows OsStr shim is lossy

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/path.zig:9-19`
- **rustty:** `crates/rustty-os/src/path.rs:33-41, 208-211`

The reference command is raw bytes; the port's `cmd: &str` cannot express a non-UTF-8 command name (callers must have already lossily converted). On Windows `os_str_from_bytes` maps non-UTF-8 to `""` (`unwrap_or("")`), so a PATH entry containing WTF-8 makes `open_file` open `""` → NotFound → `continue`, where Zig's fs converts WTF-8 to WTF-16 and opens the real candidate.

**Evidence:** `path.zig:13` `cmd: []const u8`; `path.rs:35` `cmd: &str`, `:208-211` `std::ffi::OsStr::new(std::str::from_utf8(b).unwrap_or(""))`.

**Note from the semantic lens:** the `&str`-signature half alone is unobservable today (`os/path.zig`'s `expand` has no non-test caller in either tree); the finding survives on the Windows shim, and should be re-scoped to it.

**Suggested fix:** Take `cmd: &[u8]`; on Windows convert bytes via WTF-8/`OsString::from_wide` equivalents rather than defaulting to an empty path.

#### S112-F5 — Flatpak host-command block additionally gated on cargo feature `flatpak`; error mapping and NUL handling differ

- **Status:** CONFIRMED
- **Kind:** missing-branch
- **Oracle:** `src/os/passwd.zig:58-124`
- **rustty:** `crates/rustty-os/src/passwd.rs:84-162`

The reference runs the getent host-command path whenever `build_config.flatpak` is true; the port requires BOTH `build_config::flatpak()` AND `cfg(feature = "flatpak")`. Pty/spawn/wait errors are all mapped to `Error::OutOfMemory` (reference propagates the real error), and `CString::new(v).unwrap()` panics on an interior NUL where `alloc.dupeZ` would copy.

**Evidence:** `passwd.zig:59-62` `if (internal_os.isFlatpak()) flatpak: { if (comptime !build_config.flatpak) {...break} ... }`; `passwd.rs:86-91` `if !build_config::flatpak() {..break..} #[cfg(feature = "flatpak")] { ... }`, `:99/:114/:115` `.map_err(|_| Error::OutOfMemory)`, `:157-158` `CString::new(v).unwrap()`.

**Note from the semantic lens:** the headline cfg-divergence half is refuted — `rustty-termio/Cargo.toml:31` chains `flatpak = ["rustty-os/flatpak", "rustty-build-config/flatpak"]`, the only enabler in the workspace, so the two flags cannot diverge in any buildable configuration. The finding stands on the collapsed error mapping (the crate has an unused `Error::Os(i32)` variant) and the `CString::new(..).unwrap()` panic.

**Suggested fix:** Carry real OS errors in `Error::Os`, and truncate/handle interior NUL instead of `unwrap`.

#### S112-F6 — macOS pipe fallback leaks both fds when fcntl(F_SETFD) fails

- **Status:** CONFIRMED
- **Kind:** missing-error-path
- **Oracle:** `src/os/pipe.zig:9-11` (via `src/lib/compat/fd.zig:42-46`)
- **rustty:** `crates/rustty-os/src/pipe.rs:55-69`

The reference's `compat_fd.pipe2` fallback has `errdefer { close(fds[0]); close(fds[1]); }` around the FD_CLOEXEC loop; the port returns `Err(last_os_error())` from inside the loop (both the F_GETFD and F_SETFD failure arms) without closing the freshly created descriptors.

**Evidence:** `compat/fd.zig:42-46` `const fds = try pipe(); errdefer { close(fds[0]); close(fds[1]); }`; `pipe.rs:60-68` `for &fd in &fds { ... if flags < 0 { return Err(..) } if fcntl(..) < 0 { return Err(..) } }` with no close.

**Suggested fix:** Close both fds before returning the error in the fcntl failure arms.

#### S112-F7 — openThread stderr logging is unbounded per line and thread-spawn failure panics

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/open.zig:66-67, 79-101`
- **rustty:** `crates/rustty-os/src/open.rs:166-167, 196-221`

The reference reads stderr with a 256-byte buffer: a line longer than 256 bytes hits `StreamTooLong` and is logged in 256-byte chunks; the port uses `read_until` with an unbounded Vec so long lines are logged whole (log-text difference only). The reference `try std.Thread.spawn` returns an error to the caller if the reaper thread cannot be created; the port's `std::thread::spawn` panics on failure.

**Evidence:** `open.zig:85-96` `var buffer: [256]u8 ... error.StreamTooLong => reader.take(buffer.len)`, `:66` `const thread = try std.Thread.spawn(...)`; `open.rs:204-217` `BufReader::new(stderr) ... read_until(b'\n', &mut line)`, `:166` `std::thread::spawn(...)`. No `thread::Builder` / `OpenError::Thread` variant exists in the crate.

**Suggested fix:** Use `std::thread::Builder::new().spawn()` and map the error into `OpenError`; optionally cap line reads at 256 bytes to match chunking.

#### S112-F8 — connect() addrlen and comm-read error arm differ from the reference

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/systemd.zig:37-53, 136-149`
- **rustty:** `crates/rustty-os/src/systemd.rs:51-57, 167-185`

The reference passes `@offsetOf(sockaddr.un,"path") + path.len` (the full 110-byte struct) as addrlen to `connect`; the port passes the exact path length. Separately, the reference logs two distinct errors (`unable to open` vs `problems reading from`) and reads at most 16 bytes; the port reads the whole file and logs only the open message.

**Evidence:** `systemd.zig:137-141` `connect(socket, &socket_address, @offsetOf(std.os.linux.sockaddr.un, "path") + socket_address.path.len)`, `:50-53` `readAll(&comm_data_buf) catch { log.err("problems reading from ...") }`; `systemd.rs:169-170` `let addr_len = (path_off + socket_path.len()) as socklen_t`, `:51-57` single `std::fs::read` error arm.

**Note from the semantic lens:** the addrlen half is refuted — for AF_UNIX pathname sockets the kernel accepts both forms and resolves the identical path (the port writes a NUL at `sun_path[len]`), and neither side uses the abstract namespace. Only the collapsed log arm survives, and it is cosmetic; the 16-byte read cap is equivalent because the port truncates to `TASK_COMM_LEN` after reading.

**Suggested fix:** Keep the two log arms if exact parity is wanted (and optionally pass `size_of::<sockaddr_un>()` as addrlen).

#### S112-F9 — Port/percent number parsing does not accept Zig parseInt's '_' digit separators

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/uri.zig:21-91`
- **rustty:** `crates/rustty-os/src/uri.rs:321-336`

The std.Uri subset is reimplemented; Zig's `std.fmt.parseInt(u16, s, 10)` skips `_` separators after the first digit and accepts a `+` sign, so `file://host:8_0/` parses as port 80 in Zig, while the port's `str::parse::<u16>` rejects it as `InvalidPort`.

**Evidence:** `std/fmt.zig:439-442` rejects only leading/trailing `_` and `continue`s on interior `_`; `std/Uri.zig:256,262` uses that call for the port. `uri.rs:321-327` `parse::<u16>().ok()`, `:329-336` `u8::from_str_radix(txt, 16).ok()`.

**Note from the semantic lens:** the percent-decode half of the original description is wrong and does not contribute — `std/Uri.zig:96` parses a 2-character field where any `_` is necessarily leading or trailing and is therefore rejected by Zig too. Only the port-number half stands.

**Suggested fix:** Implement Zig's `parseWithSign` semantics (skip `_` after the first digit, `+` prefix, `-` rejected for unsigned) in `parse_int_u16`.

#### S112-F11 — homedir.home errors collapsed into NoHomeDir

- **Status:** CONFIRMED
- **Kind:** missing-error-path
- **Oracle:** `src/os/xdg.zig:90-100`
- **rustty:** `crates/rustty-os/src/xdg.rs:155-169`

The reference `try homedir.home(...)` propagates whatever error `home` returns (its only member being `error.BufferTooSmall`); the port maps every `homedir::home` error to `DirError::NoHomeDir`, so a home path longer than the 1024-byte buffer yields the same error value as "no home directory".

**Evidence:** `xdg.zig:92` `if (try homedir.home(io, environ_map, &buf)) |home|`; `xdg.rs:157` `homedir::home(environ_map, &mut buf).map_err(|_| DirError::NoHomeDir)?`; `rustty-os/src/homedir.rs:23-26` `enum Error { BufferTooSmall }`.

**Suggested fix:** Add a `Home(homedir::Error)` variant to `DirError` and propagate it.

#### S113-F2 — Metal log.debug/log.warn call sites preserved only as comments

- **Status:** CONFIRMED
- **Kind:** missing-branch
- **Oracle:** `src/renderer/Metal.zig:85-88, 174, 443`
- **rustty:** `crates/rustty-renderer/src/metal/metal_impl.rs:250-251, 844-846, 923-926`

The device-properties `log.debug` in init, the `log.warn` in displayCallback on drawFrame error, and the `log.warn` in warmup on Shaders.init failure are all commented out ("no logging facade wired"). The same crate does have working log macros (`log_warn!`/`log_info!`/`log_err!`/`log_debug!` in `opengl/gl.rs:54-69`), so the diagnostics are silently lost on the Metal path.

**Evidence:** `Metal.zig:85-88` `log.debug("device properties default_storage_mode={} max_texture_size={}", ...)`, `:174` `log.warn("Error drawing frame in display callback, err={}", .{err})`, `:443` `log.warn("metal warmup shader init failed err={}", .{err})` vs `metal_impl.rs:250-251` (comment), `:844-846`, `:923-926`.

**Suggested fix:** Route the three sites through the crate's log macros with the oracle's message text.

#### S113-F3 — Overlay rect-draw error warnings are swallowed silently

- **Status:** CONFIRMED
- **Kind:** missing-branch
- **Oracle:** `src/renderer/Overlay.zig:195-197, 245-247, 291-293`
- **rustty:** `crates/rustty-renderer/src/overlay.rs:329-341, 461-470, 509-518`

The three `catch |err| log.warn(...)` sites (hyperlink border, semantic prompt bar, semantic content highlight) collapse to `if let Err(_err) = ... { /* comment */ }`; the error payload (`OverlayError`, kept only under `#[allow(dead_code)]`) is never surfaced. Control flow (continue after error) matches.

**Evidence:** `Overlay.zig:196` `std.log.warn("Error drawing hyperlink border: {}", .{err})`, `:246` `log.warn("Error drawing semantic prompt bar: {}", .{err})`, `:292` `log.warn("Error drawing semantic content highlight: {}", .{err})` vs `overlay.rs:337-341`, `:469`, `:517` (comments only); `overlay.rs:772-778` `#[allow(dead_code)] enum OverlayError`.

**Suggested fix:** Emit the three warnings through the crate's log macros with the oracle text and the `OverlayError` Display.

#### S113-F4 — present: re-enable GL_FRAMEBUFFER_SRGB failure is swallowed without the oracle's log.err

- **Status:** CONFIRMED
- **Kind:** missing-branch
- **Oracle:** `src/renderer/OpenGL.zig:307-309`
- **rustty:** `crates/rustty-renderer/src/opengl/opengl_impl.rs:406-410, 499-501`

The oracle's `defer gl.enable(GL_FRAMEBUFFER_SRGB) catch |err| log.err("Error re-enabling GL_FRAMEBUFFER_SRGB, err={}", .{err})` is reproduced structurally (always re-enable, never mask the blit result) but the `log.err` is a comment in both `present` and `present_to_default`, even though `log_err!` exists in the same crate.

**Evidence:** `OpenGL.zig:307-309` vs `opengl_impl.rs:406-410` `if draw::enable(gl::FRAMEBUFFER_SRGB).is_err() { // log.err(...) }` and `:499-501` same.

**Suggested fix:** Call `log_err!("Error re-enabling GL_FRAMEBUFFER_SRGB, err={err}")` in the `is_err()` arm.

#### S113-F5 — wasm log: allocation-failure path returns silently in Zig but the Rust port always heap-formats

- **Status:** CONFIRMED
- **Kind:** behavioral-difference
- **Oracle:** `src/os/wasm/log.zig:32-50`
- **rustty:** `crates/rustty-os/src/wasm/log.rs:58-87` (presence lens anchored `:57-85`)

The oracle formats into a 2048-byte stack buffer first, and only on overflow falls back to `allocPrint`, which on OOM `return`s (log dropped, no crash) — explicitly so that an OOM error can still be logged. The Rust port builds `prefix`/`txt` with `format!`/`to_string()` unconditionally (heap), then copies into the 2048 buffer only if it fits; under wasm OOM the Rust allocation aborts via `handle_alloc_error` instead of dropping the message, and the stack-buffer copy has no observable effect.

**Evidence:** `log.zig:36-50` `var buf: [2048]u8 ... bufPrint(&buf, txt, args) catch str: { allocated = true; break :str std.fmt.allocPrint(wasm.alloc, txt, args) catch return; }` vs `log.rs:68-83` `let prefix = ....to_string()/format!(...)`, `let txt = format!("{level_txt}{prefix}{formatted}")`, then `if bytes.len() <= buf.len() { copy } else { bytes }`; caller `rustty-vt-c/src/sys.rs:536` additionally pre-formats with `std::fmt::format(args)`.

**Suggested fix:** Format with `core::fmt::Write` into the stack buffer first (a fixed-capacity writer) and only fall back to a `try_reserve`-guarded String, returning on failure, to preserve the OOM-tolerant path.

#### S112-F10 — Several windows.zig declarations are omitted or relocated out of the bridge module

- **Status:** DISPUTED (refuted by the semantic lens; presence lens confirmed)
- **Kind:** missing-function
- **Oracle:** `src/os/windows.zig:12-15, 23-36, 44-55, 80, 105-109, 216-223, 240-254, 257-260`
- **rustty:** `crates/rustty-os/src/windows.rs:15-22, 183-189, 214-220, 349-353`

Not present in `rustty-os::windows`: `unexpectedStatus`, `HINSTANCE`, `LARGE_INTEGER`, `LPCWSTR`, `ULONG`, `UNICODE_STRING`, the `OVERLAPPED` struct, `FILE_NON_DIRECTORY_FILE`, the ntdll types (`ACCESS_MASK`, `IO_STATUS_BLOCK`, `NTSTATUS`, `OBJECT_ATTRIBUTES`) and `exp.ntdll.NtCreateFile`, `exp.kernel32.WaitForSingleObject`/`GetExitCodeProcess`/`CloseHandle`/`CreateFileW`/`CreateNamedPipeW`/`SetHandleInformation` (the last four exist at top level instead of under `exp.kernel32`). `ProcThreadAttributeNumber` is a closed enum (reference is non-exhaustive `_`). Use sites are covered by relocations: `WaitForSingleObject`/`GetExitCodeProcess` in `rustty-termio/src/command.rs:1697-1698`, `OVERLAPPED` in `rustty-xev/src/windows.rs:297`, and the `NtCreateFile(\Device\Null)` path substituted by `CreateFileW("NUL")` in `rustty-termio/src/command.rs:775-783`.

**Evidence:** `windows.zig:15` `pub const unexpectedStatus`, `:44-55` `pub const OVERLAPPED = extern struct {...}`, `:80` `FILE_NON_DIRECTORY_FILE`, `:216-223` `WaitForSingleObject`/`GetExitCodeProcess`, `:240-254` `ntdll.NtCreateFile`; `windows.rs:15-22` PORT NOTE explicitly not transcribing them, `:214-220` note on the ntdll types.

**Refuting lens — semantic (confidence: medium), verbatim:** "No observable difference. The omitted items in ghostty/src/os/windows.zig (:15 unexpectedStatus, :22-36 HINSTANCE/LARGE_INTEGER/LPCWSTR/ULONG/UNICODE_STRING, :44-55 OVERLAPPED, :80 FILE_NON_DIRECTORY_FILE, :105-109 ACCESS_MASK/IO_STATUS_BLOCK/NTSTATUS/OBJECT_ATTRIBUTES, :239-254 ntdll.NtCreateFile) are pure re-exports of std.os.windows type aliases and one extern with no call site; the finder itself confirms every use site is covered (rustty-termio/src/command.rs:1697-1698 for WaitForSingleObject/GetExitCodeProcess, rustty-xev/src/windows.rs:297 for OVERLAPPED, command.rs:775-783 CreateFileW for the \\Device\\Null path). Declaration surface with no caller cannot change behaviour, and windows.rs:15-22/214-220 document the convergence rationale. ProcThreadAttributeNumber's closed enum (windows.rs:349-353) is likewise unobservable: the only value ever constructed is ProcThreadAttributePseudoConsole = 22 (windows.zig:97-102), and the reference's `_` arm has no constructor anywhere. Whole module is #![cfg(windows)] on a macOS-first fork."

**Suggested fix (if fidelity of surface is wanted):** Transcribe the remaining declarations (or re-export the relocated ones) so `rustty-os::windows` carries the full reference surface, and make `ProcThreadAttributeNumber` an open newtype.

#### S112-F12 — align_pointer can overflow before its overflow check

- **Status:** DISPUTED (refuted by the semantic lens; presence lens confirmed)
- **Kind:** behavioral-difference
- **Oracle:** `src/os/mach.zig:107`
- **rustty:** `crates/rustty-os/src/mach.rs:255-269`

`mem.alignPointer` returns null when the aligned address would overflow. The port computes `mem_align_forward(addr, align)` = `(addr + align - 1) & !(align - 1)` first, which panics in debug / wraps in release when `addr + align - 1` overflows usize, and only then checks `aligned < addr`.

**Evidence:** `mach.zig:107` `mem.alignPointer(slice.ptr, alignment_bytes) orelse return null`; `mach.rs:255-268` `fn mem_align_forward(addr, alignment) { (addr + alignment - 1) & !(alignment - 1) }` then `if aligned < addr { return None }`.

**Refuting lens — semantic (confidence: medium), verbatim:** "Difference is confined to a practically unreachable state and is release-identical. std/mem.zig:4284-4287 does `@addWithOverflow(addr, align_to - 1)` and returns null on the carry; rustty mach.rs:255-268 computes `(addr + alignment - 1) & !(alignment - 1)` then tests `aligned < addr`. In release the Rust add wraps, the mask leaves a value below addr, and align_pointer returns None — exactly the Zig result. Only a debug build differs (overflow panic), and only when mmap (mach.rs call site mirroring mach.zig:99-107) returns a base address within `alignment` bytes of usize::MAX, which the kernel never does. No reachable input produces a different observable result."

**Suggested fix:** Use `addr.checked_add(align - 1)` and return None on overflow.

## Performance-only differences (not findings)

- `mach.rs`: the `next_mmap_addr_hint` placement hint (atomic load / cmpxchg around mmap) is omitted; a null hint lets the kernel choose the address, allocation results are identical.
- `systemd.rs`: `launchedBySystemd` reads the whole `/proc/<ppid>/comm` file and truncates to 16 bytes instead of a 16-byte `readAll`; comm is at most 16 bytes so the compared bytes are identical.
- `resourcesdir.rs`: `maybeDir` returns an owned `String` of base/sub instead of a slice into the caller's buffer (borrow-checker); bytes identical.
- `shell.rs`: `ShellEscapeWriter` collapses the Zig `drain(data, splat)` chunk loop into a single `writeAll`→`writeEscaped` call; the escaped byte stream is identical.
- `path.rs`: `tokenize_path` is a split+filter iterator over the PATH bytes instead of `std.mem.tokenizeScalar`; same tokens in the same order.
- `uri.rs`: `std.Uri` is reimplemented over byte slices with index arithmetic instead of the SliceReader cursor; same slices produced.
- Metal option factories take `&self` and `.clone()` the `Retained` device (a refcount bump) where Zig copies the struct by value; identical observable behaviour.
- OpenGL `present_last_target` `take()`s and re-stores the move-only `Target` instead of copying it; same observable result.
- `State` handoff uses a Mutex+Condvar gate instead of a raw futex on `handoff_gen`; the fast path remains a single relaxed load and the 1ms timed wait / generation check semantics are the same.
- Overlay: ported z2d `move_to`/`line_to`/`close_path` are infallible (Zig's only failure there is OOM, which the Rust global allocator aborts on rather than returns).
- OpenGL `prepare_context` returns `(major, minor)` (oracle returns void) and parses `GL_VERSION` separately because the `gl` crate loader does not report a version; gate condition identical.

## Refuted candidates

No finding was refuted by both lenses, so none was dropped from the report. Two findings were refuted by one lens only and are retained above as DISPUTED:

- **S112-F10 — Several windows.zig declarations are omitted or relocated out of the bridge module** — semantic lens: the omitted items are callerless type aliases/externs on a `#![cfg(windows)]` module of a macOS-first fork, and every use site is covered by a relocation, so no behaviour can differ.
- **S112-F12 — align_pointer can overflow before its overflow check** — semantic lens: in release the wrapping add + mask yields exactly Zig's `null` result; only a debug build differs, and only for mmap addresses within `alignment` bytes of `usize::MAX`, which the kernel never returns.

Additionally, three CONFIRMED findings had one of their sub-claims refuted in verification and are annotated in place: S112-F4 (the `&str` signature half — no non-test caller), S112-F5 (the cargo-feature divergence half — the feature chain forbids it), S112-F8 (the `connect` addrlen half — kernel-equivalent for pathname sockets), and S112-F9 (the percent-decode half — Zig also rejects `_` in a 2-character field).
