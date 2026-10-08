//! PORTS: ghostty src/terminal/c/terminal.zig — the effect callback
//! surface: the `Effects` struct and its trampolines, the C callback
//! fn-ptr typedefs, and the clipboard / desktop-notification /
//! progress-report / program-status / semantic-prompt / unknown-sequence /
//! mode-config / device-attributes extern structs they carry.

use super::*;

/// C callback state for terminal effects.
///
/// Most trampolines are always installed on the stream handler. They check
/// these fields and do nothing when the C callback is null.
///
/// A few trampolines are only installed while their C callback is set,
/// because the stream handler behaves differently when the Zig effect is
/// null:
///
/// - `unknown_sequence`: unknown sequences are skipped without being
///   captured.
/// - `clipboard_write`: Kitty clipboard writes fail up front instead of
///   collecting data for a write that can never finish.
/// - `clipboard_read`: clipboard reads are denied.
/// - `program_status`: the OSC 7501 support query gets no reply, so
///   programs know not to send reports.
// PORT NOTE (internal, NOT C-ABI): mirrors the reference `Effects` struct field
// for field. The fn-ptr typedefs and `CDeviceAttributes` ARE part of the C ABI
// (named below). `da_features_buf` is the scratch buffer the device-attributes
// trampoline fills (`[64]device_attributes.Primary.Feature`).
pub struct Effects {
    pub userdata: *mut c_void,
    pub write_pty: Option<WritePtyFn>,
    pub bell: Option<BellFn>,
    pub color_scheme: Option<ColorSchemeFn>,
    pub device_attributes_cb: Option<DeviceAttributesFn>,
    pub enquiry: Option<EnquiryFn>,
    pub xtversion: Option<XtversionFn>,
    pub title_changed: Option<TitleChangedFn>,
    pub pwd_changed: Option<PwdChangedFn>,
    pub size_cb: Option<SizeFn>,
    pub clipboard_write: Option<ClipboardWriteFn>,
    pub clipboard_read: Option<ClipboardReadFn>,
    pub desktop_notification: Option<DesktopNotificationFn>,
    pub progress_report: Option<ProgressReportFn>,
    pub program_status: Option<ProgramStatusFn>,
    pub semantic_prompt: Option<SemanticPromptFn>,
    pub reset: Option<ResetFn>,
    pub unknown_sequence: Option<UnknownSequenceFn>,
    pub render_hold: Option<RenderHoldFn>,

    /// Scratch buffer for DA1 feature codes. The device attributes
    /// trampoline converts C feature codes into this buffer and returns
    /// a slice pointing into it. Storing it here ensures the slice
    /// remains valid after the trampoline returns, since the caller
    /// (`reportDeviceAttributes`) reads it before any re-entrant call.
    pub da_features_buf: [rustty_vt::device_attributes::Feature; 64],
}

// PORT NOTE: the reference `Effects` defaults every callback to `null`, the
// `userdata` to `null`, and `da_features_buf` to `undefined` (it is always
// overwritten before being read). `TerminalWrapper { effects: Effects = .{} }`
// relies on those defaults; this is the parity analog (`undefined` -> an
// arbitrary `Feature` placeholder, never observed).
impl Default for Effects {
    fn default() -> Effects {
        Effects {
            userdata: std::ptr::null_mut(),
            write_pty: None,
            bell: None,
            color_scheme: None,
            device_attributes_cb: None,
            enquiry: None,
            xtversion: None,
            title_changed: None,
            pwd_changed: None,
            size_cb: None,
            clipboard_write: None,
            clipboard_read: None,
            desktop_notification: None,
            progress_report: None,
            program_status: None,
            semantic_prompt: None,
            reset: None,
            unknown_sequence: None,
            render_hold: None,
            da_features_buf: [rustty_vt::device_attributes::Feature::AnsiColor; 64],
        }
    }
}

/// C function pointer type for the write_pty callback.
// pub const WritePtyFn = *const fn (Terminal, ?*anyopaque, [*]const u8, usize) callconv(lib.calling_conv) void;
pub type WritePtyFn = extern "C" fn(Terminal, *mut c_void, *const u8, usize);

/// C function pointer type for the bell callback.
// pub const BellFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) void;
pub type BellFn = extern "C" fn(Terminal, *mut c_void);

/// C function pointer type for the color_scheme callback.
/// Returns true and fills out_scheme if a color scheme is available,
/// or returns false to silently ignore the query.
// pub const ColorSchemeFn = *const fn (Terminal, ?*anyopaque, *device_status.ColorScheme) callconv(lib.calling_conv) bool;
pub type ColorSchemeFn =
    extern "C" fn(Terminal, *mut c_void, *mut device_status::ColorScheme) -> bool;

/// C function pointer type for the enquiry callback.
/// Returns the response bytes. The memory must remain valid
/// until the callback returns.
// pub const EnquiryFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) lib.String;
pub type EnquiryFn = extern "C" fn(Terminal, *mut c_void) -> LibString;

/// C function pointer type for the xtversion callback.
/// Returns the version string (e.g. "ghostty 1.2.3"). The memory
/// must remain valid until the callback returns. An empty string
/// (len=0) causes the default "libghostty" to be reported.
// pub const XtversionFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) lib.String;
pub type XtversionFn = extern "C" fn(Terminal, *mut c_void) -> LibString;

/// C function pointer type for the title_changed callback.
// pub const TitleChangedFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) void;
pub type TitleChangedFn = extern "C" fn(Terminal, *mut c_void);

/// C function pointer type for the render_hold callback.
// pub const RenderHoldFn = *const fn (Terminal, ?*anyopaque, bool) callconv(lib.calling_conv) void;
pub type RenderHoldFn = extern "C" fn(Terminal, *mut c_void, bool);

/// C function pointer type for the pwd_changed callback.
// pub const PwdChangedFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) void;
pub type PwdChangedFn = extern "C" fn(Terminal, *mut c_void);

/// C function pointer type for the size callback. Used by XTWINOPS queries
/// and VT-driven mode 2048 enable reports. Returns true and fills out_size
/// if size is available, or false to suppress the XTWINOPS response or
/// mode 2048 report.
// pub const SizeFn = *const fn (Terminal, ?*anyopaque, *size_report.Size) callconv(lib.calling_conv) bool;
pub type SizeFn = extern "C" fn(Terminal, *mut c_void, *mut size_report::Size) -> bool;

/// C function pointer type for the device_attributes callback.
/// Returns true and fills out_attrs if attributes are available,
/// or returns false to silently ignore the query.
// pub const DeviceAttributesFn = *const fn (Terminal, ?*anyopaque, *CDeviceAttributes) callconv(lib.calling_conv) bool;
pub type DeviceAttributesFn = extern "C" fn(Terminal, *mut c_void, *mut CDeviceAttributes) -> bool;

/// A single MIME representation in a clipboard write.
///
/// C: GhosttyClipboardContent -> RusttyClipboardContent (R2)
// pub const ClipboardContent = extern struct { mime: lib.String, data: lib.String };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClipboardContent {
    pub mime: LibString,
    pub data: LibString,
}

/// A protocol-neutral request to replace or clear clipboard contents.
/// The embedder answers by calling `reply` with the request before the
/// callback returns.
///
/// C: GhosttyClipboardWrite -> RusttyClipboardWrite (R2)
// pub const ClipboardWrite = extern struct { size, location, contents,
//     contents_len, name, granted, can_remember, ctx, reply };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClipboardWrite {
    pub size: usize,
    pub location: clipboard::Location,
    /// Null when `contents_len` is 0 (an empty write clears the destination).
    pub contents: *const ClipboardContent,
    pub contents_len: usize,
    pub name: LibString,
    pub granted: bool,
    pub can_remember: bool,
    /// Terminal-owned reply state; opaque to the embedder.
    // PORT NOTE (R9 callback-ctx): points at a STACK `ClipboardWriteCtx`
    // inside the write trampoline's frame; valid only for the duration of
    // the C callback (LIFETIMES.tsv: borrowed-during-call). Never retained,
    // never written through.
    pub ctx: *const c_void,
    pub reply: ClipboardWriteReplyFn,
}

/// The reply to a clipboard write request.
///
/// C: GhosttyClipboardWriteReply -> RusttyClipboardWriteReply (R2)
// pub const ClipboardWriteReply = extern struct { size, result, remember };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClipboardWriteReply {
    pub size: usize,
    pub result: clipboard::WriteStatus,
    pub remember: bool,
}

