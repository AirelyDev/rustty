//! Wrapper for handling textures.
//!
//! PORTS: ghostty `src/renderer/metal/Texture.zig` (a file-as-struct: the
//! top-level `texture`/`width`/`height`/`bpp` fields ARE the struct).
//!
//! PORT NOTE (R3/objc2, ARC): ghostty's `deinit()` (`self.texture.release()`)
//! dissolves into the `Retained<ProtocolObject<dyn MTLTexture>>` field's
//! automatic `Drop` — no explicit `Drop` impl is needed. objc2's typed
//! `MTLTextureDescriptor` setters replace ghostty's untyped `setProperty`
//! message sends (identical Apple selectors), and `newTextureWithDescriptor:`
//! returns `Option<Retained<…>>` where ghostty's `?*anyopaque` `orelse` yields
//! `error.MetalFailed`.

use core::ffi::c_void;
use core::ptr::NonNull;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLDevice, MTLTexture, MTLTextureDescriptor};

use rustty_util::quirks::inlineAssert as assert;

use crate::metal::api;

// const log = std.log.scoped(.metal);
// PORT NOTE: no logging facade is wired in the port yet (see `shadertoy.rs`).

/// Options for initializing a texture.
//
// PORT NOTE (§C): ghostty's `Texture.Options` → `TextureOptions` (see
// `buffer.rs` for the flat-naming rationale).
pub struct TextureOptions {
    /// MTLDevice
    pub device: Retained<ProtocolObject<dyn MTLDevice>>,
    pub pixel_format: api::MTLPixelFormat,
    pub resource_options: api::MTLResourceOptions,
    pub usage: api::MTLTextureUsage,
}

pub struct Texture {
    /// The underlying MTLTexture Object.
    pub texture: Retained<ProtocolObject<dyn MTLTexture>>,

    /// The width of this texture.
    pub width: usize,
    /// The height of this texture.
    pub height: usize,

    /// Bytes per pixel for this texture.
    pub bpp: usize,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// A Metal API call failed.
    MetalFailed,
}

impl Texture {
    /// Initialize a texture
    pub fn init(
        opts: TextureOptions,
        width: usize,
        height: usize,
        data: Option<&[u8]>,
    ) -> Result<Self, Error> {
        // Create our descriptor
        //
        // PORT NOTE (ARC): ghostty's explicit `alloc`/`init` two-step plus
        // `defer desc.release()` becomes objc2's `MTLTextureDescriptor::new()`
        // (`[[MTLTextureDescriptor alloc] init]`) owned by a `Retained` that
        // releases at end of scope.
        let desc = MTLTextureDescriptor::new();

        // Set our properties
        desc.setPixelFormat(opts.pixel_format);
        // SAFETY: objc2 marks `setWidth`/`setHeight` `unsafe` (dimension
        // invariants); `width`/`height` are plain sizes handed to Metal.
        unsafe {
            desc.setWidth(width);
            desc.setHeight(height);
        }
        desc.setResourceOptions(opts.resource_options.resolve());
        desc.setUsage(opts.usage.resolve());

        // Initialize
        let texture = opts
            .device
            .newTextureWithDescriptor(&desc)
            .ok_or(Error::MetalFailed)?;

        let this = Self {
            texture,
            width,
            height,
            bpp: bpp_of(opts.pixel_format),
        };

        // If we have data, we set it here.
        if let Some(d) = data {
            assert(d.len() == width * height * this.bpp);
            this.replace_region(0, 0, width, height, d);
        }

        Ok(this)
    }

    // PORT NOTE (ARC): `deinit()` (`self.texture.release()`) dissolves into the
    // `Retained` field's automatic `Drop` — no manual impl.

