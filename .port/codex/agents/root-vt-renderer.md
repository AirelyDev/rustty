# VT allocation, FreeBSD xev, and Linux renderer audit

Audit date: 2026-07-31  
Rustty: `5243303fbb2f2cfd0832edfb8fa49d72ce322d81`  
Ghostty: `6ad1fe7d8cbda36c77b337a96c9bea8a77883699`

This report contains findings developed by the primary reviewer and sent to
other reviewers for falsification. It is intentionally source- and
consequence-specific; stale `DEFERRED` comments whose bodies are now implemented
were rejected.

## Findings

| ID | Severity | Target | Finding |
| --- | --- | --- | --- |
| RV-001 | P1 | FreeBSD | `rustty-xev` selects kqueue but compiles only its macOS implementation; real consumers cannot type-check against the fallback. |
| RV-002 | P2 | C ABI / all non-macOS embedders | `rustty_terminal_new` ignores the caller allocator and cannot return its allocation failure. |
| RV-003 | P1 | Linux GTK/OpenGL | Kitty graphics are parsed/stored but never uploaded or drawn. |
| RV-004 | P2 | Linux GTK/OpenGL | `background-image` is accepted but never loaded or drawn. |
| RV-005 | P2 | Linux GTK/OpenGL | Custom shaders are loaded/compiled but their uniforms, render target, and post-processing passes are omitted. |
| RV-006 | P2 | Linux GTK/OpenGL | Renderer scrollbar updates are dropped by a permanent no-op mailbox. |
| RV-007 | P3 | VT core / C ABI | Recoverable allocator-failure behavior and two exact failure assertions are absent; Rust aborts at global allocations where Ghostty preserves state and returns. |
| RV-008 | P4 | Linux GTK/OpenGL inspector | Renderer inspector overlays are never built, uploaded, or drawn. |

### RV-001 — FreeBSD xev consumers cannot compile against the fallback seam

Rustty's backend selector explicitly returns `Backend::Kqueue` and the singleton
`[Kqueue]` candidate set on FreeBSD
(`crates/rustty-xev/src/backend.rs:46-51,100-104`). The implementation does not
follow that selector: the kqueue loop, common loop types, async/process/stream/
timer watchers, and public kqueue API exports are all gated to macOS
(`crates/rustty-xev/src/lib.rs:180-226,290-323`). The catch-all
`no_backend_seam` excludes macOS/Linux/Windows/WASI but does not exclude FreeBSD
(`lib.rs:612-629`), so FreeBSD receives `Loop`, `Async`, `Timer`, `Process`, and
`Stream` methods implemented with `unimplemented!` (`lib.rs:630-702`).

Ghostty's pinned libxev is explicit in the opposite direction: its selector maps
FreeBSD to kqueue, `main.zig` instantiates `Kqueue`, its root test imports that
backend on FreeBSD, and `backend/kqueue.zig` contains target-specific FreeBSD
`Kevent`, `NOTE.EXIT`, syscall, and eventfd paths. This is not a claim that the
macOS kqueue code could simply be cfg-widened; Ghostty has real FreeBSD-specific
branches that Rustty has not ported.

Impact: even if the independent GTK and renderer FreeBSD blockers were fixed,
the async terminal runtime would not type-check: the fallback lacks public
`Options`, `RunMode`, callback types, and methods required by termio and core.
A narrow caller that only reaches the matching placeholder methods would panic,
but the product fails earlier at compilation. `cargo tree -p rustty-termio --target
x86_64-unknown-freebsd -i rustty-xev --offline` confirmed the live dependency
from termio (and renderer/inspector) to this crate. The target standard library is
not installed, so no full FreeBSD binary was claimed to have been executed.
`rustc --print cfg --target x86_64-unknown-freebsd` did confirm the evaluated
`target_os="freebsd"` predicates.

### RV-002 — terminal construction disregards the public allocator contract

Both projects' current headers describe the allocator argument to terminal
construction and the allocator header promises explicit allocation plus graceful
out-of-memory results. Ghostty's constructor creates the terminal, wrapper, IO
object, terminal internals, and stream using `lib.alloc.default(alloc_)`, catching
failure as `out_of_memory` (`src/terminal/c/terminal.zig:376-450`).

