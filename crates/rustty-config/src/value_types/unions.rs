//! PORTS: ghostty src/config/Config.zig — the in-file tagged unions
//! `WorkingDirectory`, `FontStyle` and `BackgroundBlur`, plus the
//! `std.fmt.parseInt` error set (`ParseIntError`) and `parseInt(u8, _, 0)`.

use super::*;

// ===========================================================================
// Unions (tagged) with method sigs.
// ===========================================================================

/// See working-directory
// pub const WorkingDirectory = union(enum) { home, inherit, path: []const u8 };
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkingDirectory {
    /// Resolve to the current user's home directory during config finalize.
    Home,
    /// Inherit the working directory from the launching process.
    Inherit,
    /// Use an explicit working directory path. This may be not be
    /// expanded until finalize is called.
    Path(Vec<u8>),
}

impl WorkingDirectory {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input_: ?[]const u8) !void
    pub fn parse_cli(&mut self, input_: Option<&[u8]>) -> Result<(), ParseError> {
        // var input = input_ orelse return error.ValueRequired;
        let mut input = input_.ok_or(Error::ValueRequired)?;
        // input = std.mem.trim(u8, input, &std.ascii.whitespace);
        input = trim_ascii_ws(input);
        // if (input.len == 0) return error.ValueRequired;
        if input.is_empty() {
            return Err(Error::ValueRequired);
        }

        // Match path.zig behavior for quoted values.
        // if (input.len >= 2 and input[0] == '"' and input[input.len - 1] == '"') {
        if input.len() >= 2 && input[0] == b'"' && input[input.len() - 1] == b'"' {
            // input = input[1 .. input.len - 1];
            input = &input[1..input.len() - 1];
        }

        // if (std.mem.eql(u8, input, "home")) {
        if input == b"home" {
            // self.* = .home;
            *self = WorkingDirectory::Home;
            // return;
            return Ok(());
        }

        // if (std.mem.eql(u8, input, "inherit")) {
        if input == b"inherit" {
            // self.* = .inherit;
            *self = WorkingDirectory::Inherit;
            // return;
            return Ok(());
        }

        // self.* = .{ .path = try alloc.dupe(u8, input) };
        *self = WorkingDirectory::Path(input.to_vec());
        Ok(())
    }

    /// Expand tilde paths in .path values.
    // pub fn finalize(self: *Self, alloc: Allocator) Allocator.Error!void
    pub fn finalize(&mut self) {
        // const path = switch (self.*) {
        //     .path => |path| path,
        //     else => return,
        // };
        let path = match self {
            WorkingDirectory::Path(path) => path.clone(),
            _ => return,
        };

        // var buf: [std.fs.max_path_bytes]u8 = undefined;
        // PORT NOTE: `std.fs.max_path_bytes` is the platform max path length
        // (4096 on Linux/macOS); we size the stack buffer the same.
        let mut buf = [0u8; MAX_PATH_BYTES];
        // const expanded = expandHome(path, &buf);
        let expanded = crate::config_struct::expand_home(&path, &mut buf);

        // if (std.mem.eql(u8, expanded, path)) return;
        if expanded == &path[..] {
            return;
        }
        // self.* = .{ .path = try alloc.dupe(u8, expanded) };
        *self = WorkingDirectory::Path(expanded.to_vec());
    }

    // pub fn value(self: Self) ?[]const u8
    pub fn value(&self) -> Option<&[u8]> {
        // return switch (self) {
        //     .path => |path| path,
        //     .home, .inherit => null,
        // };
        match self {
            WorkingDirectory::Path(path) => Some(path),
            WorkingDirectory::Home | WorkingDirectory::Inherit => None,
        }
    }

    // pub fn clone(self: Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> WorkingDirectory {
        // return switch (self) {
        //     .path => |path| .{ .path = try alloc.dupe(u8, path) },
        //     else => self,
        // };
        match self {
            WorkingDirectory::Path(path) => WorkingDirectory::Path(path.clone()),
            _ => self.clone(),
        }
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // switch (self) {
        match self {
            // .home, .inherit => try formatter.formatEntry([]const u8, @tagName(self)),
            WorkingDirectory::Home => format_entry_string(name, b"home", writer),
            WorkingDirectory::Inherit => format_entry_string(name, b"inherit", writer),
            // .path => |path| try formatter.formatEntry([]const u8, path),
            WorkingDirectory::Path(path) => format_entry_string(name, path, writer),
        }
    }
}

impl FormatEntry for WorkingDirectory {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        WorkingDirectory::format_entry(self, name, writer)
    }
}

// pub const FontStyle = union(enum) { default: void, false: void, name: [:0]const u8 };
//
// PORT NOTE: the Config field default is `.{ .default = {} }` — the `default`
// arm, which is the first variant, so `#[derive(Default)]` reproduces it exactly.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum FontStyle {
    /// Use the default font style that font discovery finds.
    #[default]
    Default,
    /// Disable this font style completely. This will fall back to using
    /// the regular font when this style is encountered.
    False,
    /// A specific named font style to use for this style.
    Name(Vec<u8>),
}

