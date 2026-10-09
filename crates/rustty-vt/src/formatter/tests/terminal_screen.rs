//! formatter tests: `TerminalFormatter` and `ScreenFormatter` (selections,
//! pin maps, cursor/style/hyperlink/protection/charset/mode extras).

use super::*;

// test "TerminalFormatter plain no selection"
#[test]
fn terminalformatter_plain_no_selection() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\nworld");
    }
    let t_ptr: *const Terminal = &t;
    let formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::plain());
    formatter.format(&mut builder).unwrap();
    assert_eq!(builder.as_slice(), b"hello\nworld");
    t.deinit(Global);
}

// test "TerminalFormatter vt with palette"
#[test]
fn terminalformatter_vt_with_palette() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        // Modify some palette colors using VT sequences
        s.next_slice(b"\x1b]4;0;rgb:12/34/56\x1b\\");
        s.next_slice(b"\x1b]4;1;rgb:ab/cd/ef\x1b\\");
        s.next_slice(b"\x1b]4;255;rgb:ff/00/ff\x1b\\");
        s.next_slice(b"test");
    }
    let t_ptr: *const Terminal = &t;
    let formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
    formatter.format(&mut builder).unwrap();
    let output = builder.clone();

    // Create a second terminal and apply the output
    let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s2 = t2.vt_stream();
        s2.next_slice(output.as_slice());
    }

    // Verify the palettes match
    assert_eq!(t.colors.palette.current[0], t2.colors.palette.current[0]);
    assert_eq!(t.colors.palette.current[1], t2.colors.palette.current[1]);
    assert_eq!(
        t.colors.palette.current[255],
        t2.colors.palette.current[255]
    );
    t.deinit(Global);
    t2.deinit(Global);
}

// test "TerminalFormatter with selection"
#[test]
fn terminalformatter_with_selection() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"line1\r\nline2\r\nline3");
    }
    let pages = unsafe { &(*t.screens.active).pages };
    let tl = pages.pin(crate::point::Point::Active(coord(0, 1))).unwrap();
    let br = pages.pin(crate::point::Point::Active(coord(4, 1))).unwrap();
    let t_ptr: *const Terminal = &t;
    let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::plain());
    formatter.content = Content::Selection(Some(Selection::init(tl, br, false)));
    formatter.format(&mut builder).unwrap();
    assert_eq!(builder.as_slice(), b"line2");
    t.deinit(Global);
}

// test "TerminalFormatter plain with pin_map"
#[test]
fn terminalformatter_plain_with_pin_map() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello, world");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let t_ptr: *const Terminal = &t;
    let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::plain());
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello, world");

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    for i in 0..output.len() {
        assert_eq!(
            pin(node, i as size::CellCountInt, 0),
            pin_map.get(i).unwrap()
        );
    }
    t.deinit(Global);
}

// test "TerminalFormatter plain multiline with pin_map"
#[test]
fn terminalformatter_plain_multiline_with_pin_map() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\nworld");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let t_ptr: *const Terminal = &t;
    let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::plain());
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello\nworld");

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    // "hello" (5 chars)
    for i in 0..5 {
        assert_eq!(node, pin_map.get(i).unwrap().node);
        assert_eq!(i as size::CellCountInt, pin_map.get(i).unwrap().x);
        assert_eq!(0, pin_map.get(i).unwrap().y);
    }
    // "\n" maps to end of first line
    assert_eq!(node, pin_map.get(5).unwrap().node);
    // "world" (5 chars)
    for i in 0..5 {
        let idx = 6 + i;
        assert_eq!(node, pin_map.get(idx).unwrap().node);
        assert_eq!(i as size::CellCountInt, pin_map.get(idx).unwrap().x);
        assert_eq!(1, pin_map.get(idx).unwrap().y);
    }
    t.deinit(Global);
}

// test "TerminalFormatter vt with palette and pin_map"
#[test]
fn terminalformatter_vt_with_palette_and_pin_map() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        // Modify some palette colors using VT sequences
        s.next_slice(b"\x1b]4;0;rgb:12/34/56\x1b\\");
        s.next_slice(b"test");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let t_ptr: *const Terminal = &t;
    let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();

    // Verify pin map - palette bytes should be mapped to top left
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    for i in 0..output.len() {
        assert_eq!(node, pin_map.get(i).unwrap().node);
    }
    t.deinit(Global);
}

