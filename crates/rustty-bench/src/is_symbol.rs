//! This benchmark tests the throughput of grapheme break calculation.
//! This is a common operation in terminal character printing for terminals
//! that support grapheme clustering.
// const IsSymbol = @This();

// const std = @import("std");
// const assert = std.debug.assert;
// const Allocator = std.mem.Allocator;
// const Benchmark = @import("Benchmark.zig");
// const options = @import("options.zig");
// const UTF8Decoder = @import("../terminal/UTF8Decoder.zig");
// const uucode = @import("uucode");
// const symbols_table = @import("../unicode/symbols_table.zig").table;
use crate::benchmark::{Benchmark, Error, VTable};
use crate::cli::BenchmarkAction;
use crate::options::{self, read_slice_short, CacheLineBuf, DataFile};
use allocator_api2::alloc::Global as Allocator;
use core::ffi::c_void;
use core::hint::black_box;
use rustty_unicode::props_uucode::MAX_CODE_POINT;
use rustty_unicode::symbols_table::table as symbols_table;
use rustty_vt::pagelist::OutOfMemory;
use rustty_vt::utf8_decoder::UTF8Decoder;

// const log = std.log.scoped(.@"is-symbol-bench");
const LOG_SCOPE: &str = "is-symbol-bench";

// opts: Options,
// /// The file, opened in the setup function.
// data_f: ?std.fs.File = null,
pub struct IsSymbol {
    opts: Options,
    /// The file, opened in the setup function.
    data_f: Option<DataFile>,
}

// pub const Options = struct {
pub struct Options {
    /// Which test to run.
    // mode: Mode = .uucode,
    mode: Mode,

    /// The data to read as a filepath. If this is "-" then
    /// we will read stdin. If this is unset, then we will
    /// do nothing (benchmark is a noop). It'd be more unixy to
    /// use stdin by default but I find that a hanging CLI command
    /// with no interaction is a bit annoying.
    // data: ?[]const u8 = null,
    data: Option<Vec<u8>>,
}

impl Default for Options {
    fn default() -> Options {
        Options {
            // mode: Mode = .uucode,
            mode: Mode::Uucode,
            data: None,
        }
    }
}

// pub const Mode = enum {
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// uucode implementation
    // uucode,
    Uucode,

    /// Ghostty's table-based approach.
    // table,
    Table,
}

impl Mode {
    fn from_name(name: &[u8]) -> Option<Mode> {
        match name {
            b"uucode" => Some(Mode::Uucode),
            b"table" => Some(Mode::Table),
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
            b"mode" => {
                self.mode = rustty_args::args::parse_field_enum(value, Mode::from_name)?;
                Ok(())
            }
            b"data" => {
                self.data = Some(rustty_args::args::parse_field_string(value)?);
                Ok(())
            }
            _ => Err(rustty_args::args::Error::InvalidField),
        }
    }
}

impl IsSymbol {
    /// Create a new terminal stream handler for the given arguments.
    // pub fn create(alloc: Allocator, opts: Options) !*IsSymbol {
    pub fn create(alloc: Allocator, opts: Options) -> Result<Box<IsSymbol>, OutOfMemory> {
        let _ = alloc;
        Ok(Box::new(IsSymbol { opts, data_f: None }))
    }

    // pub fn destroy(self: *IsSymbol, alloc: Allocator) void {
    pub fn destroy(self: Box<IsSymbol>, alloc: Allocator) {
        let _ = alloc;
        drop(self);
    }

    // pub fn benchmark(self: *IsSymbol) Benchmark {
    pub fn benchmark(&mut self) -> Benchmark {
        Benchmark::init(
            self as *mut Self as *mut c_void,
            VTable {
                // .stepFn = switch (self.opts.mode) { .uucode => stepUucode, .table => stepTable },
                step_fn: match self.opts.mode {
                    Mode::Uucode => Self::step_uucode,
                    Mode::Table => Self::step_table,
                },
                setup_fn: Some(Self::setup),
                teardown_fn: Some(Self::teardown),
            },
        )
    }

