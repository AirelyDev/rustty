//! rustty-config::formatter_file
//!
//! PORTS: ghostty src/config/formatter_file.zig
//!
//! PORT STATUS: ported
//!
//! The whole-config FORMAT pass: `FileFormatter`, which formats an entire
//! `Config` in the file-like format by walking every public field and calling
//! the per-field `formatEntry` dispatch (`Config::format_entry_field`, the port
//! of `formatter.formatEntry(field.type, field.name, value, writer)`). Every
//! line/comment/`test {}` is ported (R1); retargeted only for product identity
//! (R2).
//
// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const Config = @import("Config.zig");
// const Key = @import("key.zig").Key;
// const help_strings = @import("help_strings");
// const formatter = @import("formatter.zig");
//
// PORT NOTE: `formatter.formatEntry(field.type, field.name, value, writer)` is the
// generic comptime dispatch; its per-field application over the `Config` fields is
// `Config::format_entry_field` (config_struct.rs). The `Allocator`/`Config.default`
// machinery the reference threads for change-tracking is the `Config::default()`
// data value + the `Config::changed` per-`Key` dispatch.

// IMPORTANT: This is in a separate file from formatter.zig because it
// puts a build-time dependency on Config.zig which brings in too much
// into libghostty-vt tests which reference some formattable types.
//
// PORT NOTE (R4): the reference's file-split rationale — keeping `Config` out of
// the leaf `formatter.zig` so libghostty-vt tests don't pull it in — is realized
// structurally by the crate split: `formatter.zig` is `rustty-cfg-fmt` (a
// near-leaf), and this `FileFormatter` (which needs `Config`) lives in
// `rustty-config`. Same intent, enforced by the acyclicity gate.

use rustty_cfg_fmt::{FormatError, Writer};

use crate::config_struct::{Config, CONFIG_FIELDS};

/// FileFormatter is a formatter implementation that outputs the
/// config in a file-like format. This uses more generous whitespace,
/// can include comments, etc.
// pub const FileFormatter = struct {
//     alloc: Allocator,
//     config: *const Config,
//
//     /// Include comments for documentation of each key
//     docs: bool = false,
//
//     /// Only include changed values from the default.
//     changed: bool = false,
//
// PORT NOTE: the reference holds an `alloc` (used only to build the change-tracking
// `default` config). The Rust `Config::default()` needs no allocator, so no alloc
// field is carried; the borrow of `config` is taken at `format` time.
pub struct FileFormatter<'a> {
    pub config: &'a Config,

    /// Include comments for documentation of each key
    pub docs: bool,

    /// Only include changed values from the default.
    pub changed: bool,
}

