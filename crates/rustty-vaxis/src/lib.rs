//! rustty-vaxis — pure-Rust port of the vendored `libvaxis` 0.5.1 subset (R3).
//!
//! CHUNK 1a — the CORE VALUE TYPES only. Module-per-Zig-file layout:
//! - [`cell`] — `Cell.zig` (`Style`/`Color`/`Segment`/`Cell` + SGR bits)
//! - [`key`] — `Key.zig` (`Key` + constants + matchers + `Modifiers`/`KittyFlags`)
//! - [`mouse`] — `Mouse.zig`
//! - [`grapheme_cache`] — `GraphemeCache.zig`
//! - [`image`] — `Image.zig` STRUCT SHAPE (zigimg/Window bodies are R5 seams)
//! - [`Winsize`] — the `main.zig:38` terminal-size struct
//!
//! CHUNK 1b adds the width/grapheme surface:
//! - [`gwidth`] — `gwidth.zig` (grapheme-width Method/eawToWidth/gwidth, 11-test pin)
//! - [`unicode`] — `unicode.zig` (Old-API grapheme iterator wrapper)
//! - [`uucode`] — the vendored `uucode` 0.1.0 runtime subset (types/get/grapheme/utf8
//!   + the stage1/stage2 tables `build.rs` reproduces from the vendored UCD 16.0.0)
//!
//! CHUNK 2 adds the RENDERING layer:
//! - [`screen`] — `Screen.zig` (the flat `Vec<Cell<'a>>` cell buffer)
//! - [`internal_screen`] — `InternalScreen.zig` (the diffing back buffer; arena → per-cell
//!   owned `Vec<u8>`)
//! - [`window`] — `Window.zig` (the window view: `child`/border, `write_cell`/`read_cell`,
//!   `fill`/`clear`, cursor, `print`/`print_segment` gwidth-aware layout, `scroll`,
//!   `has_mouse`; `Window<'a>` holds `screen: *mut Screen<'a>` and is `Copy`, §5)
//!
//! CHUNK 3a adds the INPUT-PARSING layer (the pure half of the backend):
//! - [`event`] — `event.zig` (the `Event<'a>` union; `Paste` owns a `Vec<u8>`)
//! - [`ctlseqs`] — `ctlseqs.zig` (~60 `&[u8]` control-sequence byte constants)
//! - [`parser`] — `Parser.zig` (the ground/escape/csi/osc/dcs/sos/pm/apc/ss3 state
//!   machine; Kitty keyboard, SGR/xterm mouse, bracketed paste, OSC-52 base64, the
//!   c1b-uucode grapheme path; `parse<'a>(&'a mut self, &'a [u8])` — PURE, no
//!   tty/Loop coupling, 34 assertion-for-assertion tests)
//!
//! CHUNK 3b adds the IMPURE half of the backend (the tty backend + concurrency):
//! - [`queue`] — `queue.zig` (the thread-safe fixed-size BLOCKING two-mirror ring;
//!   `Mutex` + `not_full`/`not_empty` `Condvar`; the 7-test Miri pin)
//! - [`tty`] — `tty.zig` (`PosixTty` termios/ioctl/sigaction FFI; the SIGWINCH global
//!   handler; `TestTty` pipe mock; `WindowsTty` = `#[cfg(windows)]` R5 seam)
//! - [`loop_`] — `Loop.zig` (the construct-then-spawn read thread: raw-self `*mut Loop`,
//!   `should_quit: AtomicBool`, `thread: Cell`, the type-erased winsize callback; the
//!   queued `T` OWNS its grapheme text via `Key::into_owned`; `handle_event_generic`
//!   ported LIVE against the [`vaxis::Vaxis`] placeholder; the `LoopEvent` trait for
//!   Zig's `@hasField`)
//! - [`vaxis`] — the Vaxis placeholder (Chunk 3b), later REPLACED by the real `Vaxis<'a>`
//!
//! CHUNK 4a REPLACES the placeholder with the real `Vaxis<'a>` — the main object + the
//! render diff-and-emit engine:
//! - [`vaxis`] — `Vaxis.zig` (the `Vaxis<'a>` struct: caps/screen/screen_last/state/opts +
//!   the query-futex; init/deinit/reset_state/resize/alt-screen/window; queryTerminal/
//!   deviceStatusReport capability queries; the RENDER diff-and-emit engine and its simpler
//!   `pretty_print` mirror; the setters/queries; the portable kitty-graphics transmit paths.
//!   The two `zigimg`-decode paths (`transmit_image`/`load_image`) are R5 seams. Filling the
//!   placeholder threaded `'a` through `Loop<'a, T>`.)
//!
//! PORT STATUS: done (chunk 1a — value types; chunk 1b — gwidth/unicode + the uucode
//! width/grapheme tables; chunk 2 — the Screen/InternalScreen/Window rendering layer;
//! chunk 3a — the event/ctlseqs/parser input-parsing layer; chunk 3b — the tty backend
//! and concurrency layer (queue/tty/loop_) plus the Vaxis placeholder; chunk 4a — the real
//! `Vaxis<'a>` object + the render diff-and-emit engine; chunk 4b — the `widgets` tier +
//! the `TextInput` gap-buffer widget). The chunk sequence is FINISHED; the crate's
//! remaining live seams (DEFERRALS.md "rustty-vaxis Chunks 4a/4b"): the two zigimg
//! seams (`Vaxis::transmit_image`/`load_image`) plus `Image::draw`/`Image::cell_size`
//! (unreachable until the zigimg tier lands — an `Image` is only constructed through
//! those two), and the `#[cfg(windows)]` family (`WindowsTty` + the windows `tty_run`
//! arm + `TestTty::next_event`, DEFERRALS.md Chunk 3b).
//!
//! CHUNK 4b adds the first WIDGET (the new `widgets` tier, ABOVE `window`):
//! - [`widgets`] — `widgets.zig` aggregator + [`widgets::text_input`] (`widgets/TextInput.zig`):
//!   the single-line text-input widget over a raw-UTF-8 **gap `Buffer`** (`&[u8]`/`Vec<u8>`,
//!   never a lossy `String`); `update(Event)` key-dispatch, cursor/word motion, delete, and a
//!   gwidth-aware `draw` (record-then-replay — the `Cell<'a>` borrows into `self.buf`, §6).
//!   Self-contained over `Window` → no R5 seam; a pure leaf → Miri-primary.

pub mod cell;
pub mod ctlseqs;
pub mod event;
pub mod grapheme_cache;
pub mod gwidth;
pub mod image;
pub mod internal_screen;
pub mod key;
pub mod loop_;
pub mod mouse;
pub mod parser;
pub mod queue;
pub mod screen;
pub mod tty;
pub mod unicode;
pub mod uucode;
pub mod vaxis;
pub mod widgets;
pub mod window;

// so `vaxis::Loop` matches Zig's `vaxis.Loop`, and `vaxis::Tty` matches `vaxis.Tty`
pub use loop_::Loop;
pub use tty::Tty;
pub use vaxis::Vaxis;

/// The size of the terminal screen
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Winsize {
    pub rows: u16,
    pub cols: u16,
    pub x_pixel: u16,
    pub y_pixel: u16,
}