    // fn setup(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn setup(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut IsSymbol) };
        // assert(self.data_f == null);
        assert!(self_.data_f.is_none());
        self_.data_f = match options::data_file(self_.opts.data.as_deref()) {
            Ok(f) => f,
            Err(err) => {
                eprintln!("[{LOG_SCOPE}] error opening data file err={err}");
                return Err(Error::BenchmarkFailed);
            }
        };
        Ok(())
    }

    // fn teardown(ptr: *anyopaque) void {
    unsafe fn teardown(ptr: *mut c_void) {
        let self_ = unsafe { &mut *(ptr as *mut IsSymbol) };
        if self_.data_f.is_some() {
            self_.data_f = None;
        }
    }

    // fn stepUucode(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_uucode(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut IsSymbol) };

        // const f = self.data_f orelse return;
        let f = match self_.data_f.as_mut() {
            Some(f) => f,
            None => return Ok(()),
        };
        let r = f;

        let mut d = UTF8Decoder::new();
        let mut buf = CacheLineBuf::new();
        loop {
            let n = match read_slice_short(r, &mut buf.0) {
                Ok(n) => n,
                Err(err) => {
                    eprintln!("[{LOG_SCOPE}] error reading data file err={err:?}");
                    return Err(Error::BenchmarkFailed);
                }
            };
            if n == 0 {
                break;
            }

            for &c in &buf.0[..n] {
                let (cp_, consumed) = d.next(c);
                assert!(consumed);
                if let Some(cp) = cp_ {
                    // std.mem.doNotOptimizeAway(uucode.get(.is_symbol, cp));
                    //
                    // PORT NOTE (§7.3): rustty has no live `uucode` dependency
                    // (R3) — it embeds uucode-derived tables. The faithful
                    // reproduction of the live `uucode.get(.is_symbol, cp)` is
                    // its `get`-closure rule, written inline over already-`pub`
                    // APIs: an out-of-range guard (`cp > MAX_CODE_POINT`) around
                    // the symbols table lookup.
                    black_box(if cp > MAX_CODE_POINT {
                        false
                    } else {
                        symbols_table().get(cp)
                    });
                }
            }
        }
        Ok(())
    }

    // fn stepTable(ptr: *anyopaque) Benchmark.Error!void {
    unsafe fn step_table(ptr: *mut c_void) -> Result<(), Error> {
        let self_ = unsafe { &mut *(ptr as *mut IsSymbol) };

        // const f = self.data_f orelse return;
        let f = match self_.data_f.as_mut() {
            Some(f) => f,
            None => return Ok(()),
        };
        let r = f;

        let mut d = UTF8Decoder::new();
        let mut buf = CacheLineBuf::new();
        loop {
            let n = match read_slice_short(r, &mut buf.0) {
                Ok(n) => n,
                Err(err) => {
                    eprintln!("[{LOG_SCOPE}] error reading data file err={err:?}");
                    return Err(Error::BenchmarkFailed);
                }
            };
            if n == 0 {
                break;
            }

            for &c in &buf.0[..n] {
                let (cp_, consumed) = d.next(c);
                assert!(consumed);
                if let Some(cp) = cp_ {
                    // std.mem.doNotOptimizeAway(symbols_table.get(cp));
                    black_box(symbols_table().get(cp));
                }
            }
        }
        Ok(())
    }
}

// test IsSymbol {
#[test]
fn is_symbol() {
    let alloc = Allocator;
    // const impl: *IsSymbol = try .create(alloc, .{});
    let mut impl_ = IsSymbol::create(alloc, Options::default()).unwrap();
    let bench = impl_.benchmark();
    let _ = bench.run(crate::benchmark::RunMode::Once).unwrap();
    impl_.destroy(alloc);
}

impl BenchmarkAction for IsSymbol {
    type Options = Options;
    fn create(alloc: Allocator, opts: Options) -> Result<Box<Self>, OutOfMemory> {
        IsSymbol::create(alloc, opts)
    }
    fn destroy(self: Box<Self>, alloc: Allocator) {
        IsSymbol::destroy(self, alloc)
    }
    fn benchmark(&mut self) -> Benchmark {
        IsSymbol::benchmark(self)
    }
}
