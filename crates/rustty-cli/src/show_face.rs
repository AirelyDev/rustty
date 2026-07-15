//! rustty-cli::show_face
//!
//! PORTS: ghostty src/cli/show_face.zig (0t) — the `+show-face` action.
//!
//! The whole font pipeline this action drives is ported in `rustty-font-shaper`
//! (the D1 shaper tier): `SharedGridSet`/`SharedGrid`/`CodepointResolver`/
//! `Collection`/`DesiredSize` and the CoreText discoverer. This is the real port,
//! NOT a stub — the earlier "blocked on rustty-font discovery/SharedGridSet" note
//! was a mis-attribution (that subsystem lives in the shaper crate). The one seam
//! is the break-#9 config DI bridge: the reference `SharedGridSet.DerivedConfig
//! .init(alloc, config)` is duck-typed over the GLOBAL config, while the shaper
//! models `DerivedConfig` via local stand-in config types; the conversion is
//! cli-local glue ([`font_shared_grid_set_derived_config`]), mirroring
//! rustty-core's identical `font_shared_grid_set_derived_config` (surface.rs).

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
//   PORT NOTE: the reference threads an arena; rustty uses owned values on the
//   global allocator (RAII), so no arena is created.
// const Action = @import("ghostty.zig").Action;
use crate::rustty::{Action, Error};
// const args = @import("args.zig");
use rustty_args::args;
// const diagnostics = @import("diagnostics.zig");
use rustty_args::diagnostics::DiagnosticList;
// const font = @import("../font/main.zig");
//   -> the shaper tier's `font.*` re-export surface.
use rustty_font_shaper as font;
// const configpkg = @import("../config.zig");
// const Config = configpkg.Config;
use rustty_config::Config;

use allocator_api2::alloc::Global;
use std::io::Write;

// pub const Options = struct {
pub struct Options {
    /// This is set by the CLI parser for deinit.
    // _arena: ?ArenaAllocator = null,
    //   PORT NOTE: dropped (owned fields free on drop).

    /// The codepoint to search for.
    // cp: ?u21 = null,
    //   PORT NOTE: `u21` has no native Rust type; modeled as `u32` with the
    //   21-bit range enforced at parse time (the reference `u21` rejects
    //   values > 0x1F_FFFF via `parseInt`).
    pub cp: Option<u32>,

    /// Search for all of the codepoints in the string.
    // string: ?[]const u8 = null,
    pub string: Option<Vec<u8>>,

    /// Font style to search for.
    // style: font.Style = .regular,
    pub style: font::Style,

    /// If specified, force text or emoji presentation.
    // presentation: ?font.Presentation = null,
    pub presentation: Option<font::Presentation>,

    // Enable arg parsing diagnostics so that we don't get an error if
    // there is a "normal" config setting on the cli.
    // _diagnostics: diagnostics.DiagnosticList = .{},
    pub _diagnostics: DiagnosticList,
}

impl Default for Options {
    /// The reference struct-literal defaults (`.{}`): note `style` defaults to
    /// `.regular` (a hand-written `Default` — Rust enums have no field default).
    fn default() -> Options {
        Options {
            cp: None,
            string: None,
            style: font::Style::Regular,
            presentation: None,
            _diagnostics: DiagnosticList::default(),
        }
    }
}

impl Options {
    // pub fn deinit(self: *Options) void {
    //     if (self._arena) |arena| arena.deinit();
    //     self.* = undefined;
    // }
    //   PORT NOTE: owned fields free on drop; the `defer opts.deinit()` in
    //   `runArgs` is realized by RAII drop at scope end.
    pub fn deinit(&self) {
        let _ = self;
    }

    // /// Enables "-h" and "--help" to work.
    // pub fn help(self: Options) !void { _ = self; return Action.help_error; }
    //   -> the `args::Target::help`/`has_help_decl` hooks below.
}

