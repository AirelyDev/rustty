# Linux / FreeBSD GTK application-runtime parity audit

## Outcome

Seven verified gaps were found: two P1, four P2, and one P4. The highest-risk
result is structural: FreeBSD selects GTK as its default runtime but Rustty
compiles both the backend and its launch function only for Linux, so the
ordinary FreeBSD executable reaches an unconditional `unimplemented!` panic.
On Linux, the GTK runtime is substantially present, but two application-level
quit branches, original-locale propagation, native Wayland/X11 selection, and
desktop-resource installation still diverge from the reference.

| ID | Severity | Confidence | Verified consequence |
| --- | --- | --- | --- |
| GTK-BSD-001 | P1 | High | Default FreeBSD executable has no compiled GUI backend and panics at launch |
| GTK-BSD-002 | P1 | High | Application Quit cannot be confirmed while a live process requires confirmation |
| GTK-BSD-003 | P2 | High | Non-confirming application Quit exits the loop without destroying Rustty windows |
| GTK-BSD-004 | P2 | High | GTK child processes lose launch-time `LANG` whenever it was set |
| GTK-BSD-005 | P2 | High | Default Linux build always selects the no-op window-protocol backend |
| GTK-BSD-006 | P2 | Medium-high | Standard Rustty build/install never generates or installs Linux integration resources |
| GTK-BSD-007 | P4 | High | Debug/release-safe GTK windows instantiate a blank warning container |

Reproducer and raw observed results:

- `.port/codex/scripts/gtk-bsd/check_runtime_gaps.sh`
- `.port/codex/evidence/gtk-bsd/verification.md`

## Audit universe and limitations

- Rustty: `5243303fbb2f2cfd0832edfb8fa49d72ce322d81`
- Ghostty: `6ad1fe7d8cbda36c77b337a96c9bea8a77883699`
- Toolchain: Rust 1.93.1, host `aarch64-apple-darwin`
- No Linux or FreeBSD Rust target was installed. A Linux GUI session and GTK
  native libraries were also unavailable. Target-resolved dependency graphs
  and exact `cfg` reachability were therefore used for Linux/FreeBSD claims;
  interactive GTK behavior was not exercised.
- Fresh host checks passed: `cargo test -p rustty-build-config` (1/1) and
  `cargo check -p rustty --no-default-features`. These prove the inspected host
  seams remain coherent, not that the uninstalled targets compile.
- Both product repositories remained read-only. Only `.port/codex/` was written.

## Verified findings

### GTK-BSD-001 — FreeBSD defaults to an unavailable GTK runtime and panics

**Severity:** P1  
**Scope:** FreeBSD executable startup  
**Confidence:** High

**Ghostty contract.** Ghostty maps Linux and FreeBSD to GTK by default
(`/Users/gael/projects/ghostty/src/apprt/runtime.zig:14-23`), maps an executable
with that runtime to the concrete GTK apprt (`src/apprt.zig:38-49`), and its
main function initializes and runs that apprt (`src/main_ghostty.zig:99-114`).

**Rustty behavior.** Rustty preserves the same Linux-or-FreeBSD default
(`/Users/gael/projects/rustty/crates/rustty-build-config/src/apprt.rs:25-35`),
but exposes the GTK module only under `target_os = "linux"`
(`crates/rustty-apprt/src/lib.rs:34-47`). Its gtk4/libadwaita dependencies are
likewise in a Linux-only target table (`crates/rustty-apprt/Cargo.toml:288-309`).
The only GTK dispatch and `run_gtk` function are Linux-only
(`crates/rustty/src/main_rustty.rs:277-304,332-403`). On FreeBSD the runtime is
therefore `Gtk`, the guarded branch is absent, and execution falls directly to
the unconditional `unimplemented!` at lines 299-304.

**Impact.** The intended default FreeBSD product path cannot launch. This is
not a missing optional integration: it is the runtime chosen by the build
configuration itself.

**Observed verification.** The static reproducer confirmed all four predicates
(FreeBSD selects GTK; module and launcher are Linux-only; the fallback panics).
`cargo tree --target x86_64-unknown-freebsd` contained no GTK, libadwaita,
Wayland, or X11 native dependency match. A target compile exited 101 at the
missing `std` crate because `x86_64-unknown-freebsd` is not installed.

