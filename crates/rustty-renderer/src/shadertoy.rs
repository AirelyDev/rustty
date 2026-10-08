//! rustty-renderer shadertoy
//!
//! PORTS: ghostty src/renderer/shadertoy.zig
//!
//! PORT STATUS: ported (R1). The self-contained custom-shader TRANSPILE path —
//! GLSL -> (glslang) -> SPIR-V -> (spirv-cross) -> MSL/GLSL — is fully ported
//! against the vendored `glslang-sys` + `spirv-cross-sys` FFI, with all 4
//! `test{}` blocks (`spirv`, `spirv invalid`, `shadertoy to msl`,
//! `shadertoy to glsl`).
//!
//! `loadFromFiles` / `loadFromFile` are ALSO ported now (custom-shader milestone).
//! They take a `configpkg.RepeatablePath` (`@import("../config.zig")`); the crate
//! split forbids a `rustty-renderer -> rustty-config` edge (`rustty-config` ALREADY
//! deps `rustty-renderer` — the `Message::ChangeConfig` back-edge — so it would
//! cycle, G7 RED). Resolved the same way as the background-image `Path`: the load
//! path takes the renderer-local [`crate::generic::RepeatablePath`] (a `Vec<Path>`
//! over the renderer-local byte-faithful `CString` `Path`), and `Config` maps its
//! own `RepeatablePath` into it via `RendererConfig::custom_shaders` (config-side,
//! acyclic). No new crate edge; `check_acyclic.sh` stays green.
//
// const std = @import("std");
// const builtin = @import("builtin");
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
// const glslang = @import("glslang");
// const spvcross = @import("spirv_cross");
// const configpkg = @import("../config.zig");
//
// PORT NOTE: the reference imports the pkg/glslang + pkg/spirv-cross SAFE
// wrappers (`glslang.Shader`, `glslang.Program`, `glslang.init`,
// `glslang.testing.ensureInit`, `glslang.c`, `spvcross.c`) layered over the raw
// C. The rustty tree vendors only the raw `glslang-sys` / `spirv-cross-sys` FFI
// (no separate safe-wrapper crates), so the handful of wrapper methods
// shadertoy.zig actually uses are inlined here as private helpers, byte-faithful
// to pkg/glslang/{init,shader,program,test}.zig. `glslang.c` -> `gl`,
// `spvcross.c` -> `spvc`.
use glslang_sys as gl;
use spirv_cross_sys as spvc;

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

// The renderer-local config types the load path consumes (acyclic — see the module
// doc): `RepeatablePath` is a `Vec<Path>` over the byte-faithful `CString` `Path`.
use crate::generic::{Path, RepeatablePath};

// PORT NOTE: `cargo test` runs tests concurrently across threads, but the
// vendored glslang / SPIRV-Cross C++ libraries carry process-global state that
// is NOT safe for concurrent compilation (`zig test` runs serially, so the
// reference never hits this). Serialize the whole transpile FFI (`spirv_from_glsl`
// AND `spv_cross`) behind one process-global lock. Custom-shader compilation
// happens at renderer startup (rarely concurrent), so the production cost is
// negligible.
static SHADER_FFI_LOCK: Mutex<()> = Mutex::new(());

// const log = std.log.scoped(.shadertoy);
// PORT NOTE (task-14c-2): the `log` facade is wired for this crate
// (`Cargo.toml`'s `log = { workspace = true }`; DECISION-logging.md §6r call
// shape 3); all THREE of `shadertoy.zig`'s live `log` sites are RESTORED beneath
// their R1 comments. (`shadertoy.zig:424`'s `// log.warn("glsl={s}", .{glsl});` is
// commented in the REFERENCE too, so it stays a comment here — un-commenting it
// would be a divergence, not a restoration.)
//
// R1 note on `{s}` over byte slices: the reference's paths and spirv-cross logs are
// raw `[]const u8`. The lossy UTF-8 conversion below is confined to the MESSAGE;
// no ported datum is routed through it (`zig-byteslice-to-rust-rawbytes` governs
// data, not diagnostics — the same carve-out 14c-1 took in `generic.rs`).
/// `shadertoy.zig:11`'s `const log = std.log.scoped(.shadertoy);`.
const LOG_SCOPE: &str = "shadertoy";

/// S117-F1: an align(16)-carrying newtype over `[f32; 4]`.
///
/// std140 requires every vec4 (and every std140-array element — GLSL rounds an
/// array element's stride up to a vec4 multiple, which is exactly why the oracle
/// spells e.g. `channel_time`/`palette` as `[N][4]f32` rather than `[N][3]f32` or
/// `[N]f32`) onto a 16-byte boundary. The oracle expresses this as a per-field
/// `align(16)` on an `extern struct` (ghostty src/renderer/shadertoy.zig:14-42);
/// Rust has no per-field alignment, so where Rust's own natural (4-byte, from the
/// `f32`/`i32` element types) field placement would NOT already land the next
/// `align(16)` member on a 16-byte offset, the alignment has to ride on the
/// member's own type instead.
///
/// Deriving the std140 offsets field-by-field (see the task-5 report for the
/// full working) shows this is load-bearing for exactly THREE members of
/// `Uniforms`: `channel_time` (natural offset 28, needs 32), `current_cursor`
/// (natural 196, needs 208), and `palette` (natural 296, needs 304). Every other
/// `align(16)` member in the oracle already lands on a 16-byte offset under plain
/// natural alignment, because the preceding same-alignment run's total size is
/// itself a multiple of 16 (e.g. `mouse`/`date` follow two back-to-back 64-byte
/// `[4][4]f32` groups). `previous_cursor` is ALSO given this type — not because
/// its own offset (224) needs forcing, but so it stays the same type as
/// `current_cursor`, which `generic.rs`'s `u.previous_cursor = u.current_cursor`
/// assignment requires.
#[repr(C, align(16))]
#[derive(Clone, Copy, Debug, Default)]
pub struct Vec4(pub [f32; 4]);

impl From<[f32; 4]> for Vec4 {
    fn from(v: [f32; 4]) -> Self {
        Vec4(v)
    }
}

impl PartialEq for Vec4 {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

// So call sites (and the `generic.rs` unit tests) can compare/construct against a
// bare `[f32; 4]` literal without an explicit `.into()`/`Vec4(..)` wrap everywhere.
impl PartialEq<[f32; 4]> for Vec4 {
    fn eq(&self, other: &[f32; 4]) -> bool {
        self.0 == *other
    }
}

/// The uniform struct used for shadertoy shaders.
//
// PORT NOTE: the reference is an `extern struct` with per-field `align(16)`
// (vec3/vec4/matrix members) and `align(4)` (scalar members) matching the
// std140 GPU-uniform layout. Rust cannot express per-field alignment, so the
// three members that actually need a forced 16-byte offset jump (see the `Vec4`
// doc above) are retyped through `Vec4`; every other field keeps its natural
// `[f32; N]`/scalar type because natural alignment already puts it at the
// correct std140 offset. The const-asserts below pin the layout the GPU draw
// path (`generic.rs:1893`/`6668-6672`) memcpy's straight to the GPU.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Uniforms {
    pub resolution: [f32; 3],    // align(16); offset 0 needs no forcing
    pub time: f32,               // align(4)
    pub time_delta: f32,         // align(4)
    pub frame_rate: f32,         // align(4)
    pub frame: i32,              // align(4)
    pub channel_time: [Vec4; 4], // align(16) — FORCED: 28 -> 32
    pub channel_resolution: [[f32; 4]; 4], // align(16); offset 96 needs no forcing
    pub mouse: [f32; 4],         // align(16); offset 160 needs no forcing
    pub date: [f32; 4],          // align(16); offset 176 needs no forcing
    pub sample_rate: f32,        // align(4)
    pub current_cursor: Vec4,    // align(16) — FORCED: 196 -> 208
    pub previous_cursor: Vec4,   // align(16); offset 224 needs no forcing (see Vec4 doc)
    pub current_cursor_color: [f32; 4], // align(16); offset 240 needs no forcing
    pub previous_cursor_color: [f32; 4], // align(16); offset 256 needs no forcing
    pub current_cursor_style: i32, // align(4)
    pub previous_cursor_style: i32, // align(4)
    pub cursor_visible: i32,     // align(4)
    pub cursor_change_time: f32, // align(4)
    pub time_focus: f32,         // align(4)
    pub focus: i32,              // align(4)
    pub palette: [Vec4; 256],    // align(16) — FORCED: 296 -> 304
    pub background_color: [f32; 4], // align(16); offset 4400 needs no forcing
    pub foreground_color: [f32; 4], // align(16); offset 4416 needs no forcing
    pub cursor_color: [f32; 4],  // align(16); offset 4432 needs no forcing
    pub cursor_text: [f32; 4],   // align(16); offset 4448 needs no forcing
    // `selection_foreground_color` is declared before `selection_background_color`, the
    // order of `iSelectionForegroundColor` / `iSelectionBackgroundColor` in the GLSL prefix
    // (`shaders/shadertoy_prefix.glsl`), so each uniform receives the colour it is named for
    // (upstream `80456f057`). Tests
    // `custom_shader_selection_uniform_layout_matches_compiled_glsl` and
    // `globals_block_members_receive_the_uniform_fields_they_are_named_for`.
    pub selection_foreground_color: [f32; 4], // align(16); offset 4464 needs no forcing
    pub selection_background_color: [f32; 4], // align(16); offset 4480 needs no forcing
}