impl args::Target for Options {
    // inline for (info.@"struct".fields) — the four non-`_` fields.
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), args::ParseError> {
        match key {
            // cp: ?u21
            b"cp" => {
                // A set-but-empty value resets to the default (null).
                if let Some(v) = value {
                    if v.is_empty() {
                        self.cp = None;
                        return Ok(());
                    }
                }
                // `--cp=65` / `0x41` / `0o101` / `0b1000001` — base auto-detected
                // by `parse_field_int` (`std.fmt.parseInt(u21, v, 0)`), then bound-
                // checked to the 21-bit range (`u21` rejects > 0x1F_FFFF).
                let parsed: u32 = args::parse_field_int(value)?;
                if parsed > 0x1F_FFFF {
                    return Err(args::Error::InvalidValue);
                }
                self.cp = Some(parsed);
                Ok(())
            }
            // string: ?[]const u8
            b"string" => {
                if let Some(v) = value {
                    if v.is_empty() {
                        self.string = None;
                        return Ok(());
                    }
                }
                self.string = Some(args::parse_field_string(value)?);
                Ok(())
            }
            // style: font.Style
            b"style" => {
                // A set-but-empty value resets to the default (`.regular`).
                if let Some(v) = value {
                    if v.is_empty() {
                        self.style = font::Style::Regular;
                        return Ok(());
                    }
                }
                // std.meta.stringToEnum(font.Style, value)
                self.style = args::parse_field_enum(value, |name| match name {
                    b"regular" => Some(font::Style::Regular),
                    b"bold" => Some(font::Style::Bold),
                    b"italic" => Some(font::Style::Italic),
                    b"bold_italic" => Some(font::Style::BoldItalic),
                    _ => None,
                })?;
                Ok(())
            }
            // presentation: ?font.Presentation
            b"presentation" => {
                if let Some(v) = value {
                    if v.is_empty() {
                        self.presentation = None;
                        return Ok(());
                    }
                }
                // std.meta.stringToEnum(font.Presentation, value)
                self.presentation = Some(args::parse_field_enum(value, |name| match name {
                    b"text" => Some(font::Presentation::Text),
                    b"emoji" => Some(font::Presentation::Emoji),
                    _ => None,
                })?);
                Ok(())
            }
            // return error.InvalidField;
            _ => Err(args::Error::InvalidField),
        }
    }

    // canTrackDiags(Options) == @hasField(Options, "_diagnostics") == true.
    fn diagnostics_mut(&mut self) -> Option<&mut DiagnosticList> {
        Some(&mut self._diagnostics)
    }

    // @hasDecl(Options, "help") == true.
    fn has_help_decl(&self) -> bool {
        true
    }

    // pub fn help(self: Options) !void { return Action.help_error; }
    fn help(&mut self) -> Option<Result<(), args::ParseError>> {
        let _ = Action::Help;
        Some(Err(args::Error::ActionHelpRequested))
    }
}

/// The `show-face` command shows what font face rustty will use to render a
/// specific codepoint. Note that this command does not take into consideration
/// grapheme clustering or any other Unicode features that might modify the
/// presentation of a codepoint, so this may show a different font face than
/// rustty uses to render a codepoint in a terminal session.
///
/// Flags:
///
///   * `--cp`: Find the face for a single codepoint. The codepoint may be specified
///     in decimal (`--cp=65`), hexadecimal (`--cp=0x41`), octal (`--cp=0o101`), or
///     binary (`--cp=0b1000001`).
///
///   * `--string`: Find the face for all of the codepoints in a string. The
///     string must be a valid UTF-8 sequence.
///
///   * `--style`: Search for a specific style. Valid options are `regular`, `bold`,
///     `italic`, and `bold_italic`.
///
///   * `--presentation`: If set, force searching for a specific presentation
///     style. Valid options are `text` and `emoji`. If unset, the presentation
///     style of a codepoint will be inferred from the Unicode standard.
// pub fn run(alloc: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    // var iter = try args.argsIterator(alloc);
    // defer iter.deinit();
    let mut iter = args::args_iterator()?;

    // var stdout_buffer: [1024]u8 = undefined;
    // var stdout_writer = std.fs.File.stdout().writer(&stdout_buffer);
    // const stdout = &stdout_writer.interface;
    let mut stdout = std::io::BufWriter::new(std::io::stdout());

    // var stderr_buffer: [1024]u8 = undefined;
    // var stderr_writer = std.fs.File.stdout().writer(&stderr_buffer);
    // const stderr = &stderr_writer.interface;
    //   PORT NOTE (reference behavior, show_face.zig:73): the "stderr" writer is
    //   built from `File.stdout()` — the reference writes its error/diagnostic
    //   output to STDOUT, not stderr. Reproduced faithfully (R3): our `stderr`
    //   writer also targets stdout.
    let mut stderr = std::io::BufWriter::new(std::io::stdout());

    // const result = runArgs(alloc, &iter, stdout, stderr);
    let result = run_args(&mut iter, &mut stdout, &mut stderr);
    // stdout.flush() catch {};
    let _ = stdout.flush();
    // stderr.flush() catch {};
    let _ = stderr.flush();
    iter.deinit();
    // return result;
    result
}

