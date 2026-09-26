//! The OpenGL shader backend container.
//!
//! PORTS: ghostty `src/renderer/opengl/shaders.zig:1-160,302-376` — the `Shaders`
//! container (pipeline collection + post-pipelines), the `PipelineDescription`/
//! `PipelineCollection` types, the `pipeline_descs` table, `Shaders.init`/`deinit`,
//! and `initPostPipelines`/`initPostPipeline`. The vertex/uniform POD structs
//! (`shaders.zig:163-300`) live in the sibling [`super::gpu_abi`]. This is the
//! OpenGL twin of `crate::metal::shaders`.
//!
//! PORT NOTE (`@Type` synthesis → hand-written struct): Zig synthesizes
//! `PipelineCollection` from `pipeline_descs` via `@Type(.{.@"struct" = …})` with
//! named fields `bg_color`/`cell_bg`/`cell_text`/`image`/`bg_image` (each a
//! `Pipeline`). Rust has no type synthesis, so the struct is hand-written with
//! those exact field names (generic reads `shaders.pipelines.cell_text` etc., so
//! they are load-bearing), and the `inline for (pipeline_descs)` init/deinit loops
//! become explicit per-field construction — the shape `metal::shaders`
//! established.
//!
//! PORT NOTE (HARD SPOT, `autoAttribute` tables): Zig's `autoAttribute`
//! (`Pipeline.zig:76-164`) reflects over a vertex struct's fields at comptime to
//! build the `glVertexAttrib*Format` calls. Rust has no comptime field reflection,
//! so each instanced vertex struct (`CellText`/`Image`/`BgImage`) gets a
//! hand-written [`VertexLayout`] table of [`AttribDesc`] rows (the pre-computed
//! `FT -> format` switch + `@offsetOf`) + stride (`@sizeOf`). The tables are pinned
//! to [`super::gpu_abi`] by `const{}` drift guards. `bg_color`/`cell_bg` (the
//! full-screen-triangle vertex fn, no vertex input) pass `None`. This is exactly
//! the shape `metal::shaders` established.
//!
//! PORT NOTE (`@embedFile` + `processIncludes` → `build.rs` + `include_bytes!`):
//! Zig resolves `#include "common.glsl"` at comptime in `loadShaderCode`
//! (`shaders.zig:347-376`). Rust has no comptime file embedding with include
//! resolution, so `build.rs` ports `processIncludes` at build time — it walks each
//! `.glsl` asset, inlines the (single) `#include "common.glsl"`, and writes the
//! resolved output (NUL-terminated) to `$OUT_DIR/glsl/*.glsl`. This module then
//! embeds those via `include_bytes!` as `&CStr` consts (the `[:0]const u8` the
//! reference's `loadShaderCode` produced).

use std::ffi::CStr;

use super::gpu_abi::{BgImage, CellText, Image};
use super::pipeline::{
    AttribClass, AttribDesc, Options as PipelineOptions, Pipeline, StepFunction, VertexLayout,
};

// const log = std.log.scoped(.opengl);   (opengl/shaders.zig:8)
// RESTORED (task-14c-3, the OpenGL 14c stage): the reference's one `log.warn` call
// site in this file (`Shaders.init`'s post-pipeline recovery, opengl/shaders.zig:123)
// is live below. No `LEDGER.tsv` row claims it — its Metal twin is claimed by
// `S119-F1`, whose rustty column names only `metal/shaders.rs` — so it is restored
// under PROCEDURE.md blind spot 3 and reported under its own heading.
const LOG_SCOPE: &str = "opengl";

/// PORTS: the inferred error set of `Shaders.init` — a pipeline compile/link
/// failure surfacing through [`Pipeline::init`]. Rust has no globally-interned
/// error names, so `Shaders`'s error IS the pipeline module's error.
pub type Error = super::pipeline::Error;