// test "TerminalFormatter with selection and pin_map"
#[test]
fn terminalformatter_with_selection_and_pin_map() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"line1\r\nline2\r\nline3");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let pages = unsafe { &(*t.screens.active).pages };
    let tl = pages.pin(crate::point::Point::Active(coord(0, 1))).unwrap();
    let br = pages.pin(crate::point::Point::Active(coord(4, 1))).unwrap();
    let t_ptr: *const Terminal = &t;
    let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::plain());
    formatter.content = Content::Selection(Some(Selection::init(tl, br, false)));
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"line2");

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    // "line2" (5 chars) from row 1
    for i in 0..5 {
        assert_eq!(node, pin_map.get(i).unwrap().node);
        assert_eq!(i as size::CellCountInt, pin_map.get(i).unwrap().x);
        assert_eq!(1, pin_map.get(i).unwrap().y);
    }
    t.deinit(Global);
}

// test "Screen plain single line"
#[test]
fn screen_plain_single_line() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello, world");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let screen_ptr: *const Screen = t.screens.active;
    let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::plain());
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello, world");

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    for i in 0..output.len() {
        assert_eq!(
            pin(node, i as size::CellCountInt, 0),
            pin_map.get(i).unwrap()
        );
    }
    t.deinit(Global);
}

// test "Screen plain multiline"
#[test]
fn screen_plain_multiline() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\nworld");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let screen_ptr: *const Screen = t.screens.active;
    let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::plain());
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"hello\nworld");

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    // "hello" (5 chars)
    for i in 0..5 {
        assert_eq!(node, pin_map.get(i).unwrap().node);
        assert_eq!(i as size::CellCountInt, pin_map.get(i).unwrap().x);
        assert_eq!(0, pin_map.get(i).unwrap().y);
    }
    // "\n" maps to end of first line
    assert_eq!(node, pin_map.get(5).unwrap().node);
    // "world" (5 chars)
    for i in 0..5 {
        let idx = 6 + i;
        assert_eq!(node, pin_map.get(idx).unwrap().node);
        assert_eq!(i as size::CellCountInt, pin_map.get(idx).unwrap().x);
        assert_eq!(1, pin_map.get(idx).unwrap().y);
    }
    t.deinit(Global);
}

// test "Screen plain with selection"
#[test]
fn screen_plain_with_selection() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"line1\r\nline2\r\nline3");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let pages = unsafe { &(*t.screens.active).pages };
    let tl = pages.pin(crate::point::Point::Active(coord(0, 1))).unwrap();
    let br = pages.pin(crate::point::Point::Active(coord(4, 1))).unwrap();
    let screen_ptr: *const Screen = t.screens.active;
    let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::plain());
    formatter.content = Content::Selection(Some(Selection::init(tl, br, false)));
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.as_slice();
    assert_eq!(output, b"line2");

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    // "line2" (5 chars) from row 1
    for i in 0..5 {
        assert_eq!(node, pin_map.get(i).unwrap().node);
        assert_eq!(i as size::CellCountInt, pin_map.get(i).unwrap().x);
        assert_eq!(1, pin_map.get(i).unwrap().y);
    }
    t.deinit(Global);
}

// test "Screen vt with cursor position"
#[test]
fn screen_vt_with_cursor_position() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Position cursor at a specific location
    {
        let mut s = t.vt_stream();
        s.next_slice(b"hello\r\nworld");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let screen_ptr: *const Screen = t.screens.active;
    let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::vt());
    formatter.extra.cursor = true;
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.clone();

    // Create a second terminal and apply the output
    let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s2 = t2.vt_stream();
        s2.next_slice(output.as_slice());
    }

    // Verify cursor positions match
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, unsafe {
        (*t2.screens.active).cursor.x
    });
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, unsafe {
        (*t2.screens.active).cursor.y
    });

    // Verify pin map - the extras should be mapped to the last pin
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    let content_len = b"hello\r\nworld".len();
    // Content bytes map to their positions
    for i in 0..content_len {
        assert_eq!(node, pin_map.get(i).unwrap().node);
    }
    // Extra bytes (cursor position) map to last content pin
    for i in content_len..output.len() {
        assert_eq!(node, pin_map.get(i).unwrap().node);
    }
    t.deinit(Global);
    t2.deinit(Global);
}

