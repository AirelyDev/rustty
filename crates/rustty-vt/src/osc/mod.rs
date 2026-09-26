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

#[derive(Debug, Clone, PartialEq)]
pub enum Command {
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
    /// NUL-terminated bytes). Modeled as owned `Vec<u8>` — NOT a lossy `String`.
    ChangeWindowTitle(Vec<u8>),

    /// Set the icon of the terminal window. The name of the icon is not
    /// well defined, so this is currently ignored by Ghostty at the time
    /// of writing this. We just parse it so that we don't get parse errors
    /// in the log.
    ///
    /// PORT NOTE (fidelity): reference `[:0]const u8` — raw bytes, not lossy.
    ChangeWindowIcon(Vec<u8>),

    /// Semantic prompt command: https://gitlab.freedesktop.org/Per_Bothner/specifications/blob/master/proposals/semantic-prompts.md
    SemanticPrompt(SemanticPrompt),

    /// Set or get clipboard contents. If data is "?", then the current
    /// clipboard contents are sent to the pty. Otherwise, the contents
    /// are set on the clipboard.
    ClipboardContents {
        kind: u8,
        /// PORT NOTE (fidelity): reference `data: [:0]const u8` — raw bytes.
        data: Vec<u8>,
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
        value: Vec<u8>,
    },

    /// OSC 22. Set the mouse shape. There doesn't seem to be a standard
    /// naming scheme for cursors but it looks like terminals such as Foot
    /// are moving towards using the W3C CSS cursor names. For OSC parsing,
    /// we just parse whatever string is given.
    MouseShape {
        /// PORT NOTE (fidelity): reference `value: [:0]const u8` — raw bytes.
        value: Vec<u8>,
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
        requests: color::List,
        terminator: Terminator,
    },

    /// Kitty color protocol, OSC 21
    /// https://sw.kovidgoyal.net/kitty/color-stack/#id1
    KittyColorProtocol(kitty_color::OSC),

    /// Show a desktop notification (OSC 9 or OSC 777)
    ShowDesktopNotification {
        /// PORT NOTE (fidelity): reference `title: [:0]const u8` — raw bytes.
        title: Vec<u8>,
        /// PORT NOTE (fidelity): reference `body: [:0]const u8` — raw bytes.
        body: Vec<u8>,
    },

    /// Start a hyperlink (OSC 8)
    HyperlinkStart {
        /// PORT NOTE (fidelity): reference `id: ?[:0]const u8` — optional raw bytes.
        id: Option<Vec<u8>>,
        /// PORT NOTE (fidelity): reference `uri: [:0]const u8` — raw bytes.
        uri: Vec<u8>,
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
    ConemuShowMessageBox(Vec<u8>),

    /// ConEmu change tab title (OSC 9;3)
    ConemuChangeTabTitle(ConemuChangeTabTitle),

    /// ConEmu progress report (OSC 9;4)
    ConemuProgressReport(ProgressReport),

    /// ConEmu wait input (OSC 9;5)
    ConemuWaitInput,

    /// ConEmu GUI macro (OSC 9;6)
    ///
    /// PORT NOTE (fidelity): reference `[:0]const u8` — raw bytes.
    ConemuGuimacro(Vec<u8>),

    /// ConEmu run process (OSC 9;7)
    ///
    /// PORT NOTE (fidelity): reference `[:0]const u8` — raw bytes.
    ConemuRunProcess(Vec<u8>),

    /// ConEmu output environment variable (OSC 9;8)
    ///
    /// PORT NOTE (fidelity): reference `[:0]const u8` — raw bytes.
    ConemuOutputEnvironmentVariable(Vec<u8>),

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
    ConemuComment(Vec<u8>),

    /// Kitty text sizing protocol (OSC 66)
    KittyTextSizing(parsers::kitty_text_sizing::OSC),

    KittyClipboardProtocol(KittyClipboardProtocol),

    /// Kitty drag and drop protocol (OSC 72)
    /// https://sw.kovidgoyal.net/kitty/drag-and-drop-protocol/
    KittyDndProtocol(KittyDndProtocol),

    /// OSC 3008. Hierarchical context signalling (UAPI spec).
    /// https://uapi-group.org/specifications/specs/osc_context/
    ContextSignal(parsers::context_signal::Command),

    /// Kitty desktop notifications (OSC 99)
    KittyDesktopNotification(KittyDesktopNotification),
}

// PORT NOTE: `conemu_change_tab_title: union(enum) { reset, value: [:0]const u8 }`
// — the anonymous tagged union is hoisted to a named type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConemuChangeTabTitle {
    Reset,
    /// PORT NOTE (fidelity): reference `value: [:0]const u8` — raw bytes.
    Value(Vec<u8>),
}

