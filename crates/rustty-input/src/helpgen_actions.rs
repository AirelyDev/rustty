//! This module is a help generator for keybind actions documentation.
//! It can generate documentation in different formats (plaintext for CLI,
//! markdown for website) while maintaining consistent content.
//
// PORTS: ghostty src/input/helpgen_actions.zig
//
// PORT STATUS: done
//
// PORT NOTE: the reference imports
//   const std = @import("std");
//   const KeybindAction = @import("Binding.zig").Action;
//   const help_strings = @import("help_strings");
// `KeybindAction` maps to the ported `crate::binding::Action`. `help_strings` is
// a BUILD-GENERATED module (its `KeybindAction` namespace holds, per action
// field, the action's doc-comment text harvested at build time). That build
// artifact is not yet produced in rustty; following the established
// deferred-seam precedent, `help_strings.KeybindAction` is modeled by the
// `HelpStrings` seam trait below. The reference's `inline for` over
// `@typeInfo(KeybindAction).@"union".fields` maps to a loop over
// `Action::FIELD_NAMES` (the union field order, single source-of-truth).
// `std.Io.Writer` maps to the ported `rustty_cfg_fmt::Writer`;
// `std.Io.Writer.Allocating` to `AllocatingWriter`. The crate's empty `test {}`
// inventory is preserved (helpgen_actions.zig has 0 tests).

use crate::binding::Action;
use rustty_cfg_fmt::{AllocatingWriter, Writer};

/// Format options for generating keybind actions documentation
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    /// Plain text output with indentation
    Plaintext,
    /// Markdown formatted output
    Markdown,
}

impl Format {
    fn format_field_name<W: Writer>(
        self,
        writer: &mut W,
        field_name: &[u8],
    ) -> Result<(), W::Error> {
        match self {
            Format::Plaintext => {
                writer.write_all(field_name)?;
                writer.write_all(b":\n")
            }
            Format::Markdown => {
                writer.write_all(b"## `")?;
                writer.write_all(field_name)?;
                writer.write_all(b"`\n")
            }
        }
    }

    fn format_doc_line<W: Writer>(self, writer: &mut W, line: &[u8]) -> Result<(), W::Error> {
        match self {
            Format::Plaintext => {
                writer.write_all(b"  ")?;
                writer.write_all(line)?;
                writer.write_all(b"\n")
            }
            Format::Markdown => {
                writer.write_all(line)?;
                writer.write_all(b"\n")
            }
        }
    }

    fn header(self) -> Option<&'static [u8]> {
        match self {
            Format::Plaintext => None,
            Format::Markdown => Some(
                b"---\n\
                title: Keybinding Action Reference\n\
                description: Reference of all rustty keybinding actions.\n\
                editOnGithubLink: https://github.com/ghostty-org/ghostty/edit/main/src/input/Binding.zig\n\
                ---\n\
                \n\
                This is a reference of all rustty keybinding actions.\n\
                \n\
                \n",
            ),
        }
    }
}

// PORT NOTE (DEFERRED SEAM, R4): `help_strings.KeybindAction` — the
// build-generated per-action doc text. Modeled as a trait so `generate` can be
// driven by the real build artifact when it lands; until then a caller may pass
// `NoHelpStrings` (no field has docs), which reproduces the `show_docs = false`
// / no-`@hasDecl` paths exactly.
pub trait HelpStrings {
    /// `@hasDecl(help_strings.KeybindAction, field_name)`
    fn has_decl(&self, field_name: &str) -> bool;
    /// `@field(help_strings.KeybindAction, field_name)`
    fn field(&self, field_name: &str) -> &[u8];
}

/// A `HelpStrings` with no docs for any field. Used where the build-generated
/// table is deliberately not consulted (and reproduced the pre-codegen state);
/// retained as the injection seam's null case (`show_docs = false` / no
/// `@hasDecl` paths) and for tests.
pub struct NoHelpStrings;

impl HelpStrings for NoHelpStrings {
    fn has_decl(&self, _field_name: &str) -> bool {
        false
    }
    fn field(&self, _field_name: &str) -> &[u8] {
        b""
    }
}

