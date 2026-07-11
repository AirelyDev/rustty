//! rustty-cli::list_themes
//!
//! PORTS: ghostty src/cli/list_themes.zig (0t) — the `+list-themes` action.
//!
//! PORT STATUS: C5 (CLI_BLUEPRINT.md §1/§7 + Risk 2) — FULLY LANDED (WIRE2 finale).
//! The theme ENUMERATION — the plain/non-tty listing (`--plain`, or output piped to
//! another command), the ONLY path the KAT and any redirected pipe hit — lands LIVE
//! here, built on the ported `config/theme.zig` (`LocationIterator`) +
//! `Config.loadFile`. The interactive vaxis+zf fuzzy-finder picker (`Preview` and its
//! helpers, the `preview` entry, `resolveAutoThemePath`/`writeAutoThemeFile`,
//! `color`, `ThemeListElement.toUri`) is now ALSO LIVE (WIRE2, DEFERRALS.md "cli C5
//! RESOLVED"): filled from the preserved verbatim Zig bodies on `rustty-vaxis`
//! (`Loop`/`Vaxis`/`Window`/`Tty`/`Style`/`Color`/`Key`/`Mouse`/`widgets::TextInput`)
//! plus `rustty-zf` (`zf::rank`). See the `WIRE2 (cli C5 finale)` banner at the tail for
//! the aliasing/lifetime/divergence PORT NOTEs. Tests cover the pure
//! `rank_filtered`, `reselect`, and `to_uri` kernels plus four `draw`
//! render-to-`Vec<u8>` smokes (Miri-clean); the interactive `run` Loop path is
//! `#[ignore]`-on-device. comment-fidelity (scoped-G3b) applies.

// const std = @import("std");
// const args = @import("args.zig");
use rustty_args::args;
// const Action = @import("ghostty.zig").Action;
use crate::ghostty::{Action, Error};
// const Config = @import("../config/Config.zig");
// const configpkg = @import("../config.zig");
use rustty_config::Config;
// const themepkg = @import("../config/theme.zig");
use rustty_config::Location;
use rustty_config::LocationIterator;
// const tui = @import("tui.zig");
use crate::tui;
// const global_state = &@import("../global.zig").state;
//   PORT NOTE (BREAK #1): the reference process-global `state.resources_dir` is
//   `rustty_core::STATE` (the `OnceLock<GlobalState>` core wires up; ADV-FIX 2).
//   Reading `.resources_dir.app()` off it mirrors `global_state.resources_dir
//   .app()`. Note the config `theme.zig` `.resources` arm reads a SEPARATE config-
//   crate seam (`theme::set_resources_dir`); both are populated from the same
//   `GlobalState.resources_dir` by the bin's init (C6), so they stay consistent.

// const vaxis = @import("vaxis");
// const zf = @import("zf");
//   Used by the interactive picker (WIRE2) — imported at the `WIRE2 (cli C5 finale)`
//   banner below (`rustty_vaxis::*` + `rustty_zf::zf::rank`).

use std::io::Write;

// When the number of filtered themes is less than or equal to this threshold,
// the window position will be reset to 0 to show all results from the top.
// This ensures better visibility for small result sets while maintaining
// scroll position for larger lists.
// const SMALL_LIST_THRESHOLD = 10;
//   (used only by the deferred picker's `updateFiltered`; kept for fidelity.)
#[allow(dead_code)]
const SMALL_LIST_THRESHOLD: usize = 10;

// const ColorScheme = enum { all, dark, light };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorScheme {
    All,
    Dark,
    Light,
}

impl ColorScheme {
    /// `@typeInfo(ColorScheme).@"enum".fields |f| f.name` — the ordered variant
    /// names, consumed by the shell-completion generators (`Action::option_specs`
    /// classifies `list-themes --color` as `Enum(ColorScheme::VALUE_NAMES)`).
    pub const VALUE_NAMES: &'static [&'static [u8]] = &[b"all", b"dark", b"light"];

    // std.meta.stringToEnum(ColorScheme, value) — the `--color` value lookup.
    fn from_name(name: &[u8]) -> Option<ColorScheme> {
        Some(match name {
            b"all" => ColorScheme::All,
            b"dark" => ColorScheme::Dark,
            b"light" => ColorScheme::Light,
            _ => return None,
        })
    }
}

// pub const Options = struct {
pub struct Options {
    /// If true, print the full path to the theme.
    // path: bool = false,
    pub path: bool,

    /// If true, force a plain list of themes.
    // plain: bool = false,
    pub plain: bool,

    /// Specifies the color scheme of the themes to include in the list.
    // color: ColorScheme = .all,
    pub color: ColorScheme,
}

impl Default for Options {
    /// The reference struct-literal defaults (`.{}`): `color` defaults to `.all`,
    /// so this is a hand-written `Default`.
    fn default() -> Options {
        Options {
            path: false,
            plain: false,
            color: ColorScheme::All,
        }
    }
}

impl Options {
    // pub fn deinit(self: Options) void {
    //     _ = self;
    // }
    pub fn deinit(&self) {
        let _ = self;
    }

    // /// Enables "-h" and "--help" to work.
    // pub fn help(self: Options) !void {
    //     _ = self;
    //     return Action.help_error;
    // }
    //   -> the `args::Target::help`/`has_help_decl` hooks below.
}

impl args::Target for Options {
    // inline for (info.@"struct".fields) — the two bool flags + the `color` enum.
    fn parse_into_field(
        &mut self,
        key: &[u8],
        value: Option<&[u8]>,
    ) -> Result<(), args::ParseError> {
        match key {
            // path: bool
            b"path" => {
                self.path = args::parse_field_bool(value)?;
                Ok(())
            }
            // plain: bool
            b"plain" => {
                self.plain = args::parse_field_bool(value)?;
                Ok(())
            }
            // color: ColorScheme (enum — std.meta.stringToEnum)
            b"color" => {
                self.color = args::parse_field_enum(value, ColorScheme::from_name)?;
                Ok(())
            }
            // return error.InvalidField;
            _ => Err(args::Error::InvalidField),
        }
    }

    // @hasDecl(Options, "help") == true.
    fn has_help_decl(&self) -> bool {
        true
    }

    // pub fn help(self: Options) !void { return Action.help_error; }
    fn help(&mut self) -> Option<Result<(), args::ParseError>> {
        let _ = Action::Help; // (documents the reference `Action.help_error` source)
        Some(Err(args::Error::ActionHelpRequested))
    }
}

// const ThemeListElement = struct {
pub struct ThemeListElement {
    // location: themepkg.Location,
    pub location: Location,
    // path: []const u8,
    pub path: Vec<u8>,
    // theme: []const u8,
    pub theme: Vec<u8>,
    // rank: ?f64 = null,
    //   PORT NOTE (WIRE2): the picker's `updateFiltered` writes `theme.rank` while
    //   `themes` is held as a SHARED `&[ThemeListElement]` (so theme names/paths can
    //   be printed with the persistent screen's `'a` lifetime — a whole-`&mut` view
    //   would poison those `&'a` reads). `rank` is therefore interior-mutable
    //   (`Cell`) so the ranking pass can faithfully set it through the shared slice.
    //   `rank` is otherwise write-only (never read for display), so `Cell` is sound
    //   and single-threaded (themes never cross the read-thread boundary).
    pub rank: std::cell::Cell<Option<f64>>,
}

impl ThemeListElement {
    // fn lessThan(_: void, lhs: @This(), rhs: @This()) bool {
    //     // TODO: use Unicode-aware comparison
    //     return std.ascii.orderIgnoreCase(lhs.theme, rhs.theme) == .lt;
    // }
    //
    // PORT NOTE: `sortUnstable` requires a total-order comparator, so `run` sorts
    // via `order_ignore_case` DIRECTLY (the reference's `lessThan` predicate is
    // exactly `orderIgnoreCase(...) == .lt`); this method preserves the reference
    // decl + its TODO for R1.
    #[allow(dead_code)]
    fn less_than(lhs: &ThemeListElement, rhs: &ThemeListElement) -> bool {
        // TODO: use Unicode-aware comparison
        order_ignore_case(&lhs.theme, &rhs.theme) == std::cmp::Ordering::Less
    }

    // pub fn toUri(self: *const ThemeListElement, alloc: std.mem.Allocator) ![]const u8 {
    //   VAXIS-DEFERRED: called only from the picker's `draw` (link URI). Preserved
    //   verbatim in the tail block.
}

