# Canonical non-macOS parity findings

Audit date: 2026-07-31  
Rustty: `5243303fbb2f2cfd0832edfb8fa49d72ce322d81`  
Ghostty: `6ad1fe7d8cbda36c77b337a96c9bea8a77883699`

This is the deduplicated, cross-reviewed finding catalog. Workstream reports in
`agents/` retain the original investigations; when their initial severity or
wording differs, this file is authoritative. `P1` means a target/build blocker
or complete failure of a major advertised feature, `P2` a material functional
failure, `P3` a narrow/edge or verification-contract failure, and `P4` a minor
diagnostic or distribution-quality gap.

## Executive result

Twenty verified gaps remain after adversarial falsification: **5 P1, 9 P2,
3 P3, and 3 P4**.

| ID | Sev. | Scope | Verified gap |
| --- | --- | --- | --- |
| F-001 | P1 | Windows | Shared OS support does not compile; the VT C library and written ConPTY path are cut off. |
| F-002 | P1 | FreeBSD | The default runtime selects GTK, but GTK is Linux-only and startup reaches an unimplemented arm. |
| F-003 | P1 | FreeBSD, later Windows | OpenGL is selected, but the renderer implementation/export/dependencies are Linux-only. |
| F-004 | P1 | FreeBSD | xev selects kqueue, but the kqueue API is macOS-only; real consumers cannot type-check. |
| F-005 | P1 | Linux | Kitty graphics are parsed and stored but never rendered. |
| F-006 | P2 | Browser/WASM | The VT callback table is private; browser embedders cannot install any effect callback. |
| F-007 | P2 | Default Linux build | Neither native Wayland nor X11 integration is enabled by default. |
| F-008 | P2 | C ABI | `rustty_terminal_new` ignores the caller-supplied allocator. |
| F-009 | P2 | Linux GTK | Application-wide Quit cannot complete when confirmation is required. |
| F-010 | P2 | Linux GTK | Non-confirming application Quit exits the loop without destroying windows. |
| F-011 | P2 | Linux GTK | Captured launch-time `LANG` is discarded from child environments. |
| F-012 | P2 | Linux | `background-image` configuration is accepted but not rendered. |
| F-013 | P2 | Linux | Custom shaders are loaded/compiled but never applied to frames. |
| F-014 | P2 | Linux GTK | Renderer scrollbar updates are discarded by a no-op mailbox. |
| F-015 | P3 | VT core, principally Linux | Two recoverable PageList allocation failures become panic/abort paths. |
| F-016 | P3 | Test infrastructure | The hard VT differential gate is backed by stale binaries and source that no longer builds. |
| F-017 | P3 | Test infrastructure | No Rust coverage-guided or live differential fuzz target exists. |
| F-018 | P4 | Linux distribution | Linux integration templates have no repository build/install consumer. |
| F-019 | P4 | Linux GTK | The debug/release-safe performance warning widget is blank. |
| F-020 | P4 | Linux inspector | Live renderer overlay controls have no OpenGL consumer. |

## P1 findings

### F-001 — Windows shared OS support does not compile

Ghostty builds and tests lib-vt on native Windows and provides Windows page
allocation and non-POSIX home-directory paths. Rustty's `rustty-os` instead
compiles several POSIX modules on Windows even though `libc` is Unix-only,
contains explicit unported Windows page allocator and passwd `compile_error!`
sites, and boxes `WindowsPtyError` as `dyn Error` without implementing that
trait. The failure reaches both the VT C library and the already-written ConPTY
runtime through real dependency edges.

