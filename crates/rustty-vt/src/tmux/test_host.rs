//! Test-only host that drives tmux control mode from raw terminal bytes.
//!
//! No reference counterpart. The reference tests call `Parser.put` and
//! `Viewer.next` directly. The R13 tests (task R13-A) need the whole input
//! path, so this host feeds bytes through the VT `Stream`, the DCS handler, the
//! control-mode parser and the viewer, the way termio's
//! `stream_handler.zig` `dcsCommand` tmux arm does (without its mailbox).
//! Commands the viewer sends to tmux are recorded instead of written.

use crate::alloc::Global;

use crate::dcs;
use crate::parser;
use crate::stream;

use super::viewer::{Action, Input, Viewer};
use super::ControlNotification;

#[derive(Default)]
pub(crate) struct Host {
    pub dcs: dcs::Handler,
    pub viewer: Option<Viewer>,
    /// Every notification the control parser produced, by kind name.
    pub notifications: Vec<&'static str>,
    /// Commands the viewer asked to send to tmux.
    pub commands: Vec<Vec<u8>>,
    /// `.exit` actions from the viewer (it became defunct).
    pub viewer_exits: usize,
}

pub(crate) type HostStream = stream::Stream<'static, Host>;

pub(crate) fn stream() -> HostStream {
    stream::Stream::init(stream::Options::with_handler(Host::default()))
}

fn kind(n: &ControlNotification) -> &'static str {
    match n {
        ControlNotification::Enter => "enter",
        ControlNotification::Exit => "exit",
        ControlNotification::BlockEnd(_) => "block_end",
        ControlNotification::BlockErr(_) => "block_err",
        ControlNotification::Output { .. } => "output",
        ControlNotification::SessionChanged { .. } => "session_changed",
        ControlNotification::SessionsChanged => "sessions_changed",
        ControlNotification::LayoutChange { .. } => "layout_change",
        ControlNotification::WindowAdd { .. } => "window_add",
        ControlNotification::WindowRenamed { .. } => "window_renamed",
        ControlNotification::WindowPaneChanged { .. } => "window_pane_changed",
        ControlNotification::ClientDetached { .. } => "client_detached",
        ControlNotification::ClientSessionChanged { .. } => "client_session_changed",
    }
}

impl Host {
    fn command(&mut self, cmd: Option<dcs::Command>) {
        let Some(mut cmd) = cmd else { return };
        if let dcs::Command::Tmux(n) = &mut cmd {
            let n = core::mem::replace(n, ControlNotification::Exit);
            self.notifications.push(kind(&n));
            match n {
                ControlNotification::Enter => {
                    assert!(self.viewer.is_none());
                    self.viewer = Some(Viewer::init(Global).expect("Viewer::init"));
                }
                ControlNotification::Exit => {
                    if let Some(mut viewer) = self.viewer.take() {
                        viewer.deinit();
                    }
                }
                n => {
                    if let Some(viewer) = self.viewer.as_mut() {
                        for action in viewer.next(Input::Tmux(n)) {
                            match action {
                                Action::Exit => self.viewer_exits += 1,
                                Action::Command(c) => self.commands.push(c.clone()),
                                Action::Windows(_) => {}
                            }
                        }
                    }
                }
            }
        }
        cmd.deinit();
    }

    pub fn viewer(&mut self) -> &mut Viewer {
        self.viewer.as_mut().expect("the tmux viewer must be live")
    }
}

impl stream::Handler for Host {
    fn deinit(&mut self) {
        self.dcs.deinit();
        if let Some(mut viewer) = self.viewer.take() {
            viewer.deinit();
        }
    }

    fn dcs_hook(&mut self, d: parser::DCS) {
        let cmd = self.dcs.hook(
            Global,
            dcs::DCS {
                intermediates: &d.intermediates,
                params: &d.params,
                final_: d.final_,
            },
        );
        self.command(cmd);
    }

    fn dcs_put(&mut self, byte: u8) {
        let cmd = self.dcs.put(byte);
        self.command(cmd);
    }

    fn dcs_unhook(&mut self) {
        let cmd = self.dcs.unhook();
        self.command(cmd);
    }
}

/// A `%begin`/`%end` block carrying `content`, as tmux frames a command reply.
pub(crate) fn block(n: usize, content: &[u8]) -> Vec<u8> {
    let mut out = format!("%begin {n} {n} 1\n").into_bytes();
    out.extend_from_slice(content);
    if !content.is_empty() && !content.ends_with(b"\n") {
        out.push(b'\n');
    }
    out.extend_from_slice(format!("%end {n} {n} 1\n").as_bytes());
    out
}