/// The `list-themes` command is used to preview or list all the available
/// themes for rustty.
///
/// If this command is run from a TTY, a TUI preview of the themes will be
/// shown. While in the preview, `F1` will bring up a help screen and `ESC` will
/// exit the preview. Other keys that can be used to navigate the preview are
/// listed in the help screen.
///
/// If this command is not run from a TTY, or the output is piped to another
/// command, a plain list of theme names will be printed to the screen. A plain
/// list can be forced using the `--plain` CLI flag.
///
/// Two different directories will be searched for themes.
///
/// The first directory is the `themes` subdirectory of your rustty
/// configuration directory. This is `$XDG_CONFIG_HOME/ghostty/themes` or
/// `~/.config/ghostty/themes`.
///
/// The second directory is the `themes` subdirectory of the rustty resources
/// directory. rustty ships with a multitude of themes that will be installed
/// into this directory. On macOS, this directory is the
/// `Ghostty.app/Contents/Resources/ghostty/themes`. On Linux, this directory
/// is the `share/ghostty/themes` (wherever you installed the rustty "share"
/// directory). If you're running rustty from the source, this is the
/// `zig-out/share/ghostty/themes` directory.
///
/// You can also set the `RUSTTY_RESOURCES_DIR` environment variable to point
/// to the resources directory.
///
/// Flags:
///
///   * `--path`: Show the full path to the theme.
///
///   * `--plain`: Force a plain listing of themes.
///
///   * `--color`: Specify the color scheme of the themes included in the list.
///                This can be `dark`, `light`, or `all`. The default is `all`.
// pub fn run(gpa_alloc: std.mem.Allocator) !u8 {
//   PORT NOTE: the `--color` doc continuation is aligned under the flag text at
//   the reference's exact indentation (R1 verbatim); the lint that flags the
//   over-indent is allowed locally rather than reflowing the reference doc.
#[allow(clippy::doc_overindented_list_items)]
pub fn run() -> Result<u8, Error> {
    // var opts: Options = .{};
    // defer opts.deinit();
    let mut opts = Options::default();

    {
        // var iter = try args.argsIterator(gpa_alloc);
        let mut iter = args::args_iterator()?;
        // defer iter.deinit();
        // try args.parse(Options, gpa_alloc, &opts, &iter);
        let result = args::parse(&mut opts, &mut iter);
        iter.deinit();
        result?;
    }

    // var arena = std.heap.ArenaAllocator.init(gpa_alloc);
    // const alloc = arena.allocator();
    //   PORT NOTE: the reference threads an arena for the path/name dupes; the Rust
    //   port owns each `Vec<u8>`/`String`, so no arena is needed.

    // var stdout_buf: [4096]u8 = undefined;
    // var stdout_file: std.fs.File = .stdout();
    // var stdout_writer = stdout_file.writer(&stdout_buf);
    // const stdout = &stdout_writer.interface;
    let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());

    // var stderr_buf: [4096]u8 = undefined;
    // var stderr_writer = std.fs.File.stderr().writer(&stderr_buf);
    // const stderr = &stderr_writer.interface;
    let mut stderr = std::io::BufWriter::new(std::io::stderr().lock());

    // const resources_dir = global_state.resources_dir.app();
    // if (resources_dir == null)
    //     try stderr.print("Could not find the Ghostty resources directory. Please ensure " ++
    //         "that Ghostty is installed correctly.\n", .{});
    //
    // PORT NOTE: `STATE.get()` is `None` until the bin (C6) inits the process
    // global; that collapses into the reference's `resources_dir == null` warning
    // path (rather than dereferencing an uninitialized global), so the observable
    // behavior — print the warning — is preserved.
    let resources_dir = rustty_core::STATE.get().and_then(|s| s.resources_dir.app());
    if resources_dir.is_none() {
        writeln!(
            stderr,
            "Could not find the rustty resources directory. Please ensure \
             that rustty is installed correctly."
        )?;
    }

    // var count: usize = 0;
    let mut count: usize = 0;

    // var themes: std.ArrayList(ThemeListElement) = .empty;
    let mut themes: Vec<ThemeListElement> = Vec::new();

    // var it: themepkg.LocationIterator = .{ .arena_alloc = arena.allocator() };
    let mut it = LocationIterator::new();

    // while (try it.next()) |loc| {
    while let Some(loc) = it.next() {
        // var dir = std.fs.cwd().openDir(loc.dir, .{ .iterate = true }) catch |err| switch (err) {
        //     error.FileNotFound => continue,
        //     else => {
        //         std.debug.print("error trying to open {s}: {}\n", .{ loc.dir, err });
        //         continue;
        //     },
        // };
        // defer dir.close();
        let dir = match std::fs::read_dir(os_str_from_bytes(&loc.dir)) {
            Ok(d) => d,
            Err(err) => match err.kind() {
                std::io::ErrorKind::NotFound => continue,
                _ => {
                    eprintln!(
                        "error trying to open {}: {}",
                        String::from_utf8_lossy(&loc.dir),
                        err
                    );
                    continue;
                }
            },
        };

        // var walker = dir.iterate();
        // while (try walker.next()) |entry| {
        for entry in dir {
            let entry = entry?;
            // switch (entry.kind) {
            //     .file, .sym_link => {
            let ft = entry.file_type()?;
            if ft.is_file() || ft.is_symlink() {
                let name = entry.file_name();
                let name_bytes = os_name_bytes(&name);
                // if (std.mem.eql(u8, entry.name, ".DS_Store"))
                //     continue;
                if name_bytes == b".DS_Store" {
                    continue;
                }
                // count += 1;
                count += 1;

                // const path = try std.fs.path.join(alloc, &.{ loc.dir, entry.name });
                let path = join(&[&loc.dir, name_bytes]);
                // try themes.append(alloc, .{
                //     .path = path,
                //     .location = loc.location,
                //     .theme = try alloc.dupe(u8, entry.name),
                // });
                themes.push(ThemeListElement {
                    path,
                    location: loc.location,
                    theme: name_bytes.to_vec(),
                    rank: std::cell::Cell::new(None),
                });
            }
            //     else => {},
            // }
        }
    }

    // if (count == 0) {
    if count == 0 {
        // try stderr.print("No themes found, check to make sure that the themes were installed correctly.", .{});
        write!(
            stderr,
            "No themes found, check to make sure that the themes were installed correctly."
        )?;
        // return 1;
        stderr.flush()?;
        return Ok(1);
    }

    // std.mem.sortUnstable(ThemeListElement, themes.items, {}, ThemeListElement.lessThan);
    //   (ThemeListElement.lessThan == `std.ascii.orderIgnoreCase(theme) == .lt`.)
    themes.sort_unstable_by(|lhs, rhs| order_ignore_case(&lhs.theme, &rhs.theme));

    // if (tui.can_pretty_print and !opts.plain and stdout_file.isTty()) {
    //     try preview(gpa_alloc, themes.items, opts.color);
    //     return 0;
    // }
    use std::io::IsTerminal;
    if tui::CAN_PRETTY_PRINT && !opts.plain && std::io::stdout().is_terminal() {
        // VAXIS/ZF-DEFERRED (DEFERRALS.md "cli C5"): the interactive picker.
        preview(&themes, opts.color)?;
        return Ok(0);
    }

    // var theme_config = try Config.default(gpa_alloc);
    // defer theme_config.deinit();
    //   PORT NOTE: `Config` drops at scope end; `_arena` is a bool flag in the port.
    let mut theme_config = Config::default_config();
    // for (themes.items) |theme| {
    for theme in &themes {
        // try theme_config.loadFile(theme_config._arena.?.allocator(), theme.path);
        theme_config.load_file(&theme.path)?;
        // if (!shouldIncludeTheme(opts.color, theme_config)) {
        //     continue;
        // }
        if !should_include_theme(opts.color, &theme_config) {
            continue;
        }
        // if (opts.path)
        //     try stdout.print("{s} ({t}) {s}\n", .{ theme.theme, theme.location, theme.path })
        // else
        //     try stdout.print("{s} ({t})\n", .{ theme.theme, theme.location });
        if opts.path {
            stdout.write_all(&theme.theme)?;
            write!(stdout, " ({}) ", location_tag_name(theme.location))?;
            stdout.write_all(&theme.path)?;
            stdout.write_all(b"\n")?;
        } else {
            stdout.write_all(&theme.theme)?;
            write!(stdout, " ({})", location_tag_name(theme.location))?;
            stdout.write_all(b"\n")?;
        }
    }

    // Don't forget to flush!
    // try stdout.flush();
    stdout.flush()?;

    opts.deinit();
    // return 0;
    Ok(0)
}

// fn shouldIncludeTheme(theme_filter: ColorScheme, theme_config: Config) bool {
fn should_include_theme(theme_filter: ColorScheme, theme_config: &Config) -> bool {
    // const rf = @as(f32, @floatFromInt(theme_config.background.r)) / 255.0;
    let rf = theme_config.background.r as f32 / 255.0;
    // const gf = @as(f32, @floatFromInt(theme_config.background.g)) / 255.0;
    let gf = theme_config.background.g as f32 / 255.0;
    // const bf = @as(f32, @floatFromInt(theme_config.background.b)) / 255.0;
    let bf = theme_config.background.b as f32 / 255.0;
    // const luminance = 0.2126 * rf + 0.7152 * gf + 0.0722 * bf;
    let luminance = 0.2126 * rf + 0.7152 * gf + 0.0722 * bf;
    // const is_dark = luminance < 0.5;
    let is_dark = luminance < 0.5;
    // return (theme_filter == .all) or (theme_filter == .dark and is_dark) or (theme_filter == .light and !is_dark);
    (theme_filter == ColorScheme::All)
        || (theme_filter == ColorScheme::Dark && is_dark)
        || (theme_filter == ColorScheme::Light && !is_dark)
}

// ===========================================================================
// PORT NOTE: helpers — the std.fs / std.ascii analogues `run` uses inline. These add
// nothing the reference doesn't already do (R1): minimal Rust glue for the std
// calls the reference makes inline (mirrors config/theme.rs's own helpers).
// ===========================================================================

// PORT NOTE: `std.ascii.orderIgnoreCase` — compares two slices treating ASCII
// letters case-insensitively; on a common prefix, the shorter slice orders first.
fn order_ignore_case(a: &[u8], b: &[u8]) -> std::cmp::Ordering {
    let n = a.len().min(b.len());
    let mut i = 0;
    while i < n {
        let la = a[i].to_ascii_lowercase();
        let lb = b[i].to_ascii_lowercase();
        match la.cmp(&lb) {
            std::cmp::Ordering::Equal => {}
            other => return other,
        }
        i += 1;
    }
    a.len().cmp(&b.len())
}

/// `{t}` (`@tagName`) of a `themepkg.Location` — `user` / `resources`.
fn location_tag_name(loc: Location) -> &'static str {
    match loc {
        Location::User => "user",
        Location::Resources => "resources",
    }
}

/// `std.fs.path.join` for the in-tree uses here (POSIX targets the harness runs):
/// joins with the platform separator, skipping empty components, no trailing sep.
fn join(parts: &[&[u8]]) -> Vec<u8> {
    let sep = std::path::MAIN_SEPARATOR as u8;
    let mut out: Vec<u8> = Vec::new();
    for part in parts {
        if part.is_empty() {
            continue;
        }
        if !out.is_empty() && *out.last().unwrap() != sep {
            out.push(sep);
        }
        out.extend_from_slice(part);
    }
    out
}

/// Build an `&OsStr` from raw path bytes without a lossy round-trip (R3/fidelity:
/// byte-slices stay bytes). POSIX-only, matching the parity harness targets.
#[cfg(unix)]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    use std::os::unix::ffi::OsStrExt;
    std::ffi::OsStr::from_bytes(bytes)
}

#[cfg(not(unix))]
fn os_str_from_bytes(bytes: &[u8]) -> &std::ffi::OsStr {
    std::ffi::OsStr::new(std::str::from_utf8(bytes).unwrap_or(""))
}

/// The raw bytes of a directory entry's `file_name()` (`entry.name`), byte-
/// faithful on POSIX.
#[cfg(unix)]
fn os_name_bytes(name: &std::ffi::OsStr) -> &[u8] {
    use std::os::unix::ffi::OsStrExt;
    name.as_bytes()
}

#[cfg(not(unix))]
fn os_name_bytes(name: &std::ffi::OsStr) -> &[u8] {
    name.to_str().unwrap_or("").as_bytes()
}

// ===========================================================================
// WIRE2 (cli C5 finale) — the interactive vaxis+zf fuzzy-finder theme picker,
// ported LIVE from `cli/list_themes.zig` (`Preview` + `Event` + `color` +
// `lorem_ipsum` + `preview` + `toUri` + `resolveAutoThemePath`/`writeAutoThemeFile`).
//
// PORT NOTES (structural, R1):
//  * The reference `Preview` is a heap struct (`allocator.create(Preview)`) that a
//    `vaxis.Loop(Event)` holds by raw pointer into `&self.tty`/`&self.vx` while the
//    read thread runs. The Rust port maps this to the PROVEN C3b/loop_-lifecycle
//    shape: `tty`/`vx` are SEPARATE run-scope locals (stable addresses, never moved
//    for the read thread's life) and the reference `Preview`'s remaining fields
//    become a `DrawState` local. `Loop::new(&mut tty, &mut vx)` + the disjoint-field
//    `update`/`draw` free fns are exactly the `loop_::tests::loop_lifecycle`
//    (Miri-green) idiom — no whole-object `&mut` is ever formed over the aliased
//    bytes. The fresh `.port/phase-2/mirihar-wire2` harness re-pins this under
//    `cargo +nightly miri test` (see memory `shaper-hook-sb-clean-interleave`:
//    PROVE on an isolated Miri harness, don't hand-reason SB).
//  * The persistent `Vaxis<'a>` screen stores each printed `Segment`'s `text` BY
//    REFERENCE (`Cell<'a>`; `Window<'a>` holds an invariant `*mut Screen<'a>`), so
//    EVERY byte printed each frame must live for `'a`. `themes` is held as a SHARED
//    `&'a [ThemeListElement]` (theme names/paths are `&'a`) and every per-frame
//    temporary (`toUri`, `allocPrint`, diagnostics) is duped into ONE `'a`-lived
//    session `Arena` (the reference frees a per-iteration arena; the port uses a
//    single session arena because the borrows outlive one iteration — a documented,
//    memory-only divergence).
//  * `text_input.drawWithStyle` needs `&'a mut TextInput` + `Window<'a>` (invariant
//    screen lifetime forces `'s == 'a`), which the per-frame `&mut` cannot supply;
//    the search box renders the buffer bytes directly (arena-duped) — the visible
//    search string is identical (the widget cursor glyph is the only omission).
//    Documented forced divergence.
//  * Owned-`KeyOwned` across the queue → `borrow_key`; clipboard has NO `alloc` arg
//    (Rust sig divergence); `TerminalColor` cell-* arms resolve to the reference's
//    missing-value fallback; Wyhash digest short-circuit → an equal-`Vec<usize>`
//    compare (byte-observably identical); `std.Uri` → a faithful `file://` escaper;
//    `LoadError` has no `Display` → `{err:?}` (cosmetic err-pane divergence).
// ===========================================================================

use crate::pretty_arena::{styled, Arena};
use rustty_config::value_types::TerminalColor;
use rustty_vaxis::cell::{Cell as VCell, Color, Hyperlink, Scheme, Segment, Style, Underline};
use rustty_vaxis::key::{self, Key, KeyOwned, Modifiers};
use rustty_vaxis::loop_::LoopEvent;
use rustty_vaxis::mouse::{Button, Mouse, Shape, Type as MouseType};
use rustty_vaxis::vaxis::Options as VaxisOptions;
use rustty_vaxis::widgets::text_input::{Event as TiEvent, TextInput};
use rustty_vaxis::window::{BorderOptions, ChildOptions, PrintOptions, Where, Window, Wrap};
use rustty_vaxis::{Loop, Tty, Vaxis, Winsize};
use rustty_zf::zf::{rank as zf_rank, RankOptions};

// const lorem_ipsum = @embedFile("lorem_ipsum.txt");
//   PORT NOTE (R3): `@embedFile` → `include_bytes!`, kept as `&[u8]` (never a lossy
//   `String`) — `drawPreview` splits it on raw bytes (`splitAny(u8, .., " \n")`).
const LOREM_IPSUM: &[u8] = include_bytes!("lorem_ipsum.txt");