// S117-F1 layout pins: compile-time freeze so the std140 offsets/size can never
// silently drift — not just for the three FORCED members, but for every member
// the oracle marks `align(16)`. This matters because the three forced members
// (`channel_time`/`current_cursor`/`palette`) act as *absorbers*: an unrelated
// layout break inserted anywhere between two forced members (e.g. widening a
// scalar, reordering a field) would shift every *unforced* member's offset
// while still leaving the forced offsets — and even `size_of` — unchanged,
// so pinning only those 3 lets a broken layout pass silently. Pinning all 16
// `align(16)` members' offsets (plus size and struct alignment) closes that
// gap. Derived field-by-field from the oracle's `align(16)` annotations
// (ghostty src/renderer/shadertoy.zig:14-42); cross-checked against the
// `Globals` block in shaders/shadertoy_prefix.glsl.
const _: () = assert!(std::mem::offset_of!(Uniforms, resolution) == 0);
const _: () = assert!(std::mem::offset_of!(Uniforms, channel_time) == 32);
const _: () = assert!(std::mem::offset_of!(Uniforms, channel_resolution) == 96);
const _: () = assert!(std::mem::offset_of!(Uniforms, mouse) == 160);
const _: () = assert!(std::mem::offset_of!(Uniforms, date) == 176);
const _: () = assert!(std::mem::offset_of!(Uniforms, current_cursor) == 208);
const _: () = assert!(std::mem::offset_of!(Uniforms, previous_cursor) == 224);
const _: () = assert!(std::mem::offset_of!(Uniforms, current_cursor_color) == 240);
const _: () = assert!(std::mem::offset_of!(Uniforms, previous_cursor_color) == 256);
const _: () = assert!(std::mem::offset_of!(Uniforms, palette) == 304);
const _: () = assert!(std::mem::offset_of!(Uniforms, background_color) == 4400);
const _: () = assert!(std::mem::offset_of!(Uniforms, foreground_color) == 4416);
const _: () = assert!(std::mem::offset_of!(Uniforms, cursor_color) == 4432);
const _: () = assert!(std::mem::offset_of!(Uniforms, cursor_text) == 4448);
// The selection pair is in the GLSL order (upstream `80456f057`).
const _: () = assert!(std::mem::offset_of!(Uniforms, selection_foreground_color) == 4464);
const _: () = assert!(std::mem::offset_of!(Uniforms, selection_background_color) == 4480);
const _: () = assert!(std::mem::size_of::<Uniforms>() == 4496);
const _: () = assert!(std::mem::align_of::<Uniforms>() == 16);

/// The target to load shaders for.
// pub const Target = enum { glsl, msl };
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Target {
    Glsl,
    Msl,
}

/// The error set for the shadertoy transpile path.
//
// PORT NOTE: the reference uses Zig error unions (`error.GlslangFailed`,
// `error.SpvcFailed`, `error.SpirvInvalid`, `error.OutOfMemory`,
// `error.GlslangInitFailed`, `error.FileNotFound`, `error.StreamTooLong`);
// collected here into one enum. `FileNotFound`/`StreamTooLong`/`Io` are used
// by the `load_from_file*` load path.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Error {
    GlslangInitFailed,
    GlslangFailed,
    OutOfMemory,
    SpvcFailed,
    SpirvInvalid,
    FileNotFound,
    StreamTooLong,
    Io,
}

/// Load a set of shaders from files and convert them to the target
/// format. The shader order is preserved.
// pub fn loadFromFiles(alloc_gpa, paths: configpkg.RepeatablePath, target) ![]const [:0]const u8
//
// PORT NOTE: `paths: configpkg.RepeatablePath` -> the renderer-local
// `&RepeatablePath` (config-coupled type routed through the renderer-local mirror,
// acyclic — see the module doc). Returns `Vec<CString>` (the reference `[]const
// [:0]const u8` — owned, NUL-terminated shader sources; byte-faithful, never lossy
// `String`). The reference `alloc_gpa` + `errdefer free(shader)` dissolve into
// `Vec`/`CString` Drop (a mid-loop error drops the already-built sources
// automatically).
pub fn load_from_files(paths: &RepeatablePath, target: Target) -> Result<Vec<CString>, Error> {
    // var list: std.ArrayList([:0]const u8) = .empty;
    // defer list.deinit(alloc_gpa);
    // errdefer for (list.items) |shader| alloc_gpa.free(shader);
    let mut list: Vec<CString> = Vec::new();

    // for (paths.value.items) |item| {
    for item in &paths.value {
        // const path, const optional = switch (item) {
        //     .optional => |path| .{ path, true },
        //     .required => |path| .{ path, false },
        // };
        let (path, optional): (&CStr, bool) = match item {
            Path::Optional(p) => (p.as_c_str(), true),
            Path::Required(p) => (p.as_c_str(), false),
        };

        // const shader = loadFromFile(alloc_gpa, path, target) catch |err| {
        //     if (err == error.FileNotFound and optional) {
        //         continue;
        //     }
        //
        //     return err;
        // };
        let shader = match load_from_file(path, target) {
            Ok(s) => s,
            Err(Error::FileNotFound) if optional => continue,
            Err(err) => return Err(err),
        };
        // log.info("loaded custom shader path={s}", .{path});
        log::info!(
            target: LOG_SCOPE,
            "loaded custom shader path={}",
            String::from_utf8_lossy(path.to_bytes())
        );
        // try list.append(alloc_gpa, shader);
        list.push(shader);
    }

    // return try list.toOwnedSlice(alloc_gpa);
    Ok(list)
}

