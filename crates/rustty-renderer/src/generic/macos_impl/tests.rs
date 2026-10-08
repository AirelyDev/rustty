//! Unit tests for `generic::macos_impl`, the `Renderer(Metal)` arm of ghostty
//! src/renderer/generic.zig: the shared fixtures, the display-link test seams, and the
//! Chunk 3b/3c unit tests. The topic tests live in `tests/`.

use super::draw::frame_completed_on;
use super::*;
use crate::generic::{Color, RepeatablePath};
use crate::link;
use crate::size::{
    AlphaBlending, BackgroundBlur, BackgroundImageFit, BackgroundImagePosition, CellSize,
    FontShapingBreak, GridSize, Padding, ScreenSize, Size, WindowColorspace,
};
use rustty_font_shaper::codepoint_resolver::{CodepointResolver, DescriptorCache, StyleStatus};
use rustty_font_shaper::collection::{AddOptions, Collection, LoadOptions, SizeAdjustment};
use rustty_font_shaper::face::{
    DesiredSize, Face, Options as FaceOptions, FREETYPE_LOAD_FLAGS_DEFAULT,
};
use rustty_font_shaper::library::Library;
use rustty_font_shaper::Style as FontStyle;
use rustty_vt::kitty;
use rustty_vt::modes::ModePacked;
use rustty_vt::terminal::{Colors, Options as TermOptions, Terminal};
use rustty_zigstd::arena_allocator::ArenaAllocator;
use rustty_zigstd::thread::Mutex;

// PORT NOTE: reuse the exact `opts(cols, rows)` helper from the cell.rs /
// cursor.rs test modules (the established test-helper duplication precedent).
fn opts(cols: CellCountInt, rows: CellCountInt) -> TermOptions {
    TermOptions {
        cols,
        rows,
        max_scrollback_bytes: Some(10_000),
        max_scrollback_lines: None,
        colors: Colors::default(),
        default_modes: ModePacked::default(),
        default_cursor_style: rustty_vt::screen::CursorStyle::Block,
        default_cursor_blink: Some(false),
        default_xt_checksum: Default::default(),
        kitty_image_storage_limit: 320 * 1000 * 1000,
        kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
    }
}

// GPU_RD_CHUNK3B_DESIGN §E.2: the 3b test (and the Chunk-4 golden) construct a
// `DerivedConfig` literal with fixed values; `DerivedConfig::init<C>` stays a
// deferred config-parsing seam. The four search colors mirror the reference
// config defaults (Config.zig:1116/1117/1131/1132); `cursor-color` defaults to
// null in the reference but we set it red so `add_cursor` renders a
// deterministic, visible color for the golden (GOLDEN-GATE PROOF, §E.2).
fn derived_config_for_test() -> DerivedConfig {
    DerivedConfig {
        arena: ArenaAllocator::init(),
        font_thicken: false,
        font_thicken_strength: 0,
        font_features: Vec::new(),
        font_styles: StyleStatus::init_fill(true),
        font_shaping_break: FontShapingBreak::default(),
        cursor_color: Some(TerminalColor::Color(Color {
            r: 0xFF,
            g: 0,
            b: 0,
        })),
        cursor_opacity: 1.0,
        cursor_text: None,
        background: color::RGB { r: 0, g: 0, b: 0 },
        background_opacity: 1.0,
        background_opacity_cells: false,
        foreground: color::RGB {
            r: 0xFF,
            g: 0xFF,
            b: 0xFF,
        },
        selection_background: None,
        selection_foreground: None,
        search_background: TerminalColor::Color(Color {
            r: 0xFF,
            g: 0xE0,
            b: 0x82,
        }),
        search_foreground: TerminalColor::Color(Color { r: 0, g: 0, b: 0 }),
        search_selected_background: TerminalColor::Color(Color {
            r: 0xF2,
            g: 0xA5,
            b: 0x7E,
        }),
        search_selected_foreground: TerminalColor::Color(Color { r: 0, g: 0, b: 0 }),
        bold_color: None,
        faint_opacity: 255,
        min_contrast: 1.0,
        padding_color: WindowPaddingColor::Background,
        custom_shaders: RepeatablePath::default(),
        bg_image: None,
        bg_image_opacity: 1.0,
        bg_image_position: BackgroundImagePosition::TopLeft,
        bg_image_fit: BackgroundImageFit::Contain,
        bg_image_repeat: false,
        // VERIFIED (§E.2): `link::Set::empty()` does NOT exist; `Set { links }`
        // has a pub field, so construct the empty (Global-allocated) set directly.
        links: link::Set { links: Vec::new() },
        vsync: false,
        colorspace: WindowColorspace::Srgb,
        blending: AlphaBlending::Native,
        background_blur: BackgroundBlur::False,
        scroll_to_bottom_on_output: false,
        custom_shader_animation: crate::thread::CustomShaderAnimation::True,
    }
}

