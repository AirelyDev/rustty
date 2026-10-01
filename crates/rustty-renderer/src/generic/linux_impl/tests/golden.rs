//! Tests for `generic::linux_impl` (ghostty src/renderer/generic.zig, the
//! `Renderer(OpenGL)` arm) — the offscreen golden renders (cursor/fg/bg, kitty image
//! and eviction, custom shaders).

use super::*;

/// L4 T11 MILESTONE GATE: render a deterministic terminal Screen OFFSCREEN into a
/// GL FBO (via the monomorphic `Renderer(OpenGL)`'s `draw_frame`), read the pixels
/// back with `Target::read_rgba`, and assert the STRUCTURE the port is responsible
/// for (background color region, foreground glyph presence, block cursor at grid
/// (2,0)). The OpenGL twin of `macos_impl::tests::golden_renders_cursor_fg_bg`
/// (generic.rs:6520) — SAME input ("Ab" on 10×3), SAME structural facts.
///
/// NOT A CROSS-API BYTE COMPARISON: this deliberately does NOT compare GL bytes
/// against the Metal golden's bytes. They are different rendering pipelines
/// (llvmpipe software rasterizer here vs. Metal there) and are NOT expected to
/// produce identical pixels — only the same STRUCTURAL result. The thresholds are
/// the Metal golden's own THRESHOLD PHILOSOPHY (structural, lenient enough to
/// survive AA/rounding differences across GPU backends — generic.rs:6478-6490),
/// applied fresh to the GL readback, never a literal byte-diff.
///
/// RUNS FOR REAL only under `LIBGL_ALWAYS_SOFTWARE=1` (llvmpipe ⇒ GL 4.5 core on
/// this host); CLEAN-SKIPS (never panics, never fabricates a golden) on the
/// default virgl driver, which exposes no desktop core profile (Task-1 finding).
/// Holds the process-wide `test_support::gl_lock` for the whole context lifetime
/// (see its docs); `_guard` declared FIRST so it drops LAST (after `deinit`
/// destroys the context).
#[test]
fn golden_renders_cursor_fg_bg() {
    let _guard = test_support::gl_lock();

    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // GL-context skip
    };

    // "Ab" on a 10×3 grid ⇒ a block cursor at grid (2,0) (the empty cell after
    // "Ab"; focused + blink_visible ⇒ block). Verified by the T10 test.
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"Ab");
        s.deinit();
    }
    let st = state::State {
        mutex: &mut mutex,
        terminal: &mut t,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: state::Mouse::default(),
        // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    };
    // Drive the full pipeline through update_frame: fills cells + uniforms;
    // rebuild_cells sets dirty. cursor_blink_visible=true ⇒ visible block cursor.
    r.update_frame(&st, true).unwrap();
    // sync ⇒ the frame is drawn + `gl.finish()` lands the pixels + the completion
    // trampoline runs.
    r.draw_frame(true).unwrap();

    // Read back the FBO pixels of the frame just drawn (RGBA, rows tight).
    let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
    let px = r.swap_chain.as_ref().unwrap().frames[idx]
        .target
        .read_rgba();
    // llvmpipe's pixels, declared defined for memcheck (a no-op natively). Its
    // blending multiplies the destination by (1 - source alpha) = 0, and memcheck
    // cannot tell that product is defined, so it marks read-back pixels that hold
    // their expected values undefined (NEW-FINDINGS N412; the C program
    // `.port/tools/valgrind-llvmpipe-probe/probe.c` gets the same report with no
    // Rustty code). Only these bytes are exempted: every other value this test
    // branches on stays checked (review L-6 F2).
    rustty_zigstd::valgrind::memcheck::make_mem_defined(&px);

    let w = r.size.screen.width as usize;
    let h = r.size.screen.height as usize;
    let cw = r.grid_metrics.cell_width as usize;
    let ch = r.grid_metrics.cell_height as usize;
    assert_eq!(px.len(), w * h * 4);

    // GL Y-FLIP (the one real divergence from the Metal golden): the projection is a
    // top-left-origin `ortho2d` (generic.zig `updateScreenSizeUniforms`) so grid row 0
    // renders to the TOP of the FBO — but OpenGL's `glReadPixels` reads BOTTOM-UP
    // (origin bottom-left), so a top-down grid pixel row `y` lands at readback row
    // `h-1-y`. (Metal's IOSurface readback is already top-down, so its golden samples
    // `y` directly.) `sample(x, y)` samples the terminal-grid pixel (x, y).
    let sample = |x: usize, y: usize| px_at(&px, w, x, h - 1 - y);

    // (A) CURSOR CELL — solid red block at grid (2,0). Sample the cell interior
    //     center. Can only pass if `cell_text` ran with `is_cursor_glyph` set +
    //     `cursor_color` red reached the shader ⇒ proves the packed `bools` byte
    //     (bit 1) AND the uniform serialization.
    let (rr, g, b, _a) = sample(2 * cw + cw / 2, ch / 2);
    assert!(rr > 128, "cursor cell not bright-red (R={rr})");
    assert!(
        rr > g && rr > b,
        "cursor cell not red-dominant (R={rr} G={g} B={b})"
    );

    // (B) BACKGROUND — config bg is black (0,0,0), opacity 1.0 ⇒ bg_color =
    //     [0,0,0,255]. Sample an empty cell far from glyphs/cursor (grid (8,2),
    //     bottom row): cell_bg is transparent there (alpha 0) ⇒ the opaque
    //     full-screen bg_color layer shows through. The RGB check ALONE is VACUOUS
    //     (config bg RGB == clear RGB == 0), so we ALSO assert the ALPHA: the
    //     bg_color step writes an opaque layer (a=255) whereas the render-pass clear
    //     leaves a=0 ⇒ `ba > 128` can only pass if the full-screen bg_color step ran
    //     AND `bg_color.a = 255` serialized (offset 128 + 3).
    let (br, bg2, bb, ba) = sample(8 * cw + cw / 2, 2 * ch + ch / 2);
    assert!(
        br < 96 && bg2 < 96 && bb < 96,
        "background not dark (R={br} G={bg2} B={bb})"
    );
    assert!(
        ba > 128,
        "bg_color step absent / bg alpha not opaque (A={ba})"
    );

    // (C) FOREGROUND GLYPH present in row 0 — "A" at grid (0,0) is white. Scan the
    //     cell for a bright mark (antialiased glyph ⇒ max luminance over the cell).
    //     Proves the atlas sync + `cell_text` glyph sampling.
    let mut maxlum = 0u32;
    for yy in 0..ch {
        for xx in 0..cw {
            let (r3, g3, b3, _) = sample(xx, yy);
            maxlum = maxlum.max((r3 as u32 + g3 as u32 + b3 as u32) / 3);
        }
    }
    assert!(
        maxlum > 128,
        "no fg glyph rendered in cell (0,0) (maxlum={maxlum})"
    );

    deinit_test_renderer(&mut r);
    t.deinit(Global.into());
}

