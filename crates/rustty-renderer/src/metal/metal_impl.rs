//! Graphics API wrapper for Metal.
//!
//! PORTS: ghostty `src/renderer/Metal.zig` — the `Metal = @This()` graphics-API
//! wrapper that `GenericRenderer(Metal)` is monomorphized over. Per the GPU R-d
//! sub-blueprint (`GPU_RD_TRAIT_DESIGN.md`) the port is MONOMORPHIC: the concrete
//! `Metal` leaf, no `GraphicsApi` trait hierarchy.
//!
//! PORT NOTE (R3/objc2): ghostty's `pkg/objc` (`objc.Object` / manual message
//! sends / `objc.AutoreleasePool`) → `objc2` (`Retained`/`ProtocolObject`, ARC via
//! `Drop`, `objc2::ffi::objc_autoreleasePool{Push,Pop}`); `pkg/macos.graphics` (the
//! `graphics.Rect` layer-bounds read in `surfaceSize`) → the windowing-host seam
//! (deferred, needs `objc2-core-graphics`); `metal/api.zig` → `objc2-metal` under
//! ghostty's names (`super::api`); `metal/IOSurfaceLayer.zig` → the windowing-host
//! layer seam (not ported in the offscreen scope).
//!
//! PORT NOTE (R1 ledger, Metal.zig:21-30 — the GraphicsAPI self-alias + leaf-type
//! re-exports): ghostty declares
//!     pub const GraphicsAPI = Metal;                          // (line 21)
//!     pub const Target = @import("metal/Target.zig");         // (22)
//!     pub const Frame = @import("metal/Frame.zig");           // (23)
//!     pub const RenderPass = @import("metal/RenderPass.zig");  // (24)
//!     pub const Pipeline = @import("metal/Pipeline.zig");      // (25)
//!     const bufferpkg = @import("metal/buffer.zig");          // (26)
//!     pub const Buffer = bufferpkg.Buffer;                    // (27)
//!     pub const Sampler = @import("metal/Sampler.zig");        // (28)
//!     pub const Texture = @import("metal/Texture.zig");        // (29)
//!     pub const shaders = @import("metal/shaders.zig");        // (30)
//! In the MONOMORPHIC port `GraphicsAPI = Metal` is the identity self-alias (Chunk
//! 3's generic names `crate::metal::Metal` directly, so no alias is emitted); the
//! eight leaf re-exports route to the already-landed leaf modules
//! `super::{target::Target, frame::Frame, render_pass::RenderPass,
//! pipeline::Pipeline, buffer::Buffer, sampler::Sampler, texture::Texture,
//! shaders}` (Chunks 0/1a/1b) — reached via the `use super::…` imports below (and
//! `crate::metal::*` from the generic), rather than re-declared here. This comment
//! is the R1 every-line ledger for lines 21-30.
//!
//! PORT NOTE (self: Metal by-value → &self): Metal.zig's option factories take
//! `self: Metal` (a struct copy sharing ObjC refs with no retain). `Metal` holds
//! `Retained` (not `Copy`) in Rust, so every read-only method takes `&self` and
//! `.clone()`s the device into the owned-device Options (a refcount bump on a
//! short-lived Options — behavior-equivalent). `*const Metal` → `&self`,
//! `*Metal` → `&mut self`.
//!
//! DEFERRED (windowing-host seams M-1..M-5, offscreen scope): the `layer:
//! IOSurfaceLayer` field (M-1), `init(alloc, opts)`'s NSView/IOSurfaceLayer attach
//! half (M-2), `surfaceSize`'s layer-bounds read (M-3), `loopEnter`+`displayCallback`
//! (M-4), and `present`'s layer `setSurface` (M-5). Each is an R5-preserved
//! `unimplemented!("windowing-host")` or offscreen stub with the verbatim Zig body
//! kept as a comment. The offscreen entry point is `init_offscreen`.

use std::ffi::CStr;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLCommandQueue, MTLDevice, MTLGPUFamily};

use rustty_render_types::AlphaBlending; // ← Part A hoist (no config edge)

use super::api;
use super::buffer::BufferOptions;
use super::frame::{self, CompletionFn, Frame};
use super::iosurface_layer::{self, IOSurfaceLayer};
use super::sampler::SamplerOptions;
use super::shaders::{self, Shaders};
use super::target::{self, Target};
use super::texture::{self, Texture, TextureOptions};

// W5 (`W5_DESIGN §3.2`): `loop_enter`/`display_callback` name the monomorphic
// `Renderer` for the `offset_of!` container_of. This is an intra-crate
// back-reference to the type `Metal` is monomorphised under — allowed
// (`check_acyclic.sh` is crate-level; `Renderer` and `Metal` share `rustty-renderer`).
use crate::generic::Renderer;
use core::ffi::c_void;

// const std = @import("std");
// const assert = @import("../quirks.zig").inlineAssert;  → rustty_util::quirks::inlineAssert
// const Allocator = std.mem.Allocator;
// const builtin = @import("builtin");                    → #[cfg(target_os = "macos")]
// const log = std.log.scoped(.metal);
// PORT NOTE (task-14c-2): the `log` facade is wired for this crate
// (`Cargo.toml`'s `log = { workspace = true }`; DECISION-logging.md §6r call
// shape 3) and this file's `log.*` sites are RESTORED. Each keeps the
// reference's call as an R1 comment with the live emission beneath it.
//
/// `Metal.zig:39`'s `const log = std.log.scoped(.metal);` — the `@tagName` of the
/// reference's scope, carried as the `log` crate's `target`.
const LOG_SCOPE: &str = "metal";

/// Names the anonymous `struct { width: u32, height: u32 }` that `surfaceSize`
/// returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SurfaceSize {
    pub width: u32,
    pub height: u32,
}

/// Metal-local error. Ports `Metal.zig`'s `error{NoMetalDevice}` (the only Metal-
/// *originated* error; the resource constructors return their own leaf `Error`s,
/// matching Zig's per-call inferred unions). The aggregate `RendererError` that
/// `generic.zig` maps leaf errors into is a CHUNK-3 concern
/// (`GPU_RD_TRAIT_DESIGN.md` §A shared vocabulary), not introduced here.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// `Metal.zig` `error{NoMetalDevice}` — `chooseDevice` found no usable
    /// MTLDevice.
    NoMetalDevice,
    /// Ghostty `error{ObjCFailed}` — propagated from `IOSurfaceLayer.setSurface` /
    /// `IOSurfaceLayer.init` (main-thread objc dispatch or subclass alloc failure,
    /// `IOSurfaceLayer.zig:138-145`). Distinct from `NoMetalDevice` (present error
    /// set). `iosurface_layer::Error` is itself `enum Error { ObjCFailed }`
    /// (`iosurface_layer.rs:33-34`) ⇒ the fold is an identity map.
    ObjCFailed,
}

/// Graphics API wrapper for Metal. Ports `Metal.zig`'s `Metal = @This()` struct.
pub struct Metal {
    // Metal.zig:41 — `layer: IOSurfaceLayer,` (non-optional in ghostty, which always
    // has a host view). SANCTIONED DIVERGENCE (R1-documented): rustty added the
    // offscreen golden path (`init_offscreen`, no view) BEFORE the windowing tier, so
    // the minimal faithful encoding is `Option`: None == offscreen (present no-op,
    // surface_size == offscreen_size), Some == on-screen (layer attached to an NSView).
    // Placed first to match Metal.zig's field order (`layer` before `device`).
    pub layer: Option<IOSurfaceLayer>,
    /// MTLDevice
    pub device: Retained<ProtocolObject<dyn MTLDevice>>,
    /// MTLCommandQueue
    pub queue: Retained<ProtocolObject<dyn MTLCommandQueue>>,