impl<'a> FileFormatter<'a> {
    /// Construct a `FileFormatter` for `config` with `docs`/`changed` both off,
    /// matching the reference's struct defaults (`docs = false`, `changed =
    /// false`).
    pub fn new(config: &'a Config) -> FileFormatter<'a> {
        FileFormatter {
            config,
            docs: false,
            changed: false,
        }
    }

    /// Implements std.fmt so it can be used directly with std.fmt.
    // pub fn format(
    //     self: FileFormatter,
    //     writer: *std.Io.Writer,
    // ) std.Io.Writer.Error!void {
    pub fn format<W: Writer>(&self, writer: &mut W) -> Result<(), FormatError<W::Error>> {
        // @setEvalBranchQuota(10_000);

        // If we're change-tracking then we need the default config to
        // compare against.
        // var default: ?Config = if (self.changed)
        //     Config.default(self.alloc) catch return error.WriteFailed
        // else
        //     null;
        // defer if (default) |*v| v.deinit();
        //   CORRECTION (task W-4, NEW-FINDINGS N366): `Config.default(alloc)` is
        //   `Config::default_config()` (the struct defaults PLUS the default
        //   keybinds, command-palette entries and URL link). This compared
        //   against the bare struct defaults (`Config::default()`), so
        //   `+show-config` listed every default keybind and palette entry as a
        //   change (MEASURED against the reference exe on Windows).
        let default: Option<Config> = if self.changed {
            Some(Config::default_config())
        } else {
            None
        };

        // inline for (@typeInfo(Config).@"struct".fields) |field| {
        //     if (field.name[0] == '_') continue;
        //
        // PORT NOTE: the comptime `inline for` over the struct fields (skipping
        // `_`-prefixed internals) becomes a runtime loop over `CONFIG_FIELDS` —
        // the ordered `(field.name, @field(Key, field.name))` pairs for exactly
        // the non-`_` fields (the same 204 fields, a COUNT CONTRACT).
        for &(name, key) in CONFIG_FIELDS {
            // const value = @field(self.config, field.name);
            // const do_format = if (default) |*d| format: {
            //     const key = @field(Key, field.name);
            //     break :format d.changed(self.config, key);
            // } else true;
            //
            // (PORT NOTE (c5f4f00ed): the `|*d|` capture-by-pointer — upstream's
            // Valgrind fix for the whole-Config by-value copy — is structurally
            // N-A here: `&default` already borrows, and `Config` is not `Copy`.)
            let do_format = if let Some(d) = &default {
                d.changed(self.config, key)
            } else {
                true
            };

            // if (do_format) {
            if do_format {
                // const do_docs = self.docs and @hasDecl(help_strings.Config, field.name);
                // if (do_docs) {
                //     const help = @field(help_strings.Config, field.name);
                //     var lines = std.mem.splitScalar(u8, help, '\n');
                //     while (lines.next()) |line| {
                //         try writer.print("# {s}\n", .{line});
                //     }
                // }
                //
                // PORT NOTE: `help_strings.Config` is the build-generated per-key
                // doc table (`crate::help_strings::config_help`, emitted by
                // build.rs from each field's `///` doc block — the
                // `helpgen.genConfig` analog). `@hasDecl(help_strings.Config,
                // field.name)` becomes `config_help(key).is_some()`; an
                // undocumented field yields `None` (no `# ` doc lines emitted).
                let help = crate::help_strings::config_help(key);
                let do_docs = self.docs && help.is_some();
                if do_docs {
                    // var lines = std.mem.splitScalar(u8, help, '\n');
                    // while (lines.next()) |line| { try writer.print("# {s}\n", ...); }
                    for line in help.unwrap().split(|&c| c == b'\n') {
                        writer.write_all(b"# ")?;
                        writer.write_all(line)?;
                        writer.write_all(b"\n")?;
                    }
                }

                // formatter.formatEntry(
                //     field.type,
                //     field.name,
                //     value,
                //     writer,
                // ) catch return error.WriteFailed;
                //
                // PORT NOTE: the reference collapses every `formatEntry` error
                // (writer failures AND the fixed-buffer `error.OutOfMemory`)
                // into `error.WriteFailed` here. The port cannot fabricate a
                // `W::Error` value for the OutOfMemory case, so `format`
                // surfaces the full `FormatError` union instead — the same
                // failure behavior (formatting aborts with an error), with the
                // variant preserved.
                self.config.format_entry_field(name, writer)?;

                // if (do_docs) try writer.print("\n", .{});
                if do_docs {
                    writer.write_all(b"\n")?;
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rustty_cfg_fmt::AllocatingWriter;

    // test "format default config"
    #[test]
    fn format_default_config() {
        // const alloc = testing.allocator;
        // var cfg = try Config.default(alloc);
        // defer cfg.deinit();
        let cfg = Config::default();

        // var buf: std.Io.Writer.Allocating = .init(alloc);
        // defer buf.deinit();
        let mut buf = AllocatingWriter::init();

        // We just make sure this works without errors. We aren't asserting output.
        // const fmt: FileFormatter = .{
        //     .alloc = alloc,
        //     .config = &cfg,
        // };
        // try fmt.format(&buf.writer);
        let fmt = FileFormatter::new(&cfg);
        fmt.format(&mut buf).unwrap();

        //std.log.warn("{s}", .{buf.written()});
    }

    // test "format default config changed"
    //
    // PORT NOTE: this test uses `changed = true`, whose per-field
    // `default.changed(self.config, key)` dispatch is the EQUAL/DIFF (CHANGED)
    // pass — `Config::changed` resolves `key` to a field and returns
    // `!deepEqual(field.type, old, new)`, relying on `DeepEqual` for every field
    // type (config-owned types in `deep_equal.rs`; the foreign leaf types —
    // `MetricModifier`/`KeyRemapSet`/`CursorStyle`/`WindowPaddingBalance`/
    // `OptionAsAlt`/`ReleaseChannel` — in their home crates per the orphan rule).
    // That pass has landed (`Config::changed` is fully implemented), so this test
    // runs normally.
    #[test]
    // PORT NOTE: the reference is `var cfg = try Config.default(alloc); cfg.@"font-size" = 42;`
    // — a default then a single-field mutation. Clippy's `field_reassign_with_default`
    // would fold that into a struct literal, deviating from the reference's
    // statement shape; the reassignment is kept verbatim and the lint allowed.
    #[allow(clippy::field_reassign_with_default)]
    fn format_default_config_changed() {
        // const alloc = testing.allocator;
        // var cfg = try Config.default(alloc);
        // defer cfg.deinit();
        let mut cfg = Config::default();
        // cfg.@"font-size" = 42;
        cfg.font_size = 42.0;

        // var buf: std.Io.Writer.Allocating = .init(alloc);
        // defer buf.deinit();
        let mut buf = AllocatingWriter::init();

        // We just make sure this works without errors. We aren't asserting output.
        // const fmt: FileFormatter = .{
        //     .alloc = alloc,
        //     .config = &cfg,
        //     .changed = true,
        // };
        // try fmt.format(&buf.writer);
        let fmt = FileFormatter {
            config: &cfg,
            docs: false,
            changed: true,
        };
        fmt.format(&mut buf).unwrap();

        //std.log.warn("{s}", .{buf.written()});
    }

    // Task W-4 (N366): with `changed = true`, a config built like the
    // reference's `Config.default(alloc)` (default keybinds, palette entries and
    // link included) with one field changed prints exactly that field. The
    // reference test above asserts nothing; this pins the comparison base.
    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn format_changed_compares_against_the_full_default_config() {
        let mut cfg = Config::default_config();
        cfg.font_size = 42.0;
        let mut buf = AllocatingWriter::init();
        let fmt = FileFormatter {
            config: &cfg,
            docs: false,
            changed: true,
        };
        fmt.format(&mut buf).unwrap();
        assert_eq!("font-size = 42\n", String::from_utf8_lossy(buf.written()));
    }
}
