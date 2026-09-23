//! Tests for the HarfBuzz shaper — a faithful port of the `test { ... }` blocks
//! in ghostty `src/font/shaper/harfbuzz.zig` (25 tests: 21 executing + 4 gated
//! `error.SkipZigTest`). Kept in a sibling file (mirroring `page_tests.rs`),
//! attached as the `tests` submodule of `harfbuzz.rs`.
//!
//! PORT NOTE (backend): these tests run wherever this shaper is the ACTIVE one —
//! i.e. on the targets whose `Backend::default()` has `hasHarfbuzz() == true`:
//! `fontconfig_freetype` (Linux/BSD) and `freetype_windows` (Windows), with
//! FreeType faces carrying an `hb_font` (`face/freetype.rs`). They do NOT run on
//! macOS: the Darwin default backend is `.coretext`, whose `hasHarfbuzz()` is
//! false, so `shape.rs` gates this whole module off macOS — the same set the
//! reference type-checks it on (see the `pub mod harfbuzz` PORT NOTE in
//! `shape.rs` for the Zig-lazy-vs-Rust-eager reasoning). They previously ran on
//! macOS against a CoreText face given an `hb_font` it does not have in the
//! ported configuration; that was the departure, not the coverage. The
//! assertions are structural (cell counts and grid X positions driven by
//! clusters, plus relative offset orderings) so they hold across every
//! HarfBuzz-shaping backend, exactly as the reference's do.

use super::*;

use allocator_api2::alloc::Global;

use crate::codepoint_resolver::{CodepointResolver, DescriptorCache, StyleStatus};
use crate::collection::{AddOptions, Collection, LoadOptions, SizeAdjustment};
use crate::face::{DesiredSize, Options as FaceOptions, DEFAULT_DPI, FREETYPE_LOAD_FLAGS_DEFAULT};
use crate::library::Library;
use crate::shared_grid::SharedGrid;
use crate::Face;
use crate::Style;

use rustty_vt::kitty;
use rustty_vt::modes as modespkg;
// PORT NOTE (WS20): `Mode` and `contains_subslice` below are used ONLY by the
// discovered-font tests, so they carry the same gate — otherwise a build that
// elides those tests warns `unused`, which `-D warnings` rejects. The gate is
// `not(wasm32)`, mirroring the reference's ONLY skip condition for these tests
// (`if (font.Discover == void) return error.SkipZigTest`, comptime-true only on
// the `.web_canvas => void` wasm arm); every native target has a live discoverer
// (CoreText on macOS, Fontconfig on Linux/BSD, the directory scanner on Windows).
#[cfg(not(target_arch = "wasm32"))]
use rustty_vt::modes::Mode;
use rustty_vt::page::{Cell, CellRgb, ContentTag};
use rustty_vt::point::{Coordinate, Point};
use rustty_vt::render::RenderState;
use rustty_vt::terminal::{Colors, Options, Terminal};

// PORT NOTE: the reference `terminal.Terminal.init(alloc, .{ .cols, .rows })`
// relies on `Options` field defaults; Rust `Options` has none, so this helper
// supplies them verbatim (mirrors `render.rs`'s test helper).
fn term_opts(cols: rustty_vt::size::CellCountInt, rows: rustty_vt::size::CellCountInt) -> Options {
    Options {
        cols,
        rows,
        max_scrollback_bytes: Some(10_000),
        max_scrollback_lines: None,
        colors: Colors::default(),
        default_modes: modespkg::ModePacked::default(),
        default_cursor_style: rustty_vt::screen::CursorStyle::Block,
        default_cursor_blink: Some(false),
        kitty_image_storage_limit: 320 * 1000 * 1000,
        kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
    }
}

// PORT NOTE (cells mapping): every `runIterator` call below passes the row's
// cells. The reference expresses this as
//     .cells = state.row_data.get(0).cells.slice(),
// (a `MultiArrayList` row's `.cells.slice()`); rustty's `RenderState` row is a
// `render::Row` whose `cells: CellList` (SoA), and `CellList::slice()` yields
// the same `MultiArrayList(Cell).Slice` analogue, so this stays
// `cells: state.row_data[N].cells.slice()` throughout.

fn face_opts() -> FaceOptions {
    FaceOptions {
        size: DesiredSize {
            points: 12.0,
            xdpi: DEFAULT_DPI,
            ydpi: DEFAULT_DPI,
        },
        freetype_load_flags: FREETYPE_LOAD_FLAGS_DEFAULT,
    }
}

fn regular_add() -> AddOptions {
    AddOptions {
        style: Style::Regular,
        fallback: false,
        size_adjustment: SizeAdjustment::None,
    }
}

// const TestShaper = struct { alloc, shaper: Shaper, grid: *SharedGrid, lib: Library };
struct TestShaper {
    shaper: Shaper,
    // grid: *SharedGrid — a heap `SharedGrid` (the reference `alloc.create`);
    // `Box` gives the pointer-stable address `TextRun.grid` needs.
    grid: Box<SharedGrid<Global>>,
    lib: Library,
}

impl TestShaper {
    // pub fn deinit(self: *TestShaper) void {
    //     self.shaper.deinit();
    //     self.grid.deinit(self.alloc);
    //     self.alloc.destroy(self.grid);
    //     self.lib.deinit();
    // }
    fn deinit(&mut self) {
        self.shaper.deinit();
        self.grid.deinit();
        // `alloc.destroy(self.grid)` — the `Box` frees on drop.
        self.lib.deinit();
    }

    /// Raw pointer to the heap grid (for `RunOptions.grid`).
    fn grid_ptr(&mut self) -> *mut SharedGrid<Global> {
        &mut *self.grid as *mut SharedGrid<Global>
    }
}

// const TestFont = enum { inconsolata, monaspace_neon, arabic };
#[derive(Clone, Copy)]
enum TestFont {
    Inconsolata,
    MonaspaceNeon,
    Arabic,
}

/// Helper to return a fully initialized shaper.
// fn testShaper(alloc: Allocator) !TestShaper {
//     return try testShaperWithFont(alloc, .inconsolata);
// }
fn test_shaper() -> TestShaper {
    test_shaper_with_font(TestFont::Inconsolata)
}