// ---- ThemeListElement.toUri (link URI for the picker's draw) ----
impl ThemeListElement {
    // pub fn toUri(self: *const ThemeListElement, alloc) ![]const u8 {
    //     const uri = std.Uri{ .scheme="file", .host=.{.raw=""}, .path=.{.raw=self.path} };
    //     ... uri.writeToStream(.{ .scheme=true, .authority=true, .path=true }); ...
    //   PORT NOTE: a faithful `file://` URI builder over `&[u8]` — `scheme + "//" +
    //   authority("") + percent-encoded path` per `std.Uri.writeToStream` (RFC 3986
    //   path escaping). Returns an owned `Vec<u8>` (the `toOwnedSlice`).
    fn to_uri(&self) -> Vec<u8> {
        uri_from_path(&self.path)
    }
}

// `std.Uri.writeToStream` path escaping: unreserved + sub-delims + ":" "@" "/" are
// written verbatim; everything else is `%XX`.
fn uri_from_path(path: &[u8]) -> Vec<u8> {
    fn is_path_char(b: u8) -> bool {
        b.is_ascii_alphanumeric()
            || matches!(b, b'-' | b'.' | b'_' | b'~') // unreserved
            || matches!(b, b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'=') // sub-delims
            || matches!(b, b':' | b'@' | b'/')
    }
    let mut out: Vec<u8> = Vec::new();
    out.extend_from_slice(b"file://");
    for &b in path {
        if is_path_char(b) {
            out.push(b);
        } else {
            let _ = write!(out, "%{b:02X}");
        }
    }
    out
}

// const Event = union(enum) { key_press, mouse, color_scheme, winsize };
//   PORT NOTE (§2): Zig's `union(enum)` field-presence gate (`@hasField`) is modeled
//   by which `LoopEvent` constructors return `Some`; these four mirror the reference
//   `Event` exactly. `KeyPress` carries `KeyOwned` (not a borrowed `Key<'a>`) because
//   the queued `T` must be `Send + 'static`.
enum Event {
    // key_press: vaxis.Key  (owned across the queue)
    KeyPress(KeyOwned),
    // mouse: vaxis.Mouse
    Mouse(Mouse),
    // color_scheme: vaxis.Color.Scheme
    ColorScheme(Scheme),
    // winsize: vaxis.Winsize
    Winsize(Winsize),
}

impl LoopEvent for Event {
    fn key_press(k: KeyOwned) -> Option<Self> {
        Some(Event::KeyPress(k))
    }
    fn mouse(m: Mouse) -> Option<Self> {
        Some(Event::Mouse(m))
    }
    fn color_scheme(s: Scheme) -> Option<Self> {
        Some(Event::ColorScheme(s))
    }
    fn winsize(ws: Winsize) -> Option<Self> {
        Some(Event::Winsize(ws))
    }
    // all other constructors default to None ≡ Zig's `@hasField` false.
}

// The `mode: enum { normal, help, search, save }` field.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Normal,
    Help,
    Search,
    Save,
}

// The row-style discriminant `enum { normal, highlighted, selected }` in `draw`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum RowStyle {
    Normal,
    Highlighted,
    Selected,
}

// const Preview = struct { ... } — the reference struct's fields, MINUS `tty`/`vx`
// (run-scope locals, see the module note) and `allocator` (dropped). `themes` is a
// SHARED `&'a` slice; `arena` is the session arena; `rank` writes go through the
// `Cell` on each element.
struct DrawState<'a> {
    // should_quit: bool
    should_quit: bool,
    // mouse: ?vaxis.Mouse
    mouse: Option<Mouse>,
    // themes: []ThemeListElement
    themes: &'a [ThemeListElement],
    // (the `'a`-lived per-frame allocation arena; the reference's per-iteration arena)
    arena: &'a Arena,
    // filtered: std.ArrayList(usize)
    filtered: Vec<usize>,
    // current: usize
    current: usize,
    // window: usize
    window: usize,
    // hex: bool
    hex: bool,
    // mode: enum { normal, help, search, save }
    mode: Mode,
    // color_scheme: vaxis.Color.Scheme
    color_scheme: Scheme,
    // text_input: vaxis.widgets.TextInput
    text_input: TextInput,
    // theme_filter: ColorScheme
    theme_filter: ColorScheme,
}

// borrow_key — `KeyOwned` → `Key<'_>` (§5b). The queued `Event::KeyPress` owns a
// `KeyOwned` (no `matches`); the dispatch needs a borrowed `Key<'_>` view.
fn borrow_key(k: &KeyOwned) -> Key<'_> {
    Key {
        codepoint: k.codepoint,
        text: k.text.as_deref(),
        shifted_codepoint: k.shifted_codepoint,
        base_layout_codepoint: k.base_layout_codepoint,
        mods: k.mods,
    }
}

// fn preview(allocator, themes: []ThemeListElement, theme_filter: ColorScheme) !void {
//   The picker entry. Creates the session arena + drives the read-loop lifecycle.
fn preview(themes: &[ThemeListElement], theme_filter: ColorScheme) -> Result<(), Error> {
    // The session arena outlives `vx` (declared/created before it) so the screen's
    // `'a` cell borrows into duped bytes stay valid — mirrors `defer arena.deinit()`.
    let arena = Arena::init();
    run_preview(&arena, themes, theme_filter)
}

// The reference `Preview.init` + `run` + `deinit`, flattened over run-scope locals
// (the proven separate-`var` shape). `'a` = the session arena / themes lifetime.
fn run_preview<'a>(
    arena: &'a Arena,
    themes: &'a [ThemeListElement],
    theme_filter: ColorScheme,
) -> Result<(), Error> {
    // === Preview.init ===
    // .tty = try .init(buf),   (buf: [4096]u8)
    let mut tty = Tty::init(vec![0u8; 4096])?;
    // .vx = try vaxis.init(allocator, .{}),
    let mut vx: Vaxis<'a> = Vaxis::init(VaxisOptions::default());
    let mut st = DrawState {
        // .should_quit = false,
        should_quit: false,
        // .mouse = null,
        mouse: None,
        // .themes = themes,
        themes,
        arena,
        // .filtered = try .initCapacity(allocator, themes.len),
        filtered: Vec::with_capacity(themes.len()),
        // .current = 0,
        current: 0,
        // .window = 0,
        window: 0,
        // .hex = false,
        hex: false,
        // .mode = .normal,
        mode: Mode::Normal,
        // .color_scheme = .light,
        color_scheme: Scheme::Light,
        // .text_input = .init(allocator),
        text_input: TextInput::init(),
        // .theme_filter = theme_filter,
        theme_filter,
    };
    // try self.updateFiltered();   (pre-loop, no read thread yet)
    update_filtered(&mut st)?;

    // === Preview.run ===
    // ALIASING DISCIPLINE (PROVEN by `.port/phase-2/mirihar-wire2` under Stacked AND
    // Tree Borrows; memory `shaper-hook-sb-clean-interleave`): the `vaxis.Loop` holds
    // raw `*mut Tty`/`*mut Vaxis` and its read thread DEREFS them for the loop's life.
    // Deriving the raw ptrs via `&raw mut` (a `SharedReadWrite` place-projection) and
    // then accessing `tty`/`vx` ONLY through those raw ptrs (TRANSIENT `&mut *ptr`
    // reborrows) keeps the read thread's tag live — whereas `Loop::new(&mut tty, ..)`
    // + a later `tty.writer()` on the LOCAL pushes a `Unique` that POPS the read
    // thread's raw tag off the borrow stack (SB UB; the mirihar reproduces + rejects
    // exactly that shape). So: raw ptrs once, transient reborrows thereafter; the
    // `update`/`draw` free fns receive `&mut *ptr` (disjoint fields, never `&mut
    // *self`). After `stop()` joins the reader, the locals are touched directly (sound).
    let tty_ptr: *mut Tty = &raw mut tty;
    let vx_ptr: *mut Vaxis<'a> = &raw mut vx;

    // var loop: vaxis.Loop(Event) = .{ .tty = &self.tty, .vaxis = &self.vx };
    let loop_ = Loop::<Event>::new(tty_ptr, vx_ptr);
    // try loop.init();
    loop_.init()?;
    // try loop.start();
    loop_.start()?;

    // const writer = self.tty.writer();  (all `tty`/`vx` access via the raw ptrs)
    // SAFETY (each `unsafe` below): `tty_ptr`/`vx_ptr` are the `&raw mut` of the still-
    // live run-scope locals; every reborrow is transient (dropped at the call) and
    // shares the read thread's `SharedReadWrite` parent — no `Unique` outlives a call.
    // try self.vx.enterAltScreen(writer);
    unsafe { (*vx_ptr).enter_alt_screen((*tty_ptr).writer()) }?;
    // try self.vx.setTitle(writer, "👻 Ghostty Theme Preview 👻");
    unsafe {
        (*vx_ptr).set_title(
            (*tty_ptr).writer(),
            "👻 Ghostty Theme Preview 👻".as_bytes(),
        )
    }?;
    // try self.vx.queryTerminal(writer, 1 * std.time.ns_per_s);
    unsafe { (*vx_ptr).query_terminal((*tty_ptr).writer(), 1_000_000_000) }?;
    // try self.vx.setMouseMode(writer, true);
    unsafe { (*vx_ptr).set_mouse_mode((*tty_ptr).writer(), true) }?;
    // if (self.vx.caps.color_scheme_updates) try self.vx.subscribeToColorSchemeUpdates(writer);
    if unsafe { (*vx_ptr).caps.color_scheme_updates } {
        unsafe { (*vx_ptr).subscribe_to_color_scheme_updates((*tty_ptr).writer()) }?;
    }

    // while (!self.should_quit) {
    while !st.should_quit {
        // (the reference's per-iteration arena is the session arena here — see note.)
        // loop.pollEvent();
        loop_.poll_event();
        // while (loop.tryEvent()) |event| try self.update(event, alloc);
        while let Some(event) = loop_.try_event() {
            // update(vx, tty, st, event) — transient disjoint reborrows via the raw ptrs.
            update(
                unsafe { &mut *vx_ptr },
                unsafe { &mut *tty_ptr },
                &mut st,
                event,
            )?;
        }
        // try self.draw(alloc);
        draw(unsafe { &mut *vx_ptr }, &mut st);
        // try self.vx.render(writer);
        unsafe { (*vx_ptr).render((*tty_ptr).writer()) }?;
        // try writer.flush();
        unsafe { (*tty_ptr).writer().flush() }?;
    }

    // PORT NOTE: join the read thread before teardown (the reference relies on
    // process exit; Rust must stop/join so no `*mut tty`/`*mut vx` outlives the
    // locals). `stop` posts a DSR to unblock the blocking read, then joins.
    loop_.stop();

    // === Preview.deinit ===  (filtered/text_input drop on their own)
    //   The reader is joined/dead now, so whole-`tty`/`vx` access is sound again.
    // self.vx.deinit(allocator, self.tty.writer());
    vx.deinit(tty.writer());
    // self.tty.deinit();
    tty.deinit();
    Ok(())
}

// fn up(self: *Preview, count: usize) void  — pure integer nav over DrawState.
fn up(st: &mut DrawState, count: usize) {
    if st.filtered.is_empty() {
        st.current = 0;
        return;
    }
    // self.current -|= count;
    st.current = st.current.saturating_sub(count);
}

// fn down(self: *Preview, count: usize) void
fn down(st: &mut DrawState, count: usize) {
    if st.filtered.is_empty() {
        st.current = 0;
        return;
    }
    // self.current += count;
    st.current += count;
    // if (self.current >= self.filtered.items.len) self.current = self.filtered.items.len - 1;
    if st.current >= st.filtered.len() {
        st.current = st.filtered.len() - 1;
    }
}