Observed verification: with the installed MSVC Rust target,
`cargo check -p rustty-os --target x86_64-pc-windows-msvc --offline` exited 101
with 25 Rust errors. `rustty-xev` passed for that same target, which falsifies a
blanket target/toolchain explanation. Native SDK-dependent C failures were not
counted because this host cannot distinguish them from sysroot setup. This
finding merges duplicate PT-001/SR-001 reports. Full trace:
[`agents/platform-targets.md`](agents/platform-targets.md#pt-001--windows-vt-c-library-is-stopped-by-rust-source-blockers)
and [`agents/shared-runtime.md`](agents/shared-runtime.md#sr-001--windows-shared-runtime-cannot-compile).

### F-002 — FreeBSD selects a GTK runtime that is not compiled

Both projects default FreeBSD to GTK. Rustty compiles its GTK module,
dependencies, and executable launch arm only for Linux. FreeBSD is not routed
to the supported headless arm; if compilation reaches runtime dispatch it falls
through to `unimplemented!` and panics rather than launching a GUI. The `gtk`
feature is deliberately a no-op off Linux, so this is not repaired by the
default feature set.

This was verified from the target selector through module/dependency cfgs to the
sole fallback arm, and challenged with repository-wide searches for another
FreeBSD runtime. No FreeBSD standard library was installed, so no native/cross
execution is claimed. This is one of three independent FreeBSD blockers, not a
duplicate of the renderer or event-loop failures below. Full trace:
[`agents/gtk-bsd.md`](agents/gtk-bsd.md#gtk-bsd-001--freebsd-defaults-to-an-unavailable-gtk-runtime-and-panics).

### F-003 — OpenGL renderer is absent on FreeBSD (and later Windows)

Rustty's build configuration, like Ghostty, selects OpenGL for every non-WASM,
non-Darwin target. Its renderer crate nevertheless declares the OpenGL module,
public `Renderer` alias, implementation module, `gl`, and `khronos-egl`
dependencies only on Linux. Core names `rustty_renderer::Renderer` on every
non-macOS target. FreeBSD therefore cannot compile a surface renderer. The same
contradiction becomes a later-stage Windows blocker after F-001 is removed.

The static target-contract verifier confirmed `target_os="freebsd"`, the
non-Darwin OpenGL selector, and the lack of any FreeBSD-capable export. Searches
found no alternate renderer alias. The absence of a FreeBSD Rust std prevents a
full compiler transcript, so the claim is limited to the deterministic cfg/type
failure. Full trace:
[`agents/shared-runtime.md`](agents/shared-runtime.md#sr-002--opengl-renderer-is-excluded-from-freebsd).

### F-004 — FreeBSD xev selects kqueue but exports an unusable fallback

`rustty-xev` explicitly selects `Backend::Kqueue` for FreeBSD, but its kqueue
modules, loop types, watchers, public exports, and `backend()` accessor are all
macOS-only. FreeBSD receives a skeletal `no_backend_seam`. Real termio/core
consumers require absent `Options`, `RunMode`, callback/error types, and methods
such as `Loop::run`, `Process::wait`, and `Async::wait`; ordinary product code
therefore fails type checking before the fallback's `unimplemented!` bodies can
run.

The pinned libxev oracle contains actual FreeBSD `Kevent`, syscall, note, and
eventfd branches. `rustc --print cfg --target x86_64-unknown-freebsd` and a
target-specific dependency tree confirmed the selected cfg and live consumer
edges. Cross-review specifically rejected the weaker “runtime panic only”
description. Full trace:
[`agents/platform-second-cross-review.md`](agents/platform-second-cross-review.md#scr-001--p1-freebsd-selects-kqueue-but-exports-an-unusable-fallback-seam).

### F-005 — Linux never renders Kitty graphics

Rustty enables a Kitty image store, parses APC graphics commands, and mutates
the active screen's image storage. The live Linux OpenGL renderer intentionally
has no image state, omits Kitty dirty/update and texture upload work, and omits
all three ordered placement layers (below backgrounds, below text, above text).
Applications can therefore send accepted Kitty graphics that consume terminal
state and receive protocol handling while nothing is displayed.

Ghostty's generic renderer performs the corresponding update, upload, and three
draw groups. Focused Rustty Kitty parser tests passed, proving the producer is
live and narrowing the defect to Linux presentation. Cross-review raised the
initial P2 to P1 because this is complete failure of a major terminal protocol
feature, not a minor visual mismatch. Full trace:
[`agents/platform-second-cross-review.md`](agents/platform-second-cross-review.md#scr-002--p1-linux-never-renders-kitty-graphics).

## P2 findings

### F-006 — Browser VT effect callbacks cannot be installed

The public VT ABI accepts raw function pointers for terminal-effect callbacks,
including logging and PNG decoding. Ghostty deliberately exports its WASM
indirect function table and makes it growable so JavaScript can insert those
callbacks. Rustty's normal WASM build emits a private fixed-size table. Its
manual patcher removes the maximum but still does not export the table; the
module has no callback imports and no alternate registration bridge.

Artifact inspection observed zero table exports/imports and `{min: 212,
max: 212}` before patching, then zero exports and an unbounded table after
patching. Thus all callback-based effects—not only PNG—are inaccessible to a
browser embedder. Other WASM crates passed target checks, narrowing the finding
to this ABI bridge. Full trace:
[`agents/gtk-cross-review.md`](agents/gtk-cross-review.md#pt-003--uphold-impact-is-broader-than-stated).

### F-007 — Default Linux build uses only the no-op winproto backend

Ghostty detects the Wayland/X11 capabilities of the GTK installation and
enables them unless overridden. Rustty's default feature set enables `gtk` but
leaves both `wayland` and `x11` opt-in, selecting the always-present no-op
backend in a normal build. This loses Rustty's native quick-terminal, urgency,
appearance, resize, and protocol-specific child-environment integration; basic
GDK windowing and the portal shortcut fallback may still work.

The Linux target feature tree contained neither backend or its protocol crates.
An explicit `--features wayland,x11` does compile the real source, so the gap is
default build behavior, not missing implementations. Full trace:
[`agents/platform-targets.md`](agents/platform-targets.md#pt-004--default-linux-build-compiles-only-the-no-op-winproto-backend).

### F-008 — the terminal constructor ignores its allocator

The current C header accepts a caller allocator and documents graceful OOM.
Ghostty uses it for terminal construction and returns OOM when it rejects an
allocation. Rustty resolves the supplied allocator into `_alloc`, then uses
`allocator_api2::Global`, global `Box`, and a process-global stream allocator;
destruction likewise uses global `Box::from_raw`.

The same C program was compiled against fresh current libraries with an
allocator that rejects every request. Ghostty returned `-1`, a null terminal,
and made one allocator call. Rustty returned success, a non-null terminal, and
made zero calls. This rules out a mere allocation-order difference. It breaks
arena/quota/failure-injection and heap-domain contracts, but was calibrated P2
because normal global allocation still constructs a working terminal. Reproducer:
[`scripts/check-terminal-allocator.c`](scripts/check-terminal-allocator.c);
observations: [`evidence/terminal-allocator/observed.txt`](evidence/terminal-allocator/observed.txt).

### F-009 — application-wide Quit confirmation is a dead end

Both the `app.quit` action and the general apprt `Action::Quit` route to
`Application::quit`. When core reports that confirmation is needed, Rustty
finds a possible parent, prints a stale warning, presents no dialog, and leaves
the application running. Ghostty constructs and wires `CloseConfirmationDialog`
for `Target::App`; Rustty already has that dialog implementation, but this route
does not use it.

Cross-review found working window, tab, and surface confirmation paths, so the
application can be closed by alternatives and the initial P1 was reduced to P2.
The app-wide Quit action itself remains a verified reachable failure. Full trace:
[`agents/shared-cross-review.md`](agents/shared-cross-review.md#gtk-bsd-002--verified-dead-end-but-p2-is-better-calibrated-than-p1).

### F-010 — `quit_now` does not destroy GTK windows

Ghostty enumerates and destroys its toplevel windows before ending the loop.
Rustty enumerates them but discards each value, then only clears its running
flag. Later GTK/runtime teardown does not repeat the missing destruction
cascade; core teardown can encounter live surface resources and a font-grid
empty assertion before GObject finalization.

The route is live for application Quit when confirmation is not required.
Static GTK predicates passed and cross-review traced every later teardown layer;
without a Linux GUI session no crash was claimed or severity elevated. Full
trace: [`agents/shared-cross-review.md`](agents/shared-cross-review.md#gtk-bsd-003--verified-p2-later-teardown-does-not-mitigate-it).

### F-011 — GTK child processes lose launch-time `LANG`

GTK startup captures and stores the launch language, but the only accessor
returns `None`. The child-environment builder then removes `LANG` and has no
saved value to restore. Ghostty returns the stored value. Shell startup or an
explicit `env = LANG=...` can mask the behavior, but there is no automatic
fallback, so spawned commands do not inherit the promised launch locale.

Cross-review traced the exact environment transformation and rejected the theory
that another accessor or teardown path restores it. Full trace:
[`agents/shared-cross-review.md`](agents/shared-cross-review.md#gtk-bsd-004--verified-p2-the-stored-launch-lang-is-stranded).

### F-012 — Linux accepts but ignores `background-image`

The config parser and renderer `DerivedConfig` accept the image path plus fit,
opacity, position, and repeat. Linux never decodes or uploads the source, ignores
path changes, and always takes the solid-color background pass. Updating the
presentation metadata only changes a buffer for an image that cannot exist.

The focused derived-config test passed, confirming that the value reaches the
renderer rather than being rejected. Ghostty performs load/change/upload/draw,
and no alternate GTK layer consumes the option. Full trace:
[`agents/platform-second-cross-review.md`](agents/platform-second-cross-review.md#scr-003--p2-linux-accepts-but-ignores-background-image).

### F-013 — Linux compiles custom shaders but never applies them

Linux receives shader paths, derives `has_custom_shaders`, reads the files, and
creates shader pipelines. Its live frame path omits terminal/per-frame uniforms,
offscreen target selection, and every post-processing pass, so a successfully
compiled shader has no visual effect. The comment claiming shaders are always
false describes only a golden test fixture and is false for production config.

The focused config test passed and the source trace reaches actual pipeline
creation, disproving “unsupported and rejected.” Ghostty performs the missing
state, target, and post-pass work. Full trace:
[`agents/platform-second-cross-review.md`](agents/platform-second-cross-review.md#scr-004--p2-linux-loads-custom-shaders-but-never-applies-them).

### F-014 — renderer scrollbar updates never reach GTK

Linux's renderer keeps a unit `SurfaceMailbox` whose `push` always returns zero;
core receives a real mailbox but discards it before renderer construction. The
draw path repeatedly tries to publish a dirty `Scrollbar`, while the otherwise
implemented core and GTK consumers never receive it. Ghostty sends the same
message from its draw defer.

Renderer health also crosses this no-op boundary, but the corresponding Ghostty
GTK action is currently incomplete, so that was not inflated into a second
finding. Full trace:
[`agents/root-vt-renderer.md`](agents/root-vt-renderer.md#rv-006--linux-scrollbar-state-never-reaches-gtk).

## P3 findings

### F-015 — PageList erases two recoverable allocation failures

Ghostty's preserved-page inspection returns OOM while keeping compressed bytes
and statistics unchanged; search propagates it and inspector displays a disabled
placeholder. Rustty uses raw `alloc_zeroed` and asserts non-null, so live search
and inspector panic. Separately, Ghostty can reject the exact retained encoded
buffer during opportunistic compression, leave the first node resident, and
advance to the next serial. Rustty creates that buffer with infallible `to_vec()`
on the global allocator, so failure aborts rather than returning `false`.

The two exact Ghostty failure assertions are explicitly not ported. Existing
Rustty incompressible/decommit and success-path tests passed, but exercise other
branches. An audit-only isolated failing-global-allocator probe reproduced the
preserved-page panic with the node still compressed, and then a child `SIGABRT`
at the 4,626-byte encoded allocation. This also falsifies Rustty's test comment
that injection is impossible; the repo already uses this testing pattern.
Severity remains P3 because genuine allocation exhaustion is narrow. Full trace:
[`agents/shared-cross-review.md`](agents/shared-cross-review.md#pl-oom-001--pagelist-erased-two-recoverable-allocation-boundaries).

### F-016 — hard VT differential gate has stale provenance

The checked-in differential driver still calls the old three-argument terminal
constructor and cannot compile against either current project's four-argument
header. The build path selects the current Ghostty header, but the checked-in
executables predate the synchronized ABI change; they can still print equality
without testing today's code. An audit-only compatibility shim allowed fresh
current Rustty/Ghostty libraries to match byte-for-byte on all 12 curated VT
fixtures, so this is a reproducibility/oracle defect—not a demonstrated terminal
behavior mismatch.

The old binaries also report 21 C golden cases, but that output is not accepted
as current-oracle proof. Full trace:
[`agents/test-infrastructure-cross-review.md`](agents/test-infrastructure-cross-review.md#ti-001--current-harness-source-cannot-reproduce-the-passing-binaries).

### F-017 — no coverage-guided or live differential fuzz target

`cargo-fuzz` is installed, but Rustty has no `crates/rustty-vt/fuzz/` target.
The G5 gate honestly takes a `PROGRESS/FUZZ-WIRED` path and exits 0 instead of
running a campaign. The repo has valuable deterministic/randomized tests and
corpus tools, but none provide coverage-guided Rust fuzzing or live differential
execution against Ghostty. Current Ghostty carries parser, stream, and OSC fuzz
targets; its CI builds rather than runs a long campaign, which limits but does
not erase the parity gap. Full trace:
[`agents/test-infrastructure-cross-review.md`](agents/test-infrastructure-cross-review.md#ti-002--g5-has-no-rust-fuzz-target-or-live-differential-smoke).

## P4 findings

### F-018 — Linux integration resources have no build/install path

Rustty contains desktop, DBus, systemd, AppStream, Dolphin, Nautilus, and icon
templates under `dist/linux/`, but neither xtask, Makefile, nor CI processes and
installs them. Ghostty's normal Linux resource build does so and exercises a
Flatpak configuration. An external distro can package the Rustty files, so the
verified gap is repository-supplied build/install wiring rather than absent
template content or terminal functionality. Full trace:
[`agents/platform-targets.md`](agents/platform-targets.md#pt-005--linux-desktop-resources-are-orphaned-from-buildinstall-paths).

### F-019 — GTK performance-warning widget is blank

The warning UI resource exists, and main/inspector templates instantiate its
type when their `debug` property is true. The Rust subclass never binds or
initializes the template, leaving an empty `AdwBin`. This also affects the
`release-safe` profile because debug assertions are enabled there. It is a
missing diagnostic rather than terminal behavior. Full trace:
[`agents/shared-cross-review.md`](agents/shared-cross-review.md#gtk-bsd-007--verified-p4-the-warning-widget-is-a-blank-container).

### F-020 — Linux drops live inspector overlay controls

The inspector exposes hyperlink and semantic-prompt overlay checkboxes and
publishes selected features to shared renderer state. Linux omits feature
extraction, CPU overlay rebuild, image upload, and the above-text draw. Ghostty
performs all four steps. The normal terminal remains usable; this is a broken
optional diagnostic visualization. Full trace:
[`agents/platform-second-cross-review.md`](agents/platform-second-cross-review.md#scr-005--p4-linux-discards-live-inspector-overlay-controls).

## What did not become a finding

The whole-tree audits found no unexplained missing current file, public function,
or test after mapping documented exceptions. Fresh shimmed VT fixtures found no
ordinary-input mismatch. Frozen assertion/test-name gates were stale rather than
evidence of product loss. Numerous old `DEFERRED` markers now have bodies and
tests. A contaminated parallel corpus run had shared-temporary-file races and was
discarded. See [`REJECTED_AND_UNRESOLVED.md`](REJECTED_AND_UNRESOLVED.md) for the
full negative record and [`COVERAGE.md`](COVERAGE.md) for limitations.

