//! rustty-os::edit
//!
//! PORT STATUS: ported
//!
//! Ports the reference's `src/os/edit.zig` — every line, comment, and `test {}`
//! is ported here (R1). Retargeted only for product identity (R2).

// PORT NOTE: the reference imports `std`, `global = @import("../global.zig")`,
// and `ShellEscapeWriter = @import("shell.zig").ShellEscapeWriter`. In rustty
// the captured process environment lives in the leaf `rustty-global` crate
// (`environ()`), and the shell escaper + the allocating writer it feeds are the
// crate-local ports of `os/shell.zig` (`shell::ShellEscapeWriter`) and
// `std.Io.Writer.Allocating` (`string_encoding::AllocatingWriter`).
#![allow(non_snake_case)]

use crate::shell::ShellEscapeWriter;
use crate::string_encoding::{AllocatingWriter, Writer};
use rustty_global::environ::Environ;

// PORT NOTE: `const log = std.log.scoped(.os_edit);`: the log facade with the
// scope as target. CORRECTION (review W-4, N380): this was an `eprintln!`, which
// ignored the logging settings.
const LOG_SCOPE: &str = "os_edit";

fn log_warn(msg: &str) {
    log::warn!(target: LOG_SCOPE, "{msg}");
}

/// PORT NOTE: the reference's anonymous `enum` field type of `Options`. Hoisted
/// to a named enum so the field can name it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DefaultEditor {
    /// Return an error if $EDITOR or $VISUAL is not defined.
    Failure,
    /// Default to "vi" if $EDITOR or $VISUAL is not defined.
    Vi,
    /// Default to "nano" if $EDITOR or $VISUAL is not defined.
    Nano,
}

/// PORT NOTE: `const Options = struct { default_editor: enum {...} = .vi };`.
pub struct Options {
    pub default_editor: DefaultEditor,
}

impl Default for Options {
    /// PORT NOTE: `default_editor: ... = .vi` (the struct field default) and
    /// `pub const default: Options = .{};` collapse into the `Default` impl —
    /// `Options::default()` is the reference's `Options.default`.
    fn default() -> Options {
        Options {
            default_editor: DefaultEditor::Vi,
        }
    }
}

/// PORT NOTE: `pub const Error = error{NoEditorConfigured};`. The reference's
/// full return-error union `(Error || std.Io.Writer.Error ||
/// std.mem.Allocator.Error)` reduces to just this: the `AllocatingWriter` is
/// `Infallible` and allocation is implicit/infallible in this environment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    NoEditorConfigured,
}

/// PORT NOTE: `pub fn getConfigEditCommand(alloc, path, options) !...`. The
/// explicit `alloc` parameter is dropped (allocation is implicit in Rust), and
/// the returned `[:0]const u8` (NUL-terminated, length excludes the NUL) is
/// modeled as an owned `Vec<u8>` whose final byte is the NUL terminator, exactly
/// as `shell::ShellCommandBuilder::toOwnedSlice` does.
///
/// PORT NOTE: this thin wrapper reads the process-global captured environment
/// (`global.environ()`) and delegates the resolution to
/// [`getConfigEditCommandEnviron`]. Splitting the environment source out is the
/// sanctioned testability seam (see `rustty_global::environ::sync_environ`'s
/// doc: "re-factor the code to take an environment map instead, where you can
/// modify the environment as needed"); it lets the `test {}` cases below drive a
/// deterministic local `Environ` instead of racing the process-global block
/// under `libtest`'s parallel runner.
pub fn getConfigEditCommand(path: &[u8], options: &Options) -> Result<Vec<u8>, Error> {
    let environ = rustty_global::environ::environ();
    getConfigEditCommandEnviron(&environ, path, options)
}