// pub fn update(self: *Preview, event: Event, alloc) !void {
//   Free fn over disjoint field borrows (§3/§5): `vx`/`tty` are needed only by the
//   `.winsize` resize + the clipboard arms; every other arm mutates only `st`.
fn update<'a>(
    vx: &mut Vaxis<'a>,
    tty: &mut Tty,
    st: &mut DrawState<'a>,
    event: Event,
) -> Result<(), Error> {
    let ctrl = Modifiers {
        ctrl: true,
        ..Default::default()
    };
    let none = Modifiers::default();
    let shift = Modifiers {
        shift: true,
        ..Default::default()
    };
    match event {
        // .key_press => |key| {
        Event::KeyPress(owned) => {
            let key = borrow_key(&owned);
            // if (key.matches('c', .{ .ctrl = true })) self.should_quit = true;
            if key.matches('c' as u32, ctrl) {
                st.should_quit = true;
            }
            // switch (self.mode) {
            match st.mode {
                // .normal => {
                Mode::Normal => {
                    // if (key.matchesAny(&.{ 'q', escape }, .{})) self.should_quit = true;
                    if key.matches_any(&['q' as u32, key::escape], none) {
                        st.should_quit = true;
                    }
                    // if (key.matchesAny(&.{ '?', f1 }, .{})) self.mode = .help;
                    if key.matches_any(&['?' as u32, key::f1], none) {
                        st.mode = Mode::Help;
                    }
                    // if (key.matches('h', .{ .ctrl = true })) self.mode = .help;
                    if key.matches('h' as u32, ctrl) {
                        st.mode = Mode::Help;
                    }
                    // if (key.matches('/', .{})) self.mode = .search;
                    if key.matches('/' as u32, none) {
                        st.mode = Mode::Search;
                    }
                    // if (key.matchesAny(&.{ enter, kp_enter }, .{})) self.mode = .save;
                    if key.matches_any(&[key::enter, key::kp_enter], none) {
                        st.mode = Mode::Save;
                    }
                    // if (key.matchesAny(&.{ 'x', '/' }, .{ .ctrl = true })) { buf.clearRetainingCapacity(); updateFiltered(); }
                    if key.matches_any(&['x' as u32, '/' as u32], ctrl) {
                        st.text_input.buf.clear_retaining_capacity();
                        update_filtered(st)?;
                    }
                    // if (key.matchesAny(&.{ home, kp_home }, .{})) self.current = 0;
                    if key.matches_any(&[key::home, key::kp_home], none) {
                        st.current = 0;
                    }
                    // if (key.matchesAny(&.{ end, kp_end }, .{})) self.current = self.filtered.items.len - 1;
                    //   PORT NOTE (issue 4): `wrapping_sub(1)` mirrors ReleaseFast's `usize`
                    //   wrap on an empty filter; `draw` resets `current = 0` before any index,
                    //   so the wrapped value is never used to index (matching the reference).
                    if key.matches_any(&[key::end, key::kp_end], none) {
                        st.current = st.filtered.len().wrapping_sub(1);
                    }
                    // if (key.matchesAny(&.{ 'j', '+', down, kp_down, kp_add }, .{})) self.down(1);
                    if key.matches_any(
                        &['j' as u32, '+' as u32, key::down, key::kp_down, key::kp_add],
                        none,
                    ) {
                        down(st, 1);
                    }
                    // if (key.matchesAny(&.{ page_down, kp_down }, .{})) self.down(20);
                    if key.matches_any(&[key::page_down, key::kp_down], none) {
                        down(st, 20);
                    }
                    // if (key.matchesAny(&.{ 'k', '-', up, kp_up, kp_subtract }, .{})) self.up(1);
                    if key.matches_any(
                        &[
                            'k' as u32,
                            '-' as u32,
                            key::up,
                            key::kp_up,
                            key::kp_subtract,
                        ],
                        none,
                    ) {
                        up(st, 1);
                    }
                    // if (key.matchesAny(&.{ page_up, kp_page_up }, .{})) self.up(20);
                    if key.matches_any(&[key::page_up, key::kp_page_up], none) {
                        up(st, 20);
                    }
                    // if (key.matchesAny(&.{ 'h', 'x' }, .{})) self.hex = true;
                    if key.matches_any(&['h' as u32, 'x' as u32], none) {
                        st.hex = true;
                    }
                    // if (key.matches('d', .{})) self.hex = false;
                    if key.matches('d' as u32, none) {
                        st.hex = false;
                    }
                    // if (key.matches('c', .{})) copy theme name  else if (key.matches('c', .{shift})) copy path;
                    //   PORT NOTE (§1 divergence): NO `alloc` arg. (issue 4) guard the
                    //   `filtered[current]` index on an empty filter.
                    if key.matches('c' as u32, none) {
                        if !st.filtered.is_empty() {
                            vx.copy_to_system_clipboard(
                                tty.writer(),
                                &st.themes[st.filtered[st.current]].theme,
                            )?;
                        }
                    } else if key.matches('c' as u32, shift) && !st.filtered.is_empty() {
                        // (issue 4) guard the `filtered[current]` index on an empty filter.
                        vx.copy_to_system_clipboard(
                            tty.writer(),
                            &st.themes[st.filtered[st.current]].path,
                        )?;
                    }
                    // if (key.matches('f', .{})) { cycle theme_filter (all->dark->light->all); updateFiltered(); }
                    if key.matches('f' as u32, none) {
                        st.theme_filter = match st.theme_filter {
                            ColorScheme::All => ColorScheme::Dark,
                            ColorScheme::Dark => ColorScheme::Light,
                            ColorScheme::Light => ColorScheme::All,
                        };
                        update_filtered(st)?;
                    }
                }
                // .help => {
                Mode::Help => {
                    // if (key.matches('q', .{})) self.should_quit = true;
                    if key.matches('q' as u32, none) {
                        st.should_quit = true;
                    }
                    // if (key.matchesAny(&.{ '?', escape, f1 }, .{})) self.mode = .normal;
                    if key.matches_any(&['?' as u32, key::escape, key::f1], none) {
                        st.mode = Mode::Normal;
                    }
                    // if (key.matches('h', .{ .ctrl = true })) self.mode = .normal;
                    if key.matches('h' as u32, ctrl) {
                        st.mode = Mode::Normal;
                    }
                }
                // .search => search: {
                Mode::Search => 'search: {
                    // if (key.matchesAny(&.{ escape, enter }, .{})) { self.mode = .normal; break :search; }
                    if key.matches_any(&[key::escape, key::enter], none) {
                        st.mode = Mode::Normal;
                        break 'search;
                    }
                    // if (key.matchesAny(&.{ 'x', '/' }, .{ .ctrl = true })) { text_input.clearRetainingCapacity(); updateFiltered(); break :search; }
                    if key.matches_any(&['x' as u32, '/' as u32], ctrl) {
                        st.text_input.clear_retaining_capacity();
                        update_filtered(st)?;
                        break 'search;
                    }
                    // try self.text_input.update(.{ .key_press = key });
                    st.text_input.update(TiEvent::KeyPress(key));
                    // try self.updateFiltered();
                    update_filtered(st)?;
                }
                // .save => {
                Mode::Save => {
                    // if (key.matches('q', .{})) self.should_quit = true;
                    if key.matches('q' as u32, none) {
                        st.should_quit = true;
                    }
                    // if (key.matchesAny(&.{ escape, enter, kp_enter }, .{})) self.mode = .normal;
                    if key.matches_any(&[key::escape, key::enter, key::kp_enter], none) {
                        st.mode = Mode::Normal;
                    }
                    // if (key.matches('w', .{})) self.saveSelectedTheme();
                    if key.matches('w' as u32, none) {
                        save_selected_theme(st);
                    }
                }
            }
        }
        // .color_scheme => |color_scheme| self.color_scheme = color_scheme,
        Event::ColorScheme(cs) => st.color_scheme = cs,
        // .mouse => |mouse| self.mouse = mouse,
        Event::Mouse(m) => st.mouse = Some(m),
        // .winsize => |ws| try self.vx.resize(allocator, self.tty.writer(), ws),
        Event::Winsize(ws) => vx.resize(tty.writer(), ws)?,
    }
    Ok(())
}

// ---- the ui_* color helpers (methods on Preview → free fns over the scheme) ----
// pub fn ui_fg(self) vaxis.Color
fn ui_fg(scheme: Scheme) -> Color {
    match scheme {
        Scheme::Light => Color::Rgb([0x00, 0x00, 0x00]),
        Scheme::Dark => Color::Rgb([0xff, 0xff, 0xff]),
    }
}
// pub fn ui_bg(self) vaxis.Color
fn ui_bg(scheme: Scheme) -> Color {
    match scheme {
        Scheme::Light => Color::Rgb([0xff, 0xff, 0xff]),
        Scheme::Dark => Color::Rgb([0x00, 0x00, 0x00]),
    }
}
// pub fn ui_standard(self) vaxis.Style
fn ui_standard(scheme: Scheme) -> Style {
    Style {
        fg: ui_fg(scheme),
        bg: ui_bg(scheme),
        ..Default::default()
    }
}
// pub fn ui_hover_bg(self) vaxis.Color
fn ui_hover_bg(scheme: Scheme) -> Color {
    match scheme {
        Scheme::Light => Color::Rgb([0xbb, 0xbb, 0xbb]),
        Scheme::Dark => Color::Rgb([0x22, 0x22, 0x22]),
    }
}
// pub fn ui_highlighted(self) vaxis.Style
fn ui_highlighted(scheme: Scheme) -> Style {
    Style {
        fg: ui_fg(scheme),
        bg: ui_hover_bg(scheme),
        ..Default::default()
    }
}
// pub fn ui_selected_fg(self) vaxis.Color
fn ui_selected_fg(_scheme: Scheme) -> Color {
    // .light/.dark both 0x00aa00
    Color::Rgb([0x00, 0xaa, 0x00])
}
// pub fn ui_selected_bg(self) vaxis.Color
fn ui_selected_bg(scheme: Scheme) -> Color {
    match scheme {
        Scheme::Light => Color::Rgb([0xaa, 0xaa, 0xaa]),
        Scheme::Dark => Color::Rgb([0x33, 0x33, 0x33]),
    }
}
// pub fn ui_selected(self) vaxis.Style
fn ui_selected(scheme: Scheme) -> Style {
    Style {
        fg: ui_selected_fg(scheme),
        bg: ui_selected_bg(scheme),
        ..Default::default()
    }
}
// pub fn ui_err_fg(self) vaxis.Color
fn ui_err_fg(_scheme: Scheme) -> Color {
    // .light/.dark both 0xff0000
    Color::Rgb([0xff, 0x00, 0x00])
}
// pub fn ui_err(self) vaxis.Style
fn ui_err(scheme: Scheme) -> Style {
    Style {
        fg: ui_err_fg(scheme),
        bg: ui_bg(scheme),
        ..Default::default()
    }
}

// fn color(config: Config, palette: usize) vaxis.Color
fn color(config: &Config, palette: usize) -> Color {
    // .rgb = .{ palette.value[palette].r, .g, .b }
    Color::Rgb([
        config.palette.value[palette].r,
        config.palette.value[palette].g,
        config.palette.value[palette].b,
    ])
}

// `.{ .row_offset = r, .col_offset = c }`
fn propt(row: u16, col: u16) -> PrintOptions {
    PrintOptions {
        row_offset: row,
        col_offset: col,
        ..Default::default()
    }
}

// A segment carrying a hyperlink URI (theme name/path rows).
fn linked<'x>(text: &'x [u8], style: Style, uri: &'x [u8]) -> Segment<'x> {
    Segment {
        text,
        style,
        link: Hyperlink { uri, params: b"" },
    }
}