    /// Alpha blending mode.
    // Metal.zig: `blending: configpkg.Config.AlphaBlending`. Now the Part-A hoisted
    // `rustty_render_types::AlphaBlending` (no config edge). Drives sRGB pixel-format
    // selection in init_shaders/init_target/texture_options via `blending.is_linear()`.
    pub blending: AlphaBlending,

    /// The default storage mode to use for resources created with our device.
    ///
    /// This is based on whether the device is a discrete GPU or not, since
    /// discrete GPUs do not have unified memory and therefore do not support
    /// the "shared" storage mode, instead we have to use the "managed" mode.
    pub default_storage_mode: api::StorageMode,

    /// The maximum 2D texture width and height supported by the device.
    pub max_texture_size: u32,

    /// We start an AutoreleasePool before `drawFrame` and end it afterwards.
    // Metal.zig: `autorelease_pool: ?*objc.AutoreleasePool = null`.
    // `objc.AutoreleasePool` wraps `objc_autoreleasePoolPush()`/`Pop()`; objc2
    // exposes those same two runtime fns as `objc2::ffi::objc_autoreleasePoolPush()
    // -> *mut c_void` / `objc_autoreleasePoolPop(ptr)`. The token is the pushed pool
    // pointer.
    autorelease_pool: Option<*mut core::ffi::c_void>,

    /// OFFSCREEN seam M-3: the offscreen surface size (replaces `layer.bounds` in
    /// `surfaceSize`). Set by `init_offscreen`, changed afterwards by
    /// `set_offscreen_size`; returned (clamped to `max_texture_size`) by
    /// `surface_size`.
    offscreen_size: SurfaceSize,

    /// PORT-ONLY (N437): the layer display callback's context, owned here
    /// (`Box::into_raw`, freed in `Drop` after the callback is cleared). Null until
    /// `loop_enter` installs the callback.
    display_ctx: *mut DisplayCallbackCtx,
}

/// PORT-ONLY (N437): what the CoreAnimation display callback needs, in its own
/// allocation: the whole-`Renderer` pointer (the reference's `ctx`) and a handle
/// to the renderer's `draw_mutex`, so the callback locks without reading the
/// renderer's bytes (see `crate::latest_frame`).
pub struct DisplayCallbackCtx {
    renderer: *mut Renderer,
    lock: crate::latest_frame::DrawMutex,
}

impl Metal {
    // Metal.zig: `pub const custom_shader_target: shadertoy.Target = .msl;`
    pub const CUSTOM_SHADER_TARGET: crate::shadertoy::Target = crate::shadertoy::Target::Msl;
    // The fragCoord for Metal shaders is +Y = down.
    // Metal.zig: `pub const custom_shader_y_is_down = true;`
    pub const CUSTOM_SHADER_Y_IS_DOWN: bool = true;
    /// Triple buffering.
    // Metal.zig: `pub const swap_chain_count = 3;`
    pub const SWAP_CHAIN_COUNT: usize = 3;
}

/// `Metal.zig` `fn chooseDevice() error{NoMetalDevice}!objc.Object`.
fn choose_device() -> Result<Retained<ProtocolObject<dyn MTLDevice>>, Error> {
    let mut chosen_device: Option<Retained<ProtocolObject<dyn MTLDevice>>> = None;

    // .macos arm (this module is `#[cfg(target_os = "macos")]`). The Zig `.ios`
    // arm (`MTLCreateSystemDefaultDevice`) and `else => @compileError` are dead
    // comptime arms preserved as comments (R1):
    //     .ios => chosen_device = objc.Object.fromId(mtl.MTLCreateSystemDefaultDevice());
    //     else => @compileError("unsupported target for Metal");
    // Zig `defer devices.release()` → ARC: the `Retained` array drops at end of
    // scope. Zig `var iter = devices.iterate(); while (iter.next()) |device|` maps
    // to indexed `count()`/`objectAtIndex()` (each returns an owned `Retained`).
    let devices = api::MTLCopyAllDevices(); // Retained<NSArray<ProtocolObject<dyn MTLDevice>>>
    let n = devices.count();
    for i in 0..n {
        // `objectAtIndex` returns Retained<ProtocolObject<dyn MTLDevice>>.
        let device = devices.objectAtIndex(i);
        // We want a GPU that's connected to a display.
        if device.isHeadless() {
            continue;
        }
        chosen_device = Some(device.clone());
        // If the user has an eGPU plugged in, they probably want
        // to use it. Otherwise, integrated GPUs are better for
        // battery life and thermals.
        if device.isRemovable() || device.isLowPower() {
            break;
        }
    }

    // Zig `const device = chosen_device orelse return error.NoMetalDevice; return
    // device.retain();` — the `Retained` already owns the +1 (objc2 ARC), so
    // returning it *is* the `retain()`; no explicit retain needed.
    chosen_device.ok_or(Error::NoMetalDevice)
}

/// Determines the maximum 2D texture size supported by the device.
/// We need to clamp our frame size to this if it's larger.
// Metal.zig `fn queryMaxTextureSize(device: objc.Object) u32`.
fn query_max_texture_size(device: &ProtocolObject<dyn MTLDevice>) -> u32 {
    // https://developer.apple.com/metal/Metal-Feature-Set-Tables.pdf
    if device.supportsFamily(MTLGPUFamily::Apple10) {
        return 32768;
    }
    if device.supportsFamily(MTLGPUFamily::Apple3) {
        return 16384;
    }
    8192
}

/// The four device-derived fields the offscreen-portable init prologue produces
/// (`init_device`): device, command queue, default storage mode, max texture size.
/// A named alias keeps the shared-prologue return type readable (clippy
/// `type_complexity`).
type DeviceParts = (
    Retained<ProtocolObject<dyn MTLDevice>>,
    Retained<ProtocolObject<dyn MTLCommandQueue>>,
    api::StorageMode,
    u32,
);

impl Metal {
    /// The offscreen-portable half of `Metal.zig` `init` (lines 72-88): chooseDevice,
    /// newCommandQueue, and device metadata (`default_storage_mode`,
    /// `max_texture_size`). Shared by `init_offscreen` (layer None) and `init`
    /// (windowed, adds the view attach) so neither duplicates it. Returns the four
    /// device-derived fields.
    fn init_device() -> Result<DeviceParts, Error> {
        // Choose our MTLDevice and create a MTLCommandQueue for that device.
        let device = choose_device()?;
        // errdefer device.release() — ARC via Retained Drop on the early return below.
        let queue = device.newCommandQueue().ok_or(Error::NoMetalDevice)?;
        // errdefer queue.release() — ARC via Retained Drop.
        // (Zig `newCommandQueue` cannot fail in practice; the `?*anyopaque` null
        // maps to None → NoMetalDevice.)

        // Grab metadata about the device.
        //   .ios => .shared;  else => hasUnifiedMemory ? .shared : .managed
        // (the `.ios` arm — manage mode is not supported by iOS — is a dead
        //  comptime arm preserved in this comment, R1.)
        let default_storage_mode = if device.hasUnifiedMemory() {
            api::StorageMode::Shared
        } else {
            api::StorageMode::Managed
        };
        let max_texture_size = query_max_texture_size(&device);
        // log.debug("device properties default_storage_mode={} max_texture_size={}",
        //           .{ default_storage_mode, max_texture_size });
        // (R1 rendering note: Zig's `{}` on an enum prints the fully qualified
        //  `<type>.<tag>`; the port has only a derived `Debug`, so this renders
        //  `Shared`/`Managed` where the reference renders
        //  `mtl.MTLResourceOptions.StorageMode.shared`. Known, accepted.)
        log::debug!(
            target: LOG_SCOPE,
            "device properties default_storage_mode={default_storage_mode:?} max_texture_size={max_texture_size}"
        );

        Ok((device, queue, default_storage_mode, max_texture_size))
    }