/// The real `help_strings.KeybindAction` table, generated at build time from each
/// `Action` variant's `///` doc block (`crate::help_strings::keybind_help`).
/// `@hasDecl(help_strings.KeybindAction, field)` == `keybind_help(field).is_some()`;
/// `@field(...)` == the stored doc bytes.
pub struct RealHelpStrings;

impl HelpStrings for RealHelpStrings {
    fn has_decl(&self, field_name: &str) -> bool {
        crate::help_strings::keybind_help(field_name.as_bytes()).is_some()
    }
    fn field(&self, field_name: &str) -> &[u8] {
        crate::help_strings::keybind_help(field_name.as_bytes()).unwrap_or(b"")
    }
}

/// Generate keybind actions documentation with the specified format
//
// PORT NOTE: the reference signature is
//   pub fn generate(writer, format, show_docs, page_allocator) !void
// `page_allocator` backs `std.Io.Writer.Allocating`; in Rust the allocating
// writer owns its buffer, so the allocator parameter is dropped. The
// `help_strings` build artifact is threaded as the `help_strings` seam argument.
pub fn generate<W: Writer, H: HelpStrings>(
    writer: &mut W,
    format: Format,
    show_docs: bool,
    help_strings: &H,
) -> Result<(), W::Error> {
    // if (format.header()) |header| { try writer.writeAll(header); }
    if let Some(header) = format.header() {
        writer.write_all(header)?;
    }

    // var stream: std.Io.Writer.Allocating = .init(page_allocator);
    // defer stream.deinit();
    let mut stream = AllocatingWriter::init();

    // const fields = @typeInfo(KeybindAction).@"union".fields;
    // inline for (fields) |field| { ... }
    let fields = Action::FIELD_NAMES;
    for field_name in fields {
        // if (field.name[0] == '_') continue;
        if field_name.as_bytes()[0] == b'_' {
            continue;
        }

        // Write previously stored doc comment below all related actions
        // if (show_docs and @hasDecl(help_strings.KeybindAction, field.name)) {
        //     try writer.writeAll(stream.written());
        //     try writer.writeAll("\n");
        //     stream.clearRetainingCapacity();
        // }
        if show_docs && help_strings.has_decl(field_name) {
            writer.write_all(stream.written())?;
            writer.write_all(b"\n")?;
            stream = AllocatingWriter::init();
        }

        // if (show_docs) {
        //     try format.formatFieldName(writer, field.name);
        // } else {
        //     try writer.writeAll(field.name);
        //     try writer.writeAll("\n");
        // }
        if show_docs {
            format.format_field_name(writer, field_name.as_bytes())?;
        } else {
            writer.write_all(field_name.as_bytes())?;
            writer.write_all(b"\n")?;
        }

        // if (show_docs and @hasDecl(help_strings.KeybindAction, field.name)) {
        //     var iter = std.mem.splitScalar(u8, @field(help_strings.KeybindAction, field.name), '\n');
        //     while (iter.next()) |s| {
        //         if (iter.peek() == null and s.len == 0) continue;
        //         try format.formatDocLine(&stream.writer, s);
        //     }
        // }
        if show_docs && help_strings.has_decl(field_name) {
            let text = help_strings.field(field_name);
            // PORT NOTE: `std.mem.splitScalar(u8, text, '\n')` with `iter.peek()`
            // lookahead. Reproduced by iterating the '\n'-separated parts and
            // skipping a trailing empty last part.
            let parts: Vec<&[u8]> = text.split(|&c| c == b'\n').collect();
            for (idx, s) in parts.iter().enumerate() {
                // If it is the last line and empty, then skip it.
                if idx == parts.len() - 1 && s.is_empty() {
                    continue;
                }
                // PORT NOTE: doc lines are written into `stream` (the allocating
                // writer, error = Infallible), NOT the output `writer`; the
                // result cannot fail, so it is not propagated as `W::Error`.
                let Ok(()) = format.format_doc_line(&mut stream, s);
            }
        }
    }

    // Write any remaining buffered documentation
    // if (stream.written().len > 0) { try writer.writeAll(stream.written()); }
    if !stream.written().is_empty() {
        writer.write_all(stream.written())?;
    }

    Ok(())
}