// pub fn draw(self: *Preview, alloc) !void
//   Free fn over disjoint `&mut vx` / `&mut st` (§7). Never names `tty`, never forms
//   a whole `&mut Preview`. Infallible (all draw ops); the tty writes stay in `run`.
fn draw<'a>(vx: &mut Vaxis<'a>, st: &mut DrawState<'a>) {
    let arena: &'a Arena = st.arena;
    let themes: &'a [ThemeListElement] = st.themes;
    let scheme = st.color_scheme;

    // const win = self.vx.window();
    let win = vx.window();
    // win.clear();
    win.clear();
    // self.vx.setMouseShape(.default);
    vx.set_mouse_shape(Shape::Default);

    // const theme_list = win.child(.{ .x_off=0, .y_off=0, .width=32, .height=win.height });
    let theme_list = win.child(ChildOptions {
        x_off: 0,
        y_off: 0,
        width: Some(32),
        height: Some(win.height),
        ..Default::default()
    });

    // var highlight: ?usize = null;
    let mut highlight: Option<usize> = None;

    // if (self.mouse) |mouse| { self.mouse = null; ... }
    if let Some(mouse) = st.mouse.take() {
        // if (self.mode == .normal) {
        if st.mode == Mode::Normal {
            // if (mouse.button == .wheel_up) self.up(1);
            if mouse.button == Button::WheelUp {
                up(st, 1);
            }
            // if (mouse.button == .wheel_down) self.down(1);
            if mouse.button == Button::WheelDown {
                down(st, 1);
            }
            // if (theme_list.hasMouse(mouse)) |_| {
            if Window::has_mouse(theme_list, Some(mouse)).is_some() {
                // if (mouse.button == .left and mouse.type == .release) {
                if mouse.button == Button::Left && mouse.r#type == MouseType::Release {
                    // const selection = self.window + mouse.row;
                    let selection = st.window + mouse.row as usize;
                    // if (selection < self.filtered.items.len) self.current = selection;
                    if selection < st.filtered.len() {
                        st.current = selection;
                    }
                }
                // highlight = mouse.row;
                highlight = Some(mouse.row as usize);
            }
        }
    }

    // if (self.filtered.items.len == 0) { current=0; window=0; } else { window scroll clamp }
    if st.filtered.is_empty() {
        st.current = 0;
        st.window = 0;
    } else {
        // const start = self.window;
        let start = st.window;
        // const end = self.window + theme_list.height - 1;
        let end = st.window + theme_list.height as usize - 1;
        // if (self.current > end) self.window = self.current - theme_list.height + 1;
        if st.current > end {
            st.window = st.current - theme_list.height as usize + 1;
        }
        // if (self.current < start) self.window = self.current;
        if st.current < start {
            st.window = st.current;
        }
        // if (self.window >= self.filtered.items.len) self.window = self.filtered.items.len - 1;
        if st.window >= st.filtered.len() {
            st.window = st.filtered.len() - 1;
        }
    }

    // theme_list.fill(.{ .style = self.ui_standard() });
    theme_list.fill(VCell {
        style: ui_standard(scheme),
        ..Default::default()
    });

    // for (0..theme_list.height) |row_capture| {
    for row in 0..theme_list.height {
        // const index = self.window + row;
        let index = st.window + row as usize;
        // if (index >= self.filtered.items.len) break;
        if index >= st.filtered.len() {
            break;
        }
        // const theme = self.themes[self.filtered.items[index]];
        let theme = &themes[st.filtered[index]];

        // const style: enum { normal, highlighted, selected } = style: { ... };
        let style = if index == st.current {
            RowStyle::Selected
        } else if highlight == Some(row as usize) {
            RowStyle::Highlighted
        } else {
            RowStyle::Normal
        };

        // if (style == .selected) printSegment("❯ ", ui_selected(), {row,0});
        if style == RowStyle::Selected {
            let _ = theme_list
                .print_segment(styled("❯ ".as_bytes(), ui_selected(scheme)), propt(row, 0));
        }
        // printSegment(theme.theme, style-dependent, link=toUri, {row,2});
        let seg_style = match style {
            RowStyle::Normal => ui_standard(scheme),
            RowStyle::Highlighted => ui_highlighted(scheme),
            RowStyle::Selected => ui_selected(scheme),
        };
        let uri = arena.dupe(&theme.to_uri());
        let _ = theme_list.print_segment(linked(&theme.theme, seg_style, uri), propt(row, 2));

        // if (style == .selected) { right-pad + " ❮" tail }
        if style == RowStyle::Selected {
            // if (theme.theme.len < theme_list.width - 4) { for (2+len .. width-2) print " " }
            if theme.theme.len() < (theme_list.width - 4) as usize {
                for i in (2 + theme.theme.len())..(theme_list.width as usize - 2) {
                    let _ = theme_list
                        .print_segment(styled(b" ", ui_selected(scheme)), propt(row, i as u16));
                }
            }
            // printSegment(" ❮", ui_selected(), {row, width-2});
            let _ = theme_list.print_segment(
                styled(" ❮".as_bytes(), ui_selected(scheme)),
                propt(row, theme_list.width - 2),
            );
        }
    }

    // try self.drawPreview(alloc, win, theme_list.x_off + theme_list.width);
    draw_preview(st, win, theme_list.x_off + theme_list.width as i32);

    // switch (self.mode) {
    match st.mode {
        // .normal => win.hideCursor();
        Mode::Normal => {
            win.hide_cursor();
        }
        // .help => { hideCursor; 60x22 bordered child + the 19-row key_help table }
        Mode::Help => {
            win.hide_cursor();
            let width: u16 = 60;
            let height: u16 = 22;
            let child = win.child(ChildOptions {
                x_off: (win.width / 2).saturating_sub(width / 2) as i32,
                y_off: (win.height / 2).saturating_sub(height / 2) as i32,
                width: Some(width),
                height: Some(height),
                border: BorderOptions {
                    r#where: Where::All,
                    style: ui_standard(scheme),
                    ..Default::default()
                },
            });
            child.fill(VCell {
                style: ui_standard(scheme),
                ..Default::default()
            });

            // const key_help = [_]struct { keys, help }{ ... };
            let key_help: [(&[u8], &[u8]); 19] = [
                (b"^C, q, ESC", b"Quit."),
                (b"F1, ?, ^H", b"Toggle help window."),
                (b"f", b"Cycle through theme filters."),
                ("k, ↑".as_bytes(), b"Move up 1 theme."),
                (b"ScrollUp", b"Move up 1 theme."),
                (b"PgUp", b"Move up 20 themes."),
                ("j, ↓".as_bytes(), b"Move down 1 theme."),
                (b"ScrollDown", b"Move down 1 theme."),
                (b"PgDown", b"Move down 20 themes."),
                (b"h, x", b"Show palette numbers in hexadecimal."),
                (b"d", b"Show palette numbers in decimal."),
                (b"c", b"Copy theme name to the clipboard."),
                (b"C", b"Copy theme path to the clipboard."),
                (b"Home", b"Go to the start of the list."),
                (b"End", b"Go to the end of the list."),
                (b"/", b"Start search."),
                (b"^X, ^/", b"Clear search."),
                ("⏎".as_bytes(), b"Save theme or close search window."),
                (b"w", b"Write theme to auto config file."),
            ];
            // for (key_help, 0..) |help, captured_i| { keys@2, "—"@15, help@17 }
            for (captured_i, (keys, help)) in key_help.iter().enumerate() {
                let i = captured_i as u16;
                let _ = child.print_segment(styled(keys, ui_standard(scheme)), propt(i + 1, 2));
                let _ = child.print_segment(
                    styled("—".as_bytes(), ui_standard(scheme)),
                    propt(i + 1, 15),
                );
                let _ = child.print_segment(styled(help, ui_standard(scheme)), propt(i + 1, 17));
            }
        }
        // .search => bordered child + text_input.drawWithStyle
        Mode::Search => {
            let child = win.child(ChildOptions {
                x_off: 20,
                y_off: (win.height - 5) as i32,
                width: Some(win.width - 40),
                height: Some(3),
                border: BorderOptions {
                    r#where: Where::All,
                    style: ui_standard(scheme),
                    ..Default::default()
                },
            });
            child.fill(VCell {
                style: ui_standard(scheme),
                ..Default::default()
            });
            // self.text_input.drawWithStyle(child, self.ui_standard());
            //   PORT NOTE (forced divergence): `drawWithStyle` needs `&'a mut TextInput`
            //   + `Window<'a>` (the invariant `*mut Screen<'a>` forces `'s == 'a`), which
            //   the per-frame `&mut` cannot supply. Render the buffer bytes directly
            //   (arena-duped, `first_half ++ second_half`) — the visible search string is
            //   identical; only the widget's cursor glyph is omitted.
            let mut buffer: Vec<u8> = Vec::new();
            buffer.extend_from_slice(st.text_input.buf.first_half());
            buffer.extend_from_slice(st.text_input.buf.second_half());
            let text = arena.dupe(&buffer);
            let _ = child.print_segment(styled(text, ui_standard(scheme)), propt(0, 0));
        }
        // .save => 92x17 child + the 14-line save_instructions
        Mode::Save => {
            let width: u16 = 92;
            let height: u16 = 17;
            let child = win.child(ChildOptions {
                x_off: (win.width / 2).saturating_sub(width / 2) as i32,
                y_off: (win.height / 2).saturating_sub(height / 2) as i32,
                width: Some(width),
                height: Some(height),
                border: BorderOptions {
                    r#where: Where::All,
                    style: ui_standard(scheme),
                    ..Default::default()
                },
            });
            child.fill(VCell {
                style: ui_standard(scheme),
                ..Default::default()
            });
            // const theme = self.themes[self.filtered.items[self.current]];  (issue-4 guard)
            //   allocPrint "theme = {s}" → arena-duped; empty filter → skip the line.
            let theme_line: &[u8] = if !st.filtered.is_empty() {
                let theme = &themes[st.filtered[st.current]];
                let mut v: Vec<u8> = Vec::new();
                v.extend_from_slice(b"theme = ");
                v.extend_from_slice(&theme.theme);
                arena.dupe(&v)
            } else {
                b""
            };
            let save_instructions: [&[u8]; 14] = [
                "To apply this theme, add the following line to your Ghostty configuration:"
                    .as_bytes(),
                b"",
                theme_line,
                b"",
                "Save the configuration file and then reload it to apply the new theme.".as_bytes(),
                b"",
                "Or press 'w' to write an auto theme file to your system's preferred default config path."
                    .as_bytes(),
                "Then add the following line to your Ghostty configuration and reload:".as_bytes(),
                b"",
                "config-file = ?auto/theme.ghostty".as_bytes(),
                b"",
                "For more details on configuration and themes, visit the Ghostty documentation:"
                    .as_bytes(),
                b"",
                "https://ghostty.org/docs/config/reference".as_bytes(),
            ];
            // for (save_instructions, 0..) |instruction, captured_i| { instruction@2 }
            for (captured_i, instruction) in save_instructions.iter().enumerate() {
                let i = captured_i as u16;
                let _ =
                    child.print_segment(styled(instruction, ui_standard(scheme)), propt(i + 1, 2));
            }
        }
    }
}