/// F-005 PROVING TEST — the OpenGL twin of `macos_impl::tests::
/// kitty_image_pixel_path` (generic.rs:7498). Drives a real `Terminal` with a
/// Kitty-graphics transmit+display APC, runs `update_frame` (which enters
/// `image::State::kitty_update` through the `kitty_requires_update` guard) then
/// `draw_frame` (which uploads the pending image to the GPU), and asserts the
/// STATE the path produces.
///
/// BEFORE THIS WAVE the whole path was absent on Linux: `linux_impl` had no
/// `images` field, so a transmitted Kitty image was parsed and stored by the vt
/// layer and then silently dropped by the renderer (F-005).
///
/// The assertions (a)-(d) are ported VERBATIM from the macOS twin — which
/// asserts STATE, not pixels — plus one GL-ONLY EXTRA: a pixel readback proving
/// the `.kitty_below_text` band actually reached the framebuffer. There is no
/// macOS-twin analogue for the extra; it follows the structural-threshold
/// philosophy of `golden_renders_cursor_fg_bg` (a region/color assertion with
/// tolerance, never a byte-diff against Metal).
///
/// Live-GL: clean-skips via `golden_renderer`, hard-fails under
/// `RUSTTY_REQUIRE_GL=1`. `_guard` first so it drops last (after `deinit`).
#[test]
fn kitty_image_pixel_path() {
    let _guard = test_support::gl_lock();

    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // GL-context skip
    };

    // Transmit AND display (a=T) a 1x2 direct RGB image (f=24), id 1, all
    // pixels 0xFF (`////////` = 6 bytes of 0xFF). a=T places it at the
    // cursor → a pin placement, marking storage dirty.
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
    // The vt `Terminal` starts with `width_px`/`height_px == 0` (the port
    // sets them only through the surface). Kitty `pixel_size` divides by the
    // cell size (`width_px / cols`), so pin the pixel dims to the renderer's
    // cell metrics — exactly what a real surface resize would do — else the
    // placement's dest size is 0 and it is (correctly) culled.
    t.width_px = r.grid_metrics.cell_width * 10;
    t.height_px = r.grid_metrics.cell_height * 3;
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b_Ga=T,f=24,i=1,s=1,v=2,c=1,r=1;////////\x1b\\");
        s.deinit();
    }

    // Sanity: the image + a placement now live in storage, dirty is set.
    let storage = unsafe { &(*t.screens.active).kitty_images };
    assert!(storage.image_by_id(1).is_some(), "image not in storage");
    assert!(!storage.placements.is_empty(), "no placement in storage");
    assert!(storage.dirty, "storage not marked dirty by the APC");

    // Drive the renderer. `terminal_state.update` snapshots the terminal;
    // `update_frame` → `kitty_requires_update` (dirty) == true → `kitty_update`.
    r.terminal_state.update(Global.into(), &mut t).unwrap();
    {
        let st = kitty_state(&mut mutex, &mut t);
        r.update_frame(&st, true).unwrap();
    }

    // (a) the terminal-walk + containment math ran → a placement was computed.
    assert!(
        !r.images.kitty_placements.is_empty(),
        "kitty_update computed no placements"
    );
    // (b) `prep_kitty_image` queued the image (pending) or it is already up.
    let entry = r
        .images
        .images
        .get(&crate::image::Id::Kitty(1))
        .expect("no Id::Kitty(1) image entry after kitty_update");
    assert!(
        entry.image.has_texture() || entry.image.is_pending(),
        "image neither pending nor uploaded after kitty_update"
    );
    // (c) `kitty_update`'s tail cleared `storage.dirty` (through the raw ptr).
    assert!(
        !unsafe { &(*t.screens.active).kitty_images }.dirty,
        "kitty_update did not clear storage.dirty"
    );

    // `draw_frame` uploads pending images → `convert` (rgb→rgba) +
    // `Texture::init` run → the image is Ready with a GL texture.
    r.draw_frame(true).unwrap();
    assert!(
        r.images
            .images
            .get(&crate::image::Id::Kitty(1))
            .expect("image entry vanished")
            .image
            .has_texture(),
        "image has no GPU texture after draw_frame (convert/upload did not run)"
    );

    // EXTRA (GL-only, no macOS-twin analogue): the image was not merely uploaded
    // — the `.kitty_below_text` band actually DREW it. The placement is a 1-cell
    // wide, 1-row tall, all-0xFF (white) image at the cursor, i.e. grid (0,0) on
    // an otherwise black background. Sample the cell center and require a bright
    // pixel. Structural threshold (>128 on all three channels), never a byte
    // comparison against Metal — llvmpipe and Metal are different rasterizers.
    // Y-FLIP as in `golden_renders_cursor_fg_bg`: `glReadPixels` is bottom-up.
    let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
    let px = r.swap_chain.as_ref().unwrap().frames[idx]
        .target
        .read_rgba();
    let w = r.size.screen.width as usize;
    let h = r.size.screen.height as usize;
    let cw = r.grid_metrics.cell_width as usize;
    let ch = r.grid_metrics.cell_height as usize;
    assert_eq!(px.len(), w * h * 4);
    let (ir, ig, ib, _ia) = px_at(&px, w, cw / 2, h - 1 - ch / 2);
    assert!(
        ir > 128 && ig > 128 && ib > 128,
        "the kitty image was uploaded but never drawn: cell (0,0) is \
         (R={ir} G={ig} B={ib}), expected the all-0xFF white image"
    );

    deinit_test_renderer(&mut r);
    t.deinit(Global.into());
}

