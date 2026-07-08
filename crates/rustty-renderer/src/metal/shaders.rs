//! The Metal shader backend container.
//!
//! PORTS: the GPU-backend machinery of ghostty `src/renderer/metal/shaders.zig` —
//! the `Shaders` container (library + pipeline collection + post-pipelines), the
//! `PipelineCollection`/`PipelineDescription` types, `initLibrary`, and
//! `initPostPipelines`/`initPostPipeline`.
//!
//! PORT NOTE (module split): ghostty's single `metal/shaders.zig` holds BOTH the
//! vertex/uniform POD structs (`CellText`/`CellBg`/`Uniforms`/`Image`/`BgImage`)
//! AND this GPU-backend machinery. rustty already split the PODs to
//! `crate::shaders` (the headless R-a port), so `metal::shaders` (this module) is
//! DISTINCT from `crate::shaders`: it re-uses `crate::shaders::{CellText, Image,
//! BgImage}` for the attribute tables and owns only the GPU-backend machinery.
//!
//! PORT NOTE (§G item 3, `@Type` synthesis): Zig synthesizes `PipelineCollection`
//! from `pipeline_descs` via `@Type(.{.@"struct" = …})` (named fields
//! `bg_color`/`cell_bg`/`cell_text`/`image`/`bg_image`, each a `Pipeline`). Rust
//! has no type synthesis → the struct is hand-written with those exact field names
//! (generic.zig reads `shaders.pipelines.cell_text` etc., so they are load-bearing),
//! and the `inline for (pipeline_descs)` init/deinit loops become explicit
//! per-field construction.

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_foundation::NSString;
use objc2_metal::{MTLDevice, MTLLibrary};

use crate::metal::api;
use crate::metal::api::MTLVertexFormat as F;
use crate::metal::api::MTLVertexStepFunction as SF;
// The GPU-ABI POD types (`crate::metal::gpu_abi`) are what is LITERALLY in the
// per-instance buffers (§2.2), so the `VertexLayout` tables + drift guards pin
// THEM. `Image` STAYS `crate::shaders::Image` — it is already ABI, no `ImageGpu`.
use crate::metal::gpu_abi::{BgImageGpu, CellTextGpu};
use crate::metal::pipeline::{
    Attachment, Options as PipelineOptions, Pipeline, VertexAttribute as VA, VertexLayoutRef,
};
use crate::shaders::Image;

// const log = std.log.scoped(.metal);
// PORT NOTE: no logging facade is wired in the port yet (see `buffer.rs`); the
// `log.debug`/`log.warn`/`log.err` call sites are preserved as comments.

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    /// A Metal API call failed.
    MetalFailed,
}

// -------------------------------------------------------------------------------
// HARD SPOT 3: the vertex-attribute tables (the hand-written `autoAttribute`).
//
// `Pipeline.zig`'s comptime `autoAttribute` reflects over a vertex struct's fields
// to build the MTLVertexDescriptor. Rust has no comptime field reflection, so each
// instanced vertex struct (`CellText`/`Image`/`BgImage`) gets a hand-written const
// table of `(MTLVertexFormat, offset)` rows + stride (== `@sizeOf`). The
// `MTLVertexFormat` per field reproduces `Pipeline.zig`'s `FT -> MTLVertexFormat`
// switch (on the field's backing/tag type); the offsets reproduce `@offsetOf`. The
// `bg_color`/`cell_bg` (full-screen-triangle vertex fn, no vertex input) + all
// `post_pipelines` pass `None`.
// -------------------------------------------------------------------------------

/// A vertex struct that has an MTLVertexDescriptor attribute table.
pub trait VertexLayout: Sized {
    const LAYOUT: VertexLayoutRef;
}

// PORT NOTE (§2.2 re-pin): the `VertexLayout` for the `cell_text` vertex is on
// `CellTextGpu` (the ABI POD literally in `FrameState.cells`), NOT the ergonomic
// `crate::shaders::CellText`. Offsets/stride are byte-identical to the pre-Chunk-4
// `CellText` table (`[0,8,16,20,24,28,29]`, stride 32) — the GPU vertex-fetch
// behavior is UNCHANGED; only the *content* of byte 29 (`is_cursor_glyph` now in
// bit 1) is ABI-fixed by the `gpu_abi` serializer.
impl VertexLayout for CellTextGpu {
    const LAYOUT: VertexLayoutRef = VertexLayoutRef {
        attrs: &[
            VA {
                format: F::UInt2,
                offset: 0,
            }, // glyph_pos:  [2]u32
            VA {
                format: F::UInt2,
                offset: 8,
            }, // glyph_size: [2]u32
            VA {
                format: F::Short2,
                offset: 16,
            }, // bearings:   [2]i16
            VA {
                format: F::UShort2,
                offset: 20,
            }, // grid_pos:   [2]u16
            VA {
                format: F::UChar4,
                offset: 24,
            }, // color:      [4]u8
            VA {
                format: F::UChar,
                offset: 28,
            }, // atlas:      Atlas enum(u8) -> u8
            VA {
                format: F::UChar,
                offset: 29,
            }, // bools:      packed(u8) -> u8
        ],
        stride: core::mem::size_of::<CellTextGpu>(),
    };
}

