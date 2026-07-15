//! Fish completions. Port of ghostty's `src/extra/fish.zig`.

use std::any::TypeId;

use rustty_cli::rustty::{Action, OptCompletionKind};
use rustty_config::config_struct::{
    config_completion_kind, config_fields, value_type, ConfigCompletionKind,
};
use rustty_config::help_strings::config_help;

/// A fish completions configuration that contains all the available commands
/// and options.
// pub const completions = comptimeGenerateCompletions();
pub fn completions() -> Vec<u8> {
    let mut writer = Vec::new();
    write_completions(&mut writer);
    writer
}

// fn writeCompletions(writer: *std.Io.Writer) !void {
fn write_completions(writer: &mut Vec<u8>) {
    {
        writer.extend_from_slice(b"set -l commands \"");
        for (count, action) in Action::ALL.iter().enumerate() {
            if count > 0 {
                writer.extend_from_slice(b" ");
            }
            writer.extend_from_slice(b"+");
            writer.extend_from_slice(action.tag_name().as_bytes());
        }
        writer.extend_from_slice(b"\"\n");
    }

    writer.extend_from_slice(b"complete -c rustty -f\n");

    writer.extend_from_slice(b"complete -c rustty -s e -l help -f\n");
    writer.extend_from_slice(
        b"complete -c rustty -n \"not __fish_seen_subcommand_from $commands\" -l version -f\n",
    );

    for (name, key) in config_fields() {
        writer.extend_from_slice(
            b"complete -c rustty -n \"not __fish_seen_subcommand_from $commands\" -l ",
        );
        writer.extend_from_slice(name);
        // if (field.type != bool) " -r" else " "
        if value_type(*key) != TypeId::of::<bool>() {
            writer.extend_from_slice(b" -r");
        } else {
            writer.extend_from_slice(b" ");
        }
        if name.starts_with(b"font-family") {
            writer.extend_from_slice(b" -f  -a \"(rustty +list-fonts | grep '^[A-Z]')\"");
        } else if *name == *b"theme" {
            writer.extend_from_slice(
                b" -f -a \"(rustty +list-themes | sed -E 's/^(.*) \\(.*\\$/\\1/')\"",
            );
        } else if *name == *b"working-directory" {
            writer.extend_from_slice(b" -f -k -a \"(__fish_complete_directories)\"");
        } else {
            let kind = config_completion_kind(*key);
            // if (field.type != Config.RepeatablePath) " -f" else " -F"
            if kind == ConfigCompletionKind::RepeatablePath {
                writer.extend_from_slice(b" -F");
            } else {
                writer.extend_from_slice(b" -f");
            }
            match kind {
                ConfigCompletionKind::Bool => {}
                ConfigCompletionKind::Enum(values) => {
                    writer.extend_from_slice(b" -a \"");
                    for (i, v) in values.iter().enumerate() {
                        if i > 0 {
                            writer.extend_from_slice(b" ");
                        }
                        writer.extend_from_slice(v);
                    }
                    writer.extend_from_slice(b"\"");
                }
                ConfigCompletionKind::PackedFlags(fields) => {
                    writer.extend_from_slice(b" -a \"");
                    for (i, f) in fields.iter().enumerate() {
                        if i > 0 {
                            writer.extend_from_slice(b" ");
                        }
                        writer.extend_from_slice(f);
                        writer.extend_from_slice(b" no-");
                        writer.extend_from_slice(f);
                    }
                    writer.extend_from_slice(b"\"");
                }
                // RepeatablePath (struct w/ parseCLI) + Other -> the reference's
                // `.@"struct"` (non-packed) / `else` arms emit nothing.
                ConfigCompletionKind::RepeatablePath | ConfigCompletionKind::Other => {}
            }
        }

        // if (@hasDecl(help_strings.Config, field.name)) { … getDescription … }
        if let Some(help) = config_help(*key) {
            let desc = get_description(help);
            writer.extend_from_slice(b" -d \"");
            writer.extend_from_slice(&desc);
            writer.extend_from_slice(b"\"");
        }

        writer.extend_from_slice(b"\n");
    }

    {
        writer.extend_from_slice(
            b"complete -c rustty -n \"string match -q -- '+*' (commandline -pt)\" -f -a \"",
        );
        for (count, action) in Action::ALL.iter().enumerate() {
            if count > 0 {
                writer.extend_from_slice(b" ");
            }
            writer.extend_from_slice(b"+");
            writer.extend_from_slice(action.tag_name().as_bytes());
        }
        writer.extend_from_slice(b"\"\n");
    }

    for action in Action::ALL {
        // if (std.mem.eql(u8, "help", field.name)) continue;
        // if (std.mem.eql(u8, "version", field.name)) continue;
        if action.tag_name() == "help" {
            continue;
        }
        if action.tag_name() == "version" {
            continue;
        }

        let tag = action.tag_name().as_bytes();
        for spec in action.option_specs() {
            writer.extend_from_slice(b"complete -c rustty -n \"__fish_seen_subcommand_from +");
            writer.extend_from_slice(tag);
            writer.extend_from_slice(b"\" -l ");
            writer.extend_from_slice(spec.name);
            // if (opt.type != bool) " -r" else ""
            if spec.kind != OptCompletionKind::Bool {
                writer.extend_from_slice(b" -r");
            }

            // special case +validate_config --config-file
            if *spec.name == *b"config-file" {
                writer.extend_from_slice(b" -F");
            } else {
                writer.extend_from_slice(b" -f");
            }

            match spec.kind {
                OptCompletionKind::Bool => {}
                OptCompletionKind::Enum(values) | OptCompletionKind::OptionalEnum(values) => {
                    writer.extend_from_slice(b" -a \"");
                    for (i, v) in values.iter().enumerate() {
                        if i > 0 {
                            writer.extend_from_slice(b" ");
                        }
                        writer.extend_from_slice(v);
                    }
                    writer.extend_from_slice(b"\"");
                }
                // .optional (non-enum child) + else -> nothing.
                OptCompletionKind::OptionalOther | OptCompletionKind::Other => {}
            }
            writer.extend_from_slice(b"\n");
        }
    }
}