// -------------------------------------------------------------------------------
// The resolved GLSL sources.
//
// `build.rs`'s Linux step pre-resolves each asset's `#include "common.glsl"` and
// writes the NUL-terminated result to `$OUT_DIR/glsl/*.glsl`; we embed them here
// as `&CStr` — the `[:0]const u8` that `loadShaderCode` (`shaders.zig:347`)
// returned. `from_bytes_with_nul_unchecked` is sound: the resolver appends exactly
// one trailing NUL and GLSL source contains no interior NUL.
//
// The reference `loadShaderCode` doc, carried with its port (this macro + the
// `build.rs` include resolver):
//
// /// Load shader code from the target path, processing `#include` directives.
// ///
// /// Comptime only for now, this code is really sloppy and makes a bunch of
// /// assumptions about things being well formed and file names not containing
// /// quote marks. If we ever want to process `#include`s for custom shaders
// /// then we need to write something better than this for it.
// -------------------------------------------------------------------------------

macro_rules! glsl {
    ($name:literal) => {
        // SAFETY: `build.rs` writes each resolved asset with exactly one trailing
        // NUL and no interior NUL (GLSL source has none).
        unsafe {
            CStr::from_bytes_with_nul_unchecked(include_bytes!(concat!(
                env!("OUT_DIR"),
                "/glsl/",
                $name
            )))
        }
    };
}

const FULL_SCREEN_V: &CStr = glsl!("full_screen.v.glsl");
const BG_COLOR_F: &CStr = glsl!("bg_color.f.glsl");
const CELL_BG_F: &CStr = glsl!("cell_bg.f.glsl");
const CELL_TEXT_V: &CStr = glsl!("cell_text.v.glsl");
const CELL_TEXT_F: &CStr = glsl!("cell_text.f.glsl");
const IMAGE_V: &CStr = glsl!("image.v.glsl");
const IMAGE_F: &CStr = glsl!("image.f.glsl");
const BG_IMAGE_V: &CStr = glsl!("bg_image.v.glsl");
const BG_IMAGE_F: &CStr = glsl!("bg_image.f.glsl");

// -------------------------------------------------------------------------------
// The hand-written `autoAttribute` tables (HARD SPOT).
//
// Each row reproduces `Pipeline.zig:99-162`'s `IT -> attributeIFormat/
// attributeFormat` switch on the field's backing/tag integer type, and each
// `offset` reproduces `@offsetOf`. All integer-typed fields use `attributeIFormat`
// (`AttribClass::Integer`); all `f32` fields use `attributeFormat`
// (`AttribClass::Float`, `normalized = false`). None of these structs has an `f64`
// field, so `AttribClass::Long` is unused here.
// -------------------------------------------------------------------------------

/// `cell_text` vertex (`super::gpu_abi::CellText`, `shaders.zig:234-257`).
const CELL_TEXT_ATTRS: &[AttribDesc] = &[
    AttribDesc {
        class: AttribClass::Integer,
        size: 2,
        typ: gl::UNSIGNED_INT,
        offset: 0,
    }, // glyph_pos:  [2]u32
    AttribDesc {
        class: AttribClass::Integer,
        size: 2,
        typ: gl::UNSIGNED_INT,
        offset: 8,
    }, // glyph_size: [2]u32
    AttribDesc {
        class: AttribClass::Integer,
        size: 2,
        typ: gl::SHORT,
        offset: 16,
    }, // bearings:   [2]i16
    AttribDesc {
        class: AttribClass::Integer,
        size: 2,
        typ: gl::UNSIGNED_SHORT,
        offset: 20,
    }, // grid_pos:   [2]u16
    AttribDesc {
        class: AttribClass::Integer,
        size: 4,
        typ: gl::UNSIGNED_BYTE,
        offset: 24,
    }, // color:      [4]u8
    AttribDesc {
        class: AttribClass::Integer,
        size: 1,
        typ: gl::UNSIGNED_BYTE,
        offset: 28,
    }, // atlas:      Atlas enum(u8)
    AttribDesc {
        class: AttribClass::Integer,
        size: 1,
        typ: gl::UNSIGNED_BYTE,
        offset: 29,
    }, // bools:      packed(u8)
];
const CELL_TEXT_LAYOUT: VertexLayout = VertexLayout {
    attrs: CELL_TEXT_ATTRS,
    stride: core::mem::size_of::<CellText>(),
};