    /// OFFSCREEN constructor (CHUNK-2 golden/smoke entry point). Runs the
    /// offscreen-portable half of `Metal.zig`'s `init`: chooseDevice +
    /// newCommandQueue + device metadata, WITHOUT the NSView/IOSurfaceLayer half
    /// (windowing-host seam M-2). Takes `blending` + the fixed offscreen size
    /// directly, so CHUNK 2 needs neither `renderer::Options` nor `apprt`.
    pub fn init_offscreen(
        blending: AlphaBlending,
        width: u32,
        height: u32,
    ) -> Result<Metal, Error> {
        // comptime switch (builtin.os.tag) { .macos, .ios => {}, else => @compileError(...) };
        //   → the module cfg `#[cfg(target_os = "macos")]` is that comptime gate.

        let (device, queue, default_storage_mode, max_texture_size) = Self::init_device()?;

        // OFFSCREEN seam M-2: the reference here fetches the underlying view
        // (`opts.rt_surface.platform.nsview`) + scale factor and builds/attaches an
        // `IOSurfaceLayer` (wantsLayer / addSublayer / clipsToBounds / contentsScale /
        // needsDisplayOnBoundsChange). None of that is offscreen-portable, so the
        // offscreen path keeps the sanctioned no-layer divergence: `layer` is an
        // explicit `None` (present is the no-op arm, surface_size returns
        // `offscreen_size`). The on-screen constructor `Metal::init` performs the
        // attach — see its verbatim body.

        Ok(Metal {
            layer: None,
            device,
            queue,
            blending,
            default_storage_mode,
            max_texture_size,
            autorelease_pool: None,
            offscreen_size: SurfaceSize { width, height },
            display_ctx: core::ptr::null_mut(),
        })
    }

    /// DEFERRED (windowing-host seam M-2): the real `Metal.zig` `init(alloc, opts)`.
    ///
    /// Needs `opts.rt_surface.platform.nsview` (a live NSView) + `IOSurfaceLayer` +
    /// the wantsLayer/addSublayer/clipsToBounds/contentsScale/
    /// needsDisplayOnBoundsChange attach. None of that is offscreen-portable; it
    /// lands with the windowing-host tier. The offscreen ctor is `init_offscreen`.
    ///
    /// Verbatim Zig body (Metal.zig:64-156), preserved for R1/R5:
    // pub fn init(alloc: Allocator, opts: rendererpkg.Options) !Metal {
    //     comptime switch (builtin.os.tag) {
    //         .macos, .ios => {},
    //         else => @compileError("unsupported platform for Metal"),
    //     };
    //     _ = alloc;
    //     // Choose our MTLDevice and create a MTLCommandQueue for that device.
    //     const device = try chooseDevice();
    //     errdefer device.release();
    //     const queue = device.msgSend(objc.Object, objc.sel("newCommandQueue"), .{});
    //     errdefer queue.release();
    //     // Grab metadata about the device.
    //     const default_storage_mode: mtl.MTLResourceOptions.StorageMode = switch (comptime builtin.os.tag) {
    //         .ios => .shared,
    //         else => if (device.getProperty(bool, "hasUnifiedMemory")) .shared else .managed,
    //     };
    //     const max_texture_size = queryMaxTextureSize(device);
    //     log.debug("device properties default_storage_mode={} max_texture_size={}",
    //         .{ default_storage_mode, max_texture_size });
    //     const ViewInfo = struct { view: objc.Object, scaleFactor: f64 };
    //     // Get the metadata about our underlying view that we'll be rendering to.
    //     const info: ViewInfo = switch (apprt.runtime) {
    //         apprt.embedded => .{
    //             .scaleFactor = @floatCast(opts.rt_surface.content_scale.x),
    //             .view = switch (opts.rt_surface.platform) {
    //                 .macos => |v| v.nsview,
    //                 .ios => |v| v.uiview,
    //             },
    //         },
    //         else => @compileError("unsupported apprt for metal"),
    //     };
    //     // Create an IOSurfaceLayer which we can assign to the view to make
    //     // it in to a "layer-hosting view", so that we can manually control
    //     // the layer contents.
    //     var layer = try IOSurfaceLayer.init();
    //     errdefer layer.release();
    //     // Add our layer to the view.
    //     //
    //     // On macOS we do this by making the view "layer-hosting"
    //     // by assigning it to the view's `layer` property BEFORE
    //     // setting `wantsLayer` to `true`.
    //     //
    //     // On iOS, views are always layer-backed, and `layer`
    //     // is readonly, so instead we add it as a sublayer.
    //     switch (comptime builtin.os.tag) {
    //         .macos => {
    //             info.view.setProperty("layer", layer.layer.value);
    //             info.view.setProperty("wantsLayer", true);
    //         },
    //         .ios => {
    //             const view_layer = objc.Object.fromId(info.view.getProperty(?*anyopaque, "layer"));
    //             view_layer.msgSend(void, objc.sel("addSublayer:"), .{layer.layer.value});
    //         },
    //         else => @compileError("unsupported target for Metal"),
    //     }
    //     // Ensure that if our layer is oversized it
    //     // does not overflow the bounds of the view.
    //     info.view.setProperty("clipsToBounds", true);
    //     // Ensure that our layer has a content scale set to
    //     // match the scale factor of the window. This avoids
    //     // magnification issues leading to blurry rendering.
    //     layer.layer.setProperty("contentsScale", info.scaleFactor);
    //     // This makes it so that our display callback will actually be called.
    //     layer.layer.setProperty("needsDisplayOnBoundsChange", true);
    //     return .{
    //         .layer = layer,
    //         .device = device,
    //         .queue = queue,
    //         .blending = opts.config.blending,
    //         .default_storage_mode = default_storage_mode,
    //         .max_texture_size = max_texture_size,
    //     };
    // }
    /// `Metal.zig` `init(alloc, opts)` — the on-screen constructor. `view` is the
    /// host NSView/UIView (ghostty `info.view`), `scale_factor` is
    /// `opts.rt_surface.content_scale.x` (ghostty `info.scaleFactor`), `blending`
    /// maps ghostty's `opts.config.blending`. `view` is type-erased
    /// (`&objc2::runtime::AnyObject`, byte-faithful to `info.view: objc.Object`) to
    /// keep R4 (no app-kit dep). See the verbatim Zig body above.
    pub fn init(
        view: &objc2::runtime::AnyObject,
        scale_factor: f64,
        blending: AlphaBlending,
    ) -> Result<Metal, Error> {
        let (device, queue, default_storage_mode, max_texture_size) = Self::init_device()?;

        // Create an IOSurfaceLayer which we can assign to the view to make it in to a
        // "layer-hosting view", so that we can manually control the layer contents.
        // `IOSurfaceLayer::init` returns `iosurface_layer::Error::ObjCFailed` ⇒ fold
        // to the dedicated `Error::ObjCFailed` (NOT NoMetalDevice; W1-3 present error
        // set). errdefer layer.release() → ARC via Retained Drop on `?` early return.
        let layer = IOSurfaceLayer::init()
            .map_err(|iosurface_layer::Error::ObjCFailed| Error::ObjCFailed)?;

        // Add our layer to the view.
        //
        // On macOS we do this by making the view "layer-hosting" by assigning it to
        // the view's `layer` property BEFORE setting `wantsLayer` to `true` (the
        // ORDER is load-bearing). The Zig `.ios` arm
        // (`view_layer.addSublayer:(layer.layer.value)`, Metal.zig:128-131) is a dead
        // comptime arm preserved as a comment (R1).
        let l: *const objc2_quartz_core::CALayer = layer.as_ca_layer();
        // SAFETY: `view` is a live NSView; `setLayer:`/`setWantsLayer:` are the
        // standard layer-hosting AppKit selectors, reached via `msg_send!` over the
        // type-erased `&AnyObject` (no app-kit dep, R4).
        unsafe {
            let _: () = objc2::msg_send![view, setLayer: l];
            let _: () = objc2::msg_send![view, setWantsLayer: true];
        }
        // Ensure that if our layer is oversized it does not overflow the bounds of
        // the view.
        // SAFETY: `setClipsToBounds:` on the live NSView.
        unsafe {
            let _: () = objc2::msg_send![view, setClipsToBounds: true];
        }
        // Ensure that our layer has a content scale set to match the scale factor of
        // the window. This avoids magnification issues leading to blurry rendering.
        layer.as_ca_layer().setContentsScale(scale_factor);
        // This makes it so that our display callback will actually be called.
        layer.as_ca_layer().setNeedsDisplayOnBoundsChange(true);

        Ok(Metal {
            layer: Some(layer),
            device,
            queue,
            blending,
            default_storage_mode,
            max_texture_size,
            autorelease_pool: None,
            // OFFSCREEN seam M-3 no longer drives surface_size on-screen (the layer
            // branch, W1-3); keep a zeroed offscreen_size — it is never read while
            // `layer.is_some()`.
            offscreen_size: SurfaceSize {
                width: 0,
                height: 0,
            },
            display_ctx: core::ptr::null_mut(),
        })
    }

