//! formatter tests: HTML output, palette options, codepoint maps,
//! hyperlinks and the pin-map allocator routing.

use super::*;

// test "Page html with multiple styles"
#[test]
fn page_html_with_multiple_styles() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Set bold, then italic, then reset
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[1mbold\x1b[3mitalic\x1b[0mnormal");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(page, Options::html());
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(
        output,
        concat!(
            "<div style=\"font-family: monospace; white-space: pre;\">",
            "<div style=\"display: inline;font-weight: bold;\">bold</div>",
            "<div style=\"display: inline;font-weight: bold;font-style: italic;\">italic</div>",
            "normal",
            "</div>"
        )
        .as_bytes()
    );
    t.deinit(Global);
}

// test "Page html plain text"
#[test]
fn page_html_plain_text() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello, world");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(page, Options::html());
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    // Plain text without styles should be wrapped in monospace div
    assert_eq!(
        output,
        b"<div style=\"font-family: monospace; white-space: pre;\">hello, world</div>"
    );
    t.deinit(Global);
}

// test "Page html with colors"
#[test]
fn page_html_with_colors() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Set red foreground, blue background
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[31;44mcolored");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(page, Options::html());
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(
        output,
        concat!(
            "<div style=\"font-family: monospace; white-space: pre;\">",
            "<div style=\"display: inline;color: var(--vt-palette-1);background-color: var(--vt-palette-4);\">colored</div>",
            "</div>"
        ).as_bytes()
    );
    t.deinit(Global);
}

// test "TerminalFormatter html with palette"
#[test]
fn terminalformatter_html_with_palette() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        // Modify some palette colors
        s.next_slice(b"\x1b]4;0;rgb:12/34/56\x1b\\");
        s.next_slice(b"\x1b]4;1;rgb:ab/cd/ef\x1b\\");
        s.next_slice(b"\x1b]4;255;rgb:ff/00/ff\x1b\\");
        s.next_slice(b"test");
    }
    let t_ptr: *const Terminal = &t;
    let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::html());
    formatter.extra.palette = true;
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();

    // Verify palette CSS variables are emitted
    assert!(index_of(output, b"<style>:root{").is_some());
    assert!(index_of(output, b"--vt-palette-0: #123456;").is_some());
    assert!(index_of(output, b"--vt-palette-1: #abcdef;").is_some());
    assert!(index_of(output, b"--vt-palette-255: #ff00ff;").is_some());
    assert!(index_of(output, b"}</style>").is_some());
    assert!(index_of(output, b"test").is_some());
    t.deinit(Global);
}

// test "Page html with background and foreground colors"
#[test]
fn page_html_with_background_and_foreground_colors() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(
        page,
        Options {
            emit: Format::Html,
            background: Some(crate::color::RGB {
                r: 0x12,
                g: 0x34,
                b: 0x56,
            }),
            foreground: Some(crate::color::RGB {
                r: 0xab,
                g: 0xcd,
                b: 0xef,
            }),
            ..Options::html()
        },
    );
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(
        output,
        b"<div style=\"font-family: monospace; white-space: pre;background-color: #123456;color: #abcdef;\">hello</div>"
    );
    t.deinit(Global);
}