// fn runArgs(alloc_gpa, argsIter: anytype, stdout: *std.Io.Writer, stderr: *std.Io.Writer) !u8 {
fn run_args<I, SO, SE>(args_iter: &mut I, stdout: &mut SO, stderr: &mut SE) -> Result<u8, Error>
where
    I: args::ArgIterator + rustty_args::diagnostics::LocationProvider,
    SO: Write,
    SE: Write,
{
    // Its possible to build Ghostty without font discovery!
    // if (comptime font.Discover == void) { try stderr.print(...); return 1; }
    //
    // PORT NOTE: on non-macOS the shaper's Fontconfig discoverer is DEFERRED
    // (discovery.rs), so this build currently has no usable discovery mechanism —
    // the faithful analogue of the reference's `font.Discover == void` arm. On
    // macOS `font::Discover` = CoreText (fully ported) and this arm is skipped.
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (&args_iter, &stdout);
        write!(
            stderr,
            "{app} was built without a font discovery mechanism. This is a compile-time\n\
             option. Please review how {app} was built from source, contact the\n\
             maintainer to enable a font discovery mechanism, and try again.",
            app = rustty_identity::APP_NAME,
        )?;
        return Ok(1);
    }

    #[cfg(target_os = "macos")]
    {
        // var opts: Options = .{};
        // defer opts.deinit();
        let mut opts = Options::default();

        // args.parse(Options, alloc_gpa, &opts, argsIter) catch |err| switch (err) {
        //     error.ActionHelpRequested => return err,
        //     else => { try stderr.print("Error parsing args: {}\n", .{err}); return 1; },
        // };
        match args::parse(&mut opts, args_iter) {
            Ok(()) => {}
            Err(args::Error::ActionHelpRequested) => return Err(Error::ActionHelpRequested),
            Err(err) => {
                writeln!(stderr, "Error parsing args: {err:?}")?;
                return Ok(1);
            }
        }

        // Print out any diagnostics, unless it's likely that the diagnostic was
        // generated trying to parse a "normal" configuration setting. Exit with an
        // error code if any diagnostics were printed.
        // if (!opts._diagnostics.empty()) {
        if !opts._diagnostics.empty() {
            // var exit: bool = false;
            let mut exit = false;
            // outer: for (opts._diagnostics.items()) |diagnostic| {
            'outer: for diagnostic in opts._diagnostics.items() {
                // if (diagnostic.location != .cli) continue :outer;
                if !matches!(
                    diagnostic.location,
                    rustty_args::diagnostics::Location::Cli(_)
                ) {
                    continue 'outer;
                }
                // inner: inline for (@typeInfo(Options).@"struct".fields) |field| {
                //     if (field.name[0] == '_') continue :inner;
                //     if (std.mem.eql(u8, field.name, diagnostic.key)) { print; exit = true; }
                // }
                //   The non-`_` Options fields are `cp`/`string`/`style`/`presentation`.
                if matches!(
                    diagnostic.key.as_slice(),
                    b"cp" | b"string" | b"style" | b"presentation"
                ) {
                    // try stderr.print("config error: {f}\n", .{diagnostic});
                    stderr.write_all(b"config error: ")?;
                    let mut msg = Vec::new();
                    diagnostic.format(&mut msg);
                    stderr.write_all(&msg)?;
                    stderr.write_all(b"\n")?;
                    exit = true;
                }
            }
            // if (exit) return 1;
            if exit {
                return Ok(1);
            }
        }

        // var arena = ArenaAllocator.init(alloc_gpa);
        // defer arena.deinit();
        // const alloc = arena.allocator();
        //   PORT NOTE: dropped — owned values on the global allocator (RAII).

        // if (opts.cp == null and opts.string == null) {
        //     try stderr.print("You must specify a codepoint with --cp or a string with --string\n", .{});
        //     return 1;
        // }
        if opts.cp.is_none() && opts.string.is_none() {
            writeln!(
                stderr,
                "You must specify a codepoint with --cp or a string with --string"
            )?;
            return Ok(1);
        }

        // var config = Config.load(alloc) catch |err| {
        //     try stderr.print("Unable to load config: {}", .{err});
        //     return 1;
        // };
        // defer config.deinit();
        let config = match Config::load() {
            Ok(config) => config,
            Err(err) => {
                write!(stderr, "Unable to load config: {err:?}")?;
                return Ok(1);
            }
        };

        // Print out any diagnostics generated from parsing the config, unless
        // the diagnostic might have been generated because it's actually an
        // action-specific argument.
        // if (!config._diagnostics.empty()) {
        if !config._diagnostics.empty() {
            // outer: for (config._diagnostics.items()) |diagnostic| {
            'outer: for diagnostic in config._diagnostics.items() {
                // inner: inline for (@typeInfo(Options).@"struct".fields) |field| {
                //     if (field.name[0] == '_') continue :inner;
                //     if (std.mem.eql(u8, field.name, diagnostic.key) and
                //         (diagnostic.location == .none or diagnostic.location == .cli)) continue :outer;
                // }
                if matches!(
                    diagnostic.key.as_slice(),
                    b"cp" | b"string" | b"style" | b"presentation"
                ) && matches!(
                    diagnostic.location,
                    rustty_args::diagnostics::Location::None
                        | rustty_args::diagnostics::Location::Cli(_)
                ) {
                    continue 'outer;
                }
                // try stderr.print("config error: {f}\n", .{diagnostic});
                stderr.write_all(b"config error: ")?;
                let mut msg = Vec::new();
                diagnostic.format(&mut msg);
                stderr.write_all(&msg)?;
                stderr.write_all(b"\n")?;
            }
        }

        // var font_grid_set = font.SharedGridSet.init(alloc) catch |err| {
        //     try stderr.print("Unable to initialize font grid set: {}", .{err});
        //     return 1;
        // };
        // errdefer font_grid_set.deinit();
        //   PORT NOTE: on macOS `SharedGridSet.init`'s error set is uninhabited (the
        //   `NoopLibrary` `error{}` arm), so the `catch` print is statically
        //   unreachable (`match e {}` discharges it). The reference has only an
        //   `errdefer` (no unconditional `defer deinit`): no error propagates after
        //   this point (every failure below is caught → `return Ok(1)`), so the set
        //   is dropped by RAII at scope end (the reference leaks it on success).
        let mut font_grid_set = font::SharedGridSet::init(Global).unwrap_or_else(|e| match e {});

        // const font_size: font.face.DesiredSize = .{
        //     .points = config.@"font-size",
        //     .xdpi = 96,
        //     .ydpi = 96,
        // };
        let font_size = font::face::DesiredSize {
            points: config.font_size,
            xdpi: 96,
            ydpi: 96,
        };

        // var font_config = font.SharedGridSet.DerivedConfig.init(alloc, config) catch |err| {
        //     try stderr.print("Unable to initialize font config: {}", .{err});
        //     return 1;
        // };
        //   PORT NOTE (break #9 DI bridge): the reference `DerivedConfig.init` is
        //   duck-typed over the GLOBAL config; rustty-font-shaper models
        //   `DerivedConfig` via local stand-in config types (config.rs), so this is
        //   cli-local glue (identical to rustty-core's `font_shared_grid_set_derived
        //   _config`). All fields convert exactly and it is infallible (Global aborts
        //   on OOM), so the reference `catch` is a comment.
        let font_config = font_shared_grid_set_derived_config(&config);

        // const font_grid_key, const font_grid = font_grid_set.ref(&font_config, font_size) catch |err| {
        //     try stderr.print("Unable to get font grid: {}", .{err});
        //     return 1;
        // };
        // defer font_grid_set.deref(font_grid_key);
        let (font_grid_key, font_grid) = match font_grid_set.r#ref(&font_config, font_size) {
            Ok(pair) => pair,
            Err(err) => {
                write!(stderr, "Unable to get font grid: {err:?}")?;
                return Ok(1);
            }
        };

        // The reference `defer font_grid_set.deref(font_grid_key)` runs on EVERY exit
        // after `ref` (the caught early returns + the final `return 0`); realized by
        // computing the result in a block, then deref-ing before returning.
        let result: Result<u8, Error> = (|| {
            // if (opts.cp) |cp| {
            if let Some(cp) = opts.cp {
                // if (try lookup(alloc, stdout, stderr, font_grid, opts.style, opts.presentation, cp)) |rc| return rc;
                if let Some(rc) =
                    lookup(stdout, stderr, font_grid, opts.style, opts.presentation, cp)?
                {
                    return Ok(rc);
                }
            }
            // if (opts.string) |string| {
            if let Some(string) = &opts.string {
                // const view = std.unicode.Utf8View.init(string) catch |err| {
                //     try stderr.print("Unable to parse string as unicode: {}", .{err});
                //     return 1;
                // };
                let view = match std::str::from_utf8(string) {
                    Ok(view) => view,
                    Err(err) => {
                        write!(stderr, "Unable to parse string as unicode: {err:?}")?;
                        return Ok(1);
                    }
                };
                // var it = view.iterator();
                // while (it.nextCodepoint()) |cp| {
                for ch in view.chars() {
                    let cp = ch as u32;
                    // if (try lookup(alloc, stdout, stderr, font_grid, opts.style, opts.presentation, cp)) |rc| return rc;
                    if let Some(rc) =
                        lookup(stdout, stderr, font_grid, opts.style, opts.presentation, cp)?
                    {
                        return Ok(rc);
                    }
                }
            }

            // return 0;
            Ok(0)
        })();

        // defer font_grid_set.deref(font_grid_key);
        font_grid_set.deref(font_grid_key);

        opts.deinit();

        result
    }
}