/// Load a single shader from a file and convert it to the target language
/// ready to be used with renderers.
// pub fn loadFromFile(alloc_gpa, path: []const u8, target) ![:0]const u8
//
// PORT NOTE: `path: []const u8` -> `&CStr` (the renderer-local `Path` carries a
// `CString`; the load reads the path bytes). Returns an owned NUL-terminated
// `CString` (reference `[:0]const u8`). The reference arena (`src`/`glsl`/`spirv`
// intermediates) becomes local `Vec`s dropped at scope end; the final result is an
// owned `CString` returned to the caller.
pub fn load_from_file(path: &CStr, target: Target) -> Result<CString, Error> {
    // var arena = ArenaAllocator.init(alloc_gpa);
    // defer arena.deinit();
    // const alloc = arena.allocator();

    // Read it all into memory -- we don't expect shaders to be large.
    // const src = src: {
    //     // Load the shader file
    //     const cwd = std.fs.cwd();
    //     const file = try cwd.openFile(path, .{});
    //     defer file.close();
    //
    //     break :src try file.readToEndAlloc(alloc, 4 * 1024 * 1024); // 4MB
    // };
    //
    // PORT NOTE: `std.fs.cwd().openFile(path)` over the raw path bytes. On unix the
    // path bytes map to `OsStr` via `OsStrExt::from_bytes` (byte-faithful — the
    // reference opens the exact bytes, never a lossy `String`). `error.FileNotFound`
    // -> `Error::FileNotFound` (drives the optional-arm skip in `load_from_files`);
    // other I/O -> `Error::Io`; exceeding the 4MB cap -> `Error::StreamTooLong`.
    let src: Vec<u8> = read_shader_file(path)?;

    // Convert to full GLSL
    // const glsl: [:0]const u8 = glsl: {
    //     var stream: std.Io.Writer.Allocating = .init(alloc);
    //     try glslFromShader(&stream.writer, src);
    //     try stream.writer.writeByte(0);
    //     break :glsl stream.written()[0 .. stream.written().len - 1 :0];
    // };
    //
    // The trailing `writeByte(0)` is the NUL sentinel (`[:0]const u8`); `glsl` keeps
    // it (only `spirv_from_glsl`'s `src.as_ptr()` reaches C, so the sentinel byte is
    // the C-string terminator, exactly like the reference slice-minus-one aliasing).
    let mut glsl: Vec<u8> = Vec::new();
    glsl_from_shader(&mut glsl, &src);
    glsl.push(0);

    // Convert to SPIR-V
    // const spirv: []const u8 = spirv: {
    //     var stream: std.Io.Writer.Allocating = .init(alloc);
    //     var errlog: SpirvLog = .{ .alloc = alloc };
    //     defer errlog.deinit();
    //     spirvFromGlsl(&stream.writer, &errlog, glsl) catch |err| {
    //         if (errlog.info.len > 0 or errlog.debug.len > 0) {
    //             log.warn("spirv error path={s} info={s} debug={s}", .{
    //                 path, errlog.info, errlog.debug,
    //             });
    //         }
    //
    //         return err;
    //     };
    //
    //     // SpirV pointer must be aligned to 4 bytes since we expect
    //     // a slice of words.
    //     var list: std.ArrayListAligned(u8, .of(u32)) = .empty;
    //     try list.appendSlice(alloc, stream.written());
    //     break :spirv list.items;
    // };
    let mut spirv: Vec<u8> = Vec::new();
    let mut errlog = SpirvLog::default();
    if let Err(err) = spirv_from_glsl(&mut spirv, Some(&mut errlog), &glsl) {
        // if (errlog.info.len > 0 or errlog.debug.len > 0) {
        //     log.warn("spirv error path={s} info={s} debug={s}", .{
        //         path, errlog.info, errlog.debug,
        //     });
        // }
        if !errlog.info.is_empty() || !errlog.debug.is_empty() {
            log::warn!(
                target: LOG_SCOPE,
                "spirv error path={} info={} debug={}",
                String::from_utf8_lossy(path.to_bytes()),
                String::from_utf8_lossy(&errlog.info),
                String::from_utf8_lossy(&errlog.debug)
            );
        }
        return Err(err);
    }
    // (The reference re-packs into a u32-aligned list for the `@alignCast`; the
    // port's `spv_cross` already copies into a `Vec<u32>` internally, so `spirv` is
    // passed directly — the same subsumption the `shadertoy_to_msl` test documents.)

    // Convert to MSL
    // return switch (target) {
    //     // Important: using the alloc_gpa here on purpose because this
    //     // is the final result that will be returned to the caller.
    //     .glsl => try glslFromSpv(alloc_gpa, spirv),
    //     .msl => try mslFromSpv(alloc_gpa, spirv),
    // };
    let out: Vec<u8> = match target {
        Target::Glsl => glsl_from_spv(&spirv)?,
        Target::Msl => msl_from_spv(&spirv)?,
    };
    // The transpiled source is C-string text (no interior NUL — `c_str_to_vec` stops
    // at the first NUL), so `CString::new` cannot fail; map defensively.
    CString::new(out).map_err(|_| Error::SpvcFailed)
}

// PORT NOTE (helper): the reference's `openFile` + `readToEndAlloc(alloc, 4MB)`.
// Byte-faithful path handling (unix `OsStrExt::from_bytes`); 4MB cap -> StreamTooLong.
// On Windows the path bytes are WTF-8, converted as Zig's `openFile` does
// (`rustty_os::path::os_string_from_bytes`); a path that is not WTF-8 is Zig's
// `error.InvalidWtf8`, which this port's error set folds into `Io` like the
// other open errors.
fn read_shader_file(path: &CStr) -> Result<Vec<u8>, Error> {
    use std::io::Read;
    #[cfg(not(windows))]
    let os = {
        use std::os::unix::ffi::OsStrExt;
        std::ffi::OsStr::from_bytes(path.to_bytes())
    };
    #[cfg(windows)]
    let os = rustty_os::path::os_string_from_bytes(path.to_bytes()).ok_or(Error::Io)?;
    let mut file = std::fs::File::open(os).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => Error::FileNotFound,
        _ => Error::Io,
    })?;
    // readToEndAlloc(alloc, 4 * 1024 * 1024) — cap read at 4MB, else StreamTooLong.
    const MAX: u64 = 4 * 1024 * 1024;
    let mut buf = Vec::new();
    file.by_ref()
        .take(MAX + 1)
        .read_to_end(&mut buf)
        .map_err(|_| Error::Io)?;
    if buf.len() as u64 > MAX {
        return Err(Error::StreamTooLong);
    }
    Ok(buf)
}

/// Convert a ShaderToy shader into valid GLSL.
///
/// ShaderToy shaders aren't full shaders, they're just implementing a
/// mainImage function and don't define any of the uniforms. This function
/// will convert the ShaderToy shader into a valid GLSL shader that can be
/// compiled and linked.
//
// PORT NOTE: the reference takes `writer: *std.Io.Writer` and returns `!void`.
// The port writes into a `&mut Vec<u8>` (the faithful analogue of the
// `Writer.Allocating` used by every caller); `Vec` growth aborts on OOM, so it
// is infallible here.
pub fn glsl_from_shader(writer: &mut Vec<u8>, src: &[u8]) {
    // const prefix = @embedFile("shaders/shadertoy_prefix.glsl");
    const PREFIX: &[u8] = include_bytes!("shaders/shadertoy_prefix.glsl");
    // try writer.writeAll(prefix);
    writer.extend_from_slice(PREFIX);
    // try writer.writeAll("\n\n");
    writer.extend_from_slice(b"\n\n");
    // try writer.writeAll(src);
    writer.extend_from_slice(src);
}

