//! Tests for `generic::linux_impl` (ghostty src/renderer/generic.zig, the
//! `Renderer(OpenGL)` arm) — the F-012 background image and the F-020 inspector
//! overlay.

use super::*;

// ===================================================================
// F-012 — background image (w2 WI-4)
// ===================================================================

/// F-012 PROVING TEST — the OpenGL twin of `macos_impl::tests::
/// golden_renders_background_image` (generic.rs:7277). Point the config at a real
/// PNG on disk, run `prep_background_image` (open + `FileType` sniff + wuffs
/// decode → `bg_image = Some(Pending)`) and `update_bg_image_buffer`
/// (opacity/fit → `BgImageGpu`), then `draw_frame` — whose
/// `upload_background_image` turns the pending image into a `Ready(Texture)` and
/// whose base pass takes the `bg_image` step INSTEAD of the `bg_color` step. Read
/// the FBO back and assert the sampled pixel is the DECODED image color, distinct
/// from the plain black `bg_color` the negative control shows.
///
/// BEFORE THIS WAVE `background-image` was accepted by the config and its vertex
/// buffer was synced, but nothing on Linux ever decoded, uploaded, or drew it
/// (F-012) — this test could not even be written, because `bg_image` was
/// permanently `None`.
///
/// DETERMINISM PINS (mirroring the cursor golden): `AlphaBlending::Native`,
/// `colorspace = Srgb`, and `fit = Stretch` so the solid image fills the whole
/// screen ⇒ every pixel is the image color (trivial sampling).
///
/// Live-GL: clean-skips via `golden_renderer`, hard-fails under
/// `RUSTTY_REQUIRE_GL=1`. `_guard` first so it drops last (after `deinit`).
#[test]
fn golden_renders_background_image() {
    use std::ffi::CString;

    let _guard = test_support::gl_lock();

    // The distinct bg-image color (blue-dominant), unambiguously != black.
    const IMG_RGB: [u8; 3] = [0x30, 0x60, 0xC0];

    // Build a golden renderer, optionally with a bg image at `path`, draw one
    // frame, and return the center-pixel RGBA of the bottom row.
    fn draw_center(with_bg: Option<&std::path::Path>) -> Option<(u8, u8, u8, u8)> {
        let mut r = golden_renderer(10, 3)?;

        if let Some(p) = with_bg {
            // Point the config at the on-disk PNG (required arm) + stretch it.
            let bytes = p.as_os_str().as_encoded_bytes().to_vec();
            r.config.bg_image = Some(crate::generic::Path::Required(CString::new(bytes).unwrap()));
            r.config.bg_image_fit = BackgroundImageFit::Stretch;
            r.config.bg_image_opacity = 1.0;
            // Decode + build the vertex/opacity buffer (the two calls
            // `change_config` would make on a bg-image change).
            r.prep_background_image().expect("prep_background_image");
            r.update_bg_image_buffer();
            // The decode must have produced a pending bg image.
            assert!(
                matches!(r.bg_image, Some(crate::image::Image::Pending(_))),
                "prep did not stage a pending bg image"
            );
        }

        // An (empty) terminal drives update_frame/draw_frame (as the cursor
        // golden does); the bg-image step does not depend on cell content.
        let mut mutex = Mutex::new();
        let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
        {
            let st = kitty_state(&mut mutex, &mut t);
            r.update_frame(&st, true).unwrap();
        }
        r.draw_frame(true).unwrap();

        if with_bg.is_some() {
            // After the frame, the pending image was uploaded to the GPU.
            assert!(
                matches!(r.bg_image, Some(crate::image::Image::Ready(_))),
                "bg image not uploaded to a Ready texture"
            );
        }

        let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
        let px = r.swap_chain.as_ref().unwrap().frames[idx]
            .target
            .read_rgba();
        let w = r.size.screen.width as usize;
        let h = r.size.screen.height as usize;
        let cw = r.grid_metrics.cell_width as usize;
        let ch = r.grid_metrics.cell_height as usize;
        // Sample the center of an empty cell on the bottom row (grid (5,2)).
        // Y-FLIP: `glReadPixels` is bottom-up (see `golden_renders_cursor_fg_bg`).
        let out = px_at(&px, w, 5 * cw + cw / 2, h - 1 - (2 * ch + ch / 2));
        deinit_test_renderer(&mut r);
        t.deinit(Global.into());
        Some(out)
    }

    // Write the solid bg-image PNG to a temp file.
    let png_bytes = encode_solid_png(2, 2, IMG_RGB);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("rustty_gl_bg_{nanos}.png"));
    std::fs::write(&path, &png_bytes).unwrap();

    // (bg image) — sample should be the decoded image color.
    let bg = match draw_center(Some(&path)) {
        Some(v) => v,
        None => {
            let _ = std::fs::remove_file(&path);
            return; // GL-context skip
        }
    };
    // (negative control) — no bg image ⇒ the plain black bg_color.
    let none = draw_center(None).expect("GL probed above");
    let _ = std::fs::remove_file(&path);

    // `read_rgba` returns (R, G, B, A). The image is blue-dominant
    // (0x30,0x60,0xC0): B ≈ 0xC0 (>128), R ≈ 0x30 (<128), B > R, opaque.
    // Structural thresholds, never a byte-diff against the Metal golden.
    let (br, bg2, bb, ba) = bg;
    assert!(bb > 128, "bg-image blue channel too low (B={bb})");
    assert!(br < 128, "bg-image red channel too high (R={br})");
    assert!(bb > br, "bg-image not blue-dominant (B={bb} R={br})");
    assert!(ba > 128, "bg-image not opaque (A={ba})");

    // UD354 (DECISIONS M5): under `native` blending the image's sRGB bytes
    // must reach the target unchanged (within rounding), not encoded a second
    // time. The reference's always-sRGB render target turns #3060C0 into
    // #78A5E1, which the structural checks above still accept.
    for (got, want, channel) in [
        (br, IMG_RGB[0], 'R'),
        (bg2, IMG_RGB[1], 'G'),
        (bb, IMG_RGB[2], 'B'),
    ] {
        assert!(
            got.abs_diff(want) <= 2,
            "bg-image {channel}={got} is not the image's {want} (sRGB encoded twice?)"
        );
    }

    // The negative control is dark (black bg_color) — proves the bg-image step
    // (not the bg_color fallback) produced the color above.
    let (nr, ng, nb, _na) = none;
    assert!(
        nr < 96 && ng < 96 && nb < 96,
        "negative control not dark (R={nr} G={ng} B={nb})"
    );
    assert!(
        bb > nb + 64,
        "bg-image blue not distinct from bg_color (bg B={bb} vs none B={nb})"
    );
}