**Adversarial counter-check.** The apparent escape hatch—treating FreeBSD as a
deliberately headless build—does not survive inspection: Rustty explicitly
selects and describes GTK as the full-featured default on FreeBSD. No alternate
FreeBSD GUI launcher or target-specific GTK module exists. The default `gtk`
Cargo feature being enabled does not help: an inverse feature tree confirmed
that `rustty-apprt/gtk` is logically active on FreeBSD, while the dependency
table, module, and launcher all remain OS-gated to Linux.

### GTK-BSD-002 — Application Quit confirmation is a reachable dead end

**Severity:** P1  
**Scope:** Linux GTK application actions and apprt `Quit`  
**Confidence:** High

**Ghostty contract.** When `needsConfirmQuit()` is true, Ghostty constructs a
`CloseConfirmationDialog` targeting the application, connects its
`close-request` response to the final quit handler, and presents it
(`/Users/gael/projects/ghostty/src/apprt/gtk/class/application.zig:601-644`).

**Rustty behavior.** Rustty registers a live `app.quit` action
(`/Users/gael/projects/rustty/crates/rustty-apprt/src/gtk/class/application.rs:770-803`),
whose callback invokes `Application::quit` (lines 1807-1814). When the bound
core probe says confirmation is needed, `quit` only finds an unused `_parent`,
prints a warning claiming the dialog will land in a later task, and leaves the
run loop active (lines 2832-2869).

**Impact.** A user invoking application Quit while any running terminal needs
confirmation receives no dialog and no action-local way to approve the quit;
the requested application-wide operation does not complete.

**Observed verification.** The static reproducer matched the registered warning-
only branch. The source trace proves reachability from both the standard
application action and the apprt action dispatch (`Application::quit` is the
common endpoint).

**Adversarial counter-check.** The warning is stale, not a truthful deferral.
Rustty already has a complete dialog class with `Target::App`, a
`close-request` signal, constructor, and presenter
(`crates/rustty-apprt/src/gtk/class/close_confirmation_dialog.rs:24-43,150-188`).
The same class is live for window, tab, and surface close requests
(`class/window.rs:1749-1809`; `class/split_tree.rs:673-691`). No alternate
application-level handler presents it.

### GTK-BSD-003 — `quit_now` skips GTK window destruction

**Severity:** P2  
**Scope:** Linux GTK application shutdown without confirmation  
**Confidence:** High

**Ghostty contract.** Ghostty enumerates GTK toplevels, filters for its own
window class, destroys those windows, and only then clears the run-loop flag.
The comments identify this as the proper cascading shutdown sequence while
avoiding foreign fcitx windows
(`/Users/gael/projects/ghostty/src/apprt/gtk/class/application.zig:646-672`).

**Rustty behavior.** Rustty performs the same toplevel enumeration but reduces
every loop body to `let _ = window;`, then clears `running`
(`/Users/gael/projects/rustty/crates/rustty-apprt/src/gtk/class/application.rs:2871-2895`).
The no-confirmation branch calls this method directly (lines 2844-2853).

**Impact.** Ordinary application Quit bypasses the reference's ordered GTK
window/surface destruction and relies on later application/core teardown and
process exit. Window-owned shutdown/finalization work is not allowed to cascade
before the event loop stops.

**Observed verification.** The reproducer matched the toplevel loop's no-op
body. The executable's post-loop path only calls apprt termination and core-app
destruction (`crates/rustty/src/main_rustty.rs:380-400`); it does not supply the
missing per-window destroy pass.

**Adversarial counter-check.** The source comment says no windows exist until a
future task, but that premise is obsolete: the current GTK window class and
surface creation paths are populated. `deinit_private` clears app-owned fields
and resources (`class/application.rs:1850-1912`) but contains no toplevel
destroy. The process will exit, so this finding does not assert that windows
remain visibly open forever or that data is lost; it asserts the verified loss
of the orderly shutdown contract.

### GTK-BSD-004 — Launch-time `LANG` is captured, stored, then discarded

**Severity:** P2  
**Scope:** Linux GTK child shell/command environment  
**Confidence:** High

**Ghostty contract.** Ghostty captures the incoming `LANG` before applying its
configured UI-language override
(`/Users/gael/projects/ghostty/src/apprt/gtk/class/application.zig:279-290`),
returns the captured value through `savedLanguage` (lines 475-478), and restores
that value into every terminal subprocess environment
(`src/apprt/gtk/class/surface.zig:1593-1606`). This deliberately prevents a UI
translation override from silently changing shell locale.