// test "Page html with escaping"
#[test]
fn page_html_with_escaping() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"<tag>&\"'text");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::html());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(
        output,
        b"<div style=\"font-family: monospace; white-space: pre;\">&lt;tag&gt;&amp;&quot;&#39;text</div>"
    );

    // Verify point map length matches output
    assert_eq!(output.len(), point_map.len());

    // Opening wrapper div
    let wrapper_start = "<div style=\"font-family: monospace; white-space: pre;\">";
    let wrapper_start_len = wrapper_start.len();
    for i in 0..wrapper_start_len {
        assert_eq!(coord(0, 0), point_map[i]);
    }

    // Verify each character maps correctly, accounting for escaping
    let offset = wrapper_start_len;
    // < (4 bytes: &lt;) -> x=0
    for i in 0..4 {
        assert_eq!(coord(0, 0), point_map[offset + i]);
    }
    // t (1 byte) -> x=1
    assert_eq!(coord(1, 0), point_map[offset + 4]);
    // a (1 byte) -> x=2
    assert_eq!(coord(2, 0), point_map[offset + 5]);
    // g (1 byte) -> x=3
    assert_eq!(coord(3, 0), point_map[offset + 6]);
    // > (4 bytes: &gt;) -> x=4
    for i in 0..4 {
        assert_eq!(coord(4, 0), point_map[offset + 7 + i]);
    }
    // & (5 bytes: &amp;) -> x=5
    for i in 0..5 {
        assert_eq!(coord(5, 0), point_map[offset + 11 + i]);
    }
    // " (6 bytes: &quot;) -> x=6
    for i in 0..6 {
        assert_eq!(coord(6, 0), point_map[offset + 16 + i]);
    }
    // ' (5 bytes: &#39;) -> x=7
    for i in 0..5 {
        assert_eq!(coord(7, 0), point_map[offset + 22 + i]);
    }
    // t (1 byte) -> x=8
    assert_eq!(coord(8, 0), point_map[offset + 27]);
    // e (1 byte) -> x=9
    assert_eq!(coord(9, 0), point_map[offset + 28]);
    // x (1 byte) -> x=10
    assert_eq!(coord(10, 0), point_map[offset + 29]);
    // t (1 byte) -> x=11
    assert_eq!(coord(11, 0), point_map[offset + 30]);
    t.deinit(Global);
}

// test "Page html with unicode as numeric entities"
#[test]
fn page_html_with_unicode_as_numeric_entities() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Box drawing characters that caused issue #9426
    {
        let mut s = t.vt_stream();
        s.next_slice("╰─ ❯".as_bytes());
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(page, Options::html());
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    // Expected: box drawing chars as numeric entities
    // ╰ = U+2570 = 9584, ─ = U+2500 = 9472, ❯ = U+276F = 10095
    assert_eq!(
        output,
        b"<div style=\"font-family: monospace; white-space: pre;\">&#9584;&#9472; &#10095;</div>"
    );
    t.deinit(Global);
}

// test "Page html trailing blank lines"
#[test]
fn page_html_trailing_blank_lines() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\nworld\r\n\r\n");
    }

    let pages = unsafe { &(*t.screens.active).pages };
    assert!(!pages.pages.first.is_null());
    assert!(core::ptr::eq(pages.pages.first, pages.pages.last));

    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(page, Options::html());

    let state = formatter.format_with_state(&mut builder).unwrap();
    let output = builder.as_slice();

    // The closing div behaves as a newline
    assert_eq!((unsafe { &*page }.size.rows - 2) as usize, state.rows);
    assert_eq!(
        b"<div style=\"font-family: monospace; white-space: pre;\">hello\nworld</div>".as_slice(),
        output,
    );
    t.deinit(Global);
}

// test "Page html ascii characters unchanged"
#[test]
fn page_html_ascii_characters_unchanged() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(page, Options::html());
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    // ASCII should be emitted directly
    assert_eq!(
        output,
        b"<div style=\"font-family: monospace; white-space: pre;\">hello world</div>"
    );
    t.deinit(Global);
}

// test "Page html mixed ascii and unicode"
#[test]
fn page_html_mixed_ascii_and_unicode() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice("test ╰─❯ ok".as_bytes());
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(page, Options::html());
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    // Mix of ASCII and Unicode entities
    assert_eq!(output, b"<div style=\"font-family: monospace; white-space: pre;\">test &#9584;&#9472;&#10095; ok</div>");
    t.deinit(Global);
}