/// Attach and bring the viewer to the point where it has asked for
/// `list-windows`: the startup block, the session change, and the version reply.
pub(crate) fn attach(s: &mut HostStream) {
    s.next_slice(b"\x1bP1000p");
    s.next_slice(&block(1, b""));
    s.next_slice(b"%session-changed $1 main\n");
    s.next_slice(&block(2, b"3.5a"));
    let last = s
        .handler
        .commands
        .last()
        .expect("the viewer must ask tmux for something");
    assert!(
        last.starts_with(b"list-windows"),
        "the viewer must be waiting for list-windows, got {:?}",
        String::from_utf8_lossy(last)
    );
}

/// `XXXX,<layout>` with the checksum tmux would compute.
pub(crate) fn with_checksum(layout: &[u8]) -> Vec<u8> {
    let mut out = super::layout::Checksum::calculate(layout)
        .as_string()
        .to_vec();
    out.push(b',');
    out.extend_from_slice(layout);
    out
}

/// The `list-windows` reply line for one window (`Format.list_windows`).
pub(crate) fn list_windows_line(
    window_id: usize,
    width: usize,
    height: usize,
    layout: &[u8],
) -> Vec<u8> {
    let mut out = format!("$1 @{window_id} {width} {height} ").into_bytes();
    out.extend_from_slice(&with_checksum(layout));
    out
}

/// Whether a `capture-pane` command asks for `-C`: a flag cluster (`-C`,
/// `-peC`, ...) containing `C`. Option arguments (`-S -`, `-E -1`, `-t %7`)
/// are never all-alphabetic clusters containing `C`, so they cannot match.
fn capture_has_escape_flag(cmd: &[u8]) -> bool {
    cmd.split(|b| b.is_ascii_whitespace()).any(|tok| {
        tok.len() > 1
            && tok[0] == b'-'
            && tok[1..].iter().all(u8::is_ascii_alphabetic)
            && tok[1..].contains(&b'C')
    })
}

/// The reply tmux writes to a control client for `cmd`, a `capture-pane -p -e`
/// command, when the pane's `-e` capture is `raw` (grid text with its SGR,
/// SO/SI and OSC 8 sequences, lines joined by `\n`, the final newline already
/// stripped as `-p` does).
///
/// tmux writes the capture into the `%begin`/`%end` block as-is
/// (`cmd-capture-pane.c` `cmd_capture_pane_exec`: `control_write(c, "%.*s",
/// ...)`), so without `-C` every sequence arrives with a raw ESC. With `-C`,
/// `grid.c` `grid_string_cells_code`, `grid_string_cells_add_code` and
/// `grid_string_cells_add_hyperlink` write each ESC as the four characters
/// `\033`, SO and SI as `\016` and `\017`, and `grid_string_cells` writes a
/// one-byte `\` cell as `\\` (tmux 1.8, where `-C` arrived, through master
/// `72e83b00`). The grid holds no other C0 byte (a tab cell is written as a raw
/// `\t` either way), so mapping every byte of `raw` is the same transformation,
/// with one caveat: a hyperlink's URI and id arrive already vis-encoded
/// (`hyperlinks.c` `utf8_stravis`, tmux 3.4 onward) and `-C` copies them
/// verbatim, so this models tmux only while `raw`'s URIs and ids contain no
/// `\` (as in the tests that use it).
pub(crate) fn capture_reply(cmd: &[u8], raw: &[u8]) -> Vec<u8> {
    assert!(
        cmd.starts_with(b"capture-pane "),
        "not a capture: {:?}",
        String::from_utf8_lossy(cmd)
    );
    if !capture_has_escape_flag(cmd) {
        return raw.to_vec();
    }
    let mut out = Vec::with_capacity(raw.len() * 2);
    for &b in raw {
        match b {
            0x1B => out.extend_from_slice(b"\\033"),
            0x0E => out.extend_from_slice(b"\\016"),
            0x0F => out.extend_from_slice(b"\\017"),
            b'\\' => out.extend_from_slice(b"\\\\"),
            _ => {
                assert!(
                    b >= b' ' || b == b'\t' || b == b'\n',
                    "not grid text: {b:#x}"
                );
                out.push(b);
            }
        }
    }
    out
}

/// `capture_reply` escapes only for a command carrying `-C`, alone or in a
/// flag cluster, and never mistakes an option argument for it.
#[test]
fn capture_escape_flag_detection() {
    assert!(capture_has_escape_flag(b"capture-pane -peC -t %1\n"));
    assert!(capture_has_escape_flag(
        b"capture-pane -p -e -C -q -S - -E -1 -t %1\n"
    ));
    assert!(!capture_has_escape_flag(
        b"capture-pane -p -e -q -S - -E -1 -t %1\n"
    ));
}
