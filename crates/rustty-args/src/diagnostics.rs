//! rustty-args::diagnostics
//!
//! PORTS: ghostty src/cli/diagnostics.zig

// const std = @import("std");
// const builtin = @import("builtin");
// const assert = @import("../quirks.zig").inlineAssert;
// const Allocator = std.mem.Allocator;
// const build_config = @import("../build_config.zig");
use rustty_build_config as build_config;
use rustty_util::quirks::inlineAssert as assert;

// PORT NOTE: `[:0]const u8` (sentinel-terminated byte slices) and `[]const u8`
// map to owned `Vec<u8>` / borrowed `&[u8]` per the fidelity rule (byte-slices,
// never lossy `String`). Diagnostic owns its `key`/`message` bytes (the
// reference allocates them from the arena); since the Rust port's arena model
// hands out owned allocations rather than a bump pointer, owned `Vec<u8>` is the
// faithful representation of "memory allocated from the arena".

/// A diagnostic message from parsing. This is used to provide additional
/// human-friendly warnings and errors about the parsed data.
///
/// All of the memory for the diagnostic is allocated from the arena
/// associated with the config structure. If an arena isn't available
/// then diagnostics are not supported.
// pub const Diagnostic = struct {
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    // location: Location = .none,
    pub location: Location,
    // key: [:0]const u8 = "",
    pub key: Vec<u8>,
    // message: [:0]const u8,
    pub message: Vec<u8>,
}

impl Default for Diagnostic {
    fn default() -> Diagnostic {
        Diagnostic {
            location: Location::None,
            key: Vec::new(),
            // PORT NOTE: `message` has no reference default (it is a required
            // field). Constructors below always set it; this default exists only
            // for the `location`/`key` defaults the reference declares.
            message: Vec::new(),
        }
    }
}

impl Diagnostic {
    /// Write the full user-friendly diagnostic message to the writer.
    // pub fn format(self: *const Diagnostic, writer: *std.Io.Writer) !void {
    pub fn format(&self, writer: &mut Vec<u8>) {
        // switch (self.location) {
        match &self.location {
            // .none => {},
            Location::None => {}
            // .cli => |index| try writer.print("cli:{}:", .{index}),
            Location::Cli(index) => {
                writer.extend_from_slice(format!("cli:{index}:").as_bytes());
            }
            // .file => |file| try writer.print("{s}:{}:", .{ file.path, file.line }),
            Location::File { path, line } => {
                writer.extend_from_slice(path);
                writer.extend_from_slice(format!(":{line}:").as_bytes());
            }
        }

        // if (self.key.len > 0) {
        if !self.key.is_empty() {
            // try writer.print("{s}: ", .{self.key});
            writer.extend_from_slice(&self.key);
            writer.extend_from_slice(b": ");
        // } else if (self.location != .none) {
        } else if self.location != Location::None {
            // try writer.print(" ", .{});
            writer.push(b' ');
        }

        // try writer.print("{s}", .{self.message});
        writer.extend_from_slice(&self.message);
    }

    // pub fn clone(self: *const Diagnostic, alloc: Allocator) Allocator.Error!Diagnostic {
    pub fn clone_in(&self) -> Diagnostic {
        Diagnostic {
            // .location = try self.location.clone(alloc),
            location: self.location.clone_in(),
            // .key = try alloc.dupeZ(u8, self.key),
            key: self.key.clone(),
            // .message = try alloc.dupeZ(u8, self.message),
            message: self.message.clone(),
        }
    }
}

/// The possible locations for a diagnostic message. This is used
/// to provide context for the message.
// pub const Location = union(enum) {
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Location {
    None,
    // cli: usize,
    Cli(usize),
    // file: struct { path: []const u8, line: usize },
    File { path: Vec<u8>, line: usize },
}

/// PORTS: ghostty `Location.Key` — `@typeInfo(Location).@"union".tag_type.?`.
/// The tag of the `Location` union, used by `DiagnosticList.containsLocation`.
// pub const Key = @typeInfo(Location).@"union".tag_type.?;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocationKey {
    None,
    Cli,
    File,
}

