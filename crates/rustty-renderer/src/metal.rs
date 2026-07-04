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
//! CHUNK 1a lands the three PURE resource-leaf wrappers (`buffer`/`texture`/
//! `sampler`) over `objc2`/`objc2-metal` (`GPU_RD_TRAIT_DESIGN.md` §B/§C).
//!
//! CHUNK 1b lands the five draw-plumbing leaves (`pipeline`/`render_pass`/
//! `target`/`frame`/`shaders`) — the `Pipeline`/`RenderPass` step machinery, the
//! IOSurface-backed offscreen `Target`, the `Frame` completion contract, and the
//! `Shaders` backend container (metallib load + the pipeline collection). These
//! compile + link against Chunk-1a + `crate::shaders` with no reference to the
//! not-yet-existing `GenericRenderer`/`Metal` (Frame's completion callback is a
//! forward-declared fn-pointer contract; the concrete wiring lands in Chunk 3).
//!
//! DEFERRED (Chunk 2/3+, `GPU_RD_BLUEPRINT.md`): the `Metal` impl and the
//! `generic.zig` renderer body — the golden-image-gated offscreen draw path; the
//! Frame async `addCompletedHandler` present (windowing-host seam); and Target's
//! DisplayP3 `set_color_space` (needs objc2-core-graphics, Chunk 4).

pub mod api;
pub mod buffer;
pub mod frame;
pub mod pipeline;
pub mod render_pass;
pub mod sampler;
pub mod shaders;
pub mod target;
pub mod texture;

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
        let mut target = Target::init(TargetOptions {
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
        unsafe fn noop_completed(
            _ctx: &CompletionContext,
            _buffer: &ProtocolObject<dyn MTLCommandBuffer>,
        ) {
        }

        // Frame::begin: type-erased null renderer ptr; raw `*mut` target. The raw
        // pointer does not hold the borrow, so `target` stays usable below.
        let target_ptr: *mut Target = &mut target;
        let mut frame = Frame::begin(
            FrameOptions { queue: &queue },
            core::ptr::null_mut(),
            target_ptr,
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
