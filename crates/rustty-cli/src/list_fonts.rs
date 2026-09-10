//! rustty-cli::list_fonts
//!
//! PORTS: ghostty src/cli/list_fonts.zig (0t) — the `+list-fonts` action.
//!
//! Discovery is fully ported in `rustty-font-shaper` (the D1 shaper tier ABOVE
//! `rustty-vt`+`rustty-font`): on macOS `font::Discover` = the CoreText
//! discoverer (`discovery.rs`), `font::Library` = `NoopLibrary`; on Linux/BSD
//! `font::Discover` = the Fontconfig discoverer, `font::Library` =
//! `FreetypeLibrary`. In both cases the `DeferredFace::{family_name, name}`
//! accessors read the discovered pattern/face. This is the real port, NOT a stub.
//! The reference's `font.Discover == void` arm maps ONLY to the wasm `web_canvas`
//! backend (no enumerable font list in the browser).

// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const ArenaAllocator = std.heap.ArenaAllocator;
//   PORT NOTE: the reference threads an arena for all the per-run allocations.
//   Rustty uses owned `Vec`/`HashMap` values on the global allocator (RAII), so
//   there is no arena to create; the `_arena` bookkeeping is dropped tree-wide.
// const Action = @import("ghostty.zig").Action;
use crate::rustty::{Action, Error};
// const args = @import("args.zig");
use rustty_args::args;
// const font = @import("../font/main.zig");
//   -> the shaper tier's `font.*` re-export surface (`Library`/`Discover`/
//   `Descriptor`/`DeferredFace`).
use rustty_font_shaper as font;

use std::io::Write;

// const log = std.log.scoped(.list_fonts);
//
// PORT NOTE: the reference's scope tag, carried through the `log` facade as
// `target: LOG_SCOPE` (DECISION-logging.md §6r4).
const LOG_SCOPE: &str = "list_fonts";
//   PORT NOTE: no logging facade is wired into this crate; the two `log.err`
//   calls (on `familyName`/`name` failures) are ported as verbatim comments and
//   are no-ops, matching the discovery.rs `log.*`-as-comment convention.

// pub const Options = struct {
#[derive(Default)]
pub struct Options {
    /// This is set by the CLI parser for deinit.
    // _arena: ?ArenaAllocator = null,
    //   PORT NOTE: dropped (owned fields free on drop; the reference `deinit`
    //   only tears down `_arena`).

    /// The font family to search for. If this is set, then only fonts
    /// matching this family will be listed.
    // family: ?[:0]const u8 = null,
    pub family: Option<Vec<u8>>,

    /// The style name to search for.
    // style: ?[:0]const u8 = null,
    pub style: Option<Vec<u8>>,

    /// Font styles to search for. If this is set, then only fonts that
    /// match the given styles will be listed.
    // bold: bool = false,
    pub bold: bool,
    // italic: bool = false,
    pub italic: bool,
}

impl Options {
    // pub fn deinit(self: *Options) void {
    //     if (self._arena) |arena| arena.deinit();
    //     self.* = undefined;
    // }
    //   PORT NOTE: owned fields free on drop; no `_arena`.
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
            // family: ?[:0]const u8
            b"family" => {
                // For optional fields, a set-but-empty value resets to the
                // default (null) — the reference `parseIntoField` empty branch.
                if let Some(v) = value {
                    if v.is_empty() {
                        self.family = None;
                        return Ok(());
                    }
                }
                self.family = Some(args::parse_field_sentinel_string(value)?);
                Ok(())
            }
            // style: ?[:0]const u8
            b"style" => {
                if let Some(v) = value {
                    if v.is_empty() {
                        self.style = None;
                        return Ok(());
                    }
                }
                self.style = Some(args::parse_field_sentinel_string(value)?);
                Ok(())
            }
            // bold: bool
            b"bold" => {
                self.bold = args::parse_field_bool(value)?;
                Ok(())
            }
            // italic: bool
            b"italic" => {
                self.italic = args::parse_field_bool(value)?;
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
        let _ = Action::Help;
        Some(Err(args::Error::ActionHelpRequested))
    }
}