impl VertexLayout for Image {
    const LAYOUT: VertexLayoutRef = VertexLayoutRef {
        attrs: &[
            VA {
                format: F::Float2,
                offset: 0,
            }, // grid_pos:    [2]f32
            VA {
                format: F::Float2,
                offset: 8,
            }, // cell_offset: [2]f32
            VA {
                format: F::Float4,
                offset: 16,
            }, // source_rect: [4]f32
            VA {
                format: F::Float2,
                offset: 32,
            }, // dest_size:   [2]f32
        ],
        stride: core::mem::size_of::<Image>(),
    };
}

// §2.3 re-pin: on `BgImageGpu` (byte-identical `[0,4]`, stride 8).
impl VertexLayout for BgImageGpu {
    const LAYOUT: VertexLayoutRef = VertexLayoutRef {
        attrs: &[
            VA {
                format: F::Float,
                offset: 0,
            }, // opacity: f32
            VA {
                format: F::UChar,
                offset: 4,
            }, // info:    Info packed(u8) -> u8
        ],
        stride: core::mem::size_of::<BgImageGpu>(),
    };
}

// Compile-time drift guards (a failed const panics at build time): they pin (a)
// `size_of::<V>()` to the Zig `@sizeOf`, and (b) every table offset to
// `offset_of!` — so the hand-written table can never silently drift from the Rust
// struct, and the Rust struct can never silently drift from the Zig `@sizeOf`.
//
// PORT NOTE (R1): this UN-DEFERS `metal/shaders.zig`'s lone `test { expectEqual(32,
// @sizeOf(CellText)) }` as a compile-time invariant (strictly stronger than a
// runtime `#[test]`). The reference test comment ("Minimizing the size of this
// struct is important, so we test it in order to be aware of any changes.") is
// preserved in `crate::shaders`.
//
// CHUNK-4 UN-DEFER: the `atlas`/`bools`/`info` bytes are now shader-correct — the
// layout/offset guards below pin `CellTextGpu`/`BgImageGpu` (what is LITERALLY in
// the buffer, §2.2/§2.3) whose `From` serializers pack the packed-bool byte, and
// the `gpu_abi` byte unit-test asserts the exact bit values. (GPU_RD_CHUNK1B_DESIGN
// §OPEN RISKS #1 is now closed.)
//
// PORT NOTE (R1 — do NOT lose the reference test{}): `metal/shaders.zig`'s lone
// `test { expectEqual(32, @sizeOf(CellText)) }` stands on the ERGONOMIC struct, so
// the standalone `size_of::<crate::shaders::CellText>() == 32` guard below is KEPT
// (G3b does not gate this crate — this const guard is its only enforcement; the
// re-pin to `CellTextGpu` must not drop it).
const _: () = {
    // The reference test{} (on the ergonomic type).
    assert!(core::mem::size_of::<crate::shaders::CellText>() == 32);

    // The buffer-ABI layout guards (on the POD literally in the vertex buffer).
    assert!(core::mem::size_of::<CellTextGpu>() == 32);
    assert!(core::mem::offset_of!(CellTextGpu, glyph_pos) == 0);
    assert!(core::mem::offset_of!(CellTextGpu, glyph_size) == 8);
    assert!(core::mem::offset_of!(CellTextGpu, bearings) == 16);
    assert!(core::mem::offset_of!(CellTextGpu, grid_pos) == 20);
    assert!(core::mem::offset_of!(CellTextGpu, color) == 24);
    assert!(core::mem::offset_of!(CellTextGpu, atlas) == 28);
    assert!(core::mem::offset_of!(CellTextGpu, bools) == 29);

    assert!(core::mem::size_of::<Image>() == 40);
    assert!(core::mem::offset_of!(Image, grid_pos) == 0);
    assert!(core::mem::offset_of!(Image, cell_offset) == 8);
    assert!(core::mem::offset_of!(Image, source_rect) == 16);
    assert!(core::mem::offset_of!(Image, dest_size) == 32);

    assert!(core::mem::size_of::<BgImageGpu>() == 8);
    assert!(core::mem::offset_of!(BgImageGpu, opacity) == 0);
    assert!(core::mem::offset_of!(BgImageGpu, info) == 4);
};

