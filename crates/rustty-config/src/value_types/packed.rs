//! PORTS: ghostty src/config/Config.zig — the in-file packed structs (bitfield
//! bools), their `PackedStruct` (PARSE) impls, the C-API bit projection for
//! `c_get`, and their `FormatEntry` impls.

use super::*;

// ===========================================================================
// Packed structs (bitfield bools; byte-faithful field set + defaults).
// ===========================================================================

// pub const FontSyntheticStyle = packed struct { bold, italic, "bold-italic" = true };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FontSyntheticStyle {
    pub bold: bool,
    pub italic: bool,
    pub bold_italic: bool,
}

impl Default for FontSyntheticStyle {
    fn default() -> Self {
        Self {
            bold: true,
            italic: true,
            bold_italic: true,
        }
    }
}

/// See "font-shaping-break" for documentation
// pub const FontShapingBreak = packed struct { cursor: bool = true };
//
// PORT NOTE (Chunk 3a A1 hoist — GPU_RD_CHUNK3A_DESIGN.md): named as a
// `renderer/generic.zig` `DerivedConfig` field type (`font_shaping_break`); the struct
// + its `Default` (`cursor: true`) + `DeepEqual` are hoisted to the rustty-render-types
// leaf (the `AlphaBlending` precedent) to break the config->renderer cycle. Re-exported
// here so `value_types::FontShapingBreak` and the field/defaults/Key use-sites keep
// resolving. The config-facing machinery stays here as config-local free fns / a
// parse newtype (`FontShapingBreakParse` / `font_shaping_break_{to_c_uint,format_entry}`
// below) because `PackedStruct` (rustty-args) / `FormatEntry` (rustty-cfg-fmt) impls on
// the now-foreign type would violate the orphan rule.
pub use rustty_render_types::FontShapingBreak;

// pub const ShellIntegrationFeatures = packed struct { cursor, sudo, title, "ssh-env", "ssh-terminfo", path };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShellIntegrationFeatures {
    pub cursor: bool,
    pub sudo: bool,
    pub title: bool,
    pub ssh_env: bool,
    pub ssh_terminfo: bool,
    pub path: bool,
}

impl Default for ShellIntegrationFeatures {
    fn default() -> Self {
        Self {
            cursor: true,
            sudo: false,
            title: true,
            ssh_env: false,
            ssh_terminfo: false,
            path: true,
        }
    }
}

// pub const SplitPreserveZoom = packed struct { navigation: bool = false };
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SplitPreserveZoom {
    pub navigation: bool,
}

// pub const AppNotifications = packed struct { "clipboard-copy", "config-reload" = true };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppNotifications {
    pub clipboard_copy: bool,
    pub config_reload: bool,
}

impl Default for AppNotifications {
    fn default() -> Self {
        Self {
            clipboard_copy: true,
            config_reload: true,
        }
    }
}

/// See bell-features
// pub const BellFeatures = packed struct { system, audio, attention, title, border };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BellFeatures {
    pub system: bool,
    pub audio: bool,
    pub attention: bool,
    pub title: bool,
    pub border: bool,
}

impl Default for BellFeatures {
    fn default() -> Self {
        Self {
            system: false,
            audio: false,
            attention: true,
            title: true,
            border: false,
        }
    }
}

/// See freetype-load-flags
// pub const FreetypeLoadFlags = packed struct { hinting, "force-autohint", monochrome, autohint, light };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FreetypeLoadFlags {
    // The defaults here at the time of writing this match the defaults
    // for Freetype itself. rustty hasn't made any opinionated changes
    // to these defaults. (Strictly speaking, `light` isn't FreeType's
    // own default, but appears to be the effective default with most
    // Fontconfig-aware software using FreeType, so until rustty
    // implements Fontconfig support we default to `light`.)
    pub hinting: bool,
    pub force_autohint: bool,
    pub monochrome: bool,
    pub autohint: bool,
    pub light: bool,
}

impl Default for FreetypeLoadFlags {
    fn default() -> Self {
        Self {
            hinting: true,
            force_autohint: false,
            monochrome: false,
            autohint: true,
            light: true,
        }
    }
}

// pub const ScrollToBottom = packed struct { keystroke = true, output = false; pub const default = .{} };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScrollToBottom {
    pub keystroke: bool,
    pub output: bool,
}

