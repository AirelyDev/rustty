//! Tests for the CoreText shaper — a faithful port of the 27 `test { ... }`
//! blocks in ghostty `src/font/shaper/coretext.zig`. Kept in a sibling file
//! (mirroring `harfbuzz_tests.rs`), attached as the `tests` submodule of
//! `coretext.rs`.
//!
//! PORT NOTE (backend): these tests run on the macOS `coretext` configuration —
//! CoreText faces (`face/coretext.rs`) + this CoreText shaper. The discovered-font
//! tests (`shape Devanagari`, the Tai Tham / Javanese / Chakma / Bengali cases)
//! assert EXACT CoreText metrics validated against live macOS CoreText, so they
//! require the named system fonts to be present; on a font-not-found they skip
//! (return early), matching the reference's `catch return error.SkipZigTest`.

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
use rustty_vt::modes::Mode;
use rustty_vt::page::{Cell, CellRgb, ContentTag};
use rustty_vt::point::{Coordinate, Point};
use rustty_vt::render::RenderState;
use rustty_vt::terminal::{Colors, Options, Terminal};

// PORT NOTE: `terminal.Terminal.init(alloc, .{ .cols, .rows })` relies on field
// defaults; Rust `Options` has none, so this helper supplies them verbatim
// (mirrors `harfbuzz_tests.rs`).
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
    // `grid: *SharedGrid` — a heap `SharedGrid` (the reference `alloc.create`);
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

// const TestFont = enum { code_new_roman, geist_mono, inconsolata, jetbrains_mono, monaspace_neon, nerd_font };
// PORT NOTE: `code_new_roman` is a faithful member of the reference enum but is
// not selected by any of the 27 ported tests; `#[allow(dead_code)]` keeps the
// variant (and its `switch` arm) without a never-constructed warning.
#[derive(Clone, Copy)]
#[allow(dead_code)]
enum TestFont {
    CodeNewRoman,
    GeistMono,
    Inconsolata,
    JetbrainsMono,
    MonaspaceNeon,
    NerdFont,
}

/// Helper to return a fully initialized shaper.
// fn testShaper(alloc: Allocator) !TestShaper { return try testShaperWithFont(alloc, .inconsolata); }
fn test_shaper() -> TestShaper {
    test_shaper_with_font(TestFont::Inconsolata)
}

