# Build, distribution, Windows, and browser/WASM parity audit

Audit date: 2026-07-31

## Baseline and method

- Rustty: `5243303fbb2f2cfd0832edfb8fa49d72ce322d81`
- Ghostty oracle: `6ad1fe7d8cbda36c77b337a96c9bea8a77883699`
- Toolchain: `rustc 1.93.1`, `cargo 1.93.1`
- Host: `aarch64-apple-darwin`
- Installed Rust targets: `aarch64-apple-darwin`, `wasm32-unknown-unknown`,
  `wasm32-wasip1`, `x86_64-pc-windows-msvc`
- Initial and final production-tree status: no tracked production changes;
  `git status --short` reported only `?? .port/codex/`, the shared audit tree.

This workstream compared build matrices, target gates, native/platform
dependencies, generated and installed resources, Windows library reachability,
and emitted WASM structure. Both product trees were treated as read-only. The
only authored analyzer is `.port/codex/scripts/inspect-wasm-table.mjs`; build
artifacts are under `.port/codex/evidence/platform-targets/`.

## Verified findings

| ID | Severity | Target | Finding | Confidence |
|---|---|---|---|---|
| PT-001 | P1 | Windows | The shipped VT C library dependency graph cannot compile because `rustty-os` eagerly compiles POSIX modules and an explicitly unported Windows page allocator. | High |
| PT-002 | P1 | FreeBSD | The default executable selects GTK, but Rustty compiles GTK only on Linux and reaches `unimplemented!` on FreeBSD. | High |
| PT-003 | P2 | Browser/WASM | The VT WASM build emits a private, fixed-size callback table; the standalone patcher removes the cap but still does not export the table. | High |
| PT-004 | P2 | Linux | A default Cargo build enables GTK but neither real window-protocol backend, unlike Ghostty's GTK-target auto-detection. | High |
| PT-005 | P2 | Linux distribution | Linux desktop templates exist but no Rustty build/install path processes or installs them. | High |

### PT-001 — Windows VT C library is stopped by Rust source blockers

**Ghostty contract.** Ghostty's lib-vt build matrix explicitly includes
`x86_64-windows-gnu` and WASM (`ghostty/.github/workflows/test.yml:573-586`). It
also runs lib-vt tests, builds the library, and tests static linking on a native
Windows runner (`ghostty/.github/workflows/test.yml:728-747`), plus a GNU-ABI
headless build (`ghostty/.github/workflows/test.yml:749-761`). This is backed by
target implementations rather than a matrix-only assertion: terminal pages use
the generic page allocator off Darwin (`ghostty/src/terminal/PageList.zig:518-530`),
and Ghostty supplies `VirtualAlloc`/`VirtualFree` behavior on Windows
(`ghostty/src/terminal/page.zig:30-37`, `ghostty/src/terminal/page.zig:58-81`).
Ghostty avoids importing POSIX passwd C declarations on Windows and leaves the
unsupported `get` body lazy (`ghostty/src/os/passwd.zig:19-24`,
`ghostty/src/os/passwd.zig:34-41`); its Windows home-directory path does not call
passwd (`ghostty/src/os/homedir.zig:12-23`, `ghostty/src/os/homedir.zig:79-84`).

**Rustty divergence.** `libc` is declared only for Unix
(`crates/rustty-os/Cargo.toml:67-71`), but locale, mach, passwd, path, and PTY are
gated only against WASM and are therefore compiled on Windows
(`crates/rustty-os/src/lib.rs:76-101`). The page allocator explicitly says the
Windows VM path is unported and emits two unconditional target compile errors
(`crates/rustty-os/src/page_allocator.rs:350-366`). The Windows passwd function
also emits a target compile error (`crates/rustty-os/src/passwd.rs:182-188`),
while live locale and path code names the unavailable Unix-only `libc` dependency
(`crates/rustty-os/src/locale.rs:417-453`,
`crates/rustty-os/src/path.rs:43-53`). A further independent Windows error is
that `WindowsPtyError` is boxed as `dyn Error` without implementing that trait
(`crates/rustty-os/src/pty.rs:525-533`, `crates/rustty-os/src/pty.rs:638-649`).