    /// W-ATTACH (macos-swift-app tier): attach an already-offscreen `Metal` to a
    /// live host NSView, flipping `self.layer` from `None` (offscreen) to
    /// `Some(IOSurfaceLayer)` (on-screen). This is the layer-hosting attach half
    /// of `Metal::init` (`Metal.zig:110-126`) applied to the existing offscreen
    /// device/queue/shaders — used because the live renderer is built via
    /// `init_offscreen` (the apprt-free `Renderer::init`, generic.rs), then handed
    /// its host view at `finalizeSurfaceInit`. Keeps device/queue; only creates +
    /// attaches the layer. MUST run on the main thread, BEFORE the render thread's
    /// `loop_enter` (so the display callback installs against `Some(layer)`).
    pub fn attach_view(
        &mut self,
        view: &objc2::runtime::AnyObject,
        scale_factor: f64,
    ) -> Result<(), Error> {
        let layer = IOSurfaceLayer::init()
            .map_err(|iosurface_layer::Error::ObjCFailed| Error::ObjCFailed)?;
        let l: *const objc2_quartz_core::CALayer = layer.as_ca_layer();
        // SAFETY: `view` is a live NSView; layer-hosting via `setLayer:` BEFORE
        // `setWantsLayer:true` (order load-bearing), reached over the type-erased
        // `&AnyObject` (no app-kit dep, R4). Identical to `Metal::init`'s attach.
        unsafe {
            let _: () = objc2::msg_send![view, setLayer: l];
            let _: () = objc2::msg_send![view, setWantsLayer: true];
            let _: () = objc2::msg_send![view, setClipsToBounds: true];
        }
        layer.as_ca_layer().setContentsScale(scale_factor);
        layer.as_ca_layer().setNeedsDisplayOnBoundsChange(true);
        self.layer = Some(layer);
        Ok(())
    }

    /// Actions taken before doing anything in `drawFrame`.
    ///
    /// Right now we use this to start an AutoreleasePool.
    // pub fn drawFrameStart(self: *Metal) void
    pub fn draw_frame_start(&mut self) {
        // Metal.zig: `assert(self.autorelease_pool == null);`
        rustty_util::quirks::inlineAssert(self.autorelease_pool.is_none());
        // Zig `self.autorelease_pool = .init()` == objc_autoreleasePoolPush().
        // SAFETY: objc2's ObjC runtime autorelease-pool push; the returned token is
        // paired with the pop in `draw_frame_end` (balance asserted above/there,
        // matching Zig).
        self.autorelease_pool = Some(unsafe { objc2::ffi::objc_autoreleasePoolPush() });
    }

    /// Actions taken after `drawFrame` is done.
    ///
    /// Right now we use this to end our AutoreleasePool.
    // pub fn drawFrameEnd(self: *Metal) void
    pub fn draw_frame_end(&mut self) {
        // Metal.zig: `assert(self.autorelease_pool != null);`
        rustty_util::quirks::inlineAssert(self.autorelease_pool.is_some());
        // Zig `self.autorelease_pool.?.deinit(); self.autorelease_pool = null;` == pop.
        let pool = self.autorelease_pool.take().unwrap();
        // SAFETY: `pool` is the exact token returned by the matching
        // `objc_autoreleasePoolPush` in `draw_frame_start` (we just `take`-cleared
        // the field), popped exactly once.
        unsafe { objc2::ffi::objc_autoreleasePoolPop(pool) };
    }