/// `image` vertex (`super::gpu_abi::Image`, `shaders.zig:263-268`).
const IMAGE_ATTRS: &[AttribDesc] = &[
    AttribDesc {
        class: AttribClass::Float,
        size: 2,
        typ: gl::FLOAT,
        offset: 0,
    }, // grid_pos:    [2]f32
    AttribDesc {
        class: AttribClass::Float,
        size: 2,
        typ: gl::FLOAT,
        offset: 8,
    }, // cell_offset: [2]f32
    AttribDesc {
        class: AttribClass::Float,
        size: 4,
        typ: gl::FLOAT,
        offset: 16,
    }, // source_rect: [4]f32
    AttribDesc {
        class: AttribClass::Float,
        size: 2,
        typ: gl::FLOAT,
        offset: 32,
    }, // dest_size:   [2]f32
];
const IMAGE_LAYOUT: VertexLayout = VertexLayout {
    attrs: IMAGE_ATTRS,
    stride: core::mem::size_of::<Image>(),
};

/// `bg_image` vertex (`super::gpu_abi::BgImage`, `shaders.zig:271-300`).
const BG_IMAGE_ATTRS: &[AttribDesc] = &[
    AttribDesc {
        class: AttribClass::Float,
        size: 1,
        typ: gl::FLOAT,
        offset: 0,
    }, // opacity: f32
    AttribDesc {
        class: AttribClass::Integer,
        size: 1,
        typ: gl::UNSIGNED_BYTE,
        offset: 4,
    }, // info:    Info packed(u8)
];
const BG_IMAGE_LAYOUT: VertexLayout = VertexLayout {
    attrs: BG_IMAGE_ATTRS,
    stride: core::mem::size_of::<BgImage>(),
};

// Drift guards: pin every table offset/stride to `super::gpu_abi` (a failed const
// panics at build time), so the hand-written tables can never silently drift from
// the ABI structs — the `metal::shaders` doctrine.
const _: () = {
    // CellText.
    assert!(CELL_TEXT_LAYOUT.stride == 32);
    assert!(CELL_TEXT_ATTRS[0].offset as usize == core::mem::offset_of!(CellText, glyph_pos));
    assert!(CELL_TEXT_ATTRS[1].offset as usize == core::mem::offset_of!(CellText, glyph_size));
    assert!(CELL_TEXT_ATTRS[2].offset as usize == core::mem::offset_of!(CellText, bearings));
    assert!(CELL_TEXT_ATTRS[3].offset as usize == core::mem::offset_of!(CellText, grid_pos));
    assert!(CELL_TEXT_ATTRS[4].offset as usize == core::mem::offset_of!(CellText, color));
    assert!(CELL_TEXT_ATTRS[5].offset as usize == core::mem::offset_of!(CellText, atlas));
    assert!(CELL_TEXT_ATTRS[6].offset as usize == core::mem::offset_of!(CellText, bools));

    // Image.
    assert!(IMAGE_LAYOUT.stride == 48);
    assert!(IMAGE_ATTRS[0].offset as usize == core::mem::offset_of!(Image, grid_pos));
    assert!(IMAGE_ATTRS[1].offset as usize == core::mem::offset_of!(Image, cell_offset));
    assert!(IMAGE_ATTRS[2].offset as usize == core::mem::offset_of!(Image, source_rect));
    assert!(IMAGE_ATTRS[3].offset as usize == core::mem::offset_of!(Image, dest_size));

    // BgImage.
    assert!(BG_IMAGE_LAYOUT.stride == 8);
    assert!(BG_IMAGE_ATTRS[0].offset as usize == core::mem::offset_of!(BgImage, opacity));
    assert!(BG_IMAGE_ATTRS[1].offset as usize == core::mem::offset_of!(BgImage, info));
};

