//! The Metal graphics-API backend (macOS).
//!
//! PORTS: ghostty `src/renderer/Metal.zig` + `src/renderer/metal/*.zig` — the
//! Metal draw path that `GenericRenderer(Metal)` is monomorphized over. Per the
//! GPU R-d sub-blueprint (`GPU_RD_TRAIT_DESIGN.md`), the port is MONOMORPHIC:
//! the concrete `Metal` leaf types, no `GraphicsApi` trait hierarchy (a set of
//! one — OpenGL is Linux-deferred; the trait is a documented Goal-2 seam only).
//! WebGL is not a `GraphicsApi` at all: the reference's `.webgl` arm selects the
//! bare 2-line `WebGL` placeholder, ported COMPLETE at `crate::webgl` (WS14).
//!
//! PORT STATUS (GPU R-d OFFSCREEN slice — Chunks 0–4, DONE):
//!
//! - `api` — `metal/api.zig`: the Metal enum/extern-struct/extern-fn vocabulary,
//!   ported as `objc2-metal` re-exports under ghostty's names (R3: identical
//!   Apple constants), with the two `packed struct` bit-field types
//!   (`MTLResourceOptions`/`MTLTextureUsage`) re-shaped to compose objc2's
//!   bitflags ([ADV-FIX F3]). (Chunk 0)
//! - The shader library (`build.rs` compiles `src/shaders/shaders.metal` into
//!   `$OUT_DIR/rustty.metallib` via `xcrun metal`/`metallib`, setting the
//!   `rustty_metallib` cfg on success), embedded by the runtime. (Chunk 0)
//! - The resource-leaf wrappers `buffer`/`texture`/`sampler` (Chunk 1a) and the
//!   draw-plumbing leaves `pipeline`/`render_pass`/`target` (IOSurface-backed
//!   offscreen)/`frame` (the completion contract)/`shaders` (metallib load + the
//!   pipeline collection) (Chunk 1b).
//! - The `Metal` impl (`metal_impl.rs`: `init_offscreen`/`init_shaders`/the
//!   resource-option factories/the frame lifecycle, Chunk 2) + `gpu_abi` (the
//!   byte-faithful GPU-struct ABI, Chunk 4). The offscreen draw path is LIVE and
//!   golden-gated (`generic::macos_impl::tests::golden_renders_cursor_fg_bg`).
//!
//! DEFERRED (a later tier, `GPU_RD_BLUEPRINT.md`): the Frame async
//! `addCompletedHandler` present + the real windowed layer-present path
//! (windowing-host seam); `Metal::init` (needs a live NSView/IOSurfaceLayer);
//! Target's DisplayP3 `set_color_space`; and the OpenGL/GTK backends (WebGL is
//! ported complete — `crate::webgl`, a 2-line reference placeholder).

pub mod api;
pub mod buffer;
pub mod cvdisplaylink; // W2 — the CoreVideo CVDisplayLink loop driver
pub mod frame;
pub mod gpu_abi;
pub mod iosurface_layer; // W0.T3 — CALayer subclass present primitive
pub mod metal_impl;
pub mod pipeline;
pub mod render_pass;
pub mod sampler;
pub mod shaders;
pub mod target;
// RUSTTY-ONLY `#[cfg(test)]` strict-mode pivot for every "no Metal device ⇒ skip"
// arm in this crate (GPU_RD_BLUEPRINT risk F5). Sibling of
// `opengl::gl::test_support`'s `RUSTTY_REQUIRE_GL`.
#[cfg(test)]
pub(crate) mod test_support;
pub mod texture;

// The `Metal` graphics-API wrapper (ports `src/renderer/Metal.zig`). Re-exported
// at `metal::` so the crate-root `pub type Renderer = GenericRenderer<Metal>` alias
// (Chunk 3) can name `crate::metal::Metal`.
pub use metal_impl::Metal;

