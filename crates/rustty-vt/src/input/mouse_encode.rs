//! rustty-vt input/mouse_encode
//!
//! PORTS: ghostty src/input/mouse_encode.zig
//!
//! PORT STATUS: done

// PORT NOTE: the reference imports are
//   const std = @import("std");
//   const testing = std.testing;
//   const terminal = @import("../terminal/main.zig");
//   const Terminal = terminal.Terminal;
//   const renderer_size = @import("../renderer/size.zig");
//   const point = @import("../terminal/point.zig");
//   const key = @import("key.zig");
//   const mouse = @import("mouse.zig");
//
// In rustty:
//   - `terminal.MouseEvent`  == `crate::mouse::Event`  (terminal/main.zig aliases
//     `MouseEvent = mouse.Event`).
//   - `terminal.MouseFormat` == `crate::mouse::Format`.
//   - `terminal.mouse.eventSendsMotion` == `crate::mouse::event_sends_motion`.
//   - `renderer_size` is the rustty-render-types leaf (the A1 hoist that breaks
//     the vt<->renderer cycle): `Size`, `Coordinate`, `CoordinateTag`, etc.
//   - `point` is `crate::point` (its `Coordinate` with `.eql`).
//   - `key`/`mouse` are the sibling input modules (`super::key`, `super::mouse`).
//
// `Terminal` is referenced only by `Options.fromTerminal`; the terminal-flags
// read is ported as a seam below.

use crate::mouse as terminal_mouse;
use crate::point;
use rustty_render_types as renderer_size;

use super::key;
use super::mouse;

// const log = std.log.scoped(.mouse_encode);
const LOG_SCOPE: &str = "mouse_encode";

/// Options that affect mouse encoding behavior and provide runtime context.
//
// PORT NOTE: the reference is `pub const Options = struct { ... }` with field
// defaults (`event = .none`, `format = .x10`, `any_button_pressed = false`,
// `last_cell = null`); `size` has no default. `#[derive(Clone, Copy)]` matches
// the freely-copied value type. There is no `Default` derive because `size` is
// required; callers build `Options` with explicit field values just as the
// reference's struct-literal call sites do.
//
// PORT NOTE: the reference `Options` is a trivially-Copy value type, but the
// Rust `last_cell: Option<&'a mut Option<point::Coordinate>>` field holds a
// mutable borrow and so cannot be `Copy`/`Clone`. The helper functions that the
// reference calls with a by-value `opts` (`should_report`, `button_code`,
// `pos_*`) only read the Copy fields, so they take `&Options` here; this is the
// faithful equivalent of the by-value pass with no behavioral difference.
pub struct Options<'a> {
    /// Terminal mouse reporting mode (X10, normal, button, any).
    pub event: terminal_mouse::Event,

    /// Terminal mouse reporting format.
    pub format: terminal_mouse::Format,

    /// Full renderer size used to convert surface-space pixel positions
    /// into grid cell coordinates (for most formats) and terminal-space
    /// pixel coordinates (for SGR-Pixels), as well as to determine
    /// whether a position falls outside the visible viewport.
    pub size: renderer_size::Size,

    /// Whether any mouse button is currently pressed. When a motion
    /// event occurs outside the viewport, it is only reported if a
    /// button is held down and the event mode supports motion tracking.
    /// Without this, out-of-viewport motions are silently dropped.
    ///
    /// This should reflect the state of the current event as well, so
    /// if the encoded event is a button press, this should be true.
    pub any_button_pressed: bool,

    /// Last reported viewport cell for motion deduplication.
    /// If null, motion deduplication state is not tracked.
    //
    // PORT NOTE: the reference is `?*?point.Coordinate` — an optional pointer to
    // an optional cell. The encoder both reads (`last.*`) and writes (`last.* =
    // cell`) through it. Ported as `Option<&'a mut Option<point::Coordinate>>`,
    // the faithful mutable-borrow equivalent of the optional pointer.
    pub last_cell: Option<&'a mut Option<point::Coordinate>>,
}