/// All the comptime-known info about a pipeline.
///
/// PORTS: `opengl/shaders.zig:45-62` — `PipelineDescription`. The comptime
/// `vertex_attributes: ?type` becomes `Option<VertexLayout>` (the runtime table);
/// `step_fn`'s Zig default `.per_vertex` and `vertex_attributes`' default `null`
/// are supplied explicitly at each `PIPELINE_DESCS` entry.
struct PipelineDescription {
    vertex_attributes: Option<VertexLayout>,
    vertex_fn: &'static CStr,
    fragment_fn: &'static CStr,
    step_fn: StepFunction,
    blending_enabled: bool,
}

impl PipelineDescription {
    /// PORTS: `PipelineDescription.initPipeline` (`shaders.zig:54-61`).
    fn init_pipeline(&self) -> Result<Pipeline, Error> {
        Pipeline::init(
            self.vertex_attributes,
            PipelineOptions {
                vertex_fn: self.vertex_fn,
                fragment_fn: self.fragment_fn,
                step_fn: self.step_fn,
                blending_enabled: self.blending_enabled,
            },
        )
    }
}

/// PORTS: `opengl/shaders.zig:10-43` — the `pipeline_descs` table, flattened to a
/// module `const`. The `cell_text`/`image`/`bg_image` entries carry their
/// [`VertexLayout`]; `bg_color`/`cell_bg` (full-screen-triangle vertex fn, no
/// vertex input) carry `None`. Each `.0` name mirrors the reference's `pipeline[0]`
/// (the `@Type`-synthesized field name).
const PIPELINE_DESCS: &[(&str, PipelineDescription)] = &[
    (
        "bg_color",
        PipelineDescription {
            vertex_attributes: None,
            vertex_fn: FULL_SCREEN_V,
            fragment_fn: BG_COLOR_F,
            step_fn: StepFunction::PerVertex,
            blending_enabled: false,
        },
    ),
    (
        "cell_bg",
        PipelineDescription {
            vertex_attributes: None,
            vertex_fn: FULL_SCREEN_V,
            fragment_fn: CELL_BG_F,
            step_fn: StepFunction::PerVertex,
            blending_enabled: true,
        },
    ),
    (
        "cell_text",
        PipelineDescription {
            vertex_attributes: Some(CELL_TEXT_LAYOUT),
            vertex_fn: CELL_TEXT_V,
            fragment_fn: CELL_TEXT_F,
            step_fn: StepFunction::PerInstance,
            blending_enabled: true,
        },
    ),
    (
        "image",
        PipelineDescription {
            vertex_attributes: Some(IMAGE_LAYOUT),
            vertex_fn: IMAGE_V,
            fragment_fn: IMAGE_F,
            step_fn: StepFunction::PerInstance,
            blending_enabled: true,
        },
    ),
    (
        "bg_image",
        PipelineDescription {
            vertex_attributes: Some(BG_IMAGE_LAYOUT),
            vertex_fn: BG_IMAGE_V,
            fragment_fn: BG_IMAGE_F,
            step_fn: StepFunction::PerInstance,
            blending_enabled: true,
        },
    ),
];

/// The collection of available render pipelines.
///
/// PORTS: `opengl/shaders.zig:64-82` — the `@Type`-synthesized `PipelineCollection`,
/// hand-written with the exact field names read by generic
/// (`shaders.pipelines.cell_text` etc.), so they are load-bearing.
pub struct PipelineCollection {
    pub bg_color: Pipeline,
    pub cell_bg: Pipeline,
    pub cell_text: Pipeline,
    pub image: Pipeline,
    pub bg_image: Pipeline,
}

/// This contains the state for the shaders used by the OpenGL renderer.
///
/// PORTS: `opengl/shaders.zig:84-160`.
pub struct Shaders {
    /// Collection of available render pipelines.
    pub pipelines: PipelineCollection,

    /// Custom shaders to run against the final drawable texture. Each shader is
    /// run in sequence against the output of the previous shader.
    //
    // PORT NOTE: Zig `[]const Pipeline` + `alloc.alloc`; a `Vec` owns it.
    pub post_pipelines: Vec<Pipeline>,