**Impact.** The parity VT C library cannot be built for Windows, so Windows
embedders cannot consume the artifact Ghostty builds and tests. `cargo tree -p
rustty-vt-c --target x86_64-pc-windows-msvc -i rustty-os` observed both direct
and `rustty-font`-mediated paths from `rustty-vt-c` to the failing crate.

**Verification.** With the installed MSVC standard library,

```text
CARGO_TARGET_DIR=.port/codex/evidence/platform-targets/target-windows-os \
  cargo check -p rustty-os --target x86_64-pc-windows-msvc --offline
```

exited 101 with 25 Rust errors: the two page-allocator `compile_error!` calls,
the passwd `compile_error!`, unresolved `libc` uses in locale/mach/path, and the
missing `WindowsPtyError: Error` bound. The broader `rustty-vt-c` and headless
`rustty` checks failed on the same Rust source blockers.

**Counter-check.** `cargo check -p rustty-xev --target
x86_64-pc-windows-msvc --offline` passed, showing that the installed target and
Rustty's IOCP-capable event-loop dependency are not blanket blockers. Checking
`rustty-os` in isolation removes the unrelated missing-Windows-SDK-header errors
seen when native C dependencies of the whole VT library are built.

**Scope and limitation.** This finding covers Ghostty's demonstrated Windows
lib-vt/headless contract, not a Windows GUI application. A native Windows SDK is
not installed on the audit host, so additional C dependency issues after these
Rust blockers are fixed remain unmeasured.

### PT-002 — FreeBSD defaults to a GTK runtime that Rustty does not compile

**Ghostty contract.** Ghostty selects GTK by default on both Linux and FreeBSD
(`ghostty/src/apprt/runtime.zig:14-23`), resolves an executable GTK runtime to the
actual GTK module (`ghostty/src/apprt.zig:38-52`), then creates the app, initializes
the runtime, and runs its event loop (`ghostty/src/main_ghostty.zig:77-114`). Its
shared dependency setup includes FreeBSD in the PTY C-import targets
(`ghostty/src/build/SharedDeps.zig:209-220`).

**Rustty divergence.** Rustty copies the Linux/FreeBSD GTK default
(`crates/rustty-build-config/src/apprt.rs:25-35`), but the GTK module itself is
compiled only for `target_os = "linux"` (`crates/rustty-apprt/src/lib.rs:34-47`),
all GTK dependencies are Linux-only
(`crates/rustty-apprt/Cargo.toml:288-343`), and the executable's GTK launch arm is
also Linux-only (`crates/rustty/src/main_rustty.rs:277-292`). FreeBSD is not the
headless `None` runtime handled at `crates/rustty/src/main_rustty.rs:229-258`, so
it falls through to the unconditional `unimplemented!`
(`crates/rustty/src/main_rustty.rs:294-304`).

**Impact.** A default FreeBSD executable has no compiled backend for the runtime
it selected and panics during startup instead of launching the GTK application.

**Verification.** A complete compile-time-to-runtime source trace found the
FreeBSD selector, the Linux-only GTK module/dependencies/dispatch, and the sole
fallback panic. `cargo test -p rustty-build-config --offline` passed its one
host test but provided no FreeBSD-specific assertion.

**Counter-check.** A repository-wide search for FreeBSD handling in Rustty's
application runtime found the default selector but no FreeBSD GTK module or
launch arm. Conversely, Ghostty's executable selector and event-loop path are
not Linux-gated. This is not merely a missing Cargo feature: Rustty's default
`gtk` feature is deliberately a no-op off Linux
(`crates/rustty/Cargo.toml:21-36`).

**Scope and limitation.** No FreeBSD Rust target or FreeBSD runtime was installed,
so the final binary was not cross-compiled or executed. The reported failure is
the deterministic source path if compilation reaches `main`; earlier target
compile failures, if any, would make target viability worse rather than falsify
the missing backend.

### PT-003 — Browser embedders cannot populate the VT callback table

**Ghostty contract.** Ghostty's WASM lib-vt build deliberately exports the
indirect function table so JavaScript can insert callbacks for terminal effects
(`ghostty/src/build/GhosttyLibVt.zig:51-56`). It then makes the bounded table
growable and installs the patched artifact as `ghostty-vt.wasm`
(`ghostty/src/build/GhosttyLibVt.zig:61-85`).