// test "Page VT with palette option emits RGB"
#[test]
fn page_vt_with_palette_option_emits_rgb() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        // Set a custom palette color and use it
        s.next_slice(b"\x1b]4;1;rgb:ab/cd/ef\x1b\\");
        s.next_slice(b"\x1b[31mred");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };

    // Without palette option - should emit palette index
    {
        builder.clear();
        let formatter: PageFormatter = PageFormatter::init(page, Options::vt());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"\x1b[0m\x1b[38;5;1mred\x1b[0m");
    }

    // With palette option - should emit RGB directly
    {
        builder.clear();
        let mut o = Options::vt();
        o.palette = Some(&t.colors.palette.current as *const crate::color::Palette);
        let formatter: PageFormatter = PageFormatter::init(page, o);
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(output, b"\x1b[0m\x1b[38;2;171;205;239mred\x1b[0m");
    }
    t.deinit(Global);
}

// test "Page html with palette option emits RGB"
#[test]
fn page_html_with_palette_option_emits_rgb() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        // Set a custom palette color and use it
        s.next_slice(b"\x1b]4;1;rgb:ab/cd/ef\x1b\\");
        s.next_slice(b"\x1b[31mred");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };

    // Without palette option - should emit CSS variable
    {
        builder.clear();
        let formatter: PageFormatter = PageFormatter::init(page, Options::html());
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(
            output,
            concat!(
                "<div style=\"font-family: monospace; white-space: pre;\">",
                "<div style=\"display: inline;color: var(--vt-palette-1);\">red</div>",
                "</div>"
            )
            .as_bytes()
        );
    }

    // With palette option - should emit RGB directly
    {
        builder.clear();
        let mut o = Options::html();
        o.palette = Some(&t.colors.palette.current as *const crate::color::Palette);
        let formatter: PageFormatter = PageFormatter::init(page, o);
        formatter.format(&mut builder).unwrap();
        let output = builder.as_slice();
        assert_eq!(
            output,
            concat!(
                "<div style=\"font-family: monospace; white-space: pre;\">",
                "<div style=\"display: inline;color: rgb(171, 205, 239);\">red</div>",
                "</div>"
            )
            .as_bytes()
        );
    }
    t.deinit(Global);
}

// test "Page VT style reset properly closes styles"
#[test]
fn page_vt_style_reset_properly_closes_styles() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Set bold, then reset with SGR 0
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[1mbold\x1b[0mnormal");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    builder.clear();
    let formatter: PageFormatter = PageFormatter::init(page, Options::vt());
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    // The reset should properly close the bold style
    assert_eq!(output, b"\x1b[0m\x1b[1mbold\x1b[0mnormal");
    t.deinit(Global);
}

// test "Page codepoint_map single replacement"
#[test]
fn page_codepoint_map_single_replacement() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    // Replace 'o' with 'x'
    let map = vec![CodepointMap {
        range: ['o' as u21, 'o' as u21],
        replacement: Replacement::Codepoint('x' as u21),
    }];
    let mut o = Options::plain();
    o.codepoint_map = Some(map);
    let mut formatter: PageFormatter = PageFormatter::init(page, o);
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hellx wxrld");

    // Verify point map - each output byte should map to original cell position
    assert_eq!(output.len(), point_map.len());
    // "hello world" -> "hellx wxrld"
    // h e l l o   w o r l d
    // 0 1 2 3 4 5 6 7 8 9 10
    assert_eq!(coord(0, 0), point_map[0]); // h
    assert_eq!(coord(1, 0), point_map[1]); // e
    assert_eq!(coord(2, 0), point_map[2]); // l
    assert_eq!(coord(3, 0), point_map[3]); // l
    assert_eq!(coord(4, 0), point_map[4]); // x (was o)
    assert_eq!(coord(5, 0), point_map[5]); // space
    assert_eq!(coord(6, 0), point_map[6]); // w
    assert_eq!(coord(7, 0), point_map[7]); // x (was o)
    assert_eq!(coord(8, 0), point_map[8]); // r
    assert_eq!(coord(9, 0), point_map[9]); // l
    assert_eq!(coord(10, 0), point_map[10]); // d
    t.deinit(Global);
}