impl<'a> Options<'a> {
    /// Initialize from terminal and renderer state. The caller may still
    /// set any_button_pressed and last_cell on the returned value.
    //
    // PORT NOTE: the reference reads `t.flags.mouse_event` and
    // `t.flags.mouse_format` off a `*const Terminal`. `Terminal` lands in vt-12;
    // until the flags struct is wired, this constructor takes the already-read
    // `event`/`format` values directly (the seam), preserving the contract that
    // `any_button_pressed`/`last_cell` keep their defaults for the caller to set.
    pub fn from_terminal(
        event: terminal_mouse::Event,
        format: terminal_mouse::Format,
        size: renderer_size::Size,
    ) -> Options<'a> {
        Options {
            event,
            format,
            size,
            any_button_pressed: false,
            last_cell: None,
        }
    }
}

/// A normalized mouse event for protocol encoding.
//
// PORT NOTE: the reference `pub const Event = struct { ... }` defaults
// `action = .press`, `button = null`, `mods = .{}`, `pos = .{}`. Ported with a
// `Default` impl that reproduces those field defaults; `#[derive(Clone, Copy)]`
// matches the value type.
#[derive(Clone, Copy)]
pub struct Event {
    /// The action of this mouse event.
    pub action: mouse::Action,

    /// The button involved in this event. This can be null in the
    /// case of a motion action with no pressed buttons.
    pub button: Option<mouse::Button>,

    /// Keyboard modifiers held during this event.
    pub mods: key::Mods,

    /// Mouse position in terminal-space pixels, with (0, 0) at the top-left
    /// of the terminal. Negative values are allowed and indicate positions
    /// above or to the left of the terminal. Values larger than the terminal
    /// size are also allowed and indicate right or below the terminal.
    pub pos: Pos,
}

impl Default for Event {
    fn default() -> Self {
        Event {
            action: mouse::Action::Press,
            button: None,
            mods: key::Mods::default(),
            pos: Pos::default(),
        }
    }
}

/// Mouse position in surface-space pixels.
//
// PORT NOTE: the reference is `pub const Pos = extern struct { x: f32 = 0,
// y: f32 = 0 }`. `#[repr(C)]` mirrors `extern struct`; `Default` gives the
// zero defaults.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Pos {
    pub x: f32,
    pub y: f32,
}