impl Default for ScrollToBottom {
    fn default() -> Self {
        Self {
            keystroke: true,
            output: false,
        }
    }
}

impl ScrollToBottom {
    // pub const default: ScrollToBottom = .{};
    pub fn default_const() -> ScrollToBottom {
        ScrollToBottom::default()
    }
}

/// See notify-on-command-finish
// pub const NotifyOnCommandFinishAction = packed struct { bell = true, notify = false };
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NotifyOnCommandFinishAction {
    pub bell: bool,
    pub notify: bool,
}

impl Default for NotifyOnCommandFinishAction {
    fn default() -> Self {
        Self {
            bell: true,
            notify: false,
        }
    }
}

// ===========================================================================
// PackedStruct impls (the PARSE `parsePackedStruct` contract for the bool-only
// packed structs). `set_all(b)` sets every field to `b` (the "true"/"false"
// whole-struct shorthand); `set_field(part, value)` sets one named field by its
// Zig field name (hyphenated for hyphenated fields), returning whether a field
// matched. These reproduce the reference's `inline for (info.fields) |field| {
// assert(field.type == bool); @field(result, field.name) = ... }`.
// ===========================================================================

// packed struct { bold, italic, "bold-italic" }
impl PackedStruct for FontSyntheticStyle {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"bold", b"italic", b"bold-italic"]
    }
    fn set_all(&mut self, b: bool) {
        self.bold = b;
        self.italic = b;
        self.bold_italic = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"bold" => self.bold = value,
            b"italic" => self.italic = value,
            b"bold-italic" => self.bold_italic = value,
            _ => return false,
        }
        true
    }
}

// packed struct { cursor }
//
// PORT NOTE (Chunk 3a): `FontShapingBreak` is hoisted to rustty-render-types, so
// `impl PackedStruct for FontShapingBreak` (a rustty-args trait on a now-foreign type)
// would violate the orphan rule. This config-local NEWTYPE carries the `PackedStruct`
// contract (a local type -> legal) so the single ported `parse_packed_struct` is reused
// verbatim; the parse handler unwraps `.0`. Pure cycle-break glue (R1 no-loss).
#[derive(Default)]
pub(crate) struct FontShapingBreakParse(pub FontShapingBreak);

impl PackedStruct for FontShapingBreakParse {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"cursor"]
    }
    fn set_all(&mut self, b: bool) {
        self.0.cursor = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"cursor" => self.0.cursor = value,
            _ => return false,
        }
        true
    }
}

// packed struct { cursor, sudo, title, "ssh-env", "ssh-terminfo", path }
impl PackedStruct for ShellIntegrationFeatures {
    fn field_names() -> &'static [&'static [u8]] {
        &[
            b"cursor",
            b"sudo",
            b"title",
            b"ssh-env",
            b"ssh-terminfo",
            b"path",
        ]
    }
    fn set_all(&mut self, b: bool) {
        self.cursor = b;
        self.sudo = b;
        self.title = b;
        self.ssh_env = b;
        self.ssh_terminfo = b;
        self.path = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"cursor" => self.cursor = value,
            b"sudo" => self.sudo = value,
            b"title" => self.title = value,
            b"ssh-env" => self.ssh_env = value,
            b"ssh-terminfo" => self.ssh_terminfo = value,
            b"path" => self.path = value,
            _ => return false,
        }
        true
    }
}

// packed struct { navigation }
impl PackedStruct for SplitPreserveZoom {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"navigation"]
    }
    fn set_all(&mut self, b: bool) {
        self.navigation = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"navigation" => self.navigation = value,
            _ => return false,
        }
        true
    }
}

// packed struct { "clipboard-copy", "config-reload" }
impl PackedStruct for AppNotifications {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"clipboard-copy", b"config-reload"]
    }
    fn set_all(&mut self, b: bool) {
        self.clipboard_copy = b;
        self.config_reload = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"clipboard-copy" => self.clipboard_copy = value,
            b"config-reload" => self.config_reload = value,
            _ => return false,
        }
        true
    }
}

// packed struct { system, audio, attention, title, border }
impl PackedStruct for BellFeatures {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"system", b"audio", b"attention", b"title", b"border"]
    }
    fn set_all(&mut self, b: bool) {
        self.system = b;
        self.audio = b;
        self.attention = b;
        self.title = b;
        self.border = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"system" => self.system = value,
            b"audio" => self.audio = value,
            b"attention" => self.attention = value,
            b"title" => self.title = value,
            b"border" => self.border = value,
            _ => return false,
        }
        true
    }
}

