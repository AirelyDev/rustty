# Second cross-review: FreeBSD xev and Linux renderer omissions

Audit date: 2026-07-31

Inputs reviewed:

- Rustty `5243303fbb2f2cfd0832edfb8fa49d72ce322d81`
- Ghostty `6ad1fe7d8cbda36c77b337a96c9bea8a77883699`
- `.port/codex/agents/platform-targets.md`
- `.port/codex/agents/platform-cross-review.md`

This review independently traced the FreeBSD event-loop cfg surface and the
Linux GTK renderer from live producers to the final draw calls. The FreeBSD
failure is stronger than a latent `unimplemented!` panic: Rustty's real
termio/core consumers cannot type-check against the selected seam. The Linux
renderer omissions have different user-visible consequences and should not be
collapsed into one generic "overlay/update-frame" finding.

## Final verdict

| Candidate | Verdict after falsification | Final severity | Catalog treatment |
|---|---|---:|---|
| FreeBSD xev selector/export contradiction | Verified compile-time blocker | P1 | Keep as a distinct root. Group its target-level impact with PT-002 and SR-002 as a third independent FreeBSD blocker, not a third affected-target count. |
| Linux Kitty graphics omission | Verified major supported-feature failure | P1 | Keep separately. The terminal accepts/stores Kitty graphics, but Linux has no renderer image state and draws none of the three placement layers. |
| Linux configured background image omission | Verified configuration mismatch | P2 | Keep separately. The option reaches renderer `DerivedConfig`, but Linux never decodes, uploads, or draws it. |
| Linux custom-shader omission | Verified configuration mismatch | P2 | Keep separately. Linux loads/compiles configured shader files, but omits state updates, offscreen targeting, and post-process passes, so they do not affect output. |
| Linux inspector debug-overlay omission | Verified diagnostic parity defect | P4 | Keep separately or fold into an inspector diagnostics section. The live inspector can enable overlay features, but Linux discards them before rebuild/upload/draw. |

The three image-backed Linux defects share missing OpenGL image-state work and
may share remediation, but their product contracts and severities differ.
Custom shaders are a separate post-processing path.

## SCR-001 — P1: FreeBSD selects kqueue but exports an unusable fallback seam

### The contradiction

Rustty's backend selector explicitly maps macOS, iOS, and FreeBSD to kqueue in
both `Default` and `candidates()`
(`crates/rustty-xev/src/backend.rs:40-50`,
`crates/rustty-xev/src/backend.rs:92-104`). The actual kqueue implementation,
loop API, watchers, and public exports are nevertheless macOS-only
(`crates/rustty-xev/src/lib.rs:164-226`,
`crates/rustty-xev/src/lib.rs:290-342`). Even the public `backend()` accessor is
macOS-only (`crates/rustty-xev/src/lib.rs:89-92`).

FreeBSD therefore satisfies the broad negative cfg and receives
`no_backend_seam` (`crates/rustty-xev/src/lib.rs:611-625`). That seam's message
claims libxev declares no backend for the target
(`crates/rustty-xev/src/lib.rs:628-631`), directly contradicting both Rustty's
selector and the pinned libxev oracle. It exposes only skeletal `Loop`, `Async`,
`Completion`, `Timer`, `Process`, `Stream`, `WriteRequest`, and `WriteQueue`
types (`crates/rustty-xev/src/lib.rs:633-656`), with a few constructors and
destructors implemented as `unimplemented!` (`crates/rustty-xev/src/lib.rs:658-700`).

### Why this is a typing failure, not merely a runtime panic

The real consumers require public declarations and methods the fallback does
not define. Termio re-exports the target-selected xev API
(`crates/rustty-termio/src/xev_seam.rs:19`) and immediately constructs
`Loop::init(xev::Options::default())`
(`crates/rustty-termio/src/thread.rs:158-194`). It later calls
`Loop::run(xev::RunMode::UntilDone)`
(`crates/rustty-termio/src/thread.rs:418`,
`crates/rustty-termio/src/thread.rs:833`) and its callbacks name such absent
types as `CompletionState`, `TimerRunError`, `CallbackAction`, and `WaitError`
(`crates/rustty-termio/src/thread.rs:1027-1250`). Exec additionally needs
`Process::wait`, `Stream::init_fd`, and `Timer::run`
(`crates/rustty-termio/src/exec.rs:420-463`,
`crates/rustty-termio/src/exec.rs:610-648`). Core's renderer thread repeats the
same contract with `Options`, `Async::wait`, and `RunMode`
(`crates/rustty-core/src/renderer_seam.rs:552-590`,
`crates/rustty-core/src/renderer_seam.rs:715-715`,
`crates/rustty-core/src/renderer_seam.rs:1035-1081`).