/// The `list-fonts` command is used to list all the available fonts for
/// Rustty. This uses the exact same font discovery mechanism Rustty uses to
/// find fonts to use.
///
/// When executed with no arguments, this will list all available fonts, sorted
/// by family name, then font name. If a family name is given with `--family`,
/// the sorting will be disabled and the results instead will be shown in the
/// same priority order Rustty would use to pick a font.
///
/// Flags:
///
///   * `--bold`: Filter results to specific bold styles. It is not guaranteed
///     that only those styles are returned. They are only prioritized.
///
///   * `--italic`: Filter results to specific italic styles. It is not guaranteed
///     that only those styles are returned. They are only prioritized.
///
///   * `--style`: Filter results based on the style string advertised by a font.
///     It is not guaranteed that only those styles are returned. They are only
///     prioritized.
///
///   * `--family`: Filter results to a specific font family. The family handling
///     is identical to the `font-family` set of Rustty configuration values, so
///     this can be used to debug why your desired font may not be loading.
// pub fn run(alloc: Allocator) !u8 {
pub fn run() -> Result<u8, Error> {
    // var iter = try args.argsIterator(alloc);
    let mut iter = args::args_iterator()?;
    // defer iter.deinit();
    // return try runArgs(alloc, &iter);
    let result = run_args(&mut iter);
    iter.deinit();
    result
}

// fn runArgs(alloc_gpa: Allocator, argsIter: anytype) !u8 {
fn run_args<I>(args_iter: &mut I) -> Result<u8, Error>
where
    I: args::ArgIterator + rustty_args::diagnostics::LocationProvider,
{
    // var config: Options = .{};
    // defer config.deinit();
    let mut config = Options::default();
    // try args.parse(Options, alloc_gpa, &config, argsIter);
    args::parse(&mut config, args_iter)?;

    // Use an arena for all our memory allocs
    // var arena = ArenaAllocator.init(alloc_gpa);
    // defer arena.deinit();
    // const alloc = arena.allocator();
    //   PORT NOTE: dropped — owned `Vec`/`HashMap` on the global allocator (RAII).

    // Its possible to build Ghostty without font discovery!
    // if (comptime font.Discover == void) { ...print...; return 1; }
    //
    // PORT NOTE: rustty's `font::Discover` is a real discoverer on every native
    // target — CoreText on macOS, Fontconfig on Linux/BSD (both fully ported in
    // the shaper's `discovery.rs`) — so this arm is skipped there and the live
    // discovery body below runs. The `font.Discover == void` arm maps ONLY to the
    // wasm `web_canvas` backend, whose browser has no enumerable font list.
    #[cfg(target_arch = "wasm32")]
    {
        let _ = &config;
        // var buffer: [1024]u8 = undefined;
        // var stderr_writer = std.fs.File.stderr().writer(&buffer);
        // const stderr = &stderr_writer.interface;
        let mut stderr = std::io::BufWriter::new(std::io::stderr().lock());
        // try stderr.print(
        //     \\Ghostty was built without a font discovery mechanism. ...
        // , .{});
        //   R2: the product name routes through rustty-identity. Oracle prints
        //   the capitalized product name at both positions here.
        write!(
            stderr,
            "{app} was built without a font discovery mechanism. This is a compile-time\n\
             option. Please review how {app} was built from source, contact the\n\
             maintainer to enable a font discovery mechanism, and try again.",
            app = rustty_identity::APP_NAME_TITLECASE,
        )?;
        // try stderr.flush();
        stderr.flush()?;
        config.deinit();
        // return 1;
        return Ok(1);
    }

    #[cfg(not(target_arch = "wasm32"))]
    {
        // Look up all available fonts. The library is only used by backends
        // that need it (the Windows backend opens candidate font files with
        // FreeType); other backends ignore it.
        // var font_lib = try font.Library.init(alloc);
        //   PORT NOTE: on macOS `font::Library` = `NoopLibrary` whose `init` error
        //   set is uninhabited (the reference `error{}` arm); `?` discharges it via
        //   `match e {}`. On Linux/BSD `font::Library` = `FreetypeLibrary` whose
        //   `init` returns a real `ft::FtError` — the reference `try` propagates it
        //   (folded into `Error::FontLibrary`).
        #[cfg(target_os = "macos")]
        let mut font_lib = font::Library::init().unwrap_or_else(|e| match e {});
        #[cfg(not(target_os = "macos"))]
        let mut font_lib = font::Library::init()?;
        // defer font_lib.deinit();
        // var disco = font.Discover.init(font_lib);
        let mut disco = font::Discover::init(font_lib);
        // defer disco.deinit();
        //
        // PORT NOTE (W9F-02): the reference `defer`s run on EVERY exit,
        // including the error returns, and these owners hold raw native
        // handles (on the Fontconfig backend) that only manual `deinit` frees.
        // Rust has no defer, so the fallible body runs in immediately-invoked
        // closures and each owner's `deinit` runs unconditionally on the way
        // out, whatever the result.
        let result = (|| -> Result<u8, Error> {
            // var disco_it = try disco.discover(alloc, .{
            //     .family = config.family,
            //     .style = config.style,
            //     .bold = config.bold,
            //     .italic = config.italic,
            //     .monospace = config.family == null,
            // });
            let mut disco_it = disco.discover(font::Descriptor {
                family: config.family.clone(),
                style: config.style.clone(),
                bold: config.bold,
                italic: config.italic,
                monospace: config.family.is_none(),
                ..font::Descriptor::default()
            })?;
            // defer disco_it.deinit(); — see the W9F-02 PORT NOTE above.
            let result = run_discovery(&config, &mut disco_it);
            disco_it.deinit();
            result
        })();
        disco.deinit();
        font_lib.deinit();
        config.deinit();

        // return 0; — (`run_discovery`'s `Ok(0)` success value flows out here,
        // after the materialized `defer`s above.)
        result
    }
}

