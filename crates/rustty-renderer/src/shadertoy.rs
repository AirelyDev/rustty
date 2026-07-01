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
//! DEFERRED (see .port/phase-2/DEFERRALS.md): `loadFromFiles` / `loadFromFile`.
//! They take a `configpkg.RepeatablePath` (`@import("../config.zig")`), but in
//! the rustty crate split `rustty-config` ALREADY depends on `rustty-renderer`
//! (config/Cargo.toml), so a `rustty-renderer -> rustty-config` edge would form
//! a crate cycle (G7 RED). The two functions are NOT GPU-coupled and NOT
//! exercised by any of the 4 tests; their verbatim bodies are preserved below
//! (R5, never a silent drop) and land once the renderer<->config custom-shader
//! load layering is resolved (the surface/apprt wiring milestone).
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

use std::os::raw::{c_char, c_int, c_uint, c_void};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

// PORT NOTE: `cargo test` runs tests concurrently across threads, but the
// vendored glslang / SPIRV-Cross C++ libraries carry process-global state that
// is NOT safe for concurrent compilation (`zig test` runs serially, so the
// reference never hits this). Serialize the whole transpile FFI (`spirv_from_glsl`
// AND `spv_cross`) behind one process-global lock. Custom-shader compilation
// happens at renderer startup (rarely concurrent), so the production cost is
// negligible.
static SHADER_FFI_LOCK: Mutex<()> = Mutex::new(());

// const log = std.log.scoped(.shadertoy);
// PORT NOTE: no logging facade is wired in the port yet, so the `log.info`/
// `log.warn` call sites are preserved as comments (mirrors the tree-wide
// `std.log.scoped` handling in e.g. rustty-config path.rs).

/// The uniform struct used for shadertoy shaders.
//
// PORT NOTE: the reference is an `extern struct` with per-field `align(16)`
// (vec3/vec4/matrix members) and `align(4)` (scalar members) matching the
// std140 GPU-uniform layout. Rust cannot express per-field alignment, so this
// is `#[repr(C)]` with every field declared in order; the EXACT std140 layout
// (with const-asserts) is pinned when the GPU draw path (R-d/Phase 3) actually
// consumes this struct. It is unused by this milestone's transpile path.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct Uniforms {
    pub resolution: [f32; 3],                 // align(16)
    pub time: f32,                            // align(4)
    pub time_delta: f32,                      // align(4)
    pub frame_rate: f32,                      // align(4)
    pub frame: i32,                           // align(4)
    pub channel_time: [[f32; 4]; 4],          // align(16)
    pub channel_resolution: [[f32; 4]; 4],    // align(16)
    pub mouse: [f32; 4],                      // align(16)
    pub date: [f32; 4],                       // align(16)
    pub sample_rate: f32,                     // align(4)
    pub current_cursor: [f32; 4],             // align(16)
    pub previous_cursor: [f32; 4],            // align(16)
    pub current_cursor_color: [f32; 4],       // align(16)
    pub previous_cursor_color: [f32; 4],      // align(16)
    pub current_cursor_style: i32,            // align(4)
    pub previous_cursor_style: i32,           // align(4)
    pub cursor_visible: i32,                  // align(4)
    pub cursor_change_time: f32,              // align(4)
    pub time_focus: f32,                      // align(4)
    pub focus: i32,                           // align(4)
    pub palette: [[f32; 4]; 256],             // align(16)
    pub background_color: [f32; 4],           // align(16)
    pub foreground_color: [f32; 4],           // align(16)
    pub cursor_color: [f32; 4],               // align(16)
    pub cursor_text: [f32; 4],                // align(16)
    pub selection_background_color: [f32; 4], // align(16)
    pub selection_foreground_color: [f32; 4], // align(16)
}

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
// only by the deferred `loadFromFile*` path (kept for signature fidelity).
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