/// Convert a GLSL shader into SPIR-V assembly.
//
// PORT NOTE: `src: [:0]const u8` is a NUL-terminated GLSL string; the port takes
// `src: &[u8]` whose trailing byte is the NUL sentinel (`test_glsl_z` /
// `loadFromFile` append it). Only `src.as_ptr()` is passed to C (as `input.code`),
// so the slice length is irrelevant. `errlog: ?*SpirvLog` -> `Option<&mut SpirvLog>`.
pub fn spirv_from_glsl(
    writer: &mut Vec<u8>,
    mut errlog: Option<&mut SpirvLog>,
    src: &[u8],
) -> Result<(), Error> {
    // Serialize the transpile FFI (see SHADER_FFI_LOCK PORT NOTE). Held for the
    // whole function; dropped (unlocked) on every return path.
    let _ffi_guard = SHADER_FFI_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // So we can run unit tests without fear.
    // if (builtin.is_test) try glslang.testing.ensureInit();
    if cfg!(test) {
        glslang_ensure_init()?;
    }

    // const c = glslang.c;
    // const input: c.glslang_input_t = .{ ... };
    //
    // PORT NOTE: the bindgen `glslang_input_t` carries two extra trailing fields
    // (`callbacks`, `callbacks_ctx`) that the reference leaves defaulted; zero-init
    // the struct then set the 13 fields the reference sets (mirrors glslang-sys KAT).
    let mut input: gl::glslang_input_t = unsafe { std::mem::zeroed() };
    input.language = gl::GLSLANG_SOURCE_GLSL as _;
    input.stage = gl::GLSLANG_STAGE_FRAGMENT as _;
    input.client = gl::GLSLANG_CLIENT_VULKAN as _;
    input.client_version = gl::GLSLANG_TARGET_VULKAN_1_2 as _;
    input.target_language = gl::GLSLANG_TARGET_SPV as _;
    input.target_language_version = gl::GLSLANG_TARGET_SPV_1_5 as _;
    input.code = src.as_ptr() as *const c_char;
    input.default_version = 100;
    input.default_profile = gl::GLSLANG_NO_PROFILE as _;
    input.force_default_version_and_profile = 0;
    input.forward_compatible = 0;
    input.messages = gl::GLSLANG_MSG_DEFAULT_BIT as _;
    input.resource = unsafe { gl::glslang_default_resource() };

    // const shader = try glslang.Shader.create(&input);
    // defer shader.delete();
    let shader = unsafe { gl::glslang_shader_create(&input) };
    if shader.is_null() {
        return Err(Error::OutOfMemory);
    }

    // shader.preprocess(&input) catch |err| {
    //     if (errlog) |ptr| ptr.fromShader(shader) catch {};
    //     return err;
    // };
    if unsafe { gl::glslang_shader_preprocess(shader, &input) } == 0 {
        if let Some(ptr) = &mut errlog {
            ptr.from_shader(shader);
        }
        unsafe { gl::glslang_shader_delete(shader) };
        return Err(Error::GlslangFailed);
    }
    // shader.parse(&input) catch |err| {
    //     if (errlog) |ptr| ptr.fromShader(shader) catch {};
    //     return err;
    // };
    if unsafe { gl::glslang_shader_parse(shader, &input) } == 0 {
        if let Some(ptr) = &mut errlog {
            ptr.from_shader(shader);
        }
        unsafe { gl::glslang_shader_delete(shader) };
        return Err(Error::GlslangFailed);
    }

    // const program = try glslang.Program.create();
    // defer program.delete();
    let program = unsafe { gl::glslang_program_create() };
    if program.is_null() {
        unsafe { gl::glslang_shader_delete(shader) };
        return Err(Error::OutOfMemory);
    }
    // program.addShader(shader);
    unsafe { gl::glslang_program_add_shader(program, shader) };
    // program.link(
    //     c.GLSLANG_MSG_SPV_RULES_BIT |
    //         c.GLSLANG_MSG_VULKAN_RULES_BIT,
    // ) catch |err| {
    //     if (errlog) |ptr| ptr.fromProgram(program) catch {};
    //     return err;
    // };
    if unsafe {
        gl::glslang_program_link(
            program,
            (gl::GLSLANG_MSG_SPV_RULES_BIT | gl::GLSLANG_MSG_VULKAN_RULES_BIT) as c_int,
        )
    } == 0
    {
        if let Some(ptr) = &mut errlog {
            ptr.from_program(program);
        }
        unsafe {
            gl::glslang_program_delete(program);
            gl::glslang_shader_delete(shader);
        }
        return Err(Error::GlslangFailed);
    }
    // program.spirvGenerate(c.GLSLANG_STAGE_FRAGMENT);
    unsafe { gl::glslang_program_SPIRV_generate(program, gl::GLSLANG_STAGE_FRAGMENT as _) };
    // const size = program.spirvGetSize();
    let size = unsafe { gl::glslang_program_SPIRV_get_size(program) } as usize;
    // const ptr = try program.spirvGetPtr();
    let ptr = unsafe { gl::glslang_program_SPIRV_get_ptr(program) };
    // const ptr_u8: [*]u8 = @ptrCast(ptr);
    let ptr_u8 = ptr as *const u8;
    // const slice_u8: []u8 = ptr_u8[0 .. size * 4];
    let slice_u8 = unsafe { std::slice::from_raw_parts(ptr_u8, size * 4) };
    // try writer.writeAll(slice_u8);
    writer.extend_from_slice(slice_u8);

    unsafe {
        gl::glslang_program_delete(program);
        gl::glslang_shader_delete(shader);
    }
    Ok(())
}

/// Retrieve errors from spirv compilation.
// pub const SpirvLog = struct {
//     alloc: Allocator,
//     info: [:0]const u8 = "",
//     debug: [:0]const u8 = "",
//
// PORT NOTE: `alloc` is folded into `Vec<u8>` ownership; `info`/`debug` are owned
// byte buffers (the reference's `dupeZ` copies are NUL-terminated `[:0]const u8`;
// the port stores the bytes sans sentinel — only `.len()` is observed). Rust
// `Drop` frees them, subsuming the reference `deinit`.
#[derive(Default)]
pub struct SpirvLog {
    pub info: Vec<u8>,
    pub debug: Vec<u8>,
}

impl SpirvLog {
    // fn fromShader(self: *SpirvLog, shader: *glslang.Shader) !void {
    // PARITY: named after the reference `fromShader`/`fromProgram` (mutating
    // `self` methods); not the `from_*`-takes-no-self convention clippy expects.
    #[allow(clippy::wrong_self_convention)]
    fn from_shader(&mut self, shader: *mut gl::glslang_shader_t) {
        // const info = try shader.getInfoLog();
        // const debug = try shader.getDebugInfoLog();
        let info = unsafe { c_str_to_vec(gl::glslang_shader_get_info_log(shader)) };
        let debug = unsafe { c_str_to_vec(gl::glslang_shader_get_info_debug_log(shader)) };
        // self.info = "";
        // self.debug = "";
        self.info = Vec::new();
        self.debug = Vec::new();
        // if (info.len > 0) self.info = try self.alloc.dupeZ(u8, info);
        if !info.is_empty() {
            self.info = info;
        }
        // if (debug.len > 0) self.debug = try self.alloc.dupeZ(u8, debug);
        if !debug.is_empty() {
            self.debug = debug;
        }
    }

    // fn fromProgram(self: *SpirvLog, program: *glslang.Program) !void {
    #[allow(clippy::wrong_self_convention)]
    fn from_program(&mut self, program: *mut gl::glslang_program_t) {
        // const info = try program.getInfoLog();
        // const debug = try program.getDebugInfoLog();
        let info = unsafe { c_str_to_vec(gl::glslang_program_get_info_log(program)) };
        let debug = unsafe { c_str_to_vec(gl::glslang_program_get_info_debug_log(program)) };
        self.info = Vec::new();
        self.debug = Vec::new();
        if !info.is_empty() {
            self.info = info;
        }
        if !debug.is_empty() {
            self.debug = debug;
        }
    }
}

/// Convert SPIR-V binary to MSL.
// pub fn mslFromSpv(alloc: Allocator, spv: []const u8) ![:0]const u8 {
pub fn msl_from_spv(spv: &[u8]) -> Result<Vec<u8>, Error> {
    // const c = spvcross.c;
    // return try spvCross(alloc, spvcross.c.SPVC_BACKEND_MSL, spv, (struct { ... }).setOptions);
    fn set_options(options: spvc::spvc_compiler_options) -> Result<(), Error> {
        // We enable decoration binding, because we need this
        // to properly locate the uniform block to index 1.
        // if (c.spvc_compiler_options_set_bool(
        //     options,
        //     c.SPVC_COMPILER_OPTION_MSL_ENABLE_DECORATION_BINDING,
        //     c.SPVC_TRUE,
        // ) != c.SPVC_SUCCESS) {
        //     return error.SpvcFailed;
        // }
        if unsafe {
            spvc::spvc_compiler_options_set_bool(
                options,
                spvc::SPVC_COMPILER_OPTION_MSL_ENABLE_DECORATION_BINDING,
                SPVC_TRUE,
            )
        } != spvc::SPVC_SUCCESS
        {
            return Err(Error::SpvcFailed);
        }
        Ok(())
    }
    spv_cross(spvc::SPVC_BACKEND_MSL, spv, Some(set_options))
}

