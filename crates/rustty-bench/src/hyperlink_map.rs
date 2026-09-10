//! Benchmark hyperlink cell-map lookups and remove/insert churn.
//!
//! Hyperlink cells are stored in a fixed-capacity, open-addressed hash map.
//! The `churn` mode models terminal output that repeatedly replaces cells in
//! a page whose hyperlink map is already close to full. This is particularly
//! useful for catching probe-length cliffs at high load factors.
// const HyperlinkMap = @This();

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const terminal = @import("../terminal/main.zig");
// const hyperlink = @import("../terminal/hyperlink.zig");
// const Benchmark = @import("Benchmark.zig");
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::{BenchmarkAction, CreateError};
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use core::hint::black_box;
use rustty_vt::hyperlink;
use rustty_vt::page::{hyperlink_set_item_size, Capacity, Page};

// const log = std.log.scoped(.@"hyperlink-map-bench");
const LOG_SCOPE: &str = "hyperlink-map-bench";

// opts: Options,
// page: terminal.Page,
// link_id: hyperlink.Id,
// entry_count: usize,
pub struct HyperlinkMap {
    opts: Options,
    page: Page,
    link_id: hyperlink::Id,
    entry_count: usize,
}

// pub const Options = struct {
pub struct Options {
    /// Requested hyperlink working-set size. Must be a power of two and at
    /// least 16. The map may reserve additional probe headroom.
    // entries: u16 = 4096,
    entries: u16,

    /// Percentage of the map populated before the timed operation.
    /// Values above 100 are treated as 100.
    // @"load-percent": u8 = 100,
    load_percent: u8,

    /// Number of complete passes over the populated cells per step.
    // loops: u16 = 1,
    loops: u16,

    /// Operation to perform in the timed region.
    // mode: Mode = .churn,
    mode: Mode,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            // entries: u16 = 4096,
            entries: 4096,
            // @"load-percent": u8 = 100,
            load_percent: 100,
            // loops: u16 = 1,
            loops: 1,
            // mode: Mode = .churn,
            mode: Mode::Churn,
        }
    }
}

// pub const Mode = enum {
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Look up every populated hyperlink cell.
    // lookup,
    Lookup,

    /// Remove and reinsert every populated hyperlink cell.
    // churn,
    Churn,
}

impl Mode {
    fn from_name(name: &[u8]) -> Option<Mode> {
        match name {
            b"lookup" => Some(Mode::Lookup),
            b"churn" => Some(Mode::Churn),
            _ => None,
        }
    }
}