/// F-005 PROVING TEST (eviction half) — GL-ONLY, no macOS-twin analogue.
///
/// Covers the third leg of the oracle's image lifecycle: upload → draw →
/// **evict**. After a drawn image exists, the Kitty delete-all APC (`a=d`) makes
/// the vt layer drop the placement/image; the next `update_frame` re-walks the
/// terminal and `kitty_update`'s prune marks the orphaned renderer-side image for
/// unload, and the next `draw_frame`'s upload sweep removes it from the map and
/// deletes its GL texture (`image.zig:73-78`, `State::upload`'s `isUnloading`
/// arm). Without the twin's explicit `Texture::deinit` this would leak a GL
/// texture per transmitted image for the life of the surface.
///
/// Live-GL: clean-skips via `golden_renderer`, hard-fails under
/// `RUSTTY_REQUIRE_GL=1`.
#[test]
fn kitty_eviction_removes_image() {
    let _guard = test_support::gl_lock();

    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // GL-context skip
    };

    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
    t.width_px = r.grid_metrics.cell_width * 10;
    t.height_px = r.grid_metrics.cell_height * 3;
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b_Ga=T,f=24,i=1,s=1,v=2,c=1,r=1;////////\x1b\\");
        s.deinit();
    }

    // Frame 1: transmit + place + upload.
    r.terminal_state.update(Global.into(), &mut t).unwrap();
    {
        let st = kitty_state(&mut mutex, &mut t);
        r.update_frame(&st, true).unwrap();
    }
    r.draw_frame(true).unwrap();
    assert!(
        r.images
            .images
            .get(&crate::image::Id::Kitty(1))
            .is_some_and(|e| e.image.has_texture()),
        "precondition: the image must be uploaded before we evict it"
    );

    // Delete everything — `d=A` (capital) is "all placements AND the image
    // data" (`command::Delete::All(true)`); the lowercase `d=a` default would
    // drop only the placements and KEEP the image in storage, which is not an
    // eviction. Then re-drive the renderer.
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b_Ga=d,d=A\x1b\\");
        s.deinit();
    }
    r.terminal_state.update(Global.into(), &mut t).unwrap();
    {
        let st = kitty_state(&mut mutex, &mut t);
        r.update_frame(&st, true).unwrap();
    }
    // The placement is gone from the renderer's own list right away.
    assert!(
        r.images.kitty_placements.is_empty(),
        "kitty_update kept a placement for a deleted image"
    );
    assert!(
        match r.images.images.get(&crate::image::Id::Kitty(1)) {
            None => true,
            Some(e) => e.image.is_unloading(),
        },
        "the orphaned image was not marked for unload"
    );

    // The upload sweep inside `draw_frame` performs the eviction (and the GL
    // texture delete).
    r.draw_frame(true).unwrap();
    assert!(
        r.images.images.is_empty(),
        "the unloaded image was not evicted from the renderer image map"
    );

    deinit_test_renderer(&mut r);
    t.deinit(Global.into());
}

