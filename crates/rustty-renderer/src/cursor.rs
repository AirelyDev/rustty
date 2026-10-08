//! PORTS: ghostty src/renderer/cursor.zig
//!
//! PORT STATUS: done — every `test{}` block ported.

// const std = @import("std");
// const terminal = @import("../terminal/main.zig");
use rustty_vt::render::RenderState;

// PORT NOTE: the reference `terminal.CursorStyle` is `Screen.CursorStyle`, ported
// as `rustty_vt::cursor::Style`. Aliased here to `TerminalCursorStyle` so the
// `fromTerminal` port reads against the reference name without colliding with
// this module's own `Style`.
use rustty_vt::cursor::Style as TerminalCursorStyle;

/// Available cursor styles for drawing that renderers must support.
/// This is a superset of terminal cursor styles since the renderer supports
/// some additional cursor states such as the hollow block.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    // Typical cursor input styles
    Block,
    BlockHollow,
    Bar,
    Underline,

    // Special cursor styles
    Lock,
}

impl Style {
    /// Create a cursor style from the terminal style request.
    pub fn from_terminal(term: TerminalCursorStyle) -> Style {
        match term {
            TerminalCursorStyle::Bar => Style::Bar,
            TerminalCursorStyle::Block => Style::Block,
            TerminalCursorStyle::BlockHollow => Style::BlockHollow,
            TerminalCursorStyle::Underline => Style::Underline,
        }
    }
}

// pub const StyleOptions = struct { preedit, focused, blink_visible };
#[derive(Clone, Copy, Default)]
pub struct StyleOptions {
    pub preedit: bool,
    pub focused: bool,
    pub blink_visible: bool,
}

/// Returns the cursor style to use for the current render state or null
/// if a cursor should not be rendered at all.
// PARITY: the reference is a sequence of `if (cond) return ...;` guards; the
// leading `if (state.cursor.viewport == null) return null;` is kept as an
// explicit guard (not collapsed to `?`) so the ported control flow matches the
// reference statement-for-statement.
#[allow(clippy::question_mark)]
pub fn style(state: &RenderState, opts: StyleOptions) -> Option<Style> {
    // Note the order of conditionals below is important. It represents
    // a priority system of how we determine what state overrides cursor
    // visibility and style.

    // The cursor must be visible in the viewport to be rendered.
    if state.cursor.viewport.is_none() {
        return None;
    }

    // If we are in preedit, then we always show the block cursor. We do
    // this even if the cursor is explicitly not visible because it shows
    // an important editing state to the user.
    if opts.preedit {
        return Some(Style::Block);
    }

    // If we're at a password input its always a lock.
    if state.cursor.password_input {
        return Some(Style::Lock);
    }

    // If the cursor is explicitly not visible by terminal mode, we don't render.
    if !state.cursor.visible {
        return None;
    }

    // If we're not focused, our cursor is always visible so that
    // we can show the hollow box.
    if !opts.focused {
        return Some(Style::BlockHollow);
    }

    // If the cursor is blinking and our blink state is not visible,
    // then we don't show the cursor.
    if state.cursor.blinking && !opts.blink_visible {
        return None;
    }

    // Otherwise, we use whatever style the terminal wants.
    Some(Style::from_terminal(state.cursor.visual_style))
}

#[cfg(test)]
// PARITY: the reference assertions compare `style(...) == null`; the ported
// assertions keep that equality form (`== None`) rather than `.is_none()`.
#[allow(clippy::partialeq_to_none)]
mod tests {
    use super::*;
    use rustty_vt::alloc::Global;
    use rustty_vt::cursor::Style as VtCursorStyle;
    use rustty_vt::kitty;
    use rustty_vt::modes::{self as modespkg, Mode};
    use rustty_vt::size;
    use rustty_vt::terminal::{Colors, Options, ScrollViewport, Terminal};

    // PORT NOTE: the reference `Terminal.init(alloc, .{ .cols, .rows })` relies on
    // `Options` field defaults; the Rust `Options` has no field defaults, so this
    // helper supplies them verbatim (mirrors the rustty-vt render.rs test helper).
    fn opts(cols: size::CellCountInt, rows: size::CellCountInt) -> Options {
        Options {
            cols,
            rows,
            max_scrollback_bytes: Some(10_000),
            max_scrollback_lines: None,
            colors: Colors::default(),
            default_modes: modespkg::ModePacked::default(),
            default_cursor_style: rustty_vt::screen::CursorStyle::Block,
            default_cursor_blink: Some(false),
            default_xt_checksum: Default::default(),
            kitty_image_storage_limit: 320 * 1000 * 1000,
            kitty_image_loading_limits: kitty::graphics_image::Limits::DIRECT,
        }
    }

