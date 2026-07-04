//! Wrapper for handling samplers.
//!
//! PORTS: ghostty `src/renderer/metal/Sampler.zig` (a file-as-struct: the
//! top-level `sampler` field IS the struct).
//!
//! PORT NOTE (R3/objc2, ARC): ghostty's `deinit()` (`self.sampler.release()`)
//! dissolves into the `Retained<ProtocolObject<dyn MTLSamplerState>>` field's
//! automatic `Drop`. objc2's typed `MTLSamplerDescriptor` setters replace
//! ghostty's untyped `setProperty` message sends, and
//! `newSamplerStateWithDescriptor:` returns `Option<Retained<…>>` where
//! ghostty's `?*anyopaque` `orelse` yields `error.MetalFailed`.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_metal::{MTLDevice, MTLSamplerDescriptor, MTLSamplerState};

use crate::metal::api;

// const log = std.log.scoped(.metal);
// PORT NOTE: no logging facade is wired in the port yet (see `shadertoy.rs`).

/// Options for initializing a sampler.
//
// PORT NOTE (§C): ghostty's `Sampler.Options` → `SamplerOptions` (see
// `buffer.rs` for the flat-naming rationale).
pub struct SamplerOptions {
    /// MTLDevice
    pub device: Retained<ProtocolObject<dyn MTLDevice>>,
    pub min_filter: api::MTLSamplerMinMagFilter,
    pub mag_filter: api::MTLSamplerMinMagFilter,
    pub s_address_mode: api::MTLSamplerAddressMode,
    pub t_address_mode: api::MTLSamplerAddressMode,
}

pub struct Sampler {
    /// The underlying MTLSamplerState Object.
    pub sampler: Retained<ProtocolObject<dyn MTLSamplerState>>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// A Metal API call failed.
    MetalFailed,
}

impl Sampler {
    /// Initialize a sampler
    pub fn init(opts: SamplerOptions) -> Result<Self, Error> {
        // Create our descriptor
        //
        // PORT NOTE (ARC): ghostty's `alloc`/`init` two-step plus
        // `defer desc.release()` becomes objc2's `MTLSamplerDescriptor::new()`
        // (`[[MTLSamplerDescriptor alloc] init]`) owned by a scope-releasing
        // `Retained`.
        let desc = MTLSamplerDescriptor::new();

        // Properties
        desc.setMinFilter(opts.min_filter);
        desc.setMagFilter(opts.mag_filter);
        desc.setSAddressMode(opts.s_address_mode);
        desc.setTAddressMode(opts.t_address_mode);

        // Create the sampler state
        let sampler = opts
            .device
            .newSamplerStateWithDescriptor(&desc)
            .ok_or(Error::MetalFailed)?;

        Ok(Self { sampler })
    }

    // PORT NOTE (ARC): `deinit()` (`self.sampler.release()`) dissolves into the
    // `Retained` field's automatic `Drop` — no manual impl.
}
