//! Wrapper for handling render pipelines.
//!
//! PORTS: ghostty `src/renderer/metal/Pipeline.zig`.
//!
//! PORT NOTE (R3/objc2, ARC): ghostty's manual `alloc`/`init` + `defer release()`
//! on the `MTLRenderPipelineDescriptor`/`MTLVertexDescriptor` become objc2's
//! `MTLRenderPipelineDescriptor::new()`/`MTLVertexDescriptor::new()` owned by a
//! scope-releasing `Retained`, and `deinit()` (`self.state.release()`) dissolves
//! into the `Retained<ProtocolObject<dyn MTLRenderPipelineState>>` field's `Drop`.
//! ghostty's untyped `setProperty`/`msgSend` sends map to objc2's typed setters
//! (identical Apple selectors), and `newRenderPipelineStateWithDescriptor:error:`
//! returns a `Result` where ghostty's `checkError(err)` yields `error.MetalFailed`.
//!
//! PORT NOTE (HARD SPOT 3, autoAttribute): ghostty's `autoAttribute` uses comptime
//! field reflection (`@typeInfo(T).@"struct".fields`) to build the vertex-attribute
//! descriptor from a vertex struct. Rust has no comptime field reflection, so the
//! comptime `?type` `VertexAttributes` becomes a runtime
//! `Option<VertexLayoutRef>` — a hand-written per-struct attribute table
//! (`(MTLVertexFormat, offset)` rows + stride) whose entries reproduce ghostty's
//! `autoAttribute` format switch and `@offsetOf`. The tables live in
//! `metal::shaders` (beside the pipeline descriptions that select them) and are
//! pinned to the Rust struct layout by `const{}` drift guards there; the reference
//! `autoAttribute` reflection loop is preserved verbatim below (`auto_attribute`).

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{
    MTLBlendFactor, MTLBlendOperation, MTLDevice, MTLLibrary, MTLRenderPipelineDescriptor,
    MTLRenderPipelineState, MTLVertexDescriptor,
};

use crate::metal::api;

// const log = std.log.scoped(.metal);
// PORT NOTE (updated task-14c-1): the `log` facade IS wired for this crate now
// (`Cargo.toml`'s `log = { workspace = true }`; DECISION-logging.md §6r call
// shape 3), and `generic.rs`'s `macos_impl` is the worked example. This file's
// `log.*` sites are simply NOT RESTORED YET — row S118-F4's stage owns them, and will
// add `const LOG_SCOPE: &str = "metal";` here alongside the first live
// emission. Until then each site stays a comment at its location. Scheduled
// work, not a missing dependency.
// The `log.err` site in `checkError` is the one it will fill.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// A Metal API call failed.
    MetalFailed,
}

/// Describes a color attachment.
//
// PORT NOTE (§C): ghostty's `Pipeline.Options.Attachment` is flattened to a
// module-scope `Attachment` (Rust lacks Zig's in-struct `Options.Attachment`
// namespacing; see `buffer.rs` for the flat-naming rationale).
pub struct Attachment {
    pub pixel_format: api::MTLPixelFormat,
    // Zig default = true; callers pass it explicitly.
    pub blending_enabled: bool,
}

/// Options for initializing a render pipeline.
pub struct Options<'a> {
    /// MTLDevice
    pub device: &'a ProtocolObject<dyn MTLDevice>,

    /// Name of the vertex function
    pub vertex_fn: &'a str,
    /// Name of the fragment function
    pub fragment_fn: &'a str,

    /// MTLLibrary to get the vertex function from
    pub vertex_library: &'a ProtocolObject<dyn MTLLibrary>,
    /// MTLLibrary to get the fragment function from
    pub fragment_library: &'a ProtocolObject<dyn MTLLibrary>,

    /// Vertex step function
    // Zig default `.per_vertex`; callers pass it explicitly.
    pub step_fn: api::MTLVertexStepFunction,

    /// Info about the color attachments used by this render pipeline.
    pub attachments: &'a [Attachment],
}