// pub fn drawPreview(self, alloc, win: vaxis.Window, x_off_unconverted: i17) !void
//   Reads only `st` (filtered/current/hex/color_scheme) + `win` — never `tty`/`vx`.
//   Catches the `loadFile` error internally (error pane), so it is infallible.
fn draw_preview<'a>(st: &DrawState<'a>, win: Window<'a>, x_off_unconverted: i32) {
    let arena: &'a Arena = st.arena;
    let themes: &'a [ThemeListElement] = st.themes;
    let scheme = st.color_scheme;

    // const x_off: u16 = @intCast(x_off_unconverted);
    let x_off = x_off_unconverted as u16;
    // const width: u16 = win.width - x_off;
    let width: u16 = win.width - x_off;

    // if (self.filtered.items.len > 0) {
    if !st.filtered.is_empty() {
        // const theme = self.themes[self.filtered.items[self.current]];
        let theme = &themes[st.filtered[st.current]];

        // var config = try Config.default(alloc);
        let mut config = Config::default_config();

        // config.loadFile(..., theme.path) catch |err| { error pane; return; };
        if let Err(err) = config.load_file(&theme.path) {
            let theme_path_len = theme.path.len() as u16;
            let child = win.child(ChildOptions {
                x_off: x_off as i32,
                y_off: 0,
                width: Some(width),
                height: Some(win.height),
                ..Default::default()
            });
            child.fill(VCell {
                style: ui_standard(scheme),
                ..Default::default()
            });
            let middle = child.height / 2;
            {
                // "Unable to open {s} from:"
                let mut v: Vec<u8> = Vec::new();
                v.extend_from_slice(b"Unable to open ");
                v.extend_from_slice(&theme.theme);
                v.extend_from_slice(b" from:");
                let text = arena.dupe(&v);
                let text_len = text.len() as u16;
                let _ = child.print_segment(
                    styled(text, ui_err(scheme)),
                    propt(
                        middle.saturating_sub(1),
                        (child.width / 2).saturating_sub(text_len / 2),
                    ),
                );
            }
            {
                // theme.path (linked)
                let uri = arena.dupe(&theme.to_uri());
                let _ = child.print_segment(
                    linked(&theme.path, ui_err(scheme), uri),
                    propt(middle, (child.width / 2).saturating_sub(theme_path_len / 2)),
                );
            }
            {
                // "{}" of the error — PORT NOTE (issue 3): `LoadError` has NO `Display`,
                // so `{err:?}` (Debug); a documented COSMETIC divergence (err-pane text).
                let text = arena.dupe(format!("{err:?}").as_bytes());
                let text_len = text.len() as u16;
                let _ = child.print_segment(
                    styled(text, ui_err(scheme)),
                    propt(middle + 1, (child.width / 2).saturating_sub(text_len / 2)),
                );
            }
            return;
        }

        // var next_start: u16 = 0;
        let mut next_start: u16 = 0;

        // fg/bg/cursor/selection colors
        let fg = Color::Rgb([
            config.foreground.r,
            config.foreground.g,
            config.foreground.b,
        ]);
        let bg = Color::Rgb([
            config.background.r,
            config.background.g,
            config.background.b,
        ]);
        // PORT NOTE (§1 fidelity): `TerminalColor::Color(c) => rgb`; the cell-* arms
        // have no rgb → resolve to the reference's missing-value fallback (cursor_fg→bg,
        // cursor_bg→fg, selection_fg→bg, selection_bg→fg).
        let cursor_fg = config
            .cursor_text
            .as_ref()
            .and_then(term_color_rgb)
            .unwrap_or(bg);
        let cursor_bg = config
            .cursor_color
            .as_ref()
            .and_then(term_color_rgb)
            .unwrap_or(fg);
        let selection_fg = config
            .selection_foreground
            .as_ref()
            .and_then(term_color_rgb)
            .unwrap_or(bg);
        let selection_bg = config
            .selection_background
            .as_ref()
            .and_then(term_color_rgb)
            .unwrap_or(fg);
        let cursor = Style {
            fg: cursor_fg,
            bg: cursor_bg,
            ..Default::default()
        };
        let standard_selection = Style {
            fg: selection_fg,
            bg: selection_bg,
            ..Default::default()
        };
        let standard = Style {
            fg,
            bg,
            ..Default::default()
        };
        let standard_bold = Style {
            fg,
            bg,
            bold: true,
            ..Default::default()
        };
        let standard_italic = Style {
            fg,
            bg,
            italic: true,
            ..Default::default()
        };
        let standard_bold_italic = Style {
            fg,
            bg,
            bold: true,
            italic: true,
            ..Default::default()
        };
        let standard_underline = Style {
            fg,
            bg,
            ul_style: Underline::Single,
            ..Default::default()
        };
        let standard_double_underline = Style {
            fg,
            bg,
            ul_style: Underline::Double,
            ..Default::default()
        };
        let standard_dashed_underline = Style {
            fg,
            bg,
            ul_style: Underline::Dashed,
            ..Default::default()
        };
        let standard_curly_underline = Style {
            fg,
            bg,
            ul_style: Underline::Curly,
            ..Default::default()
        };
        let standard_dotted_underline = Style {
            fg,
            bg,
            ul_style: Underline::Dotted,
            ..Default::default()
        };

        // { title/path child (height 4) }
        {
            let theme_len = theme.theme.len() as u16;
            let theme_path_len = theme.path.len() as u16;
            let child = win.child(ChildOptions {
                x_off: x_off as i32,
                y_off: next_start as i32,
                width: Some(width),
                height: Some(4),
                ..Default::default()
            });
            child.fill(VCell {
                style: standard,
                ..Default::default()
            });
            let uri1 = arena.dupe(&theme.to_uri());
            let _ = child.print_segment(
                linked(&theme.theme, standard_bold_italic, uri1),
                propt(1, (child.width / 2).saturating_sub(theme_len / 2)),
            );
            let uri2 = arena.dupe(&theme.to_uri());
            let _ = child.print_segment(
                Segment {
                    text: &theme.path,
                    style: standard,
                    link: Hyperlink {
                        uri: uri2,
                        params: b"",
                    },
                },
                PrintOptions {
                    row_offset: 2,
                    col_offset: (child.width / 2).saturating_sub(theme_path_len / 2),
                    wrap: Wrap::None,
                    ..Default::default()
                },
            );
            next_start += child.height;
        }

        // if (config._diagnostics.items().len > 0) { diagnostics child }
        if !config._diagnostics.items().is_empty() {
            let diagnostic_items_len = config._diagnostics.items().len() as u16;
            let child = win.child(ChildOptions {
                x_off: x_off as i32,
                y_off: next_start as i32,
                width: Some(width),
                height: Some(if config._diagnostics.items().is_empty() {
                    0
                } else {
                    2 + diagnostic_items_len
                }),
                ..Default::default()
            });
            {
                let text = b"Problems were encountered trying to load the theme:";
                let text_len = text.len() as u16;
                let _ = child.print_segment(
                    styled(text, ui_err(scheme)),
                    propt(0, (child.width / 2).saturating_sub(text_len / 2)),
                );
            }
            // for (config._diagnostics.items(), 0..) |diag, i| { diag.format(&buf); print@(2+i,2); }
            for (captured_i, diag) in config._diagnostics.items().iter().enumerate() {
                let i = captured_i as u16;
                let mut buf: Vec<u8> = Vec::new();
                diag.format(&mut buf);
                let text = arena.dupe(&buf);
                let _ = child.print_segment(styled(text, ui_err(scheme)), propt(2 + i, 2));
            }
            next_start += child.height;
        }

        // { 6-row 16-color swatch grid }
        {
            let child = win.child(ChildOptions {
                x_off: x_off as i32,
                y_off: next_start as i32,
                width: Some(width),
                height: Some(6),
                ..Default::default()
            });
            child.fill(VCell {
                style: standard,
                ..Default::default()
            });
            for i in 0u16..16 {
                let r = i / 8;
                let c = i % 8;
                // hex " {x:0>2}" / decimal "{d:3}"
                let text = if st.hex {
                    arena.dupe(format!(" {i:02x}").as_bytes())
                } else {
                    arena.dupe(format!("{i:3}").as_bytes())
                };
                let _ = child.print_segment(styled(text, standard), propt(3 * r, c * 8));
                let swatch = Style {
                    fg: color(&config, i as usize),
                    bg,
                    ..Default::default()
                };
                let _ =
                    child.print_segment(styled("████".as_bytes(), swatch), propt(3 * r, 4 + c * 8));
                let _ = child.print_segment(
                    styled("████".as_bytes(), swatch),
                    propt(3 * r + 1, 4 + c * 8),
                );
            }
            next_start += child.height;
        }

        // { 24-row ziggzagg.zig `bat` sample }
        {
            let child = win.child(ChildOptions {
                x_off: x_off as i32,
                y_off: next_start as i32,
                width: Some(width),
                height: Some(24),
                ..Default::default()
            });
            let bold = Style {
                fg,
                bg,
                bold: true,
                ..Default::default()
            };
            let color1 = Style {
                fg: color(&config, 1),
                bg,
                ..Default::default()
            };
            let color2 = Style {
                fg: color(&config, 2),
                bg,
                ..Default::default()
            };
            let color3 = Style {
                fg: color(&config, 3),
                bg,
                ..Default::default()
            };
            let color4 = Style {
                fg: color(&config, 4),
                bg,
                ..Default::default()
            };
            let color5 = Style {
                fg: color(&config, 5),
                bg,
                ..Default::default()
            };
            let color6 = Style {
                fg: color(&config, 6),
                bg,
                ..Default::default()
            };
            let color6ul = Style {
                fg: color(&config, 6),
                bg,
                ul_style: Underline::Single,
                ..Default::default()
            };
            let color10 = Style {
                fg: color(&config, 10),
                bg,
                ..Default::default()
            };
            let color12 = Style {
                fg: color(&config, 12),
                bg,
                ..Default::default()
            };
            let color238 = Style {
                fg: color(&config, 238),
                bg,
                ..Default::default()
            };
            child.fill(VCell {
                style: standard,
                ..Default::default()
            });
            let _ = child.print(
                &[
                    styled("→".as_bytes(), color2),
                    styled(b" ", standard),
                    styled(b"bat", color4),
                    styled(b" ", standard),
                    styled(b"ziggzagg.zig", color6ul),
                ],
                propt(0, 2),
            );
            {
                let _ = child.print(&[styled("───────┬".as_bytes(), color238)], propt(1, 2));
                if child.width > 10 {
                    for col in 10..child.width {
                        let _ = child.print(&[styled("─".as_bytes(), color238)], propt(1, col));
                    }
                }
            }
            let _ = child.print(
                &[
                    styled("       │ ".as_bytes(), color238),
                    styled(b"File: ", standard),
                    styled(b"ziggzagg.zig", bold),
                ],
                propt(2, 2),
            );
            {
                let _ = child.print(&[styled("───────┼".as_bytes(), color238)], propt(3, 2));
                if child.width > 10 {
                    for col in 10..child.width {
                        let _ = child.print(&[styled("─".as_bytes(), color238)], propt(3, col));
                    }
                }
            }
            let _ = child.print(
                &[
                    styled("   1   │ ".as_bytes(), color238),
                    styled(b"const", color5),
                    styled(b" std ", standard),
                    styled(b"= @import", color5),
                    styled(b"(", standard),
                    styled("\"std\"".as_bytes(), color10),
                    styled(b");", standard),
                ],
                propt(4, 2),
            );
            let _ = child.print(&[styled("   2   │".as_bytes(), color238)], propt(5, 2));
            let _ = child.print(
                &[
                    styled("   3   │ ".as_bytes(), color238),
                    styled(b"pub ", color5),
                    styled(b"fn ", color12),
                    styled(b"main", color2),
                    styled(b"() ", standard),
                    styled(b"!", color5),
                    styled(b"void", color12),
                    styled(b" {", standard),
                ],
                propt(6, 2),
            );
            let _ = child.print(
                &[
                    styled("   4   │     ".as_bytes(), color238),
                    styled(b"const ", color5),
                    styled(b"stdout ", standard),
                    styled(b"=", color5),
                    styled(b" std.Io.getStdOut().writer();", standard),
                ],
                propt(7, 2),
            );
            let _ = child.print(
                &[
                    styled("   5   │     ".as_bytes(), color238),
                    styled(b"var ", color5),
                    styled(b"i:", standard),
                    styled(b" usize", color12),
                    styled(b" =", color5),
                    styled(b" 1", color4),
                    styled(b";", standard),
                ],
                propt(8, 2),
            );
            let _ = child.print(
                &[
                    styled("   6   │     ".as_bytes(), color238),
                    styled(b"while ", color5),
                    styled(b"(i ", standard),
                    styled(b"<= ", color5),
                    styled(b"16", color4),
                    styled(b") : (i ", standard),
                    styled(b"+= ", color5),
                    styled(b"1", color4),
                    styled(b") {", standard),
                ],
                propt(9, 2),
            );
            let _ = child.print(
                &[
                    styled("   7   │         ".as_bytes(), color238),
                    styled(b"if ", color5),
                    styled(b"(i ", standard),
                    styled(b"% ", color5),
                    styled(b"15 ", color4),
                    styled(b"== ", color5),
                    styled(b"0", color4),
                    styled(b") {", standard),
                ],
                propt(10, 2),
            );
            let _ = child.print(
                &[
                    styled("   8   │             ".as_bytes(), color238),
                    styled(b"try ", color5),
                    styled(b"stdout.writeAll(", standard),
                    styled("\"ZiggZagg".as_bytes(), color10),
                    styled("\\n".as_bytes(), color12),
                    styled("\"".as_bytes(), color10),
                    styled(b");", standard),
                ],
                propt(11, 2),
            );
            let _ = child.print(
                &[
                    styled("   9   │         ".as_bytes(), color238),
                    styled(b"} ", standard),
                    styled(b"else if ", color5),
                    styled(b"(i ", standard),
                    styled(b"% ", color5),
                    styled(b"3 ", color4),
                    styled(b"== ", color5),
                    styled(b"0", color4),
                    styled(b") {", standard),
                ],
                propt(12, 2),
            );
            let _ = child.print(
                &[
                    styled("  10   │             ".as_bytes(), color238),
                    styled(b"try ", color5),
                    styled(b"stdout.writeAll(", standard),
                    styled("\"Zigg".as_bytes(), color10),
                    styled("\\n".as_bytes(), color12),
                    styled("\"".as_bytes(), color10),
                    styled(b");", standard),
                ],
                propt(13, 2),
            );
            let _ = child.print(
                &[
                    styled("  11   │         ".as_bytes(), color238),
                    styled(b"} ", standard),
                    styled(b"else if ", color5),
                    styled(b"(i ", standard),
                    styled(b"% ", color5),
                    styled(b"5 ", color4),
                    styled(b"== ", color5),
                    styled(b"0", color4),
                    styled(b") {", standard),
                ],
                propt(14, 2),
            );
            let _ = child.print(
                &[
                    styled("  12   │             ".as_bytes(), color238),
                    styled(b"try ", color5),
                    styled(b"stdout.writeAll(", standard),
                    styled("\"Zagg".as_bytes(), color10),
                    styled("\\n".as_bytes(), color12),
                    styled("\"".as_bytes(), color10),
                    styled(b");", standard),
                ],
                propt(15, 2),
            );
            let _ = child.print(
                &[
                    styled("  13   │         ".as_bytes(), color238),
                    styled(b"} ", standard),
                    styled(b"else ", color5),
                    styled(b"{", standard),
                ],
                propt(16, 2),
            );
            let _ = child.print(
                &[
                    styled("  14   │             ".as_bytes(), color238),
                    styled(b"try ", color5),
                    styled(
                        "stdout.print(\"{d}\\n\", .{i})".as_bytes(),
                        standard_selection,
                    ),
                    styled(b";", cursor),
                ],
                propt(17, 2),
            );
            let _ = child.print(
                &[
                    styled("  15   │         ".as_bytes(), color238),
                    styled(b"}", standard),
                ],
                propt(18, 2),
            );
            let _ = child.print(
                &[
                    styled("  16   │     ".as_bytes(), color238),
                    styled(b"}", standard),
                ],
                propt(19, 2),
            );
            let _ = child.print(
                &[
                    styled("  17   │ ".as_bytes(), color238),
                    styled(b"}", standard),
                ],
                propt(20, 2),
            );
            {
                let _ = child.print(&[styled("───────┴".as_bytes(), color238)], propt(21, 2));
                if child.width > 10 {
                    for col in 10..child.width {
                        let _ = child.print(&[styled("─".as_bytes(), color238)], propt(21, col));
                    }
                }
            }
            let _ = child.print(
                &[
                    styled(b"ghostty ", color6),
                    styled(b"on ", standard),
                    styled(" main ".as_bytes(), color4),
                    styled(b"[+] ", color1),
                    styled(b"via ", standard),
                    styled(" v0.13.0 ".as_bytes(), color3),
                    styled(b"via ", standard),
                    styled("  impure (ghostty-env)".as_bytes(), color4),
                ],
                propt(22, 2),
            );
            let _ = child.print(
                &[
                    styled("✦ ".as_bytes(), color4),
                    styled(b"at ", standard),
                    styled(b"10:36:15 ", color3),
                    styled("→".as_bytes(), color2),
                ],
                propt(23, 2),
            );
            next_start += child.height;
        }

        // if (next_start < win.height) { lorem_ipsum fill }
        if next_start < win.height {
            let child = win.child(ChildOptions {
                x_off: x_off as i32,
                y_off: next_start as i32,
                width: Some(width),
                height: Some(win.height - next_start),
                ..Default::default()
            });
            child.fill(VCell {
                style: standard,
                ..Default::default()
            });
            // var it = std.mem.splitAny(u8, lorem_ipsum, " \n");
            let words = split_any(LOREM_IPSUM, b" \n");
            let mut idx = 0usize;
            // var row: u16 = 1; var col: u16 = 2;
            let mut row: u16 = 1;
            let mut col: u16 = 2;
            // while (row < child.height) {
            while row < child.height {
                // const word = it.next() orelse line: { it.reset(); it.next() orelse unreachable };
                if idx >= words.len() {
                    idx = 0;
                }
                let word = words[idx];
                idx += 1;
                let word_len = word.len() as u16;
                // if (col + word.len > child.width) { row += 1; col = 2; }
                if col as usize + word.len() > child.width as usize {
                    row += 1;
                    col = 2;
                }
                // const style: vaxis.Style = style: { the 9 special-word styles };
                let style = if word == b"ipsum" {
                    Style {
                        fg: color(&config, 2),
                        bg,
                        ..Default::default()
                    }
                } else if word == b"consectetur" {
                    standard_bold
                } else if word == b"reprehenderit" {
                    standard_italic
                } else if word == b"Praesent" {
                    standard_bold_italic
                } else if word == b"auctor" {
                    standard_underline
                } else if word == b"dui" {
                    standard_double_underline
                } else if word == b"erat" {
                    standard_dashed_underline
                } else if word == b"enim" {
                    standard_dotted_underline
                } else if word == b"odio" {
                    standard_curly_underline
                } else {
                    standard
                };
                let text = arena.dupe(word);
                let _ = child.print_segment(styled(text, style), propt(row, col));
                // col += word_len + 1;
                col += word_len + 1;
            }
        }
    } else {
        // else => the full-window "No theme found!" centered pane
        let child = win.child(ChildOptions {
            x_off: 0,
            y_off: 0,
            width: Some(win.width),
            height: Some(win.height),
            ..Default::default()
        });
        child.fill(VCell {
            style: ui_standard(scheme),
            ..Default::default()
        });
        let _ = child.print_segment(
            styled("No theme found!".as_bytes(), ui_standard(scheme)),
            propt(win.height / 2 - 1, win.width / 2 - 7),
        );
    }
}