Thus ordinary FreeBSD compilation fails name and method resolution before any
fallback body can execute. A narrow program could call one of the fallback's
matching zero-argument constructors and panic, but that is not Rustty's product
path. The primary impact is an unbuildable target, satisfying P1.

### Oracle and counter-checks

Pinned libxev selects kqueue for FreeBSD in both its default and candidate APIs
(`ghostty/zig-pkg/libxev-0.0.0-86vtcwIRFADbH4hk-EjROXxlrKIRPQdA41XiTSytYO-F/src/backend.zig:12-38`),
then forwards the complete selected API, including `Loop`, `Completion`,
`Options`, `RunMode`, callbacks, and watchers
(`ghostty/zig-pkg/libxev-0.0.0-86vtcwIRFADbH4hk-EjROXxlrKIRPQdA41XiTSytYO-F/src/main.zig:4-40`).
Its platform tests explicitly reference `Kqueue` on FreeBSD
(`ghostty/zig-pkg/libxev-0.0.0-86vtcwIRFADbH4hk-EjROXxlrKIRPQdA41XiTSytYO-F/src/main.zig:107-118`).
The kqueue backend reports itself available there, initializes and runs a real
kqueue loop, uses FreeBSD's `Kevent`/`kevent`, and supplies an eventfd wakeup
(`ghostty/zig-pkg/libxev-0.0.0-86vtcwIRFADbH4hk-EjROXxlrKIRPQdA41XiTSytYO-F/src/backend/kqueue.zig:27-42`,
`ghostty/zig-pkg/libxev-0.0.0-86vtcwIRFADbH4hk-EjROXxlrKIRPQdA41XiTSytYO-F/src/backend/kqueue.zig:51-127`,
`ghostty/zig-pkg/libxev-0.0.0-86vtcwIRFADbH4hk-EjROXxlrKIRPQdA41XiTSytYO-F/src/backend/kqueue.zig:300-308`,
`ghostty/zig-pkg/libxev-0.0.0-86vtcwIRFADbH4hk-EjROXxlrKIRPQdA41XiTSytYO-F/src/backend/kqueue.zig:1859-1878`,
`ghostty/zig-pkg/libxev-0.0.0-86vtcwIRFADbH4hk-EjROXxlrKIRPQdA41XiTSytYO-F/src/watcher/async.zig:55-66`).

Fresh `rustc --print cfg --target x86_64-unknown-freebsd` confirmed
`target_os="freebsd"`, which deterministically selects Rustty's fallback. The
FreeBSD standard library is not installed, so no final compiler diagnostic was
captured. Previously passing Windows and WASI xev checks, plus the correct
FreeBSD selector, rule out a blanket xev or target-detection failure.

This is not a duplicate of PT-002 (GTK application runtime) or SR-002 (missing
FreeBSD renderer export). Each has a different remediation boundary and each
independently prevents a usable FreeBSD product. Executive summaries should
present them together as multiple blockers on one target.

## SCR-002 — P1: Linux never renders Kitty graphics

### Reachable supported feature

Rustty enables a 320 MB Kitty image store by default
(`crates/rustty-config/src/config_struct.rs:3405-3413`,
`crates/rustty-config/src/config_struct.rs:5069`) and threads that value plus all
supported loading media into the real terminal
(`crates/rustty-termio/src/termio_impl.rs:568-582`). The APC parser recognizes
Kitty commands (`crates/rustty-vt/src/apc/mod.rs:125-140`,
`crates/rustty-vt/src/apc/mod.rs:314-332`), and execution mutates the active
screen's image storage when enabled
(`crates/rustty-vt/src/kitty/graphics_exec.rs:35-75`). This is not an unreachable
or disabled protocol surface.

The Linux GTK product uses the real OpenGL renderer export
(`crates/rustty-renderer/src/lib.rs:181-189`), core constructs its windowed
renderer (`crates/rustty-core/src/renderer_seam.rs:305-334`), and the render
thread calls its live `update_frame` and `draw_frame_raw` paths
(`crates/rustty-core/src/renderer_seam.rs:1238-1250`,
`crates/rustty-core/src/renderer_seam.rs:1486-1510`).

### Linux omission and impact

