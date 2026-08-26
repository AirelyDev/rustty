# WS4 — Full App Bootstrap (BREAK #1) + CLI-Action-from-C Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Port the full ghostty `global.zig` `state.init()` bootstrap (logging config, SIGPIPE, rlimits, crash.init, locale, glslang, oniguruma, i18n) into the rustty embedded/C and exe paths, and make `rustty_cli_try_action`'s non-null path run a detected CLI action and exit.

**Architecture:** The reference `global.zig` `GlobalState.init` is the BREAK #1 seam: `rustty-global` is the leaf state-holder, `rustty-core` binds `ResourcesDir` (Action stays `()` — `rustty-cli` depends on `rustty-core`, so the concrete cli `Action` type can NEVER live in core's `GlobalState`; the port's ratified shape, per `main_rustty.rs` ADV-FIX 2, is "detection lives above apprt"). This WS: (a) grows `rustty_apprt::init` into the full cli-free side-effect body (`init_with_action(action_detected: bool)`), (b) runs `detectArgs` at the two tiers that legitimately dep cli — the `rustty` bin (already does) and `rustty-apprt-c`'s `rustty_init` (new) — stashing the concrete action in an apprt-c-local `OnceLock` while mirroring _presence_ into `rustty_core::STATE.action: Option<()>`, and (c) implements the `rustty_cli_try_action` `Some` arm from that stash. A shared log facade (`rustty_core::logging`, the `main_ghostty.zig` `logFn` port with stderr + macOS-unified-logging sinks) makes the `RUSTTY_LOG` config observable.

**Tech Stack:** Rust 1.93.1 workspace; `libc` (sigaction/setrlimit); libSystem `os_log` via a `cc`-built C shim (rustty-os `cbits/` precedent); existing ported crates: `rustty-cli` (detect + Action::run), `rustty-crash`, `rustty-os` (ensureLocale/i18n/fixMaxFiles/getenv), `rustty-oniguruma`, `rustty-renderer::shadertoy` (glslang), `rustty-args` (parse_packed_struct).

**Oracle:** `/Users/gael/projects/ghostty` (pinned `4749c4e`) — `src/global.zig`, `src/main_c.zig`, `src/main_ghostty.zig`, `pkg/macos/os/log.zig`. Reference-only (R3).

## Global Constraints (inherited from `2026-07-23-rustty-completion-ROADMAP.md`)

- R1 full fidelity vs the ghostty oracle at `../ghostty` (pinned `4749c4e`); oracle is reference-only — no build-time dependency on it (R3).
- R2 product identity: rustty naming with per-token casing preservation (Ghostty→Rustty, ghostty→rustty); `ghostty_*` C-ABI aliases stay. Env vars follow the `GHOSTTY_RESOURCES_DIR`→`RUSTTY_RESOURCES_DIR` retarget precedent (`rustty-os/src/resourcesdir.rs:213`): the log env var is **`RUSTTY_LOG`** (already documented in the man page, `xtask/src/res_misc.rs:185`).
- R4/R5: acyclic crate graph; gates (`check_acyclic.sh`, A4 ABI gate, Miri lanes) are invariant — never weakened to pass. **This WS adds NO new C-ABI exports and changes NO export signature** — the A4/ABI gates must pass untouched.
- DG5: C-ABI panic policy is ABORT — no unwind across the ABI; no `catch_unwind`, no `"C-unwind"`.
- Allocator doctrine: `std.mem.Allocator` maps to the process-global allocator (`allocator_api2` `Global`); the reference's GPA-vs-libc selection stays the documented no-op (`seam::Allocator = ()`) — Rust's global allocator is process-wide, there is no per-state allocator to select.
- Toolchain: pinned Rust 1.93.1 (`rust-toolchain.toml`, edition 2021). LTO stays banned.
- Ledger discipline: strike DEFERRALS.md / INTEGRATION_PARITY.md entries in the same branch (Task 13).
- Commit style: `subsystem: summary (WS4 Tn)`; branch per WS; merge to main only with both platforms green (`cargo build --workspace` on macOS AND `ssh rvm 'cd ~/projects/rustty && cargo build --workspace'`).

## File Structure

| File                                                                | Responsibility                                                                         |
| ------------------------------------------------------------------- | -------------------------------------------------------------------------------------- |
| `crates/rustty-core/src/lib.rs`                                     | add `pub use rustty_global::Logging;` + `pub mod logging;`                             |
| `crates/rustty-core/src/logging.rs` (new)                           | the `main_ghostty.zig` `logFn` port (shared stderr + macOS sinks)                      |
| `crates/rustty-os/cbits/log_shim.c` (new)                           | `os_log_with_type` C-macro shim (oracle `zig_os_log_with_type` analogue)               |
| `crates/rustty-os/build.rs`                                         | compile the shim on macOS                                                              |
| `crates/rustty-os/src/macos.rs`                                     | `Log`/`LogType` — port of `pkg/macos/os/log.zig`                                       |
| `crates/rustty-global/src/lib.rs` + `Cargo.toml`                    | `seam::Rlimit = libc::rlimit` rebind + live `ResourceLimits::restore`                  |
| `crates/rustty-renderer/src/shadertoy.rs`                           | `pub fn init()` (glslang init, pkg/glslang `init.zig`)                                 |
| `crates/rustty-apprt/src/lib.rs` + `Cargo.toml`                     | `compute_logging`, `init_signals`, the full `init_with_action`                         |
| `crates/rustty/src/main_rustty.rs`                                  | bin wiring: `init_with_action(action.is_some())`, `log_fn` delegates to core           |
| `crates/rustty-apprt-c/src/lib.rs` + `Cargo.toml`                   | argv storage, `detect_from_argv`, full `rustty_init`, `rustty_cli_try_action` Some arm |
| `.port/phase-2/DEFERRALS.md`, `.port/phase-2/INTEGRATION_PARITY.md` | ledger strikes                                                                         |

Every task below runs from the repo root `/Users/gael/projects/rustty`.

---

### Task 1: Branch + `Logging` re-export + `RUSTTY_LOG` packed-struct parse (`compute_logging`)

**Files:**

- Modify: `crates/rustty-core/src/lib.rs` (near line 53, the `GlobalState` type alias)
- Modify: `crates/rustty-apprt/src/lib.rs`
- Modify: `crates/rustty-apprt/Cargo.toml`
- Test: in-crate `#[cfg(test)]` in `crates/rustty-apprt/src/lib.rs`

**Interfaces:**

- Consumes: `rustty_global::Logging` (fields `stderr: bool`, `macos: bool`, `Copy`, `Default` = `{stderr: app_runtime()!=None, macos: cfg!(macos)}`); `rustty_args::args::{PackedStruct, parse_packed_struct}` (trait: `set_all(&mut self, bool)`, `set_field(&mut self, part: &[u8], value: bool) -> bool`, `field_names() -> &'static [&'static [u8]]`, supertrait `Default`).
- Produces: `rustty_core::Logging` (re-export, used by every later task); `rustty-apprt`-private `fn compute_logging(action_detected: bool, env_value: Option<&[u8]>) -> Logging` (Task 7 consumes it).

- [ ] **Step 1: Create the branch**

```bash
git checkout main && git pull --ff-only && git checkout -b ws4-app-bootstrap
```

- [ ] **Step 2: Add the `Logging` re-export in rustty-core**

In `crates/rustty-core/src/lib.rs`, directly below the `pub type GlobalState = ...` line (line 53):

```rust
/// PARITY: reference `GlobalState.Logging` (global.zig `pub const Logging =
/// packed struct`). Re-exported at the core wiring tier so upward crates
/// (apprt, apprt-c, the bin) name it without a direct rustty-global edge.
pub use rustty_global::Logging;
```

- [ ] **Step 3: Write the failing tests**

In `crates/rustty-apprt/src/lib.rs`, add at the bottom:

```rust
#[cfg(test)]
mod bootstrap_tests {
    use super::*;

    // reference (global.zig init): `if (self.action != null) self.logging.stderr = false;`
    // then `self.logging = cli.args.parsePackedStruct(Logging, v.value) catch .{};`
    #[test]
    fn compute_logging_defaults() {
        let l = compute_logging(false, None);
        assert_eq!(l, rustty_core::Logging::default());
    }

    #[test]
    fn compute_logging_action_disables_stderr() {
        let l = compute_logging(true, None);
        assert!(!l.stderr);
    }

    #[test]
    fn compute_logging_env_stderr_flag() {
        // RUSTTY_LOG=stderr with an action: the env parse REPLACES the
        // action-adjusted logging (reference assigns `self.logging = ... catch .{}`).
        let l = compute_logging(true, Some(b"stderr"));
        assert!(l.stderr);
    }

    #[test]
    fn compute_logging_env_bool_shorthand() {
        let l = compute_logging(false, Some(b"true"));
        assert!(l.stderr && l.macos);
        let l = compute_logging(false, Some(b"false"));
        assert!(!l.stderr && !l.macos);
    }

    #[test]
    fn compute_logging_env_negation() {
        let l = compute_logging(false, Some(b"no-stderr,no-macos"));
        assert!(!l.stderr && !l.macos);
    }

    #[test]
    fn compute_logging_env_invalid_falls_back_to_default() {
        // reference `catch .{}` — a parse error yields Logging{} (NOT the
        // action-adjusted value).
        let l = compute_logging(true, Some(b"bogus-flag"));
        assert_eq!(l, rustty_core::Logging::default());
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test -p rustty-apprt bootstrap_tests`
Expected: COMPILE FAIL — `compute_logging` not found.

- [ ] **Step 5: Implement `LoggingParse` + `compute_logging`**

`crates/rustty-apprt/Cargo.toml` already has `rustty-args` and `rustty-core`. In `crates/rustty-apprt/src/lib.rs` (below the `InitError` enum):

```rust
use rustty_core::Logging;

/// PARITY: `cli.args.parsePackedStruct(Logging, v.value)` (global.zig init).
/// `Logging` is a foreign type to rustty-args' `PackedStruct` trait (orphan
/// rule), so the packed-field contract is threaded through this local newtype.
/// Field order/names mirror the reference packed struct: `stderr`, `macos`.
#[derive(Clone, Copy)]
struct LoggingParse(Logging);

impl Default for LoggingParse {
    fn default() -> Self {
        Self(Logging::default())
    }
}

impl rustty_args::args::PackedStruct for LoggingParse {
    // inline for (info.fields) |field| @field(result, field.name) = b;
    fn set_all(&mut self, b: bool) {
        self.0.stderr = b;
        self.0.macos = b;
    }

    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"stderr" => {
                self.0.stderr = value;
                true
            }
            b"macos" => {
                self.0.macos = value;
                true
            }
            _ => false,
        }
    }