/// One attribute row: an MTLVertexFormat + its byte offset within the vertex
/// struct. (The `attr.setBufferIndex(0)` is a constant in `autoAttribute`, folded
/// into the `auto_attribute` loop.)
#[derive(Clone, Copy)]
pub struct VertexAttribute {
    pub format: api::MTLVertexFormat,
    pub offset: usize,
}

/// A vertex struct's attribute table + its stride (== `size_of::<V>()`). The
/// runtime stand-in for ghostty's comptime `?type` `VertexAttributes`.
#[derive(Clone, Copy)]
pub struct VertexLayoutRef {
    pub attrs: &'static [VertexAttribute],
    pub stride: usize,
}

/// MTLRenderPipelineState
pub struct Pipeline {
    pub state: Retained<ProtocolObject<dyn MTLRenderPipelineState>>,
}

impl Pipeline {
    /// Ports `Pipeline.init(comptime VertexAttributes: ?type, opts)`. The comptime
    /// `?type` becomes an `Option<VertexLayoutRef>`: `Some(table)` builds the
    /// `MTLVertexDescriptor` from the hand-written table; `None` = no vertex input
    /// (the full-screen-triangle pipelines + post-pipelines).
    pub fn init(
        vertex_attributes: Option<VertexLayoutRef>,
        opts: Options<'_>,
    ) -> Result<Self, Error> {
        // Create our descriptor
        let desc = MTLRenderPipelineDescriptor::new();

        // Get our vertex and fragment functions and add them to the descriptor.
        {
            let str = NSString::from_str(opts.vertex_fn);
            // ghostty force-unwraps `ptr.?`; the nil (function-not-found) is
            // surfaced as `error.MetalFailed` rather than a deferred nil crash.
            let func_vert = opts
                .vertex_library
                .newFunctionWithName(&str)
                .ok_or(Error::MetalFailed)?;

            desc.setVertexFunction(Some(&func_vert));
        }
        {
            let str = NSString::from_str(opts.fragment_fn);
            let func_frag = opts
                .fragment_library
                .newFunctionWithName(&str)
                .ok_or(Error::MetalFailed)?;

            desc.setFragmentFunction(Some(&func_frag));
        }

        // If we have vertex attributes, create and add a vertex descriptor.
        if let Some(table) = vertex_attributes {
            let vertex_desc = MTLVertexDescriptor::new();

            // Our attributes are the fields of the input
            auto_attribute(table, &vertex_desc);

            // The layout describes how and when we fetch the next vertex input.
            let layouts = vertex_desc.layouts();
            {
                // SAFETY: index 0 is in range; objc2 marks the subscript unsafe
                // (it "might not be bounds-checked").
                let layout = unsafe { layouts.objectAtIndexedSubscript(0) };

                layout.setStepFunction(opts.step_fn);
                // SAFETY: objc2 marks `setStride` unsafe (stride invariants); the
                // value is `@sizeOf(V)` (the table's stride).
                unsafe { layout.setStride(table.stride) };
            }

            desc.setVertexDescriptor(Some(&vertex_desc));
        }

        // Set our color attachment
        let attachments = desc.colorAttachments();
        for (i, at) in opts.attachments.iter().enumerate() {
            // SAFETY: `i` < `opts.attachments.len()`; objc2 marks the subscript
            // unsafe ("attachmentIndex might not be bounds-checked").
            let attachment = unsafe { attachments.objectAtIndexedSubscript(i) };

            attachment.setPixelFormat(at.pixel_format);

            attachment.setBlendingEnabled(at.blending_enabled);
            // We always use premultiplied alpha blending for now.
            if at.blending_enabled {
                attachment.setRgbBlendOperation(MTLBlendOperation::Add);
                attachment.setAlphaBlendOperation(MTLBlendOperation::Add);
                attachment.setSourceRGBBlendFactor(MTLBlendFactor::One);
                attachment.setSourceAlphaBlendFactor(MTLBlendFactor::One);
                attachment.setDestinationRGBBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
                attachment.setDestinationAlphaBlendFactor(MTLBlendFactor::OneMinusSourceAlpha);
            }
        }

        // Make our state
        //
        // PORT NOTE: ghostty passes an `err` out-param and calls `checkError(err)`,
        // which logs `nserr.localizedDescription` before returning
        // `error.MetalFailed`:
        //     log.err("metal error={s}", .{str.cstringPtr(.ascii).?});
        // objc2's `newRenderPipelineStateWithDescriptor:error:_` returns a `Result`
        // whose `Err` carries the same `NSError`; the log site is preserved above
        // (not restored yet — row S118-F4; the facade itself IS wired), and the `errdefer
        // pipeline_state.release()` dissolves into `?` early-return + `Retained`
        // Drop (a failed return drops nothing — the state is not yet stored).
        let state = opts
            .device
            .newRenderPipelineStateWithDescriptor_error(&desc)
            .map_err(|_nserr| Error::MetalFailed)?;

        Ok(Self { state })
    }