// test "Page codepoint_map conflicting replacement prefers last"
#[test]
fn page_codepoint_map_conflicting_replacement_prefers_last() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    // Replace 'o' with 'x', then with 'y' - should prefer last
    let map = vec![
        CodepointMap {
            range: ['o' as u21, 'o' as u21],
            replacement: Replacement::Codepoint('x' as u21),
        },
        CodepointMap {
            range: ['o' as u21, 'o' as u21],
            replacement: Replacement::Codepoint('y' as u21),
        },
    ];
    let mut o = Options::plain();
    o.codepoint_map = Some(map);
    let formatter: PageFormatter = PageFormatter::init(page, o);
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"helly");
    t.deinit(Global);
}

// test "Page codepoint_map replace with string"
#[test]
fn page_codepoint_map_replace_with_string() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    // Replace 'o' with a multi-byte string
    let map = vec![CodepointMap {
        range: ['o' as u21, 'o' as u21],
        replacement: Replacement::String(b"XYZ".to_vec()),
    }];
    let mut o = Options::plain();
    o.codepoint_map = Some(map);
    let mut formatter: PageFormatter = PageFormatter::init(page, o);
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hellXYZ");

    // Verify point map - string replacements should all map to the original cell
    assert_eq!(output.len(), point_map.len());
    // "hello" -> "hellXYZ"
    // h e l l o
    // 0 1 2 3 4
    assert_eq!(coord(0, 0), point_map[0]); // h
    assert_eq!(coord(1, 0), point_map[1]); // e
    assert_eq!(coord(2, 0), point_map[2]); // l
    assert_eq!(coord(3, 0), point_map[3]); // l
                                           // All bytes of the replacement string "XYZ" should point to position 4 (where 'o' was)
    assert_eq!(coord(4, 0), point_map[4]); // X
    assert_eq!(coord(4, 0), point_map[5]); // Y
    assert_eq!(coord(4, 0), point_map[6]); // Z
    t.deinit(Global);
}

// test "Page codepoint_map range replacement"
#[test]
fn page_codepoint_map_range_replacement() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"abcdefg");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    // Replace 'b' through 'e' with 'X'
    let map = vec![CodepointMap {
        range: ['b' as u21, 'e' as u21],
        replacement: Replacement::Codepoint('X' as u21),
    }];
    let mut o = Options::plain();
    o.codepoint_map = Some(map);
    let formatter: PageFormatter = PageFormatter::init(page, o);
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"aXXXXfg");
    t.deinit(Global);
}

// test "Page codepoint_map multiple ranges"
#[test]
fn page_codepoint_map_multiple_ranges() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    // Replace 'a'-'m' with 'A' and 'n'-'z' with 'Z'
    let map = vec![
        CodepointMap {
            range: ['a' as u21, 'm' as u21],
            replacement: Replacement::Codepoint('A' as u21),
        },
        CodepointMap {
            range: ['n' as u21, 'z' as u21],
            replacement: Replacement::Codepoint('Z' as u21),
        },
    ];
    let mut o = Options::plain();
    o.codepoint_map = Some(map);
    let formatter: PageFormatter = PageFormatter::init(page, o);
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    // h e l l o   w o r l d
    // A A A A Z   Z Z Z A A
    assert_eq!(output, b"AAAAZ ZZZAA");
    t.deinit(Global);
}

// test "Page codepoint_map unicode replacement"
#[test]
fn page_codepoint_map_unicode_replacement() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice("hello ⚡ world".as_bytes());
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    // Replace lightning bolt with fire emoji
    let map = vec![CodepointMap {
        range: ['⚡' as u21, '⚡' as u21],
        replacement: Replacement::String("🔥".as_bytes().to_vec()),
    }];
    let mut o = Options::plain();
    o.codepoint_map = Some(map);
    let mut formatter: PageFormatter = PageFormatter::init(page, o);
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, "hello 🔥 world".as_bytes());

    // Verify point map
    assert_eq!(output.len(), point_map.len());
    // "hello ⚡ world"
    // h e l l o   ⚡   w o r l  d
    // 0 1 2 3 4 5 6   8 9 10 11 12
    // Note: ⚡ is a wide character occupying cells 6-7
    for i in 0..6 {
        assert_eq!(coord(i as size::CellCountInt, 0), point_map[i]);
    }
    // 🔥 is 4 UTF-8 bytes, all should map to cell 6 (where ⚡ was)
    let fire_start = 6; // "hello " is 6 bytes
    for i in 0..4 {
        assert_eq!(coord(6, 0), point_map[fire_start + i]);
    }
    // " world" follows
    let world_start = fire_start + 4;
    for i in 0..6 {
        assert_eq!(
            coord((8 + i) as size::CellCountInt, 0),
            point_map[world_start + i]
        );
    }
    t.deinit(Global);
}