**Rustty divergence.** Rustty's advertised VT WASM build is only a direct Cargo
release build (`.port/tools/build-vt-wasm.sh:1-15`): there is no linker setting
that exports the indirect table and no invocation of the patch step. The ported
patcher exists only as a manually invoked xtask subcommand
(`xtask/src/main.rs:78-81`, `xtask/src/main.rs:503-528`) and, by design, changes
only table limits (`xtask/src/wasm_patch_growable_table.rs:1-11`,
`xtask/src/wasm_patch_growable_table.rs:35-49`). This matters because the public
`rustty_sys_set` ABI accepts raw function pointers
(`crates/rustty-vt-c/src/lib.rs:382-400`), including decode-PNG and log callback
pointers (`crates/rustty-vt-c/src/sys.rs:223-229`,
`crates/rustty-vt-c/src/sys.rs:257-278`).

**Impact.** JavaScript cannot insert a callback into the module's private table
and pass its index to `rustty_sys_set`. Consequently browser users cannot install
the parity log callback or PNG decoder callback; the latter removes the callback
contract used to decode terminal graphics.

**Verification.** A release `rustty-vt-c` build for
`wasm32-unknown-unknown` passed. The reproducible analyzer
`.port/codex/scripts/inspect-wasm-table.mjs` observed this emitted structure:

```json
{
  "tableExports": [],
  "imports": [],
  "tables": [{ "flags": 1, "min": 212, "max": 212 }]
}
```

Thus the callback table is neither imported nor exported and cannot grow. Running
Rustty's own xtask patcher produced
`.port/codex/evidence/platform-targets/rustty_vt_c.growable.wasm`; the same analyzer
then observed `flags: 0, min: 212, max: null` but still `tableExports: []`.

**Counter-check.** The patcher works for the one property it claims: the output
table is growable. That does not repair access from JavaScript. Searches found no
`export-table` linker configuration and no alternate exported callback-registration
bridge. The module's empty import list also rules out host callbacks supplied as
WASM imports.

**Scope and limitation.** Other browser WASM surfaces remain viable: both
`cargo check -p rustty-wasm --target wasm32-unknown-unknown --offline` and
`cargo check -p rustty-web-canvas --target wasm32-unknown-unknown --offline`
passed. The finding is the VT C ABI callback contract, not blanket WASM failure.

### PT-004 — Default Linux build compiles only the no-op winproto backend

**Ghostty contract.** Ghostty asks GTK which display targets it was compiled with
using pkg-config and detects both X11 and Wayland
(`ghostty/src/build/gtk.zig:3-26`). Unless explicitly overridden, those detected
values become the build configuration (`ghostty/src/build/Config.zig:122-125`,
`ghostty/src/build/Config.zig:208-218`) and wire the X11/Wayland dependencies
(`ghostty/src/build/SharedDeps.zig:768-785`).

**Rustty divergence.** Rustty's default features contain only `gtk`; `wayland`
and `x11` are separately opt-in and explicitly off by default
(`crates/rustty/Cargo.toml:21-44`). The application crate passes those features
through separately (`crates/rustty-apprt/Cargo.toml:64-72`). Without them, only
the always-present no-op backend is compiled
(`crates/rustty-apprt/src/gtk/winproto.rs:1-32`), and backend initialization falls
through to it (`crates/rustty-apprt/src/gtk/winproto.rs:91-145`). The no-op backend
reports no quick-terminal support and performs no protocol-specific resize,
appearance, urgency, or child-environment work
(`crates/rustty-apprt/src/gtk/winproto/noop.rs:63-83`,
`crates/rustty-apprt/src/gtk/winproto/noop.rs:137-162`).

**Impact.** A normal `cargo build -p rustty` on Linux gets a GTK window but loses
the native X11/Wayland integrations Ghostty auto-enables when supported, including
quick-terminal protocol behavior, urgency/appearance synchronization, and
protocol-specific subprocess environment setup.

**Verification.** `cargo tree -p rustty --target x86_64-unknown-linux-gnu -e
features --offline` showed `rustty-apprt feature "gtk"` but no Rustty apprt
`wayland` or `x11` feature and no corresponding protocol crates. The source-level
backend dispatch then deterministically selects `App::None`.

