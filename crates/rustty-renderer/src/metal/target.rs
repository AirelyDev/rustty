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
//! PORT NOTE (S118-F1, CLOSED): ghostty sets the surface's color space to
//! Display P3 (oracle `src/renderer/metal/Target.zig:45-60`; see the
//! `set_color_space` comment in `init` for the exact port mapping). `init` now
//! applies this unconditionally, including offscreen: the offscreen golden
//! reads RAW BGRA bytes, which ignore the IOSurface colorspace tag, so tagging
//! the surface cannot perturb the golden's pinned byte output. This was
//! previously a deferred windowing-host seam pending `objc2-core-graphics`;
//! that dependency is now present (see `Cargo.toml`).

use core::ffi::c_void;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_core_foundation::{
    kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks, CFDictionary, CFNumber,
    CFNumberType, CFRetained, CFString,
};
use objc2_core_graphics::{kCGColorSpaceDisplayP3, CGColorSpace};
use objc2_io_surface::{
    kIOSurfaceBytesPerElement, kIOSurfaceColorSpace, kIOSurfaceHeight, kIOSurfacePixelFormat,
    kIOSurfaceWidth, IOSurfaceLockOptions, IOSurfaceRef,
};
use objc2_metal::{MTLDevice, MTLTexture, MTLTextureDescriptor};

use crate::metal::api;

// const log = std.log.scoped(.metal);
// PORT NOTE (updated task-14c-1): the `log` facade IS wired for this crate now
// (`Cargo.toml`'s `log = { workspace = true }`; DECISION-logging.md §6r call
// shape 3), and `generic.rs`'s `macos_impl` is the worked example. This file's
// `log.*` sites are simply NOT RESTORED YET — the `metal` group's 14c stage owns them, and will
// add `const LOG_SCOPE: &str = "metal";` here alongside the first live
// emission. Until then each site stays a comment at its location. Scheduled
// work, not a missing dependency.
// (`Target.zig` has no `log` call site of its own — the scoped-log declaration
// is kept above for inventory parity.)

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
        // PORTS (S118-F1, closes the R5 deferred seam): oracle
        // `src/renderer/metal/Target.zig:45-60`
        //     const colorspace = try graphics.ColorSpace.createNamed(.displayP3);
        //     defer colorspace.release();
        //     ... .colorspace = colorspace ...            // (IOSurface.init property)
        //     if (properties.colorspace) |space| surface.setColorSpace(space);
        // where `setColorSpace` (`pkg/macos/iosurface/iosurface.zig:73-84`) =
        // CGColorSpaceCopyPropertyList + IOSurfaceSetValue(kIOSurfaceColorSpace).
        // `CGColorSpace::with_name` + `.property_list()` (objc2-core-graphics) map
        // `createNamed`/`CGColorSpaceCopyPropertyList` 1:1; the `IOSurfaceRef::set_value`
        // call below is applied to the already-created surface (ghostty applies it
        // inside `IOSurface.init`, equivalent since nothing reads the surface between
        // its creation and this call). Applied unconditionally: this only tags
        // CoreAnimation's on-screen compositing colorspace — the offscreen golden's
        // `read_bgra` reads raw BGRA bytes, which ignore the IOSurface colorspace tag,
        // so this cannot perturb the golden's pinned byte output.

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

        // `set_color_space` (S118-F1): `CGColorSpaceCreateWithName(kCGColorSpaceDisplayP3)`
        // + `CGColorSpaceCopyPropertyList` + `IOSurfaceSetValue(kIOSurfaceColorSpace, ..)`.
        // ghostty's `graphics.ColorSpace.createNamed`/`.release()` map to
        // `CGColorSpace::with_name`'s `CFRetained` (Drop CFReleases); its
        // `orelse return error.OutOfMemory` maps to `.ok_or(Error::MetalFailed)?`
        // (there is no untagged path — ghostty's `if (properties.colorspace) |space|`
        // is always-Some at this call site, since `Target.init` always passes
        // `colorspace` as `try graphics.ColorSpace.createNamed(.displayP3)`, never
        // `null`).
        // SAFETY: reading the `kCGColorSpaceDisplayP3` extern static; a valid
        // `&'static CFString` whose address `CGColorSpaceCreateWithName` reads.
        let displayp3 = unsafe { kCGColorSpaceDisplayP3 };
        let colorspace = CGColorSpace::with_name(Some(displayp3)).ok_or(Error::MetalFailed)?;
        // ghostty `CGColorSpaceCopyPropertyList` → `CGColorSpace::property_list`;
        // its `defer foundation.CFRelease(@constCast(serialized_colorspace))` maps
        // to the returned `CFRetained<CFType>`'s Drop.
        let plist = colorspace.property_list().ok_or(Error::MetalFailed)?;
        // SAFETY: `IOSurfaceSetValue` requires `value` be of the correct type for
        // `key`; `plist` is exactly `CGColorSpaceCopyPropertyList`'s output, the
        // type `kIOSurfaceColorSpace` expects (per `pkg/macos/iosurface/iosurface.zig`'s
        // `setColorSpace`, which passes the identical property list unmodified).
        unsafe {
            // SAFETY: reading the `kIOSurfaceColorSpace` extern static; a valid
            // `&'static CFString`.
            let key = kIOSurfaceColorSpace;
            surface.set_value(key, &plist);
        }

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