// test "Page codepoint_map with styled formats"
#[test]
fn page_codepoint_map_with_styled_formats() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(10, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[31mred text\x1b[0m");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    // Replace 'e' with 'X' in styled text
    let map = vec![CodepointMap {
        range: ['e' as u21, 'e' as u21],
        replacement: Replacement::Codepoint('X' as u21),
    }];
    let mut o = Options::vt();
    o.codepoint_map = Some(map);
    let formatter: PageFormatter = PageFormatter::init(page, o);
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    // Should preserve styles while replacing text
    // "red text" becomes "rXd tXxt"
    // VT format uses \x1b[38;5;1m for palette color 1
    assert_eq!(output, b"\x1b[0m\x1b[38;5;1mrXd tXxt\x1b[0m");
    t.deinit(Global);
}

// test "Page codepoint_map empty map"
#[test]
fn page_codepoint_map_empty_map() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello world");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    // Empty map should not change anything
    let mut o = Options::plain();
    o.codepoint_map = Some(Vec::new());
    let formatter: PageFormatter = PageFormatter::init(page, o);
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello world");
    t.deinit(Global);
}

// Regression test for S151-F1: `codepoint_map` set on a `ScreenFormatter`
// (the shape `Surface::copySelectionToClipboards` uses for the
// `clipboard-codepoint-map` config option) must survive the
// ScreenFormatter -> PageListFormatter -> PageFormatter delegation chain
// and actually apply the replacement. Every hop in the reference passes
// `self.opts` by value (src/terminal/formatter.zig:486,675,897); a
// Rust `copy_options` helper that drops the map on any *content* hop
// silently makes the config option inert.
#[test]
fn codepoint_map_survives_the_screen_formatter_delegation() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"ab");
    }
    // map 'a' (0x61) -> 'Z' (0x5A), the same shape config uses.
    let map = vec![CodepointMap {
        range: ['a' as u21, 'a' as u21],
        replacement: Replacement::Codepoint('Z' as u21),
    }];
    let mut o = Options::plain();
    o.codepoint_map = Some(map);
    let mut formatter: ScreenFormatter = ScreenFormatter::init(t.screens.active, o);
    formatter.content = Content::Selection(None);
    formatter.format(&mut builder).unwrap();
    let output = String::from_utf8_lossy(&builder);
    assert_eq!(
        output.trim_end(),
        "Zb",
        "codepoint_map was dropped on the way down to PageFormatter"
    );
    t.deinit(Global);
}

// test "Page VT background color on trailing blank cells"
#[test]
fn page_vt_background_color_on_trailing_blank_cells() {
    // This test reproduces a bug where trailing cells with background color
    // but no text are emitted as plain spaces without SGR sequences.
    // This causes TUIs like htop to lose background colors on rehydration.
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(20, 5)).unwrap();
    {
        let mut s = t.vt_stream();
        // Simulate a TUI row: "CPU:" with text, then trailing cells with red background
        // to end of line (no text after the colored region).
        // \x1b[41m sets red background, then EL fills rest of row with that bg.
        s.next_slice(b"CPU:\x1b[41m\x1b[K");
        // Reset colors and move to next line with different content
        s.next_slice(b"\x1b[0m\r\nline2");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::vt());
    formatter.opts.trim = false; // Don't trim so we can see the trailing behavior
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();

    // The output should preserve the red background SGR for trailing cells on line 1.
    // Bug: the first row outputs "CPU:\r\n" only - losing the background color fill.
    // The red background should appear BEFORE the newline, not after.

    // Find position of CRLF
    let crlf_pos = match index_of(output, b"\r\n") {
        Some(p) => p,
        // No CRLF found, fail the test
        None => panic!("TestUnexpectedResult"),
    };

    // Check that red background (48;5;1) appears BEFORE the newline (on line 1)
    let line1 = &output[0..crlf_pos];
    let has_red_bg_line1 =
        index_of(line1, b"\x1b[41m").is_some() || index_of(line1, b"\x1b[48;5;1m").is_some();

    // This should be true but currently fails due to the bug
    assert!(has_red_bg_line1);
    t.deinit(Global);
}