// `TerminalColor::Color(c) => rgb`; cell-* arms → None (missing-value fallback, §1).
fn term_color_rgb(tc: &TerminalColor) -> Option<Color> {
    match tc {
        TerminalColor::Color(c) => Some(Color::Rgb([c.r, c.g, c.b])),
        _ => None,
    }
}

// `std.mem.splitAny(u8, buf, delims)` — split on ANY delimiter byte, keeping empties.
fn split_any<'b>(buf: &'b [u8], delims: &[u8]) -> Vec<&'b [u8]> {
    let mut out: Vec<&[u8]> = Vec::new();
    let mut start = 0usize;
    for (i, &b) in buf.iter().enumerate() {
        if delims.contains(&b) {
            out.push(&buf[start..i]);
            start = i + 1;
        }
    }
    out.push(&buf[start..]);
    out
}

// fn resolveAutoThemePath(alloc) ![]u8 {
fn resolve_auto_theme_path() -> Result<Vec<u8>, Error> {
    // const main_cfg_path = try configpkg.preferredDefaultFilePath(alloc);
    let main_cfg_path = rustty_config::file_load::preferred_default_file_path()?;
    // const base_dir = std.fs.path.dirname(main_cfg_path) orelse return error.BadPathName;
    let base_dir = std::path::Path::new(&main_cfg_path)
        .parent()
        .ok_or_else(|| std::io::Error::other("BadPathName"))?;
    // return try std.fs.path.join(alloc, &.{ base_dir, "auto", "theme.ghostty" });
    let base_bytes = path_bytes(base_dir);
    Ok(join(&[&base_bytes, b"auto", b"theme.ghostty"]))
}

// fn writeAutoThemeFile(alloc, theme_name: []const u8) !void {
fn write_auto_theme_file(theme_name: &[u8]) -> Result<(), Error> {
    // const auto_path = try resolveAutoThemePath(alloc);
    let auto_path = resolve_auto_theme_path()?;
    // if (std.fs.path.dirname(auto_path)) |dir| try std.fs.cwd().makePath(dir);
    let p = std::path::Path::new(os_str_from_bytes(&auto_path));
    if let Some(dir) = p.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // var f = try std.fs.createFileAbsolute(auto_path, .{ .truncate = true });
    let mut f = std::fs::File::create(os_str_from_bytes(&auto_path))?;
    // try w.interface.print("theme = {s}\n", .{theme_name});
    f.write_all(b"theme = ")?;
    f.write_all(theme_name)?;
    f.write_all(b"\n")?;
    Ok(())
}

// fn saveSelectedTheme(self: *Preview) void {
fn save_selected_theme(st: &DrawState) {
    // if (self.filtered.items.len == 0) return;
    if st.filtered.is_empty() {
        return;
    }
    // const idx = self.filtered.items[self.current]; const theme = self.themes[idx];
    let theme = &st.themes[st.filtered[st.current]];
    // writeAutoThemeFile(self.allocator, theme.theme) catch { return; };
    let _ = write_auto_theme_file(&theme.theme);
}

// `&Path` → raw bytes (POSIX byte-faithful; the parity harness targets POSIX).
#[cfg(unix)]
fn path_bytes(p: &std::path::Path) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    p.as_os_str().as_bytes().to_vec()
}

#[cfg(not(unix))]
fn path_bytes(p: &std::path::Path) -> Vec<u8> {
    p.to_string_lossy().as_bytes().to_vec()
}

// ---- updateFiltered — the zf-ranking pass (§6) ----
//
// PURE kernel: two branches keyed on `has_query` (= real_length > 0, NOT
// token-count — the all-spaces edge, issue 5):
//  has_query == true : tokens ← lowercased(buffer) split on ' ' (empty runs
//    skipped, so an ALL-SPACES buffer yields ZERO tokens but STILL takes this
//    branch); per theme keep i iff color_pass[i] && zf::rank(name, &tokens,
//    {to_lower, plain}).is_some() — `zf::rank` IS CALLED even when tokens is empty.
//  has_query == false: per theme keep i iff color_pass[i]; ranks_out[i] = None.
// Byte-faithful (`&[u8]`, never `String`).
fn rank_filtered(
    theme_names: &[&[u8]],
    buffer: &[u8],
    has_query: bool,
    color_pass: &[bool],
    ranks_out: &mut [Option<f64>],
) -> Vec<usize> {
    let mut filtered: Vec<usize> = Vec::new();
    if has_query {
        // const string = try std.ascii.allocLowerString(allocator, buffer);
        let mut string = buffer.to_vec();
        string.make_ascii_lowercase();
        // var it = std.mem.tokenizeScalar(u8, string, ' ');  (skips empty runs)
        let tokens: Vec<&[u8]> = string
            .split(|&b| b == b' ')
            .filter(|t| !t.is_empty())
            .collect();
        // for (self.themes, 0..) |*theme, i| { if (!shouldIncludeTheme) continue; rank; if rank!=null append; }
        for (i, name) in theme_names.iter().enumerate() {
            if !color_pass[i] {
                continue;
            }
            let r = zf_rank(
                name,
                &tokens,
                RankOptions {
                    to_lower: true,
                    plain: true,
                },
            );
            ranks_out[i] = r;
            if r.is_some() {
                filtered.push(i);
            }
        }
    } else {
        // for (self.themes, 0..) |*theme, i| { if (shouldIncludeTheme) { append; rank=null; } }
        for (i, _name) in theme_names.iter().enumerate() {
            if color_pass[i] {
                filtered.push(i);
                ranks_out[i] = None;
            }
        }
    }
    filtered
}

// The reselect walk (§6) — pure integer/byte math: given the rebuilt filtered, the
// pre-rebuild selected name + relative offset, returns the new (current, window).
fn reselect(
    filtered: &[usize],
    names: &[&[u8]],
    selected: Option<&[u8]>,
    relative: usize,
) -> (usize, usize) {
    // if (selected.len == 0) break :current .{ 0, 0 };
    //   PORT NOTE: the reference reads an `undefined` `selected` when the pre-rebuild
    //   filtered was empty; the port models "no prior selection" as `None` → (0, 0)
    //   (the benign resolution the reference falls into on init).
    let Some(sel) = selected else {
        return (0, 0);
    };
    // for (self.filtered.items, 0..) |index, i| {
    for (i, &index) in filtered.iter().enumerate() {
        // if (std.mem.eql(u8, self.themes[index].theme, selected)) {
        if names[index] == sel {
            // Keep the relative position but ensure all search results are visible
            let new_window = i.saturating_sub(relative);
            // If the new window would hide some results at the top, adjust it
            let win = if filtered.len() <= SMALL_LIST_THRESHOLD {
                0
            } else {
                new_window
            };
            return (i, win);
        }
    }
    // break :current .{ 0, 0 };
    (0, 0)
}