// Shared driver for the custom-shader goldens (F-013): write the given shadertoy
// body to a nanos-suffixed temp file, point the config at it, `init_shaders`
// (load + transpile GLSL + build the post pipelines + flip `has_custom_shaders`),
// then drive one "Ab" frame through `update_frame` + `draw_frame`. Returns the
// renderer, the terminal, and the readback of the frame just drawn (RGBA, rows
// tight, bottom-up per `glReadPixels`). Returns None on the GL-context skip.
//
// The OpenGL twin of `macos_impl::tests::golden_renders_custom_shader`
// (generic.rs:7042); CALLER holds `test_support::gl_lock`.
fn drive_custom_shader_frame(shader_body: &[u8]) -> Option<(Renderer, Terminal, Vec<u8>)> {
    use std::ffi::CString;

    let mut r = golden_renderer(10, 3)?;

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("rustty_gl_shader_{nanos}.glsl"));
    std::fs::write(&path, shader_body).unwrap();

    // Point the config at the on-disk shader (required arm), byte-faithful
    // path -> CString (mirrors the macOS custom-shader golden).
    let cpath = CString::new(path.as_os_str().as_encoded_bytes().to_vec()).unwrap();
    r.config.custom_shaders = RepeatablePath {
        value: vec![crate::generic::Path::Required(cpath)],
    };

    // Load + transpile + build the post pipelines + flip has_custom_shaders.
    r.init_shaders().expect("init_shaders");
    assert!(r.has_custom_shaders, "custom shaders not activated");
    assert!(
        !r.shaders.post_pipelines.is_empty(),
        "post pipelines not built"
    );

    // Drive one frame: "Ab" on 10×3 ⇒ block cursor at grid (2,0) (the T10/T11
    // fixture), visible glyphs + a red cursor cell.
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"Ab");
        s.deinit();
    }
    let st = state::State {
        mutex: &mut mutex,
        terminal: &mut t,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: state::Mouse::default(),
        // D4 handoff (oracle d34b54e9b) - reference field defaults are `.init(0)`.
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    };
    r.update_frame(&st, true).unwrap();
    r.draw_frame(true).unwrap();

    // The draw_frame setup block lazily created the per-frame CustomShaderState
    // (the config had no shaders at `Renderer::init`, so `FrameState::init` did
    // not — this is the F-013 wiring, absent before the fix).
    let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
    assert!(
        r.swap_chain.as_ref().unwrap().frames[idx]
            .custom_shader_state
            .is_some(),
        "custom_shader_state not created by draw_frame"
    );

    let px = r.swap_chain.as_ref().unwrap().frames[idx]
        .target
        .read_rgba();
    let _ = std::fs::remove_file(&path);
    Some((r, t, px))
}

