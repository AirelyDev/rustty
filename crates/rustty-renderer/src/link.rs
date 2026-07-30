//! rustty-renderer link
//!
//! PORTS: ghostty src/renderer/link.zig
//!
//! PORT STATUS: ported (R1). The renderer "link" set — an `oni.Regex`-backed
//! match of `input.Link` regexes against the render-state viewport string,
//! producing the highlighted `CellSet` — with all 3 `test{}` blocks
//! (`renderCellMap`, `renderCellMap hover links`, `renderCellMap mods no match`).
//!
//! The previous Oniguruma deferral is RESOLVED: `oniguruma-sys` + the `oni` safe
//! wrapper (`rustty-oniguruma`) have landed, so `Set.fromConfig` compiles a real
//! `oni::Regex` and `renderCellMap` runs a real `regex.search` (see
//! .port/phase-2/DEFERRALS.md). Like `rustty-input`'s `Link` (which the reference
//! also imports oniguruma for unconditionally), `rustty-oniguruma` is a
//! NON-optional dependency here.
//
// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const oni = @import("oniguruma");
// const inputpkg = @import("../input.zig");
// const terminal = @import("../terminal/main.zig");
// const point = terminal.point;
// const Screen = terminal.Screen;
// const Terminal = terminal.Terminal;
//
// PORT NOTE: `oni` -> the `rustty-oniguruma` crate (lib name `oni`), referenced
// directly as `oni::*`. `inputpkg.Link`/`inputpkg.Link.Highlight` ->
// `rustty_input::link::{Link, Highlight}`; `inputpkg.Mods` ->
// `rustty_vt::input::key_mods::Mods`. `terminal.*` -> `rustty_vt::*`
// (`point`, `RenderState`, `CellSet`, `StringMap`). The reference's unused
// file-scope `Screen`/`Terminal` aliases are dropped (the tests reach
// `Terminal` by direct path). `Allocator` -> `allocator_api2::alloc::Global`,
// matching `RenderState::string`'s allocator parameter.
use allocator_api2::alloc::Global as Allocator;
use rustty_input::link::{Highlight, Link as InputLink};
use rustty_vt::input::key_mods::Mods;
use rustty_vt::point;
use rustty_vt::render::{CellSet, RenderState, StringError, StringMap};

// const log = std.log.scoped(.renderer_link);

/// The error set for the renderer link matching path.
//
// PORT NOTE: the reference `renderCellMap` returns `!void`, unifying the
// `RenderState.string` write/OOM error and the `oni.Regex.search` error; the
// Rust port collects those into this enum. `fromConfig`'s only fallible op is
// `oni.Regex.init`, so it returns `Result<Set, oni::Error>` directly.
#[derive(Debug)]
pub enum Error {
    Oni(oni::Error),
    String(StringError),
}

impl From<oni::Error> for Error {
    fn from(e: oni::Error) -> Self {
        Error::Oni(e)
    }
}

impl From<StringError> for Error {
    fn from(e: StringError) -> Self {
        Error::String(e)
    }
}

/// The link configuration needed for renderers.
// pub const Link = struct {
pub struct Link {
    /// The regular expression to match the link against.
    // regex: oni.Regex,
    pub regex: oni::Regex,

    /// The situations in which the link should be highlighted.
    // highlight: inputpkg.Link.Highlight,
    pub highlight: Highlight,
}

impl Link {
    // pub fn deinit(self: *Link) void {
    pub fn deinit(&mut self) {
        // self.regex.deinit();
        self.regex.deinit();
    }

    /// Returns true if this link's highlight condition matches the given mouse state.
    // fn active(
    //     self: *const Link,
    //     mouse_viewport: ?point.Coordinate,
    //     mouse_mods: inputpkg.Mods,
    // ) bool {
    fn active(&self, mouse_viewport: Option<point::Coordinate>, mouse_mods: Mods) -> bool {
        // return switch (self.highlight) {
        match self.highlight {
            // .always => true,
            Highlight::Always => true,
            // .always_mods => |v| mouse_mods.equal(v),
            Highlight::AlwaysMods(v) => mouse_mods.equal(v),
            // .hover => mouse_viewport != null,
            Highlight::Hover => mouse_viewport.is_some(),
            // .hover_mods => |v| mouse_viewport != null and mouse_mods.equal(v),
            Highlight::HoverMods(v) => mouse_viewport.is_some() && mouse_mods.equal(v),
        }
    }
}