/// The body of `runArgs` between the reference's `disco_it` creation and the
/// final `return 0` — split out (W9F-02) so `run_args` can run the trailing
/// `deinit`s unconditionally, materializing the reference `defer`s on the error
/// paths too.
#[cfg(not(target_arch = "wasm32"))]
fn run_discovery(
    config: &Options,
    disco_it: &mut font::discovery::DiscoverIterator,
) -> Result<u8, Error> {
    use std::collections::HashMap;

    // var buffer: [2048]u8 = undefined;
    // var stdout_writer = std.fs.File.stdout().writer(&buffer);
    // const stdout = &stdout_writer.interface;
    let mut stdout = std::io::BufWriter::new(std::io::stdout().lock());

    // We'll be putting our fonts into a list categorized by family
    // so it is easier to read the output.
    // var families: std.ArrayList([]const u8) = .empty;
    let mut families: Vec<Vec<u8>> = Vec::new();
    // var map: std.StringHashMap(std.ArrayListUnmanaged([]const u8)) = .init(alloc);
    let mut map: HashMap<Vec<u8>, Vec<Vec<u8>>> = HashMap::new();

    {
        // while (try disco_it.next()) |face| {
        while let Some(face) = disco_it.next()? {
            // var buf: [1024]u8 = undefined;
            let mut buf = [0u8; 1024];

            // const family_buf = face.familyName(&buf) catch |err| {
            //     log.err("failed to get font family name: {}", .{err});
            //     continue;
            // };
            let family_buf = match face.family_name(&mut buf) {
                Ok(name) => name,
                Err(err) => {
                    // log.err("failed to get font family name: {}", .{err});
                    //
                    // PORT NOTE (N11): Zig's `{}` on an error renders
                    // `error.<Name>`; `{err:?}` renders the Rust error's own
                    // Debug. Workspace-dominant rendering; N11's scope.
                    log::error!(target: LOG_SCOPE, "failed to get font family name: {err:?}");
                    continue;
                }
            };
            // const family = try alloc.dupe(u8, family_buf);
            let family = family_buf.to_vec();

            // const full_name_buf = face.name(&buf) catch |err| {
            //     log.err("failed to get font name: {}", .{err});
            //     continue;
            // };
            let full_name_buf = match face.name(&mut buf) {
                Ok(name) => name,
                Err(err) => {
                    // log.err("failed to get font name: {}", .{err});
                    log::error!(target: LOG_SCOPE, "failed to get font name: {err:?}");
                    continue;
                }
            };
            // const full_name = try alloc.dupe(u8, full_name_buf);
            let full_name = full_name_buf.to_vec();

            // const gop = try map.getOrPut(family);
            // if (!gop.found_existing) {
            //     try families.append(alloc, family);
            //     gop.value_ptr.* = .{};
            // }
            // try gop.value_ptr.append(alloc, full_name);
            //   PORT NOTE: `getOrPut` preserves the reference's ordering — `families`
            //   records first-seen order; the per-family list appends full names.
            if !map.contains_key(&family) {
                families.push(family.clone());
                map.insert(family.clone(), Vec::new());
            }
            map.get_mut(&family).unwrap().push(full_name);
        }

        // Sort our keys.
        // if (config.family == null) {
        //     std.mem.sortUnstable([]const u8, families.items, {}, ...lessThan...);
        // }
        //   `std.mem.order(u8, lhs, rhs) == .lt` is byte-lexicographic order, which
        //   is exactly `Vec<u8>`'s `Ord`.
        if config.family.is_none() {
            families.sort_unstable();
        }

        // Output each
        // for (families.items) |family| {
        for family in &families {
            // const list = map.get(family) orelse continue;
            let list = match map.get_mut(family) {
                Some(list) => list,
                None => continue,
            };
            // if (list.items.len == 0) continue;
            if list.is_empty() {
                continue;
            }
            // if (config.family == null) {
            //     std.mem.sortUnstable([]const u8, list.items, {}, ...lessThan...);
            // }
            if config.family.is_none() {
                list.sort_unstable();
            }

            // try stdout.print("{s}\n", .{family});
            stdout.write_all(family)?;
            stdout.write_all(b"\n")?;
            // for (list.items) |item| try stdout.print("  {s}\n", .{item});
            for item in list.iter() {
                stdout.write_all(b"  ")?;
                stdout.write_all(item)?;
                stdout.write_all(b"\n")?;
            }
            // try stdout.print("\n", .{});
            stdout.write_all(b"\n")?;
        }

        // try stdout.flush();
        stdout.flush()?;

        // defer disco_it.deinit(); / defer disco.deinit(); / defer
        // font_lib.deinit(); / defer config.deinit(); — the reference's trailing
        // `defer`s now run in `run_args` on EVERY exit (W9F-02), so they are no
        // longer duplicated on this success path.

        // return 0;
        Ok(0)
    }
}