/// PORTS: the `anytype` iterator contract used by `Location.fromIter`. The
/// reference reflects over the iterator type at comptime: if it has a `location`
/// decl it calls `iter.location(alloc)`, otherwise it returns `.none`. In Rust
/// the iterator implements this trait; iterators without a location simply do
/// not implement it (modeled by the `Option`-returning default below being
/// overridden per iterator).
//
// PORT NOTE: `fromIter` is folded into this trait. `ArgsIterator` and
// `LineIterator` implement `location`; `SliceIterator` does not (returns `.none`
// via the default), matching `@hasDecl(Iter, "location")`.
pub trait LocationProvider {
    /// Returns a location for a diagnostic message.
    // pub fn location(self: *const Self, ...) ...!?diags.Location
    fn location(&self) -> Result<Option<Location>, LocationError> {
        // Default: the iterator has no `location` decl, so `fromIter` yields
        // `.none`.
        Ok(None)
    }
}

/// PORTS: the error sets of the iterator `location` methods. `ArgsIterator`
/// returns `error{}` (infallible) and `LineIterator` returns `Allocator.Error`;
/// since the Rust port does not surface allocation as a recoverable error, both
/// collapse to this empty error set, modeled as a unit error so the `Result`
/// surface matches the reference's `!?Location`.
#[derive(Debug, PartialEq, Eq)]
pub enum LocationError {}

impl Location {
    pub fn key(&self) -> LocationKey {
        match self {
            Location::None => LocationKey::None,
            Location::Cli(_) => LocationKey::Cli,
            Location::File { .. } => LocationKey::File,
        }
    }

    // pub fn fromIter(iter: anytype, alloc: Allocator) Allocator.Error!Location {
    //
    // PORT NOTE: the comptime type reflection (`switch (@typeInfo(T))` on
    // pointer/struct, then `@hasDecl(Iter, "location")`) is resolved by the
    // `LocationProvider` trait: any iterator passed here implements it, and the
    // default `location()` returns `Ok(None)` for iterators without a location
    // (== `@hasDecl` false => `.none`).
    // PARITY: named `from_iter` after the reference's `fromIter`.
    #[allow(clippy::should_implement_trait)]
    pub fn from_iter<I: LocationProvider + ?Sized>(iter: &I) -> Location {
        // if (!@hasDecl(Iter, "location")) return .none;
        // return (try iter.location(alloc)) orelse .none;
        match iter.location() {
            Ok(Some(loc)) => loc,
            Ok(None) => Location::None,
        }
    }

    // pub fn clone(self: *const Location, alloc: Allocator) Allocator.Error!Location {
    pub fn clone_in(&self) -> Location {
        // return switch (self.*) {
        match self {
            // .none, .cli => self.*,
            Location::None | Location::Cli(_) => self.clone(),
            // .file => |v| .{ .file = .{ .path = try alloc.dupe(u8, v.path), .line = v.line } },
            Location::File { path, line } => Location::File {
                path: path.clone(),
                line: *line,
            },
        }
    }
}

/// A list of diagnostics. The "_diagnostics" field must be this type
/// for diagnostics to be supported. If this field is an incorrect type
/// a compile-time error will be raised.
///
/// This is implemented as a simple wrapper around an array list
/// so that we can inject some logic around adding diagnostics
/// and potentially in the future structure them differently.
// pub const DiagnosticList = struct {
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiagnosticList {
    /// The list of diagnostics.
    // list: std.ArrayListUnmanaged(Diagnostic) = .{},
    list: Vec<Diagnostic>,

    /// Precomputed data for diagnostics. This is used specifically
    /// when we build libghostty so that we can precompute the messages
    /// and return them via the C API without allocating memory at
    /// call time.
    // precompute: Precompute = precompute_init,
    precompute: Precompute,
}

// const precompute_enabled = switch (build_config.artifact) {
//     // We enable precompute for tests so that the logic is
//     // semantically analyzed and run.
//     .exe, .wasm_module => builtin.is_test,
//
//     // We specifically want precompute for libghostty.
//     .lib => true,
// };
//
// PORT NOTE: `builtin.is_test` is reproduced with `cfg!(test)`. For `.exe`/
// `.wasm_module` precompute is on only under test (so the logic is exercised);
// for `.lib` it is always on.
fn precompute_enabled() -> bool {
    match build_config::artifact() {
        build_config::Artifact::Exe | build_config::Artifact::WasmModule => cfg!(test),
        build_config::Artifact::Lib => true,
    }
}