    // PORT NOTE (ARC): `deinit()` (`self.state.release()`) dissolves into the
    // `Retained` field's automatic `Drop` — no manual impl.
}

/// Ports `Pipeline.autoAttribute`: for each attribute, set format/offset and
/// bufferIndex(0).
//
// PORT NOTE (HARD SPOT 3): ghostty's `autoAttribute(T: type, attrs)` reflects over
// `@typeInfo(T).@"struct".fields`, deriving each attribute's `MTLVertexFormat` from
// the field's backing/tag type and its offset from `@offsetOf(T, field.name)`:
//
//     fn autoAttribute(T: type, attrs: objc.Object) void {
//         inline for (@typeInfo(T).@"struct".fields, 0..) |field, i| {
//             const offset = @offsetOf(T, field.name);
//             const FT = switch (@typeInfo(field.type)) {
//                 .@"struct" => |e| e.backing_integer.?,
//                 .@"enum" => |e| e.tag_type,
//                 else => field.type,
//             };
//             // Very incomplete list, expand as necessary.
//             const format = switch (FT) {
//                 [4]u8 => mtl.MTLVertexFormat.uchar4,
//                 [2]u16 => mtl.MTLVertexFormat.ushort2,
//                 [2]i16 => mtl.MTLVertexFormat.short2,
//                 f32 => mtl.MTLVertexFormat.float,
//                 [2]f32 => mtl.MTLVertexFormat.float2,
//                 [4]f32 => mtl.MTLVertexFormat.float4,
//                 i32 => mtl.MTLVertexFormat.int,
//                 [2]i32 => mtl.MTLVertexFormat.int2,
//                 [4]i32 => mtl.MTLVertexFormat.int2,
//                 u32 => mtl.MTLVertexFormat.uint,
//                 [2]u32 => mtl.MTLVertexFormat.uint2,
//                 [4]u32 => mtl.MTLVertexFormat.uint4,
//                 u8 => mtl.MTLVertexFormat.uchar,
//                 i8 => mtl.MTLVertexFormat.char,
//                 else => comptime unreachable,
//             };
//             const attr = attrs.msgSend(objc.Object, objc.sel("objectAtIndexedSubscript:"), .{@as(c_ulong, i)});
//             attr.setProperty("format", @intFromEnum(format));
//             attr.setProperty("offset", @as(c_ulong, offset));
//             attr.setProperty("bufferIndex", @as(c_ulong, 0));
//         }
//     }
//
// Rust has no comptime field reflection: the `FT -> MTLVertexFormat` switch and the
// `@offsetOf` are pre-computed into the per-struct `VertexLayoutRef` tables in
// `metal::shaders` (pinned to the Rust struct layout by `const{}` drift guards
// there). This runtime loop reproduces the descriptor-setting tail verbatim.
fn auto_attribute(table: VertexLayoutRef, vertex_desc: &MTLVertexDescriptor) {
    let attrs = vertex_desc.attributes();
    for (i, a) in table.attrs.iter().enumerate() {
        // SAFETY: `i` < `table.attrs.len()`; objc2 marks the subscript unsafe.
        let attr = unsafe { attrs.objectAtIndexedSubscript(i) };

        attr.setFormat(a.format);
        // SAFETY: objc2 marks `setOffset`/`setBufferIndex` unsafe (layout
        // invariants); the offset is the field's `@offsetOf`, bufferIndex is 0.
        unsafe {
            attr.setOffset(a.offset);
            attr.setBufferIndex(0);
        }
    }
}