/// F-013 golden 1/2 (the macOS `golden_renders_custom_shader` twin): a
/// PASS-THROUGH shadertoy post-process shader preserves the cells — the base
/// pass retargets to `back_texture` and the post pass blits it to
/// `frame.target`. Same robustness contract as the macOS twin (non-black AND
/// non-uniform — an empty-pipeline black frame fails the first, a uniform fill
/// fails the second; NOT an exact byte-diff, since the pass-through samples
/// through a GL sampler and sub-LSB filtering may perturb bytes) PLUS the
/// structural cursor fact: the red cursor cell survives the identity shader.
///
/// RUNS FOR REAL only under `LIBGL_ALWAYS_SOFTWARE=1` (llvmpipe); CLEAN-SKIPS
/// otherwise. Holds `test_support::gl_lock`; `_guard` first so it drops last.
#[test]
fn golden_renders_custom_shader_passthrough() {
    let _guard = test_support::gl_lock();

    let Some((mut r, mut t, px)) = drive_custom_shader_frame(
        b"void mainImage(out vec4 fragColor, in vec2 fragCoord){\n\
          fragColor = texture(iChannel0, fragCoord / iResolution.xy);\n}\n",
    ) else {
        return; // GL-context skip
    };
    // `px` is `drive_custom_shader_frame`'s `read_rgba()`, returned as is.
    // llvmpipe's pixels, declared defined for memcheck (a no-op natively). Its
    // blending multiplies the destination by (1 - source alpha) = 0, and memcheck
    // cannot tell that product is defined, so it marks read-back pixels that hold
    // their expected values undefined (NEW-FINDINGS N412; the C program
    // `.port/tools/valgrind-llvmpipe-probe/probe.c` gets the same report with no
    // Rustty code). Only these bytes are exempted: every other value this test
    // branches on stays checked (review L-6 F2).
    rustty_zigstd::valgrind::memcheck::make_mem_defined(&px);

    let w = r.size.screen.width as usize;
    let h = r.size.screen.height as usize;
    let cw = r.grid_metrics.cell_width as usize;
    let ch = r.grid_metrics.cell_height as usize;
    assert_eq!(px.len(), w * h * 4);
    let sample = |x: usize, y: usize| px_at(&px, w, x, h - 1 - y);

    // Non-black: some pixel has a bright channel (the white "Ab" glyphs /
    // red cursor made it through the pass-through).
    let non_black = px
        .chunks_exact(4)
        .any(|p| p[0] > 64 || p[1] > 64 || p[2] > 64);
    assert!(
        non_black,
        "cells not visible through the pass-through shader"
    );
    // Not uniform: at least one pixel differs from the first (glyph vs bg).
    let first = &px[0..4];
    let non_uniform = px.chunks_exact(4).any(|p| p != first);
    assert!(
        non_uniform,
        "frame is a uniform fill (post-process did not preserve the image)"
    );
    // Structural: the block cursor at grid (2,0) is still bright-red-dominant
    // (identity shader ⇒ the T11 golden's cursor fact holds).
    let (rr, g, b, _a) = sample(2 * cw + cw / 2, ch / 2);
    assert!(rr > 128, "cursor cell not bright-red (R={rr})");
    assert!(
        rr > g && rr > b,
        "cursor cell not red-dominant (R={rr} G={g} B={b})"
    );

    deinit_test_renderer(&mut r);
    t.deinit(Global.into());
}

