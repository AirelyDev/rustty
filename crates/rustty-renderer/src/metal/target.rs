//! Represents a render target.
//!
//! In this case, an IOSurface-backed MTLTexture.
//!
//! PORTS: ghostty `src/renderer/metal/Target.zig` (+ its `pkg/macos/iosurface`
//! `IOSurface.init` helper for the property-dictionary construction).
//!
//! PORT NOTE (R3/objc2, HARD SPOT 1 [F12]): ghostty's `macos.iosurface.IOSurface`
//! (a hand-rolled binding over `IOSurfaceCreate`) maps to `objc2-io-surface`'s
//! `IOSurfaceRef` (bindings to the SAME IOSurface.framework — not a
//! reimplementation), and `macos.graphics`/`macos.foundation` map to
//! `objc2-core-foundation`. The 4-key property `CFDictionary` of `CFNumber(.int)`
//! values is built directly here (ghostty's `IOSurface.init` did the same via
//! `foundation.Dictionary.create`). ARC: the `*IOSurface` becomes a
//! `CFRetained<IOSurfaceRef>` whose `Drop` CFReleases; the MTLTexture becomes a
//! `Retained` whose `Drop` releases.
//!
//! PORT NOTE (R5 deferred seam, offscreen scope [F12]): ghostty sets the surface's
//! color space to Display P3 (see the `set_color_space` comment in `init`). The
//! offscreen golden reads RAW BGRA bytes — which ignore the IOSurface colorspace
//! tag — and the golden's determinism pin requires `colorspace != display-p3`, so
//! the colorspace-set is SKIPPED offscreen and R5-preserved as a deferred
//! windowing-host seam (it needs `objc2-core-graphics`, a Chunk-4 dep).

use core::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_core_foundation::{
    kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionary, CFNumber,
    CFNumberType, CFRetained, CFString,
};
use objc2_io_surface::{
    kIOSurfaceBytesPerElement, kIOSurfaceHeight, kIOSurfacePixelFormat, kIOSurfaceWidth,
    IOSurfaceLockOptions, IOSurfaceRef,
};
use objc2_metal::{MTLDevice, MTLTexture, MTLTextureDescriptor};

use crate::metal::api;

// const log = std.log.scoped(.metal);
// PORT NOTE: no logging facade is wired in the port yet (see `buffer.rs`).

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// A Metal API call failed.
    MetalFailed,
}

/// Options for initializing a Target
pub struct Options<'a> {
    /// MTLDevice
    pub device: &'a ProtocolObject<dyn MTLDevice>,

    /// Desired width
    pub width: usize,
    /// Desired height
    pub height: usize,

    /// Pixel format for the MTLTexture
    pub pixel_format: api::MTLPixelFormat,
    /// Storage mode for the MTLTexture
    pub storage_mode: api::StorageMode,
}

pub struct Target {
    /// The underlying IOSurface.
    pub surface: CFRetained<IOSurfaceRef>,

    /// The underlying MTLTexture.
    pub texture: Retained<ProtocolObject<dyn MTLTexture>>,

    /// Current width of this target.
    pub width: usize,
    /// Current height of this target.
    pub height: usize,
}

