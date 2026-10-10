//! OSC (Operating System Command) related functions and types.
//!
//! OSC is another set of control sequences for terminal programs that start with
//! "ESC ]". Unlike CSI or standard ESC sequences, they may contain strings
//! and other irregular formatting so a dedicated parser is created to handle it.
//!
//! PORTS: ghostty src/terminal/osc.zig
//!
//! PORT STATUS: ported — Command enum + Terminator + Parser state machine
//! (init/deinit/reset/next/nextSlice/end, Capture writeByte/writeSlice) and
//! the per-command parsers in `parsers/*` are all ported with their tests. osc.zig's own two `test {}` blocks (the
//! `ProgressReport.State` ghostty.h check and the bottom `parsers`/`encoding`
//! reference) are ported below.

// PORT NOTE: Zig imports retargeted to ported crate paths (R2). `lib.Enum` is
// `rustty_lib::Enum`; `kitty_color` is `crate::kitty::color`; `parsers`/
// `encoding` are submodules of this module.
use rustty_lib::r#enum::EnumType;
use rustty_lib::target::Target;

use crate::kitty::color as kitty_color;

pub mod encoding;
// PORT NOTE (ca9e5b130): `osc/kitty_metadata.zig` has no importer in osc.zig
// itself (the parsers import it as `@import("../kitty_metadata.zig")`); the
// Rust module tree registers it here, on the `osc/` directory owner.
pub mod kitty_metadata;
pub mod parsers;

// pub const color = parsers.color;
pub use parsers::color;
// pub const semantic_prompt = parsers.semantic_prompt;
pub use parsers::semantic_prompt;

/// `[:0]const u8`: a byte string BORROWED from the parser, whose terminating
/// NUL sentinel is part of the borrow.
///
/// PORT NOTE (parity tail S7-7, N263): the reference's text payloads are
/// sentinel-terminated slices into the parser's own storage (the fixed
/// `buffer` or the allocating capture). They used to be owned `Vec<u8>` copies
/// here, which the reference never makes. The sentinel is kept inside the
/// borrow so that a C caller reading the string through [`ZStr::as_ptr`]
/// (`rustty_osc_command_data`'s `const char *`) stays within the memory the
/// borrow covers. Dereferencing gives the bytes WITHOUT the sentinel, which is
/// what `[:0]const u8` indexes and what `.len` counts.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ZStr<'p> {
    with_nul: &'p [u8],
}

impl<'p> ZStr<'p> {
    /// The empty string (`""`, a lone sentinel in static memory).
    pub const EMPTY: ZStr<'static> = ZStr { with_nul: b"\0" };

    /// `data[start..end :0]`: `with_nul` is the slice INCLUDING its sentinel.
    ///
    /// Zig's sentinel slice checks the sentinel (a safety check); so does this,
    /// in every build, because [`ZStr::as_ptr`] promises C a terminated string.
    #[inline]
    #[track_caller]
    pub fn from_bytes_with_nul(with_nul: &'p [u8]) -> ZStr<'p> {
        assert_eq!(Some(&0), with_nul.last(), "[:0] slice without its sentinel");
        ZStr { with_nul }
    }

    /// `data[start..end :0]`: the bytes `data[start..end]`, whose sentinel is
    /// `data[end]`.
    #[inline]
    #[track_caller]
    pub fn sentinel(data: &'p [u8], start: usize, end: usize) -> ZStr<'p> {
        ZStr::from_bytes_with_nul(&data[start..end + 1])
    }

    /// The bytes, without the sentinel (the `Vec::as_slice` spelling).
    #[inline]
    pub fn as_slice(&self) -> &'p [u8] {
        self.as_bytes()
    }

    /// The bytes, without the sentinel.
    #[inline]
    pub fn as_bytes(&self) -> &'p [u8] {
        &self.with_nul[..self.with_nul.len() - 1]
    }

    /// The bytes followed by the sentinel.
    #[inline]
    pub fn as_bytes_with_nul(&self) -> &'p [u8] {
        self.with_nul
    }

    /// `.ptr`: a NUL-terminated C string, valid while the borrow is.
    #[inline]
    pub fn as_ptr(&self) -> *const u8 {
        self.with_nul.as_ptr()
    }
}

impl core::ops::Deref for ZStr<'_> {
    type Target = [u8];
    #[inline]
    fn deref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl AsRef<[u8]> for ZStr<'_> {
    #[inline]
    fn as_ref(&self) -> &[u8] {
        self.as_bytes()
    }
}

impl core::fmt::Debug for ZStr<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{:?}", self.as_bytes().escape_ascii().to_string())
    }
}

impl PartialEq<[u8]> for ZStr<'_> {
    fn eq(&self, other: &[u8]) -> bool {
        self.as_bytes() == other
    }
}

impl PartialEq<&[u8]> for ZStr<'_> {
    fn eq(&self, other: &&[u8]) -> bool {
        self.as_bytes() == *other
    }
}

impl<const N: usize> PartialEq<[u8; N]> for ZStr<'_> {
    fn eq(&self, other: &[u8; N]) -> bool {
        self.as_bytes() == other.as_slice()
    }
}

impl<const N: usize> PartialEq<&[u8; N]> for ZStr<'_> {
    fn eq(&self, other: &&[u8; N]) -> bool {
        self.as_bytes() == other.as_slice()
    }
}