// GPU_RD_CHUNK3B_DESIGN §G.1: assemble an offscreen `Renderer` field-by-field
// (its `init` is deferred — needs a live surface). Returns `None` (skip) on
// hosts with no Metal device (the `metal.rs` precedent). This is the Chunk-4
// golden harness MINUS `drawFrame`.
fn test_renderer(cols: CellCountInt, rows: CellCountInt) -> Option<Renderer> {
    // Headless CI may lack a Metal device → skip cleanly.
    let api = match Metal::init_offscreen(AlphaBlending::Native, 256, 256) {
        Ok(m) => m,
        Err(e) => {
            // F5 strict-mode pivot (metal::test_support): a no-op by
            // default, a panic under `RUSTTY_REQUIRE_METAL=1`.
            crate::metal::test_support::require_metal_or_skip("test_renderer", &format!("{e:?}"));
            eprintln!("no Metal device; skipping");
            return None;
        }
    };
    let shaders = api.init_shaders(&[]).expect("init_shaders");
    let swap_chain = SwapChain::init(&api, false).expect("swap_chain");

    // Real font grid over the embedded JetBrains Mono Regular (the
    // `shared_grid.rs::test_grid` recipe). Leaked so the `*mut SharedGrid`
    // stays valid for the test's scope.
    let font_grid: *mut SharedGrid = {
        let lib = Library::init().unwrap();
        let mut c = Collection::init();
        c.load_options = Some(LoadOptions::new(lib));
        c.add(
            Face::init(
                lib,
                rustty_font::embedded::REGULAR,
                FaceOptions {
                    size: DesiredSize {
                        points: 12.0,
                        xdpi: 96,
                        ydpi: 96,
                    },
                    freetype_load_flags: FREETYPE_LOAD_FLAGS_DEFAULT,
                },
            )
            .unwrap(),
            AddOptions {
                style: FontStyle::Regular,
                fallback: false,
                size_adjustment: SizeAdjustment::None,
            },
        )
        .unwrap();
        let resolver = CodepointResolver {
            collection: c,
            styles: StyleStatus::init_fill(true),
            discover: None,
            codepoint_map: None,
            descriptor_cache: DescriptorCache::default(),
            sprite: None,
        };
        let grid = SharedGrid::init(Global, resolver).unwrap();
        Box::leak(Box::new(grid)) as *mut SharedGrid
    };

    // grid.metrics is loaded under the grid lock during `SharedGrid::init`.
    let grid_metrics = unsafe { (*font_grid).metrics };
    let font_shaper = Shaper::init(shape::Options::default()).expect("shaper");

    let cell = CellSize {
        width: grid_metrics.cell_width,
        height: grid_metrics.cell_height,
    };
    let size = Size {
        screen: ScreenSize {
            width: cell.width * cols as u32,
            height: cell.height * rows as u32,
        },
        cell,
        padding: Padding {
            top: 0,
            bottom: 0,
            right: 0,
            left: 0,
        },
    };

    let mut cells = Contents::default();
    cells.resize(GridSize {
        rows,
        columns: cols,
    });

    Some(Renderer {
        alloc: Global,
        draw_mutex: DrawMutex::new(),
        latest_frame: Arc::new(LatestFrame::new()),
        presentation_health: AtomicHealth::new(crate::Health::Healthy),
        config: derived_config_for_test(),
        surface_mailbox: SurfaceMailbox::DETACHED,
        grid_metrics,
        size,
        focused: true,
        visible: true,
        custom_shader_focused_changed: false,
        scrollbar: Scrollbar::ZERO,
        scrollbar_dirty: false,
        last_bottom_node: None,
        last_bottom_y: 0,
        search_matches: None,
        search_selected_match: None,
        search_matches_dirty: false,
        cells,
        cells_rebuilt: AtomicBool::new(false),
        // The fill only writes grid_size/padding_extend/cursor_pos/cursor_color/
        // bg_color; a zeroed (valid POD) `Uniforms` suffices for the rest.
        uniforms: unsafe { core::mem::zeroed() },
        custom_shader_uniforms: unsafe { core::mem::zeroed() },
        first_frame_time: None,
        last_frame_time: None,
        font_grid,
        font_shaper,
        font_shaper_cache: ShaperCache::init(),
        images: image::State::empty(),
        bg_image: None,
        bg_image_changed: false,
        bg_image_buffer: unsafe { core::mem::zeroed() },
        bg_image_buffer_modified: 0,
        api,
        display_link: None,
        health: Arc::new(AtomicHealth::new(crate::Health::Healthy)),
        health_report: Arc::new(HealthReport::new()),
        display_realized: true,
        swap_chain: Some(swap_chain),
        kitty_animation_clock: None,
        kitty_animation_next_ms: None,
        target_config_modified: 0,
        reinitialize_shaders: false,
        has_custom_shaders: false,
        shaders,
        terminal_state: RenderState::empty(),
        terminal_state_frame_count: 0,
        overlay: None,
        overlay_spare: None,
    })
}