impl Target {
    pub fn init(opts: Options<'_>) -> Result<Self, Error> {
        // We set our surface's color space to Display P3.
        // This allows us to have "Apple-style" alpha blending,
        // since it seems to be the case that Apple apps like
        // Terminal and TextEdit render text in the display's
        // color space using converted colors, which reduces,
        // but does not fully eliminate blending artifacts.
        //
        // R5 DEFERRED SEAM (`set_color_space`, offscreen scope [F12]): ghostty here
        //     const colorspace = try graphics.ColorSpace.createNamed(.displayP3);
        //     defer colorspace.release();
        //     ... .colorspace = colorspace ...            // (IOSurface.init property)
        //     if (properties.colorspace) |space| surface.setColorSpace(space);
        // where `setColorSpace` = CGColorSpaceCopyPropertyList + IOSurfaceSetValue
        // (kIOSurfaceColorSpace). The offscreen readback ignores this tag and the
        // golden determinism pin requires colorspace != display-p3, so it is SKIPPED
        // here and preserved as this deferred windowing-host seam (needs
        // objc2-core-graphics, a Chunk-4 dep).

        // The IOSurface property dictionary (ghostty `IOSurface.init` /
        // `foundation.Dictionary.create`): a 4-key dict of `CFNumber(.int)` values.
        // Width/Height/BytesPerElement are the opts; PixelFormat is the FourCC
        // 'BGRA' == 0x42475241 (kCVPixelFormatType_32BGRA), ghostty's
        // `pixel_format = .@"32BGRA"`, `bytes_per_element = 4`.
        const BGRA_FOURCC: i32 = 0x4247_5241; // 'B' 'G' 'R' 'A'
        let w = cf_number_int(opts.width as i32).ok_or(Error::MetalFailed)?;
        let h = cf_number_int(opts.height as i32).ok_or(Error::MetalFailed)?;
        let pf = cf_number_int(BGRA_FOURCC).ok_or(Error::MetalFailed)?;
        let bpe = cf_number_int(4).ok_or(Error::MetalFailed)?;

        // CFDictionaryCreate(keys, values, 4, kCFTypeDictionary{Key,Value}CallBacks).
        // Keys are the `&'static CFString` externs; values are the CFNumbers.
        // SAFETY: reading the `kIOSurface*` extern statics; each is a valid
        // `&'static CFString` whose address we hand to CoreFoundation.
        let mut keys: [*const c_void; 4] = unsafe {
            [
                (kIOSurfaceWidth as *const CFString).cast(),
                (kIOSurfaceHeight as *const CFString).cast(),
                (kIOSurfacePixelFormat as *const CFString).cast(),
                (kIOSurfaceBytesPerElement as *const CFString).cast(),
            ]
        };
        let mut vals: [*const c_void; 4] = [
            (&*w as *const CFNumber).cast(),
            (&*h as *const CFNumber).cast(),
            (&*pf as *const CFNumber).cast(),
            (&*bpe as *const CFNumber).cast(),
        ];
        // SAFETY: 4 valid keys/values; `kCFTypeDictionary{Key,Value}CallBacks`
        // retain/release the CF objects (so the dict keeps its own refs), and
        // `num_values` (4) matches the array lengths.
        let props: CFRetained<CFDictionary> = unsafe {
            CFDictionary::new(
                None,
                keys.as_mut_ptr(),
                vals.as_mut_ptr(),
                4,
                core::ptr::addr_of!(kCFTypeDictionaryKeyCallBacks),
                core::ptr::addr_of!(kCFTypeDictionaryValueCallBacks),
            )
        }
        .ok_or(Error::MetalFailed)?;

        // IOSurfaceCreate. ghostty's `orelse return error.OutOfMemory` → MetalFailed.
        // SAFETY: `props` holds the correctly-typed IOSurface properties (CFString
        // keys → CFNumber(int) values).
        let surface: CFRetained<IOSurfaceRef> =
            unsafe { IOSurfaceRef::new(&props) }.ok_or(Error::MetalFailed)?;

        // Create our descriptor
        //
        // PORT NOTE (ARC): ghostty's `alloc`/`init` two-step + `defer desc.release()`
        // becomes `MTLTextureDescriptor::new()` owned by a scope-releasing `Retained`.
        let desc = MTLTextureDescriptor::new();

        // Set our properties
        // SAFETY: objc2 marks `setWidth`/`setHeight` unsafe (dimension invariants);
        // `width`/`height` are plain sizes handed to Metal.
        unsafe {
            desc.setWidth(opts.width);
            desc.setHeight(opts.height);
        }
        desc.setPixelFormat(opts.pixel_format);
        desc.setUsage(
            api::MTLTextureUsage {
                render_target: true,
                ..Default::default()
            }
            .resolve(),
        );
        desc.setResourceOptions(
            api::MTLResourceOptions {
                // Indicate that the CPU writes to this resource but never reads it.
                cpu_cache_mode: api::CPUCacheMode::WriteCombined,
                storage_mode: opts.storage_mode,
                hazard_tracking_mode: api::HazardTrackingMode::Default,
            }
            .resolve(),
        );

        // newTextureWithDescriptor:iosurface:plane: (plane 0). ghostty's `orelse
        // return error.MetalFailed`.
        let texture = opts
            .device
            .newTextureWithDescriptor_iosurface_plane(&desc, &surface, 0)
            .ok_or(Error::MetalFailed)?;

        Ok(Self {
            surface,
            texture,
            width: opts.width,
            height: opts.height,
        })
    }