/// Encode the mouse event to the writer according to the options.
///
/// Not all events result in output.
//
// PORT NOTE: the reference signature is
//   pub fn encode(writer: *std.Io.Writer, event: Event, opts: Options)
//       std.Io.Writer.Error!void
// `std.Io.Writer` is a byte sink (the x10/utf8 formats emit raw, non-UTF-8
// bytes like `32 + button_code`), so it is ported to a generic
// `W: std::io::Write` returning `std::io::Result<()>`. `opts` is taken by
// value (`Copy`) but borrows `last_cell` mutably, so it is `mut`.
pub fn encode<W: std::io::Write>(
    writer: &mut W,
    event: Event,
    mut opts: Options,
) -> std::io::Result<()> {
    if !should_report(event, &opts) {
        return Ok(());
    }

    // Handle scenarios where the mouse position is outside the viewport.
    // We always report release events no matter where they happen.
    if event.action != mouse::Action::Release && pos_out_of_viewport(event.pos, opts.size) {
        // If we don't have a motion-tracking event mode, do nothing,
        // because events outside the viewport are never reported in
        // such cases.
        if !terminal_mouse::event_sends_motion(opts.event) {
            return Ok(());
        }

        // For motion modes, we only report if a button is currently pressed.
        // This lets a TUI detect a click over the surface + drag out
        // of the surface.
        if !opts.any_button_pressed {
            return Ok(());
        }
    }

    let cell = pos_to_cell(event.pos, opts.size);

    // We only send motion events when the cell changed unless
    // we're tracking raw pixels.
    if event.action == mouse::Action::Motion && opts.format != terminal_mouse::Format::SgrPixels {
        // PARITY: the reference nests three `if`s here —
        //   `if (opts.last_cell) |last| { if (last.*) |last_cell| { ... } }` —
        // (an optional pointer, then the optional cell behind it). The two
        // `if let`s are kept un-collapsed to mirror that exact shape; clippy's
        // `collapsible_match` is allowed rather than rewriting the control flow.
        #[allow(clippy::collapsible_match)]
        if let Some(last) = opts.last_cell.as_deref() {
            if let Some(last_cell) = *last {
                if last_cell.eql(cell) {
                    return Ok(());
                }
            }
        }
    }

    // Update the last reported cell if we are tracking it.
    if let Some(last) = opts.last_cell.as_deref_mut() {
        *last = Some(cell);
    }

    let button_code = match button_code(event, &opts) {
        Some(c) => c,
        None => return Ok(()),
    };
    match opts.format {
        terminal_mouse::Format::X10 => {
            if cell.x > 222 || cell.y > 222 {
                log::info!(target: LOG_SCOPE, "X10 mouse format can only encode X/Y up to 223");
                return Ok(());
            }

            // + 1 because our x/y are zero-indexed and the protocol uses 1-indexing.
            writer.write_all(b"\x1B[M")?;
            writer.write_all(&[32 + button_code])?;
            writer.write_all(&[32 + (cell.x as u8) + 1])?;
            writer.write_all(&[32 + (cell.y as u8) + 1])?;
        }

        terminal_mouse::Format::Utf8 => {
            let mut buf: [u8; 4] = [0; 4];
            let x_cp: u32 = (cell.x as u32) + 33;
            let y_cp: u32 = (cell.y as u32) + 33;

            // PORT NOTE: reference defect (R13, UPSTREAM-DEFECTS.md UD60). The
            // reference writes `ESC [ M` and the button, then does
            // `std.unicode.utf8Encode(x + 33) catch unreachable` for each
            // coordinate. A cell coordinate is a `u16`, so `x + 33` is a surrogate
            // (U+D800..U+DFFF) for x in 55263..=57310, which UTF-8 cannot encode:
            // ReleaseSafe panics, ReleaseFast is undefined behaviour. A grid that
            // wide is reachable through the libghostty-vt mouse encoder's `size`
            // option (for example a 60000-pixel screen with 1-pixel cells). The
            // port takes the path the X10 arm above takes for a coordinate its
            // format cannot carry: the event is not reported. The check runs
            // before anything is written, so no partial `ESC [ M` reaches the
            // writer. The X10 arm's `log.info` text is specific to X10, so no
            // message is logged here.
            let (Some(x_ch), Some(y_ch)) = (char::from_u32(x_cp), char::from_u32(y_cp)) else {
                return Ok(());
            };

            writer.write_all(b"\x1B[M")?;

            // The button code always fits in a single byte.
            writer.write_all(&[32 + button_code])?;

            let x_len = x_ch.encode_utf8(&mut buf).len();
            writer.write_all(&buf[0..x_len])?;

            let y_len = y_ch.encode_utf8(&mut buf).len();
            writer.write_all(&buf[0..y_len])?;
        }

        terminal_mouse::Format::Sgr => {
            write!(
                writer,
                "\x1B[<{};{};{}{}",
                button_code,
                cell.x + 1,
                cell.y + 1,
                if event.action == mouse::Action::Release {
                    'm'
                } else {
                    'M'
                },
            )?;
        }

        terminal_mouse::Format::Urxvt => {
            write!(
                writer,
                "\x1B[{};{};{}M",
                32 + button_code as u32,
                cell.x + 1,
                cell.y + 1,
            )?;
        }

        terminal_mouse::Format::SgrPixels => {
            let pixels = pos_to_pixels(event.pos, opts.size);
            write!(
                writer,
                "\x1B[<{};{};{}{}",
                button_code,
                pixels.x,
                pixels.y,
                if event.action == mouse::Action::Release {
                    'm'
                } else {
                    'M'
                },
            )?;
        }
    }

    Ok(())
}

/// Returns true if this event should be reported for the given mouse
/// event mode.
fn should_report(event: Event, opts: &Options) -> bool {
    match opts.event {
        terminal_mouse::Event::None => false,

        // X10 only reports button presses of left, middle, and right.
        terminal_mouse::Event::X10 => {
            event.action == mouse::Action::Press
                && event.button.is_some()
                && (event.button.unwrap() == mouse::Button::Left
                    || event.button.unwrap() == mouse::Button::Middle
                    || event.button.unwrap() == mouse::Button::Right)
        }

        // Normal mode does not report motion.
        terminal_mouse::Event::Normal => event.action != mouse::Action::Motion,

        // Button mode requires an active button for motion events.
        terminal_mouse::Event::Button => event.button.is_some(),

        // Any mode reports everything.
        terminal_mouse::Event::Any => true,
    }
}