// test "Page HTML with hyperlinks"
#[test]
fn page_html_with_hyperlinks() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Start a hyperlink, write some text, end it
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]8;;https://example.com\x1b\\link text\x1b]8;;\x1b\\ normal");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(page, Options::html());
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(
        output,
        concat!(
            "<div style=\"font-family: monospace; white-space: pre;\">",
            "<a href=\"https://example.com\">link text</a> normal",
            "</div>"
        )
        .as_bytes()
    );
    t.deinit(Global);
}

// test "Page HTML with multiple hyperlinks"
#[test]
fn page_html_with_multiple_hyperlinks() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        // Two different hyperlinks
        s.next_slice(b"\x1b]8;;https://first.com\x1b\\first\x1b]8;;\x1b\\ ");
        s.next_slice(b"\x1b]8;;https://second.com\x1b\\second\x1b]8;;\x1b\\");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(page, Options::html());
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(
        output,
        concat!(
            "<div style=\"font-family: monospace; white-space: pre;\">",
            "<a href=\"https://first.com\">first</a>",
            " ",
            "<a href=\"https://second.com\">second</a>",
            "</div>"
        )
        .as_bytes()
    );
    t.deinit(Global);
}

// test "Page HTML with hyperlink escaping"
#[test]
fn page_html_with_hyperlink_escaping() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // URL with special characters that need escaping
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]8;;https://example.com?a=1&b=2\x1b\\link\x1b]8;;\x1b\\");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(page, Options::html());
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(
        output,
        concat!(
            "<div style=\"font-family: monospace; white-space: pre;\">",
            "<a href=\"https://example.com?a=1&amp;b=2\">link</a>",
            "</div>"
        )
        .as_bytes()
    );
    t.deinit(Global);
}

// test "Page HTML with styled hyperlink"
#[test]
fn page_html_with_styled_hyperlink() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Bold hyperlink
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]8;;https://example.com\x1b\\\x1b[1mbold link\x1b[0m\x1b]8;;\x1b\\");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(page, Options::html());
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(
        output,
        concat!(
            "<div style=\"font-family: monospace; white-space: pre;\">",
            "<div style=\"display: inline;font-weight: bold;\">",
            "<a href=\"https://example.com\">bold link</div></a>",
            "</div>"
        )
        .as_bytes()
    );
    t.deinit(Global);
}

// test "Page HTML hyperlink closes style before anchor"
#[test]
fn page_html_hyperlink_closes_style_before_anchor() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Styled hyperlink followed by plain text
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]8;;https://example.com\x1b\\\x1b[1mbold\x1b[0m plain");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let formatter: PageFormatter = PageFormatter::init(page, Options::html());
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(
        output,
        concat!(
            "<div style=\"font-family: monospace; white-space: pre;\">",
            "<div style=\"display: inline;font-weight: bold;\">",
            "<a href=\"https://example.com\">bold</div> plain</a>",
            "</div>"
        )
        .as_bytes()
    );
    t.deinit(Global);
}