/// C function pointer type for replying to a clipboard write.
///
/// C: GhosttyClipboardWriteReplyFn -> RusttyClipboardWriteReplyFn (R2)
// pub const ClipboardWriteReplyFn = *const fn (*const ClipboardWrite, *const ClipboardWriteReply) callconv(lib.calling_conv) void;
pub type ClipboardWriteReplyFn = extern "C" fn(*const ClipboardWrite, *const ClipboardWriteReply);

/// The reply to a clipboard read request.
///
/// C: GhosttyClipboardReadReply -> RusttyClipboardReadReply (R2)
// pub const ClipboardReadReply = extern struct { size, result, contents,
//     contents_len, available, available_len, remember };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClipboardReadReply {
    pub size: usize,
    pub result: clipboard::ReadStatus,
    pub contents: *const ClipboardContent,
    pub contents_len: usize,
    pub available: *const LibString,
    pub available_len: usize,
    pub remember: bool,
}

/// A synchronous request to read clipboard contents. The embedder answers
/// by calling `reply` with the request before the callback returns.
///
/// C: GhosttyClipboardRead -> RusttyClipboardRead (R2)
// pub const ClipboardRead = extern struct { size, location, mimes, mimes_len,
//     list, name, granted, can_remember, ctx, reply };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ClipboardRead {
    pub size: usize,
    pub location: clipboard::Location,
    pub mimes: *const LibString,
    pub mimes_len: usize,
    pub list: bool,
    pub name: LibString,
    pub granted: bool,
    pub can_remember: bool,
    /// Terminal-owned reply state; opaque to the embedder.
    // PORT NOTE (R9 callback-ctx): points at a STACK `ClipboardReadCtx`
    // inside the read trampoline's frame; valid only for the duration of
    // the C callback (LIFETIMES.tsv: borrowed-during-call).
    pub ctx: *const c_void,
    pub reply: ClipboardReadReplyFn,
}

/// C function pointer type for replying to a clipboard read.
///
/// C: GhosttyClipboardReadReplyFn -> RusttyClipboardReadReplyFn (R2)
// pub const ClipboardReadReplyFn = *const fn (*const ClipboardRead, *const ClipboardReadReply) callconv(lib.calling_conv) void;
pub type ClipboardReadReplyFn = extern "C" fn(*const ClipboardRead, *const ClipboardReadReply);

/// C function pointer type for the clipboard_write callback. The request
/// is borrowed for the callback duration and must be answered through
/// its reply function before the callback returns.
// pub const ClipboardWriteFn = *const fn (Terminal, ?*anyopaque, *const ClipboardWrite) callconv(lib.calling_conv) void;
//
// PORT NOTE (6959fd46c/7ae9b1113 end state): the callback no longer RETURNS a
// status — the write is answered through `request.reply` (an ABI BREAK of the
// pre-range typedef; header + typedef + trampoline + example land as one unit).
pub type ClipboardWriteFn = extern "C" fn(Terminal, *mut c_void, *const ClipboardWrite);

/// C function pointer type for the clipboard_read callback. The request
/// is borrowed for the callback duration and must be answered through
/// its reply function before the callback returns.
// pub const ClipboardReadFn = *const fn (Terminal, ?*anyopaque, *const ClipboardRead) callconv(lib.calling_conv) void;
pub type ClipboardReadFn = extern "C" fn(Terminal, *mut c_void, *const ClipboardRead);

/// A request to show a desktop notification (OSC 9 / OSC 777).
///
/// C: GhosttyTerminalDesktopNotification -> RusttyTerminalDesktopNotification (R2)
// pub const DesktopNotification = extern struct { size, title, body };
//
// PORT NOTE (byte-slice rule): `title`/`body` are `RusttyString`, i.e. a raw
// `(ptr, len)` pair over the stream's own parse buffer — BYTES, not text. The
// Rust side carries them as `&[u8]` end to end (`stream::ShowDesktopNotification`)
// and never converts to `String`: an OSC 9 payload is whatever the program
// wrote, and a lossy UTF-8 conversion would silently corrupt a non-UTF-8 title
// on the way to the embedder.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct DesktopNotification {
    pub size: usize,
    pub title: LibString,
    pub body: LibString,
}

/// C function pointer type for the desktop_notification callback. The request
/// and both strings are borrowed and only valid for the callback duration.
// pub const DesktopNotificationFn = *const fn (Terminal, ?*anyopaque, *const DesktopNotification) callconv(lib.calling_conv) void;
pub type DesktopNotificationFn = extern "C" fn(Terminal, *mut c_void, *const DesktopNotification);

/// C: GhosttyTerminalProgressState -> RusttyTerminalProgressState (R2) —
/// checked by the A5 enum-discriminant arm.
// pub const ProgressState = ... (the shared `osc.Command.ProgressReport.State`)
//
// PORT NOTE: `628adaf30`'s Zig `LibEnum` factory is structurally N/A here —
// rustty hand-authors each `#[repr(C)]` projection rather than generating it —
// but the ENUM that commit shares is `47d602c42`'s type, so it lands with the
// progress-report effect.
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgressState {
    Remove = 0,
    Set = 1,
    Error = 2,
    Indeterminate = 3,
    Pause = 4,
}

impl ProgressState {
    /// Project the pure-VT `osc::ProgressReportState` onto the C ABI enum.
    /// The discriminants are identical by construction (same key order).
    pub fn from_vt(s: rustty_vt::osc::ProgressReportState) -> ProgressState {
        use rustty_vt::osc::ProgressReportState as S;
        match s {
            S::Remove => ProgressState::Remove,
            S::Set => ProgressState::Set,
            S::Error => ProgressState::Error,
            S::Indeterminate => ProgressState::Indeterminate,
            S::Pause => ProgressState::Pause,
        }
    }
}

/// A progress report emitted by the running program (OSC 9;4).
///
/// C: GhosttyTerminalProgressReport -> RusttyTerminalProgressReport (R2)
// pub const ProgressReport = extern struct { size, state, progress };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProgressReport {
    pub size: usize,
    pub state: ProgressState,
    /// Progress percentage 0..=100, or `-1` when the report omitted it.
    pub progress: i8,
}

/// C function pointer type for the progress_report callback. The report is
/// borrowed and only valid for the callback duration.
// pub const ProgressReportFn = *const fn (Terminal, ?*anyopaque, *const ProgressReport) callconv(lib.calling_conv) void;
pub type ProgressReportFn = extern "C" fn(Terminal, *mut c_void, *const ProgressReport);

/// C: GhosttyProgramStatusState -> RusttyProgramStatusState (R2)
// pub const ProgramStatusState = osc.Command.ProgramStatus.State;
pub type ProgramStatusState = rustty_vt::osc::parsers::program_status::State;

/// What a blocked program needs from the user. This matches
/// `osc.Command.ProgramStatus.Kind` but adds `none` first, because C has
/// no optional enums and a zeroed value must mean "no kind".
///
/// C: GhosttyProgramStatusKind -> RusttyProgramStatusKind (R2)
// pub const ProgramStatusKind = lib.Enum(lib.target, &.{
//     "none",
//     "permission",
//     "question",
//     "auth",
// });
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramStatusKind {
    None = 0,
    Permission = 1,
    Question = 2,
    Auth = 3,
}

impl ProgramStatusKind {
    // inline else => |tag| @field(ProgramStatusKind, @tagName(tag)),
    #[inline]
    fn from_vt(kind: rustty_vt::osc::parsers::program_status::Kind) -> ProgramStatusKind {
        use rustty_vt::osc::parsers::program_status::Kind;
        match kind {
            Kind::Permission => ProgramStatusKind::Permission,
            Kind::Question => ProgramStatusKind::Question,
            Kind::Auth => ProgramStatusKind::Auth,
        }
    }
}

/// A program status report (OSC 7501). Absent text is an empty string,
/// an absent kind is `none`, and absent progress is -1. See the header
/// for the meaning of each field.
///
/// C: GhosttyTerminalProgramStatus -> RusttyTerminalProgramStatus (R2)
// pub const ProgramStatus = extern struct {
//     size: usize,
//     state: ProgramStatusState,
//     kind: ProgramStatusKind,
//     progress: i8,
//     id: lib.String,
//     app: lib.String,
//     title: lib.String,
//     message: lib.String,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProgramStatus {
    pub size: usize,
    pub state: ProgramStatusState,
    pub kind: ProgramStatusKind,
    pub progress: i8,
    pub id: LibString,
    pub app: LibString,
    pub title: LibString,
    pub message: LibString,
}

