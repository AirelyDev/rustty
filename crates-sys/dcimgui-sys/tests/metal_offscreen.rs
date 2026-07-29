//! Offscreen Metal render KAT for the vendored Dear ImGui Metal backend
//! (`vendor/imgui/backends/imgui_impl_metal.mm`, compiled by `build.rs` into
//! the `dcimgui_metal` archive on macOS).
//!
//! The unit-test `link_imgui_impl_metal_ffi` in `src/lib.rs` only proves the
//! `ImGui_ImplMetal_*` symbols RESOLVE. This KAT proves the backend actually
//! RASTERIZES: it builds a real `MTLDevice`, an offscreen BGRA8 texture, a
//! render pass that clears it to opaque black, runs one full ImGui frame that
//! draws a window, encodes the resulting `ImDrawData` through
//! `ImGui_ImplMetal_RenderDrawData`, commits, waits, reads the texture back,
//! and asserts non-black pixels exist.
//!
//! This is the WS6 Phase-A evidence that ⌘⌥I can render at all. Metal is
//! Darwin-only, so the whole file compiles to nothing elsewhere (the GTK /
//! OpenGL3 inspector build is untouched).

#![cfg(target_os = "macos")]
#![allow(non_snake_case)]

use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Mutex;

use objc2_metal::{
    MTLCommandBuffer, MTLCommandEncoder, MTLCommandQueue, MTLCreateSystemDefaultDevice, MTLDevice,
    MTLLoadAction, MTLOrigin, MTLPixelFormat, MTLRegion, MTLRenderPassDescriptor, MTLSize,
    MTLStorageMode, MTLStoreAction, MTLTexture, MTLTextureDescriptor, MTLTextureUsage,
};

use dcimgui_sys::{
    ImGui_Begin, ImGui_CreateContext, ImGui_DestroyContext, ImGui_End, ImGui_GetDrawData,
    ImGui_GetIO, ImGui_ImplMetal_Init, ImGui_ImplMetal_NewFrame, ImGui_ImplMetal_RenderDrawData,
    ImGui_ImplMetal_Shutdown, ImGui_NewFrame, ImGui_Render, ImGui_SetCurrentContext,
    ImGui_SetNextWindowPos, ImGui_SetNextWindowSize, ImGui_StyleColorsDark, ImGui_Text, ImVec2,
};

/// Dear ImGui's current context is a plain global (`GImGui`); `cargo test`
/// runs each `#[test]` on its own thread. Serialize any test that creates a
/// context — same discipline as `src/lib.rs`'s `IMGUI_CONTEXT_TEST_LOCK`.
static IMGUI_CONTEXT_TEST_LOCK: Mutex<()> = Mutex::new(());

/// Framebuffer edge length, in pixels, for the offscreen render target.
const DIM: usize = 256;