/// F-012 — the CPU half: config → bg-image vertex/uniform buffer mapping + the
/// `modified` counter. The OpenGL twin of `macos_impl::tests::
/// update_bg_image_buffer_maps_config` (generic.rs:7766). This code was already
/// live on Linux before the wave, but it had NO Linux-side test; it is the
/// fit/position/repeat/opacity coverage the GPU-side sampling (the shader's job)
/// rests on.
#[test]
fn update_bg_image_buffer_maps_config() {
    let _guard = test_support::gl_lock();

    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // GL-context skip
    };
    // DIVERGENCE from the Metal twin's assertion (`== 1`): the macOS test builds
    // its renderer through a test-only ctor, whereas `golden_renderer` goes
    // through the REAL `Renderer::init`, which already called
    // `update_bg_image_buffer` once (generic.zig:794). So assert the INCREMENT,
    // which is the actual invariant the per-frame sync gate reads.
    let before = r.bg_image_buffer_modified;
    r.config.bg_image_position = BackgroundImagePosition::CenterCenter;
    r.config.bg_image_fit = BackgroundImageFit::Cover;
    r.config.bg_image_repeat = true;
    r.update_bg_image_buffer();
    assert_eq!(r.bg_image_buffer.info.position, BgImageInfoPosition::Mc);
    assert_eq!(r.bg_image_buffer.info.fit, BgImageInfoFit::Cover);
    assert!(r.bg_image_buffer.info.repeat);
    assert_eq!(r.bg_image_buffer_modified, before + 1);

    deinit_test_renderer(&mut r);
}