/// C function pointer type for the program_status callback. The report
/// and its strings are borrowed for the callback duration.
// pub const ProgramStatusFn = *const fn (Terminal, ?*anyopaque, *const ProgramStatus) callconv(lib.calling_conv) void;
pub type ProgramStatusFn = extern "C" fn(Terminal, *mut c_void, *const ProgramStatus);

/// C: GhosttySemanticPromptKind -> RusttySemanticPromptKind (R2)
// pub const SemanticPromptKind = Handler.SemanticPrompt.Kind;
pub type SemanticPromptKind = rustty_vt::stream_terminal::SemanticPromptKind;

/// C: GhosttySemanticPromptPromptKind -> RusttySemanticPromptPromptKind (R2)
// pub const SemanticPromptPromptKind = Handler.SemanticPrompt.PromptKind;
pub type SemanticPromptPromptKind = rustty_vt::stream_terminal::SemanticPromptPromptKind;

/// A shell integration event. See `Handler.SemanticPrompt`.
///
/// C: GhosttyTerminalSemanticPrompt -> RusttyTerminalSemanticPrompt (R2)
// pub const SemanticPrompt = extern struct {
//     size: usize,
//     kind: SemanticPromptKind,
//     prompt_kind: SemanticPromptPromptKind,
//     has_exit_code: bool,
//     exit_code: i32,
//     command: lib.String,
//     @"error": lib.String,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct SemanticPrompt {
    pub size: usize,
    pub kind: SemanticPromptKind,
    pub prompt_kind: SemanticPromptPromptKind,
    pub has_exit_code: bool,
    pub exit_code: i32,
    pub command: LibString,
    pub error: LibString,
}

/// C function pointer type for the semantic_prompt callback. The event
/// and its strings are borrowed for the callback duration.
// pub const SemanticPromptFn = *const fn (Terminal, ?*anyopaque, *const SemanticPrompt) callconv(lib.calling_conv) void;
pub type SemanticPromptFn = extern "C" fn(Terminal, *mut c_void, *const SemanticPrompt);

/// C function pointer type for the reset callback.
// pub const ResetFn = *const fn (Terminal, ?*anyopaque) callconv(lib.calling_conv) void;
pub type ResetFn = extern "C" fn(Terminal, *mut c_void);

/// A borrowed unsupported string sequence.
///
/// C: GhosttyTerminalUnknownStringSequence -> RusttyTerminalUnknownStringSequence (R2)
// pub const UnknownStringSequence = extern struct {
//     truncated: bool,
//     content: lib.String,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UnknownStringSequence {
    pub truncated: bool,
    pub content: LibString,
}

/// An OSC sequence whose number is not implemented.
///
/// C: GhosttyTerminalUnknownOscSequence -> RusttyTerminalUnknownOscSequence (R2)
// pub const UnknownOscSequence = osc.Command.Unknown.C;
pub type UnknownOscSequence = crate::osc::UnknownC;

/// C: GhosttyTerminalUnknownSequenceTag -> RusttyTerminalUnknownSequenceTag (R2)
/// — checked by the A5 enum-discriminant arm.
// pub const Tag = lib.Enum(lib.target, &.{ "apc", "osc" });
//
// PORT NOTE: the reference nests `Tag` inside `UnknownSequence`; Rust types
// cannot nest, so it lives at module scope (the vt `UnknownSequence` flattening
// precedent).
#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownSequenceTag {
    Apc = 0,
    Osc = 1,
}

/// C: GhosttyTerminalUnknownSequenceValue -> RusttyTerminalUnknownSequenceValue (R2)
// const c_union = lib.TaggedUnion(lib.target, @This(), .{ .padding = [16]u64 });
// pub const CValue = c_union.CValue;
//
// A future borrowed CSI payload may need parameter, separator, and
// intermediate arrays. Reserve 128 bytes so that representation and
// other structured sequence types can be added without an ABI break.
#[repr(C)]
#[derive(Clone, Copy)]
pub union UnknownSequenceValue {
    pub apc: UnknownStringSequence,
    pub osc: UnknownOscSequence,
    pub _padding: [u64; 16],
}

/// An unsupported terminal sequence reported to the C callback.
///
/// C: GhosttyTerminalUnknownSequence -> RusttyTerminalUnknownSequence (R2)
// pub const UnknownSequence = union(Tag) { apc: UnknownStringSequence, osc: UnknownOscSequence, ... };
// pub const C = c_union.C;
//
// PORT NOTE: the reference declares the tagged Zig union and derives the C
// projection through the `lib.TaggedUnion` factory; rustty hand-authors the
// projection (`{ tag, value }`) like `ScrollViewportC`. The value crosses the
// ABI outbound-only (by const pointer to the callback), so the tag field keeps
// its typed enum form. `UnknownSequence.cval` is the trampoline's constructor
// of this struct.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct UnknownSequence {
    pub tag: UnknownSequenceTag,
    pub value: UnknownSequenceValue,
}

// BUILD-failure gate (D2): pin the `[16]u64` padding size, the natural
// alignment, and the `c_int`-width tag. Drift here fails the build.
const _: () = assert!(size_of::<UnknownSequenceValue>() == size_of::<[u64; 16]>());
const _: () = assert!(align_of::<UnknownSequenceValue>() == 8);
const _: () = assert!(size_of::<UnknownSequenceTag>() == size_of::<c_int>());

/// C function pointer type for the unknown_sequence callback. The request
/// and its content are borrowed for the callback duration.
// pub const UnknownSequenceFn = *const fn (Terminal, ?*anyopaque, *const UnknownSequence.C) callconv(lib.calling_conv) void;
pub type UnknownSequenceFn = extern "C" fn(Terminal, *mut c_void, *const UnknownSequence);

/// A terminal mode and boolean value used for mode configuration.
///
/// C: GhosttyTerminalModeConfig -> RusttyTerminalModeConfig (R2)
// pub const ModeConfig = extern struct {
//     mode: modes.ModeTag.Backing,
//     value: bool,
// };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ModeConfig {
    pub mode: modes::ModeTagBacking,
    pub value: bool,
}

impl ModeConfig {
    // fn toMode(self: ModeConfig) ?modes.Mode {
    //     const tag: modes.ModeTag = @bitCast(self.mode);
    //     return modes.modeFromInt(tag.value, tag.ansi);
    // }
    pub(super) fn to_mode(self) -> Option<modes::Mode> {
        let tag = modes::ModeTag::from_backing(self.mode);
        modes::mode_from_int(tag.value, tag.ansi)
    }
}

/// C-compatible device attributes struct.
/// C: GhosttyDeviceAttributes -> RusttyDeviceAttributes (R2)
// pub const CDeviceAttributes = extern struct { primary, secondary, tertiary };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDeviceAttributes {
    pub primary: CDeviceAttributesPrimary,
    pub secondary: CDeviceAttributesSecondary,
    pub tertiary: CDeviceAttributesTertiary,
}

// PORT NOTE: the reference nests `Primary`/`Secondary`/`Tertiary` inside
// `CDeviceAttributes`. Rust can't nest a type in a struct value namespace, so
// they are hoisted to siblings with the identical `extern` layout.

// pub const Primary = extern struct { conformance_level: u16, features: [64]u16, num_features: usize };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDeviceAttributesPrimary {
    pub conformance_level: u16,
    pub features: [u16; 64],
    pub num_features: usize,
}

// pub const Secondary = extern struct { device_type: u16, firmware_version: u16, rom_cartridge: u16 };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDeviceAttributesSecondary {
    pub device_type: u16,
    pub firmware_version: u16,
    pub rom_cartridge: u16,
}

// pub const Tertiary = extern struct { unit_id: u32 };
#[repr(C)]
#[derive(Clone, Copy)]
pub struct CDeviceAttributesTertiary {
    pub unit_id: u32,
}

