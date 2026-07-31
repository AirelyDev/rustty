# Shared non-macOS runtime parity audit

Audit date: 2026-07-31  
Rustty: `5243303fbb2f2cfd0832edfb8fa49d72ce322d81`  
Ghostty: `6ad1fe7d8cbda36c77b337a96c9bea8a77883699`

## Result

Two P1 target-build defects are verified. Both are target reachability failures,
not textual-parity nits:

| ID | Severity | Platform | Result |
|---|---:|---|---|
| SR-001 | P1 | Windows | `rustty-os` deliberately and accidentally fails to compile for the installed MSVC target, cutting off the already-written ConPTY/termio path. |
| SR-002 | P1 | FreeBSD; later-stage Windows | Rustty selects OpenGL for FreeBSD in build-config, but compiles and exports the OpenGL renderer only on Linux, leaving `rustty_renderer::Renderer` undefined. |

No additional verified defect was promoted from the reviewed config, input,
shell-integration, terminfo, crash, inspector, font, or host-runtime candidates.
The limitations and unresolved leads at the end are intentionally not findings.

## SR-001 — Windows shared runtime cannot compile

**Severity:** P1  
**Scope:** Windows, shared runtime/library (`rustty-os` transitively blocks
termio/core/renderer and an app-runtime-none style build)  
**Confidence:** high

### Reference contract

Ghostty has live Windows terminal I/O, not merely placeholder types:

- `src/termio/Exec.zig:1013-1033` constructs the child command and hands the
  Windows pseudo console to it at line 1031.
- `src/termio/Exec.zig:1075-1085` returns ConPTY's `out_pipe`/`in_pipe` on
  Windows.
- `src/termio/Exec.zig:1770-1817` implements the Windows reader with
  `ReadFile`, `processOutput`, `PeekNamedPipe`, and the quit path.
- `.github/workflows/test.yml:749-761` is the checked-in Windows job that builds
  full `libghostty` with `-Dapp-runtime=none` (GNU ABI), separate from the
  lib-vt-only job at lines 728-747.

The reference also demonstrates why its passwd compile error is not a global
Windows build failure: `src/os/passwd.zig:34-37` places `@compileError` inside
the lazily analyzed `get`; `src/os/homedir.zig:14-18,79-83` selects
`homeWindows` and never instantiates `passwd.get` on Windows.

### Rustty behavior

`cargo check --target x86_64-pc-windows-msvc -p rustty-os` fails with 25 errors.
The earliest independent failures are:

- `crates/rustty-os/src/page_allocator.rs:350-365` explicitly says the Windows
  VM mapping path is unported and invokes `compile_error!` in both mapping
  methods.
- `crates/rustty-os/src/lib.rs:93-101` compiles `passwd`, `path`, `pipe`, and
  `pty` on every non-WASM target; consequently
  `crates/rustty-os/src/passwd.rs:182-187` turns the reference's lazy function
  error into an eager Rust module compile failure on Windows.
- `crates/rustty-os/Cargo.toml:67-71` makes `libc` Unix-only while
  `crates/rustty-os/src/locale.rs:417-453`, `mach.rs:153-195`, and
  `path.rs:53` still compile references to it on Windows.
- `crates/rustty-os/src/pty.rs:530,649` also fails because
  `WindowsPtyError` does not implement `std::error::Error` for the attempted
  boxed conversion.

### Concrete consequence and observed verification

No Windows consumer can compile `rustty-os`, so the shared Windows runtime
cannot reach Rustty's ConPTY spawn/read/write code in `rustty-termio`. The
installed target is `x86_64-pc-windows-msvc`; the command reached crate
type-checking and failed in Rustty source rather than at target installation.

Observed output is in
`evidence/shared-runtime/windows-rustty-os-check.log`. It contains the two
page-allocator compile errors, the passwd compile error, the unresolved libc
uses, and the PTY error-trait failure.

### Counter-check / falsification attempt

1. `cargo check --target x86_64-pc-windows-msvc -p rustty-identity` succeeds
   (`windows-toolchain-control.log`), disproving a missing Rust target or
   generally broken cross-check setup.