/// Convert SPIR-V binary to GLSL.
// pub fn glslFromSpv(alloc: Allocator, spv: []const u8) ![:0]const u8 {
pub fn glsl_from_spv(spv: &[u8]) -> Result<Vec<u8>, Error> {
    // const GLSL_VERSION = 430;
    const GLSL_VERSION: c_uint = 430;

    // const c = spvcross.c;
    // return try spvCross(alloc, c.SPVC_BACKEND_GLSL, spv, (struct { ... }).setOptions);
    fn set_options(options: spvc::spvc_compiler_options) -> Result<(), Error> {
        // if (c.spvc_compiler_options_set_uint(
        //     options,
        //     c.SPVC_COMPILER_OPTION_GLSL_VERSION,
        //     GLSL_VERSION,
        // ) != c.SPVC_SUCCESS) {
        //     return error.SpvcFailed;
        // }
        if unsafe {
            spvc::spvc_compiler_options_set_uint(
                options,
                spvc::SPVC_COMPILER_OPTION_GLSL_VERSION,
                GLSL_VERSION,
            )
        } != spvc::SPVC_SUCCESS
        {
            return Err(Error::SpvcFailed);
        }
        Ok(())
    }
    spv_cross(spvc::SPVC_BACKEND_GLSL, spv, Some(set_options))
}

// fn spvCross(
//     alloc: Allocator,
//     backend: spvcross.c.spvc_backend,
//     spv: []const u8,
//     comptime optionsFn_: ?*const fn (c: spvcross.c.spvc_compiler_options) error{SpvcFailed}!void,
// ) ![:0]const u8 {
fn spv_cross(
    backend: spvc::spvc_backend,
    spv: &[u8],
    options_fn_: Option<fn(spvc::spvc_compiler_options) -> Result<(), Error>>,
) -> Result<Vec<u8>, Error> {
    // Serialize the transpile FFI (see SHADER_FFI_LOCK PORT NOTE). `spv_cross` is
    // called by the caller AFTER `spirv_from_glsl` has released the lock, so
    // there is no nesting/deadlock.
    let _ffi_guard = SHADER_FFI_LOCK.lock().unwrap_or_else(|e| e.into_inner());

    // Spir-V is always a multiple of 4 because it is written as a series of words
    // if (@mod(spv.len, 4) != 0) return error.SpirvInvalid;
    // PARITY: faithful literal translation of `@mod(spv.len, 4) != 0`.
    #[allow(clippy::manual_is_multiple_of)]
    if spv.len() % 4 != 0 {
        return Err(Error::SpirvInvalid);
    }

    // PORT NOTE: the reference relies on the caller's `ArrayListAligned(u8, u32)`
    // for the `@alignCast(spv.ptr)` to a word pointer. The port takes an arbitrary
    // `&[u8]` and copies it into a `Vec<u32>` to guarantee the same 4-byte
    // alignment before handing the word pointer to `spvc_context_parse_spirv`.
    let word_count = spv.len() / 4;
    let mut words: Vec<u32> = vec![0u32; word_count];
    unsafe {
        std::ptr::copy_nonoverlapping(spv.as_ptr(), words.as_mut_ptr() as *mut u8, spv.len());
    }

    // Compiler context
    // const c = spvcross.c;
    // var ctx: c.spvc_context = undefined;
    // if (c.spvc_context_create(&ctx) != c.SPVC_SUCCESS) return error.SpvcFailed;
    // defer c.spvc_context_destroy(ctx);
    let mut ctx: spvc::spvc_context = std::ptr::null_mut();
    if unsafe { spvc::spvc_context_create(&mut ctx) } != spvc::SPVC_SUCCESS {
        return Err(Error::SpvcFailed);
    }

    // PORT NOTE: `defer c.spvc_context_destroy(ctx)` — run the remaining steps in a
    // closure so `spvc_context_destroy(ctx)` fires exactly once on every exit path.
    let result = (|| -> Result<Vec<u8>, Error> {
        // It would be better to get this out into an output parameter to
        // show users but for now we can just log it.
        // c.spvc_context_set_error_callback(ctx, @ptrCast(&(struct { ... }).callback), null);
        unsafe {
            spvc::spvc_context_set_error_callback(ctx, Some(error_callback), std::ptr::null_mut());
        }

        // Parse the Spir-V binary to an IR
        // var ir: c.spvc_parsed_ir = undefined;
        // if (c.spvc_context_parse_spirv(ctx, @ptrCast(@alignCast(spv.ptr)), spv.len / 4, &ir) != c.SPVC_SUCCESS) {
        //     return error.SpvcFailed;
        // }
        let mut ir: spvc::spvc_parsed_ir = std::ptr::null_mut();
        if unsafe {
            spvc::spvc_context_parse_spirv(
                ctx,
                words.as_ptr() as *const spvc::SpvId,
                word_count,
                &mut ir,
            )
        } != spvc::SPVC_SUCCESS
        {
            return Err(Error::SpvcFailed);
        }

        // Build our compiler to GLSL
        // var compiler: c.spvc_compiler = undefined;
        // if (c.spvc_context_create_compiler(ctx, backend, ir, c.SPVC_CAPTURE_MODE_TAKE_OWNERSHIP, &compiler) != c.SPVC_SUCCESS) {
        //     return error.SpvcFailed;
        // }
        let mut compiler: spvc::spvc_compiler = std::ptr::null_mut();
        if unsafe {
            spvc::spvc_context_create_compiler(
                ctx,
                backend,
                ir,
                spvc::SPVC_CAPTURE_MODE_TAKE_OWNERSHIP,
                &mut compiler,
            )
        } != spvc::SPVC_SUCCESS
        {
            return Err(Error::SpvcFailed);
        }

        // Setup our options if we have any
        // if (optionsFn_) |optionsFn| {
        if let Some(options_fn) = options_fn_ {
            // var options: c.spvc_compiler_options = undefined;
            // if (c.spvc_compiler_create_compiler_options(compiler, &options) != c.SPVC_SUCCESS) {
            //     return error.SpvcFailed;
            // }
            let mut options: spvc::spvc_compiler_options = std::ptr::null_mut();
            if unsafe { spvc::spvc_compiler_create_compiler_options(compiler, &mut options) }
                != spvc::SPVC_SUCCESS
            {
                return Err(Error::SpvcFailed);
            }

            // try optionsFn(options);
            options_fn(options)?;

            // if (c.spvc_compiler_install_compiler_options(compiler, options) != c.SPVC_SUCCESS) {
            //     return error.SpvcFailed;
            // }
            if unsafe { spvc::spvc_compiler_install_compiler_options(compiler, options) }
                != spvc::SPVC_SUCCESS
            {
                return Err(Error::SpvcFailed);
            }
        }

        // Compile the resulting string. This string pointer is owned by the
        // context so we don't need to free it.
        // var result: [*:0]const u8 = undefined;
        // if (c.spvc_compiler_compile(compiler, @ptrCast(&result)) != c.SPVC_SUCCESS) {
        //     return error.SpvcFailed;
        // }
        let mut result: *const c_char = std::ptr::null();
        if unsafe { spvc::spvc_compiler_compile(compiler, &mut result) } != spvc::SPVC_SUCCESS {
            return Err(Error::SpvcFailed);
        }

        // return try alloc.dupeZ(u8, std.mem.sliceTo(result, 0));
        Ok(unsafe { c_str_to_vec(result) })
    })();

    unsafe { spvc::spvc_context_destroy(ctx) };
    result
}

// PORT NOTE: `c.SPVC_TRUE` is a header `#define SPVC_TRUE ((spvc_bool)1)` macro
// that bindgen does not emit as a constant; inline its value (1) as `spvc_bool`.
const SPVC_TRUE: spvc::spvc_bool = 1;