    /// DEFERRED (windowing-host seam M-4). Metal is the ONLY backend that declares
    /// `loopEnter` (the other 7 apprt/thread `@hasDecl` hooks are no-ops); it
    /// installs the layer display callback.
    // pub fn loopEnter(self: *Metal) void {
    //     const renderer: *align(1) Renderer = @fieldParentPtr("api", self);
    //     self.layer.setDisplayCallback(
    //         @ptrCast(&displayCallback),
    //         @ptrCast(renderer),
    //     );
    // }
    // fn displayCallback(renderer: *Renderer) align(8) void {
    //     renderer.drawFrame(true) catch |err| {
    //         log.warn("Error drawing frame in display callback, err={}", .{err});
    //     };
    // }
    //
    /// W5 (`W5_DESIGN §3.2`): the seam is filled. `loopEnter`'s receiver is a raw
    /// `*Metal` and it recovers the whole `*Renderer` via `@fieldParentPtr` — so the
    /// port takes `self_ptr: *mut Metal` (not `&mut self`); see the signature note
    /// in `W5_DESIGN §3.2`. Its sole call site is `Renderer::loop_enter`, which hands
    /// it a whole-`Renderer`-provenance `*mut Metal` (`W5_DESIGN §2.4`/§3.1).
    ///
    /// N437: `lock` is a handle to the renderer's `draw_mutex`; the callback's
    /// context carries it so the main thread's draw locks without reading the
    /// renderer. The caller holds `draw_mutex`.
    ///
    /// # Safety
    /// `self_ptr` must be a whole-`Renderer`-provenance `*mut Metal` pointing at the
    /// `Renderer.api` field, projected from the renderer's raw root rather than
    /// from a reborrow (established by `Renderer::loop_enter_raw`, §3.1).
    pub unsafe fn loop_enter(self_ptr: *mut Metal, lock: crate::latest_frame::DrawMutex) {
        // const renderer: *align(1) Renderer = @fieldParentPtr("api", self);
        // container_of: Metal is Renderer.api (faithful @fieldParentPtr). byte_sub
        // keeps the whole-Renderer provenance of `self_ptr` (`W5_DESIGN §2.4`).
        let renderer: *mut Renderer = unsafe {
            (self_ptr as *mut u8)
                .byte_sub(core::mem::offset_of!(Renderer, api))
                .cast::<Renderer>()
        };
        // OFFSCREEN divergence: `layer` is `Option`. On-screen (loopEnter's only real
        // caller) it is always `Some`; a defensive `if let` keeps the offscreen ctor
        // safe.
        if let Some(layer) = unsafe { (*self_ptr).layer.as_ref() } {
            // self.layer.setDisplayCallback(@ptrCast(&displayCallback), @ptrCast(renderer));
            //
            // PORT NOTE (N437): the context is a `DisplayCallbackCtx` carrying
            // `renderer` and the `draw_mutex` handle. A second `loopEnter` replaces
            // the callback before freeing the old context.
            let ctx = Box::into_raw(Box::new(DisplayCallbackCtx { renderer, lock }));
            layer.set_display_callback(Some(display_callback), ctx as *mut c_void);
            let old = unsafe { core::mem::replace(&mut (*self_ptr).display_ctx, ctx) };
            if !old.is_null() {
                // SAFETY: `old` came from `Box::into_raw` above and the layer no
                // longer holds it.
                drop(unsafe { Box::from_raw(old) });
            }
        }
    }

    // The other apprt/thread hooks (`surfaceInit`/`finalizeSurfaceInit`/
    // `threadEnter`/`threadExit`/`loopExit`/`displayRealized`/`displayUnrealized`)
    // are NOT declared on `Metal.zig` → in the monomorphic port they simply do not
    // exist as methods; `generic.zig`'s `@hasDecl` probes for them resolve to the
    // no-op default at the call site (a Chunk-3 concern).

    /// Returns the options to use when constructing buffers.
    // pub inline fn bufferOptions(self: Metal) bufferpkg.Options
    fn buffer_options(&self) -> BufferOptions {
        BufferOptions {
            device: self.device.clone(),
            resource_options: api::MTLResourceOptions {
                // Indicate that the CPU writes to this resource but never reads it.
                cpu_cache_mode: api::CPUCacheMode::WriteCombined,
                storage_mode: self.default_storage_mode,
                hazard_tracking_mode: api::HazardTrackingMode::Default, // Zig field-default `.default`
            },
        }
    }

    // The six Zig `pub const … = bufferOptions;` aliases (Metal.zig:278-283) → six
    // forwarders (PORT NOTE: Rust cannot alias methods by `pub const`,
    // `GPU_RD_TRAIT_DESIGN.md` §C). Names MUST match what generic.zig/image.zig call.
    pub fn instance_buffer_options(&self) -> BufferOptions {
        self.buffer_options()
    }
    pub fn uniform_buffer_options(&self) -> BufferOptions {
        self.buffer_options()
    }
    pub fn fg_buffer_options(&self) -> BufferOptions {
        self.buffer_options()
    }
    pub fn bg_buffer_options(&self) -> BufferOptions {
        self.buffer_options()
    }
    pub fn image_buffer_options(&self) -> BufferOptions {
        self.buffer_options()
    }
    pub fn bg_image_buffer_options(&self) -> BufferOptions {
        self.buffer_options()
    }

    /// Returns the options to use when constructing textures.
    // pub inline fn textureOptions(self: Metal) Texture.Options
    pub fn texture_options(&self) -> TextureOptions {
        TextureOptions {
            device: self.device.clone(),
            // Using an `*_srgb` pixel format makes Metal gamma encode the pixels
            // written to it *after* blending, which means we get linear alpha
            // blending rather than gamma-incorrect blending.
            pixel_format: if self.blending.is_linear() {
                api::MTLPixelFormat::BGRA8Unorm_sRGB
            } else {
                api::MTLPixelFormat::BGRA8Unorm
            },
            resource_options: api::MTLResourceOptions {
                // Indicate that the CPU writes to this resource but never reads it.
                cpu_cache_mode: api::CPUCacheMode::WriteCombined,
                storage_mode: self.default_storage_mode,
                hazard_tracking_mode: api::HazardTrackingMode::Default,
            },
            usage: api::MTLTextureUsage {
                // textureOptions is currently only used for custom shaders, which
                // require both the shader read (for when multiple shaders are
                // chained) and render target (for the final output) usage.
                // Disabling either of these will lead to metal validation errors
                // in Xcode.
                shader_read: true,
                render_target: true,
                ..api::MTLTextureUsage::default()
            },
        }
    }

    // pub inline fn samplerOptions(self: Metal) Sampler.Options
    pub fn sampler_options(&self) -> SamplerOptions {
        SamplerOptions {
            device: self.device.clone(),
            // These parameters match Shadertoy behaviors.
            min_filter: api::MTLSamplerMinMagFilter::Linear,
            mag_filter: api::MTLSamplerMinMagFilter::Linear,
            s_address_mode: api::MTLSamplerAddressMode::ClampToEdge,
            t_address_mode: api::MTLSamplerAddressMode::ClampToEdge,
        }
    }

    /// Returns the options to use when constructing textures for images.
    // pub inline fn imageTextureOptions(self, format, srgb) Texture.Options
    pub fn image_texture_options(&self, format: ImageTextureFormat, srgb: bool) -> TextureOptions {
        TextureOptions {
            device: self.device.clone(),
            pixel_format: format.to_pixel_format(srgb),
            resource_options: api::MTLResourceOptions {
                // Indicate that the CPU writes to this resource but never reads it.
                cpu_cache_mode: api::CPUCacheMode::WriteCombined,
                storage_mode: self.default_storage_mode,
                hazard_tracking_mode: api::HazardTrackingMode::Default,
            },
            usage: api::MTLTextureUsage {
                // We only need to read from this texture from a shader.
                shader_read: true,
                ..api::MTLTextureUsage::default()
            },
        }
    }