// Build a leaked embedded-font `SharedGrid` + the pixel `Size` for a
// `cols`×`rows` grid. Factored (W1-7 de-dup) so `golden_renderer` and the
// windowed-lifecycle KAT share the identical grid recipe. Metal-independent
// (the caller does its own Metal probe). The grid is leaked so the returned
// `*mut SharedGrid` outlives the test scope. The `size.screen` is PINNED to the
// grid's pixel size so that (in `golden_renderer`) `init_offscreen`'s
// `offscreen_size == size.screen` ⇒ `draw_frame`'s `size_changed == false`
// (§3.2: projection 1:1; cell (x,y) at pixel [x*cw..(x+1)*cw) × [y*ch..(y+1)*ch)).
fn test_font_grid_and_size(cols: CellCountInt, rows: CellCountInt) -> (*mut SharedGrid, Size) {
    // Real font grid over the embedded JetBrains Mono Regular. Leaked so the
    // `*mut SharedGrid` stays valid for the test's scope.
    let font_grid: *mut SharedGrid = {
        let lib = Library::init().unwrap();
        let mut c = Collection::init();
        c.load_options = Some(LoadOptions::new(lib));
        c.add(
            Face::init(
                lib,
                rustty_font::embedded::REGULAR,
                FaceOptions {
                    size: DesiredSize {
                        points: 12.0,
                        xdpi: 96,
                        ydpi: 96,
                    },
                    freetype_load_flags: FREETYPE_LOAD_FLAGS_DEFAULT,
                },
            )
            .unwrap(),
            AddOptions {
                style: FontStyle::Regular,
                fallback: false,
                size_adjustment: SizeAdjustment::None,
            },
        )
        .unwrap();
        let resolver = CodepointResolver {
            collection: c,
            styles: StyleStatus::init_fill(true),
            discover: None,
            codepoint_map: None,
            descriptor_cache: DescriptorCache::default(),
            sprite: None,
        };
        let grid = SharedGrid::init(Global, resolver).unwrap();
        Box::leak(Box::new(grid)) as *mut SharedGrid
    };

    let grid_metrics = unsafe { (*font_grid).metrics };
    let cell = CellSize {
        width: grid_metrics.cell_width,
        height: grid_metrics.cell_height,
    };
    let size = Size {
        screen: ScreenSize {
            width: cell.width * cols as u32,
            height: cell.height * rows as u32,
        },
        cell,
        padding: Padding {
            top: 0,
            bottom: 0,
            right: 0,
            left: 0,
        },
    };
    (font_grid, size)
}

// GPU_RD_CHUNK4_DESIGN §3.2/§6.5: the GOLDEN harness = `test_renderer` with the
// offscreen size PINNED to the grid's pixel size (`size.screen`), so
// `draw_frame`'s `surface_size == self.size.screen` ⇒ `size_changed == false`
// (no in-`draw_frame` resize / `update_screen_size_uniforms` surprise;
// projection 1:1; cell (x,y) lands at pixel [x*cw..(x+1)*cw) × [y*ch..(y+1)*ch)
// — trivial pixel math). So we build the font grid + metrics + `size` FIRST,
// THEN call `init_offscreen(blending, size.screen.width, size.screen.height)`.
fn golden_renderer(cols: CellCountInt, rows: CellCountInt) -> Option<Renderer> {
    // Probe: skip cleanly on hosts with no Metal device (headless CI), BEFORE
    // the (Metal-independent) font-grid build. `init_offscreen` returns
    // `Err(NoMetalDevice)` on a headless host (the `metal.rs` / `test_renderer`
    // precedent).
    if let Err(e) = Metal::init_offscreen(AlphaBlending::Native, 1, 1) {
        // F5 strict-mode pivot (metal::test_support).
        crate::metal::test_support::require_metal_or_skip("golden_renderer", &format!("{e:?}"));
        eprintln!("no Metal device; skipping golden");
        return None;
    }

    let (font_grid, size) = test_font_grid_and_size(cols, rows);

    // DE-DUP (W1-7): delegate to the now-filled `Renderer::init` instead of
    // hand-building the field literal. `golden_renderer` PINS
    // `offscreen_size == size.screen` (§3.2) — EXACTLY what `Renderer::init`
    // builds (`init_offscreen(config.blending, size.screen.width,
    // size.screen.height)`), and `derived_config_for_test()` uses
    // `blending = Native` (⇒ `BGRA8Unorm`, no store-time sRGB re-encode),
    // `colorspace = Srgb`, and EMPTY `custom_shaders`, so the delegated
    // `api`/`shaders`/`swap_chain`/`uniforms` are field-for-field identical to
    // the former hand-built literal (bg_color/bools/min_contrast are overwritten
    // by `update_frame` before `draw_frame`; all are no-op-equal for this
    // config anyway). `init` also runs `update_font_grid_uniforms` +
    // `update_screen_size_uniforms` (fixing the projection/cell uniforms — else
    // every vertex collapses to the origin) + `update_bg_image_buffer` +
    // `prep_background_image` (a no-op with `bg_image = None`). `cells` starts
    // empty and is sized by the first `update_frame`/`rebuild_cells`. Metal was
    // probed above, so `init` succeeds — the failure path is an `.expect`.
    let opts = Options {
        config: derived_config_for_test(),
        font_grid,
        size,
        // Offscreen: no surface, hence no mailbox (push returns 0).
        surface_mailbox: SurfaceMailbox::DETACHED,
    };
    Some(Renderer::init(Global, opts).expect("Renderer::init (Metal probed above)"))
}

/// N503 (G-LSAN on macOS, P7-W16a): the macOS twin of N411's Linux helper.
/// Frees a grid built by `test_font_grid_and_size` or `test_renderer`: the
/// grid's `deinit`, the box, then the library (the order `shared_grid`'s own
/// tests use).
fn free_test_font_grid(font_grid: *mut SharedGrid) {
    // SAFETY: `font_grid` came from `Box::leak` in `test_font_grid_and_size` /
    // `test_renderer`, and its only borrower (the renderer) is torn down.
    let mut grid = unsafe { Box::from_raw(font_grid) };
    let mut lib = grid
        .resolver
        .collection
        .load_options
        .as_ref()
        .expect("the test grid sets the load options")
        .library;
    grid.deinit();
    drop(grid);
    lib.deinit();
}