// PORT NOTE (task-14c-2): the reference's inline spirv-cross error callback,
// RESTORED. It is `std.log.warn`, i.e. the `.default` scope rather than this file's
// `.shadertoy` one, so it carries `target: ""` per DECISION-logging.md §6r — not
// `LOG_SCOPE`. The callback signature matches `spvc_error_callback`.
//
// This site was UNCLAIMED by any ledger row (S117-F3's oracle column stops at
// `image.zig`; `shadertoy.zig:298` appears in no row's anchors) — restored anyway,
// per PROCEDURE.md blind spot 3.
// fn callback(_: ?*anyopaque, msg_ptr: [*c]const u8) callconv(.c) void {
//     const msg = std.mem.sliceTo(msg_ptr, 0);
//     std.log.warn("spirv-cross error message={s}", .{msg});
// }
unsafe extern "C" fn error_callback(_userdata: *mut c_void, msg_ptr: *const c_char) {
    // const msg = std.mem.sliceTo(msg_ptr, 0);
    // PORT NOTE (panic policy): this runs on a C (spirv-cross) frame, so a null
    // `msg_ptr` must not be dereferenced. The reference's `[*c]const u8` -> `sliceTo`
    // has the same contract (spirv-cross always passes a NUL-terminated string).
    if msg_ptr.is_null() {
        return;
    }
    // SAFETY: non-null and NUL-terminated, per `spvc_error_callback`'s contract.
    let msg = unsafe { std::ffi::CStr::from_ptr(msg_ptr) };
    // std.log.warn("spirv-cross error message={s}", .{msg});
    log::warn!(
        target: "",
        "spirv-cross error message={}",
        String::from_utf8_lossy(msg.to_bytes())
    );
}

// ---------------------------------------------------------------------------
// Inlined pkg/glslang safe-wrapper surface (init + testing.ensureInit + the
// `getInfoLog`/`sliceTo` helpers). Byte-faithful to pkg/glslang/{init,test}.zig.
// ---------------------------------------------------------------------------

// pkg/glslang/init.zig:
// pub fn init() !void {
//     if (c.glslang_initialize_process() == 0) return error.GlslangInitFailed;
// }
fn glslang_init() -> Result<(), Error> {
    if unsafe { gl::glslang_initialize_process() } == 0 {
        return Err(Error::GlslangInitFailed);
    }
    Ok(())
}

/// The pub surface of pkg/glslang's `init` — what `global.zig` calls as
/// `try glslang.init()` (the WS4 `GlobalState.init` wiring names it from
/// rustty-apprt). Forwards to the file-local `glslang_init` verbatim.
pub fn init() -> Result<(), Error> {
    glslang_init()
}

// pkg/glslang/test.zig:
// var initialized: bool = false;
// pub fn ensureInit() !void {
//     if (initialized) return;
//     try glslang.init();
// }
//
// PORT NOTE: exactly like the reference (and rustty-oniguruma's `testing`), the
// `initialized` flag is LOADED but never STORED, so `glslang_init` runs on every
// call. `glslang_initialize_process` is process-global + refcounted/thread-safe,
// so repeated calls are safe (extra refcounts are never finalized — an FFI-leak
// concern deferred to Linux CI per check_leaks.sh).
static INITIALIZED: AtomicBool = AtomicBool::new(false);
fn glslang_ensure_init() -> Result<(), Error> {
    if INITIALIZED.load(Ordering::SeqCst) {
        return Ok(());
    }
    glslang_init()
}

// pkg/glslang `getInfoLog`/`getDebugInfoLog` do `std.mem.sliceTo(ptr, 0)`; here we
// copy the NUL-terminated C string into an owned `Vec<u8>` (empty on null).
unsafe fn c_str_to_vec(ptr: *const c_char) -> Vec<u8> {
    if ptr.is_null() {
        return Vec::new();
    }
    unsafe { std::ffi::CStr::from_ptr(ptr).to_bytes().to_vec() }
}

// ---------------------------------------------------------------------------

/// Convert ShaderToy shader to null-terminated glsl for testing.
// fn testGlslZ(alloc: Allocator, src: []const u8) ![:0]const u8 {
#[cfg(test)]
fn test_glsl_z(src: &[u8]) -> Vec<u8> {
    // var buf: std.Io.Writer.Allocating = .init(alloc);
    // defer buf.deinit();
    // try glslFromShader(&buf.writer, src);
    // return try buf.toOwnedSliceSentinel(0);
    let mut buf: Vec<u8> = Vec::new();
    glsl_from_shader(&mut buf, src);
    buf.push(0); // sentinel (NUL) — `toOwnedSliceSentinel(0)`
    buf
}

// const test_crt = @embedFile("shaders/test_shadertoy_crt.glsl");
// const test_invalid = @embedFile("shaders/test_shadertoy_invalid.glsl");
// const test_focus = @embedFile("shaders/test_shadertoy_focus.glsl");
#[cfg(test)]
const TEST_CRT: &[u8] = include_bytes!("shaders/test_shadertoy_crt.glsl");
#[cfg(test)]
const TEST_INVALID: &[u8] = include_bytes!("shaders/test_shadertoy_invalid.glsl");
// PORT NOTE: `test_focus` is declared-but-unused in the reference (kept for
// completeness); mirror it, allowing the resulting dead-code.
#[cfg(test)]
#[allow(dead_code)]
const TEST_FOCUS: &[u8] = include_bytes!("shaders/test_shadertoy_focus.glsl");

#[cfg(test)]
mod tests {
    use super::*;

    // test "spirv"
    #[test]
    fn spirv() {
        // const src = try testGlslZ(alloc, test_crt);
        let src = test_glsl_z(TEST_CRT);

        // var buf: [4096 * 4]u8 = undefined;
        // var writer: std.Io.Writer = .fixed(&buf);
        // try spirvFromGlsl(&writer, null, src);
        let mut buf: Vec<u8> = Vec::new();
        spirv_from_glsl(&mut buf, None, &src).unwrap();
    }

    // test "spirv invalid"
    #[test]
    fn spirv_invalid() {
        // const src = try testGlslZ(alloc, test_invalid);
        let src = test_glsl_z(TEST_INVALID);

        let mut buf: Vec<u8> = Vec::new();

        // var errlog: SpirvLog = .{ .alloc = alloc };
        // defer errlog.deinit();
        let mut errlog = SpirvLog::default();
        // try testing.expectError(error.GlslangFailed, spirvFromGlsl(&writer, &errlog, src));
        let res = spirv_from_glsl(&mut buf, Some(&mut errlog), &src);
        assert!(matches!(res, Err(Error::GlslangFailed)));
        // try testing.expect(errlog.info.len > 0);
        assert!(!errlog.info.is_empty());
    }

    // test "shadertoy to msl"
    #[test]
    fn shadertoy_to_msl() {
        // const src = try testGlslZ(alloc, test_crt);
        let src = test_glsl_z(TEST_CRT);

        // var buf: std.Io.Writer.Allocating = .init(alloc);
        // try spirvFromGlsl(&buf.writer, null, src);
        let mut buf: Vec<u8> = Vec::new();
        spirv_from_glsl(&mut buf, None, &src).unwrap();

        // TODO: Replace this with an aligned version of Writer.Allocating
        // var spvlist: std.ArrayListAligned(u8, .of(u32)) = .empty;
        // try spvlist.appendSlice(alloc, buf.written());
        //
        // PORT NOTE: the reference re-packs into a u32-aligned list for the
        // `mslFromSpv` `@alignCast`; `spv_cross` already copies into a `Vec<u32>`
        // internally, so `buf` is passed directly.
        // const msl = try mslFromSpv(alloc, spvlist.items);
        let msl = msl_from_spv(&buf).unwrap();
        let _ = msl;
    }

    // test "shadertoy to glsl"
    #[test]
    fn shadertoy_to_glsl() {
        // const src = try testGlslZ(alloc, test_crt);
        let src = test_glsl_z(TEST_CRT);

        // var buf: std.Io.Writer.Allocating = .init(alloc);
        // try spirvFromGlsl(&buf.writer, null, src);
        let mut buf: Vec<u8> = Vec::new();
        spirv_from_glsl(&mut buf, None, &src).unwrap();

        // TODO: Replace this with an aligned version of Writer.Allocating
        // (see `shadertoy_to_msl` PORT NOTE — the aligned re-pack is subsumed).
        // const glsl = try glslFromSpv(alloc, spvlist.items);
        let glsl = glsl_from_spv(&buf).unwrap();

        // log.warn("glsl={s}", .{glsl});
        let _ = glsl;
    }