    /// Initializes a Texture suitable for the provided font atlas.
    // pub fn initAtlasTexture(self: *const Metal, atlas: *const font.Atlas) Texture.Error!Texture
    //
    // PORT NOTE: `rustty_font::atlas::Atlas` exposes `format`/`size` as accessor
    // methods (`atlas.format()`/`atlas.size()`) — the Zig public fields are
    // narrowed to `pub(crate)`/private in the font crate, so the renderer reads
    // them through the accessors. `atlas.size` is `u32`; `Texture::init` takes
    // `usize`, so it is widened with `as usize` (Zig widens u32→usize implicitly).
    pub fn init_atlas_texture(
        &self,
        atlas: &rustty_font::atlas::Atlas,
    ) -> Result<Texture, texture::Error> {
        let pixel_format = match atlas.format() {
            rustty_font::atlas::Format::Grayscale => api::MTLPixelFormat::R8Unorm,
            rustty_font::atlas::Format::Bgra => api::MTLPixelFormat::BGRA8Unorm_sRGB,
            // Zig `else => @panic("unsupported atlas format for Metal texture")` —
            // catches `.bgr` (Format::Bgr).
            _ => panic!("unsupported atlas format for Metal texture"),
        };

        Texture::init(
            TextureOptions {
                device: self.device.clone(),
                pixel_format,
                resource_options: api::MTLResourceOptions {
                    // Indicate that the CPU writes to this resource but never reads it.
                    cpu_cache_mode: api::CPUCacheMode::WriteCombined,
                    storage_mode: self.default_storage_mode,
                    hazard_tracking_mode: api::HazardTrackingMode::Default,
                },
                usage: api::MTLTextureUsage {
                    // We only need to read from this texture from a shader.
                    shader_read: true,
                    ..api::MTLTextureUsage::default()
                },
            },
            atlas.size() as usize,
            atlas.size() as usize,
            None,
        )
    }

    /// Initialize a new render target which can be presented by this API.
    // pub fn initTarget(self: *const Metal, width: usize, height: usize) !Target
    pub fn init_target(&self, width: usize, height: usize) -> Result<Target, target::Error> {
        Target::init(target::Options {
            // deref-coerces &Retained<..> → &ProtocolObject<..> (struct-field site)
            device: &self.device,
            // Using an `*_srgb` pixel format makes Metal gamma encode the pixels
            // written to it *after* blending, which means we get linear alpha
            // blending rather than gamma-incorrect blending.
            pixel_format: if self.blending.is_linear() {
                api::MTLPixelFormat::BGRA8Unorm_sRGB
            } else {
                api::MTLPixelFormat::BGRA8Unorm
            },
            storage_mode: self.default_storage_mode,
            width,
            height,
        })
    }

    /// Initialize our shader set (uses `blending.is_linear()` → sRGB pixel format
    /// for the shader pipelines).
    // pub fn initShaders(self: *const Metal, alloc, custom_shaders: []const [:0]const u8) !Shaders
    //
    // PORT NOTE: the reference `alloc` param is ELIDED — the already-landed
    // `Shaders::init` leaf (`metal/shaders.rs`) does not thread an Allocator
    // (objc2/Vec-managed), matching how the leaf was ported in Chunk 1b.
    // `custom_shaders: []const [:0]const u8` → `&[&CStr]`.
    pub fn init_shaders(&self, custom_shaders: &[&CStr]) -> Result<Shaders, shaders::Error> {
        Shaders::init(
            &self.device,
            custom_shaders,
            // Using an `*_srgb` pixel format makes Metal gamma encode
            // the pixels written to it *after* blending, which means
            // we get linear alpha blending rather than gamma-incorrect
            // blending.
            if self.blending.is_linear() {
                api::MTLPixelFormat::BGRA8Unorm_sRGB
            } else {
                api::MTLPixelFormat::BGRA8Unorm
            },
        )
    }

    /// Get the current size of the runtime surface.
    // pub fn surfaceSize(self: *const Metal) !struct { width: u32, height: u32 }
    //
    // seam M-3: the reference reads `layer.bounds * contentsScale`. The on-screen
    // `Some(layer)` branch is now LIVE (`objc2_core_foundation::CGRect`/`CGFloat`
    // replace `pkg/macos.graphics.Rect`/`f64`). Offscreen (`None`) there is no layer,
    // so we return the fixed `offscreen_size` set at construction. Both branches
    // clamp to `max_texture_size` (the reference's clamp: we can't create a screen
    // buffer (texture) larger than that). Verbatim Zig body (Metal.zig:216-232):
    //     const bounds = self.layer.layer.getProperty(graphics.Rect, "bounds");
    //     const scale = self.layer.layer.getProperty(f64, "contentsScale");
    //     // We need to clamp our runtime surface size to the maximum
    //     // possible texture size since we can't create a screen buffer (texture)
    //     // larger than that.
    //     return .{
    //         .width = @min(@as(u32, @intFromFloat(bounds.size.width * scale)), self.max_texture_size),
    //         .height = @min(@as(u32, @intFromFloat(bounds.size.height * scale)), self.max_texture_size),
    //     };
    pub fn surface_size(&self) -> Result<SurfaceSize, Error> {
        match &self.layer {
            Some(layer) => {
                // bounds = layer.bounds; scale = layer.contentsScale (Metal.zig:217-218).
                let bounds = layer.as_ca_layer().bounds(); // CGRect
                let scale = layer.as_ca_layer().contentsScale(); // CGFloat (f64)
                                                                 // @intFromFloat → `as u32` (trunc-toward-zero, byte-faithful); clamp each axis.
                Ok(SurfaceSize {
                    width: ((bounds.size.width * scale) as u32).min(self.max_texture_size),
                    height: ((bounds.size.height * scale) as u32).min(self.max_texture_size),
                })
            }
            None => Ok(SurfaceSize {
                width: self.offscreen_size.width.min(self.max_texture_size),
                height: self.offscreen_size.height.min(self.max_texture_size),
            }),
        }
    }

    /// PORT NOTE (rustty-only glue, additive): there is no reference counterpart.
    /// Ghostty has no offscreen product mode at all — `offscreen_size` is rustty's
    /// own OFFSCREEN seam M-3 (see the crate root's "R-d OFFSCREEN slice"), and the
    /// reference's only size source is `layer.bounds * contentsScale`, which an
    /// attached view resizes for it. An embedder that composites the finished
    /// IOSurface itself has no layer whose bounds could drive `surface_size`, so
    /// without this setter its surface is frozen at the size `init_offscreen` was
    /// given and can never follow the pane it is drawn into. Purely additive: not
    /// one byte of the on-screen (`layer.is_some()`) lane changes, and while a layer
    /// is attached `offscreen_size` stays unread (`surface_size`'s `Some` branch).
    ///
    /// `draw_frame` re-reads `api.surface_size()` every frame, so the next frame
    /// sees the new size, takes its `size_changed` branch (`generic.rs`:6356,
    /// :6475-6483 — `size.screen` + `update_screen_size_uniforms`) and replaces the
    /// swap-chain target through `FrameState::resize` (:6502-6516). Nothing here
    /// touches the GPU; the caller serializes against a concurrent draw
    /// (`Renderer::set_offscreen_size` takes `draw_mutex`).
    pub fn set_offscreen_size(&mut self, width: u32, height: u32) {
        self.offscreen_size = SurfaceSize { width, height };
    }

    /// Begin a frame.
    // pub inline fn beginFrame(self: *const Metal, renderer: *Renderer, target: *Target) !Frame
    //
    // PORT NOTE: the reference `renderer: *Renderer` (= `GenericRenderer(Metal)`) is
    // not ported in Chunk 2; `Frame::begin` (landed 1b) takes the type-erased
    // `renderer: *mut ()` PLUS an `on_completed: CompletionFn` (the type-erased
    // `&bufferCompleted`, which Chunk-3 `drawFrame` supplies). So `begin_frame`
    // forwards both. Raw `*mut` (NOT `&mut`) per the container_of /
    // `rust-mut-args-strongly-protected` aliasing model ([FIX-2]).
    //
    // Reference doc-comments on the params (R1):
    //   renderer — Once the frame has been completed, the `frameCompleted` method
    //              on the renderer is called with the health status of the frame.
    //   target   — The target is presented via the provided renderer's API when
    //              completed.
    pub fn begin_frame(
        &self,
        renderer: *mut (),
        target: *mut Target,
        on_completed: CompletionFn,
    ) -> Result<Frame, frame::Error> {
        Frame::begin(
            frame::Options { queue: &self.queue },
            renderer,
            target,
            on_completed,
        )
    }

