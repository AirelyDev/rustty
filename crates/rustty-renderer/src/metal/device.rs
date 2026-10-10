//! The Metal device context.
//!
//! This holds the MTLDevice and metadata derived from it.
//! The device is shared by all surface renderers; each renderer
//! creates its own MTLCommandQueue.
//!
//! PORTS: ghostty `src/renderer/metal/Device.zig` (new in `40d5b860d`, P8-W6 T2):
//! the device state, `chooseDevice`, `queryMaxTextureSize` and `warmup` moved out
//! of `Metal.zig` (`metal_impl.rs`) file-for-file, with their doc comments.
//!
//! PORT NOTE (`init(self: *Device, alloc)`): the reference initializes in place
//! into `App.device`, which `App.init` leaves `undefined` until then. Rust has no
//! `undefined`, so `init` writes into a `MaybeUninit<Device>` slot and leaves it
//! uninitialized on error (the core's `DeviceBox` owns that slot, N522). The
//! unused `alloc` parameter (`_ = alloc;`) is dropped, as everywhere in this crate.
//!
//! PORT NOTE (sharing across render threads): every surface renderer reads the
//! device through a `*const Device` (`Options.device`) on its own render thread.
//! That is sound: `MTLDevice` is documented thread-safe (objc2-metal declares the
//! protocol `Send + Sync`), and `default_storage_mode`/`max_texture_size` are
//! written once by `init`, before any renderer exists, and never again. `Device`
//! is therefore `Send + Sync` by construction; no `unsafe impl` is needed.

use std::mem::MaybeUninit;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLDevice, MTLGPUFamily};

use super::api;
use super::shaders::Shaders;

// const log = std.log.scoped(.metal);
/// `Device.zig:15`'s `const log = std.log.scoped(.metal);`.
const LOG_SCOPE: &str = "metal";

/// The Metal device context (`Device.zig`'s `Device = @This()`).
pub struct Device {
    /// MTLDevice
    pub device: Retained<ProtocolObject<dyn MTLDevice>>,

    /// The default storage mode to use for resources created with our device.
    ///
    /// This is based on whether the device is a discrete GPU or not, since
    /// discrete GPUs do not have unified memory and therefore do not support
    /// the "shared" storage mode, instead we have to use the "managed" mode.
    pub default_storage_mode: api::StorageMode,

    /// The maximum 2D texture width and height supported by the device.
    pub max_texture_size: u32,
}

/// `chooseDevice`'s `error{NoMetalDevice}`, the only error `init` returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    /// `chooseDevice` found no usable MTLDevice.
    NoMetalDevice,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NoMetalDevice => f.write_str("NoMetalDevice"),
        }
    }
}

impl std::error::Error for Error {}

impl Device {
    // pub fn init(self: *Device, alloc: std.mem.Allocator) !void {
    /// Initialize the device in place. On error `self_` stays uninitialized.
    pub fn init(self_: &mut MaybeUninit<Device>) -> Result<(), Error> {
        // Choose our MTLDevice.
        // const device = try chooseDevice();
        // errdefer device.release();   (ARC: the `Retained` drops on error)
        let device = choose_device()?;

        // Grab metadata about the device.
        //   .ios => .shared,  // manage mode is not supported by iOS
        //   else => if (device.getProperty(bool, "hasUnifiedMemory")) .shared else .managed,
        // (the `.ios` arm is a dead comptime arm on this macOS-only module, R1.)
        let default_storage_mode = if device.hasUnifiedMemory() {
            api::StorageMode::Shared
        } else {
            api::StorageMode::Managed
        };
        let max_texture_size = query_max_texture_size(&device);
        // log.debug(
        //     "device properties default_storage_mode={} max_texture_size={}",
        //     .{ default_storage_mode, max_texture_size },
        // );
        // (R1 rendering note: Zig's `{}` on an enum prints the fully qualified
        //  `<type>.<tag>`; the port has only a derived `Debug`, so this renders
        //  `Shared`/`Managed` where the reference renders
        //  `mtl.MTLResourceOptions.StorageMode.shared`. Known, accepted.)
        log::debug!(
            target: LOG_SCOPE,
            "device properties default_storage_mode={default_storage_mode:?} max_texture_size={max_texture_size}"
        );

        self_.write(Device {
            device,
            default_storage_mode,
            max_texture_size,
        });
        Ok(())
    }

    // pub fn deinit(self: *Device) void {
    //     self.device.release();
    //     self.* = undefined;
    // }
    //
    // PORT NOTE: the MTLDevice release is the `Retained`'s drop, which the owner
    // runs right after `deinit` (`rustty_core::app::DeviceBox::deinit` frees the
    // allocation), as `Metal::deinit` leaves its ARC releases to its drop.
    pub fn deinit(&mut self) {}

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
}

/// `Device.zig` `fn chooseDevice() error{NoMetalDevice}!objc.Object`.
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
        // We want a GPU that’s connected to a display.
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
// Device.zig `fn queryMaxTextureSize(device: objc.Object) u32`.
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

#[cfg(test)]
mod tests {
    use std::mem::MaybeUninit;

    use super::{Device, Error};

    /// Rustty-only: `init` chooses a device and fills its metadata, and
    /// `deinit` (then the drop the owner runs) releases it. Two devices can be
    /// live at once (each `init` retains the chosen MTLDevice).
    #[test]
    fn device_init_reads_metadata_and_deinit_releases() {
        let mut slot = MaybeUninit::<Device>::uninit();
        match Device::init(&mut slot) {
            Ok(()) => {}
            Err(e @ Error::NoMetalDevice) => {
                crate::metal::test_support::require_metal_or_skip(
                    "device_init_reads_metadata_and_deinit_releases",
                    &e,
                );
                eprintln!(
                    "no Metal device; skipping device_init_reads_metadata_and_deinit_releases"
                );
                return;
            }
        }
        // SAFETY: `init` returned Ok, so it wrote the slot.
        let mut device = unsafe { slot.assume_init() };
        assert!(matches!(
            device.default_storage_mode,
            super::api::StorageMode::Shared | super::api::StorageMode::Managed
        ));
        assert!([8192, 16384, 32768].contains(&device.max_texture_size));
        device.deinit();
        drop(device);
    }
}