That renderer intentionally has no `images` field
(`crates/rustty-renderer/src/generic/linux_impl.rs:25-30`,
`crates/rustty-renderer/src/generic/linux_impl.rs:237-248`). Its frame update
omits `kitty_requires_update`/`kitty_update`
(`crates/rustty-renderer/src/generic/linux_impl.rs:3019-3041`,
`crates/rustty-renderer/src/generic/linux_impl.rs:3137-3156`), its draw path does
not upload image textures (`crates/rustty-renderer/src/generic/linux_impl.rs:3550-3554`),
and all three placement layers—below cell backgrounds, below text, and above
text—are absent (`crates/rustty-renderer/src/generic/linux_impl.rs:3692`,
`crates/rustty-renderer/src/generic/linux_impl.rs:3710`,
`crates/rustty-renderer/src/generic/linux_impl.rs:3735-3736`).

The protocol can therefore accept, respond to, and retain application image
data while displaying no image on Linux. This is a complete failure of a major
terminal protocol feature on the intended Linux GUI path, so P1 is appropriate,
not P2.

Ghostty's generic renderer performs the missing dirty-state update and upload
(`ghostty/src/renderer/generic.zig:1228-1245`,
`ghostty/src/renderer/generic.zig:1548-1555`) and draws the same three layers
(`ghostty/src/renderer/generic.zig:1629-1636`,
`ghostty/src/renderer/generic.zig:1646-1652`,
`ghostty/src/renderer/generic.zig:1673-1679`). A fresh Rustty Kitty parser test
filter passed two tests, confirming that the producer side remains live; it does
not mitigate the absent Linux consumer.

## SCR-003 — P2: Linux accepts but ignores `background-image`

Rustty exposes and parses `background-image` plus opacity, position, fit, and
repeat (`crates/rustty-config/src/config_struct.rs:1546-1626`,
`crates/rustty-config/src/config_struct.rs:8516-8526`,
`crates/rustty-config/src/config_struct.rs:9751-9779`). Renderer
`DerivedConfig::init` copies the path and all presentation options
(`crates/rustty-renderer/src/generic.rs:468-479`,
`crates/rustty-renderer/src/generic.rs:522-527`). A fresh targeted renderer test
run passed `derived_config_maps_bg_image_path`.

The live Linux constructor nevertheless omits `prep_background_image`
(`crates/rustty-renderer/src/generic/linux_impl.rs:809-937`), config changes
ignore path changes (`crates/rustty-renderer/src/generic/linux_impl.rs:1205-1210`,
`crates/rustty-renderer/src/generic/linux_impl.rs:1248-1254`), and drawing always
uses the background-color pass because no decoded image can exist
(`crates/rustty-renderer/src/generic/linux_impl.rs:3674-3690`). Updating and
synchronizing the fit/position buffer does not alter that result.

Ghostty decodes the selected path and handles path changes
(`ghostty/src/renderer/generic.zig:1738-1765`,
`ghostty/src/renderer/generic.zig:1875-1900`), uploads it, and selects the
background-image pipeline when ready
(`ghostty/src/renderer/generic.zig:1548-1555`,
`ghostty/src/renderer/generic.zig:1601-1627`). No alternate Rustty GTK layer
consumes this renderer config. The setting is therefore a silent visual no-op on
Linux: a material configuration mismatch, P2.

## SCR-004 — P2: Linux loads custom shaders but never applies them

`custom-shader` and `custom-shader-animation` are live configuration keys
(`crates/rustty-config/src/config_struct.rs:389-390`,
`crates/rustty-config/src/config_struct.rs:4076-4094`,
`crates/rustty-config/src/config_struct.rs:10235-10242`). Renderer
`DerivedConfig` receives the paths
(`crates/rustty-renderer/src/generic.rs:462-468`,
`crates/rustty-renderer/src/generic.rs:522-523`), and a fresh targeted test passed
`derived_config_maps_custom_shaders`.

This case is stronger than an unported parser. Linux derives
`has_custom_shaders`, allocates initial per-frame state, and invokes
`init_shaders` (`crates/rustty-renderer/src/generic/linux_impl.rs:809-817`,
`crates/rustty-renderer/src/generic/linux_impl.rs:493-524`,
`crates/rustty-renderer/src/generic/linux_impl.rs:932-935`). `init_shaders`
actually reads the configured files and creates API shader pipelines
(`crates/rustty-renderer/src/generic/linux_impl.rs:1292-1313`), while runtime
config changes request shader reinitialization
(`crates/rustty-renderer/src/generic/linux_impl.rs:1257-1285`).

