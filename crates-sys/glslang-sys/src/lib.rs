//! glslang-sys — FFI bindings to the vendored glslang GLSL/HLSL -> SPIR-V
//! compiler (`links = "glslang"`).
//!
//! PORT STATUS: vendored (full library)
//!
//! R3: this crate copies the SAME C++ library the reference uses — glslang
//! 14.2.0 — into `vendor/` and builds it standalone (`cc` in C++17 mode +
//! `bindgen`); see build.rs. No pure-Rust substitution during parity phases.
//!
//! The FFI surface is bindgen-generated from `glslang/Include/glslang_c_interface.h`
//! and `glslang/Public/resource_limits_c.h` (the same two headers the
//! reference's `pkg/glslang/c.zig` @cImports) and re-exported verbatim below.
//! Downstream, the renderer's shader-compilation path builds on these raw
//! symbols to turn GLSL into SPIR-V.

#![allow(non_upper_case_globals)]
#![allow(non_camel_case_types)]
#![allow(non_snake_case)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));

#[cfg(test)]
mod kat {
    use super::*;

    /// Known-answer test: initialize the glslang process, compile a trivial but
    /// valid GLSL vertex shader (`void main() {}`) to SPIR-V through the C
    /// interface, and assert the produced module is well-formed — non-empty
    /// SPIR-V words whose first word is the SPIR-V magic number
    /// (`0x0723_0203`) — with no error in the shader info log. This exercises
    /// the full vendored pipeline (`glslang_initialize_process` ->
    /// `glslang_shader_create`/`preprocess`/`parse` -> `glslang_program_link`
    /// -> `glslang_program_SPIRV_generate`/`get`), proving the standalone C++
    /// build + FFI link are correct. Mirrors the reference's
    /// pkg/glslang/{init,shader,program}.zig flow.
    #[test]
    fn glslang_compiles_trivial_vertex_shader_to_spirv() {
        // The SPIR-V binary magic number, little-endian first word of every
        // SPIR-V module (SPIR-V spec 2.3 "Physical Layout").
        const SPIRV_MAGIC: u32 = 0x0723_0203;

        // Minimal valid Vulkan GLSL vertex shader.
        let source =
            std::ffi::CString::new("#version 450\nvoid main() {}\n").expect("CString source");

        unsafe {
            // 1. Process init — the reference treats a zero return as failure.
            let rc = glslang_initialize_process();
            assert_ne!(rc, 0, "glslang_initialize_process failed");

            // 2. Build the compilation input (mirrors shader.zig's test input,
            //    retargeted to the vertex stage).
            let resource = glslang_default_resource();
            assert!(
                !resource.is_null(),
                "glslang_default_resource returned null"
            );

            let mut input: glslang_input_t = std::mem::zeroed();
            input.language = GLSLANG_SOURCE_GLSL as _;
            input.stage = GLSLANG_STAGE_VERTEX as _;
            input.client = GLSLANG_CLIENT_VULKAN as _;
            input.client_version = GLSLANG_TARGET_VULKAN_1_2 as _;
            input.target_language = GLSLANG_TARGET_SPV as _;
            input.target_language_version = GLSLANG_TARGET_SPV_1_5 as _;
            input.code = source.as_ptr();
            input.default_version = 100;
            input.default_profile = GLSLANG_NO_PROFILE as _;
            input.force_default_version_and_profile = 0;
            input.forward_compatible = 0;
            input.messages = GLSLANG_MSG_DEFAULT_BIT as _;
            input.resource = resource;

            // 3. Create + preprocess + parse the shader.
            let shader = glslang_shader_create(&input);
            assert!(!shader.is_null(), "glslang_shader_create returned null");

            let info_log = |sh| {
                let p = glslang_shader_get_info_log(sh);
                if p.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
                }
            };

            let pp = glslang_shader_preprocess(shader, &input);
            assert_ne!(pp, 0, "preprocess failed: {}", info_log(shader));

            let parsed = glslang_shader_parse(shader, &input);
            assert_ne!(parsed, 0, "parse failed: {}", info_log(shader));

            // The shader info log must not report an error (glslang prefixes
            // real diagnostics with "ERROR:").
            let log = info_log(shader);
            assert!(
                !log.contains("ERROR:"),
                "shader info log reported an error: {log}"
            );

            // 4. Link into a program.
            let program = glslang_program_create();
            assert!(!program.is_null(), "glslang_program_create returned null");
            glslang_program_add_shader(program, shader);

            let linked = glslang_program_link(program, GLSLANG_MSG_DEFAULT_BIT as _);
            assert_ne!(linked, 0, "link failed: {}", {
                let p = glslang_program_get_info_log(program);
                if p.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(p).to_string_lossy().into_owned()
                }
            });

            // 5. Generate SPIR-V and read it back.
            glslang_program_SPIRV_generate(program, GLSLANG_STAGE_VERTEX as _);
            let size = glslang_program_SPIRV_get_size(program);
            assert!(size > 0, "SPIR-V module is empty");

            let mut words = vec![0u32; size];
            glslang_program_SPIRV_get(program, words.as_mut_ptr());
            assert_eq!(
                words[0], SPIRV_MAGIC,
                "first SPIR-V word is not the magic number"
            );

            // 6. Tear down (reverse of construction), then finalize the process.
            glslang_program_delete(program);
            glslang_shader_delete(shader);
            glslang_finalize_process();
        }
    }
}