// test "Page HTML hyperlink point map maps closing to previous cell"
#[test]
fn page_html_hyperlink_point_map_maps_closing_to_previous_cell() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]8;;https://example.com\x1b\\link\x1b]8;;\x1b\\ normal");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let page = unsafe { last_page(pages) };
    let mut formatter: PageFormatter = PageFormatter::init(page, Options::html());
    let mut point_map: crate::alloc::AVec<Coordinate> = crate::alloc::AVec::new_in(Global);
    formatter.point_map = Some(PointMap {
        alloc: Global,
        map: &mut point_map,
        base: 0,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    let expected_output = concat!(
        "<div style=\"font-family: monospace; white-space: pre;\">",
        "<a href=\"https://example.com\">link</a> normal",
        "</div>"
    );
    assert_eq!(output, expected_output.as_bytes());
    assert_eq!(expected_output.len(), point_map.len());

    // The </a> closing tag bytes should all map to the last cell of the link
    let closing_idx = index_of(expected_output.as_bytes(), b"</a>").unwrap();
    let expected_coord = point_map[closing_idx - 1];
    for i in closing_idx..closing_idx + "</a>".len() {
        assert_eq!(expected_coord, point_map[i]);
    }
    t.deinit(Global);
}

// -----------------------------------------------------------------------
// Parity tail S7-8 (N330): the pin map's two lists grow through the pin
// map's `alloc` (`m.map.setNode(m.alloc, ..)`, `m.map.append(m.alloc, ..)`,
// and the page formatter's `PointMap{ .alloc = m.alloc, .map =
// &m.map.points }`), with Zig's `growCapacity`, and a failed growth is
// `error.WriteFailed`.
// -----------------------------------------------------------------------

/// Format the active screen of `t` through a `TerminalFormatter` with a
/// pin map over `alloc`.
fn format_with_pin_map(
    t: &Terminal,
    opts: Options,
    alloc: crate::alloc::VtAllocator,
    pin_map: &mut PinMapMap,
) -> (io::Result<()>, Vec<u8>) {
    let mut builder: Vec<u8> = Vec::new();
    let mut formatter: TerminalFormatter = TerminalFormatter::init(t, opts);
    formatter.pin_map = Some(PinMap {
        alloc,
        map: pin_map,
    });
    let r = formatter.format(&mut builder);
    (r, builder)
}

#[test]
fn pin_map_plain_routes_through_the_pin_maps_allocator() {
    use crate::alloc::testing::{with_handle, CountingAllocator};
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\nworld");
    }
    let counting = CountingAllocator::over_heap();
    let c = counting.allocator();
    with_handle(&c, |alloc| {
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let (r, output) = format_with_pin_map(&t, Options::plain(), alloc, &mut pin_map);
        r.unwrap();
        assert_eq!(b"hello\nworld", output.as_slice());
        assert_eq!(output.len(), pin_map.count());
        // Both lists are on the pin map's allocator.
        assert_eq!(alloc, *pin_map.points.allocator());
        assert_eq!(alloc, *pin_map.nodes.allocator());
        // One node run: `growCapacity(1)` with `init_capacity` 128 / 16.
        assert_eq!(1, pin_map.nodes.len());
        assert_eq!(1 + 8, pin_map.nodes.capacity());
        // The points list: the page formatter's first append is one
        // cell (`growCapacity(1)` with `init_capacity` 128 / 8 = 17),
        // which the 11 output bytes fit.
        assert_eq!(1 + 16, pin_map.points.capacity());
        assert_eq!(2, counting.alloc_calls.get());
        assert_eq!(0, counting.remap_calls.get());
        assert_eq!(0, counting.free_calls.get());
        assert_eq!(9 * 16 + 17 * 8, counting.live_bytes.get());
        pin_map.deinit(alloc);
        assert_eq!(2, counting.free_calls.get());
        assert_eq!(0, counting.live_bytes.get());
    });
    t.deinit(Global);
}