// PORT NOTE (rustty-only smoke test, NOT a ghostty parity test): the reference
// has no unit `test{}` block over these ObjC resource wrappers. This is the
// Chunk-1 gate from `GPU_RD_BLUEPRINT.md` ("creates buffer/texture/sampler
// without panic"); it exists solely to prove the objc2 FFI bindings + the
// ghostty-shaped Options `.resolve()` composition drive real Metal object
// creation (buffer alloc/grow/sync, texture descriptor + replaceRegion, sampler
// state) on a Metal-capable host. It is skipped cleanly on hosts with no Metal
// device (headless CI).
#[cfg(test)]
mod tests {
    use objc2_metal::{MTLBuffer, MTLDevice};

    use super::api::{
        MTLCreateSystemDefaultDevice, MTLPixelFormat, MTLResourceOptions, MTLSamplerAddressMode,
        MTLSamplerMinMagFilter, MTLTextureUsage, StorageMode,
    };
    use super::buffer::{Buffer, BufferOptions};
    use super::sampler::{Sampler, SamplerOptions};
    use super::texture::{Texture, TextureOptions};

    #[test]
    fn creates_buffer_texture_sampler_without_panic() {
        // Headless CI may lack a Metal device; skip cleanly if so.
        let Some(device) = MTLCreateSystemDefaultDevice() else {
            // F5 strict-mode pivot: no-op by default, panics under
            // `RUSTTY_REQUIRE_METAL=1` (metal::test_support).
            super::test_support::require_metal_or_skip(
                "creates_buffer_texture_sampler_without_panic",
                &"MTLCreateSystemDefaultDevice() returned nil",
            );
            eprintln!("no Metal device available; skipping Metal resource-leaf smoke test");
            return;
        };

        // A live command queue proves the device is functional.
        let _queue = device
            .newCommandQueue()
            .expect("device should vend a command queue");

        // `Shared` storage keeps the buffer CPU-visible (and is the only mode
        // available on Apple Silicon — `Managed` is Intel/discrete-only).
        let resource_options = MTLResourceOptions::new(StorageMode::Shared);

        // ----- Buffer: init / init_fill / sync (grow path) -----
        let mut buf = Buffer::<u32>::init(
            BufferOptions {
                device: device.clone(),
                resource_options,
            },
            4,
        )
        .expect("Buffer::init");
        assert_eq!(buf.len, 4);
        assert!(buf.buffer.length() >= 4 * std::mem::size_of::<u32>());

        let filled = Buffer::<u32>::init_fill(
            BufferOptions {
                device: device.clone(),
                resource_options,
            },
            &[1u32, 2, 3],
        )
        .expect("Buffer::init_fill");
        assert_eq!(filled.len, 3);

        // Sync a larger array than the 4-slot buffer to force the reallocation
        // (grow) path + the CPU-side memcpy.
        buf.sync(&[10u32, 20, 30, 40, 50, 60])
            .expect("Buffer::sync");
        assert!(buf.buffer.length() >= 6 * std::mem::size_of::<u32>());
        // `raw()` hands back the underlying MTLBuffer for a RenderPass step.
        let _raw = buf.raw();

        // ----- Texture: init(None) + replace_region -----
        let tex = Texture::init(
            TextureOptions {
                device: device.clone(),
                pixel_format: MTLPixelFormat::RGBA8Unorm,
                resource_options,
                usage: MTLTextureUsage {
                    shader_read: true,
                    ..MTLTextureUsage::default()
                },
            },
            2,
            2,
            None,
        )
        .expect("Texture::init");
        assert_eq!(tex.width, 2);
        assert_eq!(tex.height, 2);
        assert_eq!(tex.bpp, 4); // RGBA8Unorm

        // 2x2 @ 4 bpp = 16 bytes.
        let pixels = [0xABu8; 2 * 2 * 4];
        tex.replace_region(0, 0, 2, 2, &pixels);

        // ----- Sampler: init -----
        let _sampler = Sampler::init(SamplerOptions {
            device: device.clone(),
            min_filter: MTLSamplerMinMagFilter::Linear,
            mag_filter: MTLSamplerMinMagFilter::Linear,
            s_address_mode: MTLSamplerAddressMode::ClampToEdge,
            t_address_mode: MTLSamplerAddressMode::ClampToEdge,
        })
        .expect("Sampler::init");
    }