fn getConfigEditCommandEnviron(
    environ: &Environ,
    path: &[u8],
    options: &Options,
) -> Result<Vec<u8>, Error> {
    let editor: Vec<u8> = 'editor: {
        // VISUAL vs. EDITOR: https://unix.stackexchange.com/questions/4859/visual-vs-editor-what-s-the-difference
        //
        // PORT NOTE: the reference's `v:`/`e:` labeled blocks probe with the
        // runtime `environ().containsUnempty(alloc, key)` (map-based, present +
        // non-empty), then read the value with `getAlloc` (also map-based),
        // `catch break :<label>` on any error. Both read `createMap`, so of two
        // same-named entries the LAST decides; neither can error here, so the
        // `catch break` fall-through is expressed as `if let Some(..)`.
        // CORRECTION (N381, task W-5b): the probe was
        // `contains_unempty_constant` (the first entry), on the claim that the
        // map was first-entry-wins too; Zig's map keeps the last entry.
        {
            if environ.contains_unempty(b"VISUAL") {
                if let Some(v) = environ.get_alloc(b"VISUAL") {
                    break 'editor v;
                }
            }
        }

        {
            if environ.contains_unempty(b"EDITOR") {
                if let Some(v) = environ.get_alloc(b"EDITOR") {
                    break 'editor v;
                }
            }
        }

        match options.default_editor {
            DefaultEditor::Failure => {
                log_warn("$EDITOR or $VISUAL must be set to open config in a new window");
                return Err(Error::NoEditorConfigured);
            }
            DefaultEditor::Vi => {
                log_warn("$EDITOR or $VISUAL not set, falling back to vi");
                break 'editor b"vi".to_vec();
            }
            DefaultEditor::Nano => {
                log_warn("$EDITOR or $VISUAL not set, falling back to nano");
                break 'editor b"nano".to_vec();
            }
        }
    };
    // PORT NOTE: `defer alloc.free(editor);` — the owned `editor` `Vec` is
    // dropped at the end of this function.

    // PORT NOTE: `var buffer: std.Io.Writer.Allocating = .init(alloc);` plus
    // `const writer = &buffer.writer;`. The `AllocatingWriter` accumulates into
    // an owned `Vec`; there is no separate buffered `writer` handle and no
    // `flush()` step (the reference's `sh.writer.flush()`/`writer.flush()` are
    // no-ops against an unbuffered allocating sink).
    let mut buffer = AllocatingWriter::init();

    // `writeAll`/`writeByte` on `AllocatingWriter` are `Infallible`.
    let _ = buffer.writeAll(&editor);
    let _ = buffer.writeByte(b' ');
    {
        let mut sh = ShellEscapeWriter::init(&mut buffer);
        let _ = sh.writeAll(path);
    }

    // PORT NOTE: `return try buffer.toOwnedSliceSentinel(0);` — the accumulated
    // bytes plus a trailing NUL, ownership transferred to the caller.
    let mut result = buffer.written().to_vec();
    result.push(0);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    // PORT NOTE: `src/os/edit.zig` ships no `test {}` blocks. These cases are
    // rustty-added coverage for the ported resolver, driven through the
    // `getConfigEditCommandEnviron` seam with deterministic local `Environ`
    // blocks (no process-global mutation, no ambient-env dependence).

    fn env(pairs: &[(&str, &str)]) -> Environ {
        Environ::from_entries(
            pairs
                .iter()
                .map(|(k, v)| {
                    let mut e = k.as_bytes().to_vec();
                    e.push(b'=');
                    e.extend_from_slice(v.as_bytes());
                    e
                })
                .collect(),
        )
    }

    /// The logical command string: `result` sans its trailing NUL.
    fn cmd(result: &[u8]) -> &[u8] {
        assert_eq!(0u8, result[result.len() - 1], "must be NUL-terminated");
        &result[..result.len() - 1]
    }

    #[test]
    fn visual_wins_over_editor() {
        let e = env(&[("VISUAL", "nvim"), ("EDITOR", "vim")]);
        let r = getConfigEditCommandEnviron(&e, b"/tmp/config", &Options::default()).unwrap();
        assert_eq!(b"nvim /tmp/config".as_slice(), cmd(&r));
    }

    #[test]
    fn editor_when_no_visual() {
        let e = env(&[("EDITOR", "vim")]);
        let r = getConfigEditCommandEnviron(&e, b"/tmp/config", &Options::default()).unwrap();
        assert_eq!(b"vim /tmp/config".as_slice(), cmd(&r));
    }

    #[test]
    fn empty_visual_falls_through_to_editor() {
        // containsUnempty requires a NON-empty value, so an empty VISUAL is
        // skipped in favor of EDITOR.
        let e = env(&[("VISUAL", ""), ("EDITOR", "vim")]);
        let r = getConfigEditCommandEnviron(&e, b"/tmp/config", &Options::default()).unwrap();
        assert_eq!(b"vim /tmp/config".as_slice(), cmd(&r));
    }

    /// N381: `containsUnempty` and `getAlloc` both read `createMap`, where the
    /// LAST of two same-named entries wins: an empty first `VISUAL` followed by
    /// a set one selects it; a set first `VISUAL` followed by an empty one falls
    /// through to `EDITOR`.
    #[test]
    fn duplicate_visual_entries_use_the_last_one() {
        let e = env(&[("VISUAL", ""), ("VISUAL", "nvim"), ("EDITOR", "vim")]);
        let r = getConfigEditCommandEnviron(&e, b"/tmp/config", &Options::default()).unwrap();
        assert_eq!(b"nvim /tmp/config".as_slice(), cmd(&r));

        let e = env(&[("VISUAL", "nvim"), ("VISUAL", ""), ("EDITOR", "vim")]);
        let r = getConfigEditCommandEnviron(&e, b"/tmp/config", &Options::default()).unwrap();
        assert_eq!(b"vim /tmp/config".as_slice(), cmd(&r));
    }

    #[test]
    fn default_vi_when_unset() {
        let e = env(&[]);
        let r = getConfigEditCommandEnviron(&e, b"/tmp/config", &Options::default()).unwrap();
        assert_eq!(b"vi /tmp/config".as_slice(), cmd(&r));
    }

    #[test]
    fn default_nano_when_unset() {
        let e = env(&[]);
        let opts = Options {
            default_editor: DefaultEditor::Nano,
        };
        let r = getConfigEditCommandEnviron(&e, b"/tmp/config", &opts).unwrap();
        assert_eq!(b"nano /tmp/config".as_slice(), cmd(&r));
    }

    #[test]
    fn failure_when_unset() {
        let e = env(&[]);
        let opts = Options {
            default_editor: DefaultEditor::Failure,
        };
        let r = getConfigEditCommandEnviron(&e, b"/tmp/config", &opts);
        assert_eq!(Err(Error::NoEditorConfigured), r);
    }

    #[test]
    fn path_is_shell_escaped() {
        // Spaces and shell-special characters in the path are escaped by the
        // ShellEscapeWriter; the editor token is not.
        let e = env(&[("EDITOR", "vim")]);
        let r =
            getConfigEditCommandEnviron(&e, b"/tmp/my config (1)", &Options::default()).unwrap();
        assert_eq!(b"vim /tmp/my\\ config\\ \\(1\\)".as_slice(), cmd(&r));
    }
}