    // RENDERER_CUSTOM_SHADERS_DESIGN §4.1 — host-only (cross-platform): the
    // `loadFromFiles`/`loadFromFile` composition reads a GLSL file, wraps + compiles
    // + transpiles it to the target (`Glsl` here — glslang + spirv-cross are host
    // libs, no Metal device is touched), AND both control-flow arms (optional-skip,
    // required-error). This is the first live exercise of the un-deferred load path.
    #[test]
    fn load_from_files_transpiles_glsl() {
        use std::ffi::CString;
        // Write the pass-through shadertoy body to a nanos-suffixed temp file
        // (mirrors the bg-image golden, generic.rs).
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rustty_shader_{nanos}.glsl"));
        std::fs::write(
            &path,
            b"void mainImage(out vec4 fragColor, in vec2 fragCoord){\n\
              fragColor = texture(iChannel0, fragCoord / iResolution.xy);\n}\n",
        )
        .unwrap();

        // Byte-faithful path -> CString (mirrors the bg-image golden).
        let cpath = CString::new(path.as_os_str().as_encoded_bytes().to_vec()).unwrap();
        let rp = RepeatablePath {
            value: vec![Path::Required(cpath)],
        };
        let out = load_from_files(&rp, Target::Glsl).unwrap();
        assert_eq!(out.len(), 1);
        assert!(!out[0].as_bytes().is_empty(), "transpiled source is empty");

        // Optional-arm missing file is SKIPPED (not an error).
        let rp_missing_opt = RepeatablePath {
            value: vec![Path::Optional(
                CString::new("/nonexistent/rustty_missing.glsl").unwrap(),
            )],
        };
        assert!(load_from_files(&rp_missing_opt, Target::Glsl)
            .unwrap()
            .is_empty());

        // Required-arm missing file is a FileNotFound error.
        let rp_missing_req = RepeatablePath {
            value: vec![Path::Required(
                CString::new("/nonexistent/rustty_missing.glsl").unwrap(),
            )],
        };
        assert!(matches!(
            load_from_files(&rp_missing_req, Target::Glsl),
            Err(Error::FileNotFound)
        ));

        let _ = std::fs::remove_file(&path);
    }

    // pkg/glslang init.zig `init` via the pub surface (global.zig `try glslang.init()`).
    #[test]
    fn public_glslang_init() {
        // glslang_initialize_process is refcounted/thread-safe; repeated calls
        // are safe (see the ensureInit PORT NOTE above).
        crate::shadertoy::init().expect("glslang init");
    }

    // S117-F1: the std140 layout the GLSL `Globals` uniform block (binding 1,
    // shaders/shadertoy_prefix.glsl) expects, hand-derived field-by-field from the
    // oracle's `align(16)`/`align(4)` annotations (ghostty src/renderer/shadertoy.zig:
    // 14-42) — see the task-5 report for the full per-field working. Offsets/size
    // confirmed against the brief's audit numbers (32 / 208 / 304 / 4496) by
    // independent derivation.
    #[test]
    fn uniforms_match_the_std140_block_in_shadertoy_prefix_glsl() {
        use std::mem::{align_of, offset_of, size_of};
        assert_eq!(offset_of!(Uniforms, channel_time), 32);
        assert_eq!(offset_of!(Uniforms, current_cursor), 208);
        assert_eq!(offset_of!(Uniforms, palette), 304);
        assert_eq!(size_of::<Uniforms>(), 4496);
        assert_eq!(align_of::<Uniforms>(), 16);
    }

    /// Reads the `Offset` of every member of the uniform block named `block`
    /// from SPIR-V words: `OpName` (5) names the block type, `OpMemberName` (6)
    /// names each member, and `OpMemberDecorate` (72) with decoration `Offset`
    /// (35) gives its byte offset. Returns `(member name, offset)` in member order.
    fn spirv_block_member_offsets(spv: &[u8], block: &str) -> Vec<(String, u32)> {
        assert_eq!(spv.len() % 4, 0, "SPIR-V is a sequence of 32-bit words");
        let words: Vec<u32> = spv
            .chunks_exact(4)
            .map(|w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
            .collect();
        assert_eq!(words[0], 0x0723_0203, "SPIR-V magic number");
        let lit = |ws: &[u32]| -> String {
            let bytes: Vec<u8> = ws.iter().flat_map(|w| w.to_le_bytes()).collect();
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            String::from_utf8(bytes[..end].to_vec()).unwrap()
        };
        let mut block_id = None;
        let mut names: std::collections::BTreeMap<(u32, u32), String> = Default::default();
        let mut offsets: std::collections::BTreeMap<(u32, u32), u32> = Default::default();
        let mut i = 5; // past the five-word header
        while i < words.len() {
            let count = (words[i] >> 16) as usize;
            let opcode = words[i] & 0xffff;
            assert!(count > 0, "malformed SPIR-V instruction at word {i}");
            let ops = &words[i + 1..i + count];
            match opcode {
                5 if lit(&ops[1..]) == block => block_id = Some(ops[0]),
                6 => {
                    names.insert((ops[0], ops[1]), lit(&ops[2..]));
                }
                72 if ops[2] == 35 => {
                    offsets.insert((ops[0], ops[1]), ops[3]);
                }
                _ => {}
            }
            i += count;
        }
        let id = block_id.unwrap_or_else(|| panic!("no OpName {block:?} in the SPIR-V"));
        names
            .iter()
            .filter(|((ty, _), _)| *ty == id)
            .map(|(key, name)| (name.clone(), offsets[key]))
            .collect()
    }

    /// Upstream `custom shader selection uniform layout matches compiled GLSL`
    /// (`80456f057`): the two selection colours sit at the std140 offsets glslang
    /// assigns `iSelectionForegroundColor` / `iSelectionBackgroundColor`.
    #[test]
    fn custom_shader_selection_uniform_layout_matches_compiled_glsl() {
        use std::mem::offset_of;
        let src = test_glsl_z(
            b"void mainImage(out vec4 color, in vec2 pixel) {\n\
              color = vec4(iSelectionForegroundColor + iSelectionBackgroundColor, 1.0);\n\
              }\n",
        );
        let mut spv = Vec::new();
        spirv_from_glsl(&mut spv, None, &src).expect("the prefix compiles");
        let members = spirv_block_member_offsets(&spv, "Globals");

        for (field, name) in [
            (
                offset_of!(Uniforms, selection_foreground_color),
                "iSelectionForegroundColor",
            ),
            (
                offset_of!(Uniforms, selection_background_color),
                "iSelectionBackgroundColor",
            ),
        ] {
            let (_, offset) = members
                .iter()
                .find(|(n, _)| n == name)
                .unwrap_or_else(|| panic!("no Globals member {name}"));
            assert_eq!(*offset as usize, field, "{name}");
        }
    }

    /// Rustty's broader check (UD20, upstream `80456f057`): every member of
    /// the GLSL `Globals` block must receive the `Uniforms` field of the same
    /// name. The block is uploaded byte for byte, so a member reads whatever
    /// field sits at its std140 offset. The offsets come from glslang itself
    /// (the prefix compiled to SPIR-V by the production `spirv_from_glsl`), not
    /// from a hand derivation. In the reference `iSelectionForegroundColor`
    /// sits on `selection_background_color` and vice versa.
    #[test]
    fn globals_block_members_receive_the_uniform_fields_they_are_named_for() {
        use std::mem::offset_of;
        let fields: &[(&str, usize)] = &[
            ("iResolution", offset_of!(Uniforms, resolution)),
            ("iTime", offset_of!(Uniforms, time)),
            ("iTimeDelta", offset_of!(Uniforms, time_delta)),
            ("iFrameRate", offset_of!(Uniforms, frame_rate)),
            ("iFrame", offset_of!(Uniforms, frame)),
            ("iChannelTime", offset_of!(Uniforms, channel_time)),
            (
                "iChannelResolution",
                offset_of!(Uniforms, channel_resolution),
            ),
            ("iMouse", offset_of!(Uniforms, mouse)),
            ("iDate", offset_of!(Uniforms, date)),
            ("iSampleRate", offset_of!(Uniforms, sample_rate)),
            ("iCurrentCursor", offset_of!(Uniforms, current_cursor)),
            ("iPreviousCursor", offset_of!(Uniforms, previous_cursor)),
            (
                "iCurrentCursorColor",
                offset_of!(Uniforms, current_cursor_color),
            ),
            (
                "iPreviousCursorColor",
                offset_of!(Uniforms, previous_cursor_color),
            ),
            (
                "iCurrentCursorStyle",
                offset_of!(Uniforms, current_cursor_style),
            ),
            (
                "iPreviousCursorStyle",
                offset_of!(Uniforms, previous_cursor_style),
            ),
            ("iCursorVisible", offset_of!(Uniforms, cursor_visible)),
            (
                "iTimeCursorChange",
                offset_of!(Uniforms, cursor_change_time),
            ),
            ("iTimeFocus", offset_of!(Uniforms, time_focus)),
            ("iFocus", offset_of!(Uniforms, focus)),
            ("iPalette", offset_of!(Uniforms, palette)),
            ("iBackgroundColor", offset_of!(Uniforms, background_color)),
            ("iForegroundColor", offset_of!(Uniforms, foreground_color)),
            ("iCursorColor", offset_of!(Uniforms, cursor_color)),
            ("iCursorText", offset_of!(Uniforms, cursor_text)),
            (
                "iSelectionForegroundColor",
                offset_of!(Uniforms, selection_foreground_color),
            ),
            (
                "iSelectionBackgroundColor",
                offset_of!(Uniforms, selection_background_color),
            ),
        ];

        let src = test_glsl_z(
            b"void mainImage(out vec4 fragColor, in vec2 fragCoord) {\n\
              fragColor = vec4(iSelectionForegroundColor + iSelectionBackgroundColor, 1.0);\n\
              }\n",
        );
        let mut spv = Vec::new();
        spirv_from_glsl(&mut spv, None, &src).expect("the prefix compiles");
        let members = spirv_block_member_offsets(&spv, "Globals");

        // Every block member is paired with exactly one field, and the other way round.
        assert_eq!(
            members.len(),
            fields.len(),
            "Globals members {members:?} vs the fields this test pairs them with"
        );
        for (name, glsl_offset) in &members {
            let (_, rust_offset) = fields
                .iter()
                .find(|(n, _)| n == name)
                .unwrap_or_else(|| panic!("GLSL member {name} has no Uniforms field here"));
            assert_eq!(
                *glsl_offset as usize, *rust_offset,
                "GLSL member {name} sits at std140 offset {glsl_offset}, but its Uniforms \
                 field is at {rust_offset}: the shader would read another field"
            );
        }
    }
}

/// task-14c-2 — proof that the restored `shadertoy.zig` diagnostics actually
/// EMIT, on real failure paths, with the reference's exact message text.
///
/// Neither test here needs a Metal device: glslang and spirv-cross are host
/// libraries. That is deliberate — this stage is otherwise almost entirely Metal
/// code, and the 14c-1 fix round demonstrated that a device-gated suite can go
/// `102 passed; 0 failed` with zero Metal tests actually executing. The one test
/// in this stage that genuinely CANNOT run without a device
/// (`metal::pipeline::tests::check_error_emits_at_the_use_site`) routes through
/// `metal::test_support::require_metal_or_skip` and prints a skip line.
#[cfg(test)]
mod restored_log_sites {
    use super::*;
    use crate::log_capture::CapturedRecord;