Rustty resolves the supplied allocator into `_alloc` and then never uses it. It
initializes the terminal with `allocator_api2::alloc::Global`, boxes the terminal
and wrapper with Rust's global heap, and initializes the parser with a process-
global allocator (`crates/rustty-vt-c/src/terminal.rs:947-968,1009,1048-1062`).
Free likewise uses `Box::from_raw`, not the caller allocator (`terminal.rs:2746-2810`).

This was behaviorally reproduced with the same C program against both current
headers and current libraries. The custom allocator rejected every request:

```text
Rustty:  result=0  terminal_nonnull=1  alloc_calls=0
Ghostty: result=-1 terminal_nonnull=0  alloc_calls=1
```

The reproducer is `scripts/check-terminal-allocator.c`; commands and raw output
are in `evidence/terminal-allocator/observed.txt`. This disproves the possible
counter-theory that Rustty merely uses a different sequence of calls through the
same allocator. It never invokes it. Consequences include bypassing arena/quota/
failure-injection allocators, allocating in the wrong heap domain for embedders,
and returning success where the API contract and Ghostty return
`RUSTTY_OUT_OF_MEMORY`/`GHOSTTY_OUT_OF_MEMORY`.

### RV-003 — Linux never renders Kitty graphics

Rustty's Linux renderer explicitly has no graphics-API-generic image state because
`crate::image` embeds a Metal texture (`generic/linux_impl.rs:20-30`). Its live
`update_frame` omits `kitty_requires_update` and `kitty_update`
(`:3028-3037,3155-3157`), and `draw_frame_raw` omits all three placement layers:
below background, below text, and above text (`:3371-3378,3550-3554,3692,3710,
3735-3736`).

Ghostty's same generic renderer updates Kitty storage on dirty frames and emits
all three ordered image draw groups (`src/renderer/generic.zig:1228-1245,
1629-1678`). Rustty's VT Kitty storage and loading limits are present, so input is
accepted and retained; only the OpenGL presentation path is missing. The verified
consequence is a silent text-only Linux rendering result for Kitty image protocol
content, not a parser rejection.

### RV-004 — Linux accepts but ignores `background-image`

The public config parser recognizes `background-image` and its fit/opacity/
position/repeat keys (`crates/rustty-config/src/config_struct.rs:358-362,
1530-1616`). `DerivedConfig` carries the path into the renderer. On Linux,
however, initialization omits `prepBackgroundImage`, config changes omit the path
change/load branch, no `bg_image` field exists, and the render pass always takes
the solid background-color arm (`generic/linux_impl.rs:925-933,1204-1255,
3550-3554,3674-3692`).

Ghostty loads/decodes the configured path, uploads the image, and emits the
background-image pipeline (`generic.zig:1743-1837,1551-1566,1601-1628`). Changing
only fit/opacity metadata in Rustty updates an otherwise unused GPU buffer; it
does not make the source image exist. Thus valid Linux configuration is accepted
without warning but produces no image.

### RV-005 — Linux custom shaders are compiled but not applied

Linux initialization derives `has_custom_shaders`, loads shader files, and calls
the OpenGL shader compiler (`generic/linux_impl.rs:810-817,1289-1313`). This rules
out “custom shaders are unsupported and rejected” as an explanation. The live
frame path then omits the state setup/teardown, per-frame uniform update and upload,
offscreen attachment selection, and every custom-shader post-processing pass
(`:3036-3037,3508-3512,3550-3554,3659-3669,3742-3744`).

Ghostty performs each of those steps (`src/renderer/generic.zig:1305-1324,
1360-1372,1685-1720`). Therefore a Linux user can supply a valid shader that is
read and compiled but cannot affect a rendered frame. The stale comment that the
golden's empty config makes `has_custom_shaders` always false is only true of that
test fixture, not the production configuration path.

### RV-006 — Linux scrollbar state never reaches GTK