/// `threadExit` + `deinit` for a renderer built on a test grid (every
/// `test_renderer` / `golden_renderer`), then the grid itself: the reference
/// tests' `defer` order, renderer before the grid it borrows. The value is
/// dropped by its owner afterwards, as `RendererBox` does.
fn deinit_test_renderer(r: &mut Renderer) {
    let font_grid = r.font_grid;
    r.thread_exit();
    r.deinit();
    free_test_font_grid(font_grid);
}

mod display_link;
mod golden;
mod health;
mod lifecycle;
mod mailbox;

// ---- `72cf50855` (#14150): the display link and the draw mutex ----------
//
// `draw_frame`'s no-redraw path used to call `sync_display_link` while
// still holding `draw_mutex`, and stopping a CVDisplayLink is a blocking
// join on CoreVideo's IO thread while the main thread's CoreAnimation
// display callback takes the same mutex through `draw_frame(sync = true)`.
// A real CoreVideo/CoreAnimation schedule cannot be driven
// deterministically, so two test-only seams stand in for it (the UD70
// `between_display_stores` pattern): `on_sync_display_link` runs first
// thing in `DisplayLinkSync::apply`, where the display link itself is
// started or stopped (the test double that records whether the mutex is
// held), and `between_draw_unlock_and_sync` runs in `draw_frame_raw`
// after the mutex is released and before the resync. Both exist only in
// this crate's test build and do nothing unless a test on the SAME thread
// installed a hook.

/// A test hook, handed the raw renderer the seam runs on.
type DrawHook = Box<dyn FnMut(*mut Renderer)>;
/// The `apply` test double.
type SyncHook = Box<dyn FnMut()>;

thread_local! {
    static ON_SYNC_DISPLAY_LINK: std::cell::RefCell<Option<SyncHook>> =
        const { std::cell::RefCell::new(None) };
    static BETWEEN_DRAW_UNLOCK_AND_SYNC: std::cell::RefCell<Option<DrawHook>> =
        const { std::cell::RefCell::new(None) };
}

fn run_hook(
    slot: &'static std::thread::LocalKey<std::cell::RefCell<Option<DrawHook>>>,
    this: *mut Renderer,
) {
    let hook = slot.with(|h| h.borrow_mut().take());
    if let Some(mut hook) = hook {
        hook(this);
        slot.with(|h| *h.borrow_mut() = Some(hook));
    }
}

pub(super) fn on_sync_display_link() {
    let hook = ON_SYNC_DISPLAY_LINK.with(|h| h.borrow_mut().take());
    if let Some(mut hook) = hook {
        hook();
        ON_SYNC_DISPLAY_LINK.with(|h| *h.borrow_mut() = Some(hook));
    }
}

pub(super) fn between_draw_unlock_and_sync(this: *mut Renderer) {
    run_hook(&BETWEEN_DRAW_UNLOCK_AND_SYNC, this);
}

#[test]
fn rebuild_cells_fills_bg_fg_cursor() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return, // skip w/o Metal
    };

    // small terminal: print "Ab" on row 0, leave a visible block cursor.
    let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"Ab");
        s.deinit();
    }
    r.terminal_state.update(Global.into(), &mut t).unwrap();
    let cs = cursor::style(
        &r.terminal_state,
        cursor::StyleOptions {
            preedit: false,
            focused: true,
            blink_visible: true,
        },
    );
    r.rebuild_cells(None, cs, &CellSet::new());

    // (1) BG: the printed cell (0,0) has a bg entry; a default-bg cell → alpha 0.
    let bg = r.terminal_state.colors.background;
    assert_eq!(*r.cells.bg_cell(0, 0), [bg.r, bg.g, bg.b, 0]);

    // (2) FG glyph: row 0 fg list (index y+1 == 1) is non-empty (glyphs for A, b).
    assert!(!r.cells.fg_rows[1].is_empty());
    let a = r.cells.fg_rows[1][0];
    assert_eq!(a.grid_pos, [0, 0]);
    assert_eq!(a.atlas, Atlas::Grayscale);
    assert!(a.glyph_size[0] > 0 && a.glyph_size[1] > 0);

    // (3) CURSOR: a block cursor glyph is present, colored the fixture's red
    //     (config.cursor_color). After printing "Ab" the block cursor sits at
    //     column x=2 (the narrow empty cell), so BOTH the cursor uniform and the
    //     cursor glyph grid_pos are [2, 0] (ref :2529-2536).
    let cur = r.cells.get_cursor_glyph().expect("cursor present");
    assert!(cur.bools.is_cursor_glyph);
    assert_eq!(&cur.color[0..3], &[0xFF, 0, 0]);
    assert_eq!(cur.grid_pos, [2, 0]);
    assert_eq!(r.uniforms.cursor_pos, [2, 0]);

    t.deinit(Global.into());
    deinit_test_renderer(&mut r);
}