**Counter-check.** Explicit `--features wayland,x11` enables the real Rustty
implementations, so this is a default build/distribution mismatch rather than
absent backend source. Ghostty also permits explicit overrides; the difference is
that its default tracks GTK capabilities while Rustty's does not.

**Scope and limitation.** The Linux target was not installed on this macOS host,
so the GUI was not executed. Basic GTK windowing may still work via GDK; this
finding is limited to Rustty's own protocol integration layer.

### PT-005 — Linux desktop resources are orphaned from build/install paths

**Ghostty contract.** Ghostty's normal resource build calls the Linux app-resource
installer for Linux (`ghostty/src/build/GhosttyResources.zig:246-253`). That path
processes and installs desktop, DBus, systemd, and AppStream templates
(`ghostty/src/build/GhosttyResources.zig:256-369`), installs Dolphin/Nautilus
actions and icons (`ghostty/src/build/GhosttyResources.zig:371-405`), and its CI
builds the Flatpak configuration (`ghostty/.github/workflows/test.yml:505-537`).

**Rustty divergence.** Rustty has corresponding templates and actions under
`dist/linux/`, but xtask exposes no Linux install/package command
(`xtask/src/main.rs:57-82`). Its `resources` path is documented for the macOS
Xcode copy phase and generates only terminfo, shell/editor, man/locale/bat,
themes, and shell-integration trees (`xtask/src/main.rs:469-500`). The root
Makefile offers only `init` and `clean` (`Makefile:1-13`). A repository-wide
search outside `dist/linux` and translations found no consumers for the desktop,
DBus, systemd, AppStream, Dolphin, or Nautilus installation names.

**Impact.** The repo-provided Linux build/install workflow yields a manually
launchable binary but does not install a desktop launcher, DBus/systemd activation,
AppStream metadata, icons, or file-manager actions, and it has no corresponding
Flatpak build lane.

**Verification.** File enumeration found seven Rustty Linux resource templates,
while the orchestration search found only the Nautilus translation updater and a
design document—not an installer. Direct inspection of all xtask subcommands and
the complete `resources()` body confirmed there is no hidden Linux install step.

**Counter-check.** The resource source files themselves are present and an
external distribution can process and package them manually. Rustty's existing
resource generator also does produce genuine runtime resources. The verified gap
is repository-supplied installation/distribution wiring, not missing template
content.

**Scope and limitation.** No external distro packaging repository was audited;
such packaging would not restore parity to Rustty's own build contract but could
mitigate end-user installations from that distributor.

## Positive coverage and controls

- `cargo check -p rustty-xev --target x86_64-pc-windows-msvc --offline`: passed.
- `cargo check -p rustty-xev --target wasm32-wasip1 --offline`: passed.
- `cargo check -p rustty-wasm --target wasm32-unknown-unknown --offline`: passed.
- `cargo check -p rustty-web-canvas --target wasm32-unknown-unknown --offline`: passed.
- Release `rustty-vt-c` for `wasm32-unknown-unknown`: passed and had zero imports.
- `cargo test -p rustty-build-config --offline`: passed (one host-target test).
- Rustty CI currently runs only Ubuntu and macOS
  (`.github/workflows/ci.yml:34-43`); Ghostty's Windows and multi-target jobs above
  explain why the Windows contract is exercised there and not in Rustty CI.

These controls prevented promotion of stale comments about `rustty-xev` or broad
"WASM is broken" claims into findings.

## Unresolved leads and environmental limits

1. **Additional Windows native dependency failures.** Whole-library Windows
   checking also encountered absent Windows SDK C/C++ headers on this macOS host.
   PT-001 is independent of that limitation, but the state after its Rust source
   blockers are removed is unresolved.
2. **FreeBSD compile-stage breadth.** A FreeBSD target/toolchain was unavailable.
   PT-002's selector-to-panic path is verified statically, but earlier compilation
   blockers elsewhere in the workspace were not enumerated.
3. **End-to-end Linux desktop behavior.** The Linux feature and packaging gaps
   are verified from build graphs and orchestration, but GTK/X11/Wayland runtime
   execution was unavailable on the macOS host.
4. **Browser packaging beyond emitted modules.** The headless and web-canvas crates
   compile and the VT module shape was inspected, but no production JS package or
   browser release pipeline was identified to exercise as an end-user artifact.
   The callback-table defect is artifact-level and does not depend on this lead.