**Rustty behavior.** Rustty also captures the old environment value before the
override (`/Users/gael/projects/rustty/crates/rustty-apprt/src/gtk/class/application.rs:2293-2303`)
and stores it in private state (lines 2392-2402). However,
`Application::saved_language()` is hardcoded to `None` (lines 2197-2208).
`Surface::default_termio_env` consumes that accessor and removes `LANG` on
`None` (`class/surface.rs:2337-2361`). Core surface initialization consumes this
environment for the terminal execution configuration
(`crates/rustty-core/src/surface.rs:2143-2156`).

**Impact.** Whenever Rustty was launched with `LANG` set, GTK-created shells and
commands receive no `LANG`. Locale-dependent program output, sorting, encoding,
and shell behavior may differ from the launch environment. The defect is
especially visible when the configured UI language differs from the user's
terminal locale, but it occurs even without an override.

**Observed verification.** The reproducer extracts the accessor body and
confirmed its exact `None` return. The paired trace confirmed the captured
`CString` is stored and the live terminal environment removes the key.

**Adversarial counter-check.** No second accessor or fallback reads the stored
field. Environment capture and the process-level override are already wired,
so this is not a harmless unimplemented capture tier; the stored value is
stranded. If Rustty truly starts without `LANG`, removal matches Ghostty's
non-Flatpak branch and there is no divergence for that control case.

### GTK-BSD-005 — Plain Linux builds disable both native window-protocol arms

**Severity:** P2  
**Scope:** Default Linux GTK build on Wayland and X11  
**Confidence:** High

**Ghostty contract.** Ghostty asks GTK's pkg-config metadata which backends it
was compiled with (`/Users/gael/projects/ghostty/src/build/gtk.zig:8-26`) and
uses those detected Wayland/X11 values as build defaults
(`src/build/Config.zig:208-218`). The runtime union then probes the enabled
native arms and falls back to `none` only when none succeeds
(`src/apprt/gtk/winproto.zig:23-48`). These arms provide protocol-specific
facilities such as quick-terminal initialization and native global-shortcut
binding (lines 66-99).

**Rustty behavior.** Rustty's default feature set is only `gtk`; `wayland` and
`x11` are independent opt-in features explicitly disabled by default
(`/Users/gael/projects/rustty/crates/rustty/Cargo.toml:21-44`). With neither,
the winproto module compiles only the no-op arm
(`crates/rustty-apprt/src/gtk/winproto.rs:1-29`), whose runtime probe falls back
to `None` (lines 113-145). That backend reports no quick-terminal support,
declines native shortcuts, and no-ops resize appearance, urgency, and
subprocess-protocol environment hooks
(`gtk/winproto/noop.rs:63-102,137-162`).

**Impact.** A normal `cargo build -p rustty` creates a GTK application but lacks
the session-native functionality Ghostty automatically includes when installed
GTK supports it. Wayland quick-terminal/layer-shell behavior and native
Wayland/X11 window integration are unavailable unless a packager knows to add
features not selected anywhere in the repository.

**Observed verification.** The default Linux target graph includes gtk4,
gdk4, and libadwaita, but the focused dependency query found no
`wayland-client`, `gdk4-x11`, or apprt `wayland`/`x11` feature. An exhaustive
production build-file search found no invocation enabling either feature.

**Adversarial counter-check.** Both native implementations do exist and can be
selected manually, so this is a default-product/configuration finding rather
than a claim that the code was never ported. The no-op shortcut binder falls
back to the XDG portal; therefore this report does not claim all global
shortcuts are broken. The verified gap is the absent automatic/default native
integration and the features that only those arms provide.

### GTK-BSD-006 — Linux integration templates have no build/install consumer

**Severity:** P2  
**Scope:** Linux packaging, desktop activation, DBus/systemd, AppStream  
**Confidence:** Medium-high

**Ghostty contract.** Ghostty's resource build processes the desktop, DBus,
systemd, and AppStream templates, substitutes product values, and installs the
outputs to their XDG locations
(`/Users/gael/projects/ghostty/src/build/GhosttyResources.zig:287-369`). It also
installs file-manager actions and icons (lines 371 onward). The executable
install path explicitly depends on `resources.install()`
(`/Users/gael/projects/ghostty/build.zig:177-183`).

**Rustty behavior.** Rustty has corresponding source templates, but they still
contain `@NAME@`, `@APPID@`, and `@RUSTTY@` placeholders
(`/Users/gael/projects/rustty/dist/linux/app.desktop.in:1-26`,
`dist/linux/dbus.service.in:1-4`, `dist/linux/systemd.service.in:1-14`). The
executable crate declares only a binary target
(`crates/rustty/Cargo.toml:1-19`) and has no build script. The project guide
calls these merely "packaging source assets" and says the automated
Flatpak/Snap/release workflows are forward references that land later
(`/Users/gael/projects/rustty/HACKING.md:82-104`).

