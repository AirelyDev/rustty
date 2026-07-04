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
//! DEFERRED (Chunk 1b+, `GPU_RD_BLUEPRINT.md`): the remaining `metal/*` leaf
//! wrappers (`Pipeline`/`RenderPass`/`Target`/`Frame`/`shaders`), the `Metal`
//! impl, and the `generic.zig` renderer body — the golden-image-gated
//! offscreen draw path.

pub mod api;
pub mod buffer;
pub mod sampler;
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
}