2. `cargo test -p rustty-os` passes 69 unit tests plus the live CF release test
   on the host (`host-rustty-os-tests.log`), disproving a platform-neutral crate
   break and isolating the failure to target selection.
3. I attempted `zig build test -Dtarget=x86_64-windows-msvc
   -Dapp-runtime=none` with all cache/output paths redirected beneath this audit
   directory. It stopped before compilation because the sandbox could not
   resolve `deps.files.ghostty.org` for HarfBuzz
   (`windows-ghostty-test.log`). This does not weaken the Rustty observation;
   it is recorded as an environment limitation rather than claimed as a local
   Ghostty build success. The reference's checked-in Windows job and live code
   establish its contract.

### Root-cause boundary

This finding groups multiple independent `rustty-os` compilation stops because
their user consequence and remediation boundary are one target gate: Windows
cannot enter the shared runtime at all. Fixing only the explicit page allocator
error would still leave passwd/libc/PTY failures.

## SR-002 — OpenGL renderer is excluded from FreeBSD

**Severity:** P1  
**Scope:** FreeBSD shared renderer/core; the same selector also becomes a
later-stage Windows blocker after SR-001  
**Confidence:** high for the cfg/export defect; host limitation prevents a full
FreeBSD cargo check

### Reference contract

- `src/renderer/backend.zig:10-21` selects WebGL only for wasm, Metal for
  Darwin, and returns OpenGL for every other OS, which includes FreeBSD and
  Windows.
- `src/renderer.zig:36-42` instantiates `GenericRenderer(OpenGL)` whenever that
  backend is selected.
- `src/build/Config.zig:20-28` carries OpenGL as the default renderer, and
  `src/build/Config.zig:220-227` explicitly recognizes FreeBSD alongside Linux
  in another supported non-Darwin build choice (i18n).

### Rustty behavior

Rustty's build selection agrees with Ghostty but its implementation gate does
not:

- `crates/rustty-build-config/src/lib.rs:73-86` recognizes FreeBSD as its own
  OS.
- `crates/rustty-build-config/src/renderer.rs:18-30` selects OpenGL for every
  non-wasm, non-Darwin target, including FreeBSD.
- `crates/rustty-renderer/src/lib.rs:98-106` declares the OpenGL module only
  under `#[cfg(target_os = "linux")]`.
- `crates/rustty-renderer/src/lib.rs:181-189` likewise exports
  `generic::Renderer` only on Linux. The remaining aliases are macOS Metal at
  lines 178-179 and wasm WebGL at lines 197-198; there is no FreeBSD alias.
- `crates/rustty-renderer/src/generic.rs:7604-7612` gates the actual OpenGL
  implementation module and re-export to Linux only.
- `crates/rustty-core/src/renderer_seam.rs:252-253` expects
  `rustty_renderer::Renderer` on *every* non-macOS target, making the missing
  alias a direct core compile failure rather than an unused API omission.
- `crates/rustty-renderer/Cargo.toml` also places the `gl` and `khronos-egl`
  dependencies under `[target.'cfg(target_os = "linux")'.dependencies]`, so
  widening only the Rust modules would still leave FreeBSD without its backend
  dependencies.

### Concrete consequence and observed verification

For `x86_64-unknown-freebsd`, Rust evaluates `target_os="freebsd"`; therefore
none of the Linux-only OpenGL module, implementation module, public Renderer
alias, or GL dependencies exists. `rustty-core` nevertheless names the missing
`rustty_renderer::Renderer`. The FreeBSD shared renderer/core cannot compile,
so no terminal surface can be built on that target.

The read-only verifier
`scripts/shared-runtime/target_contract.py` records the relevant selectors and
uses `rustc --print cfg --target x86_64-unknown-freebsd`. Its observed output in
`evidence/shared-runtime/target-contract.log` is:

```text
rustty renderer linux/freebsd cfg entries: [(105, '#[cfg(target_os = "linux")]'), (188, '#[cfg(target_os = "linux")]')]
ghostty non-Darwin renderer fallback: .../src/renderer/backend.zig:21 return .opengl;
freebsd target cfg present: True
```

The script's list is deliberately scoped to the crate-root Linux/FreeBSD
renderer exports; it finds no FreeBSD-capable entry.

### Counter-check / falsification attempt