    // PORT NOTE (rustty-only smoke test, NOT a ghostty parity test): the Chunk-1b
    // gate from `GPU_RD_BLUEPRINT.md` / `GPU_RD_CHUNK1B_DESIGN §7`. It drives the
    // five draw-plumbing leaves against a real Metal device: `Shaders::init` (loads
    // the metallib + builds all five render pipelines — exercising `pipeline.rs`,
    // the hand-written vertex-attribute tables, and `shaders.rs`'s library load), a
    // `Target::init` OFFSCREEN (exercises the HARD-SPOT-1 IOSurface CFDictionary
    // path — a wrong dict makes `IOSurfaceCreate` return null → `Error::MetalFailed`,
    // so a successful init proves the dict is correct at runtime), a `Frame::begin`
    // + a trivial `RenderPass` (clear) begin/complete, `Frame::complete(sync=true)`
    // (the HARD-SPOT-2 invoke-directly path, with a no-op `CompletionFn`), and the
    // `read_bgra` golden-support readback. The ordered-step draw loop is Chunk 3 and
    // the golden pixel comparison is Chunk 4; here we only assert no panic + the
    // readback buffer is `w*h*4` bytes. Skipped cleanly on hosts with no Metal
    // device (headless CI), like the landed resource-leaf smoke test.
    #[test]
    fn draw_plumbing_smoke() {
        use objc2::runtime::ProtocolObject;
        use objc2_metal::{MTLCommandBuffer, MTLDevice};

        use super::api::{MTLCreateSystemDefaultDevice, MTLPixelFormat, StorageMode};
        use super::frame::{CompletionContext, Frame, Options as FrameOptions};
        use super::render_pass::{Attachment, AttachmentTarget};
        use super::shaders::Shaders;
        use super::target::{Options as TargetOptions, Target};

        let Some(device) = MTLCreateSystemDefaultDevice() else {
            // F5 strict-mode pivot (metal::test_support).
            super::test_support::require_metal_or_skip(
                "draw_plumbing_smoke",
                &"MTLCreateSystemDefaultDevice() returned nil",
            );
            eprintln!("no Metal device available; skipping Metal draw-plumbing smoke test");
            return;
        };

        let queue = device
            .newCommandQueue()
            .expect("device should vend a command queue");

        // Shaders::init loads the metallib (or the source fallback) and builds all
        // five render pipelines. No post-shaders.
        let shaders =
            Shaders::init(&device, &[], MTLPixelFormat::BGRA8Unorm).expect("Shaders::init");
        assert!(shaders.post_pipelines.is_empty());

        // Target::init OFFSCREEN — exercises the IOSurface CFDictionary path.
        let w = 4usize;
        let h = 4usize;
        let target = Target::init(TargetOptions {
            device: &device,
            width: w,
            height: h,
            // Golden pins BGRA8Unorm (non-sRGB) + .Shared (Apple Silicon).
            pixel_format: MTLPixelFormat::BGRA8Unorm,
            storage_mode: StorageMode::Shared,
        })
        .expect("Target::init (IOSurface offscreen)");
        assert_eq!(target.width, w);
        assert_eq!(target.height, h);

        // A no-op completion fn (the Chunk-3 `drawFrame` installs the real one).
        fn noop_completed(
            _ctx: &CompletionContext,
            _buffer: &ProtocolObject<dyn MTLCommandBuffer>,
        ) {
        }

        // Frame::begin over a context tied to no renderer (N440: the context
        // carries clones, not pointers), so `target` stays usable below.
        let mut frame = Frame::begin(
            FrameOptions { queue: &queue },
            CompletionContext::detached(&target.surface),
            noop_completed,
        )
        .expect("Frame::begin");

        // A trivial RenderPass: clear the target, begin/complete (no draw step).
        {
            let attachments = [Attachment {
                target: AttachmentTarget::Target(&target),
                clear_color: Some([0.0, 0.0, 0.0, 1.0]),
            }];
            let pass = frame.render_pass(&attachments);
            pass.complete();
        }

        // Complete synchronously: commit → waitUntilCompleted → invoke `on_completed`.
        frame.complete(true);

        // Golden-support readback: `w*h*4` BGRA bytes out of the offscreen surface.
        let pixels = target.read_bgra();
        assert_eq!(pixels.len(), w * h * 4);
    }
}