impl FontStyle {
    // pub fn parseCLI(self: *Self, alloc: Allocator, input: ?[]const u8) !void
    pub fn parse_cli(&mut self, input: Option<&[u8]>) -> Result<(), ParseError> {
        // const value = input orelse return error.ValueRequired;
        let value = input.ok_or(Error::ValueRequired)?;

        // if (std.mem.eql(u8, value, "default")) {
        if value == b"default" {
            // self.* = .{ .default = {} };
            *self = FontStyle::Default;
            // return;
            return Ok(());
        }

        // if (std.mem.eql(u8, value, "false")) {
        if value == b"false" {
            // self.* = .{ .false = {} };
            *self = FontStyle::False;
            // return;
            return Ok(());
        }

        // const nameZ = try alloc.dupeZ(u8, value);
        // self.* = .{ .name = nameZ };
        *self = FontStyle::Name(value.to_vec());
        Ok(())
    }

    /// Returns the string name value that can be used with a font
    /// descriptor.
    // pub fn nameValue(self: Self) ?[:0]const u8
    pub fn name_value(&self) -> Option<&[u8]> {
        // return switch (self) {
        //     .default, .false => null,
        //     .name => self.name,
        // };
        match self {
            FontStyle::Default | FontStyle::False => None,
            FontStyle::Name(name) => Some(name),
        }
    }

    // pub fn clone(self: Self, alloc: Allocator) Allocator.Error!Self
    pub fn clone_in(&self) -> FontStyle {
        // return switch (self) {
        //     .default, .false => self,
        //     .name => |v| .{ .name = try alloc.dupeZ(u8, v) },
        // };
        match self {
            FontStyle::Default | FontStyle::False => self.clone(),
            FontStyle::Name(v) => FontStyle::Name(v.clone()),
        }
    }

    // pub fn formatEntry(self: Self, formatter: formatterpkg.EntryFormatter) !void
    pub fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        // switch (self) {
        match self {
            // .default, .false => try formatter.formatEntry([]const u8, @tagName(self)),
            FontStyle::Default => format_entry_string(name, b"default", writer),
            FontStyle::False => format_entry_string(name, b"false", writer),
            // .name => |name| try formatter.formatEntry([:0]const u8, name),
            FontStyle::Name(v) => format_entry_string(name, v, writer),
        }
    }
}

impl FormatEntry for FontStyle {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        FontStyle::format_entry(self, name, writer)
    }
}

// pub const BackgroundBlur = union(enum) { false, true, "macos-glass-regular", "macos-glass-clear", radius: u8 };
//
// PORT NOTE (Chunk 3a A1 hoist — GPU_RD_CHUNK3A_DESIGN.md): named as a
// `renderer/generic.zig` `DerivedConfig` field type (`background_blur`); the union +
// its `DeepEqual` (hand `.@"union"` arm) are hoisted to the rustty-render-types leaf
// (the `AlphaBlending` precedent) to break the config->renderer cycle. Re-exported here
// so `value_types::BackgroundBlur` and the field/defaults/Key use-sites keep resolving.
// The reference's value methods `parse_cli`/`enabled`/`cval`/`format_entry` stay CONFIG-
// local as free fns below (they are config/cfg-fmt-coupled and carry distinctive
// reference comments that must remain in config provenance; the renderer never calls
// them — it matches the variants directly, `generic.zig:1399`).
pub use rustty_render_types::BackgroundBlur;

// pub fn parseCLI(self: *BackgroundBlur, input: ?[]const u8) !void
pub(crate) fn background_blur_parse_cli(
    v: &mut BackgroundBlur,
    input: Option<&[u8]>,
) -> Result<(), ParseError> {
    // const input_ = input orelse {
    //     // Emulate behavior for bools
    //     self.* = .true;
    //     return;
    // };
    let input_ = match input {
        Some(x) => x,
        None => {
            // Emulate behavior for bools
            *v = BackgroundBlur::True;
            return Ok(());
        }
    };

    // Try to parse normal bools
    // if (cli.args.parseBool(input_)) |b| {
    //     self.* = if (b) .true else .false;
    //     return;
    // } else |_| {}
    if let Ok(b) = parse_bool(input_) {
        *v = if b {
            BackgroundBlur::True
        } else {
            BackgroundBlur::False
        };
        return Ok(());
    }

    // Try to parse enums
    // if (std.meta.stringToEnum(std.meta.Tag(BackgroundBlur), input_)) |v| switch (v) {
    //     inline else => |tag| tag: {
    //         // We can only parse void types
    //         const info = std.meta.fieldInfo(BackgroundBlur, tag);
    //         if (info.type != void) break :tag;
    //         self.* = @unionInit(BackgroundBlur, @tagName(tag), {});
    //         return;
    //     },
    // };
    //
    // PORT NOTE: the comptime `stringToEnum` over the tag enum + the
    // `if (info.type != void) break :tag` guard only ever matches the four
    // void-payload tags (`false`/`true`/`macos-glass-regular`/
    // `macos-glass-clear`); the `radius: u8` tag has a non-void payload so its
    // `break :tag` falls through to the int parse below. (`false`/`true` were
    // already consumed by `parseBool` above, but are listed for fidelity.)
    match input_ {
        b"false" => {
            *v = BackgroundBlur::False;
            return Ok(());
        }
        b"true" => {
            *v = BackgroundBlur::True;
            return Ok(());
        }
        b"macos-glass-regular" => {
            *v = BackgroundBlur::MacosGlassRegular;
            return Ok(());
        }
        b"macos-glass-clear" => {
            *v = BackgroundBlur::MacosGlassClear;
            return Ok(());
        }
        _ => {}
    }

    // self.* = .{ .radius = std.fmt.parseInt(u8, input_, 0) catch return error.InvalidValue };
    *v = BackgroundBlur::Radius(parse_int_u8_base0(input_).map_err(|_| Error::InvalidValue)?);
    Ok(())
}

