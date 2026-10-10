//! rustty-config::deep_equal
//!
//! PORTS: the `deepEqual` dispatch (`datastruct/comparison.zig`'s `deepEqual`)
//! as it applies to EVERY config value-type. In the reference, `Config.changed`
//! calls `deepEqual(field.type, old_value, new_value)` for the field's type,
//! which the comptime `deepEqual` resolves via `@typeInfo`:
//!
//!   * `@hasDecl(T, "equal")` types defer to their own `equal` (Color, ColorList,
//!     Palette, RepeatableString, SelectionWordChars, RepeatableFontVariation,
//!     Keybinds, RepeatableCodepointMap, RepeatableClipboardCodepointMap,
//!     RepeatableLink, RepeatableCommand, MouseScrollMultiplier, Duration,
//!     WindowPadding, RepeatableStringMap, Path, RepeatablePath, ReadableIO,
//!     RepeatableReadableIO);
//!   * `.@"enum"` / `.bool` / `.int` / `.float` compare with `==`;
//!   * `.@"struct"` recurse field-by-field (the packed-flag structs);
//!   * `.@"union"` (no `equal`) compare active tag then payload
//!     (Command, WorkingDirectory, TerminalColor, BoldColor, FontStyle,
//!     BackgroundBlur, QuickTerminalSize / its `Size`);
//!   * `.optional` is the `Option<T>` blanket impl in rustty-datastruct.
//!
//! Rust has no comptime type reflection, so each arm is an explicit `DeepEqual`
//! impl here. This is the prerequisite data half of the EQUAL/DIFF (CHANGED)
//! pass: `Config.changed` will dispatch per-`Key` to `deep_equal(&old.field,
//! &new.field)` exactly as the reference's `inline for` over the field does.
//!
//! SCOPE: every config-OWNED value type. Foreign leaf types used as config
//! fields (`MetricModifier`/`KeyRemapSet`/`CursorStyle`/`WindowPaddingBalance`/
//! `AlphaBlending`/`OptionAsAlt`/`ReleaseChannel`, and the Chunk-3a-hoisted
//! `WindowPaddingColor`/`WindowColorspace`/`BackgroundImagePosition`/
//! `BackgroundImageFit`/`FontShapingBreak`/`BackgroundBlur`) are defined in other
//! crates; the orphan rule requires their `DeepEqual` impls to live in those crates and
//! there when the CHANGED pass that needs them lands (see notes/DEFERRALS).

use rustty_datastruct::comparison::DeepEqual;

use crate::command::Command;
use crate::io::{ReadableIO, RepeatableReadableIO};
use crate::path::{Path, RepeatablePath};
use crate::repeatable_string_map::RepeatableStringMap;
use crate::value_types::*;

// ===========================================================================
// `.@"enum"` / `.bool` / `.int` / `.float` arm: `return old == new`.
//
// The reference's `inline .bool, .int, .float, .@"enum" => return old == new`
// arm. Every config enum is a plain tagged enum with no payload, so the
// field-by-field struct recursion the reference would otherwise use collapses
// to `PartialEq`. (Scalar `bool`/`u8`/`u32`/`usize`/`f32`/`f64` are covered by
// the `deep_equal_eq!` impls already in rustty-datastruct.)
// ===========================================================================

macro_rules! deep_equal_via_eq {
    ($($t:ty),* $(,)?) => {
        $(impl DeepEqual for $t {
            fn deep_equal(&self, other: &Self) -> bool {
                self == other
            }
        })*
    };
}

deep_equal_via_eq!(
    ConfirmCloseSurface,
    CustomShaderAnimation,
    NonNativeFullscreen,
    Fullscreen,
    WindowSubtitle,
    LinkPreviews,
    CopyOnSelect,
    RightClickAction,
    MiddleClickAction,
    ShellIntegration,
    OSCColorReportFormat,
    WindowTheme,
    MacWindowButtons,
    MacTitlebarStyle,
    MacTitlebarProxyIcon,
    MacHidden,
    MacAppIcon,
    MacAppIconFrame,
    MacShortcuts,
    GtkSingleInstance,
    GtkTabsLocation,
    GtkToolbarStyle,
    GtkTitlebarStyle,
    MouseShiftCapture,
    ClipboardAccess,
    WindowSaveState,
    WindowNewTabPosition,
    MacOSDockDropBehavior,
    WindowShowTabBar,
    DragHandle,
    ResizeOverlay,
    ResizeOverlayPosition,
    QuickTerminalPosition,
    QuickTerminalLayer,
    QuickTerminalScreen,
    QuickTerminalSpaceBehavior,
    QuickTerminalKeyboardInteractivity,
    GraphemeWidthMethod,
    LinuxCgroup,
    AsyncBackend,
    AutoUpdate,
    WindowDecoration,
    Scrollbar,
    NotifyOnCommandFinish,
);