// PORT NOTE (D3 container_of recovery): the reference trampolines recover the
// owning `*TerminalWrapper` from the `*Handler` via two nested
// `@fieldParentPtr`s: `Handler -> Stream` (the handler is `stream.handler`) and
// `Stream -> TerminalWrapper` (the stream is `wrapper.stream`). rustty collapses
// this into a single byte-offset subtraction off the `&mut Handler` address.
// The offset is the SUM of the two field offsets (`TerminalWrapper.stream` and
// `Stream.handler`) — the cross-crate, generic `Stream<'static>` makes a single
// nested `offset_of!(TerminalWrapper, stream.handler)` path fragile, so we sum
// the two leaf offsets (each is a plain in-crate field access).
//
// SB SOUNDNESS: the recovered `*mut TerminalWrapper` is used ONLY to read its
// SIBLING fields (`effects`, `terminal`) through the raw pointer; we never form
// an overlapping `&mut TerminalWrapper` while the caller's `&mut Handler` is
// still live (the `Handler` lives INSIDE the wrapper, so a `&mut TerminalWrapper`
// would alias it). All sibling reads go through raw pointer projection.
//
// PROVENANCE: a pointer derived (by offset subtraction) from the caller's
// `&mut Handler` would, under Stacked Borrows, only carry provenance over the
// Handler's bytes — reading the SIBLING `effects`/`terminal` through it is UB.
// `new_` therefore `expose_provenance()`s the whole `TerminalWrapper` allocation
// once; here we recover the wrapper by computing its ADDRESS (pure integer math
// off the handler address — no pointer provenance involved) and reconstituting a
// pointer over the exposed allocation via `with_exposed_provenance_mut`. This is
// the same exposed-provenance bridge `hyperlink.rs` uses for its stored page
// pointers, and it gives the recovered pointer whole-allocation provenance so the
// sibling reads are SB-sound.
///
/// Fetches a `TerminalWrapper` reference from a `Handler`.
#[inline]
fn wrapper_from_handler(handler: *mut Handler<'static>) -> *mut TerminalWrapper {
    // const stream_ptr: *Stream = @fieldParentPtr("handler", handler);
    // const wrapper: *TerminalWrapper = @fieldParentPtr("stream", stream_ptr);
    let off = offset_of!(TerminalWrapper, stream) + offset_of!(Stream<'static>, handler);
    // Compute the wrapper base ADDRESS from the handler address (integer math
    // only), then reconstitute a pointer over the exposed wrapper allocation.
    let wrapper_addr = (handler as *mut u8).addr() - off;
    std::ptr::with_exposed_provenance_mut::<TerminalWrapper>(wrapper_addr)
}

// /// Opaque context behind ClipboardWrite.ctx for the reply trampoline.
// const ClipboardWriteCtx = struct {
//     write: clipboard.Write,
// };
//
// PORT NOTE (R9 callback-ctx): the reference nests this in `Effects`; a Rust
// `impl` cannot hold a struct declaration, so it sits at module scope.
// Stack-lifetime: `ClipboardWrite.ctx` points at this inside the trampoline
// frame and the C callback must call `reply` before returning (documented
// contract). Only ever read back as `*const`.
//
// PORT NOTE (soundness, parity S166-F1): `handler` is not in the reference.
// `clipboard::Write::reply` takes the handler the effect callback was given
// (see its note), and the C reply arrives through `ClipboardWrite.reply`
// with only this ctx, so the trampoline stores a pointer derived from its own
// `&mut Handler` argument here. The trampoline does not touch its `handler`
// again until the C callback has returned, so the reply's use of this
// pointer is a use of a child of that argument's tag.
struct ClipboardWriteCtx<'a> {
    write: clipboard::Write<'a>,
    handler: *mut Handler<'static>,
}

// /// Opaque context behind ClipboardRead.ctx for the reply trampoline.
// const ClipboardReadCtx = struct {
//     read: clipboard.Read,
//     wrapper: *TerminalWrapper,
// };
//
// PORT NOTE (R9 callback-ctx): stack-lifetime, like `ClipboardWriteCtx` above
// (and module-scoped for the same reason). `wrapper` is carried so the reply
// trampoline can allocate through `ctx.wrapper.terminal.gpa()` (rustty reads
// the same allocator through `handler.terminal`, so the field is a parity
// carry; parity tail S7-4); it is a raw pointer,
// never re-borrowed as `&mut` (the wrapper's stream-handler call chain is
// live on the same wrapper — the interleaved-alias shape of the
// `shaper-hook-sb-clean-interleave` memory).
//
// PORT NOTE (soundness, parity S166-F1): `handler` as in `ClipboardWriteCtx`.
struct ClipboardReadCtx<'a> {
    read: clipboard::Read<'a>,
    #[allow(dead_code)]
    wrapper: *mut TerminalWrapper,
    handler: *mut Handler<'static>,
}