// fn lookup(alloc, stdout, stderr, font_grid: *font.SharedGrid, style, presentation, cp) !?u8 {
#[cfg(target_os = "macos")]
fn lookup<SO: Write, SE: Write>(
    stdout: &mut SO,
    stderr: &mut SE,
    font_grid: *mut font::SharedGrid,
    style: font::Style,
    presentation: Option<font::Presentation>,
    cp: u32,
) -> Result<Option<u8>, Error> {
    // SAFETY: `font_grid` points at a live grid owned by the set for this scope.
    let font_grid = unsafe { &mut *font_grid };

    // const idx = font_grid.resolver.getIndex(alloc, cp, style, presentation) orelse {
    //     try stdout.print("U+{0X:0>2} « {0u} » not found.\n", .{cp});
    //     return null;
    // };
    let idx = match font_grid
        .resolver
        .get_index(Global, cp, style, presentation)
    {
        Some(idx) => idx,
        None => {
            write!(stdout, "U+{cp:0>2X} « ")?;
            write_codepoint(stdout, cp)?;
            stdout.write_all(" » not found.\n".as_bytes())?;
            return Ok(None);
        }
    };

    // const face = font_grid.resolver.collection.getFace(idx) catch |err| switch (err) {
    //     error.SpecialHasNoFace => {
    //         try stdout.print("U+{0X:0>2} « {0u} » is handled by Ghostty's internal sprites.\n", .{cp});
    //         return null;
    //     },
    //     else => { try stderr.print("Unable to get face: {}", .{err}); return 1; },
    // };
    let face = match font_grid.resolver.collection.get_face(idx) {
        Ok(face) => face,
        Err(font::collection::Error::SpecialHasNoFace) => {
            // R2: "Ghostty's internal sprites" → the product name via rustty-identity.
            write!(stdout, "U+{cp:0>2X} « ")?;
            write_codepoint(stdout, cp)?;
            writeln!(
                stdout,
                " » is handled by {}'s internal sprites.",
                rustty_identity::APP_NAME
            )?;
            return Ok(None);
        }
        Err(err) => {
            write!(stderr, "Unable to get face: {err:?}")?;
            return Ok(Some(1));
        }
    };

    // var buf: [1024]u8 = undefined;
    let mut buf = [0u8; 1024];
    // const name = face.name(&buf) catch |err| {
    //     try stderr.print("Unable to get name of face: {}", .{err});
    //     return 1;
    // };
    // SAFETY: `face` is the live face pointer just returned by `getFace`.
    let name = match unsafe { &*face }.name(&mut buf) {
        Ok(name) => name,
        Err(err) => {
            write!(stderr, "Unable to get name of face: {err:?}")?;
            return Ok(Some(1));
        }
    };

    // try stdout.print("U+{0X:0>2} « {0u} » found in face “{1s}”.\n", .{ cp, name });
    write!(stdout, "U+{cp:0>2X} « ")?;
    write_codepoint(stdout, cp)?;
    stdout.write_all(" » found in face “".as_bytes())?;
    stdout.write_all(name)?;
    stdout.write_all("”.\n".as_bytes())?;

    // return null;
    Ok(None)
}

