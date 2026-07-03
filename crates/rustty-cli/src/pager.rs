//! rustty-cli::pager
//!
//! PORTS: ghostty src/cli/Pager.zig (2t) — the external-pager wrapper.
//!
//! A pager wraps output to an external pager program (like `less`) when
//! stdout is a TTY. The pager command is resolved as:
//!
//!   `$RUSTTY_PAGER` > `$PAGER` > `less`
//!
//! Setting either env var to an empty string disables paging.
//! If stdout is not a TTY, writes go directly to stdout.
//
// R2: the reference resolves `$GHOSTTY_PAGER` first; retargeted to the rustty
// product env var `$RUSTTY_PAGER` (same precedent as `RUSTTY_RESOURCES_DIR` in
// rustty-os). `$PAGER` and `less` are standard interop tokens, unchanged.

// const Pager = @This();
// const std = @import("std");
// const Allocator = std.mem.Allocator;
// const internal_os = @import("../os/main.zig");
use rustty_cfg_fmt::Writer;
use std::io::Write;

/// The reference `Pager` type-file (`const Pager = @This()`) → this struct.
pub struct Pager {
    /// The pager child process, if one was spawned.
    // child: ?std.process.Child = null,
    pub child: Option<std::process::Child>,
    // /// The buffered file writer used for both the pager pipe and direct
    // /// stdout paths.
    // file_writer: std.fs.File.Writer = undefined,
    //
    // PORT NOTE: the reference stores the buffered `file_writer` ON the struct so
    // `deinit` can flush it. The Rust port hands the buffered writer out from
    // `writer()` as [`PagerWriter`] (a `std.io.BufWriter` over the pipe/stdout
    // sink); the buffer is owned by that writer and flushed on explicit `flush()`
    // or on drop, so no `file_writer` field is carried and `deinit` only closes
    // the pipe + waits.
}

impl Default for Pager {
    /// The reference default `Pager{}` (`.{}`) — `child = null`.
    fn default() -> Pager {
        Pager { child: None }
    }
}

impl Pager {
    /// Initialize the pager. If stdout is a TTY, this spawns the pager
    /// process. Otherwise, output goes directly to stdout.
    // pub fn init(alloc: Allocator) Pager {
    //     return .{ .child = initPager(alloc) };
    // }
    pub fn init() -> Pager {
        Pager {
            child: init_pager(),
        }
    }

    /// Writes to the pager process if available; otherwise, stdout.
    // pub fn writer(self: *Pager, buffer: []u8) *std.Io.Writer {
    //     if (self.child) |child| {
    //         self.file_writer = child.stdin.?.writer(buffer);
    //     } else {
    //         self.file_writer = std.fs.File.stdout().writer(buffer);
    //     }
    //     return &self.file_writer.interface;
    // }
    //
    // PORT NOTE: the reference `buffer: []u8` param backs `File.Writer`'s buffer;
    // the Rust `BufWriter` owns its buffer, so the param is dropped. The returned
    // `PagerWriter` borrows `self` for the child-pipe case, so it must be dropped
    // before `deinit` (the caller scopes it accordingly, mirroring the reference
    // `const writer = pager.writer(&buffer); …; try writer.flush();` before the
    // `defer pager.deinit()` fires).
    pub fn writer(&mut self) -> PagerWriter<'_> {
        let sink = if let Some(child) = &mut self.child {
            // child.stdin.?
            Sink::Child(child.stdin.as_mut().unwrap())
        } else {
            Sink::Stdout(std::io::stdout().lock())
        };
        PagerWriter {
            inner: std::io::BufWriter::new(sink),
        }
    }

    /// Deinitialize the pager. Waits for the spawned process to exit.
    // pub fn deinit(self: *Pager) void {
    pub fn deinit(&mut self) {
        // if (self.child) |*child| {
        if let Some(mut child) = self.child.take() {
            // Flush any remaining buffered data, close the pipe so the
            // pager sees EOF, then wait for it to exit.
            // self.file_writer.interface.flush() catch {};
            //
            // PORT NOTE: the buffered data lives in the [`PagerWriter`] handed out
            // by `writer()`, which the caller flushes (and which flushes on drop)
            // before this runs; there is no `file_writer` on the struct to flush
            // here. Closing the pipe + waiting matches the reference tail.
            // if (child.stdin) |stdin| {
            //     stdin.close();
            //     child.stdin = null;
            // }
            drop(child.stdin.take());
            // _ = child.wait() catch {};
            let _ = child.wait();
        }

        // self.* = undefined;
        //   (Rust: nothing — `child` is already `None` after `take()`.)
    }
}

// PORT NOTE: the reference relies on `defer pager.deinit()` at every call site.
// `Drop` reproduces that `defer` (running the close-pipe + wait even on an early
// `?`-return), and `deinit` is idempotent (`self.child.take()`), so an explicit
// `deinit()` followed by drop is a no-op the second time.
impl Drop for Pager {
    fn drop(&mut self) {
        self.deinit();
    }
}