/// All the comptime-known info about a pipeline, so that
/// we can define them ahead-of-time in an ergonomic way.
//
// PORT NOTE: the comptime `vertex_attributes: ?type` becomes `Option<VertexLayoutRef>`
// (§2.2); `step_fn` Zig default `.per_vertex` and `vertex_attributes` default `null`
// are supplied explicitly at each `PIPELINE_DESCS` entry.
pub struct PipelineDescription {
    pub vertex_attributes: Option<VertexLayoutRef>,
    pub vertex_fn: &'static str,
    pub fragment_fn: &'static str,
    pub step_fn: api::MTLVertexStepFunction,
    pub blending_enabled: bool,
}

impl PipelineDescription {
    /// Ports `PipelineDescription.initPipeline`.
    fn init_pipeline(
        &self,
        device: &ProtocolObject<dyn MTLDevice>,
        library: &ProtocolObject<dyn MTLLibrary>,
        pixel_format: api::MTLPixelFormat,
    ) -> Result<Pipeline, Error> {
        Pipeline::init(
            self.vertex_attributes,
            PipelineOptions {
                device,
                vertex_fn: self.vertex_fn,
                fragment_fn: self.fragment_fn,
                vertex_library: library,
                fragment_library: library,
                step_fn: self.step_fn,
                attachments: &[Attachment {
                    pixel_format,
                    blending_enabled: self.blending_enabled,
                }],
            },
        )
        .map_err(|_| Error::MetalFailed)
    }
}

// The `pipeline_descs` comptime table → a module `const`. Each `cell_text`/`image`/
// `bg_image` entry carries its `VertexLayoutRef` via `Some(<Struct as
// VertexLayout>::LAYOUT)`; `bg_color`/`cell_bg` (full-screen-triangle vertex fn, no
// vertex input) carry `None`. The name in each `.0` mirrors Zig's `pipeline[0]`
// (the `@Type`-synthesized field name).
const PIPELINE_DESCS: &[(&str, PipelineDescription)] = &[
    (
        "bg_color",
        PipelineDescription {
            vertex_attributes: None,
            vertex_fn: "full_screen_vertex",
            fragment_fn: "bg_color_fragment",
            step_fn: SF::PerVertex,
            blending_enabled: false,
        },
    ),
    (
        "cell_bg",
        PipelineDescription {
            vertex_attributes: None,
            vertex_fn: "full_screen_vertex",
            fragment_fn: "cell_bg_fragment",
            step_fn: SF::PerVertex,
            blending_enabled: true,
        },
    ),
    (
        "cell_text",
        PipelineDescription {
            vertex_attributes: Some(<CellTextGpu as VertexLayout>::LAYOUT),
            vertex_fn: "cell_text_vertex",
            fragment_fn: "cell_text_fragment",
            step_fn: SF::PerInstance,
            blending_enabled: true,
        },
    ),
    (
        "image",
        PipelineDescription {
            vertex_attributes: Some(<Image as VertexLayout>::LAYOUT),
            vertex_fn: "image_vertex",
            fragment_fn: "image_fragment",
            step_fn: SF::PerInstance,
            blending_enabled: true,
        },
    ),
    (
        "bg_image",
        PipelineDescription {
            vertex_attributes: Some(<BgImageGpu as VertexLayout>::LAYOUT),
            vertex_fn: "bg_image_vertex",
            fragment_fn: "bg_image_fragment",
            step_fn: SF::PerInstance,
            blending_enabled: true,
        },
    ),
];

/// We create a type for the pipeline collection based on our desc array.
//
// PORT NOTE (§G item 3): the `@Type`-synthesized struct is hand-written with the
// exact field names read by generic.zig (`shaders.pipelines.cell_text` etc.), so
// they are load-bearing.
pub struct PipelineCollection {
    pub bg_color: Pipeline,
    pub cell_bg: Pipeline,
    pub cell_text: Pipeline,
    pub image: Pipeline,
    pub bg_image: Pipeline,
}

/// This contains the state for the shaders used by the Metal renderer.
pub struct Shaders {
    pub library: Retained<ProtocolObject<dyn MTLLibrary>>,

    /// Collection of available render pipelines.
    pub pipelines: PipelineCollection,