    // PORT NOTE (ARC): `deinit()` (`self.surface.deinit()` + `self.texture.release()`)
    // dissolves into `CFRetained`/`Retained` Drop. ghostty's `IOSurface.deinit` also
    // does `IOSurfaceSetPurgeable(.empty)` to eagerly unload before CF GC — a Goal-2
    // nicety, not behavior; `CFRetained`'s Drop CFReleases faithfully.

    /// Copy the surface's `width*height*4` BGRA bytes into a `Vec<u8>`, honoring
    /// `bytesPerRow` (the row stride may exceed `width*4`). Call AFTER the frame's
    /// command buffer has completed. (Golden-gate support; GPU_RD_BLUEPRINT §6 —
    /// rustty-new test-support code, NOT a ghostty port.)
    //
    // PORT NOTE: ghostty's own `IOSurface.lock` helper passes options `0`; this
    // readback uses `ReadOnly` (correct for a pure read — it avoids invalidating the
    // GPU copy). Both are valid; `ReadOnly` is chosen because this is rustty-new
    // golden-support code. Coherence: the golden runner is pinned to Apple Silicon
    // ([F5]) with `.Shared` storage → the IOSurface memory is CPU-coherent after
    // `waitUntilCompleted` (no synchronize blit needed; on Intel/managed storage a
    // `synchronizeResource` blit would be required — out of the offscreen scope).
    pub fn read_bgra(&self) -> Vec<u8> {
        let bpr = self.surface.bytes_per_row(); // IOSurfaceGetBytesPerRow
        let w4 = self.width * 4;
        let mut out = vec![0u8; self.width * self.height * 4];
        // SAFETY: ReadOnly lock/unlock with a null seed; `base_address` is valid
        // between lock/unlock; we read only `w4` bytes per row for `height` rows,
        // all within the surface allocation (`bpr >= w4`).
        unsafe {
            let _ = self
                .surface
                .lock(IOSurfaceLockOptions::ReadOnly, core::ptr::null_mut());
            let base = self.surface.base_address().as_ptr() as *const u8; // IOSurfaceGetBaseAddress
            for row in 0..self.height {
                core::ptr::copy_nonoverlapping(
                    base.add(row * bpr),
                    out.as_mut_ptr().add(row * w4),
                    w4,
                );
            }
            let _ = self
                .surface
                .unlock(IOSurfaceLockOptions::ReadOnly, core::ptr::null_mut());
        }
        out
    }
}

/// ghostty `foundation.Number.create(.int, &value)` → CFNumber(IntType). `value` is
/// a `c_int` (Zig `.int`).
fn cf_number_int(value: i32) -> Option<CFRetained<CFNumber>> {
    // SAFETY: `value_ptr` points at a live `i32` for the duration of the call
    // (CFNumberCreate copies the value); `IntType` matches an `i32`/`c_int`.
    unsafe {
        CFNumber::new(
            None,
            CFNumberType::IntType,
            core::ptr::addr_of!(value).cast(),
        )
    }
}