// fn getDescription(comptime help: []const u8) []const u8 {
fn get_description(help: &[u8]) -> Vec<u8> {
    // var out: [help.len * 2]u8 = undefined; var len: usize = 0;
    let mut out: Vec<u8> = Vec::with_capacity(help.len() * 2);
    // var prev_was_space = false;
    let mut prev_was_space = false;

    // for (help, 0..) |c, i| {
    for (i, &c) in help.iter().enumerate() {
        match c {
            b'.' => {
                out.push(b'.');

                // if (i + 1 >= help.len) break;
                if i + 1 >= help.len() {
                    break;
                }
                // const next = help[i + 1];
                let next = help[i + 1];
                // if (next == ' ' or next == '\n') break;
                if next == b' ' || next == b'\n' {
                    break;
                }
            }
            b'\n' => {
                // if (!prev_was_space and len > 0) { out[len] = ' '; prev_was_space = true; }
                if !prev_was_space && !out.is_empty() {
                    out.push(b' ');
                    prev_was_space = true;
                }
            }
            b'"' => {
                // out[len] = '\\'; out[len + 1] = '"'; prev_was_space = false;
                out.push(b'\\');
                out.push(b'"');
                prev_was_space = false;
            }
            _ => {
                // out[len] = c; prev_was_space = (c == ' ');
                out.push(c);
                prev_was_space = c == b' ';
            }
        }
    }

    // return out[0..len];
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "getDescription"
    #[test]
    fn get_description() {
        // const input = "First sentence with \"quotes\"\nand newlines. Second sentence.";
        let input = b"First sentence with \"quotes\"\nand newlines. Second sentence.";
        // const expected = "First sentence with \\\"quotes\\\" and newlines.";
        let expected = b"First sentence with \\\"quotes\\\" and newlines.";

        let result = super::get_description(input);
        // try testing.expectEqualStrings(expected, result);
        assert_eq!(result.as_slice(), expected.as_slice());
    }

    // Byte-exact golden diff against the committed rustty-branded fixture
    // (derived from ghostty's actually-generated file). The brand rename
    // (`Ghostty`/`ghostty` -> `Rustty`/`rustty`, including the `complete -c`
    // command name — Task 16) is the ONLY intended divergence from ghostty's
    // bytes, so the comparison canonicalizes the brand token
    // (case-insensitively) on both sides; every other byte — field names,
    // flags, enum values, ordering, description wording — must match exactly.
    #[test]
    fn completions_match_golden() {
        let got = brand_normalize(&completions());
        let want = brand_normalize(include_bytes!("../testdata/rustty.fish"));
        assert_eq!(
            got, want,
            "fish completions diverge from ghostty golden beyond the brand rename"
        );
    }

    /// Replace every case-insensitive `ghostty`/`rustty` brand token with a single
    /// placeholder so the R2 rename (config-doc `Ghostty` -> `rustty`) is neutralized
    /// while all other bytes stay significant.
    fn brand_normalize(input: &[u8]) -> Vec<u8> {
        let lower = input.to_ascii_lowercase();
        let mut out = Vec::with_capacity(input.len());
        let mut i = 0;
        while i < lower.len() {
            if lower[i..].starts_with(b"ghostty") {
                out.extend_from_slice(b"\x00brand\x00");
                i += b"ghostty".len();
            } else if lower[i..].starts_with(b"rustty") {
                out.extend_from_slice(b"\x00brand\x00");
                i += b"rustty".len();
            } else {
                out.push(lower[i]);
                i += 1;
            }
        }
        out
    }
}