/// Format a codepoint as its UTF-8 encoding — the reference `{u}` format
/// specifier (`std.fmt` UTF-8-encodes a `u21`).
#[cfg(target_os = "macos")]
fn write_codepoint<W: Write>(w: &mut W, cp: u32) -> std::io::Result<()> {
    match char::from_u32(cp) {
        Some(ch) => {
            let mut buf = [0u8; 4];
            w.write_all(ch.encode_utf8(&mut buf).as_bytes())
        }
        // A codepoint with no valid Unicode scalar (a surrogate) has no `char`; the
        // reference `{u}` would emit raw bytes. Display-only path (the codepoint is
        // already shown in hex, 0 tests) — emit U+FFFD.
        None => w.write_all("\u{FFFD}".as_bytes()),
    }
}

/// Realizes the reference `font.SharedGridSet.DerivedConfig.init(alloc, config)`
/// (the break-#9 config DI bridge): builds the rustty-font-shaper
/// `SharedGridSet.DerivedConfig` from the global config. The field list mirrors
/// the reference `SharedGridSet.zig DerivedConfig.init` assignments one-for-one
/// (== rustty-font-shaper `shared_grid_set.rs DerivedConfig::init`, whose
/// duck-typed `config: anytype` source is the global config at this callsite ==
/// rustty-core's identical `font_shared_grid_set_derived_config`, surface.rs).
/// The per-type conversions are exact: both sides' font subset types model the
/// SAME reference config types (`RepeatableString`, `FontStyle`,
/// `RepeatableFontVariation`, `RepeatableCodepointMap`, `FontSyntheticStyle`),
/// duplicated across the tier boundary by break #9; `Modifier` and `CodepointMap`
/// are literally the same rustty-font types on both sides.
#[cfg(target_os = "macos")]
fn font_shared_grid_set_derived_config(config: &Config) -> font::shared_grid_set::DerivedConfig {
    use rustty_font_shaper::config as font_config;

    // config.RepeatableString -> shaper RepeatableString: same `list` payload
    // (`overwrite_next` is a CLI-parse artifact the shaper subset never had).
    fn repeatable_string(
        v: &rustty_config::value_types::RepeatableString,
    ) -> font_config::RepeatableString {
        font_config::RepeatableString {
            list: v.list.clone(),
        }
    }

    // config.FontStyle -> shaper FontStyle: identical variants.
    fn font_style(v: &rustty_config::value_types::FontStyle) -> font_config::FontStyle {
        match v {
            rustty_config::value_types::FontStyle::Default => font_config::FontStyle::Default,
            rustty_config::value_types::FontStyle::False => font_config::FontStyle::False,
            rustty_config::value_types::FontStyle::Name(name) => {
                font_config::FontStyle::Name(name.clone())
            }
        }
    }

    // config.RepeatableFontVariation -> shaper RepeatableFontVariation: the config
    // side models `Variation.Id` as the packed `u32` (`a<<24 | b<<16 | c<<8 | d`);
    // the shaper side keeps the reference's field-wise `Id`. Same value, unpacked.
    fn font_variation(
        v: &rustty_config::value_types::RepeatableFontVariation,
    ) -> font_config::RepeatableFontVariation {
        font_config::RepeatableFontVariation {
            list: v
                .list
                .iter()
                .map(|variation| rustty_font_shaper::face::Variation {
                    id: rustty_font_shaper::face::Id {
                        a: (variation.id >> 24) as u8,
                        b: (variation.id >> 16) as u8,
                        c: (variation.id >> 8) as u8,
                        d: variation.id as u8,
                    },
                    value: variation.value,
                })
                .collect(),
        }
    }

    font::shared_grid_set::DerivedConfig {
        // .@"font-family" = try config.@"font-family".clone(alloc), (×4 styles)
        font_family: repeatable_string(&config.font_family),
        font_family_bold: repeatable_string(&config.font_family_bold),
        font_family_italic: repeatable_string(&config.font_family_italic),
        font_family_bold_italic: repeatable_string(&config.font_family_bold_italic),
        // .@"font-style" = try config.@"font-style".clone(alloc), (×4 styles)
        font_style: font_style(&config.font_style),
        font_style_bold: font_style(&config.font_style_bold),
        font_style_italic: font_style(&config.font_style_italic),
        font_style_bold_italic: font_style(&config.font_style_bold_italic),
        // .@"font-variation" = try config.@"font-variation".clone(alloc), (×4 styles)
        font_variation: font_variation(&config.font_variation),
        font_variation_bold: font_variation(&config.font_variation_bold),
        font_variation_italic: font_variation(&config.font_variation_italic),
        font_variation_bold_italic: font_variation(&config.font_variation_bold_italic),
        // .@"font-codepoint-map" = try config.@"font-codepoint-map".clone(alloc),
        //   `CodepointMap` is the same rustty-font type on both sides; deep-cloned
        //   (Global aborts on OOM — the reference returns the error).
        font_codepoint_map: font_config::RepeatableCodepointMap {
            map: config
                .font_codepoint_map
                .map
                .clone(Global)
                .expect("CodepointMap.clone: Global allocator aborts on OOM"),
        },
        // .@"font-synthetic-style" = config.@"font-synthetic-style",
        font_synthetic_style: rustty_font_shaper::collection::FontSyntheticStyle {
            bold: config.font_synthetic_style.bold,
            italic: config.font_synthetic_style.italic,
            bold_italic: config.font_synthetic_style.bold_italic,
        },
        // .@"adjust-cell-width" = config.@"adjust-cell-width", (×13 metrics —
        // `Modifier` is the same rustty-font type on both sides, `Copy`)
        adjust_cell_width: config.adjust_cell_width,
        adjust_cell_height: config.adjust_cell_height,
        adjust_font_baseline: config.adjust_font_baseline,
        adjust_underline_position: config.adjust_underline_position,
        adjust_underline_thickness: config.adjust_underline_thickness,
        adjust_strikethrough_position: config.adjust_strikethrough_position,
        adjust_strikethrough_thickness: config.adjust_strikethrough_thickness,
        adjust_overline_position: config.adjust_overline_position,
        adjust_overline_thickness: config.adjust_overline_thickness,
        adjust_cursor_thickness: config.adjust_cursor_thickness,
        adjust_cursor_height: config.adjust_cursor_height,
        adjust_box_thickness: config.adjust_box_thickness,
        adjust_icon_height: config.adjust_icon_height,
        // .@"freetype-load-flags" = if (FreetypeLoadFlags != void) config.@"freetype-load-flags" else {},
        //   On macOS the shaper's `FreetypeLoadFlags` is `()` (the reference `void`
        //   arm — exact); the config value flows on freetype targets when that DI
        //   lands with the Linux backend.
        freetype_load_flags: rustty_font_shaper::face::FREETYPE_LOAD_FLAGS_DEFAULT,
    }
}