// ===========================================================================
// `.@"struct"` arm (packed-flag structs, no `equal` decl): recurse
// field-by-field with `deepEqual`. All fields are `bool`, so field-by-field
// `deep_equal` is equivalent to `==`; we delegate to `PartialEq` to mirror the
// reference's per-field `bool == bool` exactly.
//
//   .@"struct" => |info| { inline for (info.fields) |f|
//       if (!deepEqual(f.type, @field(old, f.name), @field(new, f.name)))
//           return false; return true; }
// ===========================================================================

deep_equal_via_eq!(
    FontSyntheticStyle,
    ShellIntegrationFeatures,
    XtChecksumExtension,
    SplitPreserveZoom,
    AppNotifications,
    BellFeatures,
    FreetypeLoadFlags,
    ScrollToBottom,
    NotifyOnCommandFinishAction,
);

// ===========================================================================
// `@hasDecl(T, "equal")` arm: defer to the type's own `equal` (the reference's
//   if (@hasDecl(T, "equal")) return old.equal(new);
// fast path, checked before the `@typeInfo` switch).
// ===========================================================================

macro_rules! deep_equal_via_equal {
    ($($t:ty),* $(,)?) => {
        $(impl DeepEqual for $t {
            fn deep_equal(&self, other: &Self) -> bool {
                self.equal(other)
            }
        })*
    };
}

deep_equal_via_equal!(
    ColorList,
    Palette,
    RepeatableString,
    SelectionWordChars,
    RepeatableFontVariation,
    Keybinds,
    RepeatableCodepointMap,
    RepeatableClipboardCodepointMap,
    RepeatableLink,
    RepeatableCommand,
    MouseScrollMultiplier,
    RepeatableStringMap,
    Path,
    RepeatablePath,
    ReadableIO,
    RepeatableReadableIO,
);

// The `equal` decls on these `Copy` value-types take `self`/`other` by value
// (mirroring the reference's `fn equal(self: T, other: T)`), so they need a
// by-value forward rather than `self.equal(other)`.
macro_rules! deep_equal_via_equal_byval {
    ($($t:ty),* $(,)?) => {
        $(impl DeepEqual for $t {
            fn deep_equal(&self, other: &Self) -> bool {
                (*self).equal(*other)
            }
        })*
    };
}

deep_equal_via_equal_byval!(Color, Duration, WindowPadding);