// =============================================================================
// S118-F1 — the target IOSurface must carry the Display P3 colorspace tag (the
// oracle `src/renderer/metal/Target.zig:45-60` `set_color_space`, mirrored by
// `pkg/macos`'s `IOSurface.init`/`setColorSpace`). Before the fix, `Target::init`
// never called `IOSurfaceSetValue(kIOSurfaceColorSpace, ..)`, so the surface
// carried no colorspace tag at all — this test reads the tag back via
// `IOSurfaceCopyValue`/`CGColorSpaceCreateWithPropertyList`/`CGColorSpaceCopyName`
// (the same round-trip `pkg/macos`'s `setColorSpace` writes) and fails on a null
// read, not on an unrelated harness failure (see the `device()` skip below).
// =============================================================================
#[cfg(all(test, target_os = "macos"))]
mod p3_colorspace_tests {
    use objc2_metal::MTLDevice;

    use super::*;
    use crate::metal::api;

    fn device() -> Option<Retained<ProtocolObject<dyn MTLDevice>>> {
        api::MTLCreateSystemDefaultDevice()
    }

    #[test]
    fn target_surface_is_tagged_display_p3() {
        // This test is the SOLE evidence backing the CLOSED S118-F1 ledger row
        // (.port/parity-closure/LEDGER.tsv). A silent skip-on-no-device (a bare
        // `return` from a #[test] fn reports as a pass) would make the ledger
        // row look verified when it isn't. This is `#[cfg(target_os = "macos")]`
        // code and the macOS CI lane runs on a real Metal-capable host, so a
        // `None` device here is an environment regression worth failing loudly
        // on, not hiding.
        let dev =
            device().expect("Metal device required: this test is the sole evidence for S118-F1");

        let t = Target::init(Options {
            device: &dev,
            width: 64,
            height: 64,
            pixel_format: api::MTLPixelFormat::BGRA8Unorm,
            storage_mode: api::StorageMode::Shared,
        })
        .expect("Target::init");

        // SAFETY: reading the `kIOSurfaceColorSpace` extern static; a valid
        // `&'static CFString` whose address IOSurfaceCopyValue reads.
        let key = unsafe { kIOSurfaceColorSpace };
        let value = t.surface.value(key);
        let Some(value) = value else {
            panic!("IOSurface has no colorspace tag (kIOSurfaceColorSpace unset)");
        };

        // SAFETY: `value` is the exact CFPropertyList `setColorSpace` wrote
        // (CGColorSpaceCopyPropertyList's output), so it is of the correct type
        // for CGColorSpaceCreateWithPropertyList.
        let cs = unsafe { CGColorSpace::with_property_list(&value) }
            .expect("CGColorSpaceCreateWithPropertyList");
        let name = CGColorSpace::name(Some(&cs)).expect("CGColorSpaceCopyName");
        assert_eq!(name.to_string(), "kCGColorSpaceDisplayP3");
    }
}