    fn field_names() -> &'static [&'static [u8]] {
        &[b"stderr", b"macos"]
    }
}

/// The logging-config slice of the reference `GlobalState.init`:
///
///   if (self.action != null) self.logging.stderr = false;
///   if ((try internal_os.getenv(self.alloc, "GHOSTTY_LOG"))) |v| {
///       defer v.deinit(self.alloc);
///       self.logging = cli.args.parsePackedStruct(Logging, v.value) catch .{};
///   }
///
/// R2: the env var is `RUSTTY_LOG` (the `RUSTTY_RESOURCES_DIR` retarget
/// precedent). Pure over its inputs so it is unit-testable; `init_with_action`
/// feeds it the live env read.
fn compute_logging(action_detected: bool, env_value: Option<&[u8]>) -> Logging {
    let mut logging = Logging::default();
    // If we have an action executing, we disable logging by default
    // since we write to stderr we don't want logs messing up our
    // output.
    if action_detected {
        logging.stderr = false;
    }
    if let Some(v) = env_value {
        // `catch .{}` — a parse failure falls back to the DEFAULT Logging.
        logging = rustty_args::args::parse_packed_struct::<LoggingParse>(v)
            .map(|p| p.0)
            .unwrap_or_else(|_| Logging::default());
    }
    logging
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p rustty-apprt bootstrap_tests`
Expected: 6 passed. (Ignore an `unused` warning on `compute_logging` if the lint config flags it — Task 7 consumes it; if `-D warnings` blocks the build, add `#[cfg_attr(not(test), allow(dead_code))]` on `compute_logging` and `LoggingParse` and REMOVE it in Task 7.)

- [ ] **Step 7: Commit**

```bash
git add crates/rustty-core/src/lib.rs crates/rustty-apprt/src/lib.rs
git commit -m "rustty-apprt: RUSTTY_LOG packed-struct logging config (WS4 T1)"
```

---

### Task 2: rustty-os `macos::Log` — the unified-logging (os_log) binding

**Files:**

- Create: `crates/rustty-os/cbits/log_shim.c`
- Modify: `crates/rustty-os/build.rs`
- Modify: `crates/rustty-os/src/macos.rs` (append at end)
- Test: in-crate `#[cfg(test)]` appended in `crates/rustty-os/src/macos.rs`

**Interfaces:**

- Produces (macOS only): `rustty_os::macos::Log` with `Log::create(subsystem: &CStr, category: &CStr) -> Log`, `log.type_enabled(LogType) -> bool`, `log.log(LogType, message: &str)`, `log.release(self)`; `rustty_os::macos::LogType{Default,Info,Debug,Err,Fault}`. Task 3 consumes these.

- [ ] **Step 1: Write the failing test**

Append to `crates/rustty-os/src/macos.rs`:

```rust
// test { ... } (pkg/macos/os/log.zig)
#[cfg(all(test, target_os = "macos"))]
mod log_tests {
    use super::{Log, LogType};

    #[test]
    fn os_log_create_and_log() {
        // const log = Log.create("com.mitchellh.ghostty", "test");
        //   R2: bundle id retargeted via rustty-identity at the CALLER; the
        //   binding takes any subsystem — the KAT uses a literal like the oracle.
        let log = Log::create(c"io.airely.rustty", c"test");
        // try testing.expect(log.typeEnabled(.fault));
        assert!(log.type_enabled(LogType::Fault));
        // log.log(testing.allocator, .default, "hello {d}", .{12});
        log.log(LogType::Default, "hello 12");
        // defer log.release();
        log.release();
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p rustty-os log_tests`
Expected: COMPILE FAIL — `Log`/`LogType` not found in `macos`.

- [ ] **Step 3: Add the C shim**

Create `crates/rustty-os/cbits/log_shim.c`:

```c
/* rustty-os: os_log_with_type is a C MACRO (it captures a compile-time format
 * string), so it cannot be bound over FFI directly. This shim is the rustty
 * analogue of ghostty's `zig_os_log_with_type` C helper (pkg/macos): it fixes
 * the format to "%{public}s" and forwards a pre-formatted message, exactly what
 * pkg/macos/os/log.zig's `Log.log` does (allocPrint then forward). */
#include <os/log.h>
#include <stdint.h>

void rustty_os_log_with_type(void *log, uint8_t type, const char *msg) {
    os_log_with_type((os_log_t)log, (os_log_type_t)type, "%{public}s", msg);
}
```

In `crates/rustty-os/build.rs`, inside `fn main()` after the pty.c block (before the final `println!`):

```rust
    // macOS unified-logging shim (see cbits/log_shim.c) — the ghostty
    // `zig_os_log_with_type` analogue for pkg/macos/os/log.zig's Log.log.
    if target_os == "macos" {
        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        cc::Build::new()
            .file(manifest_dir.join("cbits/log_shim.c"))
            .warnings(false)
            .flag_if_supported("-w")
            .compile("rustty_os_log_shim");
        println!("cargo:rerun-if-changed=cbits/log_shim.c");
    }
```

- [ ] **Step 4: Implement `Log`/`LogType`**

Append to `crates/rustty-os/src/macos.rs` (above the test module from Step 1):

```rust
// ---------------------------------------------------------------------------
// PORTS: ghostty pkg/macos/os/log.zig — the unified-logging (os_log) binding
// consumed by main_ghostty.zig's `logFn` macOS sink (WS4). R3: binds the SAME
// libSystem os_log API the reference binds; `os_log_with_type` is a C macro,
// shimmed in cbits/log_shim.c (the reference's `zig_os_log_with_type`).
// ---------------------------------------------------------------------------
#[cfg(target_os = "macos")]
pub use self::oslog::{Log, LogType};

#[cfg(target_os = "macos")]
mod oslog {
    use std::ffi::{c_void, CStr, CString};
    use std::os::raw::c_char;

    extern "C" {
        // os_log_t os_log_create(const char *subsystem, const char *category);
        fn os_log_create(subsystem: *const c_char, category: *const c_char) -> *mut c_void;
        // void os_release(void *object);
        fn os_release(object: *mut c_void);
        // bool os_log_type_enabled(os_log_t log, os_log_type_t type);
        fn os_log_type_enabled(log: *mut c_void, typ: u8) -> bool;
        // cbits/log_shim.c
        fn rustty_os_log_with_type(log: *mut c_void, typ: u8, msg: *const c_char);
    }

    // pub const Log = opaque { ... };
    pub struct Log(*mut c_void);

    impl Log {
        // pub fn create(subsystem: [:0]const u8, category: [:0]const u8) *Log
        pub fn create(subsystem: &CStr, category: &CStr) -> Log {
            // SAFETY: both pointers are valid NUL-terminated C strings for the
            // duration of the call; os_log_create never returns NULL (the
            // reference unwraps with `.?`).
            Log(unsafe { os_log_create(subsystem.as_ptr(), category.as_ptr()) })
        }

        // pub fn release(self: *Log) void { c.os_release(self); }
        pub fn release(self) {
            // SAFETY: self.0 came from os_log_create and is released once
            // (by-value receiver — the handle is consumed).
            unsafe { os_release(self.0) }
        }

        // pub fn typeEnabled(self: *Log, typ: LogType) bool
        pub fn type_enabled(&self, typ: LogType) -> bool {
            // SAFETY: live os_log_t handle.
            unsafe { os_log_type_enabled(self.0, typ as u8) }
        }

        // pub fn log(self: *Log, alloc, typ: LogType, comptime format, args) void
        //   The reference allocPrints format+args to a NUL-terminated string and
        //   forwards it; the port takes the pre-formatted message (log sites
        //   format at the call, the same collapse as the bin's `log_fn`). A
        //   message with an interior NUL is logged truncated at the NUL (the
        //   reference cannot produce one from its format machinery either).
        pub fn log(&self, typ: LogType, message: &str) {
            let Ok(cmsg) = CString::new(message.as_bytes()) else {
                let cleaned: Vec<u8> =
                    message.bytes().take_while(|&b| b != 0).collect();
                let cmsg = CString::new(cleaned).expect("NUL-free by construction");
                // SAFETY: live handle + valid C string.
                unsafe { rustty_os_log_with_type(self.0, typ as u8, cmsg.as_ptr()) };
                return;
            };
            // SAFETY: live handle + valid C string.
            unsafe { rustty_os_log_with_type(self.0, typ as u8, cmsg.as_ptr()) }
        }
    }

    /// https://developer.apple.com/documentation/os/os_log_type_t?language=objc
    // pub const LogType = enum(c.os_log_type_t) { ... };
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(u8)]
    pub enum LogType {
        Default = 0x00, // OS_LOG_TYPE_DEFAULT
        Info = 0x01,    // OS_LOG_TYPE_INFO
        Debug = 0x02,   // OS_LOG_TYPE_DEBUG
        Err = 0x10,     // OS_LOG_TYPE_ERROR
        Fault = 0x11,   // OS_LOG_TYPE_FAULT
    }
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p rustty-os log_tests`
Expected: `os_log_create_and_log ... ok`.

- [ ] **Step 6: Commit**

```bash
git add crates/rustty-os/cbits/log_shim.c crates/rustty-os/build.rs crates/rustty-os/src/macos.rs
git commit -m "rustty-os: pkg/macos os_log unified-logging binding (WS4 T2)"
```

---

### Task 3: `rustty_core::logging` — the shared `logFn` port

**Files:**

- Create: `crates/rustty-core/src/logging.rs`
- Modify: `crates/rustty-core/src/lib.rs` (add `pub mod logging;` next to `pub mod surface;`)
- Test: in-crate `#[cfg(test)]` in `crates/rustty-core/src/logging.rs`

**Interfaces:**

- Consumes: `rustty_os::macos::{Log, LogType}` (Task 2); `rustty_global::Logging` (via the Task 1 re-export); `rustty_identity::BUNDLE_ID` (`"io.airely.rustty"`; rustty-core already deps rustty-identity, Cargo.toml:66); `crate::STATE`.
- Produces: `rustty_core::logging::Level{Debug,Info,Warn,Err}` (`Copy`), `pub fn log_with(level: Level, scope: &str, message: &str, logging: &Logging)`, `pub fn log(level: Level, scope: &str, message: &str)` (STATE-reading variant; falls back to `Logging::default()` when STATE is unset). Tasks 5, 7, 8, 10, 11 consume these.

- [ ] **Step 1: Write the failing test**

Create `crates/rustty-core/src/logging.rs` with ONLY the test module first:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use rustty_global::Logging;

    #[test]
    fn log_with_gates_on_stderr_flag() {
        // Both sinks off: must be a silent no-op (no panic, no output side
        // effects we can observe — this pins the flag gating compiles + runs).
        let off = Logging { stderr: false, macos: false };
        log_with(Level::Info, "default", "suppressed", &off);
        log_with(Level::Err, "test_scope", "suppressed", &off);
    }

    #[test]
    fn level_as_text_matches_reference() {
        // `level.asText()` — .warn -> "warning", .err -> "error".
        assert_eq!(Level::Debug.as_text(), "debug");
        assert_eq!(Level::Info.as_text(), "info");
        assert_eq!(Level::Warn.as_text(), "warning");
        assert_eq!(Level::Err.as_text(), "error");
    }

    #[test]
    fn state_reading_log_defaults_when_unset() {
        // `log` must not panic when STATE is not yet initialized (it falls back
        // to Logging::default()). NOTE: other tests in this binary may have set
        // STATE — either way this must not panic.
        log(Level::Info, "default", "boot message");
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Add `pub mod logging;` to `crates/rustty-core/src/lib.rs` (next to `pub mod surface;`), then:

Run: `cargo test -p rustty-core logging`
Expected: COMPILE FAIL — `Level`, `log_with`, `log` not defined.

- [ ] **Step 3: Implement the module**

Prepend to `crates/rustty-core/src/logging.rs` (above the tests):

```rust
//! rustty-core::logging
//!
//! PORTS: ghostty src/main_ghostty.zig `logFn` + `std_options` — the process
//! log sink registered as the std.log hook for BOTH the exe root and the lib
//! root (main_c.zig re-exports main_ghostty's `std_options`). Rust has no
//! global std.log hook, so call sites invoke `log`/`log_with` directly (the
//! same collapse as the bin's original `log_fn`, which Task 8 re-routes here).
//! Hosted at the core wiring tier: below apprt/apprt-c/bin, above os/global.

use rustty_global::Logging;
use std::io::Write;

/// Log severity — the reference `std.log.Level`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Err,
}

impl Level {
    // `level.asText()`.
    pub fn as_text(self) -> &'static str {
        match self {
            Level::Debug => "debug",
            Level::Info => "info",
            Level::Warn => "warning",
            Level::Err => "error",
        }
    }
}

/// The function std.log will call (main_ghostty.zig `logFn`), over an explicit
/// `Logging` (callers that hold their own state — the bin, or `init` before
/// STATE is set — pass it directly).
pub fn log_with(level: Level, scope: &str, message: &str, logging: &Logging) {
    // On Mac, we use unified logging. To view this:
    //
    //   sudo log stream --level debug --predicate 'subsystem=="io.airely.rustty"'
    //     R2: the reference predicate names the bundle id `com.mitchellh.ghostty`.
    //
    // macOS logging is thread safe so no need for locks/mutexes
    // macos: {
    //     if (comptime !builtin.target.os.tag.isDarwin()) break :macos;
    //     if (!state.logging.macos) break :macos;
    //     const prefix = if (scope == .default) "" else @tagName(scope) ++ ": ";
    //     const mac_level: macos.os.LogType = switch (level) {
    //         .debug => .debug, .info => .info, .warn => .err, .err => .fault,
    //     };
    //     const logger = macos.os.Log.create(build_config.bundle_id, @tagName(scope));
    //     defer logger.release();
    //     logger.log(std.heap.c_allocator, mac_level, prefix ++ format, args);
    // }
    #[cfg(target_os = "macos")]
    if logging.macos {
        use rustty_os::macos::{Log, LogType};
        let prefix = if scope == "default" {
            String::new()
        } else {
            format!("{scope}: ")
        };
        // Convert our levels to Mac levels
        let mac_level = match level {
            Level::Debug => LogType::Debug,
            Level::Info => LogType::Info,
            Level::Warn => LogType::Err,
            Level::Err => LogType::Fault,
        };
        // Initialize a logger. This is slow to do on every operation
        // but we shouldn't be logging too much.
        let subsystem = std::ffi::CString::new(rustty_identity::BUNDLE_ID)
            .expect("bundle id is NUL-free");
        let category = std::ffi::CString::new(scope).unwrap_or_default();
        let logger = Log::create(&subsystem, &category);
        logger.log(mac_level, &format!("{prefix}{message}"));
        logger.release();
    }

    // stderr: {
    //     // don't log debug messages to stderr unless we are a debug build
    //     if (comptime builtin.mode != .Debug and level == .debug) break :stderr;
    if !cfg!(debug_assertions) && matches!(level, Level::Debug) {
        return;
    }
    //     // skip if we are not logging to stderr
    //     if (!state.logging.stderr) break :stderr;
    if !logging.stderr {
        return;
    }
    //     const level_txt = comptime level.asText();
    //     const prefix = if (scope == .default) ": " else "(" ++ @tagName(scope) ++ "): ";
    //     nosuspend stderr.print(level_txt ++ prefix ++ format ++ "\n", args) ...
    let prefix = if scope == "default" {
        ": ".to_string()
    } else {
        format!("({scope}): ")
    };
    let mut stderr = std::io::stderr().lock();
    let _ = writeln!(stderr, "{}{prefix}{message}", level.as_text());
    let _ = stderr.flush();
}

/// The STATE-reading variant — the reference `logFn` reads the process-global
/// `state.logging`. Before `STATE` is set, falls back to `Logging::default()`
/// (the reference zero-initializes `state` — logging at defaults — before any
/// log output can happen).
pub fn log(level: Level, scope: &str, message: &str) {
    match crate::STATE.get() {
        Some(state) => log_with(level, scope, message, &state.logging),
        None => log_with(level, scope, message, &Logging::default()),
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p rustty-core logging`
Expected: 3 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-core/src/logging.rs crates/rustty-core/src/lib.rs
git commit -m "rustty-core: shared logFn port (stderr + os_log sinks) (WS4 T3)"
```

---

### Task 4: rustty-global `seam::Rlimit` rebind + live `ResourceLimits::restore`

The seam type is currently `u64` but `rustty_os::fixMaxFiles()` returns `Option<libc::rlimit>`; `rustty_termio::command.rs:463` already calls `state.rlimits.restore()` (a no-op today). Rebind the seam to the real type and make restore live.

**Files:**

- Modify: `crates/rustty-global/Cargo.toml` (add `libc`)
- Modify: `crates/rustty-global/src/lib.rs` (`seam::Rlimit`, `ResourceLimits::restore`)
- Test: in-crate `#[cfg(test)]` in `crates/rustty-global/src/lib.rs`

**Interfaces:**

- Produces: `rustty_global::seam::Rlimit = libc::rlimit` on unix (`()` elsewhere); `ResourceLimits::restore(&self)` performs `setrlimit(RLIMIT_NOFILE)`. Task 7 stores `rustty_os::fixMaxFiles()`'s return in `ResourceLimits`.

- [ ] **Step 1: Add the dependency**

In `crates/rustty-global/Cargo.toml` under `[dependencies]` (create the section if the only dep is rustty-build-config):

```toml
# BREAK #1 seam rebind (WS4): `seam::Rlimit` is the REAL `posix.rlimit`
# (`libc::rlimit`, the same type rustty-os `file::rlimit` aliases), and
# `ResourceLimits::restore` performs the reference `internal_os.restoreMaxFiles`
# setrlimit syscall directly (rustty-os sits ABOVE this leaf, so the one-line
# syscall body is mirrored here rather than imported — R4 acyclic).
libc = "0.2"
```

- [ ] **Step 2: Write the failing test**

Append to `crates/rustty-global/src/lib.rs`:

```rust
#[cfg(all(test, unix))]
mod rlimit_tests {
    use super::ResourceLimits;

    #[test]
    fn restore_round_trips_nofile() {
        // Read the current limit, then restore() it — the call must succeed and
        // leave the limit unchanged (a self-inverse KAT that needs no privilege).
        let mut cur = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        let rc = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut cur) };
        assert_eq!(rc, 0);

        let limits = ResourceLimits::init(Some(cur));
        limits.restore();

        let mut after = libc::rlimit { rlim_cur: 0, rlim_max: 0 };
        let rc = unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut after) };
        assert_eq!(rc, 0);
        assert_eq!(after.rlim_cur, cur.rlim_cur);
    }
}
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p rustty-global rlimit_tests`
Expected: COMPILE FAIL — `ResourceLimits::init(Some(cur))` mismatches `Option<u64>` (`seam::Rlimit` is still `u64`).

- [ ] **Step 4: Rebind the seam + implement restore**

In `crates/rustty-global/src/lib.rs`, replace (in `pub mod seam`):

```rust
    /// PARITY: reference `internal_os.rlimit` (the `os` crate, above us).
    pub type Rlimit = u64;
```

with:

```rust
    /// PARITY: reference `internal_os.rlimit` — which is `posix.rlimit` where
    /// the platform has it, `struct {}` otherwise (rustty-os `file.rs`
    /// reproduces that as `libc::rlimit` on unix / an empty struct elsewhere).
    /// Rebound from the former `u64` placeholder to the REAL type (WS4) so
    /// `GlobalState.rlimits` can carry `rustty_os::fixMaxFiles()`'s value.
    #[cfg(unix)]
    pub type Rlimit = libc::rlimit;
    #[cfg(not(unix))]
    pub type Rlimit = ();
```

Then replace the `restore` body:

```rust
    // PORT (WS4, formerly a deferred seam): reference body calls
    // `internal_os.restoreMaxFiles(lim)`:
    //
    // pub fn restore(self: *const ResourceLimits) void {
    //     if (self.nofile) |lim| internal_os.restoreMaxFiles(lim);
    // }
    //
    // `restoreMaxFiles` is `posix.setrlimit(.NOFILE, lim) catch log warn`
    // (rustty-os file.rs:87). rustty-os sits ABOVE this leaf crate, so the
    // one-line syscall is mirrored here via libc (the same binding rustty-os
    // uses) instead of an upward import — behavior-identical, R4 acyclic.
    pub fn restore(&self) {
        #[cfg(unix)]
        if let Some(lim) = self.nofile {
            // failure is non-fatal, matching restoreMaxFiles' `catch` swallow.
            let _ = unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &lim) };
        }
    }
```

`libc::rlimit` is not `Eq`; the `ResourceLimits` struct derives only `Default`, and `Option<libc::rlimit>` needs `Copy` for `if let Some(lim) = self.nofile` — `libc::rlimit` is `Copy`, so no derive changes are needed. If the existing `#[derive(Default)]` fails because `libc::rlimit: Default` is not satisfied, `Option<T>` defaults to `None` regardless of `T` — it compiles as-is.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rustty-global`
Expected: all pass, including `restore_round_trips_nofile`. Then check downstream:

Run: `cargo build -p rustty-termio -p rustty-core -p rustty-apprt`
Expected: green (the `restore()` call site in `rustty-termio/src/command.rs:463` still type-checks; nothing else names `seam::Rlimit` concretely — verify with `grep -rn "seam::Rlimit" crates --include="*.rs"`).

- [ ] **Step 6: Commit**

```bash
git add crates/rustty-global
git commit -m "rustty-global: rlimit seam rebind + live restore (WS4 T4)"
```

---

### Task 5: `init_signals` — SIGPIPE ignore in rustty-apprt

**Files:**

- Modify: `crates/rustty-apprt/Cargo.toml` (add `libc`)
- Modify: `crates/rustty-apprt/src/lib.rs`
- Test: extend `bootstrap_tests` in the same file

**Interfaces:**

- Produces: `rustty-apprt`-private `fn init_signals()`. Task 7 calls it inside `init_with_action`.

- [ ] **Step 1: Add the dependency**

In `crates/rustty-apprt/Cargo.toml` under `[dependencies]`:

```toml
# WS4 `initSignals` (global.zig): the SIGPIPE-ignore sigaction. R3: libc binds
# the SAME syscalls the reference's std.posix wraps (the rustty-cli precedent).
libc = "0.2"
```

- [ ] **Step 2: Write the failing test**

Add to `bootstrap_tests` in `crates/rustty-apprt/src/lib.rs`:

```rust
    #[cfg(unix)]
    #[test]
    fn init_signals_ignores_sigpipe() {
        init_signals();
        let mut current: libc::sigaction = unsafe { std::mem::zeroed() };
        let rc = unsafe {
            libc::sigaction(libc::SIGPIPE, std::ptr::null(), &mut current)
        };
        assert_eq!(rc, 0);
        assert_eq!(current.sa_sigaction, libc::SIG_IGN);
    }
```

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p rustty-apprt bootstrap_tests::init_signals_ignores_sigpipe`
Expected: COMPILE FAIL — `init_signals` not found.

- [ ] **Step 4: Implement**

In `crates/rustty-apprt/src/lib.rs` (near `compute_logging`):

```rust
// fn initSignals() void { ... } (global.zig — see the verbatim body preserved
// in rustty-global's lib.rs seam comment; this is its live wiring.)
fn init_signals() {
    // Only posix systems.
    // if (comptime builtin.os.tag == .windows) return;
    #[cfg(unix)]
    unsafe {
        // var sa: p.Sigaction = .{
        //     .handler = .{ .handler = p.SIG.IGN },
        //     .mask = p.sigemptyset(),
        //     .flags = 0,
        // };
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = libc::SIG_IGN;
        libc::sigemptyset(&mut sa.sa_mask);
        sa.sa_flags = 0;

        // We ignore SIGPIPE because it is a common signal we may get
        // due to how we implement termio. When a terminal is closed we
        // often write to a broken pipe to exit the read thread. This should
        // be fixed one day but for now this helps make this a bit more
        // robust.
        // p.sigaction(p.SIG.PIPE, &sa, null);
        libc::sigaction(libc::SIGPIPE, &sa, std::ptr::null_mut());
    }
}
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p rustty-apprt bootstrap_tests`
Expected: all pass (Task 1's six + this one).

- [ ] **Step 6: Commit**

```bash
git add crates/rustty-apprt
git commit -m "rustty-apprt: initSignals SIGPIPE ignore (WS4 T5)"
```

---

### Task 6: Expose `rustty_renderer::shadertoy::init` (glslang init)

**Files:**

- Modify: `crates/rustty-renderer/src/shadertoy.rs` (near the private `glslang_init`, ~line 721)

**Interfaces:**

- Consumes: the existing private `fn glslang_init() -> Result<(), Error>` and `pub enum Error` (`shadertoy.rs:119`, has `GlslangInitFailed`).
- Produces: `rustty_renderer::shadertoy::init() -> Result<(), rustty_renderer::shadertoy::Error>`. Task 7 calls it.

- [ ] **Step 1: Write the failing test**

Append to the existing `#[cfg(test)]` module in `crates/rustty-renderer/src/shadertoy.rs` (or a new one at file end if simpler):

```rust
    // pkg/glslang init.zig `init` via the pub surface (global.zig `try glslang.init()`).
    #[test]
    fn public_glslang_init() {
        // glslang_initialize_process is refcounted/thread-safe; repeated calls
        // are safe (see the ensureInit PORT NOTE above).
        crate::shadertoy::init().expect("glslang init");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p rustty-renderer public_glslang_init`
Expected: COMPILE FAIL — `init` not found in `shadertoy`.

- [ ] **Step 3: Implement**

In `crates/rustty-renderer/src/shadertoy.rs`, directly below `fn glslang_init`:

```rust
/// The pub surface of pkg/glslang's `init` — what `global.zig` calls as
/// `try glslang.init()` (the WS4 `GlobalState.init` wiring names it from
/// rustty-apprt). Forwards to the file-local `glslang_init` verbatim.
pub fn init() -> Result<(), Error> {
    glslang_init()
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p rustty-renderer public_glslang_init`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-renderer/src/shadertoy.rs
git commit -m "rustty-renderer: pub shadertoy::init (glslang) for state.init (WS4 T6)"
```

---

### Task 7: The full `rustty_apprt::init_with_action` — the BREAK #1 body

**Files:**

- Modify: `crates/rustty-apprt/Cargo.toml`
- Modify: `crates/rustty-apprt/src/lib.rs` (replace `InitError` + `init`)
- Test: extend `bootstrap_tests`

**Interfaces:**

- Consumes: Tasks 1–6 (`compute_logging`, `init_signals`, `rustty_os::macos` sinks via `rustty_core::logging`, `seam::Rlimit`, `shadertoy::init`); `rustty_crash::init/deinit` (`init() -> Result<(), rustty_crash::sentry::InitError>`); `rustty_os::{ensureLocale, fixMaxFiles, resourcesDir}`, `rustty_os::env::getenv` (`Result<Option<GetEnvResult>, Infallible>`, `.value: String`); `rustty_os::i18n::init(&str)`; `rustty_oniguruma::testing::ensure_init` (the process-global OnceLock guard over `onig_initialize(&[utf8])` — EXACTLY the encoding set the reference app init passes; using the guard instead of raw `init::init` is the ratified fix for the parallel-test init race); `rustty_config::theme::set_resources_dir`; `rustty_build_config::{VERSION_STRING, mode_string, app_runtime, font_backend, renderer}`; `harfbuzz_sys::hb_version`, `fontconfig_sys::FcGetVersion`.
- Produces: `pub fn init_with_action(action_detected: bool) -> Result<rustty_core::Logging, InitError>`; `pub fn init() -> Result<(), InitError>` (compat wrapper, `init_with_action(false)` — the ~12 existing gtk-test callers and the smoke KAT keep compiling unchanged); `pub enum InitError { ResourcesDir(..), Locale(std::io::Error), Glslang(..), Oniguruma(..) }` (all variants `Debug`). Tasks 8 and 10 consume `init_with_action`.

- [ ] **Step 1: Add dependencies**

In `crates/rustty-apprt/Cargo.toml` under `[dependencies]`:

```toml
# WS4 full `GlobalState.init` wiring (the BREAK #1 body, global.zig):
# crash.init (sentry), oni.init (regex), the Logging type + ResourceLimits
# seam, and build-config for the init info-log lines. All strictly-lower
# members — acyclic (G7). rustty-cli is NOT here (apprt stays cli-free,
# ADV-FIX 2: detection lives in the bin / in rustty-apprt-c).
rustty-crash = { workspace = true }
rustty-oniguruma = { workspace = true }
rustty-global = { workspace = true }
rustty-build-config = { workspace = true }
# The init log lines `dependency harfbuzz=...` / `dependency fontconfig=...`
# (global.zig:133-138), emitted only when the runtime font_backend has them.
# Path deps like rustty-font-shaper's own edges (already in the build graph).
harfbuzz-sys = { path = "../../crates-sys/harfbuzz-sys" }
fontconfig-sys = { path = "../../crates-sys/fontconfig-sys" }
```

- [ ] **Step 2: Write the failing tests**

Add to `bootstrap_tests` in `crates/rustty-apprt/src/lib.rs`:

```rust
    #[test]
    fn full_init_populates_state_and_is_idempotent() {
        let logging = init_with_action(false).expect("full init");
        // STATE is set with the same logging + a populated rlimits snapshot.
        let state = rustty_core::global_state();
        assert_eq!(state.logging, logging);
        assert!(state.action.is_none());
        // fixMaxFiles returns Some on unix in practice; assert only what the
        // contract guarantees everywhere: the field was stamped by init.
        #[cfg(unix)]
        assert!(state.rlimits.nofile.is_some());

        // Second call: no re-run of side effects, same logging back.
        let again = init_with_action(false).expect("idempotent");
        assert_eq!(again, logging);

        // The init() compat wrapper also succeeds post-init.
        init().expect("compat wrapper");

        // Signals were installed by the full init.
        #[cfg(unix)]
        {
            let mut current: libc::sigaction = unsafe { std::mem::zeroed() };
            unsafe { libc::sigaction(libc::SIGPIPE, std::ptr::null(), &mut current) };
            assert_eq!(current.sa_sigaction, libc::SIG_IGN);
        }
    }
```

NOTE: this test coexists with `init_signals_ignores_sigpipe` etc. in one process; `init_with_action` must be safe to call after them (it is — the guard below).

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p rustty-apprt bootstrap_tests::full_init_populates_state_and_is_idempotent`
Expected: COMPILE FAIL — `init_with_action` not found.

- [ ] **Step 4: Replace `InitError` + `init` with the full body**

In `crates/rustty-apprt/src/lib.rs`, replace the existing `InitError` enum and `pub fn init` (lines ~50–105) with:

```rust
/// Error returned by [`init_with_action`] / [`init`] (the global-state bootstrap).
///
/// PORT NOTE: the reference `state.init()` returns `!void` over the union of
/// allocator/env/cli/os errors. The cli `DetectError` half lives at the callers
/// that run detection (the bin; `rustty_init` in rustty-apprt-c) — apprt stays
/// cli-free (ADV-FIX 2). The `getenv` error is `Infallible` off-Windows and is
/// discharged inline. `crash.init` and `i18n.init` failures are WARN-only
/// (`catch |err| log.warn`, faithful), so they do not appear here.
#[derive(Debug)]
pub enum InitError {
    /// `apprt.runtime.resourcesDir(self.alloc)` failed (global.zig `init`).
    ResourcesDir(rustty_os::resourcesdir::ResourcesDirError),
    /// `try internal_os.ensureLocale(self.alloc)` failed.
    Locale(std::io::Error),
    /// `try glslang.init()` failed.
    Glslang(rustty_renderer::shadertoy::Error),
    /// `try oni.init(&.{oni.Encoding.utf8})` failed.
    Oniguruma(rustty_oniguruma::Error),
}

/// Once-guard for the side-effecting bootstrap. The reference contract is
/// once-only (`ghostty_init` / `main()`); the port's callers (the bin, the C
/// ABI, and the gtk integration KATs) may re-enter, so the body runs exactly
/// once and later calls return the already-computed logging. A failed init
/// leaves the guard unset (retry allowed, mirroring the reference where a
/// failed `ghostty_init` means the process exits anyway).
static INIT_DONE: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

/// Initialize rustty global state (`rustty_core::STATE`) — the FULL
/// `global.zig` `GlobalState.init` body (BREAK #1, now wired), minus the two
/// pieces that live at other tiers by ratified decision:
///   * GPA/allocator selection — the allocator seam is `()` (Rust's global
///     allocator is process-wide; allocator doctrine, PORT_RULES).
///   * `cli.action.detectArgs` — detection lives ABOVE apprt (ADV-FIX 2); the
///     caller passes `action_detected` and this body applies the
///     action-implies-no-stderr rule + stashes the PRESENCE (`Option<()>`)
///     into `STATE.action`. The concrete `cli::rustty::Action` value is held
///     by the detecting tier (the bin's own state / rustty-apprt-c's stash).
pub fn init_with_action(action_detected: bool) -> Result<rustty_core::Logging, InitError> {
    use rustty_core::logging::{log_with, Level};
    use rustty_build_config as build_config;

    let mut done = INIT_DONE.lock().expect("init guard");
    if *done {
        return Ok(rustty_core::global_state().logging);
    }

    // (reference: `self.* = .{ ... }` zero-init happens implicitly — every
    // field below is computed before the single STATE.set at the end.)

    // self.gpa = ...; self.alloc = ...;
    //   Allocator doctrine: Rust's global allocator is process-wide; the
    //   GPA-vs-libc selection collapses (seam::Allocator = ()).

    // We first try to parse any action that we may be executing.
    //   (done by the caller; `action_detected` carries the result.)
    // If we have an action executing, we disable logging by default ...
    // if ((try internal_os.getenv(self.alloc, "GHOSTTY_LOG"))) |v| { ... }
    //   R2: RUSTTY_LOG. The env error is Infallible off-Windows (discharged).
    let env = rustty_os::env::getenv("RUSTTY_LOG").unwrap_or_else(|e| match e {});
    let logging = compute_logging(action_detected, env.as_ref().map(|v| v.value.as_bytes()));

    // Setup our signal handlers before logging
    init_signals();

    // Setup our Xev backend if we're dynamic
    //   PORT NOTE: `xev.dynamic` is comptime-FALSE in the port on every target
    //   (rustty-xev is the static kqueue surface on macOS; the Linux
    //   io_uring/epoll selection is dynamic.rs's own concern) — the detect call
    //   comptime-elides, exactly like the reference on a static backend.

    // Output some debug information right away
    log_with(Level::Info, "default", &format!("rustty version={}", build_config::VERSION_STRING), &logging);
    log_with(Level::Info, "default", &format!("rustty build optimize={}", build_config::mode_string()), &logging);
    log_with(Level::Info, "default", &format!("runtime={:?}", build_config::app_runtime()), &logging);
    log_with(Level::Info, "default", &format!("font_backend={:?}", build_config::font_backend()), &logging);
    // if (comptime build_config.font_backend.hasHarfbuzz())
    //     std.log.info("dependency harfbuzz={s}", .{harfbuzz.versionString()});
    if build_config::font_backend().has_harfbuzz() {
        let (mut major, mut minor, mut micro) = (0u32, 0u32, 0u32);
        // SAFETY: hb_version writes three u32 out-params (vendored harfbuzz).
        unsafe { harfbuzz_sys::hb_version(&mut major, &mut minor, &mut micro) };
        log_with(Level::Info, "default", &format!("dependency harfbuzz={major}.{minor}.{micro}"), &logging);
    }
    // if (comptime build_config.font_backend.hasFontconfig())
    //     std.log.info("dependency fontconfig={d}", .{fontconfig.version()});
    if build_config::font_backend().has_fontconfig() {
        // SAFETY: FcGetVersion takes no args, reads a compile-time constant.
        let v = unsafe { fontconfig_sys::FcGetVersion() };
        log_with(Level::Info, "default", &format!("dependency fontconfig={v}"), &logging);
    }
    log_with(Level::Info, "default", &format!("renderer={:?}", build_config::renderer()), &logging);
    // std.log.info("libxev default backend={t}", ...) — static per target (see
    // the xev PORT NOTE above).
    let xev_backend = if cfg!(target_os = "macos") { "kqueue" } else { "io_uring" };
    log_with(Level::Info, "default", &format!("libxev default backend={xev_backend}"), &logging);

    // As early as possible, initialize our resource limits.
    // self.rlimits = .init();
    let rlimits = rustty_global::ResourceLimits::init(rustty_os::fixMaxFiles());

    // Initialize our crash reporting.
    // crash.init(self.alloc) catch |err| log.warn(...);
    if let Err(err) = rustty_crash::init() {
        log_with(
            Level::Warn,
            "default",
            &format!("sentry init failed, no crash capture available err={err:?}"),
            &logging,
        );
    }

    // We need to make sure the process locale is set properly. Locale
    // affects a lot of behaviors in a shell.
    // try internal_os.ensureLocale(self.alloc);
    rustty_os::ensureLocale().map_err(InitError::Locale)?;

    // Initialize glslang for shader compilation
    // try glslang.init();
    rustty_renderer::shadertoy::init().map_err(InitError::Glslang)?;

    // Initialize oniguruma for regex
    // try oni.init(&.{oni.Encoding.utf8});
    //   Routed through the process-global OnceLock guard (ensure_init inits
    //   EXACTLY `&[Encoding::utf8()]`, the reference's set) — the ratified fix
    //   for concurrent onig_initialize (parallel-test C-init races).
    rustty_oniguruma::testing::ensure_init().map_err(InitError::Oniguruma)?;

    // Find our resources directory once for the app so every launch
    // hereafter can use this cached value.
    // self.resources_dir = try apprt.runtime.resourcesDir(self.alloc);
    let resources_dir = rustty_os::resourcesDir().map_err(InitError::ResourcesDir)?;

    // Populate the config crate's `.resources` theme-search seam (BREAK #1)
    // from the SAME resources dir (list_themes C6 note); idempotent.
    let _ = rustty_config::theme::set_resources_dir(resources_dir.clone());

    // Setup i18n
    // if (self.resources_dir.app()) |v| internal_os.i18n.init(v) catch |err| log.warn(...);
    if let Some(v) = resources_dir.app() {
        if let Err(err) = rustty_os::i18n::init(v) {
            log_with(
                Level::Warn,
                "default",
                &format!("failed to init i18n, translations will not be available err={err:?}"),
                &logging,
            );
        }
    }

    // reference `pub var state: GlobalState = undefined;` set at init — the
    // rustty STATE is a OnceLock; construct the fully-populated state and set
    // it (ignore the already-set Err, keeping the bootstrap idempotent).
    let mut state = rustty_core::GlobalState::new((), resources_dir);
    state.action = if action_detected { Some(()) } else { None };
    state.logging = logging;
    state.rlimits = rlimits;
    let _ = rustty_core::STATE.set(state);

    *done = true;
    Ok(logging)
}

/// Compat wrapper — the pre-WS4 `init()` signature (no CLI action detected:
/// the pure GUI-launch bootstrap). The gtk integration KATs and any embedded
/// caller that performs no detection use this.
pub fn init() -> Result<(), InitError> {
    init_with_action(false).map(|_| ())
}
```

Also remove the `#[cfg_attr(not(test), allow(dead_code))]` markers from Task 1 if they were added.

- [ ] **Step 5: Run tests + downstream builds**

Run: `cargo test -p rustty-apprt bootstrap_tests`
Expected: all pass.

Run: `cargo build --workspace`
Expected: green — the bin (`rustty`) still calls `rustty_apprt::init()` (compat wrapper, updated in Task 8); apprt-c's `rustty_init` still calls `rustty_apprt::init()` (updated in Task 10). If the bin's `InitError::ResourcesDir(rustty_apprt::InitError)` variant name now reads oddly (it wraps the WIDER enum), that is Task 8's cleanup — it must still compile here because the wrapped type is unchanged (`rustty_apprt::InitError`).

- [ ] **Step 6: Commit**

```bash
git add crates/rustty-apprt
git commit -m "rustty-apprt: the full state.init body — BREAK #1 wired (WS4 T7)"
```

---

### Task 8: `rustty` bin wiring — full init + log_fn delegation

**Files:**

- Modify: `crates/rustty/src/main_rustty.rs`

**Interfaces:**

- Consumes: `rustty_apprt::init_with_action` (Task 7); `rustty_core::logging::{log_with, Level}` (Task 3); the bin already deps rustty-core (Cargo.toml:42).
- Produces: the exe path runs the complete bootstrap; `log_fn` gains the macOS unified-logging sink for free.

- [ ] **Step 1: Wire `state_init` to the full init**

In `crates/rustty/src/main_rustty.rs` `fn state_init` (lines ~91–126), replace the tail after the action-detection block:

```rust
    // If we have an action executing, we disable logging by default ... —
    // WS4: the rule now lives INSIDE `init_with_action` (compute_logging),
    // which also applies the RUSTTY_LOG override; the bin mirrors the result
    // into its own GlobalState<Action> so `log_fn` reads one source of truth.
    // (The pre-WS4 inline `if state.action.is_some() { stderr = false }` is
    // deleted — it is now dead duplication.)

    // The FULL reference init body (GHOSTTY_LOG parse, signals, xev, the
    // version/... log lines, rlimits, crash.init, ensureLocale, glslang.init,
    // oni.init, resourcesDir, i18n.init) — the BREAK #1 seam, wired in
    // rustty_apprt::init_with_action (WS4).
    state.logging =
        rustty_apprt::init_with_action(state.action.is_some()).map_err(InitError::Init)?;

    Ok(state)
```

Delete the now-dead inline block:

```rust
    if state.action.is_some() {
        state.logging.stderr = false;
    }
```

and the old `rustty_apprt::init().map_err(InitError::ResourcesDir)?;` line. Rename the enum variant `InitError::ResourcesDir` → `InitError::Init` (same payload type `rustty_apprt::InitError`) and update its match arm in `main()`:

```rust
                // else => try stderr.print("invalid CLI invocation err={}\n", .{err}),
                InitError::Init(e) => {
                    let _ =
                        stderr.write_all(format!("invalid CLI invocation err={e:?}\n").as_bytes());
                }
```

Update the `ref_all_decls` test's `InitError` reification if it names the old variant (it names only `Detect` variants — no change needed; verify).

- [ ] **Step 2: Delegate `log_fn` to the shared facade**

Replace the whole `fn log_fn` body (keep the `LogLevel` enum and its `as_text` — the reference `std_options` projection comments stay):

```rust
fn log_fn(level: LogLevel, message: &str, logging: &Logging) {
    // WS4: the full logFn (macOS unified-logging + stderr sinks) is the shared
    // rustty_core::logging port; the bin delegates with its own state's
    // logging (ADV-FIX 2: the bin's GlobalState<Action> is separate from
    // rustty_core::STATE). All main_ghostty.zig sites use the `.default` scope.
    let core_level = match level {
        LogLevel::Debug => rustty_core::logging::Level::Debug,
        LogLevel::Info => rustty_core::logging::Level::Info,
        LogLevel::Warn => rustty_core::logging::Level::Warn,
        LogLevel::Err => rustty_core::logging::Level::Err,
    };
    rustty_core::logging::log_with(core_level, "default", message, logging);
}
```

Remove the now-unused `use std::io::Write;` only if the compiler flags it (the usage-banner path still writes to stdout — it will NOT be unused; leave it). Delete `LogLevel::as_text` only if it becomes dead (`ref_all_decls` may reference `LOG_LEVEL` — keep the const).

- [ ] **Step 3: Verify the exe behaviors**

```bash
cargo build -p rustty
./target/debug/rustty +version && echo "exit=$?"
```

Expected: version output, `exit=0`, NO info logs (action ⇒ stderr off).

```bash
RUSTTY_LOG=stderr ./target/debug/rustty +version 2>&1 | head -20
```

Expected: `info: rustty version=...`, `info: rustty build optimize=...`, `info: runtime=...`, `info: font_backend=...`, `info: renderer=...`, `info: libxev default backend=kqueue` lines on stderr, then the version output.

```bash
./target/debug/rustty +bogus; echo "exit=$?"
```

Expected: the "unknown CLI action" error text, `exit=1`.

Run: `cargo test -p rustty`
Expected: green (`ref_all_decls` still passes).

- [ ] **Step 4: Commit**

```bash
git add crates/rustty/src/main_rustty.rs
git commit -m "rustty: bin runs the full state.init + shared logFn (WS4 T8)"
```

---

### Task 9: rustty-apprt-c — argv storage + `detect_from_argv`

**Files:**

- Modify: `crates/rustty-apprt-c/Cargo.toml`
- Modify: `crates/rustty-apprt-c/src/lib.rs`
- Test: in-crate `tests` module in the same file

**Interfaces:**

- Consumes: `rustty_cli::action::{detect_iter, ArgsIter, DetectError}`; `rustty_cli::rustty::Action` (`Copy`, `Debug`, impls `rustty_cli::action::Action` incl. `detect_special_case`).
- Produces: apprt-c-private `static ARGV: OnceLock<Vec<Vec<u8>>>`, `static CLI_ACTION: OnceLock<Option<rustty_cli::rustty::Action>>`, `fn detect_from_argv(argv: &[Vec<u8>]) -> Result<Option<rustty_cli::rustty::Action>, rustty_cli::action::DetectError>`. Tasks 10–11 consume all three.

- [ ] **Step 1: Add the dependency**

In `crates/rustty-apprt-c/Cargo.toml` under `[dependencies]`:

```toml
# WS4 CLI-action-from-C (main_c.zig `ghostty_cli_try_action` non-null path):
# the action DETECTOR (`cli.action.detectIter`) + the concrete
# `cli.ghostty.Action` runner. rustty-cli deps rustty-core (so core can never
# name the Action type — the BREAK #1 generic stays `()` there); this TOP-SINK
# crate legitimately deps cli (no crate deps apprt-c) — acyclic (G7), the same
# ADV-FIX 2 shape as the `rustty` bin.
rustty-cli = { workspace = true }
```

- [ ] **Step 2: Write the failing tests**

In the existing `#[cfg(test)] mod tests` in `crates/rustty-apprt-c/src/lib.rs`, add:

```rust
    // The detectArgs slice of the C-path `state.init()`: detection over the
    // argv STORED by `rustty_init` (reference `std.os.argv = argv[0..argc]`
    // then `cli.action.detectArgs` walking std.process.args == std.os.argv).
    // Pure over its input — no process-global involved — so it is directly
    // unit-testable (the global-state paths are pinned by subprocess KATs).
    #[test]
    fn detect_from_argv_plus_version() {
        let argv: Vec<Vec<u8>> = vec![b"rustty".to_vec(), b"+version".to_vec()];
        let action = crate::detect_from_argv(&argv).unwrap();
        assert_eq!(action, Some(rustty_cli::rustty::Action::Version));
    }

    #[test]
    fn detect_from_argv_none() {
        let argv: Vec<Vec<u8>> = vec![b"rustty".to_vec()];
        assert_eq!(crate::detect_from_argv(&argv).unwrap(), None);
        assert_eq!(crate::detect_from_argv(&[]).unwrap(), None);
    }

    #[test]
    fn detect_from_argv_invalid() {
        let argv: Vec<Vec<u8>> = vec![b"rustty".to_vec(), b"+bogus".to_vec()];
        assert_eq!(
            crate::detect_from_argv(&argv),
            Err(rustty_cli::action::DetectError::InvalidAction),
        );
    }

    #[test]
    fn detect_from_argv_multiple() {
        let argv: Vec<Vec<u8>> =
            vec![b"rustty".to_vec(), b"+version".to_vec(), b"+help".to_vec()];
        assert_eq!(
            crate::detect_from_argv(&argv),
            Err(rustty_cli::action::DetectError::MultipleActions),
        );
    }
```

- [ ] **Step 3: Run to verify failure**

Run: `cargo test -p rustty-apprt-c detect_from_argv`
Expected: COMPILE FAIL — `detect_from_argv` not found.

- [ ] **Step 4: Implement**

In `crates/rustty-apprt-c/src/lib.rs`, above `rustty_init`:

```rust
/// PARITY: reference `std.os.argv = argv[0..argc]` (`ghostty_init`,
/// main_c.zig) — the process argv the C host hands us, stored once. Consumed
/// by the CLI-action detection below (its only reference consumer).
static ARGV: std::sync::OnceLock<Vec<Vec<u8>>> = std::sync::OnceLock::new();

/// The concrete detected CLI action (`state.action`'s VALUE). rustty-core's
/// `GlobalState<Action>` is bound `Action = ()` (rustty-cli deps rustty-core —
/// the concrete type CANNOT live there without a cycle; R4). The ratified
/// split: `rustty_core::STATE.action: Option<()>` carries PRESENCE (set by
/// `init_with_action`), and this top-sink stash carries the VALUE for
/// `rustty_cli_try_action` — together they are the reference's one field.
static CLI_ACTION: std::sync::OnceLock<Option<rustty_cli::rustty::Action>> =
    std::sync::OnceLock::new();

/// The stored-argv view of the reference `std.process.args` iterator that
/// `cli.action.detectArgs` walks (all args INCLUDING argv0 — argv0 never
/// starts with `+` and never matches a special case, so it is harmless).
struct ArgvIter<'a> {
    argv: &'a [Vec<u8>],
    i: usize,
}

impl rustty_cli::action::ArgsIter for ArgvIter<'_> {
    fn next(&mut self) -> Option<Vec<u8>> {
        let v = self.argv.get(self.i)?.clone();
        self.i += 1;
        Some(v)
    }
}

/// `self.action = try cli.action.detectArgs(cli.ghostty.Action, self.alloc);`
/// (global.zig init) — the C-path variant over the STORED argv (the reference
/// detectArgs walks std.process.args, which on posix IS the stored
/// `std.os.argv`).
fn detect_from_argv(
    argv: &[Vec<u8>],
) -> Result<Option<rustty_cli::rustty::Action>, rustty_cli::action::DetectError> {
    let mut iter = ArgvIter { argv, i: 0 };
    rustty_cli::action::detect_iter::<rustty_cli::rustty::Action, _>(&mut iter)
}
```

(If `-D warnings` flags `ARGV`/`CLI_ACTION` as unused before Task 10 lands, add `#[allow(dead_code)]` and remove it in Task 10.)

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p rustty-apprt-c detect_from_argv`
Expected: 4 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/rustty-apprt-c
git commit -m "rustty-apprt-c: argv store + detectArgs over stored argv (WS4 T9)"
```

---

### Task 10: `rustty_init` — the full C-path bootstrap

**Files:**

- Modify: `crates/rustty-apprt-c/src/lib.rs` (`rustty_init` body + doc, lines ~168–223)
- Test: extend the in-crate `tests` module

**Interfaces:**

- Consumes: Task 9 statics + `detect_from_argv`; `rustty_apprt::init_with_action` (Task 7); `rustty_core::logging::{log, Level}` (Task 3).
- Produces: `rustty_init` detects + stashes the action and runs the full bootstrap. Signature is UNCHANGED (`extern "C" fn rustty_init(argc: usize, argv: *mut *mut c_char) -> c_int`) — no header/A4 impact. Task 11 relies on `CLI_ACTION` being stashed iff `global_state().action.is_some()`.

- [ ] **Step 1: Write the failing tests**

Add to the `tests` module:

```rust
    // reference `ghostty_init`: a detect failure means `state.init()` fails →
    // log err + return 1 (main_c.zig:105). No global state is stamped.
    #[test]
    fn rustty_init_invalid_action_returns_1() {
        let argv0 = std::ffi::CString::new("rustty").unwrap();
        let arg1 = std::ffi::CString::new("+bogus").unwrap();
        let mut argv = [
            argv0.as_ptr() as *mut c_char,
            arg1.as_ptr() as *mut c_char,
        ];
        // SAFETY: argv points at 2 valid NUL-terminated strings.
        let rc = unsafe { crate::rustty_init(2, argv.as_mut_ptr()) };
        assert_eq!(rc, 1);
    }
```

- [ ] **Step 2: Run to verify failure**

Run: `cargo test -p rustty-apprt-c rustty_init_invalid_action_returns_1`
Expected: FAIL — current `rustty_init` ignores argv and returns 0.

NOTE on in-process test coexistence: this test only exercises the ERROR path (no `CLI_ACTION`/`STATE` stamp on error — see Step 3's body: detection happens BEFORE any stash). The pre-existing `cli_try_action_null_action_returns` test calls `rustty_init(0, null)` (no action) — both orders are safe. NO in-process test may init with a VALID `+action` (that would stash `Some` process-wide and make the null-action test exit); valid-action flows are subprocess-only (Task 11).

- [ ] **Step 3: Implement the full `rustty_init`**

Replace the `rustty_init` body (keep + update the doc comment: delete the "accepted but not stored" / "BREAK #1 not yet ported" paragraphs, note the full wiring):

```rust
#[no_mangle]
pub unsafe extern "C" fn rustty_init(argc: usize, argv: *mut *mut c_char) -> c_int {
    use rustty_core::logging::{log, Level};

    // reference: assert(builtin.link_libc);  (Rust `std` always links libc.)

    // reference: std.os.argv = argv[0..argc];
    let stored: Vec<Vec<u8>> = if argv.is_null() {
        Vec::new()
    } else {
        (0..argc)
            .map(|i| {
                // SAFETY: caller contract — argv points to argc valid
                // NUL-terminated C strings (the process argv).
                let p = unsafe { *argv.add(i) };
                if p.is_null() {
                    Vec::new()
                } else {
                    unsafe { std::ffi::CStr::from_ptr(p) }.to_bytes().to_vec()
                }
            })
            .collect()
    };
    // Once-only like the reference `ghostty_init`; a repeat call keeps the
    // first argv (ignore the Err). Detection below uses the LOCAL copy so a
    // (contract-violating) second call cannot observe torn state.
    let _ = ARGV.set(stored.clone());

    // state.init():
    //   self.action = try cli.action.detectArgs(cli.ghostty.Action, self.alloc);
    //   (the try: a DetectError fails state.init() → main_c logs + returns 1.)
    let action = match detect_from_argv(&stored) {
        Ok(action) => action,
        Err(err) => {
            // std.log.err("failed to initialize ghostty error={}", .{err});
            log(
                Level::Err,
                "default",
                &format!("failed to initialize rustty error={err:?}"),
            );
            return 1;
        }
    };
    let _ = CLI_ACTION.set(action);

    //   ... the remainder of state.init() (logging config with the
    //   action-implies-no-stderr rule, signals, log lines, rlimits, crash,
    //   locale, glslang, oniguruma, resourcesDir, i18n) — the wired BREAK #1
    //   body; `action.is_some()` mirrors presence into STATE.action.
    match rustty_apprt::init_with_action(action.is_some()) {
        Ok(_) => 0,
        Err(err) => {
            // std.log.err("failed to initialize ghostty error={}", .{err});
            log(
                Level::Err,
                "default",
                &format!("failed to initialize rustty error={err:?}"),
            );
            1
        }
    }
}
```

PORT NOTE to include in the doc comment: the error log is now routed through the ported `logFn` (suppressed unless `RUSTTY_LOG=stderr`, faithful to the reference lib-mode default `stderr=false`) — this replaces the pre-WS4 unconditional `eprintln!` precedent.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p rustty-apprt-c`
Expected: all pass — including the PRE-EXISTING `cli_try_action_null_action_returns` (its `rustty_init(0, null)` now runs the FULL bootstrap: detection over empty argv → `None` → GUI path) and the smoke/abi/panic-policy integration tests.

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-apprt-c/src/lib.rs
git commit -m "rustty-apprt-c: rustty_init runs the full state.init (WS4 T10)"
```

---

### Task 11: `rustty_cli_try_action` — the non-null (run + exit) arm

**Files:**

- Modify: `crates/rustty-apprt-c/src/lib.rs` (`rustty_cli_try_action`, lines ~225–298)
- Test: subprocess KAT in the in-crate `tests` module

**Interfaces:**

- Consumes: `CLI_ACTION` (Task 9/10); `rustty_cli::rustty::Action::run(self) -> Result<u8, rustty_cli::rustty::Error>`; `rustty_core::logging`.
- Produces: the complete `ghostty_cli_try_action` semantics through the C ABI. Signature unchanged.

- [ ] **Step 1: Write the failing subprocess KAT**

Add to the `tests` module:

```rust
    // The NON-NULL path of `rustty_cli_try_action` (main_c.zig): with `+version`
    // in argv, `rustty_init` stashes the action and `rustty_cli_try_action`
    // RUNS it and EXITS with its code — so the whole flow runs in a CHILD
    // process (the panic_policy.rs re-exec pattern); the parent asserts exit 0
    // and the version output. This is the CLI-action-THROUGH-C KAT.
    #[test]
    fn cli_try_action_version_runs_and_exits() {
        const MARKER: &str = "RUSTTY_CLI_ACTION_CHILD";
        if std::env::var_os(MARKER).is_some() {
            // Child: exactly what the Swift host does (main.swift:8,31).
            let argv0 = std::ffi::CString::new("rustty").unwrap();
            let arg1 = std::ffi::CString::new("+version").unwrap();
            let mut argv = [
                argv0.as_ptr() as *mut c_char,
                arg1.as_ptr() as *mut c_char,
            ];
            // SAFETY: argv points at 2 valid NUL-terminated strings.
            let rc = unsafe { crate::rustty_init(2, argv.as_mut_ptr()) };
            assert_eq!(rc, 0, "rustty_init with +version must succeed");
            // SAFETY: global state initialized by rustty_init above.
            unsafe { crate::rustty_cli_try_action() };
            unreachable!("+version must exit the process");
        }
        let exe = std::env::current_exe().expect("current_exe");
        let out = std::process::Command::new(exe)
            .args(["--exact", "tests::cli_try_action_version_runs_and_exits", "--nocapture"])
            .env(MARKER, "1")
            .output()
            .expect("spawn child test process");
        assert!(
            out.status.success(),
            "child failed: status={:?} stderr={}",
            out.status,
            String::from_utf8_lossy(&out.stderr),
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains(rustty_identity::VERSION),
            "version output missing from: {stdout}",
        );
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p rustty-apprt-c cli_try_action_version_runs_and_exits`
Expected: FAIL — the child hits the `unreachable!("CLI-action-from-C path is deferred: ...")` and aborts (nonzero child status).

- [ ] **Step 3: Implement the Some arm**

Replace the `rustty_cli_try_action` body (delete the `TODO(port)` tail comment and the `unreachable!`; update the doc comment's "What's deferred" paragraph to "wired (WS4)"):

```rust
#[no_mangle]
pub unsafe extern "C" fn rustty_cli_try_action() {
    use rustty_core::logging::{log, Level};

    // const action = state.action orelse return;
    //   Presence lives in core STATE (`Option<()>`); the concrete value lives
    //   in the top-sink CLI_ACTION stash (see its doc) — set together by
    //   `rustty_init`, so a Some presence ALWAYS has a Some stash.
    let action = match rustty_core::global_state().action {
        Some(()) => CLI_ACTION
            .get()
            .copied()
            .flatten()
            .expect("CLI_ACTION is stashed by rustty_init whenever state.action is Some"),
        // (no action → GUI launch: return WITHOUT exiting — the windowed path)
        None => return,
    };

    // std.log.info("executing CLI action={}", .{action});
    log(Level::Info, "default", &format!("executing CLI action={action:?}"));

    // posix.exit(action.run(state.alloc) catch |err| {
    //     std.log.err("CLI action failed error={}", .{err});
    //     posix.exit(1);
    // });
    // posix.exit(0);
    //   PORT NOTE: `action.run()` takes no allocator (dropped tree-wide);
    //   `posix.exit(action.run(...))` exits with the action's u8 code, and the
    //   reference's trailing `posix.exit(0)` is unreachable (kept as comment).
    std::process::exit(match action.run() {
        Ok(code) => i32::from(code),
        Err(err) => {
            log(Level::Err, "default", &format!("CLI action failed error={err:?}"));
            std::process::exit(1);
        }
    });
    // posix.exit(0); — unreachable after the exit above (reference dead tail).
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p rustty-apprt-c`
Expected: ALL pass — the new subprocess KAT, the invalid-action test, `cli_try_action_null_action_returns`, detect tests, and the integration suites (abi_layout, smoke, panic_policy, macos_window_host).

- [ ] **Step 5: Commit**

```bash
git add crates/rustty-apprt-c/src/lib.rs
git commit -m "rustty-apprt-c: rustty_cli_try_action non-null run+exit arm (WS4 T11)"
```

---

### Task 12: macOS end-to-end — Rustty.app relaunch + CLI action through the app binary

**Files:** none (verification only; fix regressions where they surface)

- [ ] **Step 1: Full workspace gates**

```bash
cargo build --workspace
cargo test -p rustty-global -p rustty-os -p rustty-core -p rustty-renderer -p rustty-apprt -p rustty-apprt-c -p rustty -p rustty-cli
cargo clippy --workspace --all-targets
cargo fmt --all -- --check
bash .port/phase-2/check_acyclic.sh 2>/dev/null || bash scripts/check_acyclic.sh 2>/dev/null || true  # run the acyclicity gate from wherever it lives (locate with: git ls-files | grep check_acyclic)
```

Expected: all green.

- [ ] **Step 2: Rebuild the app**

```bash
bash macos/build-rustty.sh
```

Expected: `BUILD SUCCEEDED`.

- [ ] **Step 3: RUSTTY_LOG smoke on the app**

```bash
APP=$(ls -d ~/Library/Developer/Xcode/DerivedData/Rustty-*/Build/Products/Debug/Rustty.app | head -1)
RUSTTY_LOG=stderr "$APP/Contents/MacOS/Rustty" 2>&1 | head -12 &
sleep 8; kill %1 2>/dev/null
```

Expected: the init info lines (`info: rustty version=...`, `info: renderer=...`, etc.) appear on stderr AND the terminal window opens and renders (visually confirm a shell prompt, then it is killed). Also verify unified logging:

```bash
log show --last 2m --predicate 'subsystem=="io.airely.rustty"' | head -8
```

Expected: the same init messages in the unified log (the `macos` sink defaults ON).

- [ ] **Step 4: CLI action through the C ABI in the real app**

```bash
"$APP/Contents/MacOS/Rustty" +version; echo "exit=$?"
```

Expected: the version output on stdout, `exit=0`, NO window (the Swift host's `rustty_cli_try_action()` call at `main.swift:31` exits before app launch).

```bash
"$APP/Contents/MacOS/Rustty" +bogus; echo "exit=$?"
```

Expected: `exit=1` (init fails on InvalidAction; the Swift host logs `rustty_init failed` and exits — `main.swift:8-10`).

- [ ] **Step 5: Commit any fixes**

If Steps 1–4 required fixes, commit them:

```bash
git add -A && git commit -m "rustty-apprt-c: WS4 end-to-end fixes (WS4 T12)"
```

---

### Task 13: Both-platform gate, ledger strikes, merge

**Files:**

- Modify: `.port/phase-2/DEFERRALS.md` (lines ~1894–1908 "rustty-apprt embedded E0" items 1–2; line ~3077 note; the ~4502–4526 `rustty_cli_try_action` entry)
- Modify: `.port/phase-2/INTEGRATION_PARITY.md` (§3.5 item 5, ~line 200)

- [ ] **Step 1: Linux gate**

```bash
git push -u origin ws4-app-bootstrap
ssh rvm 'cd ~/projects/rustty && git fetch && git checkout ws4-app-bootstrap && cargo build --workspace && cargo test -p rustty-global -p rustty-apprt -p rustty-apprt-c'
```

Expected: green. (Linux notes: `Logging::default().macos == false`; the os_log module is macOS-gated; the harfbuzz/fontconfig log lines are LIVE there — `FontconfigFreetype` has both; the gtk tests keep passing on the `init()` wrapper.) Fix any breakage on the branch and re-run BOTH platforms.

- [ ] **Step 2: Ledger strikes**

In `.port/phase-2/DEFERRALS.md`:

1. E0 entry item 1 ("The full `state.init()` port", ~line 1894): rewrite to `**LANDED (WS4, 2026-07-23):** the full body is `rustty_apprt::init_with_action`(logging config via RUSTTY_LOG, initSignals SIGPIPE, rlimits, crash.init, ensureLocale, glslang.init, oni.init, i18n.init, the init info-log lines); detection stays above apprt (ADV-FIX 2). Remaining under OTHER entries: tree-wide`std.log.scoped` facade routing (the per-crate scoped-log sites — pre-existing, not created here).`
2. E0 entry item 2 (argv storage): mark LANDED (WS4) — `rustty_init` stores argv in the apprt-c `ARGV` OnceLock and detection consumes it.
3. The `rustty_cli_try_action` entry (~4502): mark the non-null path LANDED (WS4) with the stash-split note (presence in `STATE.action: Option<()>`, value in the apprt-c `CLI_ACTION` stash — the acyclic realization of the reference field; the entry's "bind Action in rustty-core" hypothesis was impossible: rustty-cli deps rustty-core).
4. Line ~3077 (`ensureLocale`/`glslang.init`/`oni.init`/`i18n.init` "stay BREAK #1 deferred") and the ~2961 `theme::set_resources_dir` note: update to reflect the WS4 landing.
5. Search for remaining stale claims: `grep -n "BREAK #1" .port/phase-2/DEFERRALS.md` — update every hit that asserts the init body is still deferred.

In `.port/phase-2/INTEGRATION_PARITY.md` §3.5 item 5 (~line 200): rewrite to record that the null AND non-null paths are now faithful (`detectArgs` runs in `rustty_init`; a CLI action forwarded to the embedded lib runs + exits, matching ghostty).

Also update the stale in-code seam comments this WS obsoleted (verify each still exists before editing): `crates/rustty-apprt-c/src/lib.rs:801` ("stays deferred"), `crates/rustty-vt-c`-precedent "no scoped-log sink in this leaf" notes at the sites Task 10/11 rewired, and `crates/rustty/src/main_rustty.rs`'s "deferred seam" doc-block headers (lines ~58–90).

```bash
git add .port/phase-2/DEFERRALS.md .port/phase-2/INTEGRATION_PARITY.md crates
git commit -m "ledger: strike BREAK #1 + cli_try_action + parity 3.5-5 (WS4 T13)"
```

- [ ] **Step 3: Final both-platform re-verify + merge**

```bash
cargo build --workspace && cargo test -p rustty-apprt-c -p rustty-apprt
ssh rvm 'cd ~/projects/rustty && git pull && cargo build --workspace'
git checkout main && git merge --no-ff ws4-app-bootstrap -m "merge WS4: full app bootstrap (BREAK #1) + CLI-action-from-C"
git push origin main
git branch -d ws4-app-bootstrap && git push origin --delete ws4-app-bootstrap
ssh rvm 'cd ~/projects/rustty && git checkout main && git pull'
```

Mark WS4 as authored+executed in `docs/superpowers/plans/2026-07-23-rustty-completion-ROADMAP.md`'s checklist if the roadmap is being tracked there.

---

## Self-Review Notes (already applied)

- **Spec coverage:** GPA/allocator selection → documented no-op per allocator doctrine (Task 7 comments); detectArgs → bin (pre-existing) + C path (Tasks 9–10); GHOSTTY_LOG → `RUSTTY_LOG` per the R2 env-var retarget precedent (Task 1; verification uses `RUSTTY_LOG=stderr`); crash.init (Task 7); ensureLocale + i18n.init (Task 7); SIGPIPE (Task 5); rlimits (Tasks 4, 7); glslang/oniguruma (Tasks 6, 7); xev.detect → comptime-elided no-op with PORT NOTE (Task 7); init log lines incl. harfbuzz/fontconfig (Task 7); `rustty_cli_try_action` Some arm (Task 11); embedded-harness KATs (Tasks 10–11); Rustty.app relaunch + RUSTTY_LOG + `+version`-through-C (Task 12); both-platform gate + ledger strikes + merge (Task 13). `crash.deinit`/`GlobalState.deinit` stay faithfully un-run on the exit paths (the reference `posix.exit` skips defers — pre-existing PORT NOTE in `main_rustty.rs:180-184`).
- **Type consistency:** `Logging` is `Copy`+`PartialEq` (existing derives) — `assert_eq!` in tests is valid; `init_with_action` returns `Logging` by value; `detect_from_argv` returns `Result<Option<Action>, DetectError>` and both `Action` and `DetectError` are `Copy`+`PartialEq` (existing derives); `Action::run(self) -> Result<u8, Error>` (rustty.rs:322).
- **Known judgment calls (do not re-litigate mid-execution):** (1) concrete-Action stash in apprt-c, presence-only in core — forced by `rustty-cli → rustty-core`; (2) `init()` kept as compat wrapper so the 12 Linux gtk KATs are untouched; (3) `rustty_init`/`cli_try_action` logs route through the ported logFn (default-suppressed in lib mode, faithful) instead of the pre-WS4 unconditional `eprintln!`; (4) oniguruma via `testing::ensure_init` (identical encoding set, race-safe).