impl Effects {
    // fn writePtyTrampoline(handler: *Handler, data: [:0]const u8) void {
    //     const stream_ptr: *Stream = @fieldParentPtr("handler", handler);
    //     const wrapper: *TerminalWrapper = @fieldParentPtr("stream", stream_ptr);
    //     const func = wrapper.effects.write_pty orelse return;
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, data.ptr, data.len);
    // }
    pub(crate) fn write_pty_trampoline(handler: &mut Handler<'static>, data: &[u8]) {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        // SAFETY: `wrapper` is the live owning `TerminalWrapper`; we read its
        // `effects` sibling (not the `&mut Handler` we were handed) via raw ptr.
        let func = match unsafe { (*wrapper).effects.write_pty } {
            Some(f) => f,
            None => return,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        let _live = LiveHandler::publish(wrapper, handler);
        func(wrapper, userdata, data.as_ptr(), data.len());
    }

    // fn bellTrampoline(handler: *Handler) void {
    //     ... const func = wrapper.effects.bell orelse return;
    //     func(@ptrCast(wrapper), wrapper.effects.userdata);
    // }
    pub(crate) fn bell_trampoline(handler: &mut Handler<'static>) {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.bell } {
            Some(f) => f,
            None => return,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        let _live = LiveHandler::publish(wrapper, handler);
        func(wrapper, userdata);
    }

    // fn colorSchemeTrampoline(handler: *Handler) ?device_status.ColorScheme {
    //     ... const func = wrapper.effects.color_scheme orelse return null;
    //     var scheme: device_status.ColorScheme = undefined;
    //     if (func(@ptrCast(wrapper), wrapper.effects.userdata, &scheme)) return scheme;
    //     return null;
    // }
    pub(crate) fn color_scheme_trampoline(
        handler: &mut Handler<'static>,
    ) -> Option<device_status::ColorScheme> {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let func = unsafe { (*wrapper).effects.color_scheme }?;
        let userdata = unsafe { (*wrapper).effects.userdata };
        // var scheme: device_status.ColorScheme = undefined;
        let mut scheme = core::mem::MaybeUninit::<device_status::ColorScheme>::uninit();
        let _live = LiveHandler::publish(wrapper, handler);
        if func(wrapper, userdata, scheme.as_mut_ptr()) {
            return Some(unsafe { scheme.assume_init() });
        }
        None
    }

    // fn deviceAttributesTrampoline(handler: *Handler) device_attributes.Attributes {
    //     ... const func = wrapper.effects.device_attributes_cb orelse return .{};
    //     var c_attrs: CDeviceAttributes = undefined;
    //     if (!func(@ptrCast(wrapper), wrapper.effects.userdata, &c_attrs)) return .{};
    //     const n: usize = @min(c_attrs.primary.num_features, 64);
    //     for (0..n) |i| wrapper.effects.da_features_buf[i] = @enumFromInt(c_attrs.primary.features[i]);
    //     return .{ .primary = ..., .secondary = ..., .tertiary = ... };
    // }
    pub(crate) fn device_attributes_trampoline(
        handler: &mut Handler<'static>,
    ) -> device_attributes::Attributes {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.device_attributes_cb } {
            Some(f) => f,
            None => return device_attributes::Attributes::default(),
        };
        let userdata = unsafe { (*wrapper).effects.userdata };

        // Get our attributes from the callback.
        // var c_attrs: CDeviceAttributes = undefined;
        let mut c_attrs = core::mem::MaybeUninit::<CDeviceAttributes>::uninit();
        let _live = LiveHandler::publish(wrapper, handler);
        if !func(wrapper, userdata, c_attrs.as_mut_ptr()) {
            return device_attributes::Attributes::default();
        }
        let c_attrs = unsafe { c_attrs.assume_init() };

        // Note below we use a lot of enumFromInt but its always safe
        // because all our types are non-exhaustive enums.

        // const n: usize = @min(c_attrs.primary.num_features, 64);
        let n: usize = core::cmp::min(c_attrs.primary.num_features, 64);
        // for (0..n) |i| wrapper.effects.da_features_buf[i] = @enumFromInt(c_attrs.primary.features[i]);
        for i in 0..n {
            // SAFETY: `da_features_buf` is a sibling field of the recovered
            // wrapper; mutate it via the raw ptr (never via the live
            // `&mut Handler`).
            unsafe {
                (*wrapper).effects.da_features_buf[i] =
                    feature_from_int(c_attrs.primary.features[i]);
            }
        }

        // PORT NOTE: the reference returns `features = da_features_buf[0..n]`, a
        // slice into the wrapper's long-lived scratch buffer. The Rust
        // `Primary.features` field is `&'static [Feature]`; the buffer lives as
        // long as the heap `TerminalWrapper`, and `reportDeviceAttributes` reads
        // the slice immediately (before any re-entrant call), so promoting the
        // borrow to `'static` here is sound (matches the reference's reliance on
        // the buffer outliving the trampoline return).
        let features: &'static [device_attributes::Feature] =
            unsafe { core::slice::from_raw_parts((*wrapper).effects.da_features_buf.as_ptr(), n) };

        device_attributes::Attributes {
            primary: device_attributes::Primary {
                conformance_level: conformance_level_from_int(c_attrs.primary.conformance_level),
                features,
            },
            secondary: device_attributes::Secondary {
                device_type: device_type_from_int(c_attrs.secondary.device_type),
                firmware_version: c_attrs.secondary.firmware_version,
                rom_cartridge: c_attrs.secondary.rom_cartridge,
            },
            tertiary: device_attributes::Tertiary {
                unit_id: c_attrs.tertiary.unit_id,
            },
        }
    }

    // fn enquiryTrampoline(handler: *Handler) []const u8 {
    //     ... const func = wrapper.effects.enquiry orelse return "";
    //     const result = func(@ptrCast(wrapper), wrapper.effects.userdata);
    //     if (result.len == 0) return "";
    //     return result.ptr[0..result.len];
    // }
    pub(crate) fn enquiry_trampoline(handler: &mut Handler<'static>) -> &'static [u8] {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.enquiry } {
            Some(f) => f,
            None => return b"",
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        let _live = LiveHandler::publish(wrapper, handler);
        let result = func(wrapper, userdata);
        if result.len == 0 {
            return b"";
        }
        // return result.ptr[0..result.len];
        // SAFETY: the callback contract is that `result.ptr[0..result.len]` is
        // valid for the lifetime of the call; the response is read immediately
        // by `reportEnquiry` before any re-entrant call.
        unsafe { core::slice::from_raw_parts(result.ptr, result.len) }
    }

    // fn xtversionTrampoline(handler: *Handler) []const u8 { ... identical shape ... }
    pub(crate) fn xtversion_trampoline(handler: &mut Handler<'static>) -> &'static [u8] {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.xtversion } {
            Some(f) => f,
            None => return b"",
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        let _live = LiveHandler::publish(wrapper, handler);
        let result = func(wrapper, userdata);
        if result.len == 0 {
            return b"";
        }
        unsafe { core::slice::from_raw_parts(result.ptr, result.len) }
    }

    // fn titleChangedTrampoline(handler: *Handler) void { ... }
    pub(crate) fn title_changed_trampoline(handler: &mut Handler<'static>) {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.title_changed } {
            Some(f) => f,
            None => return,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        let _live = LiveHandler::publish(wrapper, handler);
        func(wrapper, userdata);
    }

    // fn renderHoldTrampoline(handler: *Handler, held: bool) void {
    //     const wrapper = TerminalWrapper.fromHandler(handler);
    //     const func = wrapper.effects.render_hold orelse return;
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, held);
    // }
    //
    // PORT NOTE (SB, LIFETIMES.tsv `Effects render_hold`): the reference
    // takes a plain `*Handler`, and C `reset()` passes
    // `&wrapper.stream.handler`. The stream-handler effect slot needs the
    // `fn(&mut Handler, bool)` shape, so that entry forwards to
    // `render_hold_trampoline_raw`, which takes the handler as a RAW pointer.
    // C `reset` calls the raw entry with `addr_of_mut!((*w).stream.handler)`
    // and so never forms a `&mut Handler`: a callback that re-enters the
    // terminal (the documented `rustty_render_state_update`) cannot find a
    // protected handler borrow that C `reset` created.
    pub(crate) fn render_hold_trampoline(handler: &mut Handler<'static>, held: bool) {
        Self::render_hold_trampoline_raw(handler, held);
    }

    /// The body of `render_hold_trampoline` over a raw handler pointer. It
    /// only recovers the wrapper address from `handler` and never
    /// dereferences it.
    pub(crate) fn render_hold_trampoline_raw(handler: *mut Handler<'static>, held: bool) {
        let wrapper = wrapper_from_handler(handler);
        // SAFETY: `wrapper` is the live owning `TerminalWrapper`; read its
        // `effects` sibling through the raw pointer (never via the handler).
        let func = match unsafe { (*wrapper).effects.render_hold } {
            Some(f) => f,
            None => return,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        let _live = LiveHandler::publish(wrapper, handler);
        func(wrapper, userdata, held);
    }

    // fn pwdChangedTrampoline(handler: *Handler) void { ... }
    pub(crate) fn pwd_changed_trampoline(handler: &mut Handler<'static>) {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.pwd_changed } {
            Some(f) => f,
            None => return,
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        let _live = LiveHandler::publish(wrapper, handler);
        func(wrapper, userdata);
    }

    // fn sizeTrampoline(handler: *Handler) ?size_report.Size {
    //     ... const func = wrapper.effects.size_cb orelse return null;
    //     var s: size_report.Size = undefined;
    //     if (func(@ptrCast(wrapper), wrapper.effects.userdata, &s)) return s;
    //     return null;
    // }
    pub(crate) fn size_trampoline(handler: &mut Handler<'static>) -> Option<size_report::Size> {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let func = unsafe { (*wrapper).effects.size_cb }?;
        let userdata = unsafe { (*wrapper).effects.userdata };
        let mut s = core::mem::MaybeUninit::<size_report::Size>::uninit();
        let _live = LiveHandler::publish(wrapper, handler);
        if func(wrapper, userdata, s.as_mut_ptr()) {
            return Some(unsafe { s.assume_init() });
        }
        None
    }

    // fn clipboardWriteTrampoline(handler: *Handler, write: clipboard.Write) void {
    //     const wrapper = TerminalWrapper.fromHandler(handler);
    //     const func = wrapper.effects.clipboard_write orelse
    //         return write.reply(.unsupported);
    //     ...stack-first (<=4) content conversion, gpa fallback...
    //     const ctx: ClipboardWriteCtx = .{ .write = write };
    //     const request: ClipboardWrite = .{ ..., .ctx = &ctx, .reply = &clipboardWriteReplyTrampoline };
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, &request);
    // }
    pub(crate) fn clipboard_write_trampoline(
        handler: &mut Handler<'static>,
        write: clipboard::Write<'_>,
    ) {
        let wrapper = wrapper_from_handler(handler);
        let func = match unsafe { (*wrapper).effects.clipboard_write } {
            Some(f) => f,
            None => return write.reply(handler, clipboard::WriteResult::Unsupported),
        };
        let userdata = unsafe { (*wrapper).effects.userdata };

        // Most protocols currently produce one representation, so keep that
        // path allocation-free while supporting arbitrary multi-MIME writes.
        // (parity tail S7-4: past 4 the array is allocated through the
        // terminal's `gpa()`, fallibly: `catch return write.reply(.io_error)`.)
        // .mime = .init(content.mime), .data = .init(content.data)
        let to_c = |content: &clipboard::Content<'_>| ClipboardContent {
            mime: LibString::init(content.mime),
            data: LibString::init(content.data),
        };

        let mut stack_contents: [ClipboardContent; 4] = [ClipboardContent {
            mime: LibString {
                ptr: std::ptr::null(),
                len: 0,
            },
            data: LibString {
                ptr: std::ptr::null(),
                len: 0,
            },
        }; 4];
        let mut heap_contents: rustty_vt::alloc::AVec<ClipboardContent>;
        let contents: &[ClipboardContent] = if write.contents.len() <= stack_contents.len() {
            for (dst, content) in stack_contents.iter_mut().zip(write.contents) {
                *dst = to_c(content);
            }
            &stack_contents[..write.contents.len()]
        } else {
            // wrapper.terminal.gpa().alloc(ClipboardContent, write.contents.len)
            //     catch return write.reply(.io_error);
            // defer wrapper.terminal.gpa().free(contents);
            // SAFETY: `handler.terminal` is the live terminal; raw reads.
            let gpa = unsafe { (*(*handler.terminal).screens.active).alloc };
            heap_contents = rustty_vt::alloc::AVec::new_in(gpa);
            if heap_contents
                .try_reserve_exact(write.contents.len())
                .is_err()
            {
                return write.reply(handler, clipboard::WriteResult::IoError);
            }
            for content in write.contents {
                heap_contents.push(to_c(content));
            }
            &heap_contents
        };

        // const ctx: ClipboardWriteCtx = .{ .write = write };
        // (Derived last: nothing touches `handler` again until `func` returns.
        // N452: the same single raw pointer is the reply's handler and the
        // callback's `live_handler`.)
        let handler: *mut Handler<'static> = core::ptr::from_mut(handler);
        let ctx = ClipboardWriteCtx { write, handler };
        let request = ClipboardWrite {
            size: std::mem::size_of::<ClipboardWrite>(),
            location: write.location,
            contents: if contents.is_empty() {
                std::ptr::null()
            } else {
                contents.as_ptr()
            },
            contents_len: contents.len(),
            name: LibString::init(write.name),
            granted: write.granted,
            can_remember: write.can_remember,
            ctx: &ctx as *const ClipboardWriteCtx<'_> as *const c_void,
            reply: Self::clipboard_write_reply_trampoline,
        };
        let _live = LiveHandler::publish(wrapper, handler);
        func(wrapper, userdata, &request);
    }

    // fn clipboardWriteReplyTrampoline(
    //     request: *const ClipboardWrite,
    //     reply: *const ClipboardWriteReply,
    // ) callconv(lib.calling_conv) void { ... }
    //
    // # Safety (extern "C", DG5 abort-on-unwind): `request` is the borrowed
    // request whose `ctx` points at the live stack `ClipboardWriteCtx` (the C
    // callback contract: reply before returning); `reply` is borrowed for this
    // call only.
    extern "C" fn clipboard_write_reply_trampoline(
        request: *const ClipboardWrite,
        reply: *const ClipboardWriteReply,
    ) {
        use clipboard::WriteStatus;
        // const ctx: *const ClipboardWriteCtx = @ptrCast(@alignCast(request.ctx));
        // SAFETY: `request.ctx` is the trampoline frame's `ClipboardWriteCtx`,
        // still live because the callback has not returned (contract above).
        // Read-only access through `*const` — never a `&mut`.
        let ctx: *const ClipboardWriteCtx<'_> =
            unsafe { (*request).ctx as *const ClipboardWriteCtx<'_> };
        // const write = ctx.write;
        let write = unsafe { (*ctx).write };
        // SAFETY: `ctx.handler` was derived from the trampoline's live
        // `&mut Handler`, which is not used while the C callback runs (see
        // `ClipboardWriteCtx`); this is the only reference made from it.
        let handler = unsafe { &mut *(*ctx).handler };
        // switch (reply.result) { ... }
        //
        // (`.unsupported, _ =>` — an out-of-range C value is well-defined by
        // the open status type and maps to unsupported, like the reference's
        // catch-all arm.)
        match unsafe { (*reply).result } {
            WriteStatus::Success => write.reply(
                handler,
                clipboard::WriteResult::Success(clipboard::WriteResultSuccess {
                    remember: unsafe { (*reply).remember },
                }),
            ),
            WriteStatus::Denied => write.reply(handler, clipboard::WriteResult::Denied),
            WriteStatus::Busy => write.reply(handler, clipboard::WriteResult::Busy),
            WriteStatus::InvalidData => write.reply(handler, clipboard::WriteResult::InvalidData),
            WriteStatus::IoError => write.reply(handler, clipboard::WriteResult::IoError),
            _ => write.reply(handler, clipboard::WriteResult::Unsupported),
        }
    }

    // fn clipboardReadTrampoline(handler: *Handler, read: clipboard.Read) void {
    //     const wrapper = TerminalWrapper.fromHandler(handler);
    //     const func = wrapper.effects.clipboard_read orelse return;
    //     ...stackFallback(128) mime conversion...
    //     const ctx: ClipboardReadCtx = .{ .read = read, .wrapper = wrapper };
    //     const request: ClipboardRead = .{ ..., .ctx = &ctx, .reply = &clipboardReadReplyTrampoline };
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, &request);
    // }
    pub(crate) fn clipboard_read_trampoline(
        handler: &mut Handler<'static>,
        read: clipboard::Read<'_>,
    ) {
        let wrapper = wrapper_from_handler(handler);
        let Some(func) = (unsafe { (*wrapper).effects.clipboard_read }) else {
            return;
        };
        let userdata = unsafe { (*wrapper).effects.userdata };

        // Requests carry a handful of MIME types, so keep the common case
        // allocation-free. On OOM the request goes unanswered and the
        // handler replies with an empty clipboard.
        // var sfa = std.heap.stackFallback(128, wrapper.terminal.gpa());
        // const alloc = sfa.get();
        // const mimes = alloc.alloc(lib.String, read.mimes.len) catch {
        //     log.warn("out of memory converting clipboard read request", .{});
        //     return;
        // };
        // defer alloc.free(mimes);
        // (parity tail S7-4: `alloc::StackFallback` over the terminal's `gpa()`.)
        // SAFETY: `handler.terminal` is the live terminal; raw reads.
        let gpa = unsafe { (*(*handler.terminal).screens.active).alloc };
        let sfa = rustty_vt::alloc::StackFallback::<128>::new(gpa);
        let mut mimes: allocator_api2::vec::Vec<LibString, &rustty_vt::alloc::StackFallback<128>> =
            allocator_api2::vec::Vec::new_in(&sfa);
        if mimes.try_reserve_exact(read.mimes.len()).is_err() {
            log::warn!(target: LOG_SCOPE, "out of memory converting clipboard read request");
            return;
        }
        for m in read.mimes {
            mimes.push(LibString::init(m));
        }

        // const ctx: ClipboardReadCtx = .{ .read = read, .wrapper = wrapper };
        // (Derived last: nothing touches `handler` again until `func` returns.
        // N452: the same single raw pointer is the reply's handler and the
        // callback's `live_handler`.)
        let handler: *mut Handler<'static> = core::ptr::from_mut(handler);
        let ctx = ClipboardReadCtx {
            read,
            wrapper,
            handler,
        };
        let request = ClipboardRead {
            size: std::mem::size_of::<ClipboardRead>(),
            location: read.location,
            mimes: if mimes.is_empty() {
                std::ptr::null()
            } else {
                mimes.as_ptr()
            },
            mimes_len: mimes.len(),
            list: read.list,
            name: LibString::init(read.name),
            granted: read.granted,
            can_remember: read.can_remember,
            ctx: &ctx as *const ClipboardReadCtx<'_> as *const c_void,
            reply: Self::clipboard_read_reply_trampoline,
        };
        let _live = LiveHandler::publish(wrapper, handler);
        func(wrapper, userdata, &request);
    }

    // fn clipboardReadReplyTrampoline(
    //     request: *const ClipboardRead,
    //     reply: *const ClipboardReadReply,
    // ) callconv(lib.calling_conv) void { ... }
    //
    // # Safety (extern "C", DG5): `request.ctx` is the live stack
    // `ClipboardReadCtx`; `reply` and everything it points to are borrowed for
    // this call only.
    extern "C" fn clipboard_read_reply_trampoline(
        request: *const ClipboardRead,
        reply: *const ClipboardReadReply,
    ) {
        use clipboard::ReadStatus;
        // const ctx: *const ClipboardReadCtx = @ptrCast(@alignCast(request.ctx));
        // SAFETY: see `clipboard_write_reply_trampoline` — the ctx is the
        // trampoline frame's stack value, live until the callback returns.
        let ctx: *const ClipboardReadCtx<'_> =
            unsafe { (*request).ctx as *const ClipboardReadCtx<'_> };
        // const read = ctx.read;
        let read = unsafe { (*ctx).read };
        // SAFETY: as in `clipboard_write_reply_trampoline`.
        let handler = unsafe { &mut *(*ctx).handler };
        // switch (reply.result) {
        //     .success => {},
        //     .denied => return read.reply(.denied), ...
        //     .unsupported, _ => return read.reply(.unsupported),
        // }
        match unsafe { (*reply).result } {
            ReadStatus::Success => {}
            ReadStatus::Denied => return read.reply(handler, clipboard::ReadResult::Denied),
            ReadStatus::Busy => return read.reply(handler, clipboard::ReadResult::Busy),
            ReadStatus::IoError => return read.reply(handler, clipboard::ReadResult::IoError),
            _ => return read.reply(handler, clipboard::ReadResult::Unsupported),
        }

        // const c_contents: []const ClipboardContent = if (reply.contents) |ptr| ptr[0..reply.contents_len] else &.{};
        // SAFETY: the reply arrays are borrowed C memory, valid for this call
        // (caller contract).
        let c_contents: &[ClipboardContent] = if unsafe { (*reply).contents }.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts((*reply).contents, (*reply).contents_len) }
        };
        // const c_available: []const lib.String = if (reply.available) |ptr| ptr[0..reply.available_len] else &.{};
        let c_available: &[LibString] = if unsafe { (*reply).available }.is_null() {
            &[]
        } else {
            unsafe { std::slice::from_raw_parts((*reply).available, (*reply).available_len) }
        };

        // Most replies carry one representation, so keep that path
        // allocation-free while supporting arbitrary multi-MIME replies.
        // On OOM we don't reply and the handler answers with an empty
        // clipboard.
        // var sfa = std.heap.stackFallback(256, ctx.wrapper.terminal.gpa());
        // const alloc = sfa.get();
        // const contents = alloc.alloc(clipboard.Content, c_contents.len) catch {
        //     log.warn("out of memory converting clipboard read reply", .{});
        //     return;
        // };
        // defer alloc.free(contents);
        // ...
        // const available = alloc.alloc([]const u8, c_available.len) catch { ...same... };
        // defer alloc.free(available);
        // (parity tail S7-4: `alloc::StackFallback` over the terminal's
        // `gpa()`, read through the handler's raw terminal pointer: no wrapper
        // deref and no `&mut` alias of the live call chain.)
        // SAFETY: `handler.terminal` is the live terminal; raw reads.
        let gpa = unsafe { (*(*handler.terminal).screens.active).alloc };
        let sfa = rustty_vt::alloc::StackFallback::<256>::new(gpa);
        let mut contents: allocator_api2::vec::Vec<
            clipboard::Content<'_>,
            &rustty_vt::alloc::StackFallback<256>,
        > = allocator_api2::vec::Vec::new_in(&sfa);
        if contents.try_reserve_exact(c_contents.len()).is_err() {
            log::warn!(target: LOG_SCOPE, "out of memory converting clipboard read reply");
            return;
        }
        // SAFETY: each `LibString`/`ClipboardContent` names borrowed C memory
        // valid for this call.
        for c_content in c_contents {
            contents.push(clipboard::Content {
                mime: unsafe { std::slice::from_raw_parts(c_content.mime.ptr, c_content.mime.len) },
                data: unsafe { std::slice::from_raw_parts(c_content.data.ptr, c_content.data.len) },
            });
        }
        let mut available: allocator_api2::vec::Vec<&[u8], &rustty_vt::alloc::StackFallback<256>> =
            allocator_api2::vec::Vec::new_in(&sfa);
        if available.try_reserve_exact(c_available.len()).is_err() {
            log::warn!(target: LOG_SCOPE, "out of memory converting clipboard read reply");
            return;
        }
        for c_mime in c_available {
            available.push(unsafe { std::slice::from_raw_parts(c_mime.ptr, c_mime.len) });
        }

        // read.reply(.{ .success = .{ .contents = contents, .available = available, .remember = reply.remember } });
        read.reply(
            handler,
            clipboard::ReadResult::Success(clipboard::ReadResultSuccess {
                contents: &contents,
                available: &available,
                remember: unsafe { (*reply).remember },
            }),
        );
    }

    // fn desktopNotificationTrampoline(handler: *Handler, notification: Action.ShowDesktopNotification) void {
    //     const stream_ptr: *Stream = @fieldParentPtr("handler", handler);
    //     const wrapper: *TerminalWrapper = @fieldParentPtr("stream", stream_ptr);
    //     const func = wrapper.effects.desktop_notification orelse return;
    //     const request: DesktopNotification = .{ ... };
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, &request);
    // }
    pub(crate) fn desktop_notification_trampoline(
        handler: &mut Handler<'static>,
        notification: rustty_vt::stream::ShowDesktopNotification<'_>,
    ) {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let Some(func) = (unsafe { (*wrapper).effects.desktop_notification }) else {
            return;
        };
        let userdata = unsafe { (*wrapper).effects.userdata };

        // The `size` field is the sized-struct contract: the embedder may only
        // read fields present within it. It must be `size_of` the struct rustty
        // actually built, never a literal.
        let request = DesktopNotification {
            size: std::mem::size_of::<DesktopNotification>(),
            // .title = .init(notification.title), .body = .init(notification.body)
            title: LibString::init(notification.title),
            body: LibString::init(notification.body),
        };
        let _live = LiveHandler::publish(wrapper, handler);
        func(wrapper, userdata, &request);
    }

    // fn progressReportTrampoline(handler: *Handler, report: osc.Command.ProgressReport) void {
    //     ... same @fieldParentPtr recovery ...
    // }
    pub(crate) fn progress_report_trampoline(
        handler: &mut Handler<'static>,
        report: rustty_vt::osc::ProgressReport,
    ) {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let Some(func) = (unsafe { (*wrapper).effects.progress_report }) else {
            return;
        };
        let userdata = unsafe { (*wrapper).effects.userdata };

        // PORT NOTE: the `-1 == omitted` encoding and the 0..=100 clamp are
        // `ProgressReport.cval()` in rustty-vt (osc/mod.rs), already ported;
        // reuse it rather than re-deriving the encoding here.
        let cval = report.cval();
        let request = ProgressReport {
            size: std::mem::size_of::<ProgressReport>(),
            state: ProgressState::from_vt(report.state),
            progress: cval.progress,
        };
        let _live = LiveHandler::publish(wrapper, handler);
        func(wrapper, userdata, &request);
    }

    // fn programStatusTrampoline(
    //     handler: *Handler,
    //     report: osc.Command.ProgramStatus.Report,
    // ) void {
    //     const wrapper = TerminalWrapper.fromHandler(handler);
    //     const func = wrapper.effects.program_status orelse return;
    //
    //     // Both buffers hold the longest text the parser accepts, so
    //     // writing to them can't fail.
    //     var title_buf: [osc.program_status.max_title_bytes]u8 = undefined;
    //     var title: std.Io.Writer = .fixed(&title_buf);
    //     report.writeText(.title, &title) catch unreachable;
    //     var message_buf: [osc.program_status.max_msg_bytes]u8 = undefined;
    //     var message: std.Io.Writer = .fixed(&message_buf);
    //     report.writeText(.msg, &message) catch unreachable;
    //
    //     const c_report: ProgramStatus = .{
    //         .size = @sizeOf(ProgramStatus),
    //         .state = report.state,
    //         .kind = if (report.readOption(.kind)) |kind| switch (kind) {
    //             inline else => |tag| @field(ProgramStatusKind, @tagName(tag)),
    //         } else .none,
    //         .progress = if (report.readOption(.progress)) |value| @intCast(value) else -1,
    //         .id = .init(report.readOption(.id) orelse ""),
    //         .app = .init(report.readOption(.app) orelse ""),
    //         .title = .init(title.buffered()),
    //         .message = .init(message.buffered()),
    //     };
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, &c_report);
    // }
    //
    // PORT NOTE (SB, LIFETIMES.tsv `Effects program_status`): `id` and `app`
    // borrow the OSC parser's capture inside `wrapper.stream.parser`, not the
    // handler; title and message are this frame's buffers. The callback may
    // re-enter the terminal through `live_handler` (N452).
    pub(crate) fn program_status_trampoline(
        handler: &mut Handler<'static>,
        report: rustty_vt::osc::parsers::program_status::Report<'_>,
    ) {
        use rustty_vt::osc::parsers::program_status::{
            OptionKey, OptionValue, MAX_MSG_BYTES, MAX_TITLE_BYTES,
        };
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let Some(func) = (unsafe { (*wrapper).effects.program_status }) else {
            return;
        };
        let userdata = unsafe { (*wrapper).effects.userdata };

        // Both buffers hold the longest text the parser accepts, so
        // writing to them can't fail.
        // PORT NOTE: `std.Io.Writer.fixed(&buf)` is a `&mut [u8]` writer; what
        // it buffered is the prefix it advanced past.
        let mut title_buf = [0u8; MAX_TITLE_BYTES];
        let title_len = {
            let mut title: &mut [u8] = &mut title_buf;
            // report.writeText(.title, &title) catch unreachable;
            report
                .write_text(OptionKey::Title, &mut title)
                .unwrap_or_else(|_| unreachable!());
            MAX_TITLE_BYTES - title.len()
        };
        let mut message_buf = [0u8; MAX_MSG_BYTES];
        let message_len = {
            let mut message: &mut [u8] = &mut message_buf;
            // report.writeText(.msg, &message) catch unreachable;
            report
                .write_text(OptionKey::Msg, &mut message)
                .unwrap_or_else(|_| unreachable!());
            MAX_MSG_BYTES - message.len()
        };

        let str_option = |key: OptionKey| match report.read_option(key) {
            OptionValue::Str(Some(v)) => v,
            _ => b"".as_slice(),
        };
        let c_report = ProgramStatus {
            size: std::mem::size_of::<ProgramStatus>(),
            state: report.state,
            kind: match report.read_option(OptionKey::Kind) {
                OptionValue::Kind(Some(kind)) => ProgramStatusKind::from_vt(kind),
                _ => ProgramStatusKind::None,
            },
            progress: match report.read_option(OptionKey::Progress) {
                // @intCast(value): the parser only keeps 0..=100.
                OptionValue::Progress(Some(value)) => {
                    i8::try_from(value).expect("progress is at most 100")
                }
                _ => -1,
            },
            id: LibString::init(str_option(OptionKey::Id)),
            app: LibString::init(str_option(OptionKey::App)),
            title: LibString::init(&title_buf[..title_len]),
            message: LibString::init(&message_buf[..message_len]),
        };
        let _live = LiveHandler::publish(wrapper, handler);
        func(wrapper, userdata, &c_report);
    }

    // fn semanticPromptTrampoline(
    //     handler: *Handler,
    //     event: Handler.SemanticPrompt,
    // ) void {
    //     const wrapper = TerminalWrapper.fromHandler(handler);
    //     const func = wrapper.effects.semantic_prompt orelse return;
    //     const c_event: SemanticPrompt = .{
    //         .size = @sizeOf(SemanticPrompt),
    //         .kind = event.kind,
    //         .prompt_kind = event.prompt_kind,
    //         .has_exit_code = event.exit_code != null,
    //         .exit_code = event.exit_code orelse 0,
    //         .command = .init(event.command),
    //         .@"error" = .init(event.err),
    //     };
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, &c_event);
    // }
    //
    // PORT NOTE (SB, LIFETIMES.tsv `Effects semantic_prompt`): `error`
    // borrows the OSC parser's capture and `command` the handler's gpa
    // buffer, freed after the callback returns.
    pub(crate) fn semantic_prompt_trampoline(
        handler: &mut Handler<'static>,
        event: rustty_vt::stream_terminal::SemanticPrompt<'_>,
    ) {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let Some(func) = (unsafe { (*wrapper).effects.semantic_prompt }) else {
            return;
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        let c_event = SemanticPrompt {
            size: std::mem::size_of::<SemanticPrompt>(),
            kind: event.kind,
            prompt_kind: event.prompt_kind,
            has_exit_code: event.exit_code.is_some(),
            exit_code: event.exit_code.unwrap_or(0),
            command: LibString::init(event.command),
            error: LibString::init(event.err),
        };
        let _live = LiveHandler::publish(wrapper, handler);
        func(wrapper, userdata, &c_event);
    }

    // fn resetTrampoline(handler: *Handler) void {
    //     const wrapper = TerminalWrapper.fromHandler(handler);
    //     const func = wrapper.effects.reset orelse return;
    //     func(@ptrCast(wrapper), wrapper.effects.userdata);
    // }
    pub(crate) fn reset_trampoline(handler: &mut Handler<'static>) {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let Some(func) = (unsafe { (*wrapper).effects.reset }) else {
            return;
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        let _live = LiveHandler::publish(wrapper, handler);
        func(wrapper, userdata);
    }

    // fn unknownSequenceTrampoline(
    //     handler: *Handler,
    //     sequence: Handler.UnknownSequence,
    // ) void {
    //     const wrapper = TerminalWrapper.fromHandler(handler);
    //     const func = wrapper.effects.unknown_sequence orelse return;
    //     const value = UnknownSequence.cval(switch (sequence) {
    //         .apc => |apc_value| .{
    //             .apc = .{
    //                 .truncated = apc_value.truncated,
    //                 .content = .{
    //                     .ptr = apc_value.content.ptr,
    //                     .len = apc_value.content.len,
    //                 },
    //             },
    //         },
    //         .osc => |osc_value| .{ .osc = osc_value.cval() },
    //     });
    //     func(@ptrCast(wrapper), wrapper.effects.userdata, &value);
    // }
    //
    // PORT NOTE (SB, LIFETIMES.tsv `UnknownSequence osc arm`): the OSC
    // content borrows the OSC parser's buffer or capture inside
    // `wrapper.stream.parser`. A callback that sets UNKNOWN_MAX_BYTES writes
    // that parser's limit by raw place (options.rs), never through a `&mut`
    // that would retag the borrowed bytes.
    pub(crate) fn unknown_sequence_trampoline(
        handler: &mut Handler<'static>,
        sequence: rustty_vt::stream_terminal::UnknownSequence<'_>,
    ) {
        // N452: ONE raw pointer from the argument, which is not used
        // again (see `TerminalWrapper::live_handler`).
        let handler: *mut Handler<'static> = handler;
        let wrapper = wrapper_from_handler(handler);
        let Some(func) = (unsafe { (*wrapper).effects.unknown_sequence }) else {
            return;
        };
        let userdata = unsafe { (*wrapper).effects.userdata };
        // PORT NOTE: `UnknownSequence.cval(...)` — rustty hand-authors the
        // TaggedUnion C projection, so `cval` is this literal construction
        // (tag + payload zero-extended into the padded union).
        let value = match sequence {
            rustty_vt::stream_terminal::UnknownSequence::Apc(apc_value) => UnknownSequence {
                tag: UnknownSequenceTag::Apc,
                value: UnknownSequenceValue {
                    apc: UnknownStringSequence {
                        truncated: apc_value.truncated,
                        // .content = .init(apc_value.content)
                        content: LibString::init(apc_value.content),
                    },
                },
            },
            // .osc => |osc_value| .{ .osc = osc_value.cval() },
            rustty_vt::stream_terminal::UnknownSequence::Osc(osc_value) => UnknownSequence {
                tag: UnknownSequenceTag::Osc,
                value: UnknownSequenceValue {
                    osc: UnknownOscSequence::cval(osc_value),
                },
            },
        };
        let _live = LiveHandler::publish(wrapper, handler);
        func(wrapper, userdata, &value);
    }
}