// fn testShaperWithFont(alloc: Allocator, font_req: TestFont) !TestShaper {
fn test_shaper_with_font(font_req: TestFont) -> TestShaper {
    // const testEmoji = font.embedded.emoji;
    // const testEmojiText = font.embedded.emoji_text;
    let test_emoji_text = rustty_font::embedded::EMOJI_TEXT;
    // const testFont = switch (font_req) { ... };
    let test_font = match font_req {
        TestFont::Inconsolata => rustty_font::embedded::INCONSOLATA,
        TestFont::MonaspaceNeon => rustty_font::embedded::MONASPACE_NEON,
        TestFont::Arabic => rustty_font::embedded::ARABIC,
    };

    // var lib = try Library.init(alloc); errdefer lib.deinit();
    let lib = Library::init().unwrap();

    // var c = Collection.init();
    let mut c = Collection::init();
    // c.load_options = .{ .library = lib };
    c.load_options = Some(LoadOptions {
        library: lib,
        ..Default::default()
    });

    // Setup group
    // _ = try c.add(alloc, try .init(lib, testFont, .{ .size = .{ .points = 12 } }), .{ ... });
    c.add(
        Face::init(lib, test_font, face_opts()).unwrap(),
        regular_add(),
    )
    .unwrap();

    // if (comptime !font.options.backend.hasCoretext()) {
    //     // Coretext doesn't support Noto's format
    //     _ = try c.add(alloc, try .init(lib, testEmoji, ...), .{ ... });
    // } else {
    //     // On CoreText we want to load Apple Emoji, we should have it.
    //     var disco = font.Discover.init(lib);
    //     ...
    //     _ = try c.addDeferred(alloc, face, .{ ... });
    // }
    #[cfg(not(target_os = "macos"))]
    {
        c.add(
            Face::init(lib, rustty_font::embedded::EMOJI, face_opts()).unwrap(),
            regular_add(),
        )
        .unwrap();
    }
    #[cfg(target_os = "macos")]
    {
        use crate::discovery::{Descriptor, Discover};
        let mut disco = Discover::init(lib);
        let mut disco_it = disco
            .discover(Descriptor {
                family: Some(b"Apple Color Emoji".to_vec()),
                size: 12.0,
                monospace: false,
                ..Descriptor::default()
            })
            .unwrap();
        let face = disco_it.next().unwrap().unwrap();
        c.add_deferred(face, regular_add()).unwrap();
        disco_it.deinit();
        disco.deinit();
    }

    // _ = try c.add(alloc, try .init(lib, testEmojiText, ...), .{ ... });
    c.add(
        Face::init(lib, test_emoji_text, face_opts()).unwrap(),
        regular_add(),
    )
    .unwrap();

    // const grid_ptr = try alloc.create(SharedGrid);
    // grid_ptr.* = try .init(alloc, .{ .collection = c });
    let grid = Box::new(SharedGrid::init(Global, resolver_for(c)).unwrap());

    // var shaper = try Shaper.init(alloc, .{ .features = &.{"dlig"} });
    // Some of our tests rely on dlig being enabled by default
    let shaper = Shaper::init(shape::Options {
        features: vec![b"dlig".to_vec()],
    })
    .unwrap();

    TestShaper { shaper, grid, lib }
}

/// R14 strict mode for the system-font skips (the `RUSTTY_REQUIRE_METAL` /
/// `RUSTTY_REQUIRE_GL` pattern): the reference skips a shaping test when its
/// font is not installed, which a Rust test can only express as returning `ok`.
/// `RUSTTY_REQUIRE_SYSTEM_FONTS=1` turns that skip into a FAILURE. The macOS CI
/// leg exports it: every font these tests ask for except Noto Serif Tibetan (an
/// `#[ignore]`d test) is a stock macOS Supplemental font. The Linux leg does not
/// (a stock runner has none of them).
fn system_fonts_required() -> bool {
    system_fonts_required_from(std::env::var_os("RUSTTY_REQUIRE_SYSTEM_FONTS").as_deref())
}

fn system_fonts_required_from(value: Option<&std::ffi::OsStr>) -> bool {
    value == Some(std::ffi::OsStr::new("1"))
}

/// A no-op when fonts are not required (the caller prints its skip line and
/// returns), a panic when they are.
#[track_caller]
fn require_system_font_or_skip(font_req: &str, why: &str) {
    require_system_font_or_skip_impl(system_fonts_required(), font_req, why);
}

#[track_caller]
fn require_system_font_or_skip_impl(required: bool, font_req: &str, why: &str) {
    assert!(
        !required,
        "system font required but unavailable (RUSTTY_REQUIRE_SYSTEM_FONTS=1): \
         {font_req:?} {why}"
    );
    eprintln!("system font {font_req:?} {why}; skipping");
}

#[test]
fn system_font_pivot_is_strict_only_when_env_is_exactly_one() {
    use std::ffi::OsStr;
    assert!(!system_fonts_required_from(None));
    assert!(!system_fonts_required_from(Some(OsStr::new("0"))));
    assert!(system_fonts_required_from(Some(OsStr::new("1"))));
    require_system_font_or_skip_impl(false, "Some Font", "was not found");
}

#[test]
#[should_panic(expected = "system font required but unavailable (RUSTTY_REQUIRE_SYSTEM_FONTS=1)")]
fn system_font_pivot_panics_when_required() {
    require_system_font_or_skip_impl(true, "Some Font", "was not found");
}

// fn testShaperWithDiscoveredFont(alloc: Allocator, font_req: [:0]const u8) !TestShaper {
//
// PORT NOTE: `error.SkipZigTest` (font not found / not matching / discovery
// unavailable) is modeled as `Option::None` — the caller returns early — after
// the R14 pivot [`require_system_font_or_skip`], so the skip is printed, and is
// a failure under `RUSTTY_REQUIRE_SYSTEM_FONTS=1`.
#[cfg(not(target_arch = "wasm32"))]
fn test_shaper_with_discovered_font(font_req: &str) -> Option<TestShaper> {
    use crate::discovery::{Descriptor, Discover};

    // if (font.Discover == void) return error.SkipZigTest; — Discover exists here.
    // var lib = try Library.init(alloc); errdefer lib.deinit();
    let lib = Library::init().unwrap();

    // var c = Collection.init();
    let mut c = Collection::init();
    // c.load_options = .{ .library = lib };
    c.load_options = Some(LoadOptions {
        library: lib,
        ..Default::default()
    });

    // Discover and add our font to the collection.
    {
        // var disco = font.Discover.init(lib); defer disco.deinit();
        let mut disco = Discover::init(lib);
        // var disco_it = try disco.discover(alloc, .{ .family = font_req, .size = 12, .monospace = false });
        let mut disco_it = disco
            .discover(Descriptor {
                family: Some(font_req.as_bytes().to_vec()),
                size: 12.0,
                monospace: false,
                ..Descriptor::default()
            })
            .unwrap();
        // var face: font.DeferredFace = (try disco_it.next()) orelse return error.FontNotFound;
        let mut face = match disco_it.next().unwrap() {
            Some(f) => f,
            None => {
                disco_it.deinit();
                disco.deinit();
                require_system_font_or_skip(font_req, "was not found");
                return None;
            }
        };

        // Check which font was discovered - skip if it doesn't match the request
        // var name_buf: [256]u8 = undefined;
        let mut name_buf = [0u8; 256];
        // const face_name = face.name(&name_buf) catch "(unknown)";
        let face_name = face.name(&mut name_buf).unwrap_or(b"(unknown)");
        // if (std.mem.indexOf(u8, face_name, font_req) == null) return error.SkipZigTest;
        if !contains_subslice(face_name, font_req.as_bytes()) {
            face.deinit();
            disco_it.deinit();
            disco.deinit();
            require_system_font_or_skip(font_req, "was not matched (discovery fell back)");
            return None;
        }

        // _ = try c.add(alloc, try face.load(lib, .{ .size = .{ .points = 12 } }), .{ ... });
        c.add(face.load(lib, face_opts()).unwrap(), regular_add())
            .unwrap();

        disco_it.deinit();
        disco.deinit();
    }

    // const grid_ptr = try alloc.create(SharedGrid);
    // grid_ptr.* = try .init(alloc, .{ .collection = c });
    let grid = Box::new(SharedGrid::init(Global, resolver_for(c)).unwrap());

    // var shaper = try Shaper.init(alloc, .{});
    let shaper = Shaper::init(shape::Options::default()).unwrap();

    Some(TestShaper { shaper, grid, lib })
}