impl rustty_args::args::Target for Options {
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), rustty_args::args::ParseError> {
        match key {
            // entries: u16
            b"entries" => {
                self.entries = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            // @"load-percent": u8
            b"load-percent" => {
                self.load_percent = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            // loops: u16
            b"loops" => {
                self.loops = rustty_args::args::parse_field_int(value)?;
                Ok(())
            }
            b"mode" => {
                self.mode = rustty_args::args::parse_field_enum(value, Mode::from_name)?;
                Ok(())
            }
            _ => Err(rustty_args::args::Error::InvalidField),
        }
    }
}

impl HyperlinkMap {
    // pub fn create(alloc: Allocator, opts: Options) !*HyperlinkMap {
    pub fn create(_alloc: Allocator, opts: Options) -> Result<Box<HyperlinkMap>, CreateError> {
        // if (opts.entries < 16 or !std.math.isPowerOfTwo(opts.entries)) {
        //     log.err("entries must be a power of two greater than or equal to 16", .{});
        //     return error.InvalidEntries;
        // }
        if opts.entries < 16 || !opts.entries.is_power_of_two() {
            // log.err("entries must be a power of two greater than or equal to 16", .{});
            log::error!(
                target: LOG_SCOPE,
                "entries must be a power of two greater than or equal to 16"
            );
            return Err(CreateError::InvalidEntries);
        }

        // const ptr = try alloc.create(HyperlinkMap);
        // errdefer alloc.destroy(ptr);
        //
        // PORT NOTE: the reference allocates the impl struct up front and fills
        // it at the end; in Rust the `Box::new` at the end is the same thing
        // (and `errdefer alloc.destroy(ptr)` becomes an early `return`).

        // The page requests one map slot per `hyperlink_cell_multiplier` set
        // entries. Keep this relationship explicit so `entries` is the working
        // set size under test regardless of the map's reserved probe headroom.
        // const set_entries = opts.entries / 16;
        let set_entries = opts.entries / 16;
        // var page = try terminal.Page.init(.{
        //     .cols = opts.entries,
        //     .rows = 1,
        //     .hyperlink_bytes = @intCast(@as(usize, set_entries) * @sizeOf(hyperlink.Set.Item)),
        // });
        // errdefer page.deinit();
        let mut page = match Page::init(Capacity {
            cols: opts.entries,
            rows: 1,
            hyperlink_bytes: (set_entries as usize * hyperlink_set_item_size())
                as rustty_vt::size::HyperlinkCountInt,
            ..Capacity::default()
        }) {
            Ok(p) => p,
            Err(_) => return Err(CreateError::OutOfMemory),
        };

        // if (page.hyperlinkCapacity() < opts.entries) {
        //     log.err("insufficient map capacity expected_at_least={} actual={}", .{...});
        //     return error.UnexpectedCapacity;
        // }
        if page.hyperlink_capacity() < opts.entries as usize {
            // log.err("insufficient map capacity expected_at_least={} actual={}", .{ opts.entries, page.hyperlinkCapacity() });
            log::error!(
                target: LOG_SCOPE,
                "insufficient map capacity expected_at_least={} actual={}",
                opts.entries,
                page.hyperlink_capacity(),
            );
            page.deinit();
            return Err(CreateError::UnexpectedCapacity);
        }

        // const link_id = try page.insertHyperlink(.{
        //     .id = .{ .implicit = 1 },
        //     .uri = "https://example.com/benchmark",
        // });
        //
        // PORT NOTE: `Page.insertHyperlink`'s error set (strings/set out of
        // memory) is an allocation failure of the page in every arm, so it
        // folds into `CreateError::OutOfMemory`.
        let link_id = match page.insert_hyperlink(hyperlink::Hyperlink {
            id: hyperlink::HyperlinkId::Implicit(1),
            uri: b"https://example.com/benchmark".to_vec(),
        }) {
            Ok(id) => id,
            Err(_) => {
                page.deinit();
                return Err(CreateError::OutOfMemory);
            }
        };

        // const load = @min(opts.@"load-percent", 100);
        let load = core::cmp::min(opts.load_percent, 100);
        // const entry_count = @max(1, @divFloor(@as(usize, opts.entries) * load, 100));
        let entry_count = core::cmp::max(1, (opts.entries as usize * load as usize) / 100);
        // for (0..entry_count) |x| {
        for x in 0..entry_count {
            // const rac = page.getRowAndCell(x, 0);
            let rac = page.get_row_and_cell(x, 0);
            // page.hyperlink_set.use(page.memory, link_id);
            let base = page.memory_ptr();
            unsafe { page.hyperlink_set.use_id(base, link_id) };
            // try page.setHyperlink(rac.row, rac.cell, link_id);
            if page.set_hyperlink(rac.row, rac.cell, link_id).is_err() {
                page.deinit();
                return Err(CreateError::OutOfMemory);
            }
        }

        // ptr.* = .{ .opts, .page, .link_id, .entry_count };
        // return ptr;
        Ok(Box::new(HyperlinkMap {
            opts,
            page,
            link_id,
            entry_count,
        }))
    }

    // pub fn destroy(self: *HyperlinkMap, alloc: Allocator) void {
    //     self.page.deinit();
    //     alloc.destroy(self);
    // }
    pub fn destroy(mut self: Box<HyperlinkMap>, _alloc: Allocator) {
        self.page.deinit();
        drop(self);
    }

    // pub fn benchmark(self: *HyperlinkMap) Benchmark {
    pub fn benchmark(&mut self) -> Benchmark {
        Benchmark::init(
            self as *mut Self as *mut c_void,
            VTable {
                // .stepFn = switch (self.opts.mode) {
                //     .lookup => stepLookup,
                //     .churn => stepChurn,
                // },
                step_fn: match self.opts.mode {
                    Mode::Lookup => Self::step_lookup,
                    Mode::Churn => Self::step_churn,
                },
                setup_fn: None,
                teardown_fn: None,
            },
        )
    }

    // fn stepLookup(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_lookup(ptr: *mut c_void) -> Result<(), Error> {
        // const self: *HyperlinkMap = @ptrCast(@alignCast(ptr));
        let self_ = unsafe { &mut *(ptr as *mut HyperlinkMap) };

        // for (0..self.opts.loops) |_| {
        for _ in 0..self_.opts.loops {
            // for (0..self.entry_count) |x| {
            for x in 0..self_.entry_count {
                // const cell = self.page.getRowAndCell(x, 0).cell;
                let cell = self_.page.get_row_and_cell(x, 0).cell;
                // const id = self.page.lookupHyperlink(cell) orelse
                //     return error.BenchmarkFailed;
                let Some(id) = self_.page.lookup_hyperlink(cell) else {
                    return Err(Error::BenchmarkFailed);
                };
                // std.mem.doNotOptimizeAway(id);
                black_box(&id);
            }
        }
        Ok(())
    }

    // fn stepChurn(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_churn(ptr: *mut c_void) -> Result<(), Error> {
        // const self: *HyperlinkMap = @ptrCast(@alignCast(ptr));
        let self_ = unsafe { &mut *(ptr as *mut HyperlinkMap) };

        // for (0..self.opts.loops) |_| {
        for _ in 0..self_.opts.loops {
            // for (0..self.entry_count) |x| {
            for x in 0..self_.entry_count {
                // const rac = self.page.getRowAndCell(x, 0);
                let rac = self_.page.get_row_and_cell(x, 0);
                // self.page.clearHyperlink(rac.cell);
                self_.page.clear_hyperlink(rac.cell);
                // self.page.hyperlink_set.use(self.page.memory, self.link_id);
                let base = self_.page.memory_ptr();
                let link_id = self_.link_id;
                unsafe { self_.page.hyperlink_set.use_id(base, link_id) };
                // self.page.setHyperlink(rac.row, rac.cell, self.link_id) catch
                //     return error.BenchmarkFailed;
                if self_
                    .page
                    .set_hyperlink(rac.row, rac.cell, link_id)
                    .is_err()
                {
                    return Err(Error::BenchmarkFailed);
                }
            }
        }
        Ok(())
    }
}