// `Limit(T, default)` is a `.@"struct"` with a single `.int` field and no
// `equal` decl, so the reference's struct recursion collapses to `PartialEq`.
// It is generic, so it takes a blanket impl rather than a macro row.
impl<T: LimitInt + PartialEq, const DEFAULT: u128> DeepEqual for Limit<T, DEFAULT> {
    fn deep_equal(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

// ===========================================================================
// `.@"union"` arm (tagged unions WITHOUT an `equal` decl): the reference
//   const old_tag = std.meta.activeTag(old);
//   const new_tag = std.meta.activeTag(new);
//   if (old_tag != new_tag) return false;
//   switch (old) { inline else => |val, tag|
//       return deepEqual(..., val, @field(new, @tagName(tag))); }
// compares the active tag, then `deepEqual`s the active payload.
// ===========================================================================

// command: ?Command (Command = union(enum) { direct: []const [:0]const u8,
// shell: [:0]const u8 }). No `equal` decl in the reference.
impl DeepEqual for Command {
    fn deep_equal(&self, other: &Self) -> bool {
        match (self, other) {
            // direct: []const [:0]const u8 -> Vec<Vec<u8>>; the reference's
            // `[]const [:0]const u8` deepEqual arm compares len then elements.
            (Command::Direct(a), Command::Direct(b)) => {
                if a.len() != b.len() {
                    return false;
                }
                for (x, y) in a.iter().zip(b.iter()) {
                    if !rustty_datastruct::comparison::deep_equal::<[u8]>(x, y) {
                        return false;
                    }
                }
                true
            }
            // shell: [:0]const u8 -> Vec<u8>
            (Command::Shell(a), Command::Shell(b)) => {
                rustty_datastruct::comparison::deep_equal::<[u8]>(a, b)
            }
            _ => false,
        }
    }
}

// @"working-directory": ?WorkingDirectory
// (union(enum) { home, inherit, path: []const u8 }). No `equal` decl.
impl DeepEqual for WorkingDirectory {
    fn deep_equal(&self, other: &Self) -> bool {
        match (self, other) {
            (WorkingDirectory::Home, WorkingDirectory::Home) => true,
            (WorkingDirectory::Inherit, WorkingDirectory::Inherit) => true,
            (WorkingDirectory::Path(a), WorkingDirectory::Path(b)) => {
                rustty_datastruct::comparison::deep_equal::<[u8]>(a, b)
            }
            _ => false,
        }
    }
}

// selection-foreground / cursor-color / ... : ?TerminalColor
// (union(enum) { color: Color, cell-foreground, cell-background }). No `equal`.
impl DeepEqual for TerminalColor {
    fn deep_equal(&self, other: &Self) -> bool {
        match (self, other) {
            (TerminalColor::Color(a), TerminalColor::Color(b)) => a.deep_equal(b),
            (TerminalColor::CellForeground, TerminalColor::CellForeground) => true,
            (TerminalColor::CellBackground, TerminalColor::CellBackground) => true,
            _ => false,
        }
    }
}

// bold-color: ?BoldColor (union(enum) { color: Color, bright }). No `equal`.
impl DeepEqual for BoldColor {
    fn deep_equal(&self, other: &Self) -> bool {
        match (self, other) {
            (BoldColor::Color(a), BoldColor::Color(b)) => a.deep_equal(b),
            (BoldColor::Bright, BoldColor::Bright) => true,
            _ => false,
        }
    }
}

// font-style*: FontStyle (union(enum) { default, false, name: []const u8 }).
// No `equal` decl.
impl DeepEqual for FontStyle {
    fn deep_equal(&self, other: &Self) -> bool {
        match (self, other) {
            (FontStyle::Default, FontStyle::Default) => true,
            (FontStyle::False, FontStyle::False) => true,
            (FontStyle::Name(a), FontStyle::Name(b)) => {
                rustty_datastruct::comparison::deep_equal::<[u8]>(a, b)
            }
            _ => false,
        }
    }
}

// background-blur: BackgroundBlur
// (union(enum) { false, true, macos-glass-regular, macos-glass-clear,
//   radius: u8 }). No `equal` decl.
//
// PORT NOTE (Chunk 3a): `BackgroundBlur` is hoisted to rustty-render-types (the
// config->renderer cycle break); the orphan rule requires its `DeepEqual` impl (a
// datastruct trait on the now-foreign type) to live in that crate, so the `.@"union"`-
// arm impl moved there verbatim.

// quick-terminal-size: QuickTerminalSize
// (struct { primary: ?Size, secondary: ?Size }; no `equal` decl -> struct arm,
// recurse field-by-field; `Size` is the inner union arm).
impl DeepEqual for QuickTerminalSizeSize {
    fn deep_equal(&self, other: &Self) -> bool {
        match (self, other) {
            (QuickTerminalSizeSize::Percentage(a), QuickTerminalSizeSize::Percentage(b)) => {
                a.deep_equal(b)
            }
            (QuickTerminalSizeSize::Pixels(a), QuickTerminalSizeSize::Pixels(b)) => a.deep_equal(b),
            _ => false,
        }
    }
}

impl DeepEqual for QuickTerminalSize {
    fn deep_equal(&self, other: &Self) -> bool {
        // inline for (fields) -> deepEqual(?Size, primary), deepEqual(?Size, secondary)
        self.primary.deep_equal(&other.primary) && self.secondary.deep_equal(&other.secondary)
    }
}

// theme: ?Theme (struct { light: []const u8, dark: []const u8 }; no `equal`
// decl -> struct arm, recurse field-by-field).
impl DeepEqual for Theme {
    fn deep_equal(&self, other: &Self) -> bool {
        rustty_datastruct::comparison::deep_equal::<[u8]>(&self.light, &other.light)
            && rustty_datastruct::comparison::deep_equal::<[u8]>(&self.dark, &other.dark)
    }
}