// fn testShaperWithFont(alloc: Allocator, font_req: TestFont) !TestShaper {
fn test_shaper_with_font(font_req: TestFont) -> TestShaper {
    // const testEmojiText = font.embedded.emoji_text;
    let test_emoji_text = rustty_font::embedded::EMOJI_TEXT;
    // const testFont = switch (font_req) { ... };
    let test_font = match font_req {
        TestFont::CodeNewRoman => rustty_font::embedded::CODE_NEW_ROMAN,
        TestFont::Inconsolata => rustty_font::embedded::INCONSOLATA,
        TestFont::GeistMono => rustty_font::embedded::GEIST_MONO,
        TestFont::JetbrainsMono => rustty_font::embedded::JETBRAINS_MONO,
        TestFont::MonaspaceNeon => rustty_font::embedded::MONASPACE_NEON,
        TestFont::NerdFont => rustty_font::embedded::TEST_NERD_FONT,
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

    // if (font.options.backend != .coretext) {
    //     // Coretext doesn't support Noto's format
    //     _ = try c.add(alloc, try .init(lib, testEmoji, ...), .{ ... });
    // } else {
    //     // On CoreText we want to load Apple Emoji, we should have it.
    //     var disco = font.Discover.init(lib);
    //     var disco_it = try disco.discover(alloc, .{ .family = "Apple Color Emoji", .size = 12, .monospace = false });
    //     var face = (try disco_it.next()).?;
    //     _ = try c.addDeferred(alloc, face, .{ ... });
    // }
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

/// Return a fully initialized shaper by discovering a named font on the system.
// fn testShaperWithDiscoveredFont(alloc: Allocator, font_req: [:0]const u8) !TestShaper {
//
// PORT NOTE: `error.SkipZigTest` / `error.FontNotFound` (discovery returns null)
// is modeled as `Option::None` — the caller returns early — after the R14 pivot
// [`require_system_font_or_skip`], so the skip is printed, and is a failure
// under `RUSTTY_REQUIRE_SYSTEM_FONTS=1`.
fn test_shaper_with_discovered_font(font_req: &str) -> Option<TestShaper> {
    use crate::discovery::{Descriptor, Discover};

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

/// Build the `CodepointResolver` for `SharedGrid.init(alloc, .{ .collection = c })`.
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

/// `std.unicode.utf8Encode(cp, buf[i..])` — append the UTF-8 encoding of `cp`.
fn push_utf8(buf: &mut Vec<u8>, cp: u32) {
    let c = char::from_u32(cp).unwrap();
    let mut tmp = [0u8; 4];
    buf.extend_from_slice(c.encode_utf8(&mut tmp).as_bytes());
}

// test "run iterator" {
#[test]
fn run_iterator() {
    let mut testdata = test_shaper();

    {
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
        assert_eq!(3usize, count);

        state.deinit(Global);
        t.deinit(Global);
    }

    // Bad ligatures
    for bad in [b"fl".as_slice(), b"fi".as_slice(), b"st".as_slice()] {
        let mut t = Terminal::init(Global, term_opts(5, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(bad);
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
        assert_eq!(2usize, count);

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
        let mut t = Terminal::init(Global, term_opts(5, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            // Set red background
            s.next_slice(b"\x1b[48;2;255;0;0m");
            s.next_slice(b"A");
            s.deinit();
        }

        // Get our first row
        {
            let list_cell = unsafe {
                (*t.screens.active)
                    .pages
                    .get_cell(Point::Active(Coordinate { x: 1, y: 0 }))
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
            let run = it.next(Global).unwrap().unwrap();
            let cells = unsafe { (*shaper).shape(run) }.unwrap();
            assert_eq!(3usize, cells.len());
        }
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

    let mut buf: Vec<u8> = Vec::new();
    push_utf8(&mut buf, 0x1F44D); // Thumbs up plain
    push_utf8(&mut buf, 0x1F44D); // Thumbs up plain
    push_utf8(&mut buf, 0x1F3FD); // Medium skin tone

    let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(&buf);
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
    testdata.deinit();
}

// test "shape nerd fonts" {
#[test]
fn shape_nerd_fonts() {
    let mut testdata = test_shaper_with_font(TestFont::NerdFont);

    let mut buf: Vec<u8> = Vec::new();
    push_utf8(&mut buf, ' ' as u32); // space
    push_utf8(&mut buf, 0xF024B); // nf-md-folder
    push_utf8(&mut buf, ' ' as u32); // space

    let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(&buf);
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

// https://github.com/mitchellh/ghostty/issues/1708
// test "shape left-replaced lig in last run" {
#[test]
fn shape_left_replaced_lig_in_last_run() {
    let mut testdata = test_shaper_with_font(TestFont::GeistMono);

    {
        let mut t = Terminal::init(Global, term_opts(5, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"!==");
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

// https://github.com/mitchellh/ghostty/issues/1708
// test "shape left-replaced lig in early run" {
#[test]
fn shape_left_replaced_lig_in_early_run() {
    let mut testdata = test_shaper_with_font(TestFont::GeistMono);

    {
        let mut t = Terminal::init(Global, term_opts(5, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"!==X");
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

        let run = it.next(Global).unwrap().unwrap();
        assert_eq!(4usize, run.cells as usize);
        let cells = unsafe { (*shaper).shape(run) }.unwrap();
        assert_eq!(2usize, cells.len());

        state.deinit(Global);
        t.deinit(Global);
    }

    testdata.deinit();
}

// https://github.com/mitchellh/ghostty/issues/1664
// test "shape U+3C9 with JB Mono" {
#[test]
fn shape_u_3c9_with_jb_mono() {
    let mut testdata = test_shaper_with_font(TestFont::JetbrainsMono);

    {
        let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice("\u{03C9} foo".as_bytes());
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

        let mut run_count: usize = 0;
        let mut cell_count: usize = 0;
        while let Some(run) = it.next(Global).unwrap() {
            run_count += 1;
            let cells = unsafe { (*shaper).shape(run) }.unwrap();
            cell_count += cells.len();
        }
        assert_eq!(1usize, run_count);
        assert_eq!(5usize, cell_count);

        state.deinit(Global);
        t.deinit(Global);
    }

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

    let lc = unsafe {
        (*t.screens.active)
            .pages
            .get_cell(Point::Active(Coordinate { x: 0, y: 1 }))
            .unwrap()
    };
    unsafe {
        *lc.cell = Cell::init(0x1F9D4); // Person with beard
    }
    let graphemes: [u32; 4] = [
        0x1F3FB, // Light skin tone (Fitz 1-2)
        0x200D,  // ZWJ
        0x2642,  // Male sign
        0xFE0F,  // Emoji presentation selector
    ];
    unsafe {
        (*rustty_vt::pagelist::Node::page(lc.node))
            .set_graphemes(lc.row, lc.cell, &graphemes)
            .unwrap();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();

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
        let cells = unsafe { (*shaper).shape(run) }.unwrap();
        assert_eq!(1usize, cells.len());
    }
    assert_eq!(1usize, count);

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

    let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(&buf);
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

    let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(&buf);
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

    let mut t = Terminal::init(Global, term_opts(30, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"A");
        s.next_slice(b"\x1b[5C"); // 5 spaces forward
        s.next_slice(b"B");
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

// This test exists because the string it uses causes CoreText to output a
// non-monotonic run, which we need to handle by sorting the resulting buffer.
// test "shape Devanagari string" {
#[test]
fn shape_devanagari_string() {
    // We need a font that supports devanagari for this to work, if we can't
    // find Arial Unicode MS, which is a system font on macOS, we just skip
    // the test.
    let mut testdata = match test_shaper_with_discovered_font("Arial Unicode MS") {
        Some(t) => t,
        None => return,
    };

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

    let run = it.next(Global).unwrap();
    assert!(run.is_some());
    let cells = unsafe { (*shaper).shape(run.unwrap()) }.unwrap();

    // To understand the `x`/`cluster` assertions here, run with the "For
    // debugging positions" code turned on and `extra_debugging` set to true.
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

// test "shape Tai Tham vowels (position differs from advance)" {
#[test]
fn shape_tai_tham_vowels_position_differs_from_advance() {
    // We need a font that supports Tai Tham for this to work, if we can't find
    // Noto Sans Tai Tham, which is a system font on macOS, we just skip the
    // test.
    let mut testdata = match test_shaper_with_discovered_font("Noto Sans Tai Tham") {
        Some(t) => t,
        None => return,
    };

    let mut buf: Vec<u8> = Vec::new();
    // PORT NOTE: the reference literal `0x1a2F` mixes hex casing verbatim
    // (coretext.zig:1610); kept exact, with the clippy lint locally allowed.
    #[allow(clippy::mixed_case_hex_literals)]
    let tai_tham_letter = 0x1a2F;
    push_utf8(&mut buf, tai_tham_letter); // ᨯ
    push_utf8(&mut buf, 0x1a70); //  ᩰ

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
        let cell_width = unsafe { (*run.grid).metrics.cell_width };
        assert_eq!(2usize, cells.len());
        assert_eq!(0u16, cells[0].x);
        assert_eq!(0u16, cells[1].x);

        // The first glyph renders in the next cell
        assert_eq!(cell_width as i16, cells[0].x_offset);
        assert_eq!(0i16, cells[1].x_offset);
    }
    assert_eq!(1usize, count);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape Tai Tham letters (position.y differs from advance)" {
#[test]
fn shape_tai_tham_letters_position_y_differs_from_advance() {
    // We need a font that supports Tai Tham for this to work, if we can't find
    // Noto Sans Tai Tham, which is a system font on macOS, we just skip the
    // test.
    let mut testdata = match test_shaper_with_discovered_font("Noto Sans Tai Tham") {
        Some(t) => t,
        None => return,
    };

    let mut buf: Vec<u8> = Vec::new();

    // First grapheme cluster:
    push_utf8(&mut buf, 0x1a49); // HA
    push_utf8(&mut buf, 0x1a60); // SAKOT
                                 // Second grapheme cluster, combining with the first in a ligature:
    push_utf8(&mut buf, 0x1a3f); // YA
    push_utf8(&mut buf, 0x1a69); // U

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
        assert_eq!(3usize, cells.len());
        assert_eq!(0u16, cells[0].x);
        assert_eq!(0u16, cells[1].x);
        assert_eq!(0u16, cells[2].x); // U from second grapheme

        // The U glyph renders at a y below zero
        assert_eq!(-3i16, cells[2].y_offset);
    }
    assert_eq!(1usize, count);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape Javanese ligatures" {
#[test]
fn shape_javanese_ligatures() {
    // We need a font that supports Javanese for this to work, if we can't find
    // Noto Sans Javanese Regular, which is a system font on macOS, we just
    // skip the test.
    let mut testdata = match test_shaper_with_discovered_font("Noto Sans Javanese") {
        Some(t) => t,
        None => return,
    };

    let mut buf: Vec<u8> = Vec::new();

    // First grapheme cluster:
    push_utf8(&mut buf, 0xa9a4); // NA
    push_utf8(&mut buf, 0xa9c0); // PANGKON
                                 // Second grapheme cluster, combining with the first in a ligature:
    push_utf8(&mut buf, 0xa9b2); // HA
    push_utf8(&mut buf, 0xa9b8); // Vowel sign SUKU

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
        let cell_width = unsafe { (*run.grid).metrics.cell_width };
        assert_eq!(3usize, cells.len());
        assert_eq!(0u16, cells[0].x);
        assert_eq!(0u16, cells[1].x);
        assert_eq!(0u16, cells[2].x);

        // The vowel sign SUKU renders with correct x_offset
        assert!(cells[2].x_offset as i64 > 3 * cell_width as i64);
    }
    assert_eq!(1usize, count);

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}

// test "shape Chakma vowel sign with ligature (vowel sign renders first)" {
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
#[test]
fn shape_bengali_ligatures_with_out_of_order_vowels() {
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
                                 // Third grapheme cluster, combining with the second in a ligature:
    push_utf8(&mut buf, 0x099f); // TTA
    push_utf8(&mut buf, 0x09cd); // Virama
                                 // Fourth grapheme cluster, combining with the previous two in a ligature:
    push_utf8(&mut buf, 0x09b0); // RA
    push_utf8(&mut buf, 0x09c7); // Vowel sign E

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
        // See the giant "We need to reset the `cell_offset`" comment, but here
        // we should technically have the rest of these be `x` of 2, but that
        // would require going back in the stream to adjust past cells, and
        // we don't take on that complexity.
        assert_eq!(0u16, cells[2].x);
        assert_eq!(0u16, cells[3].x);
        assert_eq!(0u16, cells[4].x);
        assert_eq!(0u16, cells[5].x);
        assert_eq!(0u16, cells[6].x);
        assert_eq!(0u16, cells[7].x);

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

    let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(&buf);
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

    let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"a1b2c3d4e5");
        s.deinit();
    }

    let mut state = RenderState::empty();
    state.update(Global, &mut t).unwrap();
    let cols = t.cols;

    // Full line selection
    {
        let shaper: *mut Shaper = &mut testdata.shaper;
        let grid = testdata.grid_ptr();
        let mut it = unsafe {
            (*shaper).run_iterator(shape::RunOptions {
                grid,
                cells: state.row_data[0].cells.slice(),
                selection: Some([0, (cols - 1) as u16]),
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
                selection: Some([2, (cols - 1) as u16]),
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
            s.next_slice(b"\x1b[1m"); // Bold
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

    // Changing bg color should NOT split
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

// test "shape high plane sprite font codepoint" {
#[test]
fn shape_high_plane_sprite_font_codepoint() {
    // While creating runs, the CoreText shaper uses `0` codepoints to
    // pad its codepoint list to account for high plane characters which
    // use two UTF-16 code units. This is so that, after shaping, the string
    // indices can be used to find the originating codepoint / cluster.
    //
    // This is a problem for special (sprite) fonts, which need to be "shaped"
    // by simply returning the input codepoints verbatim. We include logic to
    // skip `0` codepoints when constructing the shaped run for sprite fonts,
    // this test verifies that it works correctly.

    let mut testdata = test_shaper();

    let mut t = Terminal::init(Global, term_opts(10, 3)).unwrap();
    {
        let mut s = t.vt_stream();
        // U+1FB70: Vertical One Eighth Block-2
        s.next_slice("\u{1FB70}".as_bytes());
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
    // We should get one run
    let run = it.next(Global).unwrap().unwrap();
    // The run state should have the UTF-16 encoding of the character.
    assert_eq!(&[0xD83Eu16, 0xDF70], unsafe {
        (*shaper).run_state.unichars.as_slice()
    });
    // The codepoint list should be padded.
    assert_eq!(
        &[
            Codepoint {
                codepoint: 0x1FB70,
                cluster: 0
            },
            Codepoint {
                codepoint: 0,
                cluster: 0
            },
        ],
        unsafe { (*shaper).run_state.codepoints.as_slice() }
    );
    // And when shape it
    let cells = unsafe { (*shaper).shape(run) }.unwrap();
    // we should have
    // - 1 cell
    assert_eq!(1u16, run.cells);
    // - at position 0
    assert_eq!(0u16, run.offset);
    // - with 1 glyph in it
    assert_eq!(1usize, cells.len());
    // - at position 0
    assert_eq!(0u16, cells[0].x);
    // - the glyph index should be equal to the codepoint
    assert_eq!(0x1FB70u32, cells[0].glyph_index);
    // - it should be a sprite font
    assert!(run.font_index.special().is_some());
    // And we should get a null run after that
    assert!(it.next(Global).unwrap().is_none());

    state.deinit(Global);
    t.deinit(Global);
    testdata.deinit();
}