// 5beb94c16: the preedit glyph is rendered with `font-thicken` and
// `font-thicken-strength`, as normal cells are. The grid caches a glyph
// per render options (`GlyphKey`), so the preedit cell's atlas slot is
// the one a thickened render of the same codepoint resolves to, and not
// the unthickened one.
#[test]
fn preedit_glyph_uses_font_thicken() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return, // skip w/o Metal
    };
    r.config.font_thicken = true;
    r.config.font_thicken_strength = 200;

    let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
    r.terminal_state.update(Global.into(), &mut t).unwrap();
    let cs = cursor::style(
        &r.terminal_state,
        cursor::StyleOptions {
            preedit: true,
            focused: true,
            blink_visible: true,
        },
    );
    let preedit = state::Preedit {
        codepoints: vec![state::Codepoint {
            codepoint: 'A' as u32,
            wide: false,
        }],
    };
    r.rebuild_cells(Some(preedit), cs, &CellSet::new());

    // The preedit sits at the cursor, (0, 0): fg row list index y+1 == 1.
    let cell = *r.cells.fg_rows[1]
        .iter()
        .find(|c| c.grid_pos == [0, 0])
        .expect("preedit glyph at the cursor");

    let render = |thicken: bool| {
        // SAFETY: the test owns the leaked grid; no other borrow is live.
        unsafe { &mut *r.font_grid }
            .render_codepoint(
                Global,
                'A' as u32,
                rustty_font_shaper::Style::Regular,
                Some(Presentation::Text),
                RenderOptions {
                    grid_metrics: r.grid_metrics,
                    thicken,
                    thicken_strength: 200,
                    ..RenderOptions::default()
                },
            )
            .unwrap()
            .expect("A renders")
    };
    let thick = render(true);
    let thin = render(false);
    assert_ne!(
        [thick.glyph.atlas_x, thick.glyph.atlas_y],
        [thin.glyph.atlas_x, thin.glyph.atlas_y],
        "the grid caches thickened and unthickened glyphs apart"
    );
    assert_eq!(
        [thick.glyph.atlas_x, thick.glyph.atlas_y],
        cell.glyph_pos,
        "the preedit glyph is the thickened render"
    );

    t.deinit(Global.into());
    deinit_test_renderer(&mut r);
}

#[test]
fn rebuild_overlay_builds_and_clears() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };

    // empty features → overlay stays None
    r.rebuild_overlay(&[]).unwrap();
    assert!(r.overlay.is_none());

    // A real feature builds the overlay at the terminal size. `rebuild_overlay`
    // sets `self.overlay = Some(new)` BEFORE it calls `apply_features`, whose
    // semantic-prompt render body now draws for real
    // (`overlay.rs::highlight_semantic_prompts` — the z2d hairline fill landed),
    // so we drive the build path directly. Post-build, the overlay is present.
    r.rebuild_overlay(&[crate::overlay::Feature::SemanticPrompts])
        .unwrap();
    assert!(r.overlay.is_some());

    // and clearing removes it
    r.rebuild_overlay(&[]).unwrap();
    assert!(r.overlay.is_none());
    deinit_test_renderer(&mut r);
}

// ===================================================================
// Chunk 3c unit tests (macOS-gated, Metal-skip; GPU_RD_CHUNK3C_DESIGN §F)
// ===================================================================

// §F.2(1) — the headline orchestrator test: build a live `state::State` +
// `Terminal`, drive `update_frame` end-to-end, and assert the same
// Contents/uniforms the direct `rebuild_cells` test checks, now reached
// through the mutex/critical-section walk, plus the bg_color + scrollbar.
#[test]
fn update_frame_fills_cells_uniforms_scrollbar() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };

    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
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

    // Contents filled (same asserts as rebuild_cells_fills_bg_fg_cursor,
    // now driven end-to-end):
    let bg = r.terminal_state.colors.background;
    assert_eq!(*r.cells.bg_cell(0, 0), [bg.r, bg.g, bg.b, 0]);
    assert!(!r.cells.fg_rows[1].is_empty());
    let cur = r.cells.get_cursor_glyph().expect("cursor present");
    assert!(cur.bools.is_cursor_glyph);
    assert_eq!(cur.grid_pos, [2, 0]);
    // written by rebuild_cells inside update_frame
    assert_eq!(r.uniforms.cursor_pos, [2, 0]);

    // bg_color uniform (§C.1-#23): opacity 1.0 → alpha 255.
    assert_eq!(r.uniforms.bg_color, [bg.r, bg.g, bg.b, 255]);

    // The scrollbar snapshot happened inside the critical section; on a
    // small non-scrolling screen the cache matches the terminal's
    // scrollbar (recomputed from the same PageList).
    let expected_scrollbar = unsafe { (*t.screens.active).pages.scrollbar() };
    assert!(r.scrollbar.eql(expected_scrollbar));

    t.deinit(Global.into());
    deinit_test_renderer(&mut r);
}