// ===========================================================================
// DEFERRED: loadFromFiles / loadFromFile (config-coupled; would cycle
// rustty-renderer -> rustty-config -> rustty-renderer). Bodies preserved
// verbatim (R5). See .port/phase-2/DEFERRALS.md.
// ===========================================================================
//
// /// Load a set of shaders from files and convert them to the target
// /// format. The shader order is preserved.
// pub fn loadFromFiles(
//     alloc_gpa: Allocator,
//     paths: configpkg.RepeatablePath,
//     target: Target,
// ) ![]const [:0]const u8 {
//     var list: std.ArrayList([:0]const u8) = .empty;
//     defer list.deinit(alloc_gpa);
//     errdefer for (list.items) |shader| alloc_gpa.free(shader);
//
//     for (paths.value.items) |item| {
//         const path, const optional = switch (item) {
//             .optional => |path| .{ path, true },
//             .required => |path| .{ path, false },
//         };
//
//         const shader = loadFromFile(alloc_gpa, path, target) catch |err| {
//             if (err == error.FileNotFound and optional) {
//                 continue;
//             }
//
//             return err;
//         };
//         log.info("loaded custom shader path={s}", .{path});
//         try list.append(alloc_gpa, shader);
//     }
//
//     return try list.toOwnedSlice(alloc_gpa);
// }
//
// /// Load a single shader from a file and convert it to the target language
// /// ready to be used with renderers.
// pub fn loadFromFile(
//     alloc_gpa: Allocator,
//     path: []const u8,
//     target: Target,
// ) ![:0]const u8 {
//     var arena = ArenaAllocator.init(alloc_gpa);
//     defer arena.deinit();
//     const alloc = arena.allocator();
//
//     // Read it all into memory -- we don't expect shaders to be large.
//     const src = src: {
//         // Load the shader file
//         const cwd = std.fs.cwd();
//         const file = try cwd.openFile(path, .{});
//         defer file.close();
//
//         break :src try file.readToEndAlloc(
//             alloc,
//             4 * 1024 * 1024, // 4MB
//         );
//     };
//
//     // Convert to full GLSL
//     const glsl: [:0]const u8 = glsl: {
//         var stream: std.Io.Writer.Allocating = .init(alloc);
//         try glslFromShader(&stream.writer, src);
//         try stream.writer.writeByte(0);
//         break :glsl stream.written()[0 .. stream.written().len - 1 :0];
//     };
//
//     // Convert to SPIR-V
//     const spirv: []const u8 = spirv: {
//         var stream: std.Io.Writer.Allocating = .init(alloc);
//         var errlog: SpirvLog = .{ .alloc = alloc };
//         defer errlog.deinit();
//         spirvFromGlsl(&stream.writer, &errlog, glsl) catch |err| {
//             if (errlog.info.len > 0 or errlog.debug.len > 0) {
//                 log.warn("spirv error path={s} info={s} debug={s}", .{
//                     path,
//                     errlog.info,
//                     errlog.debug,
//                 });
//             }
//
//             return err;
//         };
//
//         // SpirV pointer must be aligned to 4 bytes since we expect
//         // a slice of words.
//         var list: std.ArrayListAligned(u8, .of(u32)) = .empty;
//         try list.appendSlice(alloc, stream.written());
//         break :spirv list.items;
//     };
//
//     // Convert to MSL
//     return switch (target) {
//         // Important: using the alloc_gpa here on purpose because this
//         // is the final result that will be returned to the caller.
//         .glsl => try glslFromSpv(alloc_gpa, spirv),
//         .msl => try mslFromSpv(alloc_gpa, spirv),
//     };
// }

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

// PORT NOTE: the reference's inline spirv-cross error callback. `msg` logging is
// elided (no logging facade); the callback signature matches `spvc_error_callback`.
// fn callback(_: ?*anyopaque, msg_ptr: [*c]const u8) callconv(.c) void {
//     const msg = std.mem.sliceTo(msg_ptr, 0);
//     std.log.warn("spirv-cross error message={s}", .{msg});
// }
unsafe extern "C" fn error_callback(_userdata: *mut c_void, msg_ptr: *const c_char) {
    let _ = msg_ptr;
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
}
