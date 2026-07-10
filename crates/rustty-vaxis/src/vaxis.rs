//! The `Vaxis` INHABITED PLACEHOLDER (placeholder-leaf-doctrine, Chunk 3b §5).
//!
//! `Loop.vaxis: *Vaxis` and `handle_event_generic` (the ~200-LOC event router in
//! `loop_.rs`) reach deep into `Vaxis` — a LATER chunk. Rather than defer the whole
//! router as one seam, this placeholder carries the exact fields the router names
//! (`caps`/`state`/`screen`/`query_futex`/`queries_done`/`opts`) as INHABITED
//! sub-structs, so `handle_event_generic` ports LIVE (its full routing control-flow, the
//! F3 width/scaled-text detection, the capability-flag writes, the postEvent routing).
//! Only the genuine Vaxis LEAVES are `unimplemented!()`:
//! [`Vaxis::translate_mouse`], [`Vaxis::device_status_report`], and the query-futex wake.
//!
//! The Vaxis chunk later FILLS this placeholder; filling must preserve this field/method
//! surface (`cargo build --workspace` is the backstop).

use crate::gwidth::Method;
use crate::mouse::Mouse;
use crate::tty::TtyWriter;
use std::sync::atomic::AtomicU32;

/// The discovered terminal capabilities (`vx.caps.*`). The router sets these flags LIVE
/// as capability events arrive.
pub struct Capabilities {
    pub kitty_keyboard: bool,
    pub kitty_graphics: bool,
    pub rgb: bool,
    pub sgr_pixels: bool,
    pub unicode: Method,
    pub color_scheme_updates: bool,
    pub multi_cursor: bool,
    pub explicit_width: bool,
    pub scaled_text: bool,
}

/// `vx.state.*` — the in-band-resize latch the router reads/sets.
pub struct State {
    pub in_band_resize: bool,
}

/// `vx.screen.*` — only `width_method` is named by the router.
pub struct Screen {
    pub width_method: Method,
}

/// `vx.opts.*` — the options struct. `system_clipboard_allocator` is DROPPED (paste
/// owns its `Vec`, §10.16); the struct is kept inhabited for the surface.
#[derive(Default)]
pub struct Options {}

/// The `Vaxis` placeholder. Not `AtomicBool queries_done` because Zig uses `.unordered`
/// loads/stores; the Rust port uses `AtomicU32` for `query_futex` (the futex word) and an
/// `AtomicBool` for `queries_done` — both accessed with `Relaxed` (Rust's weakest,
/// closest to Zig's `.unordered`).
pub struct Vaxis {
    pub caps: Capabilities,
    pub state: State,
    pub screen: Screen,
    /// The futex word woken on the DA1 query response.
    pub query_futex: AtomicU32,
    pub queries_done: std::sync::atomic::AtomicBool,
    pub opts: Options,
}

impl Default for Vaxis {
    fn default() -> Self {
        Vaxis {
            caps: Capabilities {
                kitty_keyboard: false,
                kitty_graphics: false,
                rgb: false,
                sgr_pixels: false,
                unicode: Method::Wcwidth,
                color_scheme_updates: false,
                multi_cursor: false,
                explicit_width: false,
                scaled_text: false,
            },
            state: State {
                in_band_resize: false,
            },
            screen: Screen {
                width_method: Method::Wcwidth,
            },
            query_futex: AtomicU32::new(0),
            queries_done: std::sync::atomic::AtomicBool::new(false),
            opts: Options::default(),
        }
    }
}

impl Vaxis {
    /// LEAF (§5): the full mouse translation belongs to the Vaxis chunk.
    pub fn translate_mouse(&self, _mouse: Mouse) -> Mouse {
        unimplemented!("Vaxis::translate_mouse — placeholder leaf (filled by the Vaxis chunk)")
    }

    /// LEAF (§5): writes the DSR query to the tty. In `stop` the error is ignored
    /// (`catch {}`); the whole call is `unimplemented!()` until the Vaxis chunk lands.
    pub fn device_status_report(&self, _writer: &mut TtyWriter) -> std::io::Result<()> {
        unimplemented!("Vaxis::device_status_report — placeholder leaf (filled by the Vaxis chunk)")
    }

    /// LEAF (§5): `std.Thread.Futex.wake(&vx.query_futex, 10)` — the DA1-response wake.
    pub fn wake_query_futex(&self) {
        unimplemented!("Vaxis::wake_query_futex — placeholder leaf (filled by the Vaxis chunk)")
    }
}