    /// Set to true when deinited; a double-deinit of a defunct set is ignored, to
    /// prevent double-free.
    pub defunct: bool,
}

impl Shaders {
    /// Initialize our shader set.
    ///
    /// PORTS: `opengl/shaders.zig:103-141` — `Shaders.init`. `post_shaders` is a
    /// list of postprocess shader **source code** (not file paths).
    //
    // PORT NOTE (ownership): Zig takes an `Allocator` (for `post_pipelines`); Rust
    // owns via `Vec` Drop, so the `alloc` param dissolves. Zig's `errdefer inline
    // for (pipeline_descs) … deinit()` unwind on a partial-init failure dissolves
    // into `?` early-return + the manual per-field cleanup below (there is no
    // `Drop` on `Pipeline` — the `gl` handles are `Copy`, exactly as
    // `opengl::pipeline` documents).
    pub fn init(post_shaders: &[&CStr]) -> Result<Self, Error> {
        // Build each pipeline. On a later failure, deinit the already-built ones
        // (the reference's `errdefer inline for … deinit()`).
        let bg_color = PIPELINE_DESCS[0].1.init_pipeline()?;
        let cell_bg = match PIPELINE_DESCS[1].1.init_pipeline() {
            Ok(p) => p,
            Err(e) => {
                bg_color.deinit();
                return Err(e);
            }
        };
        let cell_text = match PIPELINE_DESCS[2].1.init_pipeline() {
            Ok(p) => p,
            Err(e) => {
                bg_color.deinit();
                cell_bg.deinit();
                return Err(e);
            }
        };
        let image = match PIPELINE_DESCS[3].1.init_pipeline() {
            Ok(p) => p,
            Err(e) => {
                bg_color.deinit();
                cell_bg.deinit();
                cell_text.deinit();
                return Err(e);
            }
        };
        let bg_image = match PIPELINE_DESCS[4].1.init_pipeline() {
            Ok(p) => p,
            Err(e) => {
                bg_color.deinit();
                cell_bg.deinit();
                cell_text.deinit();
                image.deinit();
                return Err(e);
            }
        };

        let pipelines = PipelineCollection {
            bg_color,
            cell_bg,
            cell_text,
            image,
            bg_image,
        };

        let post_pipelines = init_post_pipelines(post_shaders).unwrap_or_else(|err| {
            // If an error happens while building postprocess shaders we want to
            // just not use any postprocess shaders since we don't want to block
            // Ghostty from working.
            // log.warn("error initializing postprocess shaders err={}", .{err});
            log::warn!(target: LOG_SCOPE, "error initializing postprocess shaders err={err:?}");
            Vec::new()
        });

        Ok(Self {
            pipelines,
            post_pipelines,
            defunct: false,
        })
    }

    /// PORTS: `opengl/shaders.zig:143-159` — `Shaders.deinit`. The `defunct`
    /// double-free guard + per-field program release.
    //
    // PORT NOTE (ownership): Zig takes an `Allocator` (frees `post_pipelines`) and
    // releases each pipeline explicitly. The `gl` handles are `Copy` (no `Drop`),
    // so this ports the explicit `.deinit()` calls whole; the `Vec` frees its own
    // backing store on drop. The `defunct` flag is kept (generic asserts on it).
    pub fn deinit(&mut self) {
        if self.defunct {
            return;
        }
        self.defunct = true;

        // Release our primary shaders.
        self.pipelines.bg_color.deinit();
        self.pipelines.cell_bg.deinit();
        self.pipelines.cell_text.deinit();
        self.pipelines.image.deinit();
        self.pipelines.bg_image.deinit();

        // Release our postprocess shaders.
        for pipeline in &self.post_pipelines {
            pipeline.deinit();
        }
        self.post_pipelines.clear();
    }
}