// test "Terminal vt cursor preserves pending wrap"
#[test]
fn terminal_vt_cursor_preserves_pending_wrap() {
    let mut builder: Vec<u8> = Vec::new();

    let mut source = Terminal::init(Global, opts(4, 2)).unwrap();
    {
        let mut source_stream = source.vt_stream();
        source_stream.next_slice(b"abcd");
    }
    assert!(unsafe { (*source.screens.active).cursor.pending_wrap });

    let mut pin_map: PinMapMap = PinMapMap::EMPTY;

    let source_ptr: *const Terminal = &source;
    let mut formatter: TerminalFormatter = TerminalFormatter::init(source_ptr, Options::vt());
    formatter.extra = TerminalExtra::NONE;
    formatter.extra.screen.cursor = true;
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    assert_eq!(builder.len(), pin_map.count());

    let mut target = Terminal::init(Global, opts(4, 2)).unwrap();
    {
        let mut target_stream = target.vt_stream();
        target_stream.next_slice(builder.as_slice());
    }

    assert_eq!(
        unsafe { (*source.screens.active).cursor.pending_wrap },
        unsafe { (*target.screens.active).cursor.pending_wrap },
    );

    {
        let mut source_stream = source.vt_stream();
        source_stream.next_slice(b"X");
    }
    {
        let mut target_stream = target.vt_stream();
        target_stream.next_slice(b"X");
    }
    let source_contents = unsafe { &(*source.screens.active) }
        .dump_string_alloc(
            Global,
            crate::point::Point::Screen(crate::point::Coordinate::default()),
        )
        .unwrap();
    let target_contents = unsafe { &(*target.screens.active) }
        .dump_string_alloc(
            Global,
            crate::point::Point::Screen(crate::point::Coordinate::default()),
        )
        .unwrap();
    assert_eq!(source_contents, target_contents);
    source.deinit(Global);
    target.deinit(Global);
}

// test "Screen vt with style"
#[test]
fn screen_vt_with_style() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Set some style attributes
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[1;31mhello");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let screen_ptr: *const Screen = t.screens.active;
    let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::vt());
    formatter.extra.style = true;
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.clone();

    // Create a second terminal and apply the output
    let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s2 = t2.vt_stream();
        s2.next_slice(output.as_slice());
    }

    // Verify styles match
    assert!(unsafe { (*t.screens.active).cursor.style }
        .eql(&unsafe { (*t2.screens.active).cursor.style }));

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    for i in 0..output.len() {
        assert_eq!(node, pin_map.get(i).unwrap().node);
    }
    t.deinit(Global);
    t2.deinit(Global);
}

// test "Screen vt with hyperlink"
#[test]
fn screen_vt_with_hyperlink() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Set a hyperlink
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]8;;http://example.com\x1b\\hello");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let screen_ptr: *const Screen = t.screens.active;
    let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::vt());
    formatter.extra.hyperlink = true;
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.clone();

    // Create a second terminal and apply the output
    let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s2 = t2.vt_stream();
        s2.next_slice(output.as_slice());
    }

    // Verify hyperlinks match
    let has_link1 = unsafe { (*t.screens.active).cursor.hyperlink }.is_some();
    let has_link2 = unsafe { (*t2.screens.active).cursor.hyperlink }.is_some();
    assert_eq!(has_link1, has_link2);

    if has_link1 {
        let link1 = unsafe { &*(*t.screens.active).cursor.hyperlink.unwrap() };
        let link2 = unsafe { &*(*t2.screens.active).cursor.hyperlink.unwrap() };
        assert_eq!(link1.uri, link2.uri);
    }

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    for i in 0..output.len() {
        assert_eq!(node, pin_map.get(i).unwrap().node);
    }
    t.deinit(Global);
    t2.deinit(Global);
}