impl BenchmarkAction for HyperlinkMap {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, CreateError> {
        HyperlinkMap::create(alloc, opts)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        HyperlinkMap::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        HyperlinkMap::benchmark(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::benchmark::RunMode;

    // ---- task-14e-2 -------------------------------------------------------
    //
    // `HyperlinkMap.zig:48` is one of only two `log.err` (not `log.warn`) sites
    // in the whole benchmark tree, so it is also this crate's level check on a
    // REAL call path: the token is `error`, not `err`.

    /// An invalid `entries` option takes the reference's `log.err` +
    /// `error.InvalidEntries` arm.
    #[test]
    fn invalid_entries_logs_at_error_level_under_the_reference_scope() {
        let alloc = Allocator;
        let cap = crate::log_capture::start();
        let out = HyperlinkMap::create(
            alloc,
            Options {
                entries: 17, // not a power of two
                mode: Mode::Lookup,
                ..Options::default()
            },
        )
        .map(|_| ());
        let messages = cap.messages();
        drop(cap);

        // Positive reach: the reference's own return value from that arm.
        // (`CreateError` derives no `PartialEq`, so match rather than compare.)
        assert!(
            matches!(out, Err(CreateError::InvalidEntries)),
            "expected error.InvalidEntries, got {out:?}"
        );
        assert_eq!(
            vec![(
                log::Level::Error,
                "hyperlink-map-bench".to_string(),
                "entries must be a power of two greater than or equal to 16".to_string(),
            )],
            messages
        );
        // And the line `logFn` would write, end to end: `error(...)`, never
        // `err(...)`.
        assert_eq!(
            "error(hyperlink-map-bench): entries must be a power of two greater than or equal to 16",
            crate::log_backend::render(messages[0].0, &messages[0].1, &messages[0].2)
        );
    }

    /// The absence half: a VALID `entries` emits nothing. Positive reach is the
    /// created map, which only exists past the guard.
    #[test]
    fn a_valid_hyperlink_map_creation_is_silent() {
        let alloc = Allocator;
        let cap = crate::log_capture::start();
        let created = HyperlinkMap::create(
            alloc,
            Options {
                entries: 64,
                mode: Mode::Lookup,
                ..Options::default()
            },
        );
        let messages = cap.messages();
        drop(cap);

        let impl_ = created.expect(
            "create() failed, so this test never reached the code whose silence \
             it asserts",
        );
        impl_.destroy(alloc);
        assert_eq!(Vec::<crate::log_capture::CapturedRecord>::new(), messages);
    }

    // test HyperlinkMap { ... }
    #[test]
    fn hyperlink_map() {
        let alloc = Allocator;

        // inline for (.{ Mode.lookup, Mode.churn }) |mode| {
        for mode in [Mode::Lookup, Mode::Churn] {
            // const impl = try HyperlinkMap.create(alloc, .{ .entries = 64, .mode = mode });
            // defer impl.destroy(alloc);
            let mut impl_ = HyperlinkMap::create(
                alloc,
                Options {
                    entries: 64,
                    mode,
                    ..Options::default()
                },
            )
            .unwrap();

            // const bench = impl.benchmark();
            // _ = try bench.run(.once);
            let bench = impl_.benchmark();
            bench.run(RunMode::Once).unwrap();
            impl_.destroy(alloc);
        }
    }
}