// const Precompute = if (precompute_enabled) struct {
//     messages: std.ArrayListUnmanaged([:0]const u8) = .{},
//     ...
// } else void;
//
// PORT NOTE: `Precompute` is `void` when disabled at comptime. In Rust the type
// is always present but its `messages` are only ever populated when
// `precompute_enabled()` is true (checked at every use site, exactly as the
// reference gates with `comptime precompute_enabled`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Precompute {
    // messages: std.ArrayListUnmanaged([:0]const u8) = .{},
    messages: Vec<Vec<u8>>,
}

impl Precompute {
    // pub fn clone(self: *const Precompute, alloc: Allocator) Allocator.Error!Precompute {
    fn clone_in(&self) -> Precompute {
        // var result: Precompute = .{};
        let mut result = Precompute::default();
        // try result.messages.ensureTotalCapacity(alloc, self.messages.items.len);
        result.messages.reserve(self.messages.len());
        // for (self.messages.items) |msg| {
        for msg in &self.messages {
            // result.messages.appendAssumeCapacity(try alloc.dupeZ(u8, msg));
            result.messages.push(msg.clone());
        }
        result
    }
}

// const precompute_init: Precompute = if (precompute_enabled) .{} else {};

impl DiagnosticList {
    // pub fn clone(self: *const DiagnosticList, alloc: Allocator) Allocator.Error!DiagnosticList {
    pub fn clone_in(&self) -> DiagnosticList {
        // var result: DiagnosticList = .{};
        let mut result = DiagnosticList::default();

        // try result.list.ensureTotalCapacity(alloc, self.list.items.len);
        result.list.reserve(self.list.len());
        // for (self.list.items) |*diag| result.list.appendAssumeCapacity(try diag.clone(alloc));
        for diag in &self.list {
            result.list.push(diag.clone_in());
        }

        // if (comptime precompute_enabled) {
        if precompute_enabled() {
            // result.precompute = try self.precompute.clone(alloc);
            result.precompute = self.precompute.clone_in();
        }

        result
    }

    // pub fn append(self: *DiagnosticList, alloc: Allocator, diag: Diagnostic) Allocator.Error!void {
    pub fn append(&mut self, diag: Diagnostic) {
        // try self.list.append(alloc, diag);
        // errdefer _ = self.list.pop();
        self.list.push(diag.clone());

        // if (comptime precompute_enabled) {
        if precompute_enabled() {
            // var stream: std.Io.Writer.Allocating = .init(alloc);
            // defer stream.deinit();
            let mut stream: Vec<u8> = Vec::new();
            // diag.format(&stream.writer) catch |err| switch (err) {
            //     // WriteFailed in this instance can only mean an OOM
            //     error.WriteFailed => return error.OutOfMemory,
            // };
            diag.format(&mut stream);

            // const owned: [:0]const u8 = try stream.toOwnedSliceSentinel(0);
            // errdefer alloc.free(owned);
            let owned: Vec<u8> = stream;

            // try self.precompute.messages.append(alloc, owned);
            // errdefer _ = self.precompute.messages.pop();
            self.precompute.messages.push(owned);

            // assert(self.precompute.messages.items.len == self.list.items.len);
            assert(self.precompute.messages.len() == self.list.len());
        }
    }

    // pub fn empty(self: *const DiagnosticList) bool {
    pub fn empty(&self) -> bool {
        // return self.list.items.len == 0;
        self.list.is_empty()
    }

    // pub fn items(self: *const DiagnosticList) []const Diagnostic {
    pub fn items(&self) -> &[Diagnostic] {
        // return self.list.items;
        &self.list
    }

    /// The precomputed (formatted) message bytes for diagnostic `idx`, i.e. the
    /// reference's `self.precompute.messages.items[idx]`. Used by the
    /// `ghostty_config_get_diagnostic` C ABI to hand a stable message pointer to
    /// C. Returns `None` when precompute is disabled or `idx` is out of range.
    pub fn precompute_message(&self, idx: usize) -> Option<&[u8]> {
        self.precompute.messages.get(idx).map(Vec::as_slice)
    }

    /// Returns true if there are any diagnostics for the given
    /// location type.
    // pub fn containsLocation(self: *const DiagnosticList, location: Location.Key) bool {
    pub fn contains_location(&self, location: LocationKey) -> bool {
        // for (self.list.items) |diag| {
        for diag in &self.list {
            // if (diag.location == location) return true;
            if diag.location.key() == location {
                return true;
            }
        }

        false
    }
}