// PARITY: the reference is a labeled block `code: { if (button == null) break
// :code 3; if (release && ...) break :code 3; break :code switch
// (event.button.?) {...} }`. The `else` arm reaches the switch only when
// `event.button` is non-null, so `event.button.unwrap()` mirrors `event.button.?`
// exactly; clippy's `unnecessary_unwrap` is allowed to keep the
// branch-then-unwrap shape rather than restructuring the chain.
#[allow(clippy::unnecessary_unwrap)]
fn button_code(event: Event, opts: &Options) -> Option<u8> {
    let mut acc: u8 = {
        if event.button.is_none() {
            // Null button means motion with no pressed button.
            3
        } else if event.action == mouse::Action::Release
            && opts.format != terminal_mouse::Format::Sgr
            && opts.format != terminal_mouse::Format::SgrPixels
        {
            // Legacy releases are always encoded as button 3.
            3
        } else {
            match event.button.unwrap() {
                mouse::Button::Left => 0,
                mouse::Button::Middle => 1,
                mouse::Button::Right => 2,
                mouse::Button::Four => 64,
                mouse::Button::Five => 65,
                mouse::Button::Six => 66,
                mouse::Button::Seven => 67,
                mouse::Button::Eight => 128,
                mouse::Button::Nine => 129,
                _ => return None,
            }
        }
    };

    // X10 does not include modifiers.
    if opts.event != terminal_mouse::Event::X10 {
        if event.mods.shift {
            acc += 4;
        }
        if event.mods.alt {
            acc += 8;
        }
        if event.mods.ctrl {
            acc += 16;
        }
    }

    // Motion adds another bit.
    if event.action == mouse::Action::Motion {
        acc += 32;
    }

    Some(acc)
}

/// Terminal-space pixel position for SGR pixel reporting.
struct PixelPoint {
    x: i32,
    y: i32,
}

/// Returns true if the surface-space pixel position is outside the
/// visible viewport bounds (negative or beyond screen dimensions).
fn pos_out_of_viewport(pos: Pos, size: renderer_size::Size) -> bool {
    let max_x: f32 = size.screen.width as f32;
    let max_y: f32 = size.screen.height as f32;
    pos.x < 0.0 || pos.y < 0.0 || pos.x > max_x || pos.y > max_y
}

/// Converts a surface-space pixel position to a zero-based grid cell
/// coordinate (column, row) within the terminal viewport. Out-of-bounds
/// values are clamped to the valid grid range (0 to columns/rows - 1).
fn pos_to_cell(pos: Pos, size: renderer_size::Size) -> point::Coordinate {
    let coord: renderer_size::Coordinate =
        renderer_size::Coordinate::Surface(renderer_size::CoordinateSurface {
            x: pos.x as f64,
            y: pos.y as f64,
        });
    let grid = match coord.convert(renderer_size::CoordinateTag::Grid, size) {
        renderer_size::Coordinate::Grid(g) => g,
        // PORT NOTE: `coord.convert(.grid, size).grid` — `convert` to `.grid`
        // always yields the `.grid` arm, so the non-grid arms are unreachable
        // (the reference's tagged-union field access asserts the same).
        _ => unreachable!(),
    };
    point::Coordinate {
        x: grid.x,
        y: grid.y as u32,
    }
}

/// Converts a surface-space pixel position to terminal-space pixel
/// coordinates (accounting for padding/scaling) used by SGR-Pixels mode.
/// Unlike grid conversion, terminal-space coordinates are not clamped
/// and may be negative or exceed the terminal dimensions.
fn pos_to_pixels(pos: Pos, size: renderer_size::Size) -> PixelPoint {
    let coord: renderer_size::CoordinateTerminal =
        match renderer_size::Coordinate::Surface(renderer_size::CoordinateSurface {
            x: pos.x as f64,
            y: pos.y as f64,
        })
        .convert(renderer_size::CoordinateTag::Terminal, size)
        {
            renderer_size::Coordinate::Terminal(t) => t,
            _ => unreachable!(),
        };

    PixelPoint {
        x: coord.x.round() as i32,
        y: coord.y.round() as i32,
    }
}