    /// Replace a region of the texture with the provided data.
    ///
    /// Does NOT check the dimensions of the data to ensure correctness.
    //
    // PORT NOTE: ghostty's signature is `error{}!void` (an empty error set — it
    // cannot fail); ported as an infallible `fn` returning `()`. Correspondingly
    // `init`'s `try self.replaceRegion(...)` becomes a plain call.
    pub fn replace_region(&self, x: usize, y: usize, width: usize, height: usize, data: &[u8]) {
        // SAFETY: `data` is a valid `&[u8]` slice; objc2 marks `replaceRegion:…`
        // `unsafe` because the byte pointer must be valid, which it is. (As the
        // doc note says, ghostty does NOT bounds-check `data` against the
        // region — that contract is preserved.)
        unsafe {
            self.texture
                .replaceRegion_mipmapLevel_withBytes_bytesPerRow(
                    api::MTLRegion {
                        origin: api::MTLOrigin { x, y, z: 0 },
                        size: api::MTLSize {
                            width,
                            height,
                            depth: 1,
                        },
                    },
                    0,
                    NonNull::new(data.as_ptr() as *mut c_void).unwrap(),
                    self.bpp * width,
                );
        }
    }
}

/// Returns the bytes per pixel for the provided pixel format
//
// PORT NOTE: Zig's `switch` places `else` second (after `.invalid`); Rust's
// catch-all `_` must be the terminal arm, so the "Weird formats…" arm is moved
// to the end. Behavior is identical (the value patterns are disjoint). The
// pixel-format variants are matched against objc2's associated `const`s
// (`api::MTLPixelFormat` re-exports `objc2_metal::MTLPixelFormat`, which derives
// `PartialEq`/`Eq`, so its consts are valid patterns).
fn bpp_of(pixel_format: api::MTLPixelFormat) -> usize {
    use api::MTLPixelFormat as P;
    match pixel_format {
        // Invalid
        P::Invalid => panic!("invalid pixel format"),

        // 8-bit pixel formats
        P::A8Unorm
        | P::R8Unorm
        | P::R8Unorm_sRGB
        | P::R8Snorm
        | P::R8Uint
        | P::R8Sint
        | P::RG8Unorm
        | P::RG8Unorm_sRGB
        | P::RG8Snorm
        | P::RG8Uint
        | P::RG8Sint
        | P::Stencil8 => 1,

        // 16-bit pixel formats
        P::R16Unorm
        | P::R16Snorm
        | P::R16Uint
        | P::R16Sint
        | P::R16Float
        | P::RG16Unorm
        | P::RG16Snorm
        | P::RG16Uint
        | P::RG16Sint
        | P::RG16Float
        | P::B5G6R5Unorm
        | P::A1BGR5Unorm
        | P::ABGR4Unorm
        | P::BGR5A1Unorm
        | P::Depth16Unorm => 2,

        // 32-bit pixel formats
        P::RGBA8Unorm
        | P::RGBA8Unorm_sRGB
        | P::RGBA8Snorm
        | P::RGBA8Uint
        | P::RGBA8Sint
        | P::BGRA8Unorm
        | P::BGRA8Unorm_sRGB
        | P::RGB10A2Unorm
        | P::RGB10A2Uint
        | P::RG11B10Float
        | P::RGB9E5Float
        | P::BGR10A2Unorm
        | P::BGR10_XR
        | P::BGR10_XR_sRGB
        | P::R32Uint
        | P::R32Sint
        | P::R32Float
        | P::Depth32Float
        | P::Depth24Unorm_Stencil8 => 4,

        // 64-bit pixel formats
        P::RG32Uint
        | P::RG32Sint
        | P::RG32Float
        | P::RGBA16Unorm
        | P::RGBA16Snorm
        | P::RGBA16Uint
        | P::RGBA16Sint
        | P::RGBA16Float
        | P::BGRA10_XR
        | P::BGRA10_XR_sRGB => 8,

        // 128-bit pixel formats,
        //
        // FAITHFULNESS QUIRK (R1): ghostty returns `128` here (not `16`), an
        // inconsistency with the byte-per-pixel values above (8-bit→1, 16→2,
        // 32→4, 64→8). Reproduced verbatim, NOT corrected.
        // RE-AFFIRMED (P3-W0 T5): checked against oracle range 4749c4e93..6ad1fe7d8 — src/renderer/metal/Texture.zig is UNTOUCHED there, so this quirk is still faithful. Ledger: .port/tools/gates/stale-comments-D9.tsv.
        P::RGBA32Uint | P::RGBA32Sint | P::RGBA32Float => 128,

        // Weird formats I was too lazy to get the sizes of
        _ => panic!("pixel format size unknown (unlikely that this format was actually used, could be memory corruption)"),
    }
}