1. I looked for an alternate FreeBSD renderer alias or platform-neutral export
   in the entire renderer crate. The only concrete aliases are the mutually
   exclusive macOS, Linux, and wasm gates cited above.
2. Rustty's own backend selector returns OpenGL on FreeBSD
   (`rustty-build-config/src/renderer.rs:26-29`). This rules out an intentional
   `Renderer`-less backend choice and instead exposes an internal cfg
   contradiction.
3. Ghostty's selector was checked for a hidden Linux-only condition; it uses
   `isDarwin()` followed by an unconditional OpenGL fallback, not a Linux test.
4. A full cargo check was not possible because the FreeBSD Rust standard
   library target is not installed. This is why the finding claims the
   deterministic cfg/export failure, not a captured compiler diagnostic.

## Reviewed surfaces without a promoted finding

- **Config/actions/defaults:** the existing field set-diff script reports no
  public field difference against the checked-out Ghostty head (206 versus 206;
  only Ghostty's six private bookkeeping fields are omitted). The focused
  scrollback config test passes. Action/keycode/config source and platform cfg
  sites were sampled; no independent current drift was verified.
- **Shell integration/resources:** all 25 `rustty-termio` shell-integration
  tests selected by `cargo test -p rustty-termio shell_integration` pass,
  including missing-resource, bash/zsh/nushell, XDG, argument, and non-UTF-8
  cases (`host-shell-integration-tests.log`). Rustty carries the Bash and Zsh
  assets under `crates/rustty-extra/shell-integration/` with retargeted names.
- **Terminfo:** all five `rustty-terminfo` tests pass, including encode and
  default-profile identity (`host-terminfo-tests.log`).
- **Stub/panic sweep:** production `unimplemented!`/`todo!` sites in scope were
  enumerated (`panic-macros.txt`). The many core hits are test-only dummy
  runtimes. `Backend::childExitedAbnormally` is production text but is not the
  live abnormal-exit path: both Rustty and Ghostty's Surface implement that UI
  fallback directly, and repo-wide reachability found no call to the backend
  method. It was not promoted.
- **Font/font-shaper, renderer/render-types, crash, inspector, CLI:** inspected
  target gates and simplified/stub markers plus existing audit leads. The
  Linux/FreeBSD fontconfig path is now implemented despite stale comments that
  still call it deferred. No additional concrete consequence met the evidence
  threshold in the available host environment.

## Unresolved leads and limitations

These are not verified findings:

1. **Linux live GUI/runtime behavior:** this macOS host cannot exercise GTK,
   EGL/GLArea, Fontconfig, portals, or distribution resource lookup as a shipped
   Linux process. Source review and host tests are not a substitute for that
   execution.
2. **FreeBSD beyond the renderer gate:** the FreeBSD standard library target is
   absent, so later compiler failures in OS, Fontconfig, termio, or packaging
   may remain hidden behind SR-002.
3. **Windows errors after `rustty-os`:** the broad cross-check also exposed C/C++
   sys-crate cross-toolchain header failures (`zlib-sys`, `simdutf-sys`,
   `highway-sys`). From this macOS host I cannot distinguish missing local MSVC
   SDK/sysroot configuration from target-port defects, so those are not
   promoted. SR-001 relies only on Rust source errors that are independent of
   those toolchain headers.
4. **Ghostty Windows local check:** blocked by sandbox DNS while resolving a
   pinned HarfBuzz archive, as documented above.

## Evidence index

- `evidence/shared-runtime/baseline.log`
- `evidence/shared-runtime/windows-rustty-os-check.log`
- `evidence/shared-runtime/windows-toolchain-control.log`
- `evidence/shared-runtime/windows-ghostty-test.log`
- `evidence/shared-runtime/target-contract.log`
- `evidence/shared-runtime/host-rustty-os-tests.log`
- `evidence/shared-runtime/host-shell-integration-tests.log`
- `evidence/shared-runtime/host-terminfo-tests.log`
- `evidence/shared-runtime/host-config-focused-tests.log`
- `evidence/shared-runtime/panic-macros.txt`
- `evidence/shared-runtime/linux-cfg-sites.txt`
- `evidence/shared-runtime/windows-markers.txt`

No production file was modified and no worktree was created.