// fn initPager(alloc: Allocator) ?std.process.Child {
fn init_pager() -> Option<std::process::Child> {
    // const stdout_file: std.fs.File = .stdout();
    // if (!stdout_file.isTty()) return null;
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() {
        return None;
    }

    // Resolve the pager command: $RUSTTY_PAGER > $PAGER > `less`.
    // An empty value for either env var disables paging.
    // const ghostty_var = internal_os.getenv(alloc, "GHOSTTY_PAGER") catch null;
    //   R2: GHOSTTY_PAGER -> RUSTTY_PAGER (product env var).
    // defer if (ghostty_var) |v| v.deinit(alloc);
    let rustty_var = rustty_os::env::getenv("RUSTTY_PAGER").unwrap_or(None);
    // const pager_var = internal_os.getenv(alloc, "PAGER") catch null;
    // defer if (pager_var) |v| v.deinit(alloc);
    let pager_var = rustty_os::env::getenv("PAGER").unwrap_or(None);

    // const cmd: ?[]const u8 = cmd: {
    //     if (ghostty_var) |v| break :cmd if (v.value.len > 0) v.value else null;
    //     if (pager_var) |v| break :cmd if (v.value.len > 0) v.value else null;
    //     break :cmd "less";
    // };
    let cmd: Option<String> = 'cmd: {
        if let Some(v) = &rustty_var {
            break 'cmd if !v.value.is_empty() {
                Some(v.value.clone())
            } else {
                None
            };
        }
        if let Some(v) = &pager_var {
            break 'cmd if !v.value.is_empty() {
                Some(v.value.clone())
            } else {
                None
            };
        }
        Some("less".to_string())
    };

    // if (cmd == null) return null;
    let cmd = cmd?;

    // var child: std.process.Child = .init(&.{cmd.?}, alloc);
    // child.stdin_behavior = .Pipe;
    // child.stdout_behavior = .Inherit;
    // child.stderr_behavior = .Inherit;
    let mut command = std::process::Command::new(&cmd);
    command.stdin(std::process::Stdio::piped());
    command.stdout(std::process::Stdio::inherit());
    command.stderr(std::process::Stdio::inherit());

    // child.spawn() catch return null;
    // return child;
    command.spawn().ok()
}

/// The buffered writer handed out by [`Pager::writer`]. Writes to the spawned
/// pager's stdin pipe, or directly to stdout when no pager was spawned.
///
/// PORTS: the `*std.Io.Writer` the reference `Pager.writer` returns.
pub struct PagerWriter<'a> {
    inner: std::io::BufWriter<Sink<'a>>,
}

impl PagerWriter<'_> {
    /// PORTS: `try writer.flush()` at the reference call sites.
    pub fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

// PORTS: the writer's `writeAll` surface (used by `configfmt.format` /
// `writer.writeAll(...)`), via the `rustty_cfg_fmt::Writer` trait.
impl Writer for PagerWriter<'_> {
    type Error = std::io::Error;
    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.inner.write_all(bytes)
    }
}

/// The sink [`PagerWriter`] buffers into: the pager child's stdin pipe, or an
/// owned stdout lock. Models the reference's two `file_writer` initializations
/// (`child.stdin.?.writer` vs `std.fs.File.stdout().writer`).
enum Sink<'a> {
    Child(&'a mut std::process::ChildStdin),
    Stdout(std::io::StdoutLock<'static>),
}

impl Write for Sink<'_> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        match self {
            Sink::Child(w) => w.write(buf),
            Sink::Stdout(w) => w.write(buf),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Sink::Child(w) => w.flush(),
            Sink::Stdout(w) => w.flush(),
        }
    }
}

/// An adapter that presents any [`std::io::Write`] sink as a
/// [`rustty_cfg_fmt::Writer`]. Used where an action streams to a bare stdout
/// (e.g. `list_actions` feeding `helpgen_actions::generate`) with no pager.
pub struct IoWriter<W: Write>(pub W);

impl<W: Write> Writer for IoWriter<W> {
    type Error = std::io::Error;
    fn write_all(&mut self, bytes: &[u8]) -> std::io::Result<()> {
        self.0.write_all(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // test "pager: non-tty"
    #[test]
    fn pager_non_tty() {
        // var pager: Pager = .init(std.testing.allocator);
        // defer pager.deinit();
        let mut pager = Pager::init();
        // try std.testing.expect(pager.child == null);
        //
        // PORT NOTE: under `cargo test` stdout is captured (not a TTY), so
        // `init_pager` returns `None` — the same condition the reference relies on.
        assert!(pager.child.is_none());
        pager.deinit();
    }

    // test "pager: default writer"
    #[test]
    fn pager_default_writer() {
        // var pager: Pager = .{};
        // defer pager.deinit();
        let mut pager = Pager::default();
        // try std.testing.expect(pager.child == null);
        assert!(pager.child.is_none());
        // var buf: [4096]u8 = undefined;
        // const w = pager.writer(&buf);
        // try w.writeAll("hello");
        //
        // PORT NOTE: `w` (a `BufWriter`) has a `Drop`, so its borrow of `pager`
        // extends to end of scope; it is scoped in a block so the borrow ends
        // before the `defer`-equivalent `deinit()` runs.
        {
            let mut w = pager.writer();
            w.write_all(b"hello").unwrap();
        }
        pager.deinit();
    }
}