// test "Screen vt with protection"
#[test]
fn screen_vt_with_protection() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Enable protection mode
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[1\"qhello");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let screen_ptr: *const Screen = t.screens.active;
    let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::vt());
    formatter.extra.protection = true;
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.clone();

    // Create a second terminal and apply the output
    let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s2 = t2.vt_stream();
        s2.next_slice(output.as_slice());
    }

    // Verify protection state matches
    assert_eq!(unsafe { (*t.screens.active).cursor.protected }, unsafe {
        (*t2.screens.active).cursor.protected
    });

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    for i in 0..output.len() {
        assert_eq!(node, pin_map.get(i).unwrap().node);
    }
    t.deinit(Global);
    t2.deinit(Global);
}

// test "Screen vt with kitty keyboard"
#[test]
fn screen_vt_with_kitty_keyboard() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Set kitty keyboard flags (disambiguate + report_events = 3)
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[=3;1uhello");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let screen_ptr: *const Screen = t.screens.active;
    let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::vt());
    formatter.extra.kitty_keyboard = true;
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.clone();

    // Create a second terminal and apply the output
    let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s2 = t2.vt_stream();
        s2.next_slice(output.as_slice());
    }

    // Verify kitty keyboard state matches
    let flags1 = unsafe { (*t.screens.active).kitty_keyboard.current() }.int();
    let flags2 = unsafe { (*t2.screens.active).kitty_keyboard.current() }.int();
    assert_eq!(flags1, flags2);

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    for i in 0..output.len() {
        assert_eq!(node, pin_map.get(i).unwrap().node);
    }
    t.deinit(Global);
    t2.deinit(Global);
}

// test "Screen vt with charsets"
#[test]
fn screen_vt_with_charsets() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Set G0 to DEC special and shift to G1
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b(0\x0ehello");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let screen_ptr: *const Screen = t.screens.active;
    let mut formatter: ScreenFormatter = ScreenFormatter::init(screen_ptr, Options::vt());
    formatter.extra.charsets = true;
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.clone();

    // Create a second terminal and apply the output
    let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s2 = t2.vt_stream();
        s2.next_slice(output.as_slice());
    }

    // Verify charset state matches
    assert_eq!(unsafe { (*t.screens.active).charset.gl }, unsafe {
        (*t2.screens.active).charset.gl
    });
    assert_eq!(unsafe { (*t.screens.active).charset.gr }, unsafe {
        (*t2.screens.active).charset.gr
    });
    assert_eq!(
        unsafe {
            (*t.screens.active)
                .charset
                .charsets
                .get(crate::charsets::Slots::G0)
        },
        unsafe {
            (*t2.screens.active)
                .charset
                .charsets
                .get(crate::charsets::Slots::G0)
        }
    );

    // Verify pin map
    assert_eq!(output.len(), pin_map.count());
    let node = unsafe { (*t.screens.active).pages.pages.first };
    for i in 0..output.len() {
        assert_eq!(node, pin_map.get(i).unwrap().node);
    }
    t.deinit(Global);
    t2.deinit(Global);
}

// test "Terminal vt with scrolling region"
#[test]
fn terminal_vt_with_scrolling_region() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Set scrolling region: top=5, bottom=20
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[6;21rhello");
    }
    let t_ptr: *const Terminal = &t;
    let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
    formatter.extra.scrolling_region = true;
    formatter.format(&mut builder).unwrap();
    let output = builder.clone();

    // Create a second terminal and apply the output
    let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s2 = t2.vt_stream();
        s2.next_slice(output.as_slice());
    }

    // Verify scrolling regions match
    assert_eq!(t.scrolling_region.top, t2.scrolling_region.top);
    assert_eq!(t.scrolling_region.bottom, t2.scrolling_region.bottom);
    assert_eq!(t.scrolling_region.left, t2.scrolling_region.left);
    assert_eq!(t.scrolling_region.right, t2.scrolling_region.right);
    t.deinit(Global);
    t2.deinit(Global);
}