/// F-012 — the "no path" arm of `prep_background_image` (oracle
/// `generic.zig:1825-1829`): with `config.bg_image == None` an existing image is
/// marked for unload, and the next `draw_frame`'s `upload_background_image`
/// FREES it (`bg_image = None`). This is the config-cleared path, which the
/// golden above does not exercise.
#[test]
fn clearing_bg_image_config_unloads_the_image() {
    use std::ffi::CString;

    let _guard = test_support::gl_lock();

    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // GL-context skip
    };

    let png_bytes = encode_solid_png(2, 2, [0x30, 0x60, 0xC0]);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("rustty_gl_bg_clear_{nanos}.png"));
    std::fs::write(&path, &png_bytes).unwrap();

    r.config.bg_image = Some(crate::generic::Path::Required(
        CString::new(path.as_os_str().as_encoded_bytes().to_vec()).unwrap(),
    ));
    r.prep_background_image().expect("prep_background_image");
    assert!(
        matches!(r.bg_image, Some(crate::image::Image::Pending(_))),
        "prep did not stage a pending bg image"
    );

    // Now clear the path and re-prep: `markForUnload` on the pending image.
    r.config.bg_image = None;
    r.prep_background_image().expect("prep (cleared)");
    assert!(
        r.bg_image.as_ref().is_some_and(|i| i.is_unloading()),
        "clearing the config did not mark the bg image for unload"
    );

    // The frame's `upload_background_image` frees it.
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
    {
        let st = kitty_state(&mut mutex, &mut t);
        r.update_frame(&st, true).unwrap();
    }
    r.draw_frame(true).unwrap();
    assert!(
        r.bg_image.is_none(),
        "upload_background_image did not free the unloading bg image"
    );

    let _ = std::fs::remove_file(&path);
    deinit_test_renderer(&mut r);
    t.deinit(Global.into());
}

/// F-012 — `change_config`'s `bg_image_changed` branch (oracle
/// `generic.zig:1878-1897`): a config whose `bg_image` path DIFFERS from the
/// current one must run `prep_background_image`, and an identical path must not
/// re-decode. This is the runtime (`ChangeConfig` message) entry point, distinct
/// from the `init` entry the golden drives.
#[test]
fn change_config_reprepares_the_bg_image() {
    use std::ffi::CString;

    let _guard = test_support::gl_lock();

    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // GL-context skip
    };

    let png_bytes = encode_solid_png(2, 2, [0x30, 0x60, 0xC0]);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("rustty_gl_bg_cfg_{nanos}.png"));
    std::fs::write(&path, &png_bytes).unwrap();
    let cpath = CString::new(path.as_os_str().as_encoded_bytes().to_vec()).unwrap();

    // Precondition: the renderer was built with no bg image.
    assert!(r.bg_image.is_none());

    // None -> Some(path) IS a change ⇒ decode.
    let mut cfg = derived_config_for_test();
    cfg.bg_image = Some(crate::generic::Path::Required(cpath.clone()));
    r.change_config(cfg).expect("change_config");
    assert!(
        matches!(r.bg_image, Some(crate::image::Image::Pending(_))),
        "change_config did not decode the new bg-image path"
    );

    // Upload it so the state machine has a TEXTURE — that is what makes the
    // next two arms distinguishable (`mark_for_replace` on a textured image
    // yields `Replace{texture, pending}`; on an untextured one it is just
    // `Pending` again, which would make this test vacuous).
    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
    {
        let st = kitty_state(&mut mutex, &mut t);
        r.update_frame(&st, true).unwrap();
    }
    r.draw_frame(true).unwrap();
    assert!(
        matches!(r.bg_image, Some(crate::image::Image::Ready(_))),
        "the bg image did not upload"
    );

    // Some(path) -> Some(SAME path) is NOT a change ⇒ `prep_background_image`
    // must NOT run: the image stays plain `Ready`.
    let mut same = derived_config_for_test();
    same.bg_image = Some(crate::generic::Path::Required(cpath));
    r.change_config(same).expect("change_config (same path)");
    assert!(
        matches!(r.bg_image, Some(crate::image::Image::Ready(_))),
        "an unchanged bg-image path re-entered prep_background_image"
    );

    // Some(A) -> Some(B) IS a change ⇒ re-decode + `mark_for_replace`, which on a
    // textured image is the `Replace{texture, pending}` shape (pending AND
    // textured at once).
    let path2 = std::env::temp_dir().join(format!("rustty_gl_bg_cfg2_{nanos}.png"));
    std::fs::write(&path2, encode_solid_png(2, 2, [0xC0, 0x20, 0x20])).unwrap();
    let mut other = derived_config_for_test();
    other.bg_image = Some(crate::generic::Path::Required(
        CString::new(path2.as_os_str().as_encoded_bytes().to_vec()).unwrap(),
    ));
    r.change_config(other).expect("change_config (new path)");
    let bg = r.bg_image.as_ref().expect("bg image vanished");
    assert!(
        bg.is_pending() && bg.has_texture(),
        "a changed bg-image path did not re-enter prep_background_image"
    );

    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(&path2);
    deinit_test_renderer(&mut r);
    t.deinit(Global.into());
}