    /// Warm up the Metal device machinery. The first Metal device query in
    /// a process takes multiple milliseconds; once warm, subsequent queries
    /// are effectively free. Calling this early (e.g. on a background
    /// thread at app startup; Metal device queries are thread-safe) moves
    /// that one-time cost off the critical path of the first surface's
    /// renderer initialization.
    //
    // pub fn warmup() void {
    //
    // (`c454a3bf4`..`931a538a3` warmup chain, END STATE — the intermediate
    // `threadedWarmup` shape was dissolved and is NOT ported.)
    pub fn warmup() {
        // const device = chooseDevice() catch return;
        // defer device.release();
        //
        // (ARC: the `Retained` device drops at end of scope.)
        let Ok(device) = choose_device() else { return };

        // Create and release a command queue. The first command queue
        // created for a device pays additional one-time driver setup
        // costs; subsequent creations are much cheaper.
        // const queue = device.msgSend(objc.Object, objc.sel("newCommandQueue"), .{});
        // queue.release();
        let queue = device.newCommandQueue();
        drop(queue);

        // Build and discard our shader pipelines for both pixel formats we
        // may use (which one is used depends on the blending config). The
        // first pipeline state creation compiles shaders which is slow;
        // once warm, later creations hit driver and OS caches.
        //
        // (Zig `inline for` over the two comptime formats -> a plain loop;
        // `shaders.Shaders.init(std.heap.c_allocator, ..)`'s allocator param
        // dissolved in the port — the shaders.rs `init` PORT NOTE.)
        for format in [
            api::MTLPixelFormat::BGRA8Unorm_sRGB,
            api::MTLPixelFormat::BGRA8Unorm,
        ] {
            match Shaders::init(&device, &[], format) {
                // var s_mut = s;
                // s_mut.deinit(std.heap.c_allocator);
                Ok(s) => {
                    let mut s_mut = s;
                    s_mut.deinit();
                }
                Err(err) => {
                    // log.warn("metal warmup shader init failed err={}", .{err});
                    log::warn!(target: LOG_SCOPE, "metal warmup shader init failed err={err:?}");
                }
            }
        }
    }

    /// Present the provided target.
    // pub inline fn present(self: *Metal, target: Target, sync: bool) !void
    //
    // seam M-5: the on-screen `Some(layer)` branch is now LIVE — it calls
    // `layer.setSurfaceSync(target.surface)` / `try layer.setSurface(...)`
    // (Metal.zig:253-259). Offscreen (`None`) has no layer; the IOSurface is read
    // back via `Target::read_bgra` (landed 1b) instead, so `present` is the no-op arm
    // offscreen (the offscreen completion path reads back rather than presenting, so
    // that arm is never exercised on-screen). `iosurface_layer::Error::ObjCFailed`
    // folds into `Error::ObjCFailed` (identity — faithful to ghostty's `try
    // setSurface` propagating `error{ObjCFailed}`). Verbatim Zig body:
    //     if (sync) {
    //         self.layer.setSurfaceSync(target.surface);
    //     } else {
    //         try self.layer.setSurface(target.surface);
    //     }
    pub fn present(&mut self, target: &Target, sync: bool) -> Result<(), Error> {
        match &self.layer {
            Some(layer) => {
                if sync {
                    layer.set_surface_sync(&target.surface);
                } else {
                    // try self.layer.setSurface(target.surface);  (Metal.zig:257)
                    layer
                        .set_surface(&target.surface)
                        .map_err(|iosurface_layer::Error::ObjCFailed| Error::ObjCFailed)?;
                }
                Ok(())
            }
            // OFFSCREEN (layer None): no layer to set; the completion path reads back
            // via `Target::read_bgra` instead. Preserved no-op (never exercised
            // on-screen).
            None => Ok(()),
        }
    }

    // P7-W7 T2 (`2f0b65346`): `presentLastTarget` (a no-op for Metal) is deleted
    // with the generic renderer's call to it.
}

// fn displayCallback(renderer: *Renderer) align(8) void {
//     renderer.drawFrame(true) catch |err| {
//         log.warn("Error drawing frame in display callback, err={}", .{err});
//     };
// }
//
// The ObjC `display` method (main thread, CA commit driven by
// `needsDisplayOnBoundsChange`) fires this via the layer's stored callback. It
// recovers the raw `*mut Renderer` ctx and calls the RAW-self draw path
// (`draw_frame_raw`) — NEVER `(*renderer).draw_frame(true)`, which would form a
// protected whole-`Renderer` `&mut` and race the render thread's own draw
// (`W5_DESIGN §2.2`/§2.3). The `align(8)` in Zig is satisfied by the Rust fn-item
// ABI; the signature matches `DisplayCallback = Option<unsafe extern "C"
// fn(*mut c_void)>`.
//
// PORT NOTE (VALIDATION_REPORT §3c.3, panic policy): this fires INSIDE a Core
// Animation commit (ObjC frames on the stack). A Rust panic must never unwind
// into them — that is UB — and the Zig reference's failure mode here is an
// abort (Zig panics do not unwind). So the body is wrapped in `abort_on_unwind`
// (catch_unwind + abort) and the ABI is plain `extern "C"` (whose boundary also
// converts any escaping panic to an abort — two independent layers enforcing
// the same Zig contract).
//
/// # Safety
/// `ctx` must be the `DisplayCallbackCtx` installed by `loop_enter`, whose
/// `renderer` is the whole-`Renderer`-provenance pointer (round-tripped through
/// `Cell<*mut c_void>`, so provenance is preserved).
unsafe extern "C" fn display_callback(ctx: *mut c_void) {
    abort_on_unwind(|| {
        // SAFETY: the context outlives the installed callback (`Metal`'s `Drop`
        // clears the callback before freeing it).
        let ctx = unsafe { &*ctx.cast::<DisplayCallbackCtx>() };
        if let Err(err) = unsafe { Renderer::draw_frame_raw(ctx.renderer, &ctx.lock, true) } {
            // log.warn("Error drawing frame in display callback, err={}", .{err});
            log::warn!(target: LOG_SCOPE, "Error drawing frame in display callback, err={err:?}");
        }
    })
}

/// Run `f`, aborting the process if it panics (never unwinding past this frame).
///
/// PORT NOTE (VALIDATION_REPORT §3c.3): the Rust spelling of the Zig panic
/// contract for callbacks invoked from foreign (ObjC/Core Animation) frames — a
/// Zig panic aborts, it never unwinds into its C caller. `AssertUnwindSafe` is
/// sound because the process aborts on the Err arm (no state is ever observed
/// after a caught panic). Proven by `abort_on_unwind_aborts_process` (subprocess
/// SIGABRT KAT).
fn abort_on_unwind<R>(f: impl FnOnce() -> R) -> R {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
        Ok(r) => r,
        Err(_) => std::process::abort(),
    }
}