#[test]
fn metal_offscreen_render_writes_pixels() {
    let _guard = IMGUI_CONTEXT_TEST_LOCK
        .lock()
        .unwrap_or_else(|e| e.into_inner());

    // --- Metal device + queue -------------------------------------------
    let Some(device) = MTLCreateSystemDefaultDevice() else {
        // No Metal device (should not happen on this hardware, but a headless
        // CI box without a GPU must not hard-fail the suite).
        eprintln!("skipping: MTLCreateSystemDefaultDevice() returned nil");
        return;
    };
    let queue = device
        .newCommandQueue()
        .expect("newCommandQueue returned nil");

    // --- Offscreen BGRA8 render target (Shared storage => CPU readback) ---
    let tex_desc = MTLTextureDescriptor::new();
    tex_desc.setPixelFormat(MTLPixelFormat::BGRA8Unorm);
    // SAFETY: objc2 marks the dimension setters unsafe; DIM is a plain size.
    unsafe {
        tex_desc.setWidth(DIM);
        tex_desc.setHeight(DIM);
    }
    tex_desc.setStorageMode(MTLStorageMode::Shared);
    tex_desc.setUsage(MTLTextureUsage::RenderTarget | MTLTextureUsage::ShaderRead);
    let texture = device
        .newTextureWithDescriptor(&tex_desc)
        .expect("newTextureWithDescriptor returned nil");

    // --- Render pass: clear to opaque black, store the result ------------
    let pass = MTLRenderPassDescriptor::renderPassDescriptor();
    {
        let attachments = pass.colorAttachments();
        // SAFETY: index 0 is always valid on a render pass descriptor; objc2
        // marks the subscript unsafe because it is not bounds-checked.
        let a0 = unsafe { attachments.objectAtIndexedSubscript(0) };
        a0.setTexture(Some(&texture));
        a0.setLoadAction(MTLLoadAction::Clear);
        a0.setStoreAction(MTLStoreAction::Store);
        a0.setClearColor(objc2_metal::MTLClearColor {
            red: 0.0,
            green: 0.0,
            blue: 0.0,
            alpha: 1.0,
        });
    }

    // --- ImGui context + Metal backend -----------------------------------
    // SAFETY: single-threaded within this test (context lock held); every
    // pointer handed to Dear ImGui below is a live local.
    unsafe {
        let ctx = ImGui_CreateContext(std::ptr::null_mut());
        assert!(!ctx.is_null(), "ImGui_CreateContext(NULL) returned null");
        ImGui_SetCurrentContext(ctx);

        let io = ImGui_GetIO();
        assert!(!io.is_null(), "ImGui_GetIO() returned null");
        (*io).DisplaySize = ImVec2 {
            x: DIM as f32,
            y: DIM as f32,
        };
        (*io).DisplayFramebufferScale = ImVec2 { x: 1.0, y: 1.0 };
        (*io).DeltaTime = 1.0 / 60.0;
        ImGui_StyleColorsDark(std::ptr::null_mut());

        // `device` is an `id<MTLDevice>`; the FFI takes it untyped.
        let device_id: *mut c_void = objc2::rc::Retained::as_ptr(&device) as *mut c_void;
        assert!(
            ImGui_ImplMetal_Init(device_id),
            "ImGui_ImplMetal_Init returned false"
        );

        // --- One full frame ------------------------------------------------
        let pass_ptr: *mut c_void = objc2::rc::Retained::as_ptr(&pass) as *mut c_void;
        ImGui_ImplMetal_NewFrame(pass_ptr);
        ImGui_NewFrame();

        ImGui_SetNextWindowPos(ImVec2 { x: 8.0, y: 8.0 }, 0);
        ImGui_SetNextWindowSize(
            ImVec2 {
                x: (DIM as f32) - 16.0,
                y: (DIM as f32) - 16.0,
            },
            0,
        );
        let opened = ImGui_Begin(c"rustty-inspector-kat".as_ptr(), std::ptr::null_mut(), 0);
        assert!(opened, "ImGui_Begin returned false for a visible window");
        ImGui_Text(c"WS6 offscreen metal KAT".as_ptr());
        ImGui_End();

        ImGui_Render();
        let draw_data = ImGui_GetDrawData();
        assert!(!draw_data.is_null(), "ImGui_GetDrawData() returned null");

        // --- Encode + submit ------------------------------------------------
        let command_buffer = queue.commandBuffer().expect("commandBuffer returned nil");
        let encoder = command_buffer
            .renderCommandEncoderWithDescriptor(&pass)
            .expect("renderCommandEncoderWithDescriptor returned nil");

        let cb_ptr: *mut c_void = objc2::rc::Retained::as_ptr(&command_buffer) as *mut c_void;
        let enc_ptr: *mut c_void = objc2::rc::Retained::as_ptr(&encoder) as *mut c_void;
        ImGui_ImplMetal_RenderDrawData(draw_data, cb_ptr, enc_ptr);

        encoder.endEncoding();
        command_buffer.commit();
        command_buffer.waitUntilCompleted();

        // --- Read the texture back ------------------------------------------
        let mut pixels = vec![0u8; DIM * DIM * 4];
        texture.getBytes_bytesPerRow_fromRegion_mipmapLevel(
            NonNull::new(pixels.as_mut_ptr().cast::<c_void>()).unwrap(),
            DIM * 4,
            MTLRegion {
                origin: MTLOrigin { x: 0, y: 0, z: 0 },
                size: MTLSize {
                    width: DIM,
                    height: DIM,
                    depth: 1,
                },
            },
            0,
        );

        // Known answer: the ImGui window drew SOMETHING over the black clear.
        // Count pixels whose BGR differs from the clear (0,0,0).
        let non_black = pixels
            .chunks_exact(4)
            .filter(|px| px[0] != 0 || px[1] != 0 || px[2] != 0)
            .count();
        assert!(
            non_black > 1000,
            "ImGui_ImplMetal_RenderDrawData wrote {non_black} non-black pixels \
             (expected > 1000 for a {DIM}x{DIM} window fill) — the Metal backend did not rasterize"
        );

        ImGui_ImplMetal_Shutdown();
        ImGui_DestroyContext(ctx);
    }
}