/// F-013 golden 2/2, the BEHAVIORAL proof the post-process pass actually
/// transforms pixels (rustty-added; ghostty has no oracle test here): a
/// color-INVERTING shader turns the dark background bright and the red cursor
/// cyan. Before the F-013 wiring the base pass rendered straight to
/// `frame.target` and the post loop never ran, so the background stayed dark —
/// this test fails without the fix and passes with it.
///
/// RUNS FOR REAL only under `LIBGL_ALWAYS_SOFTWARE=1` (llvmpipe); CLEAN-SKIPS
/// otherwise. Holds `test_support::gl_lock`; `_guard` first so it drops last.
#[test]
fn golden_custom_shader_inverts_colors() {
    let _guard = test_support::gl_lock();

    let Some((mut r, mut t, px)) = drive_custom_shader_frame(
        b"void mainImage(out vec4 fragColor, in vec2 fragCoord){\n\
          vec4 c = texture(iChannel0, fragCoord / iResolution.xy);\n\
          fragColor = vec4(1.0 - c.rgb, 1.0);\n}\n",
    ) else {
        return; // GL-context skip
    };

    let w = r.size.screen.width as usize;
    let h = r.size.screen.height as usize;
    let cw = r.grid_metrics.cell_width as usize;
    let ch = r.grid_metrics.cell_height as usize;
    assert_eq!(px.len(), w * h * 4);
    let sample = |x: usize, y: usize| px_at(&px, w, x, h - 1 - y);

    // The empty background cell (8,2) was near-black (T11 golden fact B);
    // inverted it must be BRIGHT on every channel. Without the post pass this
    // samples the un-transformed dark background and fails.
    let (br, bg2, bb, _ba) = sample(8 * cw + cw / 2, 2 * ch + ch / 2);
    assert!(
        br > 128 && bg2 > 128 && bb > 128,
        "background not inverted to bright (R={br} G={bg2} B={bb})"
    );

    // The red cursor cell (2,0) inverts to cyan: green/blue dominant over red.
    let (rr, g, b, _a) = sample(2 * cw + cw / 2, ch / 2);
    assert!(
        g > rr && b > rr,
        "cursor cell not inverted to cyan-dominant (R={rr} G={g} B={b})"
    );

    deinit_test_renderer(&mut r);
    t.deinit(Global.into());
}