// fn updateFiltered(self: *Preview) !void {
//   Impure wrapper: computes `color_pass` via ONE accumulating `Config` (issue 2),
//   the `has_query`/`buffer`, calls the pure kernel, writes back `theme.rank`, then
//   applies the digest short-circuit + reselect. Loads propagate (reference `try`).
fn update_filtered(st: &mut DrawState) -> Result<(), Error> {
    // const relative = self.current -| self.window;
    let relative = st.current.saturating_sub(st.window);
    // if (self.filtered.items.len > 0) selected = self.themes[self.filtered.items[self.current]].theme;
    let selected: Option<Vec<u8>> = if !st.filtered.is_empty() {
        Some(st.themes[st.filtered[st.current]].theme.clone())
    } else {
        None
    };

    let themes = st.themes;

    // old_digest → snapshot for the equal-set short-circuit (Wyhash → Vec compare;
    //   byte-observably identical — the digest is only ever compared for equality).
    let old_filtered = st.filtered.clone();
    // self.filtered.clearRetainingCapacity();
    st.filtered.clear();

    // color_pass via ONE accumulating theme_config (issue 2, FIDELITY): no per-theme
    // reset — a theme omitting `background` inherits the previous theme's, exactly as
    // the reference `run`/`updateFiltered` do (single `Config.default` + repeated
    // `loadFile`).
    let mut theme_config = Config::default_config();
    let mut color_pass: Vec<bool> = vec![false; themes.len()];
    for (i, t) in themes.iter().enumerate() {
        // try theme_config.loadFile(theme_config._arena.?.allocator(), theme.path);
        theme_config.load_file(&t.path)?;
        color_pass[i] = should_include_theme(st.theme_filter, &theme_config);
    }

    // has_query = self.text_input.buf.realLength() > 0
    let has_query = st.text_input.buf.real_length() > 0;
    // buffer = first_half ++ second_half
    let mut buffer: Vec<u8> = Vec::new();
    buffer.extend_from_slice(st.text_input.buf.first_half());
    buffer.extend_from_slice(st.text_input.buf.second_half());

    let theme_names: Vec<&[u8]> = themes.iter().map(|t| t.theme.as_slice()).collect();
    let mut ranks_out: Vec<Option<f64>> = themes.iter().map(|t| t.rank.get()).collect();
    let new_filtered = rank_filtered(
        &theme_names,
        &buffer,
        has_query,
        &color_pass,
        &mut ranks_out,
    );
    // theme.rank = ... (write back through the interior-mutable Cell)
    for (i, t) in themes.iter().enumerate() {
        t.rank.set(ranks_out[i]);
    }
    st.filtered = new_filtered;

    // if (old_digest == new_digest) return;
    if old_filtered == st.filtered {
        return Ok(());
    }
    // if (self.filtered.items.len == 0) { current=0; window=0; return; }
    if st.filtered.is_empty() {
        st.current = 0;
        st.window = 0;
        return Ok(());
    }
    // self.current, self.window = current: { ... };
    let (cur, win) = reselect(&st.filtered, &theme_names, selected.as_deref(), relative);
    st.current = cur;
    st.window = win;
    Ok(())
}

// ===========================================================================
// Tests — the PURE kernels (Miri-clean) + a render-to-`Vec<u8>` draw smoke; the
// interactive Loop path is `#[ignore]`-on-device (needs a real `/dev/tty`).
// ===========================================================================
#[cfg(test)]
mod picker_tests {
    use super::*;

    fn te(name: &[u8], path: &[u8]) -> ThemeListElement {
        ThemeListElement {
            location: Location::User,
            path: path.to_vec(),
            theme: name.to_vec(),
            rank: std::cell::Cell::new(None),
        }
    }

    // rank_filtered inclusion + all-spaces edge (issue 5) + color gate.
    #[test]
    fn rank_filtered_inclusion() {
        let names: [&[u8]; 3] = [b"Solarized Dark", b"Gruvbox", b"Nord"];
        let pass = [true, true, true];
        let mut ranks = [None; 3];

        // has_query, buffer "grv" → only Gruvbox fuzzy-matches.
        let f = rank_filtered(&names, b"grv", true, &pass, &mut ranks);
        assert_eq!(f, vec![1]);

        // has_query=false → include all color-passing themes.
        let mut ranks2 = [None; 3];
        let f2 = rank_filtered(&names, b"", false, &pass, &mut ranks2);
        assert_eq!(f2, vec![0, 1, 2]);

        // buffer "zzz" → no match.
        let mut ranks3 = [None; 3];
        let f3 = rank_filtered(&names, b"zzz", true, &pass, &mut ranks3);
        assert!(f3.is_empty());
    }

    // all-spaces query MUST route through `zf::rank(name, &[], opts)` (has_query
    // branch), NOT the include-all `else` — pins the branch keys on `has_query`.
    #[test]
    fn rank_filtered_all_spaces() {
        let names: [&[u8]; 2] = [b"Alpha", b"Beta"];
        let pass = [true, true];
        let mut ranks = [None; 2];
        let f = rank_filtered(&names, b"   ", true, &pass, &mut ranks);

        // explicit reference: zf::rank(name, &[], {to_lower, plain}) for each name.
        let expect: Vec<usize> = names
            .iter()
            .enumerate()
            .filter(|(_, n)| {
                zf_rank(
                    n,
                    &[],
                    RankOptions {
                        to_lower: true,
                        plain: true,
                    },
                )
                .is_some()
            })
            .map(|(i, _)| i)
            .collect();
        assert_eq!(f, expect);
    }

    // color gate drops the masked index.
    #[test]
    fn rank_filtered_color_gate() {
        let names: [&[u8]; 3] = [b"A", b"B", b"C"];
        let pass = [true, false, true];
        let mut ranks = [None; 3];
        let f = rank_filtered(&names, b"", false, &pass, &mut ranks);
        assert_eq!(f, vec![0, 2]);
    }

    // rank ordering: a better (contiguous) match ranks lower — pins WIRE2 consumes
    // zf's exact ranks. "Gruvbox" ranks better for "gru" than "Gruvbox Material".
    #[test]
    fn rank_filtered_ordering() {
        let names: [&[u8]; 2] = [b"Gruvbox", b"Gruvbox Material Dark"];
        let pass = [true, true];
        let mut ranks = [None; 2];
        let f = rank_filtered(&names, b"gru", true, &pass, &mut ranks);
        assert_eq!(f, vec![0, 1]);
        // both matched; the shorter/tighter candidate has the lower (better) rank.
        assert!(ranks[0].unwrap() <= ranks[1].unwrap());
    }

    // reselect: SMALL_LIST_THRESHOLD reset + selected-preservation walk.
    #[test]
    fn reselect_pins() {
        let names: [&[u8]; 3] = [b"A", b"B", b"C"];
        // small list (<= threshold) → window resets to 0, current tracks selected.
        let (cur, win) = reselect(&[0, 1, 2], &names, Some(b"B"), 1);
        assert_eq!((cur, win), (1, 0));
        // no prior selection → (0, 0).
        let (cur2, win2) = reselect(&[0, 1, 2], &names, None, 0);
        assert_eq!((cur2, win2), (0, 0));
        // selected not present → (0, 0).
        let (cur3, win3) = reselect(&[0, 1], &names, Some(b"C"), 0);
        assert_eq!((cur3, win3), (0, 0));
    }

    // A large list keeps the relative window (no SMALL_LIST_THRESHOLD reset).
    #[test]
    fn reselect_large_list_keeps_window() {
        let names: Vec<&[u8]> = (0..20u8).map(|_| b"x".as_slice()).collect();
        // 20 filtered > SMALL_LIST_THRESHOLD (10); selected at index 15, relative 3
        // → window = 15 - 3 = 12.
        let filtered: Vec<usize> = (0..20).collect();
        // make index 15's name distinct so it is found
        let mut nm: Vec<&[u8]> = names.clone();
        nm[15] = b"target";
        let (cur, win) = reselect(&filtered, &nm, Some(b"target"), 3);
        assert_eq!((cur, win), (15, 12));
    }

    // Build a headless Vaxis (no Tty) for the render smoke.
    fn smoke_vx<'a>() -> Vaxis<'a> {
        let mut vx = Vaxis::init(VaxisOptions::default());
        let mut sink: Vec<u8> = Vec::new();
        vx.resize(
            &mut sink,
            Winsize {
                rows: 40,
                cols: 120,
                x_pixel: 0,
                y_pixel: 0,
            },
        )
        .unwrap();
        vx
    }

    fn smoke_state<'a>(
        arena: &'a Arena,
        themes: &'a [ThemeListElement],
        mode: Mode,
    ) -> DrawState<'a> {
        DrawState {
            should_quit: false,
            mouse: None,
            themes,
            arena,
            filtered: (0..themes.len()).collect(),
            current: 0,
            window: 0,
            hex: false,
            mode,
            color_scheme: Scheme::Light,
            text_input: TextInput::init(),
            theme_filter: ColorScheme::All,
        }
    }

    // draw smoke: empty filtered → "No theme found!" pane.
    #[test]
    fn draw_smoke_no_theme() {
        let arena = Arena::init();
        let themes: Vec<ThemeListElement> = Vec::new();
        let mut vx = smoke_vx();
        let mut st = smoke_state(&arena, &themes, Mode::Normal);
        st.filtered.clear();
        draw(&mut vx, &mut st);
        let mut out: Vec<u8> = Vec::new();
        vx.render(&mut out).unwrap();
        assert!(
            find_bytes(&out, b"No theme found!"),
            "expected the No-theme pane"
        );
    }

    // draw smoke: a bogus theme path → drawPreview loadFile Err → "Unable to open".
    // (loads a config file → real FS; Miri isolation blocks `File::open`, so
    // `#[cfg_attr(miri, ignore)]` — the pure kernels + the no-file draw smokes stay
    // Miri-covered.)
    #[test]
    #[cfg_attr(miri, ignore = "opens a file — Miri isolation")]
    fn draw_smoke_unable_to_open() {
        let arena = Arena::init();
        let themes = vec![te(b"Bogus", b"/nonexistent/path/to/theme.zzz")];
        let mut vx = smoke_vx();
        let mut st = smoke_state(&arena, &themes, Mode::Normal);
        draw(&mut vx, &mut st);
        let mut out: Vec<u8> = Vec::new();
        vx.render(&mut out).unwrap();
        assert!(
            find_bytes(&out, b"Unable to open"),
            "expected the error pane"
        );
        assert!(find_bytes(&out, b"Bogus"), "expected the theme name");
    }

    // draw smoke: Help mode renders a key_help entry. (loads a config file for the
    // preview pane → Miri isolation; ignored under Miri.)
    #[test]
    #[cfg_attr(miri, ignore = "opens a file — Miri isolation")]
    fn draw_smoke_help() {
        let arena = Arena::init();
        let themes = vec![te(b"Bogus", b"/nonexistent/theme.zzz")];
        let mut vx = smoke_vx();
        let mut st = smoke_state(&arena, &themes, Mode::Help);
        draw(&mut vx, &mut st);
        let mut out: Vec<u8> = Vec::new();
        vx.render(&mut out).unwrap();
        assert!(
            find_bytes(&out, b"Toggle help window."),
            "expected a key_help row"
        );
    }

    // draw smoke: empty filtered + Save mode renders without panicking (issue-4 guard).
    #[test]
    fn draw_smoke_save_empty_guard() {
        let arena = Arena::init();
        let themes: Vec<ThemeListElement> = Vec::new();
        let mut vx = smoke_vx();
        let mut st = smoke_state(&arena, &themes, Mode::Save);
        st.filtered.clear();
        draw(&mut vx, &mut st);
        let mut out: Vec<u8> = Vec::new();
        vx.render(&mut out).unwrap();
        // No theme found in the preview pane; the save pane rendered its instructions.
        assert!(find_bytes(&out, b"Ghostty configuration"));
    }

    fn find_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        haystack.windows(needle.len()).any(|w| w == needle)
    }

    // uri_from_path: ASCII path → verbatim `file://` prefix; a space is `%20`.
    #[test]
    fn to_uri_smoke() {
        assert_eq!(
            uri_from_path(b"/a/b.ghostty"),
            b"file:///a/b.ghostty".to_vec()
        );
        assert_eq!(uri_from_path(b"/a b"), b"file:///a%20b".to_vec());
    }

    // Interactive Loop path — needs a real `/dev/tty` (absent in the sandbox).
    // `Tty = PosixTty` in cli's dep graph, so `Tty::init` opens `/dev/tty`.
    #[test]
    #[ignore = "opens /dev/tty — device-only"]
    fn preview_run_smoke() {
        let themes = vec![te(b"Bogus", b"/nonexistent/theme.zzz")];
        preview(&themes, ColorScheme::All).unwrap();
    }
}