    // test "cursor: default uses configured style"
    #[test]
    fn cursor_default_uses_configured_style() {
        let alloc = Global;
        let mut term: Terminal = Terminal::init(alloc, opts(10, 10)).unwrap();

        unsafe {
            (*term.screens.active).cursor.cursor_style = VtCursorStyle::Bar;
        }
        term.modes.set(Mode::CursorBlinking, true);

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut term).unwrap();

        assert!(
            style(
                &state,
                StyleOptions {
                    preedit: false,
                    focused: true,
                    blink_visible: true
                }
            ) == Some(Style::Bar)
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    preedit: false,
                    focused: false,
                    blink_visible: true
                }
            ) == Some(Style::BlockHollow)
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    preedit: false,
                    focused: false,
                    blink_visible: false
                }
            ) == Some(Style::BlockHollow)
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    preedit: false,
                    focused: true,
                    blink_visible: false
                }
            ) == None
        );

        state.deinit(alloc);
        term.deinit(alloc);
    }

    // test "cursor: blinking disabled"
    #[test]
    fn cursor_blinking_disabled() {
        let alloc = Global;
        let mut term = Terminal::init(alloc, opts(10, 10)).unwrap();

        unsafe {
            (*term.screens.active).cursor.cursor_style = VtCursorStyle::Bar;
        }
        term.modes.set(Mode::CursorBlinking, false);

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut term).unwrap();

        assert!(
            style(
                &state,
                StyleOptions {
                    focused: true,
                    blink_visible: true,
                    ..StyleOptions::default()
                }
            ) == Some(Style::Bar)
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    focused: true,
                    blink_visible: false,
                    ..StyleOptions::default()
                }
            ) == Some(Style::Bar)
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    focused: false,
                    blink_visible: true,
                    ..StyleOptions::default()
                }
            ) == Some(Style::BlockHollow)
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    focused: false,
                    blink_visible: false,
                    ..StyleOptions::default()
                }
            ) == Some(Style::BlockHollow)
        );

        state.deinit(alloc);
        term.deinit(alloc);
    }

    // test "cursor: explicitly not visible"
    #[test]
    fn cursor_explicitly_not_visible() {
        let alloc = Global;
        let mut term = Terminal::init(alloc, opts(10, 10)).unwrap();

        unsafe {
            (*term.screens.active).cursor.cursor_style = VtCursorStyle::Bar;
        }
        term.modes.set(Mode::CursorVisible, false);
        term.modes.set(Mode::CursorBlinking, false);

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut term).unwrap();

        assert!(
            style(
                &state,
                StyleOptions {
                    focused: true,
                    blink_visible: true,
                    ..StyleOptions::default()
                }
            ) == None
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    focused: true,
                    blink_visible: false,
                    ..StyleOptions::default()
                }
            ) == None
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    focused: false,
                    blink_visible: true,
                    ..StyleOptions::default()
                }
            ) == None
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    focused: false,
                    blink_visible: false,
                    ..StyleOptions::default()
                }
            ) == None
        );

        state.deinit(alloc);
        term.deinit(alloc);
    }

    // test "cursor: always block with preedit"
    #[test]
    fn cursor_always_block_with_preedit() {
        let alloc = Global;
        let mut term = Terminal::init(alloc, opts(10, 10)).unwrap();

        let mut state: RenderState = RenderState::empty();
        state.update(alloc, &mut term).unwrap();

        // In any bool state
        assert!(
            style(
                &state,
                StyleOptions {
                    preedit: true,
                    focused: false,
                    blink_visible: false
                }
            ) == Some(Style::Block)
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    preedit: true,
                    focused: true,
                    blink_visible: false
                }
            ) == Some(Style::Block)
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    preedit: true,
                    focused: true,
                    blink_visible: true
                }
            ) == Some(Style::Block)
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    preedit: true,
                    focused: false,
                    blink_visible: true
                }
            ) == Some(Style::Block)
        );

        // If we're scrolled though, then we don't show the cursor.
        for _ in 0..100 {
            term.index().unwrap();
        }
        term.scroll_viewport(ScrollViewport::Top);
        state.update(alloc, &mut term).unwrap();

        // In any bool state
        assert!(
            style(
                &state,
                StyleOptions {
                    preedit: true,
                    focused: false,
                    blink_visible: false
                }
            ) == None
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    preedit: true,
                    focused: true,
                    blink_visible: false
                }
            ) == None
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    preedit: true,
                    focused: true,
                    blink_visible: true
                }
            ) == None
        );
        assert!(
            style(
                &state,
                StyleOptions {
                    preedit: true,
                    focused: false,
                    blink_visible: true
                }
            ) == None
        );

        state.deinit(alloc);
        term.deinit(alloc);
    }
}