// packed struct { hinting, "force-autohint", monochrome, autohint, light }
impl PackedStruct for FreetypeLoadFlags {
    fn field_names() -> &'static [&'static [u8]] {
        &[
            b"hinting",
            b"force-autohint",
            b"monochrome",
            b"autohint",
            b"light",
        ]
    }
    fn set_all(&mut self, b: bool) {
        self.hinting = b;
        self.force_autohint = b;
        self.monochrome = b;
        self.autohint = b;
        self.light = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"hinting" => self.hinting = value,
            b"force-autohint" => self.force_autohint = value,
            b"monochrome" => self.monochrome = value,
            b"autohint" => self.autohint = value,
            b"light" => self.light = value,
            _ => return false,
        }
        true
    }
}

// packed struct { keystroke, output }
impl PackedStruct for ScrollToBottom {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"keystroke", b"output"]
    }
    fn set_all(&mut self, b: bool) {
        self.keystroke = b;
        self.output = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"keystroke" => self.keystroke = value,
            b"output" => self.output = value,
            _ => return false,
        }
        true
    }
}

// packed struct { bell, notify }
impl PackedStruct for NotifyOnCommandFinishAction {
    fn field_names() -> &'static [&'static [u8]] {
        &[b"bell", b"notify"]
    }
    fn set_all(&mut self, b: bool) {
        self.bell = b;
        self.notify = b;
    }
    fn set_field(&mut self, part: &[u8], value: bool) -> bool {
        match part {
            b"bell" => self.bell = value,
            b"notify" => self.notify = value,
            _ => return false,
        }
        true
    }
}

// ===========================================================================
// Packed-struct C-API bit projection (`c_get`'s `.@"struct"` packed arm).
//
// PORTS: `getValue`'s packed-struct branch — `ptr.* = @intCast(@as(Backing,
// @bitCast(value)))`. Zig `@bitCast` of a `packed struct` reads the fields
// LSB-first in declaration order into the backing integer. Rust structs are not
// bit-packed, so `to_c_uint` reconstructs that backing integer by OR-ing each
// `bool` field at its declaration-order bit (the same field order as `set_all`).
// All these packed structs have <= `c_uint` bits, so the size guard in the
// reference always passes.
// ===========================================================================
macro_rules! packed_to_c_uint {
    ($ty:ty { $($field:ident),+ $(,)? }) => {
        impl $ty {
            /// `@intCast(@as(Backing, @bitCast(self)))` — the packed struct's
            /// backing-integer bit pattern for the C API.
            pub fn to_c_uint(&self) -> std::os::raw::c_uint {
                let mut bits: std::os::raw::c_uint = 0;
                let mut shift: u32 = 0;
                $(
                    bits |= (self.$field as std::os::raw::c_uint) << shift;
                    shift += 1;
                )+
                let _ = shift;
                bits
            }
        }
    };
}

packed_to_c_uint!(FontSyntheticStyle {
    bold,
    italic,
    bold_italic
});
// PORT NOTE (Chunk 3a): `FontShapingBreak` is hoisted to rustty-render-types, so the
// `packed_to_c_uint!`-generated inherent method would orphan on the foreign type. It
// becomes this config-local free fn (the `@bitCast` packed-struct backing-integer
// projection for `c_get`'s `.@"struct"` packed arm); the `g_packed_fn!` C-API arm calls
// it. Same bit layout as `packed_to_c_uint!` (declaration-order OR of each `bool`).
pub(crate) fn font_shaping_break_to_c_uint(v: FontShapingBreak) -> std::os::raw::c_uint {
    let mut bits: std::os::raw::c_uint = 0;
    let mut shift: u32 = 0;
    bits |= (v.cursor as std::os::raw::c_uint) << shift;
    shift += 1;
    let _ = shift;
    bits
}
packed_to_c_uint!(ShellIntegrationFeatures {
    cursor,
    sudo,
    title,
    ssh_env,
    ssh_terminfo,
    path
});
packed_to_c_uint!(SplitPreserveZoom { navigation });
packed_to_c_uint!(AppNotifications {
    clipboard_copy,
    config_reload
});
packed_to_c_uint!(BellFeatures {
    system,
    audio,
    attention,
    title,
    border
});
packed_to_c_uint!(FreetypeLoadFlags {
    hinting,
    force_autohint,
    monochrome,
    autohint,
    light
});
packed_to_c_uint!(ScrollToBottom { keystroke, output });
packed_to_c_uint!(NotifyOnCommandFinishAction { bell, notify });