/// A set of links. This provides a higher level API for renderers
/// to match against a viewport and determine if cells are part of
/// a link.
// pub const Set = struct {
pub struct Set {
    // links: []Link,
    pub links: Vec<Link>,
}

impl Set {
    /// Returns the slice of links from the configuration.
    // pub fn fromConfig(
    //     alloc: Allocator,
    //     config: []const inputpkg.Link,
    // ) !Set {
    pub fn from_config(alloc: Allocator, config: &[InputLink]) -> Result<Set, oni::Error> {
        // var links: std.ArrayList(Link) = .empty;
        // defer links.deinit(alloc);
        let _ = alloc;
        let mut links: Vec<Link> = Vec::new();

        // for (config) |link| {
        for link in config {
            // var regex = try link.oniRegex();
            // errdefer regex.deinit();
            //
            // PORT NOTE: the `errdefer regex.deinit()` guards the `append` below;
            // `Vec::push` is infallible (aborts on OOM), so it never fires. On an
            // earlier `oniRegex()` failure the already-built regexes leak their
            // `OnigRegex` handles, exactly as in the reference (whose `defer
            // links.deinit(alloc)` frees only the list backing, not each regex).
            let regex = link.oni_regex()?;
            // try links.append(alloc, .{ .regex = regex, .highlight = link.highlight });
            links.push(Link {
                regex,
                highlight: link.highlight,
            });
        }

        // return .{ .links = try links.toOwnedSlice(alloc) };
        Ok(Set { links })
    }

    // pub fn deinit(self: *Set, alloc: Allocator) void {
    pub fn deinit(&mut self, alloc: Allocator) {
        // for (self.links) |*link| link.deinit();
        for link in self.links.iter_mut() {
            link.deinit();
        }
        // alloc.free(self.links);
        let _ = alloc;
        self.links = Vec::new();
    }