/// Initialize our custom shader pipelines. The shaders argument is a
/// set of shader source code, not file paths.
///
/// PORTS: `opengl/shaders.zig:302-331` — `initPostPipelines`.
//
// PORT NOTE (ownership): Zig's `alloc.alloc(Pipeline, …)` + `errdefer` partial-free
// dissolves into a `Vec` whose `?`-propagated error deinits the pipelines built so
// far before returning. Milestone-inert: called with an empty `post_shaders`, so
// the early `return Ok(vec![])` is the only path exercised.
fn init_post_pipelines(shaders: &[&CStr]) -> Result<Vec<Pipeline>, Error> {
    // If we have no shaders, do nothing.
    if shaders.is_empty() {
        return Ok(Vec::new());
    }

    // Keeps track of how many shaders we successfully wrote.
    // (PORT NOTE: the `Vec`'s `len()` is the reference `i`.)
    //
    // Initialize our result set. If any error happens, we undo everything.
    let mut pipelines: Vec<Pipeline> = Vec::with_capacity(shaders.len());
    // Build each shader. Note we don't use "0.." to build our index
    // because we need to keep track of our length to clean up above.
    for source in shaders {
        match init_post_pipeline(source) {
            Ok(p) => pipelines.push(p),
            Err(e) => {
                // errdefer: undo everything built so far.
                for pipeline in &pipelines {
                    pipeline.deinit();
                }
                return Err(e);
            }
        }
    }

    Ok(pipelines)
}