// ===========================================================================
// Packed-struct `FormatEntry` impls.
//
// PORT NOTE: the reference has NO `formatEntry` decl on these packed structs;
// they are formatted by the generic `formatter.formatEntry`'s `.@"packed"` arm
// (`format_entry_packed_struct`), which writes the fields comma-separated in
// DECLARATION ORDER with a `no-` prefix for each `false` field, using the Zig
// field-name spelling (hyphenated where hyphenated). Rust has no struct
// reflection, so each impl supplies that ordered `(name, bool)` list explicitly
// — exactly the `inline for (info.fields)` the reference walks. The names match
// the `PackedStruct::set_field` arms above (the parse side), keeping the two
// directions byte-symmetric.
// ===========================================================================

// packed struct { bold, italic, "bold-italic" }
impl FormatEntry for FontSyntheticStyle {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        format_entry_packed_struct(
            name,
            &[
                (b"bold", self.bold),
                (b"italic", self.italic),
                (b"bold-italic", self.bold_italic),
            ],
            writer,
        )
    }
}

// packed struct { cursor }
//
// PORT NOTE (Chunk 3a): `FontShapingBreak` is hoisted to rustty-render-types, so
// `impl FormatEntry for FontShapingBreak` (a rustty-cfg-fmt trait on a now-foreign
// type) would orphan. It becomes this config-local free fn; the `f_entry!(field, fn)`
// arm calls it. Body identical to the former `FormatEntry` impl.
pub(crate) fn font_shaping_break_format_entry<W: Writer>(
    v: FontShapingBreak,
    name: &[u8],
    writer: &mut W,
) -> Result<(), FormatError<W::Error>> {
    format_entry_packed_struct(name, &[(b"cursor", v.cursor)], writer)
}

// packed struct { cursor, sudo, title, "ssh-env", "ssh-terminfo", path }
impl FormatEntry for ShellIntegrationFeatures {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        format_entry_packed_struct(
            name,
            &[
                (b"cursor", self.cursor),
                (b"sudo", self.sudo),
                (b"title", self.title),
                (b"ssh-env", self.ssh_env),
                (b"ssh-terminfo", self.ssh_terminfo),
                (b"path", self.path),
            ],
            writer,
        )
    }
}

// packed struct { navigation }
impl FormatEntry for SplitPreserveZoom {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        format_entry_packed_struct(name, &[(b"navigation", self.navigation)], writer)
    }
}

// packed struct { "clipboard-copy", "config-reload" }
impl FormatEntry for AppNotifications {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        format_entry_packed_struct(
            name,
            &[
                (b"clipboard-copy", self.clipboard_copy),
                (b"config-reload", self.config_reload),
            ],
            writer,
        )
    }
}

// packed struct { system, audio, attention, title, border }
impl FormatEntry for BellFeatures {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        format_entry_packed_struct(
            name,
            &[
                (b"system", self.system),
                (b"audio", self.audio),
                (b"attention", self.attention),
                (b"title", self.title),
                (b"border", self.border),
            ],
            writer,
        )
    }
}

// packed struct { hinting, "force-autohint", monochrome, autohint, light }
impl FormatEntry for FreetypeLoadFlags {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        format_entry_packed_struct(
            name,
            &[
                (b"hinting", self.hinting),
                (b"force-autohint", self.force_autohint),
                (b"monochrome", self.monochrome),
                (b"autohint", self.autohint),
                (b"light", self.light),
            ],
            writer,
        )
    }
}

// packed struct { keystroke, output }
impl FormatEntry for ScrollToBottom {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        format_entry_packed_struct(
            name,
            &[(b"keystroke", self.keystroke), (b"output", self.output)],
            writer,
        )
    }
}

// packed struct { bell, notify }
impl FormatEntry for NotifyOnCommandFinishAction {
    fn format_entry<W: Writer>(
        &self,
        name: &[u8],
        writer: &mut W,
    ) -> Result<(), FormatError<W::Error>> {
        format_entry_packed_struct(
            name,
            &[(b"bell", self.bell), (b"notify", self.notify)],
            writer,
        )
    }
}
