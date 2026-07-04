//! The Metal graphics-API backend (macOS).
//!
//! PORTS: ghostty `src/renderer/Metal.zig` + `src/renderer/metal/*.zig` — the
//! Metal draw path that `GenericRenderer(Metal)` is monomorphized over. Per the
//! GPU R-d sub-blueprint (`GPU_RD_TRAIT_DESIGN.md`), the port is MONOMORPHIC:
//! the concrete `Metal` leaf types, no `GraphicsApi` trait hierarchy (a set of
//! one — OpenGL is Linux-deferred, WebGL is wasm-deferred; the trait is a
//! documented Goal-2 seam only).
//!
//! PORT STATUS: Chunk 0 (the Metal API vocabulary + shader build infra) only:
//!
//! - `api` — `metal/api.zig`: the Metal enum/extern-struct/extern-fn vocabulary,
//!   ported as `objc2-metal` re-exports under ghostty's names (R3: identical
//!   Apple constants), with the two `packed struct` bit-field types
//!   (`MTLResourceOptions`/`MTLTextureUsage`) re-shaped to compose objc2's
//!   bitflags ([ADV-FIX F3]).
//!
//! The shader library (`build.rs` compiles `src/shaders/shaders.metal` into
//! `$OUT_DIR/rustty.metallib` via `xcrun metal`/`metallib`, setting the
//! `rustty_metallib` cfg on success) is embedded by the Chunk-1 runtime.
//!
//! DEFERRED (Chunk 1+, `GPU_RD_BLUEPRINT.md`): the `metal/*` leaf wrappers
//! (`buffer`/`Texture`/`Sampler`/`Pipeline`/`RenderPass`/`Target`/`Frame`/
//! `shaders`), the `Metal` impl, and the `generic.zig` renderer body — the
//! golden-image-gated offscreen draw path.

pub mod api;