// pub fn enabled(self: BackgroundBlur) bool
//
// PORT NOTE (Chunk 3a): R1-ported verbatim but currently has NO in-tree caller (the
// apprt blur-enable path is not yet ported); `#[allow(dead_code)]` keeps the
// `-D warnings` gate green until that path lands. Not a stub — the body is faithful.
#[allow(dead_code)]
pub(crate) fn background_blur_enabled(v: BackgroundBlur) -> bool {
    // return switch (self) {
    match v {
        // .false => false,
        BackgroundBlur::False => false,
        // .true => true,
        BackgroundBlur::True => true,
        // .radius => |v| v > 0,
        BackgroundBlur::Radius(r) => r > 0,
        // We treat these as true because they both imply some blur!
        // This has the effect of making the standard blur happen on
        // Linux.
        // .@"macos-glass-regular", .@"macos-glass-clear" => true,
        BackgroundBlur::MacosGlassRegular | BackgroundBlur::MacosGlassClear => true,
    }
}

// pub fn cval(self: BackgroundBlur) i16
pub fn background_blur_cval(v: BackgroundBlur) -> i16 {
    // return switch (self) {
    match v {
        // .false => 0,
        BackgroundBlur::False => 0,
        // .true => 20,
        BackgroundBlur::True => 20,
        // .radius => |v| v,
        BackgroundBlur::Radius(r) => r as i16,
        // I hate sentinel values like this but this is only for
        // our macOS application currently. We can switch to a proper
        // tagged union if we ever need to.
        // .@"macos-glass-regular" => -1,
        BackgroundBlur::MacosGlassRegular => -1,
        // .@"macos-glass-clear" => -2,
        BackgroundBlur::MacosGlassClear => -2,
    }
}

// pub fn formatEntry(self: BackgroundBlur, formatter: anytype) !void
//
// PORT NOTE (Chunk 3a): the former `impl FormatEntry for BackgroundBlur` (a
// rustty-cfg-fmt trait on the now-foreign hoisted type) would orphan; this config-local
// free fn carries the identical body and is dispatched by the `f_entry!(field, fn)` arm.
pub(crate) fn background_blur_format_entry<W: Writer>(
    v: BackgroundBlur,
    name: &[u8],
    writer: &mut W,
) -> Result<(), FormatError<W::Error>> {
    // switch (self) {
    match v {
        // .false => try formatter.formatEntry(bool, false),
        BackgroundBlur::False => format_entry_display(name, &false, writer),
        // .true => try formatter.formatEntry(bool, true),
        BackgroundBlur::True => format_entry_display(name, &true, writer),
        // .radius => |v| try formatter.formatEntry(u8, v),
        BackgroundBlur::Radius(r) => format_entry_display(name, &r, writer),
        // .@"macos-glass-regular" => try formatter.formatEntry([]const u8, "macos-glass-regular"),
        BackgroundBlur::MacosGlassRegular => {
            format_entry_string(name, b"macos-glass-regular", writer)
        }
        // .@"macos-glass-clear" => try formatter.formatEntry([]const u8, "macos-glass-clear"),
        BackgroundBlur::MacosGlassClear => format_entry_string(name, b"macos-glass-clear", writer),
    }
}

/// PORTS: the error set of `std.fmt.parseInt` — `error{ Overflow, InvalidCharacter }`.
//
// PORT NOTE (NEW-FINDINGS N120, task T-C3): this crate carried its own partial
// copies of `std.fmt.parseInt` / `parseUnsigned` / `parseFloat`. They now call
// the one port, `rustty_zigstd::fmt` (task tail-s9 moved it there from
// `rustty_args::std_fmt`), whose tests are Zig 0.16.0's std tables.
pub use rustty_zigstd::fmt::ParseIntError;

/// PORTS: `std.fmt.parseInt(u8, input, 0)` — base-0 means the radix is detected
/// from the prefix (`0x`/`0o`/`0b`).
fn parse_int_u8_base0(input: &[u8]) -> Result<u8, ParseIntError> {
    rustty_zigstd::fmt::parse_int::<u8>(input, 0)
}