#[cfg(test)]
fn test_size() -> renderer_size::Size {
    renderer_size::Size {
        screen: renderer_size::ScreenSize {
            width: 1_000,
            height: 1_000,
        },
        cell: renderer_size::CellSize {
            width: 1,
            height: 1,
        },
        padding: renderer_size::Padding {
            top: 0,
            bottom: 0,
            right: 0,
            left: 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn should_report_none_mode_never_reports() {
        let size = test_size();
        for action in [
            mouse::Action::Press,
            mouse::Action::Release,
            mouse::Action::Motion,
        ] {
            assert!(!should_report(
                Event {
                    button: Some(mouse::Button::Left),
                    action,
                    ..Default::default()
                },
                &Options {
                    event: terminal_mouse::Event::None,
                    size,
                    ..opts_defaults()
                },
            ));
        }
    }

    #[test]
    fn should_report_x10_reports_only_left_middle_right_press() {
        let size = test_size();
        // Left, middle, right presses should report.
        for btn in [
            mouse::Button::Left,
            mouse::Button::Middle,
            mouse::Button::Right,
        ] {
            assert!(should_report(
                Event {
                    button: Some(btn),
                    action: mouse::Action::Press,
                    ..Default::default()
                },
                &Options {
                    event: terminal_mouse::Event::X10,
                    size,
                    ..opts_defaults()
                },
            ));
        }

        // Release is not reported.
        assert!(!should_report(
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Release,
                ..Default::default()
            },
            &Options {
                event: terminal_mouse::Event::X10,
                size,
                ..opts_defaults()
            },
        ));

        // Motion is not reported.
        assert!(!should_report(
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Motion,
                ..Default::default()
            },
            &Options {
                event: terminal_mouse::Event::X10,
                size,
                ..opts_defaults()
            },
        ));

        // Other buttons are not reported.
        assert!(!should_report(
            Event {
                button: Some(mouse::Button::Four),
                action: mouse::Action::Press,
                ..Default::default()
            },
            &Options {
                event: terminal_mouse::Event::X10,
                size,
                ..opts_defaults()
            },
        ));

        // Null button is not reported.
        assert!(!should_report(
            Event {
                button: None,
                action: mouse::Action::Press,
                ..Default::default()
            },
            &Options {
                event: terminal_mouse::Event::X10,
                size,
                ..opts_defaults()
            },
        ));
    }

    #[test]
    fn should_report_normal_reports_press_and_release_but_not_motion() {
        let size = test_size();
        assert!(should_report(
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Press,
                ..Default::default()
            },
            &Options {
                event: terminal_mouse::Event::Normal,
                size,
                ..opts_defaults()
            },
        ));

        assert!(should_report(
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Release,
                ..Default::default()
            },
            &Options {
                event: terminal_mouse::Event::Normal,
                size,
                ..opts_defaults()
            },
        ));

        assert!(!should_report(
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Motion,
                ..Default::default()
            },
            &Options {
                event: terminal_mouse::Event::Normal,
                size,
                ..opts_defaults()
            },
        ));
    }

    #[test]
    fn should_report_button_mode_requires_a_button() {
        let size = test_size();
        // With a button, all actions report.
        for action in [
            mouse::Action::Press,
            mouse::Action::Release,
            mouse::Action::Motion,
        ] {
            assert!(should_report(
                Event {
                    button: Some(mouse::Button::Left),
                    action,
                    ..Default::default()
                },
                &Options {
                    event: terminal_mouse::Event::Button,
                    size,
                    ..opts_defaults()
                },
            ));
        }

        // Without a button (null), nothing reports.
        for action in [
            mouse::Action::Press,
            mouse::Action::Release,
            mouse::Action::Motion,
        ] {
            assert!(!should_report(
                Event {
                    button: None,
                    action,
                    ..Default::default()
                },
                &Options {
                    event: terminal_mouse::Event::Button,
                    size,
                    ..opts_defaults()
                },
            ));
        }
    }

    #[test]
    fn should_report_any_mode_reports_everything() {
        let size = test_size();
        for action in [
            mouse::Action::Press,
            mouse::Action::Release,
            mouse::Action::Motion,
        ] {
            assert!(should_report(
                Event {
                    button: Some(mouse::Button::Left),
                    action,
                    ..Default::default()
                },
                &Options {
                    event: terminal_mouse::Event::Any,
                    size,
                    ..opts_defaults()
                },
            ));
        }

        // Even null button + motion reports.
        assert!(should_report(
            Event {
                button: None,
                action: mouse::Action::Motion,
                ..Default::default()
            },
            &Options {
                event: terminal_mouse::Event::Any,
                size,
                ..opts_defaults()
            },
        ));
    }

    #[test]
    fn x10_press_left() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Press,
                mods: key::Mods {
                    shift: true,
                    alt: true,
                    ctrl: true,
                    ..Default::default()
                },
                pos: Pos { x: 0.0, y: 0.0 },
            },
            Options {
                event: terminal_mouse::Event::X10,
                format: terminal_mouse::Format::X10,
                size: test_size(),
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        assert_eq!(&[0x1B, b'[', b'M', 32, 33, 33][..], &data[..]);
    }

    #[test]
    fn x10_ignores_release() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Release,
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::X10,
                format: terminal_mouse::Format::X10,
                size: test_size(),
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        assert_eq!(0usize, data.len());
    }

    #[test]
    fn normal_ignores_motion() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Motion,
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::Normal,
                format: terminal_mouse::Format::Sgr,
                size: test_size(),
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        assert_eq!(0usize, data.len());
    }

    #[test]
    fn button_mode_requires_button() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: None,
                action: mouse::Action::Motion,
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::Button,
                format: terminal_mouse::Format::Sgr,
                size: test_size(),
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        assert_eq!(0usize, data.len());
    }

    #[test]
    fn sgr_release_keeps_button_identity() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Right),
                action: mouse::Action::Release,
                pos: Pos { x: 4.0, y: 5.0 },
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::Any,
                format: terminal_mouse::Format::Sgr,
                size: test_size(),
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        assert_eq!("\x1B[<2;5;6m", std::str::from_utf8(&data).unwrap());
    }

    #[test]
    fn sgr_motion_with_no_button() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: None,
                action: mouse::Action::Motion,
                pos: Pos { x: 1.0, y: 2.0 },
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::Any,
                format: terminal_mouse::Format::Sgr,
                size: test_size(),
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        assert_eq!("\x1B[<35;2;3M", std::str::from_utf8(&data).unwrap());
    }

    #[test]
    fn urxvt_with_modifiers() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Press,
                mods: key::Mods {
                    shift: true,
                    alt: true,
                    ctrl: true,
                    ..Default::default()
                },
                pos: Pos { x: 2.0, y: 3.0 },
            },
            Options {
                event: terminal_mouse::Event::Any,
                format: terminal_mouse::Format::Urxvt,
                size: test_size(),
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        assert_eq!("\x1B[60;3;4M", std::str::from_utf8(&data).unwrap());
    }

    #[test]
    fn utf8_encodes_large_coordinates() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Press,
                pos: Pos { x: 300.0, y: 400.0 },
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::Any,
                format: terminal_mouse::Format::Utf8,
                size: test_size(),
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        let out = &data;
        assert_eq!(&[0x1B, b'[', b'M', 32][..], &out[0..4]);

        // PORT NOTE: the reference decodes the trailing bytes with a
        // `std.unicode.Utf8View` iterator and asserts the codepoints are 333,
        // 433 then `null` (no more). `str::chars()` over the validated UTF-8
        // suffix is the faithful equivalent.
        let view = std::str::from_utf8(&out[4..]).unwrap();
        let mut it = view.chars();
        assert_eq!(333u32, it.next().unwrap() as u32);
        assert_eq!(433u32, it.next().unwrap() as u32);
        assert_eq!(None, it.next());
    }

    #[test]
    fn x10_coordinate_limit() {
        let cap = crate::log_capture::start();
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Press,
                pos: Pos { x: 223.0, y: 0.0 },
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::X10,
                format: terminal_mouse::Format::X10,
                size: test_size(),
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        assert_eq!(0usize, data.len());

        // S106-F2: the reference logs `log.info("X10 mouse format can only
        // encode X/Y up to 223", .{})` (mouse_encode.zig:123) on this exact
        // overflow path.
        let messages = cap.messages();
        assert!(
            messages
                .iter()
                .any(|(level, target, msg)| *level == log::Level::Info
                    && target == "mouse_encode"
                    && msg == "X10 mouse format can only encode X/Y up to 223"),
            "expected the X10-overflow info diagnostic, got: {messages:?}"
        );
    }

    #[test]
    fn sgr_wheel_button_mappings() {
        struct Case {
            button: mouse::Button,
            code: u8,
        }

        for c in [
            Case {
                button: mouse::Button::Four,
                code: 64,
            },
            Case {
                button: mouse::Button::Five,
                code: 65,
            },
            Case {
                button: mouse::Button::Six,
                code: 66,
            },
            Case {
                button: mouse::Button::Seven,
                code: 67,
            },
        ] {
            let mut data: Vec<u8> = Vec::new();
            let mut last: Option<point::Coordinate> = None;
            encode(
                &mut data,
                Event {
                    button: Some(c.button),
                    action: mouse::Action::Press,
                    pos: Pos { x: 0.0, y: 0.0 },
                    ..Default::default()
                },
                Options {
                    event: terminal_mouse::Event::Any,
                    format: terminal_mouse::Format::Sgr,
                    size: test_size(),
                    last_cell: Some(&mut last),
                    ..opts_defaults()
                },
            )
            .unwrap();

            let want = format!("\x1B[<{};1;1M", c.code);
            assert_eq!(want, std::str::from_utf8(&data).unwrap());
        }
    }

    #[test]
    fn urxvt_release_uses_legacy_button_3_encoding() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Right),
                action: mouse::Action::Release,
                pos: Pos { x: 2.0, y: 3.0 },
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::Any,
                format: terminal_mouse::Format::Urxvt,
                size: test_size(),
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        assert_eq!("\x1B[35;3;4M", std::str::from_utf8(&data).unwrap());
    }

    #[test]
    fn unsupported_button_is_ignored() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Ten),
                action: mouse::Action::Press,
                pos: Pos { x: 1.0, y: 1.0 },
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::Any,
                format: terminal_mouse::Format::Sgr,
                size: test_size(),
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        assert_eq!(0usize, data.len());
    }

    #[test]
    fn sgr_pixels_uses_terminal_space_cursor_coordinates() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Press,
                pos: Pos { x: 10.0, y: 20.0 },
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::Any,
                format: terminal_mouse::Format::SgrPixels,
                size: test_size(),
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        assert_eq!("\x1B[<0;10;20M", std::str::from_utf8(&data).unwrap());
    }

    #[test]
    fn sgr_pixels_release_keeps_button_identity() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Right),
                action: mouse::Action::Release,
                pos: Pos { x: 10.0, y: 20.0 },
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::Any,
                format: terminal_mouse::Format::SgrPixels,
                size: test_size(),
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        assert_eq!("\x1B[<2;10;20m", std::str::from_utf8(&data).unwrap());
    }

    #[test]
    fn position_exactly_at_viewport_boundary_is_encoded_in_final_cell() {
        let size: renderer_size::Size = renderer_size::Size {
            screen: renderer_size::ScreenSize {
                width: 10,
                height: 10,
            },
            cell: renderer_size::CellSize {
                width: 2,
                height: 2,
            },
            padding: renderer_size::Padding {
                top: 0,
                bottom: 0,
                right: 0,
                left: 0,
            },
        };

        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Press,
                pos: Pos { x: 10.0, y: 10.0 },
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::Any,
                format: terminal_mouse::Format::Sgr,
                size,
                last_cell: Some(&mut last),
                ..opts_defaults()
            },
        )
        .unwrap();

        assert_eq!("\x1B[<0;5;5M", std::str::from_utf8(&data).unwrap());
    }

    #[test]
    fn outside_viewport_motion_with_no_pressed_button_is_ignored() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Motion,
                pos: Pos { x: -1.0, y: -1.0 },
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::Any,
                format: terminal_mouse::Format::Sgr,
                size: test_size(),
                any_button_pressed: false,
                last_cell: Some(&mut last),
            },
        )
        .unwrap();

        assert_eq!(0usize, data.len());
    }

    #[test]
    fn outside_viewport_motion_with_pressed_button_is_reported() {
        let mut data: Vec<u8> = Vec::new();
        let mut last: Option<point::Coordinate> = None;
        encode(
            &mut data,
            Event {
                button: Some(mouse::Button::Left),
                action: mouse::Action::Motion,
                pos: Pos { x: -1.0, y: -1.0 },
                ..Default::default()
            },
            Options {
                event: terminal_mouse::Event::Any,
                format: terminal_mouse::Format::Sgr,
                size: test_size(),
                any_button_pressed: true,
                last_cell: Some(&mut last),
            },
        )
        .unwrap();

        assert_eq!("\x1B[<32;1;1M", std::str::from_utf8(&data).unwrap());
    }

    #[test]
    fn motion_is_deduped_by_last_cell_except_sgr_pixels() {
        let mut last: Option<point::Coordinate> = None;

        {
            let mut data: Vec<u8> = Vec::new();
            encode(
                &mut data,
                Event {
                    button: Some(mouse::Button::Left),
                    action: mouse::Action::Motion,
                    pos: Pos { x: 5.0, y: 6.0 },
                    ..Default::default()
                },
                Options {
                    event: terminal_mouse::Event::Any,
                    format: terminal_mouse::Format::Sgr,
                    size: test_size(),
                    last_cell: Some(&mut last),
                    ..opts_defaults()
                },
            )
            .unwrap();
            assert!(!data.is_empty());
        }

        {
            let mut data: Vec<u8> = Vec::new();
            encode(
                &mut data,
                Event {
                    button: Some(mouse::Button::Left),
                    action: mouse::Action::Motion,
                    pos: Pos { x: 5.0, y: 6.0 },
                    ..Default::default()
                },
                Options {
                    event: terminal_mouse::Event::Any,
                    format: terminal_mouse::Format::Sgr,
                    size: test_size(),
                    last_cell: Some(&mut last),
                    ..opts_defaults()
                },
            )
            .unwrap();
            assert_eq!(0usize, data.len());
        }

        {
            let mut data: Vec<u8> = Vec::new();
            encode(
                &mut data,
                Event {
                    button: Some(mouse::Button::Left),
                    action: mouse::Action::Motion,
                    pos: Pos { x: 5.0, y: 6.0 },
                    ..Default::default()
                },
                Options {
                    event: terminal_mouse::Event::Any,
                    format: terminal_mouse::Format::SgrPixels,
                    size: test_size(),
                    last_cell: Some(&mut last),
                    ..opts_defaults()
                },
            )
            .unwrap();
            assert!(!data.is_empty());
        }
    }

    // PORT NOTE: Rust struct-update (`..opts_defaults()`) needs a base value for
    // the fields a given call site does not set. `Options` has no `Default`
    // (its `size` is required), so the tests use this helper to supply the
    // reference's field defaults (`event = .none`, `format = .x10`,
    // `any_button_pressed = false`, `last_cell = null`) plus a placeholder
    // `size` that every call site immediately overrides. This is test glue only;
    // it adds no behavior to the encoder.
    fn opts_defaults() -> Options<'static> {
        Options {
            event: terminal_mouse::Event::None,
            format: terminal_mouse::Format::X10,
            size: renderer_size::Size {
                screen: renderer_size::ScreenSize {
                    width: 0,
                    height: 0,
                },
                cell: renderer_size::CellSize {
                    width: 1,
                    height: 1,
                },
                padding: renderer_size::Padding {
                    top: 0,
                    bottom: 0,
                    right: 0,
                    left: 0,
                },
            },
            any_button_pressed: false,
            last_cell: None,
        }
    }
}