// test "Terminal vt with cursor in origin mode"
#[test]
fn terminal_vt_with_cursor_in_origin_mode() {
    // Each stream sets a scrolling region and origin mode, then moves
    // the cursor within the region; the last one leaves a pending wrap
    // in the right column, which is restored by printing that cell again.
    // The left and right margins need mode 69.
    let streams: [&[u8]; 3] = [
        b"\x1b[3;8r\x1b[?6h\x1b[2;5H",
        b"\x1b[?69h\x1b[3;8r\x1b[4;15s\x1b[?6h\x1b[2;3H",
        b"\x1b[3;8r\x1b[?6h\x1b[2;20Hx",
    ];
    for stream in streams {
        let mut builder: Vec<u8> = Vec::new();

        let mut t = Terminal::init(Global, opts(20, 10)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(stream);
        }

        let t_ptr: *const Terminal = &t;
        let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
        formatter.extra = TerminalExtra::ALL;
        formatter.format(&mut builder).unwrap();

        let mut t2 = Terminal::init(Global, opts(20, 10)).unwrap();
        {
            let mut s2 = t2.vt_stream();
            s2.next_slice(builder.as_slice());
        }

        assert!(t2.modes.get(crate::modes::Mode::Origin));
        let (c1, c2) = unsafe { (&(*t.screens.active).cursor, &(*t2.screens.active).cursor) };
        assert_eq!(c1.x, c2.x);
        assert_eq!(c1.y, c2.y);
        assert_eq!(c1.pending_wrap, c2.pending_wrap);
        t.deinit(Global);
        t2.deinit(Global);
    }
}

// test "Terminal vt with cursor in origin mode without the region"
#[test]
fn terminal_vt_with_cursor_in_origin_mode_without_the_region() {
    let mut builder: Vec<u8> = Vec::new();

    let mut t = Terminal::init(Global, opts(20, 10)).unwrap();
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b[3;8r\x1b[?6h\x1b[2;5H");
    }

    // Without the scrolling region in the output, the replaying terminal
    // keeps full-screen margins, so the position stays absolute.
    let t_ptr: *const Terminal = &t;
    let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
    formatter.extra = TerminalExtra::ALL;
    formatter.extra.scrolling_region = false;
    formatter.format(&mut builder).unwrap();
    assert!(builder.windows(6).any(|w| w == b"\x1b[4;5H"));
    t.deinit(Global);
}

// test "Terminal vt with modes"
#[test]
fn terminal_vt_with_modes() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        // Enable some modes that differ from defaults
        s.next_slice(b"\x1b[?2004h"); // Bracketed paste
        s.next_slice(b"\x1b[?1000h"); // Mouse event normal
        s.next_slice(b"\x1b[?7l"); // Disable wraparound (default is true)
        s.next_slice(b"hello");
    }
    let t_ptr: *const Terminal = &t;
    let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
    formatter.extra.modes = true;
    formatter.format(&mut builder).unwrap();
    let output = builder.clone();

    // Create a second terminal and apply the output
    let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s2 = t2.vt_stream();
        s2.next_slice(output.as_slice());
    }

    // Verify modes match
    assert_eq!(
        t.modes.get(crate::modes::Mode::BracketedPaste),
        t2.modes.get(crate::modes::Mode::BracketedPaste)
    );
    assert_eq!(
        t.modes.get(crate::modes::Mode::MouseEventNormal),
        t2.modes.get(crate::modes::Mode::MouseEventNormal)
    );
    assert_eq!(
        t.modes.get(crate::modes::Mode::Wraparound),
        t2.modes.get(crate::modes::Mode::Wraparound)
    );
    t.deinit(Global);
    t2.deinit(Global);
}