// Parity S115-F1: `defer self.font_shaper.endFrame()` is updateFrame's
// first statement, so a frame skipped for synchronized output still
// hands the shaper's CoreText objects to the release thread.
//
// Scoped (S-14, N266) to the `.coretext` backend: the release pool is
// the CoreText shaper's (`shaper/coretext.zig`); the HarfBuzz and noop
// shapers' `endFrame` are no-ops with nothing to observe.
#[cfg(rustty_font_backend = "coretext")]
#[test]
fn update_frame_ends_the_shaper_frame_on_the_sync_output_return() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };

    // Shape a row outside `update_frame`, so the shaper's release pool
    // holds this frame's CoreText objects.
    let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"Ab");
        s.deinit();
    }
    r.terminal_state.update(Global.into(), &mut t).unwrap();
    let cs = cursor::style(
        &r.terminal_state,
        cursor::StyleOptions {
            preedit: false,
            focused: true,
            blink_visible: true,
        },
    );
    r.rebuild_cells(None, cs, &CellSet::new());
    assert!(r.font_shaper.release_pool_len() > 0);
    assert_ne!(r.terminal_state.dirty, Dirty::False);

    t.modes.set(Mode::SynchronizedOutput, true);
    let mut mutex = Mutex::new();
    let st = state::State {
        mutex: &mut mutex,
        terminal: &mut t,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: state::Mouse::default(),
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    };
    r.update_frame(&st, true).unwrap();

    // The early return was taken: the dirty reset at the end of the
    // body did not run (it is registered after that return in the
    // reference too).
    assert_ne!(r.terminal_state.dirty, Dirty::False);
    // ...and the shaper's frame was still ended.
    assert_eq!(0, r.font_shaper.release_pool_len());

    t.deinit(Global.into());
    deinit_test_renderer(&mut r);
}

// 9e6e2ea96 — renderer: reset terminal state cleanup counter. The frame
// counter must restart whenever the retained render state is cleared;
// otherwise every subsequent frame deinitializes and rebuilds the full
// state (perf collapse after ~12 minutes at 120Hz).
#[test]
fn update_frame_resets_terminal_state_frame_count() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };

    let mut mutex = Mutex::new();
    let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
    let st = state::State {
        mutex: &mut mutex,
        terminal: &mut t,
        inspector: None,
        inspector_ptr: None,
        preedit: None,
        mouse: state::Mouse::default(),
        demand: Default::default(),
        handoff_gen: Default::default(),
        handoff_park: Default::default(),
    };

    // Drive the counter to the cleanup threshold (the ~12-minutes-at-120Hz
    // mark). This frame performs the SCHEDULED deinit+rebuild...
    r.terminal_state_frame_count = 100_000;
    r.update_frame(&st, true).unwrap();
    // ...and must reset the counter (0, then +1 for this frame). Without
    // the reset the counter stays past the threshold forever and every
    // subsequent frame hits the deinit+rebuild branch.
    assert_eq!(r.terminal_state_frame_count, 1);
    r.update_frame(&st, true).unwrap();
    assert_eq!(r.terminal_state_frame_count, 2);

    t.deinit(Global.into());
    deinit_test_renderer(&mut r);
}

// §F.2(2) — direct: `screen_size`/`grid_padding`/`projection_matrix`.
// (`-1 *` mirrors `update_screen_size_uniforms`'s reference form.)
#[test]
#[allow(clippy::neg_multiply)]
fn update_screen_size_uniforms_projection() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };
    r.update_screen_size_uniforms();

    assert_eq!(
        r.uniforms.screen_size,
        [r.size.screen.width as f32, r.size.screen.height as f32]
    );

    // The fixture has all-zero padding and screen = cols*cell x rows*cell,
    // so there is no leftover and the blank padding is all zero.
    assert_eq!(r.uniforms.grid_padding, [0.0, 0.0, 0.0, 0.0]);

    // Spot-check the projection matrix against `ortho2d` recomputed from
    // the fixture.
    let terminal_size = r.size.terminal();
    let expected = math::ortho2d(
        -1.0 * r.size.padding.left as f32,
        (terminal_size.width + r.size.padding.right) as f32,
        (terminal_size.height + r.size.padding.bottom) as f32,
        -1.0 * r.size.padding.top as f32,
    );
    assert_eq!(r.uniforms.projection_matrix, expected);
    deinit_test_renderer(&mut r);
}