impl Command {
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
    // kitty_desktop_notification).
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
}

/// PORT NOTE: `parsers.semantic_prompt.Command` re-exported as
/// `Command.SemanticPrompt`.
pub type SemanticPrompt = parsers::semantic_prompt::Command;

/// PORT NOTE: `parsers.kitty_clipboard_protocol.OSC` re-exported as
/// `Command.KittyClipboardProtocol`.
pub type KittyClipboardProtocol = parsers::kitty_clipboard_protocol::OSC;

/// PORT NOTE: `parsers.kitty_dnd_protocol.OSC` re-exported as
/// `Command.KittyDndProtocol`.
pub type KittyDndProtocol = parsers::kitty_dnd_protocol::OSC;

/// PORT NOTE: `parsers.kitty_desktop_notification.OSC` re-exported as
/// `Command.KittyDesktopNotification` (`9a8e7ae18`).
pub type KittyDesktopNotification = parsers::kitty_desktop_notification::OSC;

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

pub struct Parser<'a> {
    /// Optional allocator used to accept data longer than MAX_BUF.
    /// This only applies to some commands (e.g. OSC 52) that can
    /// reasonably exceed MAX_BUF.
    pub alloc: Option<rustty_lib::allocator::ZigAllocatorRef<'a>>,

    /// Maximum number of bytes retained by an allocating capture.
    /// This is configurable primarily so callers and tests can choose a
    /// smaller policy than the default.
    pub max_allocating_bytes: usize,

    /// Where an allocating capture's memory comes from.
    // PORT NOTE (parity S174-F3): not a reference field. The reference's
    // allocating capture uses `alloc` itself
    // (`std.Io.Writer.Allocating.initCapacity(alloc, ..)`, osc.zig:509), but
    // `alloc` is a `ZigAllocatorRef<'a>` here and a capture that borrowed it
    // would give every `Stream` (whose parser shares the stream's lifetime)
    // drop glue over `'a`. So the capture's allocator is set separately, and
    // only to an allocator that outlives the parser: the embedder's allocator
    // (`crate::alloc::default(alloc_)`) for the C-created parsers (vt-c's
    // `rustty_osc_new` and the `rustty_terminal_new` stream), which is where
    // the reference would put the capture. Every other parser keeps the Rust
    // global heap, which is where their `alloc` (the process-global zig
    // allocator) allocates anyway. (S7-7 retires the split: `alloc` becomes
    // the handle itself.)
    pub capture_alloc: CaptureAllocator,

    /// Current state of the parser.
    pub state: State,

    /// Buffer for temporary storage of OSC data
    pub buffer: [u8; MAX_BUF],

    /// Capture state. If this is set then we're actively capturing the
    /// bytes coming into the parser.
    pub capture: Option<Capture>,

    /// The command that is the result of parsing.
    pub command: Command,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(non_camel_case_types)]
pub enum State {
    Start,
    Invalid,

