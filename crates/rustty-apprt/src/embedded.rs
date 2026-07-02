//! rustty-apprt::embedded
//!
//! PORTS: ghostty `src/apprt/embedded.zig` (2250 loc, 0 tests) — the application
//! runtime for the embedded version of Ghostty (when Ghostty is embedded within
//! a parent host application rather than owning the application lifecycle
//! itself; used e.g. for the macOS Swift+XCode build). This is the FIRST concrete
//! apprt backend — it proves the `apprt_api` generic trait seam against a real
//! `A = embedded::App` (EMBEDDED_BLUEPRINT §2).
//!
//! PORT STATUS: shell (chunk E0 — scaffold). The backend types + trait impls +
//! surface callbacks land in chunks E1..E10 (EMBEDDED_BLUEPRINT §7). In order:
//!
//! E1 (embedded.zig 29–342): `App` + `Options` (host callbacks) + `KeyEvent` +
//! lifecycle/event/keymap/wakeup/newSurface/…; `impl apprt_api::App for App`.
//!
//! E2 (343–609): `Platform`/`PlatformTag`/`EnvVar` + `Surface` struct +
//! `init`/`deinit`; `impl apprt_api::Surface for Surface`.
//!
//! E3 (610–990): surface event callbacks (refresh/draw/updateSize/mouse/…;
//! draw/refresh reach the Phase-3 GPU leaf at runtime).
//!
//! E4 (992–1243): `Inspector` (dcimgui+Metal) — a Phase-3 inhabited placeholder
//! with `unimplemented!()` leaves. Deferrable/parallel.
//!
//! [ADV-FIX 8] PINNING INVARIANT (correctness-critical, for E2): `embedded::Surface`
//! holds `core_surface: rustty_core::Surface<App>` BY VALUE, and core
//! `Surface::init` writes `core_surface.rt_surface = &embedded_surface` (a pointer
//! back to the container). The embedded `Surface` MUST therefore be heap-allocated
//! (Box) and never moved after init — `ghostty_surface_new` returns a `*mut Surface`
//! to a boxed, pinned backend; any move dangles `rt_surface`.
//!
//! Carved incrementally (R1: every line/comment/test{} ported as the crate is
//! carved, leaves-first, under the acyclicity gate per R4).

// PORT: reference `pub const resourcesDir = internal_os.resourcesDir;`
// (embedded.zig:26). The remaining `App`/`Surface`/`Platform`/`Inspector` decls
// land in E1..E10.
pub use rustty_os::resourcesDir;