    /// Custom shaders to run against the final drawable texture. This
    /// can be used to apply a lot of effects. Each shader is run in sequence
    /// against the output of the previous shader.
    //
    // PORT NOTE: Zig `[]const Pipeline` + `alloc.alloc`; `Vec` owns it.
    pub post_pipelines: Vec<Pipeline>,

    /// Set to true when deinited, if you try to deinit a defunct set
    /// of shaders it will just be ignored, to prevent double-free.
    pub defunct: bool,
}

impl Shaders {
    /// Initialize our shader set.
    ///
    /// "post_shaders" is an optional list of postprocess shaders to run
    /// against the final drawable texture. This is an array of shader source
    /// code, not file paths.
    //
    // PORT NOTE: Zig takes an `Allocator` (for `post_pipelines`); Rust owns via
    // `Vec`/`Retained` Drop, so the `alloc` param dissolves. `device` +
    // `pixel_format` are handed in by the caller (Chunk-2 `Metal.initShaders`, which
    // chooses `pixel_format` from blending/colorspace) — no `Metal` type dependency,
    // so this compiles in 1b. Zig's `post_shaders: []const [:0]const u8` → `&[&CStr]`.
    pub fn init(
        device: &ProtocolObject<dyn MTLDevice>,
        post_shaders: &[&std::ffi::CStr],
        pixel_format: api::MTLPixelFormat,
    ) -> Result<Self, Error> {
        let library = init_library(device)?;

        // Build each pipeline.
        //
        // PORT NOTE (ARC): Zig's `errdefer inline for (pipeline_descs) … deinit()`
        // unwinding on partial-init failure dissolves into `?` early-return +
        // `Retained` Drop — a failed later pipeline drops the already-constructed
        // `Retained` pipelines automatically. `PipelineCollection` is built
        // field-by-field rather than by name-indexed reflection (§G item 2).
        let pipelines = PipelineCollection {
            bg_color: PIPELINE_DESCS[0]
                .1
                .init_pipeline(device, &library, pixel_format)?,
            cell_bg: PIPELINE_DESCS[1]
                .1
                .init_pipeline(device, &library, pixel_format)?,
            cell_text: PIPELINE_DESCS[2]
                .1
                .init_pipeline(device, &library, pixel_format)?,
            image: PIPELINE_DESCS[3]
                .1
                .init_pipeline(device, &library, pixel_format)?,
            bg_image: PIPELINE_DESCS[4]
                .1
                .init_pipeline(device, &library, pixel_format)?,
        };

        let post_pipelines: Vec<Pipeline> =
            init_post_pipelines(device, &library, post_shaders, pixel_format).unwrap_or_else(
                |_err| {
                    // If an error happens while building postprocess shaders we
                    // want to just not use any postprocess shaders since we don't
                    // want to block Ghostty from working.
                    // log.warn("error initializing postprocess shaders err={}", .{err});
                    Vec::new()
                },
            );

        Ok(Self {
            library,
            pipelines,
            post_pipelines,
            defunct: false,
        })
    }

    /// Ports `Shaders.deinit` (the double-free guard + per-field release).
    //
    // PORT NOTE (ARC): Zig takes an `Allocator` (frees `post_pipelines`) and
    // explicitly releases each pipeline + library. In Rust those are `Retained`/`Vec`
    // fields whose `Drop` releases them when `self` drops. The `defunct` flag is kept
    // (generic.zig asserts on it) even though `Drop` makes double-free structurally
    // impossible — it preserves the idempotent-deinit behavior (double-deinit is a
    // no-op).
    pub fn deinit(&mut self) {
        if self.defunct {
            return;
        }
        self.defunct = true;

        // Release our primary shaders / Release our postprocess shaders: the
        // `Retained` pipelines + library + the `Vec<Pipeline>` release via `Drop`.
    }
}