The Linux renderer stores a unit `SurfaceMailbox`; `push` always returns zero
(`generic/linux_impl.rs:210-234`). Core passes the real
`SurfaceMailbox<A>` into its renderer seam, but the seam discards it before
constructing the real renderer (`crates/rustty-core/src/renderer_seam.rs:280-326`).
After a draw, the renderer tries to publish its new `Scrollbar`, sees zero
recipients, leaves `scrollbar_dirty` set, and repeats the no-op on future frames
(`generic/linux_impl.rs:3403-3418`).

The downstream path is otherwise implemented: core handles `Message::Scrollbar`
and GTK configures the surface's vertical adjustment
(`crates/rustty-core/src/surface.rs:3372-3376,4536-4552`;
`crates/rustty-apprt/src/gtk/class/surface.rs:1805-1845`). Ghostty sends the
message from the same draw defer (`generic.zig:1450-1455`). The missing link
therefore leaves the GTK scrollbar stale rather than representing an intentionally
unused message. Renderer-health also goes through the no-op mailbox, but GTK's
current reference action remains unimplemented, so this finding does not inflate
impact by claiming a GTK health UI regression.

### RV-007 — recoverable OOM behavior and assertions were dropped

Ghostty's page compression candidate owns its exact encoded allocation through
the PageList allocator. `compressPage` catches allocation failure before mutating
the resident node, and incremental traversal advances to the next serial. The
current test `PageList incremental compression advances after allocation failure`
arms `FailingAllocator` at that exact request and asserts: an induced failure,
`pending`, the first page still resident, and the following call compressing the
second page (`src/terminal/PageList.zig:7257-7283`).

Rustty's encoded candidate is `scratch[..encoded_len].to_vec()` on the process
global allocator (`crates/rustty-vt/src/compress/page.rs:155-159`). The enclosing
code returns `false` for a supposed `OutOfMemory`, but `CompressedPage::init`
cannot return allocation OOM (`pagelist/compress.rs:390-425`). The Rust test file
explicitly marks the allocator-failure test not ported for this reason
(`pagelist/tests.rs:3984-4011`). On allocation exhaustion the process aborts;
there is no preserved-page/advance result.

A second assertion family is also omitted. Ghostty's `PageList preserved page
keeps compressed storage` injects failure while decoding a compressed node and
asserts `OutOfMemory`, compressed storage, and unchanged stats before a successful
retry (`PageList.zig:7737-7785`). Rustty ports the success path and the resident
no-allocation assertion but states that the compressed failure arm cannot be
ported because `page_preserving_state` uses the global allocator and panics/aborts
(`pagelist/tests.rs:2995-3011`).

Controls passed: the Rust alternatives for incompressible compression and strict
decommit failure both advance correctly, and the success-path preservation test
passes. These controls narrow the finding to allocation failure; they do not
substitute for Ghostty's externally different OOM semantics.

### RV-008 — Linux inspector overlay is absent

Rustty's inspector can select `HighlightHyperlinks` and `SemanticPrompts`, and the
CPU overlay renderer has real pixel implementations. Linux `update_frame` omits
reading those features and calling `rebuild_overlay`; it also omits overlay image
upload and the above-text draw (`generic/linux_impl.rs:3033-3035,3172-3174,
3229-3231,3268-3269,3735-3736`). Ghostty's generic renderer does all three
(`generic.zig:1269-1282,1358-1360,1674-1683`). Impact is limited to optional
inspector visualization, hence P4 rather than the severity of normal terminal
graphics.

## Verification and rejected theories

- `scripts/check-explicit-port-gaps.py` checks the FreeBSD selector/implementation
  contradiction and each explicit renderer cut against the current Ghostty tree;
  all predicates pass.
- The custom allocator C reproducer was run against freshly built current
  libraries, not checked-in historical binaries.
- The curated current VT differential fixtures still matched after the stale
  driver was locally compatibility-shimmed; no normal-input VT mismatch is claimed.
- The many older `DEFERRED` comments in termio and font discovery were not treated
  as findings where live bodies and tests now exist.
- FreeBSD and Linux GUI execution remain unavailable on this macOS host. The
  findings above are limited to deterministic target cfgs, explicit production
  omissions, and caller/callee reachability rather than invented runtime traces.
