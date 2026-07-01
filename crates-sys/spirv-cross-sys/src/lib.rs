//! spirv-cross-sys — FFI bindings to the vendored KhronosGroup SPIRV-Cross
//! shader cross-compiler (`links = "spirv_cross"`).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3: this crate copies the SAME C++ library the reference uses — SPIRV-Cross
//! (C API version 0.59, upstream commit
//! 476f384eb7d9e48613c45179e502a15ab95b6b49) — into `vendor/` and builds it
//! standalone (`cc` in C++17 mode + `bindgen`); see build.rs. No pure-Rust
//! substitution during parity phases.
//!
//! The FFI surface is bindgen-generated from `spirv_cross_c.h` (the same header
//! the reference's `pkg/spirv-cross/c.zig` @cImports) and re-exported verbatim
//! below. Downstream, the renderer's Metal path builds on these raw symbols to
//! cross-compile SPIR-V into MSL/GLSL.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod kat {
    use super::*;

    /// A minimal but fully valid SPIR-V module for a trivial fragment shader
    /// (`void main() {}`), hand-assembled per the SPIR-V spec §2.3 physical
    /// layout. This is the disassembly it encodes:
    ///
    /// ```text
    ///                OpCapability Shader
    ///                OpMemoryModel Logical GLSL450
    ///                OpEntryPoint Fragment %main "main"
    ///                OpExecutionMode %main OriginUpperLeft
    ///        %void = OpTypeVoid                ; %1
    ///      %fntype = OpTypeFunction %void      ; %2
    ///        %main = OpFunction %void None %fntype  ; %3
    ///       %label = OpLabel                   ; %4
    ///                OpReturn
    ///                OpFunctionEnd
    /// ```
    ///
    /// Header: magic, version 1.0, generator 0, id-bound 5, schema 0. Each
    /// subsequent instruction's first word is `(word_count << 16) | opcode`.
    const SPIRV: [u32; 32] = [
        // ---- Header ----
        0x0723_0203, // SPIR-V magic number
        0x0001_0000, // version 1.0
        0x0000_0000, // generator magic (0 = unknown)
        0x0000_0005, // id bound (max id 4, so bound = 5)
        0x0000_0000, // schema (reserved, 0)
        // ---- OpCapability Shader ----
        0x0002_0011,
        0x0000_0001, // Capability = Shader
        // ---- OpMemoryModel Logical GLSL450 ----
        0x0003_000E,
        0x0000_0000, // AddressingModel = Logical
        0x0000_0001, // MemoryModel = GLSL450
        // ---- OpEntryPoint Fragment %3 "main" ----
        0x0005_000F,
        0x0000_0004, // ExecutionModel = Fragment
        0x0000_0003, // entry point = %main
        0x6E69_616D, // "main" (bytes m,a,i,n little-endian)
        0x0000_0000, // string null terminator + padding
        // ---- OpExecutionMode %3 OriginUpperLeft ----
        0x0003_0010,
        0x0000_0003, // %main
        0x0000_0007, // ExecutionMode = OriginUpperLeft
        // ---- %1 = OpTypeVoid ----
        0x0002_0013,
        0x0000_0001, // result = %void
        // ---- %2 = OpTypeFunction %void ----
        0x0003_0021,
        0x0000_0002, // result = %fntype
        0x0000_0001, // return type = %void
        // ---- %3 = OpFunction %void None %fntype ----
        0x0005_0036,
        0x0000_0001, // result type = %void
        0x0000_0003, // result = %main
        0x0000_0000, // function control = None
        0x0000_0002, // function type = %fntype
        // ---- %4 = OpLabel ----
        0x0002_00F8,
        0x0000_0004, // result = %label
        // ---- OpReturn ----
        0x0001_00FD,
        // ---- OpFunctionEnd ----
        0x0001_0038,
    ];

    /// Known-answer test: create a SPIRV-Cross context, parse the hand-assembled
    /// trivial fragment shader above into a `ParsedIR`, create a GLSL compiler
    /// over it (take-ownership capture mode), cross-compile, and assert the
    /// produced GLSL source is non-empty and contains a `main` entry point.
    /// This exercises the full vendored pipeline (`spvc_context_create` ->
    /// `spvc_context_parse_spirv` -> `spvc_context_create_compiler` ->
    /// `spvc_compiler_compile`), proving the standalone C++ build + FFI link are
    /// correct. Mirrors the reference's renderer shader cross-compilation flow.
    #[test]
    fn spvc_parses_and_cross_compiles_trivial_shader() {
        unsafe {
            // 1. Context — owns the whole child object hierarchy.
            let mut context: spvc_context = std::ptr::null_mut();
            let rc = spvc_context_create(&mut context);
            assert_eq!(rc, SPVC_SUCCESS, "spvc_context_create failed: {rc}");
            assert!(!context.is_null(), "spvc_context_create produced null");

            // Helper: fetch the context's last error string (empty if none).
            let last_error = |ctx: spvc_context| -> String {
                let p = spvc_context_get_last_error_string(ctx);
                if p.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
                }
            };

            // 2. Parse SPIR-V into a ParsedIR handle.
            let mut parsed_ir: spvc_parsed_ir = std::ptr::null_mut();
            let rc = spvc_context_parse_spirv(context, SPIRV.as_ptr(), SPIRV.len(), &mut parsed_ir);
            assert_eq!(
                rc,
                SPVC_SUCCESS,
                "spvc_context_parse_spirv failed: {rc} ({})",
                last_error(context)
            );
            assert!(!parsed_ir.is_null(), "parse produced a null ParsedIR");

            // 3. Create a GLSL compiler over the IR (take ownership: optimal,
            //    single cross-compile).
            let mut compiler: spvc_compiler = std::ptr::null_mut();
            let rc = spvc_context_create_compiler(
                context,
                SPVC_BACKEND_GLSL,
                parsed_ir,
                SPVC_CAPTURE_MODE_TAKE_OWNERSHIP,
                &mut compiler,
            );
            assert_eq!(
                rc,
                SPVC_SUCCESS,
                "spvc_context_create_compiler failed: {rc} ({})",
                last_error(context)
            );
            assert!(!compiler.is_null(), "create_compiler produced null");

            // 4. Cross-compile to a GLSL source string (owned by the context).
            let mut source: *const std::os::raw::c_char = std::ptr::null();
            let rc = spvc_compiler_compile(compiler, &mut source);
            assert_eq!(
                rc,
                SPVC_SUCCESS,
                "spvc_compiler_compile failed: {rc} ({})",
                last_error(context)
            );
            assert!(!source.is_null(), "compile produced a null source pointer");

            let glsl = std::ffi::CStr::from_ptr(source)
                .to_string_lossy()
                .into_owned();
            assert!(!glsl.is_empty(), "cross-compiled GLSL is empty");
            assert!(
                glsl.contains("main"),
                "cross-compiled GLSL lacks a main entry point:\n{glsl}"
            );

            // 5. One destroy tears down the whole hierarchy (ctx owns all).
            spvc_context_destroy(context);
        }
    }
}