// PORT NOTE: the reference uses `@enumFromInt` on the DA enums (always safe
// because they are non-exhaustive). rustty's ported enums carry an explicit
// catch-all variant (`Other(u16)`), so these helpers reproduce `@enumFromInt`:
// known codes map to their named variant, every other value rides the catch-all.
// They are NOT a rustty-vt change (R3) — purely the vt-c-local int->enum bridge.
fn feature_from_int(v: u16) -> device_attributes::Feature {
    use device_attributes::Feature;
    match v {
        1 => Feature::Columns132,
        2 => Feature::Printer,
        3 => Feature::Regis,
        4 => Feature::Sixel,
        6 => Feature::SelectiveErase,
        8 => Feature::UserDefinedKeys,
        9 => Feature::NationalReplacement,
        15 => Feature::TechnicalCharacters,
        16 => Feature::Locator,
        17 => Feature::TerminalState,
        18 => Feature::Windowing,
        21 => Feature::HorizontalScrolling,
        22 => Feature::AnsiColor,
        28 => Feature::RectangularEditing,
        29 => Feature::AnsiTextLocator,
        52 => Feature::Clipboard,
        other => Feature::Other(other),
    }
}

fn conformance_level_from_int(v: u16) -> device_attributes::ConformanceLevel {
    use device_attributes::ConformanceLevel;
    match v {
        1 => ConformanceLevel::VT100,
        4 => ConformanceLevel::VT132,
        6 => ConformanceLevel::VT102,
        7 => ConformanceLevel::VT131,
        12 => ConformanceLevel::VT125,
        62 => ConformanceLevel::Level2,
        63 => ConformanceLevel::Level3,
        64 => ConformanceLevel::Level4,
        65 => ConformanceLevel::Level5,
        other => ConformanceLevel::Other(other),
    }
}

fn device_type_from_int(v: u16) -> device_attributes::DeviceType {
    use device_attributes::DeviceType;
    match v {
        0 => DeviceType::VT100,
        1 => DeviceType::VT220,
        2 => DeviceType::VT240,
        18 => DeviceType::VT330,
        19 => DeviceType::VT340,
        24 => DeviceType::VT320,
        32 => DeviceType::VT382,
        41 => DeviceType::VT420,
        61 => DeviceType::VT510,
        64 => DeviceType::VT520,
        65 => DeviceType::VT525,
        other => DeviceType::Other(other),
    }
}