    // OSC command prefixes. Not all of these are valid OSCs, but may be
    // needed to "bridge" to a valid OSC (e.g. to support OSC 777 we need to
    // have a state "77" even though there is no OSC 77).
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
    S77,
    S99,
    S104,
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
        let name = format!("{self:?}");
        match name.strip_prefix('S') {
            Some(digits) if digits.bytes().all(|b| b.is_ascii_digit()) => digits.to_string(),
            _ => name.to_ascii_lowercase(),
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
    /// (for the fixed backing this is the buffer length; for the allocating
    /// backing it is the parser's `max_allocating_bytes` policy).
    pub max_bytes: usize,
}

pub enum CaptureBacking {
    /// The count of bytes captured so far into `Parser.buffer` — the Zig fixed
    /// writer's `end` index over `&self.buffer` (osc.zig:462-467).
    Fixed(usize),
    /// `std.Io.Writer.Allocating`, over `Parser.capture_alloc`.
    Allocating(allocator_api2::vec::Vec<u8, CaptureAllocator>),
}

/// The allocator of an allocating capture (see `Parser.capture_alloc`).
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

impl<'a> Parser<'a> {
    pub fn init(alloc: Option<rustty_lib::allocator::ZigAllocatorRef<'a>>) -> Parser<'a> {
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
            capture_alloc: CaptureAllocator::GLOBAL,
            state: State::Start,
            capture: None,
            command: Command::Invalid,
            buffer: [0u8; MAX_BUF],
        }
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
        match &mut self.command {
            Command::KittyColorProtocol(v) => {
                // v.deinit(self.alloc orelse break :kitty_color_protocol);
                if self.alloc.is_some() {
                    v.deinit();
                }
            }
            // .color_operation => |*v| color_operation: {
            //     v.requests.deinit(self.alloc orelse break :color_operation);
            // },
            // PORT NOTE (oracle 14c829883): the reference frees the
            // `SegmentedList` backing `requests` here to avoid an allocation
            // leak on reset. The port's `color::List` is a plain `Vec<Request>`
            // (see osc/parsers/color.rs), which Rust drops (and frees) when
            // `self.command` is overwritten by `Command::Invalid` below — no
            // manual release is needed or possible to express here.
            Command::ColorOperation { .. } => {}
            Command::ChangeWindowIcon(_)
            | Command::ChangeWindowTitle(_)
            | Command::ClipboardContents { .. }
            | Command::ConemuChangeTabTitle(_)
            | Command::ConemuComment(_)
            | Command::ConemuGuimacro(_)
            | Command::ConemuOutputEnvironmentVariable(_)
            | Command::ConemuProgressReport(_)
            | Command::ConemuRunProcess(_)
            | Command::ConemuShowMessageBox(_)
            | Command::ConemuSleep { .. }
            | Command::ConemuWaitInput
            | Command::ConemuXtermEmulation { .. }
            | Command::HyperlinkEnd
            | Command::HyperlinkStart { .. }
            | Command::Invalid
            | Command::MouseShape { .. }
            | Command::ReportPwd { .. }
            | Command::SemanticPrompt(_)
            | Command::ShowDesktopNotification { .. }
            | Command::KittyTextSizing(_)
            | Command::KittyClipboardProtocol(_)
            | Command::KittyDndProtocol(_)
            | Command::KittyDesktopNotification(_)
            | Command::ContextSignal(_) => {}
        }

        self.state = State::Start;
        self.capture = None;
        self.command = Command::Invalid;

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
                if self.alloc.is_none() {
                    // We don't have an allocator - fall back to a fixed buffer and hope
                    // that it's big enough.
                    self.capture_trailing(CaptureMode::Fixed);
                    return;
                }

                // Capture.allocating(&self.capture, alloc, self.max_allocating_bytes) catch {
                //     // The allocator failed for some reason, fall back to a fixed buffer
                //     // and hope that it's big enough.
                //     self.captureTrailing(.fixed);
                //     return;
                // };
                let mut w = allocator_api2::vec::Vec::new_in(self.capture_alloc);
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
        //     // We have overflowed our buffer or had some other error, set
        //     // the state to invalid so that we discard any further input.
        //     error.WriteFailed => self.state = .invalid,
        // };
        let cap = self.capture.as_mut().unwrap();
        if !cap.write_slice(&mut self.buffer, rem) {
            // We have overflowed our buffer or had some other error, set
            // the state to invalid so that we discard any further input.
            self.state = State::Invalid;
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
            // no Vec, no grow check — the `len < MAX_BUF` guard also lets the
            // indexing bounds check fold away.
            //
            // PORT NOTE (reshape): the success store is written as the
            // fall-through arm (`len < MAX_BUF` first) — the same shape as the
            // Zig writer's `if (end < buffer.len)` fast path — so the per-byte
            // loop takes 2 taken branches instead of 3 (measured: the fat-LTO
            // build's only osc win over release-fast was exactly this block
            // layout; see PERF_LOG.md osc-parser entry).
            match &mut cap.backing {
                CaptureBacking::Fixed(len) => {
                    if *len < MAX_BUF {
                        self.buffer[*len] = c;
                        *len += 1;
                    } else {
                        // We have overflowed our buffer or had some other error, set the
                        // state to invalid so that we discard any further input.
                        self.state = State::Invalid;
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
                        // We have overflowed our buffer or had some other error, set the
                        // state to invalid so that we discard any further input.
                        self.state = State::Invalid;
                    } else {
                        if w.len() >= w.capacity() {
                            // R7: `*|` saturating multiply, `@max(.., 1)` floor.
                            let new_capacity =
                                cap.max_bytes.min(w.capacity().saturating_mul(2).max(1));
                            // `realloc(..) catch return error.WriteFailed`,
                            // which `next` turns into `.invalid` (S174-F3).
                            if w.try_reserve_exact(new_capacity - w.len()).is_err() {
                                self.state = State::Invalid;
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
                _ => self.state = State::Invalid,
            },

            State::S3 => match c {
                b'0' => self.state = State::S30,
                _ => self.state = State::Invalid,
            },

            State::S30 => match c {
                b'0' => self.state = State::S300,
                _ => self.state = State::Invalid,
            },

            State::S300 => match c {
                b'8' => self.state = State::S3008,
                _ => self.state = State::Invalid,
            },

            State::S3008 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                _ => self.state = State::Invalid,
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
                _ => self.state = State::Invalid,
            },

            State::S10 => match c {
                b';' => {
                    if self.ensure_allocator() {
                        self.capture_trailing(CaptureMode::Fixed)
                    }
                }
                b'4' => self.state = State::S104,
                _ => self.state = State::Invalid,
            },

            State::S104 => match c {
                b';' => {
                    if self.ensure_allocator() {
                        self.capture_trailing(CaptureMode::Fixed)
                    }
                }
                _ => self.state = State::Invalid,
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
                _ => self.state = State::Invalid,
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
                _ => self.state = State::Invalid,
            },

            State::S13 => match c {
                b';' => {
                    if self.ensure_allocator() {
                        self.capture_trailing(CaptureMode::Fixed)
                    }
                }
                b'3' => self.state = State::S133,
                _ => self.state = State::Invalid,
            },

            State::S2 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                b'1' => self.state = State::S21,
                b'2' => self.state = State::S22,
                _ => self.state = State::Invalid,
            },

            State::S5 => match c {
                b';' => {
                    if self.ensure_allocator() {
                        self.capture_trailing(CaptureMode::Fixed)
                    }
                }
                b'2' => self.state = State::S52,
                b'5' => self.state = State::S55,
                _ => self.state = State::Invalid,
            },

            State::S6 => match c {
                b'6' => self.state = State::S66,
                _ => self.state = State::Invalid,
            },

            State::S52 | State::S66 => match c {
                b';' => self.capture_trailing(CaptureMode::Allocating),
                _ => self.state = State::Invalid,
            },

            State::S55 => match c {
                b'2' => self.state = State::S552,
                _ => self.state = State::Invalid,
            },

            State::S7 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                b'2' => self.state = State::S72,
                b'7' => self.state = State::S77,
                _ => self.state = State::Invalid,
            },

            // The oracle uses allocating-mode capture here — same as
            // `S52`/`S66`/`S5522` — because the protocol allows payloads up to
            // 4096 bytes per chunk plus metadata, exceeding the fixed buffer.
            State::S72 => match c {
                b';' => self.capture_trailing(CaptureMode::Allocating),
                _ => self.state = State::Invalid,
            },

            State::S77 => match c {
                b'7' => self.state = State::S777,
                _ => self.state = State::Invalid,
            },

            State::S133 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                b'7' => self.state = State::S1337,
                _ => self.state = State::Invalid,
            },

            State::S552 => match c {
                b'2' => self.state = State::S5522,
                _ => self.state = State::Invalid,
            },

            State::S1337 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                _ => self.state = State::Invalid,
            },

            State::S5522 => match c {
                b';' => self.capture_trailing(CaptureMode::Allocating),
                _ => self.state = State::Invalid,
            },

            State::S9 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                b'9' => self.state = State::S99,
                _ => self.state = State::Invalid,
            },