// §F.2(3) — direct: `cell_size`.
// rustty-only (R13, UPSTREAM-DEFECTS.md UD57): `cursor-text`,
// `selection-background` and `selection-foreground` set to
// `cell-foreground` / `cell-background` leave their custom-shader
// uniforms unchanged (the reference panics); a `.color` still writes.
// The OpenGL twin is in `generic/linux_impl.rs`.
#[test]
fn update_custom_shader_uniforms_from_state_cell_relative_colors() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };
    r.has_custom_shaders = true;

    let sentinel = [0.25, 0.5, 0.75, 1.0];
    r.custom_shader_uniforms.cursor_text = sentinel;
    r.custom_shader_uniforms.selection_background_color = sentinel;
    r.custom_shader_uniforms.selection_foreground_color = sentinel;
    r.custom_shader_uniforms.foreground_color = sentinel;
    // Sentinels for the two uniforms written AFTER the three colour
    // blocks. Neither is a value the function can produce, so they
    // pin that the function ran to its end rather than returning
    // early out of a colour block.
    r.custom_shader_uniforms.cursor_visible = -7;
    r.custom_shader_uniforms.current_cursor_style = -7;
    r.config.cursor_text = Some(crate::generic::TerminalColor::CellForeground);
    r.config.selection_background = Some(crate::generic::TerminalColor::CellForeground);
    r.config.selection_foreground = Some(crate::generic::TerminalColor::CellBackground);

    let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
    r.terminal_state.update(Global.into(), &mut t).unwrap(); // dirty = Full
    r.update_custom_shader_uniforms_from_state();

    // Positive reach: the update ran past its early returns.
    let fg = r.terminal_state.colors.foreground;
    assert_eq!(
        r.custom_shader_uniforms.foreground_color,
        [
            fg.r as f32 / 255.0,
            fg.g as f32 / 255.0,
            fg.b as f32 / 255.0,
            1.0
        ]
    );
    assert_eq!(r.custom_shader_uniforms.cursor_text, sentinel);
    assert_eq!(
        r.custom_shader_uniforms.selection_background_color,
        sentinel
    );
    assert_eq!(
        r.custom_shader_uniforms.selection_foreground_color,
        sentinel
    );

    // Positive reach PAST the three colour blocks: everything the
    // function writes after them is written. A skipped uniform must
    // not cost the rest of the update.
    assert_eq!(
        r.custom_shader_uniforms.cursor_visible,
        r.terminal_state.cursor.visible as i32
    );
    assert_ne!(r.custom_shader_uniforms.current_cursor_style, -7);

    // A `.color` value is still written.
    let red = crate::generic::Color { r: 255, g: 0, b: 0 };
    r.config.cursor_text = Some(crate::generic::TerminalColor::Color(red));
    r.config.selection_background = Some(crate::generic::TerminalColor::Color(red));
    r.config.selection_foreground = Some(crate::generic::TerminalColor::Color(red));
    r.update_custom_shader_uniforms_from_state();
    assert_eq!(r.custom_shader_uniforms.cursor_text, [1.0, 0.0, 0.0, 1.0]);
    assert_eq!(
        r.custom_shader_uniforms.selection_background_color,
        [1.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(
        r.custom_shader_uniforms.selection_foreground_color,
        [1.0, 0.0, 0.0, 1.0]
    );

    t.deinit(Global.into());
    deinit_test_renderer(&mut r);
}

#[test]
fn update_font_grid_uniforms_cell_size() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };
    r.update_font_grid_uniforms();
    assert_eq!(
        r.uniforms.cell_size,
        [
            r.grid_metrics.cell_width as f32,
            r.grid_metrics.cell_height as f32
        ]
    );
    deinit_test_renderer(&mut r);
}

// §F.2(4) — direct: config → bg-image-buffer mapping + the modified counter.
#[test]
fn update_bg_image_buffer_maps_config() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };
    r.config.bg_image_position = BackgroundImagePosition::CenterCenter;
    r.config.bg_image_fit = BackgroundImageFit::Cover;
    r.config.bg_image_repeat = true;
    r.update_bg_image_buffer();
    assert_eq!(r.bg_image_buffer.info.position, BgImageInfoPosition::Mc);
    assert_eq!(r.bg_image_buffer.info.fit, BgImageInfoFit::Cover);
    assert!(r.bg_image_buffer.info.repeat);
    assert_eq!(r.bg_image_buffer_modified, 1);
    deinit_test_renderer(&mut r);
}

// §F.2(5) — direct: custom-shader-from-state palette/colors/cursor.
#[test]
fn update_custom_shader_uniforms_from_state_palette() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };
    r.has_custom_shaders = true;

    let mut t = Terminal::init(Global.into(), opts(10, 3)).unwrap();
    r.terminal_state.update(Global.into(), &mut t).unwrap(); // dirty = Full
    r.update_custom_shader_uniforms_from_state();

    let bg = r.terminal_state.colors.background;
    let fg = r.terminal_state.colors.foreground;
    assert_eq!(
        r.custom_shader_uniforms.background_color,
        [
            bg.r as f32 / 255.0,
            bg.g as f32 / 255.0,
            bg.b as f32 / 255.0,
            1.0
        ]
    );
    assert_eq!(
        r.custom_shader_uniforms.foreground_color,
        [
            fg.r as f32 / 255.0,
            fg.g as f32 / 255.0,
            fg.b as f32 / 255.0,
            1.0
        ]
    );
    let p0 = r.terminal_state.colors.palette[0];
    assert_eq!(
        r.custom_shader_uniforms.palette[0],
        [
            p0.r as f32 / 255.0,
            p0.g as f32 / 255.0,
            p0.b as f32 / 255.0,
            1.0
        ]
    );
    assert_eq!(
        r.custom_shader_uniforms.cursor_visible,
        r.terminal_state.cursor.visible as i32
    );
    let expected_style = cursor::Style::from_terminal(r.terminal_state.cursor.visual_style) as i32;
    assert_eq!(
        r.custom_shader_uniforms.current_cursor_style,
        expected_style
    );

    t.deinit(Global.into());
    deinit_test_renderer(&mut r);
}

// §F.2(6) — direct: per-frame timing/resolution/focus. Per §RISK-7 the
// `Contents` is empty (no prior `rebuild_cells`) so `get_cursor_glyph()`
// is `None` and the cursor block is skipped; assert only the timing/
// resolution/focus fields.
#[test]
fn update_custom_shader_uniforms_for_frame_timing() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };
    r.has_custom_shaders = true;
    r.update_custom_shader_uniforms_for_frame();

    assert_eq!(r.custom_shader_uniforms.frame, 1);
    assert_eq!(
        r.custom_shader_uniforms.resolution,
        [r.size.screen.width as f32, r.size.screen.height as f32, 1.0]
    );
    assert_eq!(
        r.custom_shader_uniforms.channel_resolution[0],
        [
            r.size.screen.width as f32,
            r.size.screen.height as f32,
            1.0,
            0.0
        ]
    );
    assert_eq!(r.custom_shader_uniforms.focus, 1);
    assert!(r.first_frame_time.is_some());
    assert!(r.last_frame_time.is_some());
    deinit_test_renderer(&mut r);
}