/// Initialize a single custom shader pipeline from shader source.
///
/// PORTS: `opengl/shaders.zig:333-339` — `initPostPipeline`.
fn init_post_pipeline(data: &CStr) -> Result<Pipeline, Error> {
    Pipeline::init(
        None,
        PipelineOptions {
            vertex_fn: FULL_SCREEN_V,
            fragment_fn: data,
            // Zig omits `step_fn`/`blending_enabled` → their Zig defaults
            // (`.per_vertex`, `true`).
            step_fn: StepFunction::PerVertex,
            blending_enabled: true,
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::opengl::gl::errors::get_error;
    use crate::opengl::gl::test_support;

    // FNV-1a-64 over `bytes`. A dependency-free content hash (R3 forbids new deps)
    // used by `glsl_assets_match_reference_byte_for_byte` to pin each vendored GLSL
    // asset to the exact bytes of the pinned ghostty reference — a check that runs
    // EVERYWHERE (it needs no GL context and no external tree).
    const fn fnv1a_64(bytes: &[u8]) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut i = 0;
        while i < bytes.len() {
            hash ^= bytes[i] as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            i += 1;
        }
        hash
    }

    /// The strongest per-task R1 gate (the brief): each of the 10 GLSL assets
    /// vendored into `src/shaders/glsl/` must be BYTE-IDENTICAL to the pinned
    /// ghostty reference (`src/renderer/shaders/glsl/*.glsl` @ 4749c4e9). The
    /// vendored bytes are embedded at compile time via `include_bytes!` and pinned
    /// to the reference's FNV-1a-64 content hash (computed once from the pinned
    /// tree). This runs on every host — no GL context, no external tree needed —
    /// so it NEVER skips.
    ///
    /// When the pinned reference tree IS present (this host — the `GHOSTTY`
    /// env var or its default), the test additionally diffs the vendored bytes
    /// against the live reference file, so a drift in EITHER the vendored copy or
    /// the pin is caught. When absent, only the hash pin runs (still byte-exact).
    #[test]
    fn glsl_assets_match_reference_byte_for_byte() {
        // (vendored bytes, reference FNV-1a-64, basename).
        let assets: &[(&[u8], u64, &str)] = &[
            (
                include_bytes!("../shaders/glsl/bg_color.f.glsl"),
                0xc01f_83ba_326b_053d,
                "bg_color.f.glsl",
            ),
            (
                include_bytes!("../shaders/glsl/bg_image.f.glsl"),
                0xd601_0767_0a66_f4ba,
                "bg_image.f.glsl",
            ),
            (
                include_bytes!("../shaders/glsl/bg_image.v.glsl"),
                0xf11f_8fb9_c8f0_0e9d,
                "bg_image.v.glsl",
            ),
            (
                include_bytes!("../shaders/glsl/cell_bg.f.glsl"),
                0x553d_3bbb_1851_609d,
                "cell_bg.f.glsl",
            ),
            (
                include_bytes!("../shaders/glsl/cell_text.f.glsl"),
                0xdd0f_b5a2_0979_02b2,
                "cell_text.f.glsl",
            ),
            (
                include_bytes!("../shaders/glsl/cell_text.v.glsl"),
                0x543f_f63b_b91f_e6f9,
                "cell_text.v.glsl",
            ),
            (
                include_bytes!("../shaders/glsl/common.glsl"),
                0x4b27_9291_393c_a6cf,
                "common.glsl",
            ),
            (
                include_bytes!("../shaders/glsl/full_screen.v.glsl"),
                0xeb62_238b_2c00_86d7,
                "full_screen.v.glsl",
            ),
            (
                include_bytes!("../shaders/glsl/image.f.glsl"),
                0xd929_60c5_6e0c_24d6,
                "image.f.glsl",
            ),
            (
                include_bytes!("../shaders/glsl/image.v.glsl"),
                0xbfab_92d9_7067_06a4,
                "image.v.glsl",
            ),
        ];

        // Always-on gate: vendored bytes match the pinned reference hash.
        for (bytes, want_hash, name) in assets {
            assert_eq!(
                fnv1a_64(bytes),
                *want_hash,
                "vendored src/shaders/glsl/{name} drifted from the pinned ghostty reference",
            );
        }

        // Extra gate when the pinned reference tree is available: direct byte diff.
        let ghostty =
            std::env::var("GHOSTTY").unwrap_or_else(|_| "/home/gael/projects/ghostty".into());
        let ref_dir = std::path::Path::new(&ghostty).join("src/renderer/shaders/glsl");
        if !ref_dir.is_dir() {
            eprintln!(
                "pinned reference tree absent ({}); hash-pin gate ran, direct diff skipped",
                ref_dir.display()
            );
            return;
        }
        for (bytes, _, name) in assets {
            let ref_bytes = std::fs::read(ref_dir.join(name))
                .unwrap_or_else(|e| panic!("reading reference {name}: {e}"));
            assert_eq!(
                &ref_bytes[..],
                *bytes,
                "vendored src/shaders/glsl/{name} differs byte-for-byte from the reference",
            );
        }
    }

    /// PORTS the shape of `Shaders.init` (`opengl/shaders.zig:103-141`):
    /// `Shaders::init(&[])` succeeds against a real GL context, all five named
    /// pipeline fields are populated (each holds a compiled+linked program), and
    /// `post_pipelines` is empty. Skips cleanly (never panics) when the host has no
    /// OpenGL 4.3 core context — see [`test_support::offscreen_gl`].
    #[test]
    fn opengl_shaders_init_builds_five_pipelines() {
        let Some(_gl) = test_support::offscreen_gl("opengl_shaders_init_builds_five_pipelines")
        else {
            return;
        };
        assert!(get_error().is_ok(), "context started in a GL error state");

        let mut shaders = Shaders::init(&[]).expect("Shaders::init(&[])");
        assert!(get_error().is_ok(), "after Shaders::init");

        // All five named pipeline fields hold a real linked program (handle != 0).
        assert_ne!(shaders.pipelines.bg_color.program.id, 0);
        assert_ne!(shaders.pipelines.cell_bg.program.id, 0);
        assert_ne!(shaders.pipelines.cell_text.program.id, 0);
        assert_ne!(shaders.pipelines.image.program.id, 0);
        assert_ne!(shaders.pipelines.bg_image.program.id, 0);

        // Milestone: no post-processing shaders.
        assert!(shaders.post_pipelines.is_empty());

        shaders.deinit();
        // Idempotent: a second deinit on a defunct set is a no-op.
        shaders.deinit();
        assert!(get_error().is_ok(), "after Shaders::deinit");
    }
}