But Linux omits terminal-dependent and per-frame uniform updates
(`crates/rustty-renderer/src/generic/linux_impl.rs:3268-3272`,
`crates/rustty-renderer/src/generic/linux_impl.rs:3550-3554`), always renders the
terminal pass directly to the frame target
(`crates/rustty-renderer/src/generic/linux_impl.rs:3655-3672`), and omits every
post-process pass (`crates/rustty-renderer/src/generic/linux_impl.rs:3742-3744`).
The nearby assertion-by-comment that custom shaders are always false because a
golden config never sets them (`crates/rustty-renderer/src/generic/linux_impl.rs:3508-3512`)
is contradicted by the production config and constructor above.

Ghostty dynamically creates/removes custom frame state, targets the first pass
at its back texture, and executes the configured post pipelines into the final
target (`ghostty/src/renderer/generic.zig:1509-1521`,
`ghostty/src/renderer/generic.zig:1589-1599`,
`ghostty/src/renderer/generic.zig:1691-1720`). Rustty may surface file or shader
compilation errors, but successfully loaded shaders have no visual effect on
Linux. That is a material accepted-configuration mismatch, P2.

## SCR-005 — P4: Linux discards live inspector overlay controls

The inspector path is now live, despite stale comments that describe its UI as
future work. Its Renderer panel presents checkboxes for hyperlink and semantic
prompt overlays and mutates an ordered feature set
(`crates/rustty-inspector/src/widgets/renderer.rs:95-110`,
`crates/rustty-inspector/src/widgets/renderer.rs:135-212`), and the surface
inspector calls that draw method (`crates/rustty-inspector/src/widgets/surface.rs:408-412`).
Activating the
inspector stores its `InspectorOverlay` provider into shared renderer state
(`crates/rustty-core/src/surface.rs:2952-3001`), and the trait returns the enabled
feature values (`crates/rustty-renderer/src/state.rs:45-56`,
`crates/rustty-inspector/src/widgets/renderer.rs:286-313`). A fresh core test of
that cross-crate provider call passed.

Linux then drops the feature extraction, overlay rebuild, image upload, and
draw steps (`crates/rustty-renderer/src/generic/linux_impl.rs:3172-3180`,
`crates/rustty-renderer/src/generic/linux_impl.rs:3227-3231`,
`crates/rustty-renderer/src/generic/linux_impl.rs:3268-3269`,
`crates/rustty-renderer/src/generic/linux_impl.rs:3735-3736`). Ghostty performs
each stage (`ghostty/src/renderer/generic.zig:1269-1282`,
`ghostty/src/renderer/generic.zig:1356-1364`,
`ghostty/src/renderer/generic.zig:1416-1423`,
`ghostty/src/renderer/generic.zig:1681-1688`).

The result is a real but diagnostic-only parity defect: inspector controls can
be toggled, yet the requested grid annotations never appear on Linux. P4 matches
the audit rubric's explicit diagnostic category; it should not inherit the P1
severity of Kitty graphics merely because both would use renderer image state.

## Consolidation guidance

1. Add one P1 FreeBSD xev compile blocker, beside PT-002 and SR-002 but under the
   same target-outage summary.
2. Add one P1 Linux Kitty-graphics finding. This is a major protocol feature
   failure, not a generic cosmetic overlay issue.
3. Add separate P2 Linux background-image and custom-shader findings. Both are
   accepted user configurations with no visual effect, but they have different
   implementation paths and remediation.
4. Add the inspector overlay as P4 diagnostic parity, not as another product P1.
5. Note the shared missing OpenGL image subsystem across Kitty, background, and
   inspector when planning remediation, without erasing their separate impacts.

## Verification limits

- No FreeBSD Rust standard library or runtime was installed. SCR-001 is proven
  by deterministic cfg selection and required-name/method resolution, rather
  than a captured cross-compiler diagnostic.
- No Linux target standard library or GTK runtime was available on this macOS
  host. The Linux findings use the target-gated production source and its live
  core call path; no screenshot or GPU capture was produced.
- Fresh targeted tests passed for renderer background/custom config mapping,
  Kitty APC parsing, and the inspector overlay-provider seam. Those tests prove
  the producers are live; none exercises the deliberately omitted Linux draw
  branches.
- No production source was edited. This review authored only this report under
  `.port/codex/`.