/// F-012 — the wuffs-error → `Error::ImageDecode` mapping the `?` in
/// `prep_background_image` propagates. PURE (no GL, no renderer): runs on every
/// host, including this macOS dev box's cross-compile lanes' Linux twin builds.
#[test]
fn wuffs_error_maps_to_image_decode() {
    assert_eq!(
        Error::from(rustty_wuffs::Error::OutOfMemory),
        Error::OutOfMemory
    );
    assert_eq!(
        Error::from(rustty_wuffs::Error::WuffsError),
        Error::ImageDecode
    );
    assert_eq!(
        Error::from(rustty_wuffs::Error::Overflow),
        Error::ImageDecode
    );
}

// ===================================================================
// F-020 — inspector renderer overlay (w2 WI-5)
// ===================================================================

/// A stand-in for the inspector's `widgets::renderer::Info`, which is the real
/// `InspectorOverlay` impl. The renderer may never name a `rustty-inspector`
/// type (R4 acyclicity), so the seam is exercised through a test-local impl —
/// exactly the shape core coerces the real `Info` into.
struct FakeInspector(Vec<crate::overlay::Feature>);
impl state::InspectorOverlay for FakeInspector {
    fn overlay_features(&self) -> Vec<crate::overlay::Feature> {
        self.0.clone()
    }
}