#[test]
fn pin_map_extras_route_through_the_pin_maps_allocator() {
    use crate::alloc::testing::{with_handle, CountingAllocator};
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]4;0;rgb:12/34/56\x1b\\");
        s.next_slice(b"test");
    }
    let counting = CountingAllocator::over_heap();
    let c = counting.allocator();
    with_handle(&c, |alloc| {
        let mut pin_map: PinMapMap = PinMapMap::EMPTY;
        let (r, output) = format_with_pin_map(&t, Options::vt(), alloc, &mut pin_map);
        r.unwrap();
        assert_eq!(output.len(), pin_map.count());
        // The palette bytes (the first `m.map.append`) come first: the
        // points list's first reservation is `growCapacity(P)` for those
        // P bytes, on the pin map's allocator.
        assert_eq!(alloc, *pin_map.points.allocator());
        assert_eq!(alloc, *pin_map.nodes.allocator());
        // 5522 palette bytes, then `test` and its trailing reset:
        // growCapacity(5522) = 5522 + 2761 + 16 = 8299 holds them all.
        assert_eq!(5530, output.len());
        assert_eq!(1, pin_map.nodes.len());
        assert_eq!(9, pin_map.nodes.capacity());
        assert_eq!(8299, pin_map.points.capacity());
        assert_eq!(2, counting.alloc_calls.get());
        assert_eq!(0, counting.remap_calls.get());
        assert_eq!(0, counting.free_calls.get());
        assert_eq!(9 * 16 + 8299 * 8, counting.live_bytes.get());
        pin_map.deinit(alloc);
        assert_eq!(0, counting.live_bytes.get());
    });
    t.deinit(Global);
}

#[test]
fn pin_map_growth_failure_is_write_failed() {
    use crate::alloc::testing::{with_handle, Config, FailingAllocator};
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    // Every growth index fails in turn: the format reports the
    // reference's `error.WriteFailed` (`OutOfMemory` here), never aborts,
    // and the map frees what it holds.
    // Plain reaches the page formatter's `PointMap`; vt reaches the
    // palette extras' `m.map.append` first.
    for (fail_index, vt) in [(0, false), (1, false), (0, true), (1, true)] {
        let failing = FailingAllocator::over_heap(Config {
            fail_index,
            ..Config::default()
        });
        let c = failing.allocator();
        with_handle(&c, |alloc| {
            let mut pin_map: PinMapMap = PinMapMap::EMPTY;
            let o = if vt { Options::vt() } else { Options::plain() };
            let (r, _) = format_with_pin_map(&t, o, alloc, &mut pin_map);
            let err = r.expect_err("the pin map's growth fails");
            assert_eq!(io::ErrorKind::OutOfMemory, err.kind());
            assert!(failing.has_induced_failure.get());
            assert_eq!(fail_index, failing.allocations.get());
            pin_map.deinit(alloc);
            assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
        });
    }
    t.deinit(Global);
}

#[test]
fn selection_string_map_routes_through_the_given_allocator() {
    use crate::alloc::testing::{with_handle, Config, CountingAllocator, FailingAllocator};
    let mut t = Terminal::init(Global, opts(10, 5)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello");
    }
    let screen = unsafe { &mut *t.screens.active };
    let sel = crate::selection::Selection::init(
        screen
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 0, y: 0 }))
            .unwrap(),
        screen
            .pages
            .pin(point::Point::Active(point::Coordinate { x: 4, y: 0 }))
            .unwrap(),
        false,
    );
    let counting = CountingAllocator::over_heap();
    let c = counting.allocator();
    with_handle(&c, |alloc| {
        let map = screen
            .selection_string_map(alloc, crate::screen::SelectionString { sel, trim: true })
            .unwrap();
        assert_eq!(b"hello", map.string.as_slice());
        assert_eq!(5, map.map.count());
        assert_eq!(alloc, *map.map.points.allocator());
        assert_eq!(alloc, *map.map.nodes.allocator());
        assert_eq!(2, counting.alloc_calls.get());
        map.deinit(alloc);
        assert_eq!(0, counting.live_bytes.get());
    });
    // A failing allocator: `selectionStringMap` returns OutOfMemory.
    let failing = FailingAllocator::over_heap(Config {
        fail_index: 0,
        ..Config::default()
    });
    let c = failing.allocator();
    with_handle(&c, |alloc| {
        let r =
            screen.selection_string_map(alloc, crate::screen::SelectionString { sel, trim: true });
        assert!(r.is_err());
        assert!(failing.has_induced_failure.get());
    });
    t.deinit(Global);
}