impl Drop for Metal {
    // pub fn deinit(self: *Metal) void {
    //     self.queue.release();
    //     self.device.release();
    //     self.layer.release();
    // }
    fn drop(&mut self) {
        // PORT-ONLY (N437): clear the display callback, then free its context.
        if !self.display_ctx.is_null() {
            if let Some(layer) = self.layer.as_ref() {
                layer.set_display_callback(None, core::ptr::null_mut());
            }
            // SAFETY: `display_ctx` came from `Box::into_raw` in `loop_enter` and the
            // layer no longer holds it.
            drop(unsafe { Box::from_raw(self.display_ctx) });
            self.display_ctx = core::ptr::null_mut();
        }

        // ARC: `queue`/`device` (Retained) release automatically on drop; `layer`
        // (the `Option<IOSurfaceLayer>`) releases via `IOSurfaceLayer`'s
        // `Retained<CALayer>` Drop — `self.layer.release()` (Metal.zig:161) ⇒ that
        // Drop. Release order is immaterial under ARC. If a pool is still pushed here
        // it would leak — but drawFrameStart/End are balanced (matching Zig, whose
        // deinit also does not pop the pool). No explicit body required.
    }
}

/// Pixel format for image texture options.
// pub const ImageTextureFormat = enum { gray, rgba, bgra };
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ImageTextureFormat {
    /// 1 byte per pixel grayscale.
    Gray,
    /// 4 bytes per pixel RGBA.
    Rgba,
    /// 4 bytes per pixel BGRA.
    Bgra,
}

impl ImageTextureFormat {
    // fn toPixelFormat(self: ImageTextureFormat, srgb: bool) mtl.MTLPixelFormat
    fn to_pixel_format(self, srgb: bool) -> api::MTLPixelFormat {
        use api::MTLPixelFormat as F;
        match self {
            ImageTextureFormat::Gray => {
                if srgb {
                    F::R8Unorm_sRGB
                } else {
                    F::R8Unorm
                }
            }
            ImageTextureFormat::Rgba => {
                if srgb {
                    F::RGBA8Unorm_sRGB
                } else {
                    F::RGBA8Unorm
                }
            }
            ImageTextureFormat::Bgra => {
                if srgb {
                    F::BGRA8Unorm_sRGB
                } else {
                    F::BGRA8Unorm
                }
            }
        }
    }
}

// PORT NOTE (rustty-only smoke test, NOT a ghostty parity test): there is no
// reference `test{}` over `Metal.zig`. This CHUNK-2 gate
// (`GPU_RD_CHUNK2_DESIGN.md` §B.8) drives the offscreen `Metal` (not the raw
// leaves): `init_offscreen` (chooseDevice + newCommandQueue + device metadata) →
// `init_shaders` (all five pipelines) → `init_target` (offscreen IOSurface) →
// `begin_frame` → a trivial clear RenderPass → `complete(sync=true)` → the
// `read_bgra` readback. It asserts no panic + the readback is `w*h*4` bytes.
// Skipped cleanly on hosts with no Metal device (headless CI), like the landed
// leaf smoke tests.
#[cfg(test)]
mod tests {
    use objc2::runtime::ProtocolObject;
    use objc2_metal::MTLCommandBuffer;

    use super::super::render_pass::{Attachment, AttachmentTarget};
    use super::*;

    #[test]
    fn metal_offscreen_construct_and_begin_frame() {
        // Headless CI may lack a Metal device.
        let metal = match Metal::init_offscreen(AlphaBlending::Native, 4, 4) {
            Ok(m) => m,
            // `init_offscreen` only ever yields `NoMetalDevice`; the `ObjCFailed`
            // arm (W1-3) is unreachable here but keeps the match exhaustive.
            Err(e @ (Error::NoMetalDevice | Error::ObjCFailed)) => {
                // F5 strict-mode pivot (metal::test_support).
                crate::metal::test_support::require_metal_or_skip(
                    "metal_offscreen_construct_and_begin_frame",
                    &format!("{e:?}"),
                );
                eprintln!("no Metal device; skipping");
                return;
            }
        };
        // Apple Silicon (hasUnifiedMemory) → Shared; an Intel/discrete host would be
        // Managed. Accept either (per ADV-FIX B.8) so the test skips-or-passes on
        // any Metal host rather than failing on Intel.
        assert!(matches!(
            metal.default_storage_mode,
            api::StorageMode::Shared | api::StorageMode::Managed
        ));
        assert!(metal.max_texture_size >= 8192);
        assert_eq!(
            metal.surface_size().unwrap(),
            SurfaceSize {
                width: 4,
                height: 4
            }
        );

        // initShaders (load pipelines) → initTarget → beginFrame → trivial clear
        // pass → complete(sync).
        let shaders = metal.init_shaders(&[]).expect("init_shaders");
        assert!(shaders.post_pipelines.is_empty());
        let mut target = metal.init_target(4, 4).expect("init_target");

        // A no-op completion fn (the Chunk-3 `drawFrame` installs the real one).
        unsafe fn noop(_c: &frame::CompletionContext, _b: &ProtocolObject<dyn MTLCommandBuffer>) {}

        // Raw `*mut` target; the pointer does not hold the borrow, so `target`
        // stays usable below. Null renderer + no-op completion (Chunk-2 does not
        // exercise the container_of trampoline).
        let tp: *mut Target = &mut target;
        let mut frame = metal
            .begin_frame(core::ptr::null_mut(), tp, noop)
            .expect("begin_frame");

        // A trivial RenderPass: clear the target, begin/complete (no draw step).
        {
            let attachments = [Attachment {
                target: AttachmentTarget::Target(&target),
                clear_color: Some([0.0, 0.0, 0.0, 1.0]),
            }];
            let pass = frame.render_pass(&attachments);
            pass.complete();
        }

        // Complete synchronously: commit → waitUntilCompleted → invoke the (no-op)
        // completion fn directly.
        frame.complete(true);

        // Golden-support readback: `w*h*4` BGRA bytes out of the offscreen surface.
        let px = target.read_bgra();
        assert_eq!(px.len(), 4 * 4 * 4);
    }

    // PORT NOTE (rustty-only KAT, VALIDATION_REPORT §3c.3): pins the display-path
    // panic policy — a panic inside `abort_on_unwind` (the guard wrapped around
    // `display_callback`'s body) must ABORT the process (SIGABRT), never unwind
    // back toward the ObjC caller. Subprocess pattern: the test re-execs itself
    // with an env marker; the child runs the panicking guard and must die by
    // SIGABRT (`std::process::abort`), which the parent asserts on.
    #[test]
    fn abort_on_unwind_aborts_process() {
        const MARKER: &str = "RUSTTY_ABORT_ON_UNWIND_CHILD";
        if std::env::var_os(MARKER).is_some() {
            // Child: the panic must be converted to an abort, never an unwind.
            let _: u32 = super::abort_on_unwind(|| panic!("display-path panic"));
            unreachable!("abort_on_unwind must never return after a panic");
        }
        let exe = std::env::current_exe().expect("current_exe");
        let out = std::process::Command::new(exe)
            .args([
                "--exact",
                "metal::metal_impl::tests::abort_on_unwind_aborts_process",
            ])
            .env(MARKER, "1")
            .output()
            .expect("spawn child test process");
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(
            Some(6), // SIGABRT — std::process::abort()
            out.status.signal(),
            "child must die by SIGABRT, not exit/unwind; status={:?} stderr={}",
            out.status,
            String::from_utf8_lossy(&out.stderr),
        );
    }
}