/// F-020 PROVING TEST — the inspector's live renderer-overlay controls now have
/// an OpenGL consumer. With a `state.inspector` attached and a feature enabled,
/// `update_frame` must (1) read the features through the `InspectorOverlay`
/// seam, (2) `rebuild_overlay` a z2d surface, and (3) `images.overlay_update`
/// stage it as an `Id::Overlay` placement; `draw_frame` must then upload it and
/// take the `.overlay` draw band.
///
/// BEFORE THIS WAVE all four stages were milestone-scope-cut on Linux: the
/// inspector checkbox wrote a feature nobody read (F-020).
///
/// The negative control (`inspector: None`) proves the assertions are not
/// vacuous — `overlay` stays `None` and no overlay placement is staged.
///
/// Live-GL: clean-skips via `golden_renderer`, hard-fails under
/// `RUSTTY_REQUIRE_GL=1`.
#[test]
fn overlay_draws_on_inspector_features() {
    let _guard = test_support::gl_lock();

    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // GL-context skip
    };

    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), term_opts(10, 3)).unwrap();
    {
        // OSC 133;A marks row 0 as a shell PROMPT — the row state
        // `Overlay::highlight_semantic_prompts` looks for. It draws a
        // `min(5, cell_width)`-wide bar down the LEFT edge of the prompt rows,
        // which is what the pixel assertion below samples.
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]133;A\x1b\\Ab");
        s.deinit();
    }

    // (negative control) no inspector ⇒ no features ⇒ no overlay at all.
    {
        let st = kitty_state(&mut mutex, &mut t);
        r.update_frame(&st, true).unwrap();
    }
    assert!(
        r.overlay.is_none(),
        "an overlay was built with no inspector attached"
    );
    assert!(
        r.images.overlay_placements.is_empty(),
        "an overlay placement was staged with no inspector attached"
    );
    // Guard against a vacuous pixel comparison later: the prompt row must
    // actually be marked, else the overlay would legitimately draw nothing.
    assert_ne!(
        r.terminal_state.row_data[0].raw.semantic_prompt(),
        rustty_vt::page::SemanticPrompt::None,
        "OSC 133;A did not mark row 0 as a prompt"
    );
    // Baseline frame WITHOUT the overlay.
    r.draw_frame(true).unwrap();
    let before = {
        let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
        r.swap_chain.as_ref().unwrap().frames[idx]
            .target
            .read_rgba()
    };

    // (positive) attach an inspector reporting one enabled feature.
    let mut fake = FakeInspector(vec![crate::overlay::Feature::SemanticPrompts]);
    {
        let mut st = kitty_state(&mut mutex, &mut t);
        st.inspector = Some(&mut fake as *mut dyn state::InspectorOverlay);
        r.update_frame(&st, true).unwrap();
    }

    // (1)+(2) the features reached `rebuild_overlay`, which built a surface at
    // the terminal size.
    let overlay = r
        .overlay
        .as_ref()
        .expect("rebuild_overlay built no overlay");
    let term = r.size.terminal();
    assert_eq!(overlay.surface.get_width() as u32, term.width);
    assert_eq!(overlay.surface.get_height() as u32, term.height);

    // (3) `images.overlay_update` staged the `Id::Overlay` image + placement.
    assert_eq!(
        r.images.overlay_placements.len(),
        1,
        "overlay_update staged no overlay placement"
    );
    assert!(
        r.images.images.contains_key(&crate::image::Id::Overlay),
        "overlay_update staged no Id::Overlay image"
    );

    // `draw_frame`'s upload sweep turns it into a real GL texture, and the
    // `.overlay` band draws it (the band is guarded on `self.overlay.is_some()`,
    // which now holds).
    r.draw_frame(true).unwrap();
    assert!(
        r.images
            .images
            .get(&crate::image::Id::Overlay)
            .expect("overlay image entry vanished")
            .image
            .has_texture(),
        "the overlay image was never uploaded to a GL texture"
    );

    // (4) the `.overlay` DRAW band actually reached the framebuffer. Sample the
    // left-edge prompt bar (`highlight_semantic_prompts` draws it at pixel x
    // 0..min(5, cell_width) of the prompt rows) and require it to differ from the
    // overlay-off baseline. Structural (a difference, with a margin), never a
    // byte-diff against Metal — llvmpipe is a different rasterizer.
    // Y-FLIP as in `golden_renders_cursor_fg_bg`: `glReadPixels` is bottom-up.
    let after = {
        let idx = r.swap_chain.as_ref().unwrap().frame_index as usize;
        r.swap_chain.as_ref().unwrap().frames[idx]
            .target
            .read_rgba()
    };
    let w = r.size.screen.width as usize;
    let h = r.size.screen.height as usize;
    let ch = r.grid_metrics.cell_height as usize;
    assert_eq!(before.len(), after.len());
    let y = h - 1 - ch / 2; // vertical center of grid row 0
    let (b0, b1, b2, _) = px_at(&before, w, 2, y);
    let (a0, a1, a2, _) = px_at(&after, w, 2, y);
    let delta = (a0 as i32 - b0 as i32).abs()
        + (a1 as i32 - b1 as i32).abs()
        + (a2 as i32 - b2 as i32).abs();
    assert!(
        delta > 16,
        "the overlay was uploaded but never drawn: prompt-bar pixel is \
         ({a0},{a1},{a2}) with the overlay vs ({b0},{b1},{b2}) without \
         (delta={delta})"
    );

    deinit_test_renderer(&mut r);
    t.deinit(Global.into());
}

/// F-020 — the direct `rebuild_overlay` lifecycle, twin of `macos_impl::tests::
/// rebuild_overlay_builds_and_clears` (generic.rs:7632): empty features build
/// nothing, a real feature builds an overlay at the terminal size, and clearing
/// tears it down again. Needs a `Renderer`, hence live-GL clean-skip.
#[test]
fn rebuild_overlay_builds_and_clears() {
    let _guard = test_support::gl_lock();

    let mut r = match golden_renderer(10, 3) {
        Some(r) => r,
        None => return, // GL-context skip
    };

    // empty features → overlay stays None
    r.rebuild_overlay(&[]).unwrap();
    assert!(r.overlay.is_none());

    // A real feature builds the overlay at the terminal size.
    r.rebuild_overlay(&[crate::overlay::Feature::SemanticPrompts])
        .unwrap();
    assert!(r.overlay.is_some());

    // A second call with the SAME size takes the `reset()` fast path (oracle
    // `'existing`), not a rebuild — the overlay survives.
    r.rebuild_overlay(&[crate::overlay::Feature::SemanticPrompts])
        .unwrap();
    assert!(r.overlay.is_some());

    // and clearing removes it
    r.rebuild_overlay(&[]).unwrap();
    assert!(r.overlay.is_none());

    deinit_test_renderer(&mut r);
}