// PORT NOTE (parity tail S7-7, N263): `Command<'p>` borrows everything from
// the parser that produced it: its text payloads (see `ZStr`), as the
// reference's slices do, and the two lists the reference grows through
// `parser.alloc` (`color_operation.requests`, `kitty_color_protocol.list`),
// which the parser keeps and frees at `reset`, as the reference's
// `parser.command` does. The borrow is the reference's documented lifetime,
// "only valid until the next call to the parser", enforced by the borrow
// checker: `Parser::end` returns `Command<'_>` over `&mut self`, so the parser
// cannot be fed, reset or ended again while the command lives. Owning
// nothing, it is `Copy`, like the reference's by-value `cmd.*`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Command<'p> {
    /// This generally shouldn't ever be set except as an initial zero value.
    /// Ignore it.
    Invalid,

    /// Set the window title of the terminal
    ///
    /// If title mode 0 is set text is expect to be hex encoded (i.e. utf-8
    /// with each code unit further encoded with two hex digits).
    ///
    /// If title mode 2 is set or the terminal is setup for unconditional
    /// utf-8 titles text is interpreted as utf-8. Else text is interpreted
    /// as latin1.
    ///
    /// PORT NOTE (fidelity): the reference declares this `[:0]const u8` (raw,
    /// NUL-terminated bytes), borrowed from the parser: [`ZStr`], NOT a lossy
    /// `String`.
    ChangeWindowTitle(ZStr<'p>),

    /// Set the icon of the terminal window. The name of the icon is not
    /// well defined, so this is currently ignored by Ghostty at the time
    /// of writing this. We just parse it so that we don't get parse errors
    /// in the log.
    ///
    /// PORT NOTE (fidelity): reference `[:0]const u8` — raw bytes, not lossy.
    ChangeWindowIcon(ZStr<'p>),

    /// Semantic prompt command: https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md
    SemanticPrompt(SemanticPrompt<'p>),

    /// Set or get clipboard contents. If data is "?", then the current
    /// clipboard contents are sent to the pty. Otherwise, the contents
    /// are set on the clipboard.
    ClipboardContents {
        kind: u8,
        /// PORT NOTE (fidelity): reference `data: [:0]const u8` — raw bytes.
        data: ZStr<'p>,
        // terminator: Terminator = .st,
        // (e03475c0c: captured so the OSC 52 read response can echo it.)
        terminator: Terminator,
    },

    /// OSC 7. Reports the current working directory of the shell. This is
    /// a moderately flawed escape sequence but one that many major terminals
    /// support so we also support it. To understand the flaws, read through
    /// this terminal-wg issue: https://gitlab.freedesktop.org/terminal-wg/specifications/-/issues/20
    ReportPwd {
        /// The reported pwd value. This is not checked for validity. It should
        /// be a file URL but it is up to the caller to utilize this value.
        ///
        /// PORT NOTE (fidelity): reference `value: [:0]const u8` — raw bytes.
        value: ZStr<'p>,
    },

    /// OSC 22. Set the mouse shape. There doesn't seem to be a standard
    /// naming scheme for cursors but it looks like terminals such as Foot
    /// are moving towards using the W3C CSS cursor names. For OSC parsing,
    /// we just parse whatever string is given.
    MouseShape {
        /// PORT NOTE (fidelity): reference `value: [:0]const u8` — raw bytes.
        value: ZStr<'p>,
    },

    /// OSC color operations to set, reset, or report color settings. Some OSCs
    /// allow multiple operations to be specified in a single OSC so we need a
    /// list-like datastructure to manage them. We use std.SegmentedList because
    /// it minimizes the number of allocations and copies because a large
    /// majority of the time there will be only one operation per OSC.
    ///
    /// Currently, these OSCs are handled by `color_operation`:
    ///
    /// 4, 5, 10-19, 104, 105, 110-119
    ColorOperation {
        op: color::Operation,
        // PORT NOTE (parity tail S7-7): the parser's `color_requests`.
        requests: &'p color::List,
        terminator: Terminator,
    },

    /// Kitty color protocol, OSC 21
    /// https://sw.kovidgoyal.net/kitty/color-stack/#id1
    // PORT NOTE (parity tail S7-7): the parser's `kitty_color`.
    KittyColorProtocol(&'p kitty_color::OSC),

    /// Show a desktop notification (OSC 9 or OSC 777)
    ShowDesktopNotification {
        /// PORT NOTE (fidelity): reference `title: [:0]const u8` — raw bytes.
        title: ZStr<'p>,
        /// PORT NOTE (fidelity): reference `body: [:0]const u8` — raw bytes.
        body: ZStr<'p>,
    },

    /// Start a hyperlink (OSC 8)
    HyperlinkStart {
        /// PORT NOTE (fidelity): reference `id: ?[:0]const u8` — optional raw bytes.
        id: Option<ZStr<'p>>,
        /// PORT NOTE (fidelity): reference `uri: [:0]const u8` — raw bytes.
        uri: ZStr<'p>,
    },

    /// End a hyperlink (OSC 8)
    HyperlinkEnd,

    /// ConEmu sleep (OSC 9;1)
    ConemuSleep {
        duration_ms: u16,
    },

    /// ConEmu show GUI message box (OSC 9;2)
    ///
    /// PORT NOTE (fidelity): reference `[:0]const u8` — raw bytes.
    ConemuShowMessageBox(ZStr<'p>),

    /// ConEmu change tab title (OSC 9;3)
    ConemuChangeTabTitle(ConemuChangeTabTitle<'p>),

    /// ConEmu progress report (OSC 9;4)
    ConemuProgressReport(ProgressReport),

    /// ConEmu wait input (OSC 9;5)
    ConemuWaitInput,

    /// ConEmu GUI macro (OSC 9;6)
    ///
    /// PORT NOTE (fidelity): reference `[:0]const u8` — raw bytes.
    ConemuGuimacro(ZStr<'p>),

    /// ConEmu run process (OSC 9;7)
    ///
    /// PORT NOTE (fidelity): reference `[:0]const u8` — raw bytes.
    ConemuRunProcess(ZStr<'p>),

    /// ConEmu output environment variable (OSC 9;8)
    ///
    /// PORT NOTE (fidelity): reference `[:0]const u8` — raw bytes.
    ConemuOutputEnvironmentVariable(ZStr<'p>),

    /// ConEmu XTerm keyboard and output emulation (OSC 9;10)
    /// https://conemu.github.io/en/TerminalModes.html
    ConemuXtermEmulation {
        /// null => do not change
        /// false => turn off
        /// true => turn on
        keyboard: Option<bool>,
        /// null => do not change
        /// false => turn off
        /// true => turn on
        output: Option<bool>,
    },

    /// ConEmu comment (OSC 9;11)
    ///
    /// PORT NOTE (fidelity): reference `[:0]const u8` — raw bytes.
    ConemuComment(ZStr<'p>),

    /// Kitty text sizing protocol (OSC 66)
    KittyTextSizing(parsers::kitty_text_sizing::OSC<'p>),

    KittyClipboardProtocol(KittyClipboardProtocol<'p>),

    /// Kitty drag and drop protocol (OSC 72)
    /// https://sw.kovidgoyal.net/kitty/drag-and-drop-protocol/
    KittyDndProtocol(KittyDndProtocol<'p>),

    /// OSC 3008. Hierarchical context signalling (UAPI spec).
    /// https://uapi-group.org/specifications/specs/osc_context/
    ContextSignal(parsers::context_signal::Command<'p>),

    /// Kitty desktop notifications (OSC 99)
    KittyDesktopNotification(KittyDesktopNotification<'p>),

    /// An OSC sequence whose number this parser does not implement. Only
    /// produced when `Parser.unknown_max_bytes` is nonzero.
    Unknown(Unknown<'p>),

    /// Program status protocol (OSC 7501). A program reports what it is
    /// doing, such as working or waiting on the user, or asks whether the
    /// terminal supports the protocol. See `ProgramStatus`.
    ProgramStatus(ProgramStatus<'p>),
}

/// An OSC sequence whose number this parser does not implement. For
/// the sequence `ESC ] 7400;status=busy BEL`, `content` is
/// "7400;status=busy" and `terminator` is `.bel`.
// PORT NOTE: `Command.Unknown`, hoisted to module level like the other
// payload types. `Unknown.C` and `cval()` belong to the C API: they are
// `rustty_vt_c::osc::UnknownOscSequence` (P8-W5 T3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unknown<'p> {
    /// Every byte fed to the parser for this sequence, including the
    /// number at the start. Owned by the parser and only valid until
    /// the next parser call.
    ///
    /// When the bytes come from the VT stream, this never contains
    /// control characters (bytes below 0x20). Those end the sequence,
    /// cancel it, or are dropped before they reach the OSC parser.
    // PORT NOTE (N263): borrowed from the parser's buffer or capture, as
    // the reference's slice is ("only valid until the next parser call" is
    // the borrow).
    pub content: &'p [u8],

    /// True if the sequence was longer than `Parser.unknown_max_bytes`,
    /// or memory ran out while reading it. In that case `content`
    /// holds only the beginning of the sequence.
    pub truncated: bool,

    /// How the program ended the sequence. If you send a reply, end it
    /// the same way.
    pub terminator: Terminator,
}

// PORT NOTE: `conemu_change_tab_title: union(enum) { reset, value: [:0]const u8 }`
// — the anonymous tagged union is hoisted to a named type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConemuChangeTabTitle<'p> {
    Reset,
    /// PORT NOTE (fidelity): reference `value: [:0]const u8` — raw bytes.
    Value(ZStr<'p>),
}

impl Command<'_> {
    // pub const SemanticPrompt = parsers.semantic_prompt.Command;
    // pub const KittyClipboardProtocol = parsers.kitty_clipboard_protocol.OSC;

    // PORT NOTE: `pub const Key = LibEnum(lib.target, &.{...})` — the union's
    // tag type, built from `lib.target` (a comptime build value). For the
    // ported crate the `Command` Rust enum above *is* the tagged union (its
    // variants carry the same ordered keys as the `Key` LibEnum, in the
    // identical order: invalid, change_window_title, change_window_icon,
    // semantic_prompt, clipboard_contents, report_pwd, mouse_shape,
    // color_operation, kitty_color_protocol, show_desktop_notification,
    // hyperlink_start, hyperlink_end, conemu_sleep, conemu_show_message_box,
    // conemu_change_tab_title, conemu_progress_report, conemu_wait_input,
    // conemu_guimacro, conemu_run_process, conemu_output_environment_variable,
    // conemu_xterm_emulation, conemu_comment, kitty_text_sizing,
    // kitty_clipboard_protocol, kitty_dnd_protocol, context_signal,
    // kitty_desktop_notification, unknown).
    // NOTE: Order matters, see LibEnum documentation.

    // comptime {
    //     assert(@sizeOf(Command) == switch (@sizeOf(usize)) {
    //         4 => 44,
    //         8 => 64,
    //         else => unreachable,
    //     });
    // }
    // PORT NOTE: the comptime `@sizeOf` assertion pins the Zig union's layout;
    // Rust's enum layout is not byte-compatible with the Zig union (different
    // niche/discriminant rules), so the size invariant is intentionally not
    // asserted here. Carried as a comment per R2.

    // PORT NOTE: `command.*` coerces to its tag (`union(Key)`), giving the
    // `Command.Key` value for the active variant. `key` reproduces that
    // coercion: the `CommandKey` discriminants are the variant order (identical
    // to the LibEnum key order above), so the mapping is the index-for-index
    // projection.
    pub fn key(&self) -> CommandKey {
        match self {
            Command::Invalid => CommandKey::Invalid,
            Command::ChangeWindowTitle(_) => CommandKey::ChangeWindowTitle,
            Command::ChangeWindowIcon(_) => CommandKey::ChangeWindowIcon,
            Command::SemanticPrompt(_) => CommandKey::SemanticPrompt,
            Command::ClipboardContents { .. } => CommandKey::ClipboardContents,
            Command::ReportPwd { .. } => CommandKey::ReportPwd,
            Command::MouseShape { .. } => CommandKey::MouseShape,
            Command::ColorOperation { .. } => CommandKey::ColorOperation,
            Command::KittyColorProtocol(_) => CommandKey::KittyColorProtocol,
            Command::ShowDesktopNotification { .. } => CommandKey::ShowDesktopNotification,
            Command::HyperlinkStart { .. } => CommandKey::HyperlinkStart,
            Command::HyperlinkEnd => CommandKey::HyperlinkEnd,
            Command::ConemuSleep { .. } => CommandKey::ConemuSleep,
            Command::ConemuShowMessageBox(_) => CommandKey::ConemuShowMessageBox,
            Command::ConemuChangeTabTitle(_) => CommandKey::ConemuChangeTabTitle,
            Command::ConemuProgressReport(_) => CommandKey::ConemuProgressReport,
            Command::ConemuWaitInput => CommandKey::ConemuWaitInput,
            Command::ConemuGuimacro(_) => CommandKey::ConemuGuimacro,
            Command::ConemuRunProcess(_) => CommandKey::ConemuRunProcess,
            Command::ConemuOutputEnvironmentVariable(_) => {
                CommandKey::ConemuOutputEnvironmentVariable
            }
            Command::ConemuXtermEmulation { .. } => CommandKey::ConemuXtermEmulation,
            Command::ConemuComment(_) => CommandKey::ConemuComment,
            Command::KittyTextSizing(_) => CommandKey::KittyTextSizing,
            Command::KittyClipboardProtocol(_) => CommandKey::KittyClipboardProtocol,
            Command::KittyDndProtocol(_) => CommandKey::KittyDndProtocol,
            Command::ContextSignal(_) => CommandKey::ContextSignal,
            Command::KittyDesktopNotification(_) => CommandKey::KittyDesktopNotification,
            Command::Unknown(_) => CommandKey::Unknown,
            Command::ProgramStatus(_) => CommandKey::ProgramStatus,
        }
    }
}

/// C: RusttyOscCommandType (R2) — checked by the A5 enum-discriminant arm.
/// PORT NOTE: `Command.Key = LibEnum(lib.target, &.{...})` — the union's tag
/// type. For the `.c` target this is a fieldless `enum(c_int)` with the keys in
/// declaration order. Modeled here as a `#[repr(i32)]` Rust enum (`Command` is
/// `union(Key)`, so the tag is exactly the variant order). NOTE: Order matters,
/// see LibEnum documentation — the discriminants MUST NOT be reordered to
/// preserve C ABI compatibility.
#[repr(i32)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandKey {
    Invalid,
    ChangeWindowTitle,
    ChangeWindowIcon,
    SemanticPrompt,
    ClipboardContents,
    ReportPwd,
    MouseShape,
    ColorOperation,
    KittyColorProtocol,
    ShowDesktopNotification,
    HyperlinkStart,
    HyperlinkEnd,
    ConemuSleep,
    ConemuShowMessageBox,
    ConemuChangeTabTitle,
    ConemuProgressReport,
    ConemuWaitInput,
    ConemuGuimacro,
    ConemuRunProcess,
    ConemuOutputEnvironmentVariable,
    ConemuXtermEmulation,
    ConemuComment,
    KittyTextSizing,
    KittyClipboardProtocol,
    KittyDndProtocol,
    ContextSignal,
    KittyDesktopNotification,
    Unknown,
    ProgramStatus,
}

/// PORT NOTE: `parsers.semantic_prompt.Command` re-exported as
/// `Command.SemanticPrompt`.
pub type SemanticPrompt<'p> = parsers::semantic_prompt::Command<'p>;

/// PORT NOTE: `parsers.kitty_clipboard_protocol.OSC` re-exported as
/// `Command.KittyClipboardProtocol`.
pub type KittyClipboardProtocol<'p> = parsers::kitty_clipboard_protocol::OSC<'p>;

/// PORT NOTE: `parsers.kitty_dnd_protocol.OSC` re-exported as
/// `Command.KittyDndProtocol`.
pub type KittyDndProtocol<'p> = parsers::kitty_dnd_protocol::OSC<'p>;

/// PORT NOTE: `parsers.kitty_desktop_notification.OSC` re-exported as
/// `Command.KittyDesktopNotification` (`9a8e7ae18`).
pub type KittyDesktopNotification<'p> = parsers::kitty_desktop_notification::OSC<'p>;

/// PORT NOTE: `parsers.program_status.Command` re-exported as
/// `Command.ProgramStatus` (`bae2c3cdb`).
pub type ProgramStatus<'p> = parsers::program_status::Command<'p>;

/// PORT NOTE: `Command.ProgressReport` — the ConEmu progress report payload.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressReport {
    pub state: ProgressReportState,
    pub progress: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressReportState {
    Remove,
    Set,
    Error,
    Indeterminate,
    Pause,
}

impl ProgressReportState {
    // PORT NOTE: `pub const State = enum(c_int) {...}` — backed by `c_int`.
    // `@intFromEnum` mirrored as the declaration order (0-based).
    pub fn int_from_enum(self) -> i32 {
        match self {
            ProgressReportState::Remove => 0,
            ProgressReportState::Set => 1,
            ProgressReportState::Error => 2,
            ProgressReportState::Indeterminate => 3,
            ProgressReportState::Pause => 4,
        }
    }

    // test "ghostty.h Command.ProgressReport.State" {
    //     if (comptime build_options.artifact == .lib) return error.SkipZigTest;
    //     try lib.checkGhosttyHEnum(State, "GHOSTTY_PROGRESS_STATE_");
    // }
    // PORT NOTE (vt-6): ported below in `tests::ghostty_h_command_progress_report_state`,
    // mirroring the established `checkGhosttyHEnum` pattern (see mouse.rs Shape).
}

#[cfg(test)]
mod tests {
    use super::ProgressReportState;
    use crate::options as build_options;
    use rustty_lib::c_header::header_decls;
    use rustty_lib::henum_type;
    use rustty_lib::r#enum::{check_ghostty_henum, EnumType};

    /// The checked-in public C header, embedded at compile time.
    ///
    /// PORT NOTE: the reference `lib.checkGhosttyHEnum` does
    /// `const c = @import("ghostty.h")` — it reflects over the REAL generated
    /// header. Rust has no `@import` for a C header, so this test parses the
    /// named enum block out of the checked-in header's text. The C declarations
    /// come from the header, never from the descriptor under test (S45-F1).
    const HEADER_SRC: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../include/rustty-embed.h"
    ));

    /// The header's enum tag for `Command.ProgressReport.State`.
    const HEADER_ENUM: &str = "rustty_action_progress_report_state_e";

    fn run(src: &str) -> Result<(), String> {
        let t = state_enum_descriptor();
        let decls = header_decls(src, HEADER_ENUM);
        check_ghostty_henum(&t, "RUSTTY_PROGRESS_STATE_", |name| {
            decls.get(name).copied()
        })
    }

    /// Self-check that the probe below can actually FAIL — i.e. that it reads
    /// the header and not the descriptor.
    ///
    /// PORT NOTE: no reference analogue; the oracle gets this for free because
    /// `@import("ghostty.h")` is the real header.
    #[test]
    fn ghostty_h_command_progress_report_state_probe_can_fail() {
        run(HEADER_SRC).unwrap();

        let dropped = HEADER_SRC.replace("  RUSTTY_PROGRESS_STATE_PAUSE,\n", "");
        assert_ne!(dropped, HEADER_SRC);
        assert!(run(&dropped).is_err(), "dropped enumerator must fail");

        let renamed = HEADER_SRC.replace(
            "RUSTTY_PROGRESS_STATE_PAUSE",
            "RUSTTY_PROGRESS_STATE_PAUSED",
        );
        assert_ne!(renamed, HEADER_SRC);
        assert!(run(&renamed).is_err(), "renamed enumerator must fail");

        let revalued = HEADER_SRC.replace(
            "  RUSTTY_PROGRESS_STATE_REMOVE,\n",
            "  RUSTTY_PROGRESS_STATE_REMOVE = 7,\n",
        );
        assert_ne!(revalued, HEADER_SRC);
        assert!(run(&revalued).is_err(), "revalued enumerator must fail");
    }

    /// PORT NOTE: models the reference `enum(c_int)` `ProgressReport.State` as a
    /// [`EnumType`] (C target) so the `checkGhosttyHEnum` test can reflect over
    /// its keys and backing int. The keys are the snake_case forms of the
    /// variants, in declaration order, matching
    /// `@typeInfo(State).@"enum".fields`: remove, set, error, indeterminate,
    /// pause.
    ///
    /// PORT NOTE (N36, S156-F1): the KEY LIST was hand-typed too, so a variant
    /// ADDED to `State` with no `RUSTTY_PROGRESS_STATE_*` enumerator left this
    /// test green, where the reference's
    /// `inline for (@typeInfo(T).@"enum".fields)` fails. It goes through
    /// `henum_type!`, whose exhaustive `match` makes that a compile error.
    ///
    /// PORT NOTE (N29): the VALUES come from
    /// `ProgressReportState::int_from_enum()` — the port's model of
    /// `@intFromEnum` for this type, and the same function `cval()` ships
    /// across the C ABI. This used to call `enum_type(Target::C, ..)`, the `lib.Enum`
    /// FACTORY model, which assigns each key its INDEX — so the check compared
    /// the header to a hand-typed list's ORDER and could not observe `State`.
    fn state_enum_descriptor() -> EnumType {
        henum_type!(
            ProgressReportState,
            |v: ProgressReportState| v.int_from_enum(),
            [
                ("remove", Remove),
                ("set", Set),
                ("error", Error),
                ("indeterminate", Indeterminate),
                ("pause", Pause),
            ]
        )
    }

    // test "ghostty.h Command.ProgressReport.State" {
    //     if (comptime build_options.artifact == .lib) return error.SkipZigTest;
    //     try lib.checkGhosttyHEnum(State, "GHOSTTY_PROGRESS_STATE_");
    // }
    #[test]
    fn ghostty_h_command_progress_report_state() {
        // PORT NOTE: `if (comptime build_options.artifact == .lib) return
        // error.SkipZigTest;`. rustty-vt resolves `artifact` to the application
        // artifact (the reference `.ghostty`), so this skip path is not taken
        // and the check runs — preserved verbatim.
        if build_options::ARTIFACT == crate::build_options::Artifact::Lib {
            return;
        }

        // PORT NOTE: `try lib.checkGhosttyHEnum(State, "GHOSTTY_PROGRESS_STATE_");`,
        // retargeted to `RUSTTY_PROGRESS_STATE_` per R2. The declarations come
        // from `rustty-embed.h`'s `rustty_action_progress_report_state_e`
        // block, exactly as the reference's come from the generated header.
        run(HEADER_SRC).unwrap();
    }
}

/// sync with ghostty_action_progress_report_s
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProgressReportC {
    pub state: i32,
    pub progress: i8,
}

impl ProgressReport {
    pub fn cval(self) -> ProgressReportC {
        ProgressReportC {
            state: self.state.int_from_enum(),
            progress: match self.progress {
                Some(progress) => progress.clamp(0, 100) as i8,
                None => -1,
            },
        }
    }
}

/// The terminator used to end an OSC command. For OSC commands that demand
/// a response, we try to match the terminator used in the request since that
/// is most likely to be accepted by the calling program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Terminator {
    /// The preferred string terminator is ESC followed by \
    St,

    /// Some applications and terminals use BELL (0x07) as the string terminator.
    Bel,
}

impl Terminator {
    // pub const C = LibEnum(.c, &.{ "st", "bel" });
    pub fn c() -> EnumType {
        rustty_lib::Enum(Target::C, &[Some("st"), Some("bel")])
    }

    /// Initialize the terminator based on the last byte seen. If the
    /// last byte is a BEL then we use BEL, otherwise we just assume ST.
    pub fn init(ch: Option<u8>) -> Terminator {
        match ch {
            None => Terminator::St,
            Some(0x07) => Terminator::Bel,
            Some(_) => Terminator::St,
        }
    }

    /// The terminator as a string. This is static memory so it doesn't
    /// need to be freed.
    pub fn string(self) -> &'static [u8] {
        match self {
            Terminator::St => b"\x1b\\",
            Terminator::Bel => b"\x07",
        }
    }

    // PORT NOTE: `cval` returns the `C` LibEnum value (`@intFromEnum` of "st"/
    // "bel"). The descriptor's index for the matching key is the faithful value.
    pub fn cval(self) -> i32 {
        match self {
            Terminator::St => Terminator::c().value("st") as i32,
            Terminator::Bel => Terminator::c().value("bel") as i32,
        }
    }
}

impl std::fmt::Display for Terminator {
    fn fmt(&self, writer: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // try writer.writeAll(self.string());
        writer.write_str(std::str::from_utf8(self.string()).unwrap())
    }
}

/// Maximum size of a "normal" OSC.
pub const MAX_BUF: usize = 2048;

/// Maximum size of an OSC that requires dynamically allocated storage.
/// OSC input is untrusted, so these captures must have a finite bound.
pub const MAX_ALLOCATING_BUF: usize = 8 * 1024 * 1024;

pub struct Parser {
    /// Optional allocator used to accept data longer than MAX_BUF.
    /// This only applies to some commands (e.g. OSC 52) that can
    /// reasonably exceed MAX_BUF.
    // PORT NOTE (parity tail S7-7): `?Allocator`, the runtime allocator handle
    // (`crate::alloc::Allocator`). It gates the OSCs that need an allocator
    // (`ensureAllocator`), backs the allocating capture, and grows the two
    // command lists (`color_operation.requests`, `kitty_color_protocol.list`),
    // exactly where the reference uses `parser.alloc`. It used to be a
    // borrowed `ZigAllocatorRef<'a>` beside a separate `capture_alloc` handle
    // (S174-F3 part 1); the split is retired, and with it the parser's
    // lifetime parameter.
    pub alloc: Option<crate::alloc::Allocator>,

    /// Maximum number of bytes retained by an allocating capture.
    /// This is configurable primarily so callers and tests can choose a
    /// smaller policy than the default.
    pub max_allocating_bytes: usize,

    /// The most bytes to keep from each OSC sequence whose number this
    /// parser does not implement.
    ///
    /// Zero, the default, discards these sequences. The parser stops
    /// reading one as soon as it sees a number it doesn't know, and `end`
    /// returns null. Any other value makes `end` return `Command.unknown`
    /// with the sequence's bytes, so you can implement the sequence
    /// yourself:
    ///
    /// ```zig
    /// var p: Parser = .init(alloc);
    /// defer p.deinit();
    /// p.unknown_max_bytes = 1024;
    ///
    /// p.nextSlice("7400;status=busy");
    /// const cmd = p.end(0x07).?; // .unknown, content "7400;status=busy"
    /// ```
    ///
    /// A sequence longer than the limit is still returned, with the
    /// first bytes up to the limit and `truncated` set to true.
    ///
    /// Limits up to `MAX_BUF` use a buffer inside the parser and never
    /// allocate. Larger limits allocate for each unknown sequence. Without
    /// an allocator, a larger limit behaves like `MAX_BUF`. If memory runs
    /// out partway through a sequence, it is returned truncated.
    ///
    /// Sequences with a number the parser does implement are not affected,
    /// even when their contents are malformed.
    pub unknown_max_bytes: usize,

    /// Current state of the parser.
    pub state: State,

    /// Buffer for temporary storage of OSC data
    pub buffer: [u8; MAX_BUF],

    /// Capture state. If this is set then we're actively capturing the
    /// bytes coming into the parser.
    pub capture: Option<Capture>,

    // /// The command that is the result of parsing.
    // command: Command,
    // PORT NOTE (parity tail S7-7, N263): the command itself is not a field.
    // The reference's `command` points into this parser's own `buffer` or
    // `capture`; a Rust field holding those borrows would make the parser
    // self-referential (and a moved parser would leave them dangling). `end`
    // builds the command and returns it by value, borrowing the parser (see
    // `Command`). What the reference's `command` OWNS stays here, as it does
    // there: the two lists grown through `alloc` (`color_operation.requests`,
    // `kitty_color_protocol.list`), which the command borrows and `reset`
    // frees. `color_requests` and `kitty_color` hold the list of the last
    // color or kitty color command until then.
    pub color_requests: Option<color::List>,
    pub kitty_color: Option<kitty_color::OSC>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum State {
    Start,
    Invalid,

    /// Collecting the bytes of a sequence whose number is not
    /// implemented, to return as `Command.unknown`. Only entered when
    /// `unknown_max_bytes` is nonzero.
    Unknown,

    /// Like `unknown`, but the byte limit has been reached. Later bytes
    /// are dropped and the command is returned with `truncated` set.
    UnknownTruncated,

    // OSC command prefixes. Not all of these are valid OSCs, but may be
    // needed to "bridge" to a valid OSC (e.g. to support OSC 777 we need to
    // have a state "77" even though there is no OSC 77).
    //
    // The name of each prefix state must be exactly the bytes consumed
    // to reach it. `beginUnknown` relies on this to recover the prefix.
    // PORT NOTE: the Rust names carry an `S` (an identifier cannot start
    // with a digit), so the consumed bytes are `State::prefix`, a `const`
    // table kept next to this list, not the variant name.
    S0,
    S1,
    S2,
    S3,
    S4,
    S5,
    S6,
    S7,
    S8,
    S9,
    S30,
    S300,
    S3008,
    S10,
    S11,
    S12,
    S13,
    S14,
    S15,
    S16,
    S17,
    S18,
    S19,
    S21,
    S22,
    S52,
    S55,
    S66,
    S72,
    S75,
    S750,
    S7501,
    S77,
    S99,
    S104,
    S105,
    S110,
    S111,
    S112,
    S113,
    S114,
    S115,
    S116,
    S117,
    S118,
    S119,
    S133,
    S552,
    S777,
    S1337,
    S5522,
}

impl State {
    /// `@tagName(state)`, as `{t}` prints it: `start`, `invalid`, or the
    /// OSC number (`@"52"` is `52`).
    // PORT NOTE: the numeric reference tags are spelled `S<n>` here (a Rust
    // identifier cannot start with a digit); the tag is the variant name with
    // that `S` removed, or the lower-cased name for the two word tags.
    pub fn tag_name(self) -> String {
        // (`unknown_truncated` is the one multi-word tag.)
        if self == State::UnknownTruncated {
            return "unknown_truncated".to_string();
        }
        let name = format!("{self:?}");
        match name.strip_prefix('S') {
            Some(digits) if digits.bytes().all(|b| b.is_ascii_digit()) => digits.to_string(),
            _ => name.to_ascii_lowercase(),
        }
    }

    /// The bytes consumed to reach this prefix state: `@tagName(state)` for
    /// a prefix state (`.@"13"` after reading "13"), and "" for `start`.
    ///
    /// PORT NOTE (P8 DECISIONS P4): the reference recovers the prefix with
    /// `inline else => |s| @tagName(s)`, a comptime table of static strings.
    /// This is that table as a `const` match: no allocation, no formatting,
    /// unlike `tag_name` (which allocates and must never be used on the
    /// unknown-capture path). `invalid`, `unknown` and `unknown_truncated`
    /// are not prefix states and are unreachable here, as in the reference.
    #[inline]
    pub const fn prefix(self) -> &'static [u8] {
        match self {
            State::Start => b"",
            State::Invalid | State::Unknown | State::UnknownTruncated => unreachable!(),
            State::S0 => b"0",
            State::S1 => b"1",
            State::S2 => b"2",
            State::S3 => b"3",
            State::S4 => b"4",
            State::S5 => b"5",
            State::S6 => b"6",
            State::S7 => b"7",
            State::S8 => b"8",
            State::S9 => b"9",
            State::S30 => b"30",
            State::S300 => b"300",
            State::S3008 => b"3008",
            State::S10 => b"10",
            State::S11 => b"11",
            State::S12 => b"12",
            State::S13 => b"13",
            State::S14 => b"14",
            State::S15 => b"15",
            State::S16 => b"16",
            State::S17 => b"17",
            State::S18 => b"18",
            State::S19 => b"19",
            State::S21 => b"21",
            State::S22 => b"22",
            State::S52 => b"52",
            State::S55 => b"55",
            State::S66 => b"66",
            State::S72 => b"72",
            State::S75 => b"75",
            State::S750 => b"750",
            State::S7501 => b"7501",
            State::S77 => b"77",
            State::S99 => b"99",
            State::S104 => b"104",
            State::S105 => b"105",
            State::S110 => b"110",
            State::S111 => b"111",
            State::S112 => b"112",
            State::S113 => b"113",
            State::S114 => b"114",
            State::S115 => b"115",
            State::S116 => b"116",
            State::S117 => b"117",
            State::S118 => b"118",
            State::S119 => b"119",
            State::S133 => b"133",
            State::S552 => b"552",
            State::S777 => b"777",
            State::S1337 => b"1337",
            State::S5522 => b"5522",
        }
    }
}

/// PORT NOTE: the capture backing. The reference uses `std.Io.Writer` over
/// either a fixed buffer or an allocating buffer. The filled parser path
/// threads the fixed/allocating distinction through `CaptureMode`.
// PERF(port): restore Zig fixed-writer-over-self.buffer capture
// (osc.zig:462-512, `Capture.fixed(&self.capture, &self.buffer)`); the
// Vec-per-record downgrade is removed. The fixed backing stores only a running
// length into the Parser's inline `buffer: [u8; MAX_BUF]` (it cannot hold
// `&mut Parser.buffer` — self-referential), so the accessors below take the
// Parser's buffer as an argument and the per-byte write happens in
// `Parser::next` — zero allocation, exactly like `std.Io.Writer.fixed`.
pub struct Capture {
    pub backing: CaptureBacking,
    /// `max_bytes: usize` — the byte bound this capture may not grow beyond
    /// (for the fixed backing this is the buffer length: `MAX_BUF`, or the
    /// shorter `@min(unknown_max_bytes, MAX_BUF)` slice of an unknown capture;
    /// for the allocating backing it is the parser's `max_allocating_bytes`
    /// policy, or `unknown_max_bytes`). `captureFailed` pins it to the
    /// retained length once an unknown capture is truncated.
    pub max_bytes: usize,
}

pub enum CaptureBacking {
    /// The count of bytes captured so far into `Parser.buffer` — the Zig fixed
    /// writer's `end` index over `&self.buffer` (osc.zig:462-467).
    Fixed(usize),
    /// `std.Io.Writer.Allocating`, over `Parser.alloc`.
    Allocating(allocator_api2::vec::Vec<u8, CaptureAllocator>),
}

/// The allocator of an allocating capture (`Parser.alloc`).
///
/// CORRECTION (parity S174-F3): the allocating capture used to be a plain
/// global-heap `Vec<u8>` that aborted on allocation failure. It now
/// allocates, grows and frees through this allocator, and a failure takes
/// the reference's paths (the fixed fallback for `initCapacity`,
/// `error.WriteFailed` for a `realloc`) instead of aborting.
///
/// PORT NOTE (parity tail S7-1): this was a two-arm enum (`Global` or a
/// `&'static dyn ZigAllocator`) with its own allocator_api2 impl. It is now the
/// crate's runtime allocator handle, `crate::alloc::VtAllocator`, whose
/// `Global` arm is the same Rust global heap and whose C arm calls the
/// caller's `RusttyAllocator` vtable directly (what the `Zig` arm reached
/// through `CAllocatorAsZig`). The handle's `grow` is Zig's `realloc`
/// (`remap`, then alloc + copy + free), which is what the reference's
/// `w.allocator.realloc(..)` does here; the enum had no `grow`, so it always
/// took allocator_api2's default alloc + copy + free. The inlined arms keep the
/// T-A3 perf shape (a null test plus the `Global` call; the C arm is out of
/// line).
pub type CaptureAllocator = crate::alloc::VtAllocator;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureMode {
    Fixed,
    Allocating,
}

impl Capture {
    pub fn deinit(&mut self) {
        match &mut self.backing {
            CaptureBacking::Fixed(_) => {}
            CaptureBacking::Allocating(w) => {
                w.clear();
            }
        }
    }

    /// Return the captured trailing data. This is the data from the
    /// point that trailing data capture was requested.
    // PERF(port): `pub inline fn trailing(self: *Capture) []u8` returns
    // `self.writer.buffered()` — for the fixed writer that is the front of the
    // Parser's own `buffer` (osc.zig:491-493). The buffer is threaded in from
    // the Parser because the fixed backing stores only the running length.
    #[inline]
    pub fn trailing<'b>(&'b self, buffer: &'b [u8; MAX_BUF]) -> &'b [u8] {
        match &self.backing {
            CaptureBacking::Fixed(len) => &buffer[..*len],
            CaptureBacking::Allocating(w) => w,
        }
    }

    /// Return a mutable view of the captured trailing data. Used by parsers
    /// that need to overwrite a separator byte with a sentinel in place
    /// (e.g. OSC 777 rxvt).
    // PERF(port): fixed-backing variant of `trailing()` — the mutable view is
    // `&mut Parser.buffer[..len]`, the same in-place bytes the Zig `[]u8`
    // slice aliases (osc.zig:491-493).
    #[inline]
    pub fn trailing_mut<'b>(&'b mut self, buffer: &'b mut [u8; MAX_BUF]) -> &'b mut [u8] {
        match &mut self.backing {
            CaptureBacking::Fixed(len) => &mut buffer[..*len],
            CaptureBacking::Allocating(w) => w,
        }
    }

    /// PORT NOTE: `cap.writer.writeByte(c)` — mirror the overflow semantics of
    /// `Parser.next`: the fixed backing fails (`error.WriteFailed`) once the
    /// `MAX_BUF` capacity is exhausted, while the allocating backing always
    /// succeeds (growing on demand).
    /// Returns `true` on success and `false` on the `error.WriteFailed`
    /// overflow path (the natural mirror of the Zig `writeByte(c) catch`).
    // PERF(port): the Zig fixed writer's `writeByte` (osc.zig:543-549 via
    // std.Io.Writer.fixed) is a bounds check + store that inlines under
    // ReleaseFast; #[inline] restores cross-crate inlinability without LTO.
    // The fixed arm stores directly into the Parser's inline buffer (threaded
    // in), restoring the zero-alloc single-bounds-check idiom.
    /// Append one byte without permitting the backing allocation to grow
    /// beyond max_bytes. Allocating.Writer normally grows super-linearly,
    /// so grow it explicitly to keep the allocation itself bounded too.
    #[inline]
    pub fn write_byte(&mut self, buffer: &mut [u8; MAX_BUF], c: u8) -> bool {
        // if (self.writer.buffered().len >= self.max_bytes) return error.WriteFailed;
        match &mut self.backing {
            CaptureBacking::Fixed(len) => {
                if *len >= self.max_bytes || *len >= MAX_BUF {
                    false
                } else {
                    buffer[*len] = c;
                    *len += 1;
                    true
                }
            }
            CaptureBacking::Allocating(w) => {
                if w.len() >= self.max_bytes {
                    return false;
                }
                if w.len() >= w.capacity() {
                    // const new_capacity = @min(
                    //     self.max_bytes,
                    //     @max(w.writer.buffer.len *| 2, 1),
                    // );
                    // R7: `*|` is a saturating multiply; `@max(.., 1)` keeps a
                    // zero-capacity buffer growable.
                    let new_capacity = self.max_bytes.min(w.capacity().saturating_mul(2).max(1));
                    // w.writer.buffer = w.allocator.realloc(w.writer.buffer, new_capacity)
                    //     catch return error.WriteFailed;
                    // PORT NOTE: `try_reserve_exact` grows the Vec's backing to
                    // exactly `new_capacity` (len + additional), mirroring the
                    // bounded realloc (S174-F3).
                    if w.try_reserve_exact(new_capacity - w.len()).is_err() {
                        return false;
                    }
                }
                w.push(c);
                true
            }
        }
    }

    /// Append a slice without permitting the backing allocation to
    /// grow beyond max_bytes. This matches the byte-at-a-time
    /// semantics of writeByte: bytes are retained up to exactly
    /// max_bytes and the first byte that doesn't fit fails the
    /// write.
    // pub fn writeSlice(
    //     self: *Capture,
    //     bytes: []const u8,
    // ) error{WriteFailed}!void { ... }
    //
    /// Returns `true` on success and `false` on the `error.WriteFailed`
    /// truncation path (the natural mirror of the Zig error union — same
    /// convention as `write_byte`). Note the truncate-at-limit bound is
    /// enforced with release-checked arithmetic (R7), never a debug assert:
    /// OSC input is untrusted.
    // PERF(port): like `write_byte`, the fixed backing stores directly into
    // the Parser's inline buffer (threaded in) — zero allocation, exactly
    // like `std.Io.Writer.fixed`'s `writeAll`.
    pub fn write_slice(&mut self, buffer: &mut [u8; MAX_BUF], bytes: &[u8]) -> bool {
        // const avail = self.max_bytes - self.writer.buffered().len;
        let buffered_len = match &self.backing {
            CaptureBacking::Fixed(len) => *len,
            CaptureBacking::Allocating(w) => w.len(),
        };
        let avail = self.max_bytes - buffered_len;
        // const n = @min(bytes.len, avail);
        let n = bytes.len().min(avail);

        match &mut self.backing {
            // .fixed => {},
            // (the fixed writer's `writeAll` below is the buffer copy; its
            // capacity IS `max_bytes`, so `n` always fits)
            CaptureBacking::Fixed(len) => {
                // try self.writer.writeAll(bytes[0..n]);
                buffer[*len..*len + n].copy_from_slice(&bytes[..n]);
                *len += n;
            }
            CaptureBacking::Allocating(w) => {
                // const needed = w.writer.end + n;
                let needed = w.len() + n;
                // if (needed > w.writer.buffer.len) { ... }
                if needed > w.capacity() {
                    // const new_capacity = @min(
                    //     self.max_bytes,
                    //     @max(w.writer.buffer.len *| 2, needed),
                    // );
                    // R7: `*|` is a saturating multiply.
                    let new_capacity = self
                        .max_bytes
                        .min(w.capacity().saturating_mul(2).max(needed));
                    // w.writer.buffer = w.allocator.realloc(
                    //     w.writer.buffer,
                    //     new_capacity,
                    // ) catch return error.WriteFailed;
                    // PORT NOTE: `try_reserve_exact` mirrors the bounded
                    // realloc (S174-F3).
                    if w.try_reserve_exact(new_capacity - w.len()).is_err() {
                        return false;
                    }
                }
                // try self.writer.writeAll(bytes[0..n]);
                // PERF(port): allocator_api2 0.2's `Vec::extend_from_slice` is
                // `extend(iter().cloned())`, a byte-at-a-time push loop with no
                // memcpy specialization (review-T-A2 blocking item 1: OSC 52
                // stream 16.4 -> 40.0 ms). The reference's `writeAll` is a
                // memcpy into the already-sized buffer; do the same.
                // R7: the branch above guarantees `needed <= capacity` (either
                // it already held, or `try_reserve_exact` succeeded with
                // `new_capacity >= needed`: `max_bytes >= needed` because
                // `n <= avail`). Release-checked, not a debug assert.
                assert!(needed <= w.capacity());
                let len = w.len();
                // SAFETY: `len + n == needed <= w.capacity()` (asserted
                // above), so the destination `[len, len + n)` lies inside
                // the Vec's allocation. `bytes` is a `&[u8]` borrow distinct
                // from `w` (we hold `&mut` to `w` through `self.backing`, and
                // `bytes` is a separate argument), so the two ranges cannot
                // overlap. `u8` is `Copy` with no invalid bit patterns, so
                // the `n` copied bytes are initialized before `set_len`
                // exposes them.
                unsafe {
                    core::ptr::copy_nonoverlapping(bytes.as_ptr(), w.as_mut_ptr().add(len), n);
                    w.set_len(needed);
                }
            }
        }

        // if (n < bytes.len) return error.WriteFailed;
        if n < bytes.len() {
            return false;
        }
        true
    }
}

impl Parser {
    pub fn init(alloc: Option<crate::alloc::Allocator>) -> Parser {
        // Keeping all our undefined values together so we can
        // visually easily duplicate them in the Valgrind check below.
        // .buffer = undefined,
        //
        // if (std.valgrind.runningOnValgrind() > 0) {
        //     // Initialize our undefined fields so Valgrind can catch it.
        //     // https://github.com/ziglang/zig/issues/19148
        //     result.buffer = undefined;
        // }
        //
        // PORT NOTE: the Valgrind-guarded `buffer = undefined` initialization is
        // a no-op observability hook in Zig; Rust requires the buffer be
        // initialized, so we zero it.
        Parser {
            alloc,
            max_allocating_bytes: MAX_ALLOCATING_BUF,
            unknown_max_bytes: 0,
            state: State::Start,
            capture: None,
            color_requests: None,
            kitty_color: None,
            buffer: [0u8; MAX_BUF],
        }
    }

    /// Set `unknown_max_bytes`: the most bytes to keep from each OSC
    /// sequence whose number this parser does not implement. Zero, the
    /// default, discards them. See [`Parser::unknown_max_bytes`].
    // PORT NOTE: the reference assigns the field (`p.unknown_max_bytes =
    // 64`); the field is `pub` here too, and this setter is the named entry
    // point the stream and C API use.
    #[inline]
    pub fn set_unknown_max_bytes(&mut self, max_bytes: usize) {
        self.unknown_max_bytes = max_bytes;
    }

    /// This must be called to clean up any allocated memory.
    pub fn deinit(&mut self) {
        self.reset();
    }

    /// Reset the parser state.
    pub fn reset(&mut self) {
        // If we're capturing, then stop it.
        if let Some(cap) = self.capture.as_mut() {
            cap.deinit();
        }

        // Handle any cleanup that individual OSCs require.
        // switch (self.command) {
        //     .kitty_color_protocol => |*v| kitty_color_protocol: {
        //         v.deinit(self.alloc orelse break :kitty_color_protocol);
        //     },
        //     .color_operation => |*v| color_operation: {
        //         v.requests.deinit(self.alloc orelse break :color_operation);
        //     },
        //     .change_window_icon, .change_window_title, .clipboard_contents,
        //     .conemu_change_tab_title, .conemu_comment, .conemu_guimacro,
        //     .conemu_output_environment_variable, .conemu_progress_report,
        //     .conemu_run_process, .conemu_show_message_box, .conemu_sleep,
        //     .conemu_wait_input, .conemu_xterm_emulation, .hyperlink_end,
        //     .hyperlink_start, .invalid, .mouse_shape, .report_pwd,
        //     .semantic_prompt, .show_desktop_notification, .kitty_text_sizing,
        //     .kitty_clipboard_protocol, .kitty_dnd_protocol,
        //     .kitty_desktop_notification, .context_signal, .unknown,
        //     => {},
        // }
        // PORT NOTE (parity tail S7-7): the lists the command owned are the
        // parser's `color_requests` and `kitty_color` (see the `command` note
        // on `Parser`); both arms free them here, through the allocator they
        // grew with. The borrow checker has ended the command that borrowed
        // them before `reset` can run. The other arms own nothing.
        self.kitty_color = None;
        self.color_requests = None;

        self.state = State::Start;
        self.capture = None;
        // self.command = .invalid;

        // if (std.valgrind.runningOnValgrind() > 0) {
        //     // Initialize our undefined fields so Valgrind can catch it.
        //     // https://github.com/ziglang/zig/issues/19148
        //     self.buffer = undefined;
        // }
        // PORT NOTE: the Valgrind-guarded `buffer = undefined` re-initialization
        // is a no-op observability hook in Zig (mirrors `init`); Rust keeps the
        // buffer initialized, so there is nothing to reset here.
    }

    /// Make sure that we have an allocator. If we don't, set the state to
    /// invalid so that any additional OSC data is discarded.
    // PERF(port): Zig `inline fn` (osc.zig:441) — restore mandatory-inline
    // idiom.
    #[inline]
    fn ensure_allocator(&mut self) -> bool {
        if self.alloc.is_some() {
            return true;
        }
        // const log = std.log.scoped(.osc);
        // log.warn("An allocator is required to process OSC {t} but none was provided.", .{self.state});
        // CORRECTION (parity S156-F4): a PORT NOTE here said "Rustty has no
        // logging facade wired into this crate (matching the convention used by
        // PageList / DCS et al.), so the scoped logger and its calls are ported
        // as commented lines". The crate logs through the `log` facade.
        log::warn!(
            target: "osc",
            "An allocator is required to process OSC {} but none was provided.",
            self.state.tag_name()
        );
        self.state = State::Invalid;
        false
    }

    /// Begin capturing trailing data. All inputs to next from this point
    /// forward will be captured into the `self.capture.writer` buffer
    /// which may be backed by either a fixed size or allocating buffer
    /// depending on mode.
    ///
    /// Get the trailing data using `capture.trailing()`. Do not access
    /// the writer directly.
    // PERF(port): Zig `inline fn` (osc.zig:503) — restore mandatory-inline
    // idiom.
    #[inline]
    fn capture_trailing(&mut self, mode: CaptureMode) {
        // assert(self.capture == null);
        debug_assert!(self.capture.is_none());
        match mode {
            // .fixed => Capture.fixed(&self.capture, &self.buffer),
            // PERF(port): restore Zig fixed-writer-over-self.buffer capture
            // (osc.zig:509-512); the running length starts at 0 and the bytes
            // land in `self.buffer` inside `next` — no allocation.
            CaptureMode::Fixed => {
                self.capture = Some(Capture {
                    backing: CaptureBacking::Fixed(0),
                    // .max_bytes = buf.len,
                    max_bytes: MAX_BUF,
                });
            }

            CaptureMode::Allocating => {
                // const alloc = self.alloc orelse { ... };
                let Some(alloc) = self.alloc else {
                    // We don't have an allocator - fall back to a fixed buffer and hope
                    // that it's big enough.
                    self.capture_trailing(CaptureMode::Fixed);
                    return;
                };

                // Capture.allocating(&self.capture, alloc, self.max_allocating_bytes) catch {
                //     // The allocator failed for some reason, fall back to a fixed buffer
                //     // and hope that it's big enough.
                //     self.captureTrailing(.fixed);
                //     return;
                // };
                let mut w = allocator_api2::vec::Vec::new_in(alloc);
                if w.try_reserve_exact(MAX_BUF.min(self.max_allocating_bytes))
                    .is_err()
                {
                    self.capture_trailing(CaptureMode::Fixed);
                    return;
                }
                self.capture = Some(Capture {
                    backing: CaptureBacking::Allocating(w),
                    max_bytes: self.max_allocating_bytes,
                });
            }
        }
    }

    /// Called when the sequence's number turns out not to be one this
    /// parser implements, either at byte c or at the end of the sequence
    /// (c is null). Supported OSCs never reach this.
    ///
    /// With `unknown_max_bytes` at zero, this marks the sequence invalid,
    /// exactly as the parser did before unknown sequences were supported.
    /// Otherwise it starts collecting the sequence's bytes. The digits read
    /// so far are not stored anywhere, but the current state's name is
    /// exactly those digits (state `.@"13"` after reading "13"). So the
    /// state name is written first, then c, and the collected bytes hold
    /// the whole sequence from its first byte.
    ///
    /// This is deliberately not inline. It is referenced from every
    /// prefix state in `next`, and inlining it there grows the state
    /// machine enough to measurably slow per-byte parsing of supported
    /// OSCs (about 4% in the osc-parser benchmark).
    #[inline(never)]
    fn unknown_or_invalid(&mut self, c: Option<u8>) {
        let max_bytes = self.unknown_max_bytes;
        if max_bytes == 0 {
            // @branchHint(.likely);
            self.state = State::Invalid;
            return;
        }

        // assert(self.capture == null);
        assert!(self.capture.is_none());
        // const prefix: []const u8 = switch (self.state) {
        //     .start => "",
        //     .invalid, .unknown, .unknown_truncated => unreachable,
        //     inline else => |s| @tagName(s),
        // };
        // PORT NOTE (P8 DECISIONS P4): `State::prefix` is the comptime
        // `@tagName` table as a `const` match (never `tag_name`, which
        // allocates).
        let prefix: &'static [u8] = self.state.prefix();
        self.state = State::Unknown;

        // Limits past the fixed buffer need an allocation. Without an
        // allocator, or if allocating fails, the fixed buffer is used and
        // longer sequences are reported as truncated.
        let alloc = if max_bytes > MAX_BUF {
            self.alloc
        } else {
            None
        };
        let fixed = |max_bytes: usize| Capture {
            backing: CaptureBacking::Fixed(0),
            max_bytes,
        };
        self.capture = Some(match alloc {
            // Capture.allocating(&self.capture, a, max_bytes) catch
            //     Capture.fixed(&self.capture, &self.buffer);
            Some(a) => {
                let mut w = allocator_api2::vec::Vec::new_in(a);
                if w.try_reserve_exact(MAX_BUF.min(max_bytes)).is_ok() {
                    Capture {
                        backing: CaptureBacking::Allocating(w),
                        max_bytes,
                    }
                } else {
                    fixed(MAX_BUF)
                }
            }
            // Capture.fixed(&self.capture, self.buffer[0..@min(max_bytes, MAX_BUF)]);
            None => fixed(max_bytes.min(MAX_BUF)),
        });

        let cap = self.capture.as_mut().unwrap();
        // cap.writeSlice(prefix) catch return self.captureFailed();
        if !cap.write_slice(&mut self.buffer, prefix) {
            return self.capture_failed();
        }
        // if (c) |byte| cap.writeByte(byte) catch return self.captureFailed();
        if let Some(byte) = c {
            if !cap.write_byte(&mut self.buffer, byte) {
                self.capture_failed();
            }
        }
    }

    /// Handle a failed capture write, either from reaching the capture's
    /// limit or from an allocation failure. Supported OSCs become invalid.
    /// Unknown OSCs keep what was captured and are reported as truncated.
    ///
    /// This is inline so the supported-OSC overflow path stays a single
    /// store, as it was before unknown capture existed.
    #[inline(always)]
    fn capture_failed(&mut self) {
        match self.state {
            State::Unknown => {
                // Pin the limit to what was retained so every later write
                // fails at the first bounds check. Without this, a later
                // allocating write could succeed after an allocation
                // failure and leave a gap in the content.
                let cap = self.capture.as_mut().unwrap();
                cap.max_bytes = cap.trailing(&self.buffer).len();
                self.state = State::UnknownTruncated;
            }
            // Already truncated: the write failed at the first bounds
            // check and there is nothing more to do.
            State::UnknownTruncated => {}
            _ => self.state = State::Invalid,
        }
    }

    /// Consume a slice of bytes, advancing the parser state. This is
    /// equivalent to calling `next` for each byte in order, but is much
    /// faster once a data capture is active because the remaining bytes
    /// are appended to the capture in bulk.
    // pub fn nextSlice(self: *Parser, input: []const u8) void {
    pub fn next_slice(&mut self, input: &[u8]) {
        // if (self.state == .invalid) return;
        if self.state == State::Invalid {
            return;
        }

        // Run the state machine byte-at-a-time until a capture begins.
        // The command prefix before a capture starts is only a handful
        // of bytes so this loop is short in practice.
        let mut offset: usize = 0;
        while self.capture.is_none() {
            if offset >= input.len() {
                return;
            }
            self.next(input[offset]);
            offset += 1;
            if self.state == State::Invalid {
                return;
            }
        }

        // const rem = input[offset..];
        let rem = &input[offset..];
        if rem.is_empty() {
            return;
        }
        // self.capture.?.writeSlice(rem) catch |err| switch (err) {
        //     // We have overflowed our buffer or had some other error.
        //     // Discard any further input.
        //     error.WriteFailed => self.captureFailed(),
        // };
        let cap = self.capture.as_mut().unwrap();
        if !cap.write_slice(&mut self.buffer, rem) {
            // We have overflowed our buffer or had some other error.
            // Discard any further input.
            self.capture_failed();
        }
    }

    /// Consume the next character c and advance the parser state.
    // PERF(port): the reference forces this per-byte call inline —
    // `@call(.always_inline, osc.Parser.next, ...)` at Parser.zig:364 and
    // benchmark/OscParser.zig:103 — restore the mandatory-inline idiom.
    #[inline(always)]
    pub fn next(&mut self, c: u8) {
        // If the state becomes invalid for any reason, just discard
        // any further input.
        if self.state == State::Invalid {
            return;
        }

        // If a writer has been initialized, we just accumulate the rest of the
        // OSC sequence in the writer's buffer and skip the state machine.
        if let Some(cap) = self.capture.as_mut() {
            // cap.writer.writeByte(c) catch ... => self.state = .invalid;
            // PERF(port): the fixed arm is the Zig fixed writer's `writeByte`
            // (bounds check + store into `self.buffer`, osc.zig:543-549);
            // no Vec, no grow check.
            //
            // PORT NOTE (reshape): the success store is written as the
            // fall-through arm (`len < max_bytes` first) — the same shape as the
            // Zig writer's `if (end < buffer.len)` fast path — so the per-byte
            // loop takes 2 taken branches instead of 3 (measured: the fat-LTO
            // build's only osc win over release-fast was exactly this block
            // layout; see PERF_LOG.md osc-parser entry).
            //
            // PORT NOTE (P8 DECISIONS P4): the reference's fixed unknown
            // capture is a shorter slice of the buffer
            // (`self.buffer[0..@min(max_bytes, MAX_BUF)]`), so the bound is the
            // capture's `max_bytes`, not `MAX_BUF`. Every supported fixed
            // capture has `max_bytes == MAX_BUF`, so their branch is unchanged;
            // `max_bytes <= MAX_BUF` always holds for the fixed backing.
            match &mut cap.backing {
                CaptureBacking::Fixed(len) => {
                    if *len < cap.max_bytes {
                        self.buffer[*len] = c;
                        *len += 1;
                    } else {
                        // We have overflowed our buffer or had some other error.
                        // Discard any further input.
                        self.capture_failed();
                    }
                }
                CaptureBacking::Allocating(w) => {
                    // cap.writeByte(c) catch |err| switch (err) {
                    //     error.WriteFailed => self.state = .invalid,
                    // };
                    // PORT NOTE (727b8a02f): allocating captures are bounded by
                    // `cap.max_bytes` and grown explicitly so the allocation
                    // itself stays bounded too — this mirrors
                    // `Capture::write_byte`'s allocating arm (the fixed arm
                    // above keeps its measured inline layout).
                    if w.len() >= cap.max_bytes {
                        // We have overflowed our buffer or had some other error.
                        // Discard any further input.
                        self.capture_failed();
                    } else {
                        if w.len() >= w.capacity() {
                            // R7: `*|` saturating multiply, `@max(.., 1)` floor.
                            let new_capacity =
                                cap.max_bytes.min(w.capacity().saturating_mul(2).max(1));
                            // `realloc(..) catch return error.WriteFailed`,
                            // which `next` turns into `captureFailed` (S174-F3).
                            if w.try_reserve_exact(new_capacity - w.len()).is_err() {
                                self.capture_failed();
                                return;
                            }
                        }
                        w.push(c);
                    }
                }
            }
            return;
        }

        match self.state {
            // handled above, so should never be here
            State::Invalid => unreachable!(),

            // unknown states always have an active capture
            State::Unknown | State::UnknownTruncated => unreachable!(),

            State::Start => match c {
                b'0' => self.state = State::S0,
                b'1' => self.state = State::S1,
                b'2' => self.state = State::S2,
                b'3' => self.state = State::S3,
                b'4' => self.state = State::S4,
                b'5' => self.state = State::S5,
                b'6' => self.state = State::S6,
                b'7' => self.state = State::S7,
                b'8' => self.state = State::S8,
                b'9' => self.state = State::S9,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S3 => match c {
                b'0' => self.state = State::S30,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S30 => match c {
                b'0' => self.state = State::S300,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S300 => match c {
                b'8' => self.state = State::S3008,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S3008 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S1 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                b'0' => self.state = State::S10,
                b'1' => self.state = State::S11,
                b'2' => self.state = State::S12,
                b'3' => self.state = State::S13,
                b'4' => self.state = State::S14,
                b'5' => self.state = State::S15,
                b'6' => self.state = State::S16,
                b'7' => self.state = State::S17,
                b'8' => self.state = State::S18,
                b'9' => self.state = State::S19,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S10 => match c {
                b';' => {
                    if self.ensure_allocator() {
                        self.capture_trailing(CaptureMode::Fixed)
                    }
                }
                b'4' => self.state = State::S104,
                b'5' => self.state = State::S105,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S11 => match c {
                b';' => {
                    if self.ensure_allocator() {
                        self.capture_trailing(CaptureMode::Fixed)
                    }
                }
                b'0' => self.state = State::S110,
                b'1' => self.state = State::S111,
                b'2' => self.state = State::S112,
                b'3' => self.state = State::S113,
                b'4' => self.state = State::S114,
                b'5' => self.state = State::S115,
                b'6' => self.state = State::S116,
                b'7' => self.state = State::S117,
                b'8' => self.state = State::S118,
                b'9' => self.state = State::S119,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S4
            | State::S12
            | State::S14
            | State::S15
            | State::S16
            | State::S17
            | State::S18
            | State::S19
            | State::S21
            | State::S104
            | State::S105
            | State::S110
            | State::S111
            | State::S112
            | State::S113
            | State::S114
            | State::S115
            | State::S116
            | State::S117
            | State::S118
            | State::S119 => match c {
                b';' => {
                    if self.ensure_allocator() {
                        self.capture_trailing(CaptureMode::Fixed)
                    }
                }
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S13 => match c {
                b';' => {
                    if self.ensure_allocator() {
                        self.capture_trailing(CaptureMode::Fixed)
                    }
                }
                b'3' => self.state = State::S133,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S2 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                b'1' => self.state = State::S21,
                b'2' => self.state = State::S22,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S5 => match c {
                b';' => {
                    if self.ensure_allocator() {
                        self.capture_trailing(CaptureMode::Fixed)
                    }
                }
                b'2' => self.state = State::S52,
                b'5' => self.state = State::S55,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S6 => match c {
                b'6' => self.state = State::S66,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S52 | State::S66 => match c {
                b';' => self.capture_trailing(CaptureMode::Allocating),
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S55 => match c {
                b'2' => self.state = State::S552,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S7 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                b'2' => self.state = State::S72,
                b'5' => self.state = State::S75,
                b'7' => self.state = State::S77,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S75 => match c {
                b'0' => self.state = State::S750,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S750 => match c {
                b'1' => self.state = State::S7501,
                _ => self.unknown_or_invalid(Some(c)),
            },

            // Allocating-mode capture, as `S72`: a report body may be up to
            // 4087 bytes, exceeding the fixed buffer.
            State::S7501 => match c {
                b';' => self.capture_trailing(CaptureMode::Allocating),
                _ => self.unknown_or_invalid(Some(c)),
            },

            // The oracle uses allocating-mode capture here — same as
            // `S52`/`S66`/`S5522` — because the protocol allows payloads up to
            // 4096 bytes per chunk plus metadata, exceeding the fixed buffer.
            State::S72 => match c {
                b';' => self.capture_trailing(CaptureMode::Allocating),
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S77 => match c {
                b'7' => self.state = State::S777,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S133 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                b'7' => self.state = State::S1337,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S552 => match c {
                b'2' => self.state = State::S5522,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S1337 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S5522 => match c {
                b';' => self.capture_trailing(CaptureMode::Allocating),
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S9 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                b'9' => self.state = State::S99,
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S99 => match c {
                // OSC 99 encoded payloads can exceed the fixed buffer.
                b';' => self.capture_trailing(CaptureMode::Allocating),
                _ => self.unknown_or_invalid(Some(c)),
            },

            State::S0 | State::S22 | State::S777 | State::S8 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                _ => self.unknown_or_invalid(Some(c)),
            },
        }
    }

    /// End the sequence and return the command it contains, or null if it
    /// isn't a valid command.
    ///
    /// `terminator_ch` is the byte that ended the sequence. Commands that
    /// reply to the program end their reply the same way: BEL (0x07) gets
    /// a BEL reply, and any other byte, or null, gets an ST reply.
    ///
    /// A program can also cancel a sequence partway through by sending CAN
    /// or SUB instead of a terminator. Pass that byte as `terminator_ch`.
    /// The sequence is then discarded and this returns null, whatever
    /// command it contained. This matches xterm.
    ///
    /// ```zig
    /// p.nextSlice("2;hello");
    ///
    /// // The program sent CAN instead of BEL, so the title never changes.
    /// const cmd = p.end(std.ascii.control_code.can); // null
    /// ```
    ///
    /// The returned pointer is only valid until the next call to the parser.
    /// Copy out any data you need to keep.
    // PORT NOTE (parity tail S7-7): `?*Command` into `self.command` becomes
    // the command by value, borrowing `self` (see `Command`); "only valid
    // until the next call to the parser" is the borrow.
    pub fn end(&mut self, terminator_ch: Option<u8>) -> Option<Command<'_>> {
        // std.ascii.control_code.can, std.ascii.control_code.sub
        if matches!(terminator_ch, Some(0x18 | 0x1A)) {
            return None;
        }

        match self.state {
            State::Start => None,

            State::Invalid => None,

            State::Unknown | State::UnknownTruncated => {
                parsers::unknown::parse(self, terminator_ch)
            }

            // These states only lead to longer numbers (3 leads to 3008)
            // and are not OSCs themselves, so ending on one is unknown.
            State::S3
            | State::S30
            | State::S300
            | State::S55
            | State::S552
            | State::S6
            | State::S75
            | State::S750
            | State::S77 => {
                self.unknown_or_invalid(None);
                if self.state == State::Invalid {
                    return None;
                }
                parsers::unknown::parse(self, terminator_ch)
            }

            State::S0 | State::S2 => parsers::change_window_title::parse(self, terminator_ch),

            State::S1 => parsers::change_window_icon::parse(self, terminator_ch),

            State::S4
            | State::S5
            | State::S10
            | State::S11
            | State::S12
            | State::S13
            | State::S14
            | State::S15
            | State::S16
            | State::S17
            | State::S18
            | State::S19
            | State::S104
            | State::S105
            | State::S110
            | State::S111
            | State::S112
            | State::S113
            | State::S114
            | State::S115
            | State::S116
            | State::S117
            | State::S118
            | State::S119 => parsers::color::parse(self, terminator_ch),

            State::S7 => parsers::report_pwd::parse(self, terminator_ch),

            State::S8 => parsers::hyperlink::parse(self, terminator_ch),

            State::S9 => parsers::osc9::parse(self, terminator_ch),

            State::S21 => parsers::kitty_color::parse(self, terminator_ch),

            State::S22 => parsers::mouse_shape::parse(self, terminator_ch),

            State::S52 => parsers::clipboard_operation::parse(self, terminator_ch),

            State::S3008 => parsers::context_signal::parse(self, terminator_ch),

            State::S66 => parsers::kitty_text_sizing::parse(self, terminator_ch),

            State::S72 => parsers::kitty_dnd_protocol::parse(self, terminator_ch),

            State::S99 => parsers::kitty_desktop_notification::parse(self, terminator_ch),

            State::S7501 => parsers::program_status::parse(self, terminator_ch),

            State::S133 => parsers::semantic_prompt::parse(self, terminator_ch),

            State::S777 => parsers::rxvt_extension::parse(self, terminator_ch),

            State::S1337 => parsers::iterm2::parse(self, terminator_ch),

            State::S5522 => parsers::kitty_clipboard_protocol::parse(self, terminator_ch),
        }
    }
}

// test {
//     _ = parsers;
//     _ = encoding;
// }
// PORT NOTE (vt-6): the reference bottom `test { _ = parsers; _ = encoding; }`
// forces analysis of the `parsers` and `encoding` submodules (each of which has
// its own `test {}` blocks). Rust runs the `#[test]`s of `pub mod` submodules
// unconditionally when building the crate's test target, so referencing them
// here is automatic; the mirror below touches a public decl of each so a
// regression (a removed/renamed submodule) surfaces as a test failure.
#[cfg(test)]
mod ref_tests {
    #[test]
    fn ref_parsers_and_encoding() {
        // _ = parsers;
        let _ = super::parsers::color::parse as fn(_, _) -> _;
        // _ = encoding;
        let _ = super::encoding::is_safe_utf8 as fn(_) -> _;
    }
}

#[cfg(test)]
mod capture_tests {
    use super::*;

    // Parity S156-F4 (no reference test): `ensureAllocator` warns, naming the
    // state with `{t}`, before it invalidates the parser.
    #[test]
    fn ensure_allocator_warns_when_no_allocator_was_provided() {
        assert_eq!("start", State::Start.tag_name());
        assert_eq!("invalid", State::Invalid.tag_name());
        assert_eq!("5522", State::S5522.tag_name());

        let mut p = Parser::init(None);
        let cap = crate::log_capture::start();
        for ch in b"10;" {
            p.next(*ch);
        }
        let messages = cap.messages();
        drop(cap);
        assert_eq!(State::Invalid, p.state);
        assert_eq!(
            vec![(
                log::Level::Warn,
                "osc".to_string(),
                "An allocator is required to process OSC 10 but none was provided.".to_string(),
            )],
            messages
        );
    }

    // test "Parser end with CAN or SUB cancels the command"
    #[test]
    fn parser_end_with_can_or_sub_cancels_the_command() {
        let mut p: Parser = Parser::init(Some(crate::alloc::Global));

        // A fixed capture (OSC 2) cancelled by CAN.
        for ch in b"2;title" {
            p.next(*ch);
        }
        assert!(p.end(Some(0x18)).is_none());

        // An allocating capture (OSC 52) cancelled by SUB.
        p.reset();
        for ch in b"52;c;Zm9v" {
            p.next(*ch);
        }
        assert!(p.end(Some(0x1A)).is_none());

        // The parser still works normally after a cancel.
        p.reset();
        for ch in b"2;title" {
            p.next(*ch);
        }
        let cmd = p.end(Some(0x07)).unwrap();
        match cmd {
            Command::ChangeWindowTitle(title) => assert_eq!(title, b"title"),
            _ => panic!("expected change_window_title"),
        }
    }

    // test "Parser allocating captures have a hard limit"
    #[test]
    fn parser_allocating_captures_have_a_hard_limit() {
        let prefixes: [&str; 5] = ["52;", "66;", "72;", "99;", "5522;"];
        let limit = MAX_BUF + 1;

        for prefix in prefixes {
            let alloc = crate::alloc::Global;
            let mut p: Parser = Parser::init(Some(alloc));
            p.max_allocating_bytes = limit;

            for ch in prefix.bytes() {
                p.next(ch);
            }
            for _ in 0..limit {
                p.next(b'a');
            }

            {
                let cap = p.capture.as_ref().unwrap();
                assert_eq!(limit, cap.trailing(&p.buffer).len());
                // try testing.expectEqual(@as(usize, limit), cap.writer.buffer.len);
                // PORT NOTE: the allocating writer's backing buffer length is
                // the Vec's capacity here (explicit bounded growth keeps it
                // exact).
                match &cap.backing {
                    CaptureBacking::Allocating(w) => assert_eq!(limit, w.capacity()),
                    CaptureBacking::Fixed(_) => unreachable!(),
                }
            }

            p.next(b'a');
            assert_eq!(State::Invalid, p.state);
            {
                let cap = p.capture.as_ref().unwrap();
                assert_eq!(limit, cap.trailing(&p.buffer).len());
                match &cap.backing {
                    CaptureBacking::Allocating(w) => assert_eq!(limit, w.capacity()),
                    CaptureBacking::Fixed(_) => unreachable!(),
                }
            }

            p.deinit();
        }
    }

    // test "Parser nextSlice allocating captures have a hard limit"
    #[test]
    fn parser_next_slice_allocating_captures_have_a_hard_limit() {
        let limit = MAX_BUF + 1;

        let alloc = crate::alloc::Global;
        let mut p: Parser = Parser::init(Some(alloc));
        p.max_allocating_bytes = limit;

        // const data = try testing.allocator.alloc(u8, limit);
        // defer testing.allocator.free(data);
        // @memset(data, 'a');
        let data = vec![b'a'; limit];

        // Exactly at the limit stays valid and bounded.
        p.next_slice(b"52;");
        p.next_slice(&data);
        {
            let cap = p.capture.as_ref().unwrap();
            assert!(p.state != State::Invalid);
            assert_eq!(limit, cap.trailing(&p.buffer).len());
            // try testing.expectEqual(@as(usize, limit), cap.writer.buffer.len);
            // PORT NOTE: the allocating writer's backing buffer length is the
            // Vec's capacity here (explicit bounded growth keeps it exact).
            match &cap.backing {
                CaptureBacking::Allocating(w) => assert_eq!(limit, w.capacity()),
                CaptureBacking::Fixed(_) => unreachable!(),
            }
        }

        // One more byte overflows: the state becomes invalid and the
        // retained bytes and allocation stay bounded.
        p.next_slice(b"a");
        assert_eq!(State::Invalid, p.state);
        {
            let cap = p.capture.as_ref().unwrap();
            assert_eq!(limit, cap.trailing(&p.buffer).len());
            match &cap.backing {
                CaptureBacking::Allocating(w) => assert_eq!(limit, w.capacity()),
                CaptureBacking::Fixed(_) => unreachable!(),
            }
        }
        assert!(p.end(None).is_none());

        p.deinit();
    }

    // test "Parser nextSlice overflowing slice is truncated at the limit"
    #[test]
    fn parser_next_slice_overflowing_slice_is_truncated_at_the_limit() {
        let alloc = crate::alloc::Global;
        let mut p: Parser = Parser::init(Some(alloc));
        p.max_allocating_bytes = 4;

        p.next_slice(b"52;abcdef");
        assert_eq!(State::Invalid, p.state);
        assert!(p.end(None).is_none());

        let cap = p.capture.as_ref().unwrap();
        // try testing.expectEqualStrings("abcd", cap.trailing());
        assert_eq!(b"abcd".as_slice(), cap.trailing(&p.buffer));
        // try testing.expectEqual(@as(usize, 4), cap.writer.buffer.len);
        match &cap.backing {
            CaptureBacking::Allocating(w) => assert_eq!(4, w.capacity()),
            CaptureBacking::Fixed(_) => unreachable!(),
        }

        p.deinit();
    }

    // Port-only (no reference test): pins the bytes, length and capacity of
    // `Capture::write_slice`'s allocating arm, whose append is a raw
    // `copy_nonoverlapping` + `set_len` (review-T-A2 blocking item 1). It
    // covers the three shapes: an append that fits in the spare capacity, an
    // append that grows the buffer (doubling, then clamped to `max_bytes`),
    // and an append truncated at `max_bytes`. Distinct bytes per position
    // catch a wrong offset or a stale length.
    #[test]
    fn capture_write_slice_allocating_arm_appends_exact_bytes() {
        let mut buffer = [0u8; MAX_BUF];
        let mut w = allocator_api2::vec::Vec::new_in(CaptureAllocator::GLOBAL);
        w.try_reserve_exact(4).unwrap();
        let mut cap = Capture {
            backing: CaptureBacking::Allocating(w),
            max_bytes: 10,
        };
        let cap_of = |cap: &Capture| match &cap.backing {
            CaptureBacking::Allocating(w) => w.capacity(),
            CaptureBacking::Fixed(_) => unreachable!(),
        };
        let initial = cap_of(&cap);
        assert!(initial >= 4);

        // Fits in the reserved capacity: no growth.
        assert!(cap.write_slice(&mut buffer, b"ab"));
        assert_eq!(b"ab".as_slice(), cap.trailing(&buffer));
        assert_eq!(initial, cap_of(&cap));

        // Empty append is a no-op.
        assert!(cap.write_slice(&mut buffer, b""));
        assert_eq!(b"ab".as_slice(), cap.trailing(&buffer));

        // Needs growth: needed = 2 + 5 = 7.
        assert!(cap.write_slice(&mut buffer, b"cdefg"));
        assert_eq!(b"abcdefg".as_slice(), cap.trailing(&buffer));
        assert!(cap_of(&cap) >= 7 && cap_of(&cap) <= 10);

        // Truncated at max_bytes: the first 3 bytes land, the write fails.
        assert!(!cap.write_slice(&mut buffer, b"hijkl"));
        assert_eq!(b"abcdefghij".as_slice(), cap.trailing(&buffer));
        assert_eq!(10, cap_of(&cap));

        // Full: nothing more lands.
        assert!(!cap.write_slice(&mut buffer, b"z"));
        assert_eq!(b"abcdefghij".as_slice(), cap.trailing(&buffer));
        // The fixed buffer was never touched by the allocating arm.
        assert!(buffer.iter().all(|&b| b == 0));
    }

    // test "Parser nextSlice matches per-byte parsing"
    #[test]
    fn parser_next_slice_matches_per_byte_parsing() {
        let input = b"52;c;aGVsbG8=";

        // Every two-way split of the input must parse identically to
        // the byte-at-a-time path.
        for split in 0..=input.len() {
            let alloc = crate::alloc::Global;
            let mut p: Parser = Parser::init(Some(alloc));
            p.next_slice(&input[0..split]);
            p.next_slice(&input[split..]);

            // const cmd = p.end(null).?.*;
            let cmd = p.end(None).unwrap();
            // try testing.expect(cmd == .clipboard_contents);
            assert!(matches!(cmd, Command::ClipboardContents { .. }));
            match &cmd {
                Command::ClipboardContents { kind, data, .. } => {
                    // try testing.expectEqual(@as(u8, 'c'), cmd.clipboard_contents.kind);
                    assert_eq!(b'c', *kind);
                    // try testing.expectEqualStrings("aGVsbG8=", cmd.clipboard_contents.data);
                    assert_eq!(b"aGVsbG8=".as_slice(), data.as_slice());
                }
                _ => unreachable!(),
            }

            p.deinit();
        }
    }

    // test "Parser allocating capture limit includes parser-added bytes"
    #[test]
    fn parser_allocating_capture_limit_includes_parser_added_bytes() {
        let alloc = crate::alloc::Global;
        let mut p: Parser = Parser::init(Some(alloc));
        p.max_allocating_bytes = 4;

        for ch in "52;abcd".bytes() {
            p.next(ch);
        }
        assert!(p.end(None).is_none());
        assert_eq!(State::Invalid, p.state);

        let cap = p.capture.as_ref().unwrap();
        assert_eq!(4, cap.trailing(&p.buffer).len());
        // try testing.expectEqual(@as(usize, 4), cap.writer.buffer.len);
        match &cap.backing {
            CaptureBacking::Allocating(w) => assert_eq!(4, w.capacity()),
            CaptureBacking::Fixed(_) => unreachable!(),
        }

        p.deinit();
    }
}

// Parity tail S7-7 (Rust-only, no reference tests): the evidence that the OSC
// parser's allocations go where the reference's do (`parser.alloc`) and nowhere
// else, that a refusal takes the reference's path, and that the command's text
// payloads are BORROWED from the parser's own storage (N263). The borrow tests
// are also the Miri targets for the `Command<'p>` design (a command borrowing
// the parser buffer while the parser itself is `&mut` elsewhere).
#[cfg(test)]
mod s7_7_tests {
    use super::*;
    use crate::alloc::testing::{with_handle, Config, CountingAllocator, FailingAllocator};

    fn feed(p: &mut Parser, input: &[u8]) {
        for &ch in input {
            p.next(ch);
        }
    }

    /// `color::Request`'s size, the element of the list's shelves.
    fn request_size() -> usize {
        core::mem::size_of::<color::Request>()
    }

    // `color_operation.requests` is `SegmentedList(Request, 2)` over
    // `parser.alloc`: one or two requests allocate nothing; the third
    // allocates the shelf array and shelf 0 (4 requests); the parser keeps the
    // list (the reference's `parser.command` owns it) and `reset` frees both.
    // Exact counts: a request that reached the Rust heap, or a
    // `Vec`, would change them.
    #[test]
    fn color_requests_route_through_the_parser_allocator() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));

            feed(&mut p, b"4;1;?;2;?");
            {
                let cmd = p.end(Some(0x07)).unwrap();
                match &cmd {
                    Command::ColorOperation { requests, .. } => assert_eq!(2, requests.count()),
                    other => panic!("expected color_operation, got {other:?}"),
                }
                assert_eq!(
                    0,
                    counting.alloc_calls.get(),
                    "two requests fit the prealloc"
                );
            }
            assert_eq!(0, counting.free_calls.get());

            p.reset();
            feed(&mut p, b"4;1;?;2;?;3;?");
            {
                let cmd = p.end(Some(0x07)).unwrap();
                match &cmd {
                    Command::ColorOperation { requests, .. } => {
                        assert_eq!(3, requests.count());
                        assert_eq!(
                            color::Request::Query(color::Target::Palette(3)),
                            *requests.at(2)
                        );
                    }
                    other => panic!("expected color_operation, got {other:?}"),
                }
                assert_eq!(2, counting.alloc_calls.get(), "the shelf array and shelf 0");
                assert_eq!(
                    Some((
                        4 * request_size(),
                        core::mem::align_of::<color::Request>().trailing_zeros() as u8
                    )),
                    counting.last_alloc.get(),
                    "shelf 0 holds prealloc * 2 requests"
                );
                assert_eq!(0, counting.free_calls.get(), "live while the command is");
            }
            assert_eq!(0, counting.free_calls.get(), "the parser keeps the list");
            p.reset();
            assert_eq!(2, counting.free_calls.get(), "`reset` frees it");
            assert_eq!(0, counting.live_bytes.get());
            p.deinit();
        });
        assert_eq!(0, counting.live_bytes.get());
    }

    // `kitty_color_protocol.list` is an `ArrayList(Request)` appended through
    // `parser.alloc`: Zig 0.16's first `append` reserves `growCapacity(1)`
    // requests in one allocation; the parser keeps it until `reset` or
    // `deinit`, as the reference's `parser.command` does.
    #[test]
    fn kitty_color_list_routes_through_the_parser_allocator() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            feed(&mut p, b"21;foreground=?;background=#ffffff;cursor=");
            {
                let cmd = p.end(Some(0x1b)).unwrap();
                match &cmd {
                    Command::KittyColorProtocol(osc) => assert_eq!(3, osc.list.len()),
                    other => panic!("expected kitty_color_protocol, got {other:?}"),
                }
                let first =
                    crate::alloc::ArrayListUnmanaged::<crate::kitty::color::Request>::grow_capacity(
                        1,
                    );
                assert!(first >= 3, "one reservation holds all three");
                assert_eq!(1, counting.alloc_calls.get());
                assert_eq!(
                    first * core::mem::size_of::<crate::kitty::color::Request>(),
                    counting.last_alloc.get().unwrap().0
                );
            }
            assert_eq!(0, counting.free_calls.get(), "the parser keeps the list");
            p.deinit();
            assert_eq!(1, counting.free_calls.get(), "`deinit` frees it");
            assert_eq!(0, counting.live_bytes.get());
        });
    }

    // T2: a refused list allocation takes the reference's `catch`: the
    // command is still produced, with an empty list, and `log.info` names the
    // OSC, the error and the data. Nothing is left allocated.
    #[test]
    fn color_request_allocation_failure_logs_and_yields_an_empty_list() {
        let failing = FailingAllocator::over_heap(Config {
            fail_index: 0,
            ..Config::default()
        });
        let c = failing.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            feed(&mut p, b"4;1;?;2;?;3;?");
            let cap = crate::log_capture::start();
            {
                let cmd = p.end(Some(0x07)).unwrap();
                match &cmd {
                    Command::ColorOperation { op, requests, .. } => {
                        assert_eq!(color::Operation::osc_4, *op);
                        assert_eq!(0, requests.count());
                    }
                    other => panic!("expected color_operation, got {other:?}"),
                }
            }
            let messages = cap.messages();
            drop(cap);
            assert!(
                messages.iter().any(|(level, target, msg)| {
                    *level == log::Level::Info
                    && target == "osc_color"
                    && msg == "failed to parse OSC 4 color request err=OutOfMemory data=1;?;2;?;3;?"
                }),
                "expected the parseColor catch diagnostic, got: {messages:?}"
            );
            assert!(failing.has_induced_failure.get());
            p.deinit();
        });
        assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
        assert_eq!(failing.allocations.get(), failing.deallocations.get());
    }

    // PORT NOTE: reference defect (R13), N321. `parseGetSetDynamicColor` has no
    // `errdefer result.deinit(alloc)`: an OSC 10 whose seventh value fails to
    // grow the list leaks shelf 0 and the shelf array in the reference. The
    // port's list is freed when the error drops it. Allocation 0 and 1 are the
    // third request's shelf array and shelf 0; allocation 2 (the seventh
    // request's new shelf array) is refused.
    #[test]
    fn dynamic_color_list_growth_failure_frees_what_it_grew() {
        let failing = FailingAllocator::over_heap(Config {
            fail_index: 2,
            ..Config::default()
        });
        let c = failing.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            feed(&mut p, b"10;?;?;?;?;?;?;?");
            let cap = crate::log_capture::start();
            {
                let cmd = p.end(Some(0x07)).unwrap();
                match &cmd {
                    Command::ColorOperation { op, requests, .. } => {
                        assert_eq!(color::Operation::osc_10, *op);
                        assert_eq!(0, requests.count(), "the catch's empty list");
                    }
                    other => panic!("expected color_operation, got {other:?}"),
                }
            }
            let messages = cap.messages();
            drop(cap);
            assert!(
                messages.iter().any(|(_, target, msg)| target == "osc_color"
                    && msg.starts_with("failed to parse OSC 10 color request err=OutOfMemory")),
                "the failure path ran: {messages:?}"
            );
            assert!(failing.has_induced_failure.get());
            assert_eq!(
                2,
                failing.allocations.get(),
                "shelf array and shelf 0 were grown"
            );
            assert_eq!(
                failing.allocated_bytes.get(),
                failing.freed_bytes.get(),
                "the grown shelves are freed on the error path"
            );
            p.deinit();
        });
    }

    // T2: a refused kitty list append logs the reference's warning and skips
    // that key (`continue`): the loop reaches the next key, whose append is
    // refused as well (the failing allocator's refusal is sticky, as Zig's
    // is), so both keys log and the command is still produced.
    #[test]
    fn kitty_color_append_failure_logs_and_continues() {
        let failing = FailingAllocator::over_heap(Config {
            fail_index: 0,
            ..Config::default()
        });
        let c = failing.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            feed(&mut p, b"21;foreground=?;background=?");
            let cap = crate::log_capture::start();
            {
                let cmd = p.end(Some(0x1b)).unwrap();
                match &cmd {
                    Command::KittyColorProtocol(osc) => assert_eq!(0, osc.list.len()),
                    other => panic!("expected kitty_color_protocol, got {other:?}"),
                }
            }
            let messages = cap.messages();
            drop(cap);
            assert_eq!(
                2,
                messages
                    .iter()
                    .filter(|(level, target, msg)| *level == log::Level::Warn
                        && target == "osc_kitty_color"
                        && msg == "unable to append kitty color protocol option: OutOfMemory")
                    .count(),
                "got: {messages:?}"
            );
            p.deinit();
        });
        assert_eq!(failing.allocated_bytes.get(), failing.freed_bytes.get());
    }

    // N263: text payloads are borrowed from the parser's storage, not copied:
    // a title from the fixed capture points into `Parser.buffer`, with its NUL
    // sentinel inside the borrow, and parsing it allocates nothing.
    #[test]
    fn fixed_capture_payloads_borrow_the_parser_buffer() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            let buffer = p.buffer.as_ptr_range();
            feed(&mut p, b"0;a title");
            {
                let cmd = p.end(None).unwrap();
                let Command::ChangeWindowTitle(title) = &cmd else {
                    panic!("expected change_window_title, got {cmd:?}");
                };
                assert_eq!(b"a title", title.as_bytes());
                assert_eq!(b"a title\0", title.as_bytes_with_nul());
                assert!(
                    buffer.contains(&title.as_ptr()),
                    "the title is the buffer's bytes"
                );
                // SAFETY: `as_ptr` is a NUL-terminated string valid while the
                // command is (the C caller's read, inside the borrow).
                let c_str = unsafe { core::ffi::CStr::from_ptr(title.as_ptr().cast()) };
                assert_eq!(b"a title", c_str.to_bytes());
            }
            // The same parser, fed again (the borrow has ended).
            p.reset();
            feed(&mut p, b"8;id=x;https://example.com");
            {
                let cmd = p.end(None).unwrap();
                let Command::HyperlinkStart { id, uri } = &cmd else {
                    panic!("expected hyperlink_start, got {cmd:?}");
                };
                assert_eq!(Some(b"x".as_slice()), id.map(|id| id.as_bytes()));
                assert_eq!(b"https://example.com", uri.as_bytes());
                assert!(buffer.contains(&uri.as_ptr()));
                assert!(buffer.contains(&id.unwrap().as_ptr()));
            }
            assert_eq!(0, counting.alloc_calls.get(), "no payload copy, no list");
            p.deinit();
        });
    }

    // N263: a payload from an allocating capture points into the capture's
    // own allocation; the only allocation is the capture's (`initCapacity`,
    // `MAX_BUF` bytes), none for the payload.
    #[test]
    fn allocating_capture_payloads_borrow_the_capture() {
        let counting = CountingAllocator::over_heap();
        let c = counting.allocator();
        with_handle(&c, |alloc| {
            let mut p = Parser::init(Some(alloc));
            let buffer = p.buffer.as_ptr_range();
            feed(&mut p, b"52;c;aGVsbG8=");
            let capture = match &p.capture.as_ref().unwrap().backing {
                CaptureBacking::Allocating(w) => w.as_ptr_range(),
                CaptureBacking::Fixed(_) => panic!("OSC 52 captures with an allocator"),
            };
            {
                let cmd = p.end(Some(0x07)).unwrap();
                let Command::ClipboardContents {
                    kind,
                    data,
                    terminator,
                } = &cmd
                else {
                    panic!("expected clipboard_contents, got {cmd:?}");
                };
                assert_eq!(b'c', *kind);
                assert_eq!(Terminator::Bel, *terminator);
                assert_eq!(b"aGVsbG8=", data.as_bytes());
                assert!(
                    capture.contains(&data.as_ptr()),
                    "the data is the capture's bytes"
                );
                assert!(!buffer.contains(&data.as_ptr()));
            }
            assert_eq!(1, counting.alloc_calls.get(), "the capture only");
            assert_eq!(Some((MAX_BUF, 0)), counting.last_alloc.get());
            p.deinit();
        });
        assert_eq!(0, counting.live_bytes.get());
    }

    // The nested payloads borrow too: a kitty clipboard OSC's metadata and
    // payload, and an option value read from the metadata, are slices of the
    // capture.
    #[test]
    fn nested_payloads_and_option_values_borrow_the_capture() {
        let mut p = Parser::init(Some(crate::alloc::Global));
        feed(&mut p, b"5522;type=read:mime=dGV4dA==;cGF5bG9hZA==");
        let capture = match &p.capture.as_ref().unwrap().backing {
            CaptureBacking::Allocating(w) => w.as_ptr_range(),
            CaptureBacking::Fixed(_) => panic!("OSC 5522 captures with an allocator"),
        };
        {
            let cmd = p.end(Some(0x1b)).unwrap();
            let Command::KittyClipboardProtocol(osc) = &cmd else {
                panic!("expected kitty_clipboard_protocol, got {cmd:?}");
            };
            assert_eq!(b"type=read:mime=dGV4dA==", osc.metadata);
            assert_eq!(Some(b"cGF5bG9hZA==".as_slice()), osc.payload);
            assert!(capture.contains(&osc.metadata.as_ptr()));
            assert!(capture.contains(&osc.payload.unwrap().as_ptr()));
            let mime = osc
                .read_option(parsers::kitty_clipboard_protocol::Option_::Mime)
                .as_bytes()
                .unwrap();
            assert_eq!(b"dGV4dA==", mime);
            assert!(capture.contains(&mime.as_ptr()));
        }
        p.deinit();
    }

    // The stream hands the borrowed command to its handler (the OSC fast path
    // and the action loop), and the handler sees the parser's own bytes.
    #[test]
    fn the_stream_dispatches_the_borrowed_command() {
        use crate::stream::{Handler, Options, Stream, WindowTitle};

        #[derive(Default)]
        struct Titles {
            seen: Vec<(Vec<u8>, usize)>,
        }
        impl Handler for Titles {
            fn window_title(&mut self, v: WindowTitle) {
                self.seen.push((v.title.to_vec(), v.title.as_ptr().addr()));
            }
        }

        let mut s = Stream::init(Options {
            handler: Titles::default(),
            allocator: Some(crate::alloc::Global),
            continuation_max_bytes: None,
            osc_unknown_max_bytes: 0,
        });
        // (Compared as addresses, so no integer is cast back to a pointer:
        // strict provenance, S7-9.)
        let buffer = s.parser.osc_parser.buffer.as_ptr_range();
        let buffer = buffer.start.addr()..buffer.end.addr();
        // BEL (the bulk fast path), ESC \ (the fast path), and a byte at a
        // time (the action loop).
        s.next_slice(b"\x1b]0;one\x07\x1b]2;two\x1b\\");
        for &b in b"\x1b]0;three\x07" {
            s.next(b);
        }
        let seen = std::mem::take(&mut s.handler.seen);
        assert_eq!(
            vec![b"one".to_vec(), b"two".to_vec(), b"three".to_vec()],
            seen.iter().map(|(t, _)| t.clone()).collect::<Vec<_>>()
        );
        for (_, addr) in &seen {
            assert!(buffer.contains(addr), "the handler saw the parser's buffer");
        }
        s.deinit();
    }
}