            State::S99 => match c {
                // OSC 99 encoded payloads can exceed the fixed buffer.
                b';' => self.capture_trailing(CaptureMode::Allocating),
                _ => self.state = State::Invalid,
            },

            State::S0 | State::S22 | State::S777 | State::S8 => match c {
                b';' => self.capture_trailing(CaptureMode::Fixed),
                _ => self.state = State::Invalid,
            },
        }
    }

    /// End the sequence and return the command, if any. If the return value
    /// is null, then no valid command was found. The optional terminator_ch
    /// is the final character in the OSC sequence. This is used to determine
    /// the response terminator.
    ///
    /// The returned pointer is only valid until the next call to the parser.
    /// Callers should copy out any data they wish to retain across calls.
    pub fn end(&mut self, terminator_ch: Option<u8>) -> Option<&mut Command> {
        match self.state {
            State::Start => None,

            State::Invalid => None,

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

            State::S55 => None,

            State::S3 | State::S30 | State::S300 => None,

            State::S3008 => parsers::context_signal::parse(self, terminator_ch),

            State::S6 => None,

            State::S66 => parsers::kitty_text_sizing::parse(self, terminator_ch),

            State::S72 => parsers::kitty_dnd_protocol::parse(self, terminator_ch),

            State::S77 => None,

            State::S99 => parsers::kitty_desktop_notification::parse(self, terminator_ch),

            State::S133 => parsers::semantic_prompt::parse(self, terminator_ch),

            State::S552 => None,

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
    use rustty_lib::allocator::{GlobalZigAllocator, ZigAllocatorRef};

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

    // test "Parser allocating captures have a hard limit"
    #[test]
    fn parser_allocating_captures_have_a_hard_limit() {
        let prefixes: [&str; 5] = ["52;", "66;", "72;", "99;", "5522;"];
        let limit = MAX_BUF + 1;

        for prefix in prefixes {
            let backend = GlobalZigAllocator;
            let alloc = ZigAllocatorRef::new(&backend);
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

        let backend = GlobalZigAllocator;
        let alloc = ZigAllocatorRef::new(&backend);
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
        let backend = GlobalZigAllocator;
        let alloc = ZigAllocatorRef::new(&backend);
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
            let backend = GlobalZigAllocator;
            let alloc = ZigAllocatorRef::new(&backend);
            let mut p: Parser = Parser::init(Some(alloc));
            p.next_slice(&input[0..split]);
            p.next_slice(&input[split..]);

            // const cmd = p.end(null).?.*;
            let cmd = p.end(None).unwrap().clone();
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
        let backend = GlobalZigAllocator;
        let alloc = ZigAllocatorRef::new(&backend);
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