/// Initialize the MTLLibrary. A MTLLibrary is a collection of shaders.
//
// PORT NOTE ([F7], §3): two paths, exactly as ghostty + the blueprint — the
// embedded `.metallib` (via `newLibraryWithData`) when `build.rs` set the
// `rustty_metallib` cfg, else `newLibraryWithSource` from `include_str!`. [F7] pins
// the metallib path for GENERATING and COMPARING the golden; the source path is
// compile-portability only (the two compilers can differ sub-LSB). The `checkError`
// (`log.err("metal error={s}", …)`) is preserved as the `.map_err` below.
fn init_library(
    device: &ProtocolObject<dyn MTLDevice>,
) -> Result<Retained<ProtocolObject<dyn MTLLibrary>>, Error> {
    // const start = try std.time.Instant.now();  (shader-load timing; the `log.debug`
    // below is a Goal-2 observability nicety, preserved as a comment.)

    #[cfg(rustty_metallib)]
    let library = {
        // ghostty: macos.dispatch.Data.create(@embedFile("ghostty_metallib"),
        //   macos.dispatch.queue.getMain(), DESTRUCTOR_DEFAULT) → newLibraryWithData:error:.
        // build.rs emits `$OUT_DIR/rustty.metallib`; `DispatchData::from_bytes` copies
        // the bytes into a `dispatch_data_t` (DESTRUCTOR_DEFAULT-equivalent).
        const METALLIB: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/rustty.metallib"));
        let data = dispatch2::DispatchData::from_bytes(METALLIB);
        device
            .newLibraryWithData_error(&data)
            .map_err(|_nserr| Error::MetalFailed)?
    };
    #[cfg(not(rustty_metallib))]
    let library = {
        // Fallback ([F7] compile-portability only): compile the SAME MSL at runtime
        // (the byte-faithful mirror of the embedded metallib, minus the toolchain).
        const SRC: &str = include_str!("../shaders/shaders.metal");
        let src = NSString::from_str(SRC);
        device
            .newLibraryWithSource_options_error(&src, None)
            .map_err(|_nserr| Error::MetalFailed)?
    };

    // const end = try std.time.Instant.now();
    // log.debug("shader library loaded time={}us", .{end.since(start) / std.time.ns_per_us});

    Ok(library)
}

/// Initialize our custom shader pipelines.
///
/// The shaders argument is a set of shader source code, not file paths.
fn init_post_pipelines(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
    shaders: &[&std::ffi::CStr],
    pixel_format: api::MTLPixelFormat,
) -> Result<Vec<Pipeline>, Error> {
    // If we have no shaders, do nothing.
    if shaders.is_empty() {
        return Ok(Vec::new()); // Zig `return &.{}`
    }

    // Initialize our result set. If any error happens, we undo everything.
    //
    // PORT NOTE: Zig keeps an explicit `i` + `errdefer` to release the successfully
    // built pipelines on a later failure ("we don't use \"0..\" to build our index
    // because we need to keep track of our length to clean up above"). That unwinding
    // dissolves into `?` early-return + `Vec`/`Retained` Drop.
    let mut pipelines = Vec::with_capacity(shaders.len()); // Zig `alloc.alloc(Pipeline, shaders.len)`

    // Build each shader.
    for source in shaders {
        pipelines.push(init_post_pipeline(device, library, source, pixel_format)?);
    }

    Ok(pipelines)
}

/// Initialize a single custom shader pipeline from shader source.
fn init_post_pipeline(
    device: &ProtocolObject<dyn MTLDevice>,
    library: &ProtocolObject<dyn MTLLibrary>,
    data: &std::ffi::CStr,
    pixel_format: api::MTLPixelFormat,
) -> Result<Pipeline, Error> {
    // Create our library which has the shader source
    //
    // PORT NOTE: ghostty's `[:0]const u8` sentinel-terminated source → `&CStr` here
    // (the shadertoy transpile already produces owned NUL-terminated MSL);
    // `NSString::from_str` copies the source, so the NUL sentinel (a Zig-ABI concern)
    // is not needed for `NSString`. The `checkError` on `newLibraryWithSource:` is the
    // `.map_err` below; the `errdefer post_library.release()` dissolves into `Retained`
    // Drop (`post_library` drops at end of scope, mirroring Zig's `defer release`).
    // R1 (byte-faithful shader sources): the transpiled MSL is C-string text; decode
    // its bytes as UTF-8 WITHOUT lossy replacement. spirv-cross emits ASCII so this
    // never fails in practice, but an invalid byte is surfaced as an error rather than
    // silently corrupted to U+FFFD (the reference passes the exact `[:0]const u8` bytes).
    let text = std::str::from_utf8(data.to_bytes()).map_err(|_| Error::MetalFailed)?;
    let src = NSString::from_str(text);
    let post_library = device
        .newLibraryWithSource_options_error(&src, None)
        .map_err(|_nserr| Error::MetalFailed)?;

    Pipeline::init(
        None,
        PipelineOptions {
            device,
            vertex_fn: "full_screen_vertex",
            fragment_fn: "main0",
            vertex_library: library,
            fragment_library: &post_library,
            // Zig omits `step_fn` (defaults `.per_vertex`); supplied explicitly.
            step_fn: api::MTLVertexStepFunction::PerVertex,
            attachments: &[Attachment {
                pixel_format,
                blending_enabled: false,
            }],
        },
    )
    .map_err(|_| Error::MetalFailed)
}