    /// Writes `body` to a nanos-suffixed temp file and returns its path.
    fn temp_shader(body: &[u8]) -> std::path::PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("rustty_log_shader_{nanos}.glsl"));
        std::fs::write(&path, body).unwrap();
        path
    }

    /// TEST 1 — a plain SINGLE-ARGUMENT emission on a reachable failure path:
    /// `shadertoy.zig:298`'s `std.log.warn("spirv-cross error message={s}", .{msg})`,
    /// the callback `spvc_context_set_error_callback` installs.
    ///
    /// Two things it proves beyond "a record exists":
    ///
    /// 1. The callback is genuinely WIRED into spirv-cross — the record can only
    ///    appear because the C library called back into `error_callback`. Calling
    ///    that function directly from the test would prove nothing.
    /// 2. It targets `""`, the reference's `.default` scope, NOT `shadertoy`.
    ///    `std.log.warn` is unscoped (`main_ghostty.zig:133`: a `.default` record
    ///    gets an empty prefix); DECISION-logging.md §6r carries that as `target: ""`.
    ///    An "obvious tidy-up" to `LOG_SCOPE` fails this test.
    ///
    /// This site is claimed by NO ledger row (S117-F3's oracle column stops at
    /// `image.zig`). Restored and tested per PROCEDURE.md blind spot 3.
    #[test]
    fn spirv_cross_error_callback_emits_unscoped() {
        // Four bytes so the `spv.len % 4 != 0` guard passes, but not a valid
        // SPIR-V magic number — `spvc_context_parse_spirv` rejects it and reports
        // through the error callback.
        let capture = crate::log_capture::start();
        let res = msl_from_spv(&[0u8, 0, 0, 0]);
        let records: Vec<CapturedRecord> = capture.messages();
        drop(capture);

        assert!(
            matches!(res, Err(Error::SpvcFailed)),
            "expected the parse to fail, got {res:?}"
        );
        assert_eq!(records.len(), 1, "expected exactly one record: {records:?}");
        let (level, target, msg) = &records[0];
        assert_eq!(*level, log::Level::Warn);
        assert_eq!(
            target, "",
            "the reference uses `std.log.warn` (.default scope)"
        );
        assert!(
            msg.starts_with("spirv-cross error message="),
            "message text must be the reference's, verbatim; got {msg:?}"
        );
    }

    /// TEST 2 — the ELIDED-COMMENT site: `shadertoy.zig:117`'s
    ///
    ///     log.warn("spirv error path={s} info={s} debug={s}", .{
    ///         path, errlog.info, errlog.debug,
    ///     });
    ///
    /// The port's preserved comment read
    /// `// log.warn("spirv error path={s} info={s} debug={s}", ...);` — the
    /// ARGUMENTS were elided behind `...`. Anything restored from that comment
    /// alone cannot know that there are three of them, that they are in the order
    /// path/info/debug, or that the whole call is guarded by
    /// `if (errlog.info.len > 0 or errlog.debug.len > 0)`. This test asserts the
    /// fully-substituted string, so it fails for a restore made from the comment.
    ///
    /// The expected `info`/`debug` bodies are recomputed here by running the same
    /// compile, rather than hard-coded — glslang's diagnostic wording is its
    /// business, the ARGUMENT ORDER is ours.
    #[test]
    fn spirv_failure_warns_with_the_full_oracle_message() {
        use std::ffi::CString;

        let path = temp_shader(TEST_INVALID);
        let cpath = CString::new(path.as_os_str().as_encoded_bytes().to_vec()).unwrap();

        // Recompute what the reference's three arguments render to.
        let src = test_glsl_z(TEST_INVALID);
        let mut sink: Vec<u8> = Vec::new();
        let mut expect_log = SpirvLog::default();
        assert!(spirv_from_glsl(&mut sink, Some(&mut expect_log), &src).is_err());
        assert!(
            !expect_log.info.is_empty(),
            "fixture must actually produce a glslang info log"
        );
        let expected = format!(
            "spirv error path={} info={} debug={}",
            String::from_utf8_lossy(cpath.to_bytes()),
            String::from_utf8_lossy(&expect_log.info),
            String::from_utf8_lossy(&expect_log.debug),
        );

        let capture = crate::log_capture::start();
        let res = load_from_file(&cpath, Target::Glsl);
        let records: Vec<CapturedRecord> = capture.messages();
        drop(capture);
        let _ = std::fs::remove_file(&path);

        assert!(matches!(res, Err(Error::GlslangFailed)), "got {res:?}");
        assert_eq!(
            records,
            vec![(log::Level::Warn, "shadertoy".to_string(), expected)]
        );
    }
}