#[cfg(test)]
mod log_site_tests {
    //! task-14e-2 / S68-F4 — `cli/list_fonts.zig:123,129`.
    //!
    //! STRUCTURAL, and deliberately so. Both diagnostics sit on
    //! `DeferredFace::{familyName,name}` failure arms inside a loop driven by a
    //! live `font::discovery::DiscoverIterator`. On the CoreText path those
    //! calls fail only when the font's own name exceeds the reference's
    //! `[1024]u8` buffer, and the iterator yields faces from the SYSTEM font
    //! collection — there is no seam to inject one. Lifting the loop body the
    //! way `rustty.rs`'s `write_help_text` was lifted would not help either:
    //! the failure is in the face, not in a writer the caller supplies.
    //!
    //! So this test pins what it can actually observe: that the two emissions
    //! exist, at the reference's level, under the reference's scope, with the
    //! reference's message text. It is a weaker statement than a behavioural
    //! test and is reported as such.
    //!
    //! The macro/level tokens are ASSEMBLED FROM FRAGMENTS rather than written
    //! as literals: a literal `log::error!` in this file would be a live-looking
    //! facade call to `check_log_reconcile.py`'s scanner and to its LIVE-TREE
    //! self-test arm (the lesson task-14e-1 recorded after doing exactly that).

    const SRC: &str = include_str!("list_fonts.rs");

    fn emission(message: &str) -> String {
        let macro_name = ["log", "::", "error", "!"].concat();
        format!("{macro_name}(target: LOG_SCOPE, \"{message}\")")
    }

    #[test]
    fn both_reference_diagnostics_are_live_at_the_reference_level_and_scope() {
        // `const log = std.log.scoped(.list_fonts);`
        assert_eq!(
            1,
            SRC.matches("const LOG_SCOPE: &str = \"list_fonts\";")
                .count(),
            "the reference's scope tag is not declared exactly once"
        );

        // log.err("failed to get font family name: {}", .{err});
        // log.err("failed to get font name: {}", .{err});
        for message in [
            "failed to get font family name: {err:?}",
            "failed to get font name: {err:?}",
        ] {
            let wanted = emission(message);
            assert_eq!(
                1,
                SRC.matches(&wanted).count(),
                "expected exactly one live emission `{wanted}`"
            );
        }
    }
}