    /// Fills matches with the matches from regex link matches.
    // pub fn renderCellMap(
    //     self: *const Set,
    //     alloc: Allocator,
    //     result: *terminal.RenderState.CellSet,
    //     render_state: *const terminal.RenderState,
    //     mouse_viewport: ?point.Coordinate,
    //     mouse_mods: inputpkg.Mods,
    // ) !void {
    //
    // PORT NOTE: the reference is `self: *const Set` but mutates each link's regex
    // through the slice-element pointer (Zig `const` is shallow). Rust's
    // `oni::Regex::search` is `&mut self`, so `render_cell_map` takes `&mut self`.
    pub fn render_cell_map(
        &mut self,
        alloc: Allocator,
        result: &mut CellSet,
        render_state: &RenderState,
        mouse_viewport: Option<point::Coordinate>,
        mouse_mods: Mods,
    ) -> Result<(), Error> {
        // Fast path, not very likely since we have default links.
        // if (self.links.len == 0) return;
        if self.links.is_empty() {
            return Ok(());
        }

        // Determine if any links are active before building the string and
        // byte-to-cell map. Those buffers scale with viewport size and this
        // function runs during frame updates, so avoid allocating them when
        // the current mouse/modifier state can't highlight any regex links.
        // for (self.links) |*link| {
        //     if (link.active(mouse_viewport, mouse_mods)) break;
        // } else return;
        //
        // PORT NOTE: Zig's `for ... else` runs the `else` only when the loop
        // completed WITHOUT breaking, i.e. "no link was active -> return". The
        // Rust spelling of that is a negated `any`.
        if !self
            .links
            .iter()
            .any(|link| link.active(mouse_viewport, mouse_mods))
        {
            return Ok(());
        }

        // Convert our render state to a string + byte map.
        // var builder: std.Io.Writer.Allocating = .init(alloc);
        // defer builder.deinit();
        // var map: terminal.RenderState.StringMap = .empty;
        // defer map.deinit(alloc);
        // try render_state.string(&builder.writer, .{ .alloc = alloc, .map = &map });
        let mut builder: Vec<u8> = Vec::new();
        let mut map: StringMap = StringMap::new();
        render_state.string(alloc, &mut builder, Some(&mut map))?;

        // const str = builder.writer.buffered();
        let str = &builder;

        // Go through each link and see if we have any matches.
        // for (self.links) |*link| {
        for link in self.links.iter_mut() {
            // if (!link.active(mouse_viewport, mouse_mods)) continue;
            //
            // (9c9cf3e82 collapsed the reference's inline `switch (link.highlight)`
            // — whose comment "we use a switch here instead of an if so that we
            // can get a compile error if any other conditions are added" now
            // applies to `Link.active` above, where the exhaustive match lives.)
            if !link.active(mouse_viewport, mouse_mods) {
                continue;
            }

            // var offset: usize = 0;
            let mut offset: usize = 0;
            // while (offset < str.len) {
            while offset < str.len() {
                // var region = link.regex.search(str[offset..], .{}) catch |err| switch (err) {
                //     error.Mismatch => break,
                //     else => return err,
                // };
                // defer region.deinit();
                let mut region = match link
                    .regex
                    .search(&str[offset..], oni::SearchOptions::default())
                {
                    Ok(region) => region,
                    Err(oni::Error::Mismatch) => break,
                    Err(err) => return Err(err.into()),
                };

                // We have a match!
                // const offset_start: usize = @intCast(region.starts()[0]);
                // const offset_end: usize = @intCast(region.ends()[0]);
                // const start = offset + offset_start;
                // const end = offset + offset_end;
                let offset_start: usize = region.starts()[0] as usize;
                let offset_end: usize = region.ends()[0] as usize;
                let start = offset + offset_start;
                let end = offset + offset_end;

                // Increment our offset by the number of bytes in the match.
                // We defer this so that we can return the match before
                // modifying the offset.
                // defer offset = end;
                //
                // PORT NOTE: the `defer region.deinit()` + `defer offset = end`
                // both fire at the end of every iteration (including the reference's
                // `else continue` no-record path). The port records into `record`
                // and always runs `region.deinit()` + `offset = end` at the bottom.
                let record = match link.highlight {
                    // .always, .always_mods => {},
                    Highlight::Always | Highlight::AlwaysMods(_) => true,
                    // .hover, .hover_mods => if (mouse_viewport) |vp| {
                    //     for (map.items[start..end]) |pt| {
                    //         if (pt.eql(vp)) break;
                    //     } else continue;
                    // } else continue,
                    Highlight::Hover | Highlight::HoverMods(_) => {
                        if let Some(vp) = mouse_viewport {
                            map[start..end].iter().any(|pt| pt.eql(vp))
                        } else {
                            false
                        }
                    }
                };

                if record {
                    // Record the match
                    // for (map.items[start..end]) |pt| {
                    //     try result.put(alloc, pt, {});
                    // }
                    for pt in &map[start..end] {
                        result.put(*pt, ());
                    }
                }

                region.deinit();
                offset = end;
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use allocator_api2::alloc::Global;
    use rustty_input::link::{Action, Link as InputLink};
    use rustty_vt::input::key_mods::Mods;
    use rustty_vt::modes::{self as modespkg};
    use rustty_vt::render::{CellSet, RenderState};
    use rustty_vt::terminal::{Colors, Options, Terminal};
    use rustty_vt::{kitty, point, size};

    // Mirrors the cell.rs test harness: build a full `Options` for `Terminal::init`.
    fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> Options {
        Options {
            cols,
            rows,
            max_scrollback: 10_000,
            colors: Colors::default(),
            default_modes: modespkg::ModePacked::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    // test "renderCellMap"
    #[test]
    fn render_cell_map() {
        let alloc = Global;
        // PORT NOTE: the rust `oni` wrapper requires an explicit `onig_initialize`
        // before `Regex::init` (the reference relies on ghostty's global oni.init);
        // mirrors the vt string_map / config url tests.
        oni::testing::ensure_init().unwrap();

        // var t: terminal.Terminal = try .init(alloc, .{ .cols = 5, .rows = 3 });
        // defer t.deinit(alloc);
        let mut t: Terminal = Terminal::init(alloc, opts(5, 3)).unwrap();

        // var s = t.vtStream();
        // defer s.deinit();
        // const str = "1ABCD2EFGH\r\n3IJKL";
        // s.nextSlice(str);
        {
            let mut s = t.vt_stream();
            s.next_slice(b"1ABCD2EFGH\r\n3IJKL");
            s.deinit();
        }

        // var state: terminal.RenderState = .empty;
        // defer state.deinit(alloc);
        // try state.update(alloc, &t);
        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // Get a set
        let mut set = Set::from_config(
            alloc,
            &[
                InputLink {
                    regex: b"AB".to_vec(),
                    action: Action::Open,
                    highlight: Highlight::Always,
                },
                InputLink {
                    regex: b"EF".to_vec(),
                    action: Action::Open,
                    highlight: Highlight::Always,
                },
            ],
        )
        .unwrap();

        // Get our matches
        let mut result: CellSet = CellSet::new();
        set.render_cell_map(alloc, &mut result, &state, None, Mods::default())
            .unwrap();
        assert!(!result.contains(&point::Coordinate { x: 0, y: 0 }));
        assert!(result.contains(&point::Coordinate { x: 1, y: 0 }));
        assert!(result.contains(&point::Coordinate { x: 2, y: 0 }));
        assert!(!result.contains(&point::Coordinate { x: 3, y: 0 }));
        assert!(result.contains(&point::Coordinate { x: 1, y: 1 }));
        assert!(!result.contains(&point::Coordinate { x: 1, y: 2 }));

        set.deinit(alloc);
    }

    // test "renderCellMap hover links"
    #[test]
    fn render_cell_map_hover_links() {
        let alloc = Global;
        oni::testing::ensure_init().unwrap();

        let mut t: Terminal = Terminal::init(alloc, opts(5, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"1ABCD2EFGH\r\n3IJKL");
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // Get a set
        let mut set = Set::from_config(
            alloc,
            &[
                InputLink {
                    regex: b"AB".to_vec(),
                    action: Action::Open,
                    highlight: Highlight::Hover,
                },
                InputLink {
                    regex: b"EF".to_vec(),
                    action: Action::Open,
                    highlight: Highlight::Always,
                },
            ],
        )
        .unwrap();

        // Not hovering over the first link
        {
            let mut result: CellSet = CellSet::new();
            set.render_cell_map(alloc, &mut result, &state, None, Mods::default())
                .unwrap();

            // Test our matches
            assert!(!result.contains(&point::Coordinate { x: 0, y: 0 }));
            assert!(!result.contains(&point::Coordinate { x: 1, y: 0 }));
            assert!(!result.contains(&point::Coordinate { x: 2, y: 0 }));
            assert!(!result.contains(&point::Coordinate { x: 3, y: 0 }));
            assert!(result.contains(&point::Coordinate { x: 1, y: 1 }));
            assert!(!result.contains(&point::Coordinate { x: 1, y: 2 }));
        }

        // Hovering over the first link
        {
            let mut result: CellSet = CellSet::new();
            set.render_cell_map(
                alloc,
                &mut result,
                &state,
                Some(point::Coordinate { x: 1, y: 0 }),
                Mods::default(),
            )
            .unwrap();

            // Test our matches
            assert!(!result.contains(&point::Coordinate { x: 0, y: 0 }));
            assert!(result.contains(&point::Coordinate { x: 1, y: 0 }));
            assert!(result.contains(&point::Coordinate { x: 2, y: 0 }));
            assert!(!result.contains(&point::Coordinate { x: 3, y: 0 }));
            assert!(result.contains(&point::Coordinate { x: 1, y: 1 }));
            assert!(!result.contains(&point::Coordinate { x: 1, y: 2 }));
        }

        set.deinit(alloc);
    }

    // PORT NOTE (9c9cf3e82, WS10 T9): the reference proves the no-allocation
    // property with `std.testing.FailingAllocator{ .fail_index = 0 }` — any
    // allocation from the passed allocator is an error return. That device does
    // NOT transfer: `render_cell_map`'s `alloc` parameter is the concrete ZST
    // `allocator_api2::alloc::Global`, and the buffers this commit is about
    // (`builder`, `map`, and everything `RenderState::string` grows) are plain
    // `Vec`s on the GLOBAL allocator. There is no allocator to swap.
    //
    // So the port substitutes an equivalent, and it is a REAL allocation
    // assertion, not a structural stand-in: a counting `#[global_allocator]`
    // whose counter is THREAD-LOCAL. libtest gives every `#[test]` its own
    // thread, so a parallel test's allocations cannot perturb this count — which
    // a process-global counter could not promise. The count is sampled
    // immediately around the `render_cell_map` call only, so test setup
    // (terminal, regexes, render state) is excluded.
    //
    // Note the assertion could not be made on `result.count() == 0` alone: the
    // per-link `continue`s already produced an empty result BEFORE this commit,
    // so that arm passes with or without the fix. The allocation count is the
    // only thing that distinguishes them — which is exactly why the reference
    // reached for a failing allocator.
    struct CountingAlloc;

    thread_local! {
        static ALLOCS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
    }

    unsafe impl std::alloc::GlobalAlloc for CountingAlloc {
        unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
            ALLOCS.with(|c| c.set(c.get() + 1));
            unsafe { std::alloc::System.alloc(layout) }
        }
        unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
            unsafe { std::alloc::System.dealloc(ptr, layout) }
        }
        unsafe fn realloc(
            &self,
            ptr: *mut u8,
            layout: std::alloc::Layout,
            new_size: usize,
        ) -> *mut u8 {
            ALLOCS.with(|c| c.set(c.get() + 1));
            unsafe { std::alloc::System.realloc(ptr, layout, new_size) }
        }
        unsafe fn alloc_zeroed(&self, layout: std::alloc::Layout) -> *mut u8 {
            ALLOCS.with(|c| c.set(c.get() + 1));
            unsafe { std::alloc::System.alloc_zeroed(layout) }
        }
    }

    // D6-ALLOW(counting shim, not an allocator substitution): this lives inside
    // `#[cfg(test)] mod tests`, so it exists only in the test binary of this one
    // crate — it can never enter a shipped build. It also does not blind ASan/
    // LSan the way oven-sh/bun#30875's mimalloc did: every arm forwards to
    // `std::alloc::System`, i.e. plain `malloc`/`free`, which the sanitizer
    // interceptors still see. It only increments a thread-local counter first.
    #[global_allocator]
    static COUNTING: CountingAlloc = CountingAlloc;

    // test "renderCellMap inactive links don't allocate"
    #[test]
    fn render_cell_map_inactive_links_dont_allocate() {
        let alloc = Global;
        oni::testing::ensure_init().unwrap();

        let mut t: Terminal = Terminal::init(alloc, opts(5, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"1ABCD2EFGH\r\n3IJKL");
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // Three links, none of which the mouse state below can activate:
        //   hover        + mouse_viewport == null
        //   always_mods  + ctrl, vs empty mods
        //   hover_mods   + shift, vs null viewport AND empty mods
        let mut set = Set::from_config(
            alloc,
            &[
                InputLink {
                    regex: b"AB".to_vec(),
                    action: Action::Open,
                    highlight: Highlight::Hover,
                },
                InputLink {
                    regex: b"EF".to_vec(),
                    action: Action::Open,
                    highlight: Highlight::AlwaysMods(Mods {
                        ctrl: true,
                        ..Mods::default()
                    }),
                },
                InputLink {
                    regex: b"IJ".to_vec(),
                    action: Action::Open,
                    highlight: Highlight::HoverMods(Mods {
                        shift: true,
                        ..Mods::default()
                    }),
                },
            ],
        )
        .unwrap();

        let mut result: CellSet = CellSet::new();

        let before = ALLOCS.with(std::cell::Cell::get);
        set.render_cell_map(alloc, &mut result, &state, None, Mods::default())
            .unwrap();
        let after = ALLOCS.with(std::cell::Cell::get);

        assert_eq!(
            before,
            after,
            "renderCellMap must not allocate when no link is active \
             (allocated {} time(s))",
            after - before
        );

        // try testing.expectEqual(@as(usize, 0), result.count());
        assert_eq!(0, result.count());

        set.deinit(alloc);
    }

    // test "renderCellMap mods no match"
    #[test]
    fn render_cell_map_mods_no_match() {
        let alloc = Global;
        oni::testing::ensure_init().unwrap();

        let mut t: Terminal = Terminal::init(alloc, opts(5, 3)).unwrap();
        {
            let mut s = t.vt_stream();
            s.next_slice(b"1ABCD2EFGH\r\n3IJKL");
            s.deinit();
        }

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut t).unwrap();

        // Get a set
        let mut set = Set::from_config(
            alloc,
            &[
                InputLink {
                    regex: b"AB".to_vec(),
                    action: Action::Open,
                    highlight: Highlight::Always,
                },
                InputLink {
                    regex: b"EF".to_vec(),
                    action: Action::Open,
                    // .highlight = .{ .always_mods = .{ .ctrl = true } },
                    highlight: Highlight::AlwaysMods(Mods {
                        ctrl: true,
                        ..Default::default()
                    }),
                },
            ],
        )
        .unwrap();

        // Get our matches
        let mut result: CellSet = CellSet::new();
        set.render_cell_map(alloc, &mut result, &state, None, Mods::default())
            .unwrap();

        // Test our matches
        assert!(!result.contains(&point::Coordinate { x: 0, y: 0 }));
        assert!(result.contains(&point::Coordinate { x: 1, y: 0 }));
        assert!(result.contains(&point::Coordinate { x: 2, y: 0 }));
        assert!(!result.contains(&point::Coordinate { x: 3, y: 0 }));
        assert!(!result.contains(&point::Coordinate { x: 1, y: 1 }));
        assert!(!result.contains(&point::Coordinate { x: 1, y: 2 }));

        set.deinit(alloc);
    }
}