// UD76 (R13, task R13-C2): a saturated padding (UD71, stored by
// `contentScaleCallback` before the IO thread resizes the grid) with the
// cursor off the first column and row. The reference's unchecked
// `grid_pos * cell + padding` overflows `u32`; the port saturates, so the
// cursor uniform is the saturated edge.
#[test]
fn custom_shader_cursor_uniform_saturates_with_a_saturated_padding() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };
    r.has_custom_shaders = true;
    r.size.padding.left = u32::MAX;
    r.size.padding.top = u32::MAX;
    let cursor = crate::shaders::CellText {
        atlas: crate::shaders::Atlas::Grayscale,
        bools: crate::shaders::CellTextBools {
            is_cursor_glyph: true,
            ..Default::default()
        },
        grid_pos: [3, 2],
        color: [0, 0, 0, 255],
        ..Default::default()
    };
    r.cells
        .set_cursor(Some(cursor), Some(crate::cursor::Style::Block));
    assert_eq!(Some(cursor), r.cells.get_cursor_glyph(), "precondition");

    r.update_custom_shader_uniforms_for_frame();

    let c = r.custom_shader_uniforms.current_cursor.0;
    assert_eq!(u32::MAX as f32, c[0], "x saturates: {c:?}");
    assert!(c[1] >= u32::MAX as f32, "y saturates: {c:?}");
    deinit_test_renderer(&mut r);
}

// RESIZE (§5): `set_screen_size` stores the new padding + recomputes the
// screen-size uniforms. Offscreen-testable directly.
#[test]
#[allow(clippy::neg_multiply)]
fn set_screen_size_updates_padding_and_uniforms() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };
    let mut size = r.size;
    size.padding = Padding {
        top: 4,
        bottom: 4,
        left: 4,
        right: 4,
    };
    r.set_screen_size(size);
    assert_eq!(
        r.size.padding,
        Padding {
            top: 4,
            bottom: 4,
            left: 4,
            right: 4,
        }
    );
    // The projection is recomputed with the new padding
    // (`-1 * padding.left/top`, mirroring `update_screen_size_uniforms`).
    let ts = r.size.terminal();
    let expected = math::ortho2d(
        -1.0 * r.size.padding.left as f32,
        (ts.width + r.size.padding.right) as f32,
        (ts.height + r.size.padding.bottom) as f32,
        -1.0 * r.size.padding.top as f32,
    );
    assert_eq!(r.uniforms.projection_matrix, expected);
    deinit_test_renderer(&mut r);
}

// RESIZE (§5): `set_font_grid` swaps the grid handle, resets each frame's
// atlas-sync counters, recomputes the cell size + font-grid uniforms, and
// marks the screen dirty.
#[test]
fn set_font_grid_swaps_grid_and_resets_modified() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };
    // Dirty a frame's modified counters, then confirm the reset.
    r.swap_chain.as_mut().unwrap().frames[0].grayscale_modified = 7;
    r.swap_chain.as_mut().unwrap().frames[0].color_modified = 5;
    let grid = r.font_grid; // reuse the same live grid handle
    r.set_font_grid(grid);
    assert_eq!(
        r.swap_chain.as_ref().unwrap().frames[0].grayscale_modified,
        0
    );
    assert_eq!(r.swap_chain.as_ref().unwrap().frames[0].color_modified, 0);
    let m = unsafe { (*grid).metrics };
    assert_eq!(
        r.size.cell,
        CellSize {
            width: m.cell_width,
            height: m.cell_height,
        }
    );
    assert_eq!(
        r.uniforms.cell_size,
        [m.cell_width as f32, m.cell_height as f32]
    );
    assert_eq!(r.terminal_state.dirty, Dirty::Full); // mark_dirty
    deinit_test_renderer(&mut r);
}

// CONFIG-SWAP (§5): `change_config` reinits the shaper, updates the
// blending/contrast uniforms, bumps `target_config_modified`, and marks
// dirty. `bg_image`/`custom_shaders` stay placeholders so the deferred
// `prep_background_image` seam is unreachable (`bg_image_changed == false`).
#[test]
fn change_config_updates_blending_uniforms() {
    let mut r = match test_renderer(10, 3) {
        Some(r) => r,
        None => return,
    };
    let mut d = derived_config_for_test();
    d.blending = AlphaBlending::LinearCorrected;
    d.min_contrast = 2.0;
    r.change_config(d).expect("change_config");
    assert!(r.uniforms.bools.use_linear_blending);
    assert!(r.uniforms.bools.use_linear_correction);
    assert_eq!(r.uniforms.min_contrast, 2.0);
    assert!(r.reinitialize_shaders); // blending changed
    assert_eq!(r.target_config_modified, 1); // +%= 1
    assert_eq!(r.terminal_state.dirty, Dirty::Full);
    deinit_test_renderer(&mut r);
}