/// Build the `CodepointResolver` for `SharedGrid.init(alloc, .{ .collection = c })`
/// (all other resolver fields default).
fn resolver_for(c: Collection) -> CodepointResolver {
    CodepointResolver {
        collection: c,
        styles: StyleStatus::default(),
        discover: None,
        codepoint_map: None,
        descriptor_cache: DescriptorCache::default(),
        sprite: None,
    }
}

/// `std.mem.indexOf(u8, haystack, needle) != null`.
#[cfg(not(target_arch = "wasm32"))]
fn contains_subslice(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack.windows(needle.len()).any(|w| w == needle)
}

// test "run iterator" {
#[test]
fn run_iterator() {
    let mut testdata = test_shaper();

    {
        // Make a screen with some data
        let mut t = Terminal::init(Global, term_opts(5, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"ABCD");
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(Global, &mut t).unwrap();

        // Get our run iterator
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while it.next(Global).unwrap().is_some() {
            count += 1;
        }
        assert_eq!(1usize, count);

        state.deinit(Global);
        t.deinit(Global);
    }

    // Spaces should be part of a run
    {
        let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"ABCD   EFG");
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(Global, &mut t).unwrap();

        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while it.next(Global).unwrap().is_some() {
            count += 1;
        }
        assert_eq!(1usize, count);

        state.deinit(Global);
        t.deinit(Global);
    }

    {
        // Make a screen with some data
        let mut t = Terminal::init(Global, term_opts(5, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice("A😃D".as_bytes());
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(Global, &mut t).unwrap();

        // Get our run iterator
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while it.next(Global).unwrap().is_some() {
            count += 1;

            // All runs should be exactly length 1
            assert_eq!(1u32, unsafe { (*shaper).hb_buf.get_length() });
        }
        assert_eq!(3usize, count);

        state.deinit(Global);
        t.deinit(Global);
    }

    testdata.deinit();
}

// test "run iterator: empty cells with background set" {
#[test]
fn run_iterator_empty_cells_with_background_set() {
    let mut testdata = test_shaper();

    {
        // Make a screen with some data
        let mut t = Terminal::init(Global, term_opts(5, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            // Set red background and write A
            s.next_slice(b"\x1b[48;2;255;0;0mA");
            s.deinit();
        }

        // Get our first row
        {
            // const list_cell = t.screens.active.pages.getCell(.{ .active = .{ .x = 1 } }).?;
            let list_cell = unsafe {
                (*t.screens.active)
                    .pages
                    .get_cell(Point::Active(Coordinate { x: 1, y: 0 }))
                    .unwrap()
            };
            // const cell = list_cell.cell;
            // cell.* = .{ .content_tag = .bg_color_rgb, .content = .{ .color_rgb = .{ .r = 0xFF, .g = 0, .b = 0 } } };
            unsafe {
                (*list_cell.cell).set_content_tag(ContentTag::BgColorRgb);
                (*list_cell.cell).set_content_color_rgb(CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0,
                });
            }
        }
        {
            let list_cell = unsafe {
                (*t.screens.active)
                    .pages
                    .get_cell(Point::Active(Coordinate { x: 2, y: 0 }))
                    .unwrap()
            };
            unsafe {
                (*list_cell.cell).set_content_tag(ContentTag::BgColorRgb);
                (*list_cell.cell).set_content_color_rgb(CellRgb {
                    r: 0xFF,
                    g: 0,
                    b: 0,
                });
            }
        }

        let mut state = RenderState::empty();
        state.update(Global, &mut t).unwrap();

        // Get our run iterator
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        {
            // const run = (try it.next(alloc)).?;
            let run = it.next(Global).unwrap().unwrap();
            assert_eq!(3u32, unsafe { (*shaper).hb_buf.get_length() });
            let cells = unsafe { (*shaper).shape(run) }.unwrap();
            assert_eq!(3usize, cells.len());
        }
        // try testing.expect(try it.next(alloc) == null);
        assert!(it.next(Global).unwrap().is_none());

        state.deinit(Global);
        t.deinit(Global);
    }

    testdata.deinit();
}

// test "shape" {
#[test]
fn shape() {
    let mut testdata = test_shaper();

    // buf_idx += utf8Encode(0x1F44D) x2 (Thumbs up plain) + 0x1F3FD (Medium skin tone)
    let mut buf: Vec<u8> = Vec::new();
    push_utf8(&mut buf, 0x1F44D); // Thumbs up plain
    push_utf8(&mut buf, 0x1F44D); // Thumbs up plain
    push_utf8(&mut buf, 0x1F3FD); // Medium skin tone

    // Make a screen with some data
    let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(&buf);
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // Get our run iterator
    let shaper: *mut Shaper = &mut testdata.shaper;
    let grid = testdata.grid_ptr();
    let mut it = unsafe {
        (*shaper).run_iterator(shape::RunOptions {
            grid,
            cells: state.row_data[0].cells.slice(),
            selection: None,
            cursor_x: None,
        })
    };
    let mut count: usize = 0;
    while let Some(run) = it.next(Global).unwrap() {
        count += 1;
        assert_eq!(3u32, unsafe { (*shaper).hb_buf.get_length() });
        let _ = unsafe { (*shaper).shape(run) }.unwrap();
    }
    assert_eq!(1usize, count);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape inconsolata ligs" {
#[test]
fn shape_inconsolata_ligs() {
    let mut testdata = test_shaper();

    {
        let mut t = Terminal::init(Global, term_opts(5, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b">=");
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(Global, &mut t).unwrap();

        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;

            assert_eq!(2usize, run.cells as usize);

            let cells = unsafe { (*shaper).shape(run) }.unwrap();
            assert_eq!(1usize, cells.len());
        }
        assert_eq!(1usize, count);

        state.deinit(Global);
        t.deinit(Global);
    }

    {
        let mut t = Terminal::init(Global, term_opts(5, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"===");
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(Global, &mut t).unwrap();

        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;

            assert_eq!(3usize, run.cells as usize);

            let cells = unsafe { (*shaper).shape(run) }.unwrap();
            assert_eq!(1usize, cells.len());
        }
        assert_eq!(1usize, count);

        state.deinit(Global);
        t.deinit(Global);
    }

    testdata.deinit();
}

// test "shape monaspace ligs" {
#[test]
fn shape_monaspace_ligs() {
    let mut testdata = test_shaper_with_font(TestFont::MonaspaceNeon);

    {
        let mut t = Terminal::init(Global, term_opts(5, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b"===");
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(Global, &mut t).unwrap();

        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;

            assert_eq!(3usize, run.cells as usize);

            let cells = unsafe { (*shaper).shape(run) }.unwrap();
            assert_eq!(1usize, cells.len());
        }
        assert_eq!(1usize, count);

        state.deinit(Global);
        t.deinit(Global);
    }

    testdata.deinit();
}

// Ghostty doesn't currently support RTL and our renderers assume
// that cells are in strict LTR order. This means that we need to
// force RTL text to be LTR for rendering. This test ensures that
// we are correctly forcing RTL text to be LTR.
// test "shape arabic forced LTR" {
#[test]
fn shape_arabic_forced_ltr() {
    let mut testdata = test_shaper_with_font(TestFont::Arabic);

    let mut t = Terminal::init(Global, term_opts(120, 30)).unwrap();

    {
        let mut s = t.vt_stream();
        // s.nextSlice(@embedFile("testdata/arabic.txt"));
        s.next_slice(include_bytes!("../testdata/arabic.txt"));
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    let shaper: *mut Shaper = &mut testdata.shaper;
    let grid = testdata.grid_ptr();
    let mut it = unsafe {
        (*shaper).run_iterator(shape::RunOptions {
            grid,
            cells: state.row_data[0].cells.slice(),
            selection: None,
            cursor_x: None,
        })
    };
    let mut count: usize = 0;
    while let Some(run) = it.next(Global).unwrap() {
        count += 1;
        assert_eq!(25usize, run.cells as usize);

        let cells = unsafe { (*shaper).shape(run) }.unwrap();
        assert_eq!(25usize, cells.len());

        // var x: u16 = cells[0].x;
        let mut x: u16 = cells[0].x;
        // for (cells[1..]) |cell| { try testing.expectEqual(x + 1, cell.x); x = cell.x; }
        for cell in &cells[1..] {
            assert_eq!(x + 1, cell.x);
            x = cell.x;
        }
    }
    assert_eq!(1usize, count);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape emoji width" {
#[test]
fn shape_emoji_width() {
    let mut testdata = test_shaper();

    {
        let mut t = Terminal::init(Global, term_opts(5, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice("👍".as_bytes());
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(Global, &mut t).unwrap();

        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;

            assert_eq!(2usize, run.cells as usize);

            let cells = unsafe { (*shaper).shape(run) }.unwrap();
            assert_eq!(1usize, cells.len());
        }
        assert_eq!(1usize, count);

        state.deinit(Global);
        t.deinit(Global);
    }

    testdata.deinit();
}

// test "shape emoji width long" {
#[test]
fn shape_emoji_width_long() {
    let mut testdata = test_shaper();

    // Make a screen and add a long emoji sequence to it.
    let mut t = Terminal::init(Global, term_opts(30, 3)).unwrap();

    // var page = t.screens.active.pages.pages.first.?.data;
    // var row = page.getRow(1);
    // const cell = &row.cells.ptr(page.memory)[0];
    // cell.* = .{ .content_tag = .codepoint, .content = .{ .codepoint = 0x1F9D4 } }; // Person with beard
    // PORT NOTE: reached via the pagelist `get_cell(active x=0,y=1)`, which yields
    // the same row/cell pointers plus the owning page node (for `set_graphemes`).
    let lc = unsafe {
        (*t.screens.active)
            .pages
            .get_cell(Point::Active(Coordinate { x: 0, y: 1 }))
            .unwrap()
    };
    unsafe {
        *lc.cell = Cell::init(0x1F9D4); // Person with beard
    }
    // var graphemes = [_]u21{ 0x1F3FB, 0x200D, 0x2642, 0xFE0F };
    let graphemes: [u32; 4] = [
        0x1F3FB, // Light skin tone (Fitz 1-2)
        0x200D,  // ZWJ
        0x2642,  // Male sign
        0xFE0F,  // Emoji presentation selector
    ];
    // try page.setGraphemes(row, cell, graphemes[0..]);
    unsafe {
        (*rustty_vt::pagelist::Node::page(lc.node))
            .set_graphemes(lc.row, lc.cell, &graphemes)
            .unwrap();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // Get our run iterator
    let shaper: *mut Shaper = &mut testdata.shaper;
    let grid = testdata.grid_ptr();
    let mut it = unsafe {
        (*shaper).run_iterator(shape::RunOptions {
            grid,
            cells: state.row_data[1].cells.slice(),
            selection: None,
            cursor_x: None,
        })
    };
    let mut count: usize = 0;
    while let Some(run) = it.next(Global).unwrap() {
        count += 1;
        assert_eq!(4u32, unsafe { (*shaper).hb_buf.get_length() });

        let cells = unsafe { (*shaper).shape(run) }.unwrap();

        assert_eq!(1usize, cells.len());
    }
    assert_eq!(1usize, count);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shaper selects font for entire grapheme" {
#[test]
fn shaper_selects_font_for_entire_grapheme() {
    let mut testdata = test_shaper();

    let mut t = Terminal::init(Global, term_opts(5, 3)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice("#\u{20E3}".as_bytes()); // Combining enclosing keycap
        s.deinit();
    }

    // const primary = (try testdata.grid.getIndex(alloc, '#', .regular, null)).?;
    let primary = testdata
        .grid
        .get_index(Global, '#' as u32, Style::Regular, None)
        .unwrap()
        .unwrap();
    assert!(testdata.grid.has_codepoint(primary, '#' as u32, None));
    assert!(!testdata.grid.has_codepoint(primary, 0x20E3, None));

    // const additional = (try testdata.grid.getIndex(alloc, 0x20E3, .regular, null)).?;
    let additional = testdata
        .grid
        .get_index(Global, 0x20E3, Style::Regular, None)
        .unwrap()
        .unwrap();
    assert!(testdata.grid.has_codepoint(additional, '#' as u32, None));
    assert!(testdata.grid.has_codepoint(additional, 0x20E3, None));

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    let shaper: *mut Shaper = &mut testdata.shaper;
    let grid = testdata.grid_ptr();
    let mut it = unsafe {
        (*shaper).run_iterator(shape::RunOptions {
            grid,
            cells: state.row_data[0].cells.slice(),
            selection: None,
            cursor_x: None,
        })
    };
    // const run = (try it.next(alloc)).?;
    let run = it.next(Global).unwrap().unwrap();
    // try testing.expectEqual(additional, run.font_index);
    assert_eq!(additional, run.font_index);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape variation selector VS15" {
#[test]
fn shape_variation_selector_vs15() {
    let mut testdata = test_shaper();

    let mut buf: Vec<u8> = Vec::new();
    push_utf8(&mut buf, 0x270C); // Victory sign (default text)
    push_utf8(&mut buf, 0xFE0E); // ZWJ to force text

    // Make a screen with some data
    let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(&buf);
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // Get our run iterator
    let shaper: *mut Shaper = &mut testdata.shaper;
    let grid = testdata.grid_ptr();
    let mut it = unsafe {
        (*shaper).run_iterator(shape::RunOptions {
            grid,
            cells: state.row_data[0].cells.slice(),
            selection: None,
            cursor_x: None,
        })
    };
    let mut count: usize = 0;
    while let Some(run) = it.next(Global).unwrap() {
        count += 1;
        assert_eq!(1u32, unsafe { (*shaper).hb_buf.get_length() });

        let cells = unsafe { (*shaper).shape(run) }.unwrap();
        assert_eq!(1usize, cells.len());
    }
    assert_eq!(1usize, count);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape variation selector VS16" {
#[test]
fn shape_variation_selector_vs16() {
    let mut testdata = test_shaper();

    let mut buf: Vec<u8> = Vec::new();
    push_utf8(&mut buf, 0x270C); // Victory sign (default text)
    push_utf8(&mut buf, 0xFE0F); // ZWJ to force color

    // Make a screen with some data
    let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(&buf);
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // Get our run iterator
    let shaper: *mut Shaper = &mut testdata.shaper;
    let grid = testdata.grid_ptr();
    let mut it = unsafe {
        (*shaper).run_iterator(shape::RunOptions {
            grid,
            cells: state.row_data[0].cells.slice(),
            selection: None,
            cursor_x: None,
        })
    };
    let mut count: usize = 0;
    while let Some(run) = it.next(Global).unwrap() {
        count += 1;
        assert_eq!(1u32, unsafe { (*shaper).hb_buf.get_length() });

        let cells = unsafe { (*shaper).shape(run) }.unwrap();
        assert_eq!(1usize, cells.len());
    }
    assert_eq!(1usize, count);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape with empty cells in between" {
#[test]
fn shape_with_empty_cells_in_between() {
    let mut testdata = test_shaper();

    // Make a screen with some data
    let mut t = Terminal::init(Global, term_opts(30, 3)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(b"A");
        s.next_slice(b"\x1b[5C");
        s.next_slice(b"B");
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // Get our run iterator
    let shaper: *mut Shaper = &mut testdata.shaper;
    let grid = testdata.grid_ptr();
    let mut it = unsafe {
        (*shaper).run_iterator(shape::RunOptions {
            grid,
            cells: state.row_data[0].cells.slice(),
            selection: None,
            cursor_x: None,
        })
    };
    let mut count: usize = 0;
    while let Some(run) = it.next(Global).unwrap() {
        count += 1;

        let cells = unsafe { (*shaper).shape(run) }.unwrap();
        assert_eq!(1usize, count);
        assert_eq!(7usize, cells.len());
    }

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape Combining characters" {
#[test]
fn shape_combining_characters() {
    let mut testdata = test_shaper();

    let mut buf: Vec<u8> = Vec::new();
    push_utf8(&mut buf, 'n' as u32); // Combining
    push_utf8(&mut buf, 0x0308); // Combining
    push_utf8(&mut buf, 0x0308);
    push_utf8(&mut buf, 'a' as u32);

    // Make a screen with some data
    let mut t = Terminal::init(Global, term_opts(30, 3)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(&buf);
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // Get our run iterator
    let shaper: *mut Shaper = &mut testdata.shaper;
    let grid = testdata.grid_ptr();
    let mut it = unsafe {
        (*shaper).run_iterator(shape::RunOptions {
            grid,
            cells: state.row_data[0].cells.slice(),
            selection: None,
            cursor_x: None,
        })
    };
    let mut count: usize = 0;
    while let Some(run) = it.next(Global).unwrap() {
        count += 1;

        let cells = unsafe { (*shaper).shape(run) }.unwrap();
        assert_eq!(4usize, cells.len());
        assert_eq!(0u16, cells[0].x);
        assert_eq!(0u16, cells[1].x);
        assert_eq!(0u16, cells[2].x);
        assert_eq!(1u16, cells[3].x);
    }
    assert_eq!(1usize, count);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// This test exists because the string it uses causes HarfBuzz to output a
// non-monotonic run with our cluster level set to `characters`, which we need
// to handle by tracking the max cluster for the run.
// test "shape Devanagari string" {
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn shape_devanagari_string() {
    // We need a font that supports devanagari for this to work, if we can't
    // find Arial Unicode MS, which is a system font on macOS, we just skip
    // the test.
    let mut testdata = match test_shaper_with_discovered_font("Arial Unicode MS") {
        Some(t) => t,
        None => return,
    };

    // Make a screen with some data
    let mut t = Terminal::init(Global, term_opts(30, 3)).unwrap();

    // Disable grapheme clustering
    t.modes.set(Mode::GraphemeCluster, false);

    {
        let mut s = t.vt_stream();
        s.next_slice("अपार्टमेंट".as_bytes());
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // Get our run iterator
    let shaper: *mut Shaper = &mut testdata.shaper;
    let grid = testdata.grid_ptr();
    let mut it = unsafe {
        (*shaper).run_iterator(shape::RunOptions {
            grid,
            cells: state.row_data[0].cells.slice(),
            selection: None,
            cursor_x: None,
        })
    };

    // const run = try it.next(alloc);
    let run = it.next(Global).unwrap();
    assert!(run.is_some());
    let cells = unsafe { (*shaper).shape(run.unwrap()) }.unwrap();

    assert_eq!(8usize, cells.len());
    assert_eq!(0u16, cells[0].x);
    assert_eq!(1u16, cells[1].x);
    assert_eq!(2u16, cells[2].x);
    assert_eq!(4u16, cells[3].x);
    assert_eq!(4u16, cells[4].x);
    assert_eq!(5u16, cells[5].x);
    assert_eq!(5u16, cells[6].x);
    assert_eq!(6u16, cells[7].x);

    assert!(it.next(Global).unwrap().is_none());

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// This test fails on Linux if you have the "Noto Sans Tai Tham" font installed
// locally. Disabling this test until it can be fixed.
// test "shape Tai Tham vowels (position differs from advance)" {
#[test]
fn shape_tai_tham_vowels_position_differs_from_advance() {
    // return error.SkipZigTest;
    //
    // PORT NOTE: unconditional `error.SkipZigTest` — the entire body is disabled
    // in the reference (preserved verbatim as a comment). A Rust test that
    // returns is an effective skip (passes).

    // // Note that while this test was necessary for CoreText, the old logic was
    // // working for HarfBuzz. Still we keep it to ensure it has the correct
    // // behavior.
    // const testing = std.testing;
    // const alloc = testing.allocator;
    //
    // // We need a font that supports Tai Tham for this to work, if we can't find
    // // Noto Sans Tai Tham, which is a system font on macOS, we just skip the
    // // test.
    // var testdata = testShaperWithDiscoveredFont(alloc, "Noto Sans Tai Tham") catch return error.SkipZigTest;
    // defer testdata.deinit();
    //
    // var buf: [32]u8 = undefined;
    // var buf_idx: usize = 0;
    // buf_idx += try std.unicode.utf8Encode(0x1a2F, buf[buf_idx..]); // ᨯ
    // buf_idx += try std.unicode.utf8Encode(0x1a70, buf[buf_idx..]); //  ᩰ
    //
    // // Make a screen with some data
    // var t = try terminal.Terminal.init(alloc, .{ .cols = 30, .rows = 3 });
    // defer t.deinit(alloc);
    //
    // // Enable grapheme clustering
    // t.modes.set(.grapheme_cluster, true);
    //
    // var s = t.vtStream(); defer s.deinit();
    // s.nextSlice(buf[0..buf_idx]);
    //
    // var state: terminal.RenderState = .empty; defer state.deinit(alloc);
    // try state.update(alloc, &t);
    //
    // var shaper = &testdata.shaper;
    // var it = shaper.runIterator(.{ .grid = testdata.grid, .cells = state.row_data.get(0).cells.slice() });
    // var count: usize = 0;
    // while (try it.next(alloc)) |run| {
    //     count += 1;
    //     const cells = try shaper.shape(run);
    //     try testing.expectEqual(@as(usize, 2), cells.len);
    //     try testing.expectEqual(@as(u16, 0), cells[0].x);
    //     try testing.expectEqual(@as(u16, 0), cells[1].x);
    //     // The first glyph renders in the next cell. We expect the x_offset
    //     // to equal the cell width. However, with FreeType the cell_width is
    //     // computed from ASCII glyphs, and Noto Sans Tai Tham only has the
    //     // space character in ASCII (with a 3px advance), so the cell_width
    //     // metric doesn't match the actual Tai Tham glyph positioning.
    //     const expected_x_offset: i16 = if (comptime font.options.backend.hasFreetype()) 7 else @intCast(run.grid.metrics.cell_width);
    //     try testing.expectEqual(expected_x_offset, cells[0].x_offset);
    //     try testing.expectEqual(@as(i16, 0), cells[1].x_offset);
    // }
    // try testing.expectEqual(@as(usize, 1), count);
}

// test "shape Tibetan characters" {
//
// R14: Noto Serif Tibetan is not a stock font on any CI runner, so this test
// would skip everywhere by default; it is `#[ignore]`d instead of returning
// `ok`, and FAILS when asked for on a host without the font.
#[cfg(not(target_arch = "wasm32"))]
#[test]
#[ignore = "needs Noto Serif Tibetan, which no stock macOS or Linux install has; \
            install it and run with --ignored (R14)"]
fn shape_tibetan_characters() {
    // We need a font that has multiple glyphs for this codepoint to reproduce
    // the old broken behavior, and Noto Serif Tibetan is one of them. It's not
    // a default Mac font, and if we can't find it we just skip the test.
    let mut testdata = test_shaper_with_discovered_font("Noto Serif Tibetan")
        .expect("shape_tibetan_characters needs the Noto Serif Tibetan font installed");

    let mut buf: Vec<u8> = Vec::new();
    push_utf8(&mut buf, 0x0f00); // ༀ

    // Make a screen with some data
    let mut t = Terminal::init(Global, term_opts(30, 3)).unwrap();

    // Enable grapheme clustering
    t.modes.set(Mode::GraphemeCluster, true);

    {
        let mut s = t.vt_stream();
        s.next_slice(&buf);
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // Get our run iterator
    let shaper: *mut Shaper = &mut testdata.shaper;
    let grid = testdata.grid_ptr();
    let mut it = unsafe {
        (*shaper).run_iterator(shape::RunOptions {
            grid,
            cells: state.row_data[0].cells.slice(),
            selection: None,
            cursor_x: None,
        })
    };
    let mut count: usize = 0;
    while let Some(run) = it.next(Global).unwrap() {
        count += 1;

        let cells = unsafe { (*shaper).shape(run) }.unwrap();
        assert_eq!(2usize, cells.len());
        assert_eq!(0u16, cells[0].x);
        assert_eq!(0u16, cells[1].x);

        // The second glyph renders at the correct location
        assert!(cells[1].x_offset < 2);
    }
    assert_eq!(1usize, count);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// This test fails on Linux if you have the "Noto Sans Tai Tham" font installed
// locally. Disabling this test until it can be fixed.
// test "shape Tai Tham letters (run_offset.y differs from zero)" {
#[test]
fn shape_tai_tham_letters_run_offset_y_differs_from_zero() {
    // return error.SkipZigTest;
    //
    // PORT NOTE: unconditional `error.SkipZigTest`; the body is disabled in the
    // reference (preserved verbatim as a comment). Returning = effective skip.

    // const testing = std.testing; const alloc = testing.allocator;
    // // We need a font that supports Tai Tham for this to work, if we can't find
    // // Noto Sans Tai Tham, which is a system font on macOS, we just skip the test.
    // var testdata = testShaperWithDiscoveredFont(alloc, "Noto Sans Tai Tham") catch return error.SkipZigTest;
    // defer testdata.deinit();
    //
    // var buf: [32]u8 = undefined; var buf_idx: usize = 0;
    // // First grapheme cluster:
    // buf_idx += try std.unicode.utf8Encode(0x1a49, buf[buf_idx..]); // HA
    // buf_idx += try std.unicode.utf8Encode(0x1a60, buf[buf_idx..]); // SAKOT
    // // Second grapheme cluster, combining with the first in a ligature:
    // buf_idx += try std.unicode.utf8Encode(0x1a3f, buf[buf_idx..]); // YA
    // buf_idx += try std.unicode.utf8Encode(0x1a69, buf[buf_idx..]); // U
    //
    // var t = try terminal.Terminal.init(alloc, .{ .cols = 30, .rows = 3 });
    // defer t.deinit(alloc);
    // t.modes.set(.grapheme_cluster, true);
    // var s = t.vtStream(); defer s.deinit();
    // s.nextSlice(buf[0..buf_idx]);
    // var state: terminal.RenderState = .empty; defer state.deinit(alloc);
    // try state.update(alloc, &t);
    // var shaper = &testdata.shaper;
    // var it = shaper.runIterator(.{ .grid = testdata.grid, .cells = state.row_data.get(0).cells.slice() });
    // var count: usize = 0;
    // while (try it.next(alloc)) |run| {
    //     count += 1;
    //     const cells = try shaper.shape(run);
    //     try testing.expectEqual(@as(usize, 3), cells.len);
    //     try testing.expectEqual(@as(u16, 0), cells[0].x);
    //     try testing.expectEqual(@as(u16, 0), cells[1].x);
    //     try testing.expectEqual(@as(u16, 0), cells[2].x); // U from second grapheme
    //     // The U glyph renders at a y below zero
    //     try testing.expectEqual(@as(i16, -3), cells[2].y_offset);
    // }
    // try testing.expectEqual(@as(usize, 1), count);
}

// This test fails on Linux if you have the "Noto Sans Javanese" font installed
// locally. Disabling this test until it can be fixed.
// test "shape Javanese ligatures" {
#[test]
fn shape_javanese_ligatures() {
    // return error.SkipZigTest;
    //
    // PORT NOTE: unconditional `error.SkipZigTest`; the body is disabled in the
    // reference (preserved verbatim as a comment). Returning = effective skip.

    // const testing = std.testing; const alloc = testing.allocator;
    // // We need a font that supports Javanese for this to work, if we can't find
    // // Noto Sans Javanese Regular, which is a system font on macOS, we just skip the test.
    // var testdata = testShaperWithDiscoveredFont(alloc, "Noto Sans Javanese") catch return error.SkipZigTest;
    // defer testdata.deinit();
    //
    // var buf: [32]u8 = undefined; var buf_idx: usize = 0;
    // // First grapheme cluster:
    // buf_idx += try std.unicode.utf8Encode(0xa9a4, buf[buf_idx..]); // NA
    // buf_idx += try std.unicode.utf8Encode(0xa9c0, buf[buf_idx..]); // PANGKON
    // // Second grapheme cluster, combining with the first in a ligature:
    // buf_idx += try std.unicode.utf8Encode(0xa9b2, buf[buf_idx..]); // HA
    // buf_idx += try std.unicode.utf8Encode(0xa9b8, buf[buf_idx..]); // Vowel sign SUKU
    //
    // var t = try terminal.Terminal.init(alloc, .{ .cols = 30, .rows = 3 });
    // defer t.deinit(alloc);
    // t.modes.set(.grapheme_cluster, true);
    // var s = t.vtStream(); defer s.deinit();
    // s.nextSlice(buf[0..buf_idx]);
    // var state: terminal.RenderState = .empty; defer state.deinit(alloc);
    // try state.update(alloc, &t);
    // var shaper = &testdata.shaper;
    // var it = shaper.runIterator(.{ .grid = testdata.grid, .cells = state.row_data.get(0).cells.slice() });
    // var count: usize = 0;
    // while (try it.next(alloc)) |run| {
    //     count += 1;
    //     const cells = try shaper.shape(run);
    //     const cell_width = run.grid.metrics.cell_width;
    //     try testing.expectEqual(@as(usize, 3), cells.len);
    //     try testing.expectEqual(@as(u16, 0), cells[0].x);
    //     try testing.expectEqual(@as(u16, 0), cells[1].x);
    //     try testing.expectEqual(@as(u16, 0), cells[2].x);
    //     // The vowel sign SUKU renders with correct x_offset
    //     try testing.expect(cells[2].x_offset > 3 * cell_width);
    // }
    // try testing.expectEqual(@as(usize, 1), count);
}

// test "shape Chakma vowel sign with ligature (vowel sign renders first)" {
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn shape_chakma_vowel_sign_with_ligature() {
    // We need a font that supports Chakma for this to work, if we can't find
    // Noto Sans Chakma Regular, which is a system font on macOS, we just skip
    // the test.
    let mut testdata = match test_shaper_with_discovered_font("Noto Sans Chakma") {
        Some(t) => t,
        None => return,
    };

    let mut buf: Vec<u8> = Vec::new();
    // First grapheme cluster:
    push_utf8(&mut buf, 0x1111d); // BAA
                                  // Second grapheme cluster:
    push_utf8(&mut buf, 0x11116); // TAA
    push_utf8(&mut buf, 0x11133); // Virama
                                  // Third grapheme cluster, combining with the second in a ligature:
    push_utf8(&mut buf, 0x11120); // YYAA
    push_utf8(&mut buf, 0x1112c); // Vowel Sign U

    // Make a screen with some data
    let mut t = Terminal::init(Global, term_opts(30, 3)).unwrap();

    // Enable grapheme clustering
    t.modes.set(Mode::GraphemeCluster, true);

    {
        let mut s = t.vt_stream();
        s.next_slice(&buf);
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // Get our run iterator
    let shaper: *mut Shaper = &mut testdata.shaper;
    let grid = testdata.grid_ptr();
    let mut it = unsafe {
        (*shaper).run_iterator(shape::RunOptions {
            grid,
            cells: state.row_data[0].cells.slice(),
            selection: None,
            cursor_x: None,
        })
    };
    let mut count: usize = 0;
    while let Some(run) = it.next(Global).unwrap() {
        count += 1;

        let cells = unsafe { (*shaper).shape(run) }.unwrap();
        assert_eq!(4usize, cells.len());
        assert_eq!(0u16, cells[0].x);
        // See the giant "We need to reset the `cell_offset`" comment, but here
        // we should technically have the rest of these be `x` of 1, but that
        // would require going back in the stream to adjust past cells, and
        // we don't take on that complexity.
        assert_eq!(0u16, cells[1].x);
        assert_eq!(0u16, cells[2].x);
        assert_eq!(0u16, cells[3].x);

        // The vowel sign U renders before the TAA:
        assert!(cells[1].x_offset < cells[2].x_offset);
    }
    assert_eq!(1usize, count);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape Bengali ligatures with out of order vowels" {
#[cfg(not(target_arch = "wasm32"))]
#[test]
fn shape_bengali_ligatures_with_out_of_order_vowels() {
    // Whereas this test in CoreText had everything shaping into one giant
    // ligature, HarfBuzz splits it into a few clusters. It still looks okay
    // (see #10332).

    // We need a font that supports Bengali for this to work, if we can't find
    // Arial Unicode MS, which is a system font on macOS, we just skip the
    // test.
    let mut testdata = match test_shaper_with_discovered_font("Arial Unicode MS") {
        Some(t) => t,
        None => return,
    };

    let mut buf: Vec<u8> = Vec::new();
    // First grapheme cluster:
    push_utf8(&mut buf, 0x09b0); // RA
    push_utf8(&mut buf, 0x09be); // Vowel sign AA
                                 // Second grapheme cluster:
    push_utf8(&mut buf, 0x09b7); // SSA
    push_utf8(&mut buf, 0x09cd); // Virama
                                 // Third grapheme cluster:
    push_utf8(&mut buf, 0x099f); // TTA
    push_utf8(&mut buf, 0x09cd); // Virama
                                 // Fourth grapheme cluster:
    push_utf8(&mut buf, 0x09b0); // RA
    push_utf8(&mut buf, 0x09c7); // Vowel sign E

    // Make a screen with some data
    let mut t = Terminal::init(Global, term_opts(30, 3)).unwrap();

    // Enable grapheme clustering
    t.modes.set(Mode::GraphemeCluster, true);

    {
        let mut s = t.vt_stream();
        s.next_slice(&buf);
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // Get our run iterator
    let shaper: *mut Shaper = &mut testdata.shaper;
    let grid = testdata.grid_ptr();
    let mut it = unsafe {
        (*shaper).run_iterator(shape::RunOptions {
            grid,
            cells: state.row_data[0].cells.slice(),
            selection: None,
            cursor_x: None,
        })
    };
    let mut count: usize = 0;
    while let Some(run) = it.next(Global).unwrap() {
        count += 1;

        let cells = unsafe { (*shaper).shape(run) }.unwrap();
        assert_eq!(8usize, cells.len());
        assert_eq!(0u16, cells[0].x);
        assert_eq!(0u16, cells[1].x);

        // Whereas CoreText puts everything all into the first cell (see the
        // corresponding test), HarfBuzz splits into two clusters.
        assert_eq!(2u16, cells[2].x);
        assert_eq!(2u16, cells[3].x);
        assert_eq!(2u16, cells[4].x);
        assert_eq!(2u16, cells[5].x);
        assert_eq!(2u16, cells[6].x);
        assert_eq!(2u16, cells[7].x);

        // The vowel sign E renders before the SSA:
        assert!(cells[2].x_offset < cells[3].x_offset);
    }
    assert_eq!(1usize, count);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape box glyphs" {
#[test]
fn shape_box_glyphs() {
    let mut testdata = test_shaper();

    let mut buf: Vec<u8> = Vec::new();
    push_utf8(&mut buf, 0x2500); // horiz line
    push_utf8(&mut buf, 0x2501); //

    // Make a screen with some data
    let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(&buf);
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // Get our run iterator
    let shaper: *mut Shaper = &mut testdata.shaper;
    let grid = testdata.grid_ptr();
    let mut it = unsafe {
        (*shaper).run_iterator(shape::RunOptions {
            grid,
            cells: state.row_data[0].cells.slice(),
            selection: None,
            cursor_x: None,
        })
    };
    let mut count: usize = 0;
    while let Some(run) = it.next(Global).unwrap() {
        count += 1;
        assert_eq!(2u32, unsafe { (*shaper).hb_buf.get_length() });
        let cells = unsafe { (*shaper).shape(run) }.unwrap();
        assert_eq!(2usize, cells.len());
        assert_eq!(0x2500u32, cells[0].glyph_index);
        assert_eq!(0u16, cells[0].x);
        assert_eq!(0x2501u32, cells[1].glyph_index);
        assert_eq!(1u16, cells[1].x);
    }
    assert_eq!(1usize, count);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape selection boundary" {
#[test]
fn shape_selection_boundary() {
    let mut testdata = test_shaper();

    // Make a screen with some data
    let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(b"a1b2c3d4e5");
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // Full line selection
    {
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: Some([0, 9]),
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(1usize, count);
    }

    // Offset x, goes to end of line selection
    {
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: Some([2, 9]),
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(2usize, count);
    }

    // Offset x, starts at beginning of line
    {
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: Some([0, 3]),
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(2usize, count);
    }

    // Selection only subset of line
    {
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: Some([1, 3]),
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(3usize, count);
    }

    // Selection only one character
    {
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: Some([1, 1]),
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(3usize, count);
    }

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape cursor boundary" {
#[test]
fn shape_cursor_boundary() {
    let mut testdata = test_shaper();

    // Make a screen with some data
    let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice(b"a1b2c3d4e5");
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // No cursor is full line
    {
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(1usize, count);
    }

    {
        // Cursor at index 0 is two runs
        {
            let shaper: *mut Shaper = &mut testdata.shaper;
            let grid = testdata.grid_ptr();
            let mut it = unsafe {
                (*shaper).run_iterator(shape::RunOptions {
                    grid,
                    cells: state.row_data[0].cells.slice(),
                    selection: None,
                    cursor_x: Some(0),
                })
            };
            let mut count: usize = 0;
            while let Some(run) = it.next(Global).unwrap() {
                count += 1;
                let _ = unsafe { (*shaper).shape(run) }.unwrap();
            }
            assert_eq!(2usize, count);
        }
        // And without cursor splitting remains one
        {
            let shaper: *mut Shaper = &mut testdata.shaper;
            let grid = testdata.grid_ptr();
            let mut it = unsafe {
                (*shaper).run_iterator(shape::RunOptions {
                    grid,
                    cells: state.row_data[0].cells.slice(),
                    selection: None,
                    cursor_x: None,
                })
            };
            let mut count: usize = 0;
            while let Some(run) = it.next(Global).unwrap() {
                count += 1;
                let _ = unsafe { (*shaper).shape(run) }.unwrap();
            }
            assert_eq!(1usize, count);
        }
    }

    {
        // Cursor at index 1 is three runs
        {
            let shaper: *mut Shaper = &mut testdata.shaper;
            let grid = testdata.grid_ptr();
            let mut it = unsafe {
                (*shaper).run_iterator(shape::RunOptions {
                    grid,
                    cells: state.row_data[0].cells.slice(),
                    selection: None,
                    cursor_x: Some(1),
                })
            };
            let mut count: usize = 0;
            while let Some(run) = it.next(Global).unwrap() {
                count += 1;
                let _ = unsafe { (*shaper).shape(run) }.unwrap();
            }
            assert_eq!(3usize, count);
        }
        // And without cursor splitting remains one
        {
            let shaper: *mut Shaper = &mut testdata.shaper;
            let grid = testdata.grid_ptr();
            let mut it = unsafe {
                (*shaper).run_iterator(shape::RunOptions {
                    grid,
                    cells: state.row_data[0].cells.slice(),
                    selection: None,
                    cursor_x: None,
                })
            };
            let mut count: usize = 0;
            while let Some(run) = it.next(Global).unwrap() {
                count += 1;
                let _ = unsafe { (*shaper).shape(run) }.unwrap();
            }
            assert_eq!(1usize, count);
        }
    }
    {
        // Cursor at last col is two runs
        {
            let shaper: *mut Shaper = &mut testdata.shaper;
            let grid = testdata.grid_ptr();
            let mut it = unsafe {
                (*shaper).run_iterator(shape::RunOptions {
                    grid,
                    cells: state.row_data[0].cells.slice(),
                    selection: None,
                    cursor_x: Some(9),
                })
            };
            let mut count: usize = 0;
            while let Some(run) = it.next(Global).unwrap() {
                count += 1;
                let _ = unsafe { (*shaper).shape(run) }.unwrap();
            }
            assert_eq!(2usize, count);
        }
        // And without cursor splitting remains one
        {
            let shaper: *mut Shaper = &mut testdata.shaper;
            let grid = testdata.grid_ptr();
            let mut it = unsafe {
                (*shaper).run_iterator(shape::RunOptions {
                    grid,
                    cells: state.row_data[0].cells.slice(),
                    selection: None,
                    cursor_x: None,
                })
            };
            let mut count: usize = 0;
            while let Some(run) = it.next(Global).unwrap() {
                count += 1;
                let _ = unsafe { (*shaper).shape(run) }.unwrap();
            }
            assert_eq!(1usize, count);
        }
    }

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape cursor boundary and colored emoji" {
#[test]
fn shape_cursor_boundary_and_colored_emoji() {
    let mut testdata = test_shaper();

    // Make a screen with some data
    let mut t = Terminal::init(Global, term_opts(3, 10)).unwrap();

    {
        let mut s = t.vt_stream();
        s.next_slice("👍🏼".as_bytes());
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

    // No cursor is full line
    {
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(1usize, count);
    }

    // Cursor on emoji does not split it
    {
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: Some(0),
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(1usize, count);
    }
    {
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(1usize, count);
    }
    {
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: Some(1),
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(1usize, count);
    }
    {
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(1usize, count);
    }

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape cell attribute change" {
#[test]
fn shape_cell_attribute_change() {
    let mut testdata = test_shaper();

    // Plain >= should shape into 1 run
    {
        let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b">=");
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(Global, &mut t).unwrap();

        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(1usize, count);

        state.deinit(Global);
        t.deinit(Global);
    }

    // Bold vs regular should split
    {
        let mut t = Terminal::init(Global, term_opts(3, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            s.next_slice(b">");
            s.next_slice(b"\x1b[1m");
            s.next_slice(b"=");
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(Global, &mut t).unwrap();

        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(2usize, count);

        state.deinit(Global);
        t.deinit(Global);
    }

    // Changing fg color should split
    {
        let mut t = Terminal::init(Global, term_opts(3, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // RGB 1, 2, 3
            s.next_slice(b"\x1b[38;2;1;2;3m");
            s.next_slice(b">");
            // RGB 3, 2, 1
            s.next_slice(b"\x1b[38;2;3;2;1m");
            s.next_slice(b"=");
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(Global, &mut t).unwrap();

        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(2usize, count);

        state.deinit(Global);
        t.deinit(Global);
    }

    // Changing bg color should not split
    {
        let mut t = Terminal::init(Global, term_opts(3, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // RGB 1, 2, 3 bg
            s.next_slice(b"\x1b[48;2;1;2;3m");
            s.next_slice(b">");
            // RGB 3, 2, 1 bg
            s.next_slice(b"\x1b[48;2;3;2;1m");
            s.next_slice(b"=");
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(Global, &mut t).unwrap();

        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(1usize, count);

        state.deinit(Global);
        t.deinit(Global);
    }

    // Same bg color should not split
    {
        let mut t = Terminal::init(Global, term_opts(3, 10)).unwrap();

        {
            let mut s = t.vt_stream();
            // RGB 1, 2, 3 bg
            s.next_slice(b"\x1b[48;2;1;2;3m");
            s.next_slice(b">");
            s.next_slice(b"=");
            s.deinit();
        }

        let mut state = RenderState::empty();
        state.update(Global, &mut t).unwrap();

        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: None,
                cursor_x: None,
            })
        };
        let mut count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            count += 1;
            let _ = unsafe { (*shaper).shape(run) }.unwrap();
        }
        assert_eq!(1usize, count);

        state.deinit(Global);
        t.deinit(Global);
    }

    testdata.deinit();
}

/// `std.unicode.utf8Encode(cp, buf[i..])` — append the UTF-8 encoding of `cp`.
fn push_utf8(buf: &mut Vec<u8>, cp: u32) {
    let c = char::from_u32(cp).unwrap();
    let mut tmp = [0u8; 4];
    buf.extend_from_slice(c.encode_utf8(&mut tmp).as_bytes());
}