**Impact.** Rustty's standard Cargo build/install path cannot produce populated
desktop, DBus activation, systemd user-service, or AppStream artifacts. Unless
an external packager independently implements the missing substitutions and
install layout, those Linux desktop integrations are absent.

**Observed verification.** An exhaustive search across Rust, TOML, shell,
workflow, and Nix production files for the template filenames or their install
destinations returned no matches. `rg --files crates/rustty` confirmed the
binary crate contains no `build.rs`; the only repository code mentioning a
Linux distribution asset is the translation task for the Nautilus script, not
an installer.

**Adversarial counter-check.** The templates themselves are real and may be
consumed by a downstream distribution, so this finding does not claim they can
never be packaged. No checked-in Flatpak/Snap/Nix or release build fills that
role at the pinned revision, and the project guide explicitly identifies those
workflows as future work. Confidence is lower than source-local findings
because an out-of-repository packager can compensate.

### GTK-BSD-007 — The debug warning class never binds its existing UI

**Severity:** P4  
**Scope:** Linux GTK debug/release-safe diagnostics  
**Confidence:** High

**Ghostty contract.** Ghostty's `GhosttyDebugWarning` initializes its GTK
template and binds the version-appropriate resource during class initialization
(`/Users/gael/projects/ghostty/src/apprt/gtk/class/debug_warning.zig:9-46`).

**Rustty behavior.** Rustty explicitly describes its class as an "INHABITED
placeholder" and implements only a unit `DebugWarning` subclass with no
`class_init`, template binding, or instance template initialization
(`/Users/gael/projects/rustty/crates/rustty-apprt/src/gtk/class/debug_warning.rs:1-52`).
Yet the compiled resource contains the actual warning banner
(`assets/ui/1.3/debug-warning.blp:1-9`), the resource table includes it
(`src/gtk/gresource.rs:170-176`), and the main window template instantiates the
class whenever `template.debug` is true (`assets/ui/1.5/window.blp:150-155`).

**Impact.** Debug/release-safe GTK builds insert an empty `Adw.Bin` instead of
warning that the build has degraded performance.

**Observed verification.** The static reproducer matched the empty unit class;
the source comparison verified that Rustty lacks both template initialization
steps present in Ghostty.

**Adversarial counter-check.** The banner blueprint and resource registration
could have disproved a missing-resource claim, but they instead isolate the
defect: the data exists and is reachable by name, while the class that must
bind and initialize it is empty. No other code assigns a child to this bin.

## Unresolved leads (not promoted)

### `Surface::cgroup_path()` is a value-only placeholder

Rustty's GTK surface returns `None`
(`/Users/gael/projects/rustty/crates/rustty-apprt/src/gtk/class/surface.rs:2028-2035`),
where Ghostty forwards to its core surface cgroup path. No production caller of
the Rustty method—and no current consumer of the corresponding Ghostty GTK
accessor—was found. Without an execution path or demonstrated contract consumer,
this remains an unresolved/value-only lead rather than a finding.

## Adversarial negative checks

The following plausible-looking gaps were specifically challenged and not
promoted:

- Command-palette jump commands are live despite stale staged-port language:
  Rustty walks the core surface list, builds jump commands, and focuses the
  selected surface (`class/command_palette.rs:288-351`).
- GTK surface creation reaches a real core surface; comments implying a dormant
  seam were not treated as current behavior.
- Drag-and-drop target types and drop handlers, notification routing, and
  clipboard request routing all had concrete Rustty consumers matching the
  inspected Ghostty paths; no consequence-backed divergence was established.
- The no-op winproto's `bind_global_shortcuts = false` invokes a portal fallback,
  so it is not evidence that every default-build shortcut is missing.
- Unknown inspector properties falling through to a default/empty value were
  consistent with generated GObject property plumbing, not a product stub.

## Coverage statement

Reviewed paired runtime selection and executable entrypoints; Cargo target and
feature gates; GTK application lifecycle and actions; window/surface creation;
close confirmation; child environment construction; Wayland/X11/no-op protocol
dispatch; quick-terminal/global-shortcut consequences; DnD, clipboard, and
notification paths; debug resources; Linux desktop/DBus/systemd/AppStream
templates; and FreeBSD-specific reachability. The principal blind spots are
interactive GUI execution and an actual FreeBSD compile, both blocked by the
host environment described above.