// test "Terminal vt with tabstops"
#[test]
fn terminal_vt_with_tabstops() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        // Clear all tabs and set custom tabstops
        s.next_slice(b"\x1b[3g"); // Clear all tabs
        s.next_slice(b"\x1b[5G\x1bH"); // Set tab at column 5
        s.next_slice(b"\x1b[15G\x1bH"); // Set tab at column 15
        s.next_slice(b"\x1b[30G\x1bH"); // Set tab at column 30
        s.next_slice(b"\x1b[Hhello");
    }
    let mut pin_map: PinMapMap = PinMapMap::EMPTY;
    let t_ptr: *const Terminal = &t;
    let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
    formatter.extra.tabstops = true;
    formatter.extra.screen.cursor = true;
    formatter.pin_map = Some(PinMap {
        alloc: Global,
        map: &mut pin_map,
    });
    formatter.format(&mut builder).unwrap();
    let output = builder.clone();

    // Create a second terminal and apply the output
    let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s2 = t2.vt_stream();
        s2.next_slice(output.as_slice());
    }

    // Verify tabstops match (columns are 0-indexed in the API)
    assert_eq!(t.tabstops.get(4), t2.tabstops.get(4));
    assert_eq!(t.tabstops.get(14), t2.tabstops.get(14));
    assert_eq!(t.tabstops.get(29), t2.tabstops.get(29));
    assert!(t2.tabstops.get(4)); // Column 5 (1-indexed)
    assert!(t2.tabstops.get(14)); // Column 15 (1-indexed)
    assert!(t2.tabstops.get(29)); // Column 30 (1-indexed)
    assert!(!t2.tabstops.get(8)); // Not a tab

    // Tabstop serialization must not offset the screen contents.
    for (col, expected) in b"hello".iter().enumerate() {
        let cell = unsafe { &(*t2.screens.active).pages }
            .get_cell(crate::point::Point::Screen(coord(
                col as size::CellCountInt,
                0,
            )))
            .unwrap();
        assert_eq!(*expected as u21, unsafe { (*cell.cell).codepoint() });
    }

    // Emitting tabstops moves the cursor to each configured column. When
    // cursor state is included, it must be restored afterwards.
    assert_eq!(unsafe { (*t.screens.active).cursor.x }, unsafe {
        (*t2.screens.active).cursor.x
    });
    assert_eq!(unsafe { (*t.screens.active).cursor.y }, unsafe {
        (*t2.screens.active).cursor.y
    });

    // Verify the reordered terminal state is still represented in the map.
    assert_eq!(output.len(), pin_map.count());
    t.deinit(Global);
    t2.deinit(Global);
}

// test "Terminal vt with keyboard modes"
#[test]
fn terminal_vt_with_keyboard_modes() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s = t.vt_stream();
        // Set modify other keys mode 2
        s.next_slice(b"\x1b[>4;2m");
        s.next_slice(b"hello");
    }
    let t_ptr: *const Terminal = &t;
    let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
    formatter.extra.keyboard = true;
    formatter.format(&mut builder).unwrap();
    let output = builder.clone();

    // Create a second terminal and apply the output
    let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s2 = t2.vt_stream();
        s2.next_slice(output.as_slice());
    }

    // Verify keyboard mode matches
    assert_eq!(t.flags.modify_other_keys_2, t2.flags.modify_other_keys_2);
    assert!(t2.flags.modify_other_keys_2);
    t.deinit(Global);
    t2.deinit(Global);
}

// test "Terminal vt with pwd"
#[test]
fn terminal_vt_with_pwd() {
    let mut builder: Vec<u8> = Vec::new();
    let mut t = Terminal::init(Global, opts(80, 24)).unwrap();
    // Set pwd using OSC 7
    {
        let mut s = t.vt_stream();
        s.next_slice(b"\x1b]7;file://host/home/user\x1b\\hello");
    }
    let t_ptr: *const Terminal = &t;
    let mut formatter: TerminalFormatter = TerminalFormatter::init(t_ptr, Options::vt());
    formatter.extra.pwd = true;
    formatter.format(&mut builder).unwrap();
    let output = builder.clone();

    // Create a second terminal and apply the output
    let mut t2 = Terminal::init(Global, opts(80, 24)).unwrap();
    {
        let mut s2 = t2.vt_stream();
        s2.next_slice(output.as_slice());
    }

    // Verify pwd matches
    assert_eq!(t.pwd.as_slice(), t2.pwd.as_slice());
    t.deinit(Global);
    t2.deinit(Global);
}
