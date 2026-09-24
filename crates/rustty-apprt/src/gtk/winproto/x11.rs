//! rustty-apprt::gtk::winproto::x11
//!
//! X11 window protocol implementation for the Rustty GTK apprt.
//!
//! PORTS: ghostty `src/apprt/gtk/winproto/x11.zig` (509 loc) — the X11
//! window-protocol backend. Two types: [`App`] (per-application: the xlib
//! `Display` GDK owns, plus the interned [`Atoms`]) and [`Window`] (per-surface:
//! the `X11Surface` XID plus the CSD/blur property machinery).
//!
//! Gated `#[cfg(rustty_gtk_x11)]` at the [`super`] module; pulls the `gdk4-x11`
//! crate (Cargo `x11` feature). `gdk4-x11` re-exports the `x11-dl` crate as
//! [`gdk4_x11::x11`]; its `xlib` module gives us the `Display`/`Atom`/`Window`
//! types (so they MATCH what GDK hands back) plus the `Xlib` dynamically-loaded
//! function table (`XChangeProperty`/`XGetWindowProperty`/`XDeleteProperty`/`XFree`).
//! Unlike the reference's static `@cImport` of `Xlib.h`, `x11-dl` loads `libX11`
//! at runtime — [`App::init`] opens the (process-cached) table once and shares it
//! into every [`Window`] via an `Rc`.
//!
//! # The X11Display -> xdisplay -> XID hinge
//!
//! GDK owns the real xlib `Display`. [`App::init`] downcasts the GDK display to
//! [`gdk4_x11::X11Display`] — ELSE returns `Ok(None)`, THE ORDERED-INIT FALLBACK
//! HINGE (`x11.zig:39-42`): under a non-X11 display this makes [`super::App::init`]
//! fall through to the noop arm. It then reaches the xlib `Display*` via
//! `unsafe { x11d.xdisplay() }` (`x11.zig:44`). A window's XID comes from
//! downcasting the GDK surface to [`gdk4_x11::X11Surface`] and calling `.xid()`
//! (`x11.zig:198-204`).
//!
//! # Ownership
//!
//! The xlib `Display` is GDK-OWNED — [`App::deinit`] does NOT `XCloseDisplay` it
//! (`x11.zig:111-113`). X11 window state is property-based (no per-surface proxy
//! objects to destroy, unlike Wayland), so [`Window::deinit`] only clears the
//! cached blur region (`x11.zig:214-216`).
//!
//! # DEFERRALS
//!
//! * **Quick terminal.** [`App::supports_quick_terminal`] warns and is `false`, and
//!   [`App::init_quick_terminal`] is an empty success — matching the reference
//!   (`x11.zig:172-177`; the quick terminal is a Wayland layer-shell feature).
//!   CORRECTION (task T-D, S60-F5): this said `init_quick_terminal` "returns
//!   unsupported".

use rustty_global::EnvMapExt as _;
use std::os::raw::{c_int, c_long, c_uchar, c_uint, c_ulong, c_void};
use std::rc::Rc;

use gdk4::prelude::*;
use gtk4::prelude::*;

use gdk4_x11::x11::xlib;

use rustty_config::value_types::{BackgroundBlur, WindowDecoration};
use rustty_config::Config as CoreConfig;

use super::blur_region::BlurRegion;
use super::WinprotoError;

/// The rustty `Window` GObject the winproto `Window::init` is realized against;
/// downcast from the generic `&impl IsA<gtk4::Window>` so the arm can read the
/// per-window config (`background-blur`, `window-decoration`) the reference reads
/// off its `*ApprtWindow` (`x11.zig:236-241,244,310`).
type ApprtWindow = crate::gtk::class::window::Window;

/// `_MOTIF_WM_HINTS` flag bit: decorations field is meaningful (`Xm/MwmUtil.h`;
/// the reference's `MotifWMHints.flags.decorations` bit, `x11.zig:493-495`).
const MWM_HINTS_DECORATIONS: c_ulong = 1 << 1;
/// `_MOTIF_WM_HINTS` decorations bit: enable ALL decorations (the reference's
/// `MotifWMHints.decorations.all` bit, `x11.zig:501-502`).
const MWM_DECOR_ALL: c_ulong = 1 << 0;

/// X11 property `format` value: 32-bit-logical items, stored as `c_long` words in
/// the client API (`x11.zig:471,477`). Both properties this arm writes
/// (`_MOTIF_WM_HINTS`, `_KDE_NET_WM_BLUR_BEHIND_REGION`) use format 32.
const FORMAT_32: c_int = 32;
// PORT NOTE: the reference's `PropertyFormat.bufferType` (`x11.zig:481-488`) has no
// counterpart: the port passes `*const c_uchar` built from `#[repr(C)]` `c_long`
// words, so the alignment it guards is the source type's. Its comment, verbatim:
// The buffer type has to be a multi-pointer to bytes
// *aligned to the element type* (very important,
// otherwise you'll read garbage!)
//
// I know this is really ugly. X11 is ugly. I consider it apropos.

/// `XkbUseCoreKbd` (`X11/XKBlib.h`, via `X11/extensions/XKB.h`) — the core-keyboard
/// device spec (`x11.zig:95`). `x11-dl` does not export it, so it is spelled here.
const XKB_USE_CORE_KBD: c_uint = 0x0100;
/// `XkbMajorVersion` (`X11/XKBlib.h`) — the XKB version rustty asks for
/// (`x11.zig:78`). Not exported by `x11-dl`.
const XKB_MAJOR_VERSION: c_int = 1;
/// `XkbMinorVersion` (`X11/XKBlib.h`) (`x11.zig:79`). Not exported by `x11-dl`.
const XKB_MINOR_VERSION: c_int = 0;
/// `QueuedAfterReading` (`X11/Xlib.h`) — the `XEventsQueued` mode the reference
/// uses (`x11.zig:137`). Not exported by `x11-dl`.
const QUEUED_AFTER_READING: c_int = 1;

/// `const log = std.log.scoped(.gtk_x11)` (`x11.zig:25`).
///
/// Carried as the `log::` facade's `target:` (`DECISION-logging.md` §6r4). The
/// pre-existing `eprintln!` prints spelled this `gtk-x11`, with a HYPHEN, which
/// is not the reference's `@tagName` — the scope tag is now exact.
const LOG_SCOPE: &str = "gtk_x11";

/// PORTS: the inferred error set of `x11.zig`'s `!App` / `!Window` returns
/// (`X11Error` + `GetWindowPropertyError`, `x11.zig:434-442`). Wrapped by
/// [`super::WinprotoError::X11`].
#[derive(Debug)]
pub enum Error {
    /// `error.NotX11Surface` (`x11.zig:199,204`).
    NotX11Surface,
    /// `x11-dl` could not load `libX11`. rustty-specific: the reference links Xlib
    /// statically via `@cImport`, so it has no analogue (there is nothing to load).
    XlibLoadFailed,
    /// `error.XkbInitializationError` (`x11.zig:89,101`) — `XkbQueryExtension` or
    /// `XkbSelectEventDetails` failed.
    XkbInitializationError,
    /// `error.RequestFailed` (`x11.zig:435`) — an Xlib call reported failure.
    RequestFailed,
    /// `error.PropertyTypeMismatch` (`x11.zig:384,440`).
    PropertyTypeMismatch,
    /// `error.PropertyFormatMismatch` (`x11.zig:385,441`).
    PropertyFormatMismatch,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::NotX11Surface => write!(f, "GTK window has no X11 surface"),
            Error::XlibLoadFailed => write!(f, "failed to load libX11 (x11-dl)"),
            Error::XkbInitializationError => {
                write!(f, "failed to initialize the X11 Xkb extension")
            }
            Error::RequestFailed => write!(f, "X11 request failed"),
            Error::PropertyTypeMismatch => write!(f, "X11 property type mismatch"),
            Error::PropertyFormatMismatch => write!(f, "X11 property format mismatch"),
        }
    }
}

impl std::error::Error for Error {}

/// `glib.setPrgname(name)` with the name's raw bytes: the reference passes its
/// `[:0]const u8` config value straight to `g_set_prgname`, which does not
/// require UTF-8. C reads up to the first NUL, as it does for the reference's
/// sentinel-terminated slice.
fn set_prgname_raw(name: &[u8]) {
    let end = name.iter().position(|&b| b == 0).unwrap_or(name.len());
    let c = std::ffi::CString::new(&name[..end]).expect("no interior NUL before `end`");
    // SAFETY: `c` is a valid NUL-terminated string; GLib copies it.
    unsafe { glib::ffi::g_set_prgname(c.as_ptr()) };
}

impl From<Error> for WinprotoError {
    fn from(e: Error) -> Self {
        WinprotoError::X11(e)
    }
}

/// PORTS: `x11.zig:444-460` `Atoms`. The interned X11 atoms the property writes
/// target. The reference interns exactly these two (`x11.zig:448-459`); everything
/// else it needs (`XA_CARDINAL`, the None atom) is a compile-time constant.
#[derive(Clone, Copy)]
struct Atoms {
    /// `_KDE_NET_WM_BLUR_BEHIND_REGION` (`x11.zig:450-453`).
    kde_blur: xlib::Atom,
    /// `_MOTIF_WM_HINTS` (`x11.zig:454-457`).
    motif_wm_hints: xlib::Atom,
}

impl Atoms {
    /// PORTS: `x11.zig:448-459` `Atoms.init`. Uses GDK's per-display atom cache
    /// (`gdk_x11_get_xatom_by_name_for_display`) exactly as the reference does,
    /// rather than a raw `XInternAtom`.
    fn init(display: &gdk4_x11::X11Display) -> Atoms {
        Atoms {
            kde_blur: gdk4_x11::x11_get_xatom_by_name_for_display(
                display,
                "_KDE_NET_WM_BLUR_BEHIND_REGION",
            ),
            motif_wm_hints: gdk4_x11::x11_get_xatom_by_name_for_display(display, "_MOTIF_WM_HINTS"),
        }
    }
}

// See Xm/MwmUtil.h, packaged with the Motif Window Manager
//
// We don't really care about the other flags
// (the reference's `_rest` padding of both `flags` and `decorations`).
/// PORTS: `x11.zig:491-509` `MotifWMHints`. See `Xm/MwmUtil.h`. `#[repr(C)]` of
/// five machine words; the reference expresses `flags`/`decorations` as `c_ulong`
/// packed bitfields, but the only bits this arm touches are
/// [`MWM_HINTS_DECORATIONS`] (in `flags`) and [`MWM_DECOR_ALL`] (in `decorations`),
/// so plain words + explicit bit ops are behavior-identical and avoid a bitfield
/// dependency. Format 32 => five `c_long`-width elements on the wire.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Default)]
struct MotifWmHints {
    flags: c_ulong,
    functions: c_ulong,
    decorations: c_ulong,
    input_mode: c_long,
    status: c_ulong,
}

/// PORTS: `x11.zig:27-178` `App`. Application-wide X11 state.
pub struct App {
    /// PORTS: `x11.zig:28` `display: *xlib.Display`. GDK owns it (`x11.zig:111-113`
    /// deinit is a no-op) — this is a BORROW, never `XCloseDisplay`d here.
    display: *mut xlib::Display,
    /// The `x11-dl` function table (`XChangeProperty`/`XGetWindowProperty`/
    /// `XDeleteProperty`/`XFree`). rustty-specific: the reference calls these
    /// through the static `@cImport`. `Rc` so each [`Window`] shares the one
    /// process-cached table without re-`open`ing.
    xlib: Rc<xlib::Xlib>,
    /// PORTS: `x11.zig:29` `base_event_code: c_int`. The XKB extension's base event
    /// code, returned by `XkbQueryExtension` — [`Self::event_mods`] compares a
    /// peeked event's `type` against it (`x11.zig:142`).
    base_event_code: c_int,
    /// PORTS: `x11.zig:30` `atoms: Atoms`.
    atoms: Atoms,
}

impl std::fmt::Debug for App {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("x11::App")
            .field("display", &self.display)
            .field("base_event_code", &self.base_event_code)
            .finish()
    }
}

impl App {
    /// PORTS: `x11.zig:32-109` `App.init`. Probe the GDK display for X11; if it is
    /// not X11, return `Ok(None)` — THE ORDERED-INIT FALLBACK HINGE
    /// (`x11.zig:39-42`). Otherwise reach the xlib `Display`, set the program class
    /// (WM_CLASS), load the `libX11` function table, initialize the XKB extension
    /// (`x11.zig:72-102`), and intern the atoms.
    pub fn init(
        gdk_display: &gdk4::Display,
        app_id: &str,
        config: &CoreConfig,
    ) -> Result<Option<App>, WinprotoError> {
        // If the display isn't X11, then we don't need to do anything.
        // `gobject.ext.cast(gdk_x11.X11Display, gdk_display) orelse return null`
        // (`x11.zig:39-42`).
        let Some(x11d) = gdk_display.downcast_ref::<gdk4_x11::X11Display>() else {
            return Ok(None);
        };

        // `gdk_x11_display.getXdisplay()` (`x11.zig:44`). `unsafe` per the gdk4-x11
        // binding; the pointer is a live `Display` GDK owns for its lifetime.
        let display = unsafe { x11d.xdisplay() };

        // const x11_program_name: [:0]const u8 = if (config.@"x11-instance-name") |pn|
        //     pn
        // else if (builtin.mode == .Debug)
        //     "ghostty-debug"
        // else
        //     "ghostty";
        let x11_program_name: &[u8] = match &config.x11_instance_name {
            Some(pn) => pn,
            None if cfg!(debug_assertions) => b"rustty-debug",
            None => b"rustty",
        };

        // Set the X11 window class property (WM_CLASS) if we are on an X11
        // display.
        //
        // Note that we also set the program name here using g_set_prgname.
        // This is how the instance name field for WM_CLASS is derived when
        // calling gdk_x11_display_set_program_class; there does not seem to be
        // a way to set it directly. It does not look like this is being set by
        // our other app initialization routines currently, but since we're
        // currently deriving its value from x11-instance-name effectively, I
        // feel like gating it behind an X11 check is better intent.
        //
        // This makes the property show up like so when using xprop:
        //
        //     WM_CLASS(STRING) = "rustty", "io.airely.rustty"
        //
        // Append "-debug" on both when using the debug build.
        //
        // CORRECTION (task T-D, S60-F8): the instance name went through
        // `String::from_utf8_lossy` and `glib::set_prgname(&str)`, so a non-UTF-8
        // `x11-instance-name` reached WM_CLASS with U+FFFD in place of its bytes,
        // and the reference's comment block above had been dropped. The reference
        // hands the raw `[:0]const u8` to `glib.setPrgname`; so does
        // [`set_prgname_raw`].
        set_prgname_raw(x11_program_name);
        // `gdk_x11.X11Display.setProgramClass(gdk_display, app_id)` (`x11.zig:70`).
        x11d.set_program_class(app_id);

        // Load libX11's function table (process-cached in x11-dl). rustty-specific:
        // stands in for the reference's static `@cImport` of Xlib.
        let xlib = xlib::Xlib::open().map_err(|_| Error::XlibLoadFailed)?;

        // XKB (`x11.zig:72-102`). `XkbQueryExtension` reports the extension's base
        // event code; `XkbSelectEventDetails` then asks the server to deliver
        // `XkbStateNotify` events whose MODIFIER-state bits changed, which is what
        // `event_mods` peeks for. Both are FATAL on failure in the reference.
        // log.debug("Xkb.init: initializing Xkb", .{});
        log::debug!(target: LOG_SCOPE, "Xkb.init: initializing Xkb");
        // log.debug("Xkb.init: running XkbQueryExtension", .{});
        log::debug!(target: LOG_SCOPE, "Xkb.init: running XkbQueryExtension");
        let mut opcode: c_int = 0;
        let mut base_event_code: c_int = 0;
        let mut base_error_code: c_int = 0;
        let mut major: c_int = XKB_MAJOR_VERSION;
        let mut minor: c_int = XKB_MINOR_VERSION;
        // `if (XkbQueryExtension(...) == 0) return error.XkbInitializationError`
        // (`x11.zig:80-90`).
        if unsafe {
            (xlib.XkbQueryExtension)(
                display,
                &raw mut opcode,
                &raw mut base_event_code,
                &raw mut base_error_code,
                &raw mut major,
                &raw mut minor,
            )
        } == 0
        {
            // log.err("Fatal: error initializing Xkb extension: error executing XkbQueryExtension", .{});
            log::error!(
                target: LOG_SCOPE,
                "Fatal: error initializing Xkb extension: error executing XkbQueryExtension"
            );
            return Err(Error::XkbInitializationError.into());
        }

        // log.debug("Xkb.init: running XkbSelectEventDetails", .{});
        log::debug!(target: LOG_SCOPE, "Xkb.init: running XkbSelectEventDetails");

        // `if (XkbSelectEventDetails(display, XkbUseCoreKbd, XkbStateNotify,
        // XkbModifierStateMask, XkbModifierStateMask) == 0) return
        // error.XkbInitializationError` (`x11.zig:93-102`).
        if unsafe {
            (xlib.XkbSelectEventDetails)(
                display,
                XKB_USE_CORE_KBD,
                xlib::XkbStateNotify as c_uint,
                xlib::XkbModifierStateMask,
                xlib::XkbModifierStateMask,
            )
        } == 0
        {
            // log.err("Fatal: error initializing Xkb extension: error executing XkbSelectEventDetails", .{});
            log::error!(
                target: LOG_SCOPE,
                "Fatal: error initializing Xkb extension: error executing XkbSelectEventDetails"
            );
            return Err(Error::XkbInitializationError.into());
        }

        Ok(Some(App {
            display,
            xlib: Rc::new(xlib),
            base_event_code,
            atoms: Atoms::init(x11d),
        }))
    }

    /// Test-observability seam (rustty-specific; the reference reads the field
    /// directly from the same module). Exposes the XKB base event code
    /// [`Self::init`] captured (`x11.zig:106`) so the live X11 KAT can prove
    /// `XkbQueryExtension` actually ran.
    #[doc(hidden)]
    #[must_use]
    pub fn xkb_base_event_code(&self) -> c_int {
        self.base_event_code
    }

    /// PORTS: `x11.zig:111-113` `App.deinit`. Nothing to release — the xlib
    /// `Display` is GDK-owned (NOT closed here), and X11 state is property-based
    /// (no proxies).
    #[allow(clippy::unused_self)]
    pub fn deinit(&mut self) {}

    /// Checks for an immediate pending XKB state update event, and returns the
    /// keyboard state based on if it finds any. This is necessary as the
    /// standard GTK X11 API (and X11 in general) does not include the current
    /// key pressed in any modifier state snapshot for that event (e.g. if the
    /// pressed key is a modifier, that is not necessarily reflected in the
    /// modifiers).
    ///
    /// Returns null if there is no event. In this case, the caller should fall
    /// back to the standard GDK modifier state (this likely means the key
    /// event did not result in a modifier change).
    ///
    /// PORTS: `x11.zig:115-160` `App.eventMods`. The caller
    /// ([`super::App::event_mods`]) falls back via `key::translate_mods`
    /// (`winproto.zig:62`).
    #[must_use]
    pub fn event_mods(
        &self,
        _device: Option<&gdk4::Device>,
        _gtk_mods: gdk4::ModifierType,
    ) -> Option<rustty_vt::input::key_mods::Mods> {
        // Shoutout to Mozilla for figuring out a clean way to do this, this is
        // paraphrased from Firefox/Gecko in widget/gtk/nsGtkKeyUtils.cpp.
        //
        // `if (XEventsQueued(display, QueuedAfterReading) == 0) return null`
        // (`x11.zig:135-138`). This guard is load-bearing: XPeekEvent BLOCKS when
        // the queue is empty.
        if unsafe { (self.xlib.XEventsQueued)(self.display, QUEUED_AFTER_READING) } == 0 {
            return None;
        }

        // `var nextEvent: XEvent = undefined; _ = XPeekEvent(display, &nextEvent)`
        // (`x11.zig:140-141`). PEEK, not next: the event stays queued for GDK.
        let mut next_event = std::mem::MaybeUninit::<xlib::XEvent>::uninit();
        unsafe { (self.xlib.XPeekEvent)(self.display, next_event.as_mut_ptr()) };
        let next_event = unsafe { next_event.assume_init() };

        // `if (nextEvent.type != self.base_event_code) return null` (`x11.zig:142`).
        if unsafe { next_event.type_ } != self.base_event_code {
            return None;
        }

        // `const xkb_event: *XkbEvent = @ptrCast(&nextEvent); if
        // (xkb_event.any.xkb_type != XkbStateNotify) return null` (`x11.zig:144-145`).
        // `xlib::XEvent` is a union whose `pad: [c_long; 24]` (192 bytes on LP64) is
        // the standard Xlib event buffer, which is >= every Xkb*Event — the same
        // reinterpretation the reference does with `@ptrCast`.
        let any: xlib::XkbAnyEvent = unsafe {
            std::ptr::read_unaligned(std::ptr::from_ref(&next_event).cast::<xlib::XkbAnyEvent>())
        };
        if any.xkb_type != xlib::XkbStateNotify {
            return None;
        }

        // `const xkb_state_notify_event: *XkbStateNotifyEvent = @ptrCast(xkb_event);
        // const lookup_mods = xkb_state_notify_event.lookup_mods` (`x11.zig:147-149`).
        let state: xlib::XkbStateNotifyEvent = unsafe {
            std::ptr::read_unaligned(
                std::ptr::from_ref(&next_event).cast::<xlib::XkbStateNotifyEvent>(),
            )
        };
        // Check the state according to XKB masks.
        let lookup_mods = c_uint::from(state.lookup_mods);

        // `log.debug("X11: found extra XkbStateNotify event w/lookup_mods: {b}")`
        // (`x11.zig:152`).
        log::debug!(
            target: LOG_SCOPE,
            "X11: found extra XkbStateNotify event w/lookup_mods: {lookup_mods:b}"
        );

        // `x11.zig:149,153-157` — the XKB mask -> `input.Mods` mapping.
        let mut mods = rustty_vt::input::key_mods::Mods::default();
        if lookup_mods & xlib::ShiftMask != 0 {
            mods.shift = true;
        }
        if lookup_mods & xlib::ControlMask != 0 {
            mods.ctrl = true;
        }
        if lookup_mods & xlib::Mod1Mask != 0 {
            mods.alt = true;
        }
        if lookup_mods & xlib::Mod4Mask != 0 {
            mods.super_ = true;
        }
        if lookup_mods & xlib::LockMask != 0 {
            mods.caps_lock = true;
        }

        Some(mods)
    }

    /// PORTS: `x11.zig:162-168` `App.bindGlobalShortcuts`. X11 has no native
    /// global-hotkey mechanism rustty drives (the reference's X11 arm returns
    /// `false` unconditionally), so global keybinds keep taking the XDG desktop
    /// portal path verbatim.
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn bind_global_shortcuts(
        &mut self,
        _shortcuts: &crate::gtk::class::global_shortcuts::GlobalShortcuts,
        _config: &CoreConfig,
    ) -> bool {
        false
    }

    /// PORTS: `x11.zig:170` `App.clearGlobalShortcuts`. Nothing was bound
    /// (the reference's empty body).
    #[allow(clippy::unused_self)]
    pub fn clear_global_shortcuts(&mut self) {}

    /// PORTS: `x11.zig:172-175` `App.supportsQuickTerminal`. The quick terminal is
    /// a Wayland layer-shell feature; the reference's X11 arm returns `false`.
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn supports_quick_terminal(&self) -> bool {
        // log.warn("quick terminal is not yet supported on X11", .{});
        log::warn!(target: LOG_SCOPE, "quick terminal is not yet supported on X11");
        false
    }

    /// PORTS: `x11.zig:177` `App.initQuickTerminal` —
    /// `pub fn initQuickTerminal(_: *App, _: *ApprtWindow) !void {}`: an empty
    /// SUCCESS.
    ///
    /// CORRECTION (task T-D, S60-F5): this returned
    /// `Err(Error::QuickTerminalUnsupported)` (a variant the reference's X11 error
    /// set does not have) under a doc saying "Unsupported on X11 — returns
    /// QuickTerminalUnsupported ... the reference's body is empty". The body is
    /// empty AND returns success; the variant is removed.
    #[allow(clippy::unused_self, clippy::unnecessary_wraps)]
    pub fn init_quick_terminal(
        &mut self,
        _window: &impl gtk4::prelude::IsA<gtk4::Window>,
    ) -> Result<(), WinprotoError> {
        Ok(())
    }
}

/// PORTS: `x11.zig:180-432` `Window`. Per-surface X11 state.
pub struct Window {
    /// PORTS: `x11.zig:182` `apprt_window: *ApprtWindow`. A WEAK ref (the window
    /// owns this winproto `Window`, so a strong ref is a GObject refcount cycle);
    /// upgraded at sync time to read config. Non-owning, like the reference's raw
    /// pointer.
    apprt_window: glib::WeakRef<ApprtWindow>,
    /// PORTS: `x11.zig:183` `x11_surface: *gdk_x11.X11Surface`. Cloned (bumps the
    /// GObject refcount) rather than borrowed, to avoid a lifetime tie to `App`;
    /// GDK owns the underlying surface, so `deinit` just drops this ref.
    x11_surface: gdk4_x11::X11Surface,

    /// Borrow of [`App::display`] (a Copy pointer; GDK-owned, never closed here).
    display: *mut xlib::Display,
    /// Shared clone of [`App::xlib`] — the libX11 function table.
    xlib: Rc<xlib::Xlib>,
    /// Copy of [`App::atoms`].
    atoms: Atoms,

    /// PORTS: `x11.zig:186` `blur_region`.
    blur_region: BlurRegion,
    // Cache last applied values to avoid redundant X11 property updates.
    // Redundant property updates seem to cause some visual glitches
    // with some window managers: https://github.com/ghostty-org/ghostty/pull/8075
    /// PORTS: `x11.zig:191` `last_applied_decoration_hints`.
    last_applied_decoration_hints: Option<MotifWmHints>,
}

impl std::fmt::Debug for Window {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("x11::Window")
            .field("xid", &self.x11_surface.xid())
            .finish()
    }
}

impl Window {
    /// PORTS: `x11.zig:193-212` `Window.init`.
    pub fn init(
        app: &App,
        window: &impl gtk4::prelude::IsA<gtk4::Window>,
    ) -> Result<Window, WinprotoError> {
        let gtk_window: &gtk4::Window = window.upcast_ref();
        // The concrete rustty window carries the config the reference reads off
        // `*ApprtWindow`. If the downcast fails, `apprt_window` upgrades to `None`
        // and the config-driven paths degrade gracefully.
        let apprt = gtk_window.downcast_ref::<ApprtWindow>();

        // `apprt_window.as(gtk.Native).getSurface() orelse return error.NotX11Surface`
        // (`x11.zig:198-199`).
        let Some(gdk_surface) = gtk_window.surface() else {
            return Err(Error::NotX11Surface.into());
        };
        // `gobject.ext.cast(gdk_x11.X11Surface, surface) orelse return error.NotX11Surface`
        // (`x11.zig:201-204`).
        let Some(x11_surface) = gdk_surface.downcast_ref::<gdk4_x11::X11Surface>() else {
            return Err(Error::NotX11Surface.into());
        };

        Ok(Window {
            apprt_window: apprt.map(ObjectExt::downgrade).unwrap_or_default(),
            x11_surface: x11_surface.clone(),
            display: app.display,
            xlib: app.xlib.clone(),
            atoms: app.atoms,
            blur_region: BlurRegion::empty(),
            last_applied_decoration_hints: None,
        })
    }

    /// PORTS: `x11.zig:214-216` `Window.deinit`. Clear the cached blur region; X11
    /// state is property-based so there are no proxies to destroy, and the xlib
    /// `Display`/`X11Surface` are GDK-owned.
    pub fn deinit(&mut self) {
        self.blur_region.clear();
    }

    /// PORTS: `x11.zig:218-223` `Window.resizeEvent`. The blur region must track
    /// window resizes.
    pub fn resize_event(&mut self) -> Result<(), WinprotoError> {
        // The blur region must update with window resizes
        // self.syncBlur() catch |err| { log.err("failed to sync blur={}", .{err}); };
        if let Err(err) = self.sync_blur() {
            log::error!(target: LOG_SCOPE, "failed to sync blur={err:?}");
        }
        Ok(())
    }

    /// PORTS: `x11.zig:225-234` `Window.syncAppearance`. Re-derive both the blur
    /// region (CSD toggles change its offset) and the decoration hints.
    pub fn sync_appearance(&mut self) -> Result<(), WinprotoError> {
        // The user could have toggled between CSDs and SSDs,
        // therefore we need to recalculate the blur region offset.
        // self.syncBlur() catch |err| { log.err("failed to sync blur={}", .{err}); };
        if let Err(err) = self.sync_blur() {
            log::error!(target: LOG_SCOPE, "failed to sync blur={err:?}");
        }
        // self.syncDecorations() catch |err| { log.err("failed to sync decorations={}", .{err}); };
        if let Err(err) = self.sync_decorations() {
            log::error!(target: LOG_SCOPE, "failed to sync decorations={err:?}");
        }
        Ok(())
    }

    /// PORTS: `x11.zig:236-241` `Window.clientSideDecorationEnabled`. Returns the
    /// ACTUAL present decoration intent (the noop contract).
    #[must_use]
    pub fn client_side_decoration_enabled(&self) -> bool {
        matches!(
            self.window_decoration(),
            WindowDecoration::Auto | WindowDecoration::Client
        )
    }

    /// PORTS: `x11.zig:342-344` `Window.setUrgent`. Drives urgency via GDK's
    /// `X11Surface::set_urgency_hint` (the WM_HINTS urgency flag) — the SAME
    /// mechanism the reference uses (`self.x11_surface.setUrgencyHint(...)`), NOT a
    /// `_NET_WM_STATE_DEMANDS_ATTENTION` client message.
    pub fn set_urgent(&mut self, urgent: bool) -> Result<(), WinprotoError> {
        self.x11_surface.set_urgency_hint(urgent);
        Ok(())
    }

    /// PORTS: `x11.zig:331-340` `Window.addSubprocessEnv`. Publishes the window's
    /// XID as `WINDOWID` — the long-standing X11 convention many terminal-aware
    /// tools read. NOT product-namespaced (R2): the reference spells it `WINDOWID`
    /// because that is the ecosystem's name, not Ghostty's.
    #[allow(clippy::unnecessary_wraps)]
    pub fn add_subprocess_env(
        &mut self,
        env: &mut rustty_global::EnvMap,
    ) -> Result<(), WinprotoError> {
        // `bufPrint("{}", .{xid})` — DECIMAL (`x11.zig:333-337`).
        env.put(b"WINDOWID", self.x11_surface.xid().to_string().as_bytes());
        Ok(())
    }

    /// PORTS: `x11.zig:243-287` `Window.syncBlur`. Sets/clears the KDE blur region
    /// property from the window's `background-blur` config.
    //
    // PORT NOTE (S60-F6): the reference is `fn syncBlur(self: *Window) !void` and
    // its two callers LOG the error. This returned `()` and discarded both
    // property-write results with `let _ =`, so the reference's `failed to sync
    // blur={}` diagnostics had no error value to carry and were dropped entirely.
    // The signature is the reference's again; the `?` sites below are exactly the
    // `try`s the reference has.
    fn sync_blur(&mut self) -> Result<(), Error> {
        let Some(apprt) = self.apprt_window.upgrade() else {
            return Ok(());
        };
        let Some(config) = apprt.get_config() else {
            return Ok(());
        };
        // When blur is disabled, remove the property if it was previously set
        let blur = config.get().background_blur;

        // `if (blur.enabled()) calcForWindow(..., true) else .empty` (`x11.zig:249-257`).
        // The X11 path requests DEVICE coordinates (`to_device_coordinates = true`).
        let region = if background_blur_enabled(blur) {
            let csd = self.client_side_decoration_enabled();
            let native: &gtk4::Native = apprt.upcast_ref();
            BlurRegion::calc_for_window(native, csd, true)
        } else {
            BlurRegion::empty()
        };

        // Only update X11 properties when the blur region actually changes
        if region.eql(&self.blur_region) {
            return Ok(());
        }

        if region.slices().is_empty() {
            // `deleteProperty(kde_blur)` (`x11.zig:282`).
            self.delete_property(self.atoms.kde_blur)?;
        } else {
            // log.debug("set blur={}, window xid={}, region={}", .{ blur, xid, region });
            log::debug!(
                target: LOG_SCOPE,
                "set blur={blur:?}, window xid={xid}, region={region:?}",
                xid = self.x11_surface.xid(),
            );
            // `changeProperty(Slice, kde_blur, XA_CARDINAL, ._32, .replace, slices)`
            // (`x11.zig:273-280`). Each `Slice` is four `c_long` words => nelements
            // = 4 * len. `Slice` is `#[repr(C)]` of `i64`(=`c_long`) so the slice
            // buffer is already a contiguous long array.
            let slices = region.slices();
            let nelements = (slices.len() * 4) as c_int;
            unsafe {
                self.change_property(
                    self.atoms.kde_blur,
                    xlib::XA_CARDINAL,
                    slices.as_ptr().cast::<c_uchar>(),
                    nelements,
                )
            }?;
        }

        self.blur_region = region;
        Ok(())
    }

    /// PORTS: `x11.zig:289-329` `Window.syncDecorations`. Read the existing
    /// `_MOTIF_WM_HINTS` (to preserve other bits), set the decorations flag + `all`
    /// bit from the window-decoration config, and write it back — skipping the
    /// write when the hints are unchanged.
    //
    // PORT NOTE (S60-F6): as with [`Self::sync_blur`], the reference is
    // `fn syncDecorations(self: *Window) !void` and `syncAppearance` LOGS its
    // error. This used to swallow the error and `return`, which left
    // `failed to sync decorations={}` with nothing to report.
    fn sync_decorations(&mut self) -> Result<(), Error> {
        // `getWindowProperty(...)` catching PropertyNotFound (`x11.zig:292-307`).
        let mut hints = match self.get_motif_hints() {
            Ok(Some(h)) => h,
            // motif_wm_hints is already initialized, so this is fine
            // PropertyNotFound => default (already-zeroed) hints (`x11.zig:300-301`).
            Ok(None) => MotifWmHints::default(),
            // RequestFailed / type / format mismatch: the reference PROPAGATES,
            // and so does this now — `syncAppearance` is where it is logged.
            Err(err) => return Err(err),
        };

        // `hints.flags.decorations = true` (`x11.zig:309`).
        hints.flags |= MWM_HINTS_DECORATIONS;
        // `hints.decorations.all = switch (decoration) { .server => true, else => false }`
        // (`x11.zig:310-313`).
        if matches!(self.window_decoration(), WindowDecoration::Server) {
            hints.decorations |= MWM_DECOR_ALL;
        } else {
            hints.decorations &= !MWM_DECOR_ALL;
        }

        // Only update decoration hints when they actually change
        if self.last_applied_decoration_hints == Some(hints) {
            return Ok(());
        }

        // `changeProperty(MotifWMHints, motif_wm_hints, motif_wm_hints, ._32, .replace, &.{hints})`
        // (`x11.zig:320-327`). One element of five `c_long` words => nelements = 5.
        // The reference computes it as `words_per_elem`:
        // The number of "words" that each element `T` occupies.
        let data = std::ptr::addr_of!(hints).cast::<c_uchar>();
        unsafe {
            self.change_property(
                self.atoms.motif_wm_hints,
                self.atoms.motif_wm_hints,
                data,
                5,
            )
        }?;
        self.last_applied_decoration_hints = Some(hints);
        Ok(())
    }

    /// The window-decoration intent, read live from the apprt window
    /// (`x11.zig:237,310`). Falls back to `.auto` if the weak ref is dead.
    fn window_decoration(&self) -> WindowDecoration {
        self.apprt_window
            .upgrade()
            .map_or(WindowDecoration::Auto, |w| w.get_window_decoration())
    }

    /// PORTS: `x11.zig:346-390` `Window.getWindowProperty`, specialized to
    /// [`MotifWmHints`] (format 32, five words). Returns `Ok(None)` for the
    /// reference's `error.PropertyNotFound` (the caller treats it as "use
    /// defaults"). The `XFree` of the returned buffer mirrors `x11.zig:389`.
    fn get_motif_hints(&self) -> Result<Option<MotifWmHints>, Error> {
        // FIXME: Maybe we should switch to libxcb one day.
        // Sounds like a much better idea than whatever this is
        let mut actual_type: xlib::Atom = 0;
        let mut actual_format: c_int = 0;
        let mut nitems: c_ulong = 0;
        let mut bytes_after: c_ulong = 0;
        let mut prop: *mut c_uchar = std::ptr::null_mut();

        // `XGetWindowProperty(display, xid, name, offset=0, length=maxInt, delete=0,
        // type, &actual_type, &actual_format, &nitems, &bytes_after, &prop)`
        // (`x11.zig:367-380`).
        let (code, trapped) = self.trap_x_errors(|| unsafe {
            (self.xlib.XGetWindowProperty)(
                self.display,
                self.x11_surface.xid(),
                self.atoms.motif_wm_hints,
                0,
                c_long::MAX,
                0,
                self.atoms.motif_wm_hints,
                &mut actual_type,
                &mut actual_format,
                &mut nitems,
                &mut bytes_after,
                &mut prop,
            )
        });
        // `if (code != Success) return error.RequestFailed` (`x11.zig:381`). A
        // trapped X error takes the same path (R13, [`Self::trap_x_errors`]).
        if code != c_int::from(xlib::Success) || trapped {
            // Xlib leaves `prop` null on a failed request; free defensively in case a
            // trapped error ever accompanies a `Success` reply.
            if !prop.is_null() {
                unsafe { (self.xlib.XFree)(prop.cast::<c_void>()) };
            }
            return Err(Error::RequestFailed);
        }
        // `if (actual_type == None) return error.PropertyNotFound` (`x11.zig:383`) —
        // the X11 `None` atom is 0. Modelled as `Ok(None)`. Nothing was allocated.
        if actual_type == 0 {
            return Ok(None);
        }

        // Validate, then free unconditionally (buffer WAS allocated).
        let result = (|| {
            // `if (typ != actual_type) return error.PropertyTypeMismatch` (`x11.zig:384`).
            if actual_type != self.atoms.motif_wm_hints {
                return Err(Error::PropertyTypeMismatch);
            }
            // `if (format != actual_format) return error.PropertyFormatMismatch` (`x11.zig:385`).
            if actual_format != FORMAT_32 {
                return Err(Error::PropertyFormatMismatch);
            }
            // rustty safety-hardening beyond the reference (which blindly casts the
            // buffer to `*MotifWMHints`): a well-formed hint is five 32-bit-logical
            // words; a shorter reply would under-run the read, so treat it as absent.
            if (nitems as usize) < 5 {
                return Ok(None);
            }
            // `const data_ptr: *T = @ptrCast(prop_return); result.* = data_ptr.*`
            // (`x11.zig:387-388`). `read_unaligned` is defensive; the format-32
            // buffer is in fact `c_long`-aligned.
            Ok(Some(unsafe {
                std::ptr::read_unaligned(prop.cast::<MotifWmHints>())
            }))
        })();

        // `_ = XFree(prop_return)` (`x11.zig:389`).
        if !prop.is_null() {
            unsafe { (self.xlib.XFree)(prop.cast::<c_void>()) };
        }
        result
    }

    /// Run one Xlib property call inside a GDK X error trap and report whether the
    /// server rejected it: `(call result, trapped)`.
    ///
    /// PORT NOTE: reference defect (R13) — `.port/parity-closure/UPSTREAM-DEFECTS.md`
    /// UD3. A deliberate divergence (task 20 fix round 2), not an oversight. The
    /// reference makes `XChangeProperty` / `XDeleteProperty` / `XGetWindowProperty`
    /// with NO error trap (`x11.zig:346-431`), so an X error on them reaches GDK's
    /// default handler, which prints "received an X Window System error" and EXITS
    /// the process with status 1. MEASURED on the VM (GTK 4.22.4, GDK_BACKEND=x11,
    /// headless sway's Xwayland): a second X client destroys our toplevel (what
    /// `xdotool windowclose` does), GDK logs "GdkSurface … unexpectedly destroyed"
    /// and keeps the window realized, and the next config reload runs
    /// `syncAppearance` into `BadWindow` and the terminal exits: request 20
    /// (`GetProperty`) with blur unchanged, request 19 (`DeleteProperty`) on a reload
    /// that turns blur off. Under R13 each call is bracketed by
    /// `gdk_x11_display_error_trap_push` / `_pop` — GDK's own scoped trap, never a
    /// process-wide `XSetErrorHandler` — held by an [`XErrorTrap`] guard so it is
    /// popped even if the call panics. A trapped error takes the reference's
    /// `error.RequestFailed` path, which `resizeEvent` / `syncAppearance` already log.
    /// For `XChangeProperty`/`XDeleteProperty` the trap is the ONLY failure signal
    /// (libX11 returns 1 from both unconditionally).
    ///
    /// NOT closed by this: after the same external destroy, GDK's OWN requests still
    /// exit the process — an immediate `maximize` (request 8, `MapWindow`) and a
    /// hide/show after pumping (request 18) — in a plain GTK 4.22.4 window too
    /// (`UPSTREAM-DEFECTS.md` UD56, upstream project gtk). The trap functions are
    /// deprecated since GTK 4.18, along with the rest of GDK's X11 backend API.
    ///
    /// Not added: a `gdk_surface_is_destroyed` check before the calls (task 20 round
    /// 4). It is not deprecated and reads `true` once GDK has processed the
    /// DestroyNotify, so it would avoid the measured pumped-reload case without a
    /// trap. It cannot cover a destroy GDK has not yet processed, or one that lands
    /// between the check and the request, and the trap covers every case the check
    /// would. R13 item 1 also argues against it: the trap takes the failure path
    /// the reference's own code implies (`error.RequestFailed`, then
    /// `resizeEvent` / `syncAppearance` log `failed to sync …`), whereas an early
    /// `Ok` on a destroyed window would invent a silent skip the reference has no
    /// counterpart for. (On a destroyed window the reference never logs those
    /// errors: its first request exits the process in GDK's error handler.) A
    /// secondary, test-side reason: the check would keep the M110 test's reloads
    /// from reaching the trap. The trap alone is the complete fix.
    fn trap_x_errors<T>(&self, call: impl FnOnce() -> T) -> (T, bool) {
        let display = self.x11_surface.display();
        let guard = XErrorTrap::push(display.downcast_ref::<gdk4_x11::X11Display>());
        let result = call();
        (result, guard.pop())
    }

    /// PORTS: `x11.zig:392-422` `Window.changeProperty`, concrete over a raw
    /// (format-32) byte buffer. `nelements` is the count of 32-bit-logical items
    /// (i.e. `c_long` words). Returns [`Error::RequestFailed`] when Xlib reports
    /// failure — note Xlib's status is `0 = failure` here (`x11.zig:418-421`).
    ///
    /// # Safety
    /// `data` must point to `nelements` `c_long`-width words of the property type.
    unsafe fn change_property(
        &self,
        name: xlib::Atom,
        typ: xlib::Atom,
        data: *const c_uchar,
        nelements: c_int,
    ) -> Result<(), Error> {
        let (status, trapped) = self.trap_x_errors(|| unsafe {
            (self.xlib.XChangeProperty)(
                self.display,
                self.x11_surface.xid(),
                name,
                typ,
                FORMAT_32,
                xlib::PropModeReplace,
                data,
                nelements,
            )
        });
        // For some godforsaken reason Xlib alternates between
        // error values (0 = success) and booleans (1 = success), and they look exactly
        // the same in the signature (just `int`, since Xlib is written in C89)...
        //
        // `if (status == 0) return error.RequestFailed` (`x11.zig:421`); a trapped
        // X error takes the same path (R13, [`Self::trap_x_errors`]).
        #[cfg(test)]
        if trapped {
            TRAPPED_WRITES.with(|n| n.set(n.get() + 1));
        }
        if status == 0 || trapped {
            Err(Error::RequestFailed)
        } else {
            Ok(())
        }
    }

    /// PORTS: `x11.zig:424-431` `Window.deleteProperty`.
    fn delete_property(&self, name: xlib::Atom) -> Result<(), Error> {
        let (status, trapped) = self.trap_x_errors(|| unsafe {
            (self.xlib.XDeleteProperty)(self.display, self.x11_surface.xid(), name)
        });
        // `if (status == 0) return error.RequestFailed` (`x11.zig:430`); a trapped
        // X error takes the same path (R13, [`Self::trap_x_errors`]).
        #[cfg(test)]
        if trapped {
            TRAPPED_WRITES.with(|n| n.set(n.get() + 1));
        }
        if status == 0 || trapped {
            Err(Error::RequestFailed)
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
thread_local! {
    /// Test-only: how many `XChangeProperty` / `XDeleteProperty` calls on this
    /// thread the X error trap reported as failed. The M110 test compares it with
    /// the `failed to sync blur=` records a reload logs, so a record that no
    /// trapped write produced cannot pass for one. Not compiled outside tests.
    static TRAPPED_WRITES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

/// A pushed GDK X error trap that is always popped.
///
/// [`Window::trap_x_errors`] pops it explicitly with `error_trap_pop` (which syncs and
/// returns the error code). If the traced call ever panicked instead, `Drop` pops it
/// with `error_trap_pop_ignored`: a trap left pushed would silently swallow every
/// later X error in the process — worse than the exit it exists to prevent.
struct XErrorTrap<'a> {
    display: Option<&'a gdk4_x11::X11Display>,
}

impl<'a> XErrorTrap<'a> {
    fn push(display: Option<&'a gdk4_x11::X11Display>) -> Self {
        if let Some(d) = display {
            d.error_trap_push();
        }
        Self { display }
    }

    /// Pop the trap: `true` when the server reported an X error inside it.
    /// `error_trap_pop` syncs, so an asynchronous error on the bracketed request is
    /// reported here rather than to GDK's default handler later.
    fn pop(mut self) -> bool {
        self.display.take().is_some_and(|d| d.error_trap_pop() != 0)
    }
}

impl Drop for XErrorTrap<'_> {
    fn drop(&mut self) {
        if let Some(d) = self.display.take() {
            d.error_trap_pop_ignored();
        }
    }
}

/// PORTS: `value_types.zig` `BackgroundBlur.enabled`. Replicated from the Wayland
/// arm (`wayland.rs`) since sync_blur needs it; macOS-glass variants imply blur.
fn background_blur_enabled(v: BackgroundBlur) -> bool {
    match v {
        BackgroundBlur::False => false,
        BackgroundBlur::True
        | BackgroundBlur::MacosGlassRegular
        | BackgroundBlur::MacosGlassClear => true,
        BackgroundBlur::Radius(r) => r > 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::gtk::winproto::blur_region::Slice;
    use gtk4::subclass::prelude::ObjectSubclassIsExt as _;

    /// An `XDeleteProperty` that reports failure. libX11's real one can never do
    /// this (it returns 1 unconditionally), which is exactly why the reference's
    /// status check needs a stub to be exercised at all.
    unsafe extern "C" fn failing_delete_property(
        _display: *mut xlib::Display,
        _window: c_ulong,
        _property: c_ulong,
    ) -> c_int {
        0
    }

    /// S60-F5: X11 `supportsQuickTerminal` warns "quick terminal is not yet
    /// supported on X11" and returns false; `initQuickTerminal` is an empty
    /// SUCCESS (`x11.zig:172-177`). Needs a live X server (headless sway's
    /// Xwayland on the VM); FAILS rather than returning without one.
    #[test]
    #[ignore = "requires an X server; run with GDK_BACKEND=x11 DISPLAY=... cargo test -p rustty-apprt --features gtk,x11,wayland --lib x11_quick_terminal -- --ignored"]
    fn x11_quick_terminal_warns_unsupported_and_init_succeeds() {
        crate::gtk::init_for_display_test("x11_quick_terminal_warns_unsupported_and_init_succeeds");
        let display = gdk4::Display::default().expect("a default GDK display");
        assert!(
            display.downcast_ref::<gdk4_x11::X11Display>().is_some(),
            "this test needs the X11 GDK backend (GDK_BACKEND=x11)"
        );
        let mut x11_app = App::init(
            &display,
            "io.airely.rustty.test",
            &CoreConfig::default_config(),
        )
        .expect("x11 App::init")
        .expect("the X11 arm");

        let cap = crate::log_capture::start();
        let supported = x11_app.supports_quick_terminal();
        let messages = cap.messages();
        drop(cap);
        assert!(!supported);
        assert_eq!(
            messages,
            [(
                log::Level::Warn,
                LOG_SCOPE.to_owned(),
                "quick terminal is not yet supported on X11".to_owned(),
            )]
        );

        let window = gtk4::Window::new();
        assert!(
            x11_app.init_quick_terminal(&window).is_ok(),
            "the reference's initQuickTerminal is an empty success"
        );
        window.destroy();
    }

    /// S60-F8: `x11-instance-name` reaches `g_set_prgname` as its raw bytes
    /// (`x11.zig:46-69`, `glib.setPrgname(x11_program_name)`), which is where
    /// GDK derives WM_CLASS's instance name from. A non-UTF-8 byte is kept, not
    /// replaced by U+FFFD.
    #[test]
    #[ignore = "requires an X server; run with GDK_BACKEND=x11 DISPLAY=... cargo test -p rustty-apprt --features gtk,x11,wayland --lib x11_instance_name -- --ignored"]
    fn x11_instance_name_reaches_g_set_prgname_raw() {
        crate::gtk::init_for_display_test("x11_instance_name_reaches_g_set_prgname_raw");
        let display = gdk4::Display::default().expect("a default GDK display");
        assert!(
            display.downcast_ref::<gdk4_x11::X11Display>().is_some(),
            "this test needs the X11 GDK backend (GDK_BACKEND=x11)"
        );
        let mut config = CoreConfig::default_config();
        config.x11_instance_name = Some(b"caf\xe9-rustty".to_vec());
        let _x11_app = App::init(&display, "io.airely.rustty.test", &config)
            .expect("x11 App::init")
            .expect("the X11 arm");

        // SAFETY: GLib returns its own NUL-terminated copy of the program name.
        let prgname = unsafe {
            let p = glib::ffi::g_get_prgname();
            assert!(!p.is_null(), "App::init set a program name");
            std::ffi::CStr::from_ptr(p).to_bytes().to_vec()
        };
        assert_eq!(prgname, b"caf\xe9-rustty");
    }

    /// S60-F1: `syncBlur` commits its new region to the cache only after the
    /// property write succeeded (`x11.zig:282-286`, `try` before
    /// `self.blur_region = region`). The window's cache is seeded non-empty and blur
    /// is off, so the sync must DELETE the property. With a failing
    /// `XDeleteProperty` the sync errors and the cache keeps the old region, so the
    /// next sync retries; with libX11's own function (the positive half, proving the
    /// delete path is the one being driven) it succeeds and the cache empties.
    ///
    /// Needs a live X server: run under `GDK_BACKEND=x11` with `DISPLAY` set (the
    /// VM uses headless sway's Xwayland). FAILS rather than returning without one.
    #[test]
    #[ignore = "requires an X server; run with GDK_BACKEND=x11 DISPLAY=... cargo test -p rustty-apprt --features gtk,x11,wayland --lib sync_blur_keeps -- --ignored"]
    fn sync_blur_keeps_the_cached_region_when_the_property_write_fails() {
        crate::gtk::init_for_display_test(
            "sync_blur_keeps_the_cached_region_when_the_property_write_fails",
        );
        let _ = libadwaita::init();
        crate::gtk::register_resources();

        let display = gdk4::Display::default().expect("a default GDK display");
        assert!(
            display.downcast_ref::<gdk4_x11::X11Display>().is_some(),
            "this test needs the X11 GDK backend (GDK_BACKEND=x11)"
        );
        let app = crate::gtk::class::application::Application::build_default_for_test();
        let win = crate::gtk::class::window::Window::new(&app, None);
        win.imp()
            .config
            .replace(Some(crate::gtk::class::config::Config::new(
                &CoreConfig::default_config(),
            )));
        WidgetExt::realize(&win);

        let x11_app = App::init(
            &display,
            "io.airely.rustty.test",
            &CoreConfig::default_config(),
        )
        .expect("x11 App::init")
        .expect("the X11 arm");
        let mut window = Window::init(&x11_app, &win).expect("x11 Window::init");
        assert!(!background_blur_enabled(
            win.get_config().expect("config").get().background_blur
        ));

        let seeded = || {
            BlurRegion::from_slices(vec![Slice {
                x: 0,
                y: 0,
                width: 10,
                height: 10,
            }])
        };
        window.blur_region = seeded();

        let mut failing = xlib::Xlib::open().expect("libX11");
        failing.XDeleteProperty = failing_delete_property;
        window.xlib = Rc::new(failing);
        assert!(matches!(window.sync_blur(), Err(Error::RequestFailed)));
        assert!(
            window.blur_region.eql(&seeded()),
            "a failed write must leave the cached region untouched"
        );

        window.xlib = x11_app.xlib.clone();
        assert!(window.sync_blur().is_ok());
        assert!(
            window.blur_region.slices().is_empty(),
            "a successful delete commits"
        );
    }

    /// M110 / UD3 (R13): an X error on a winproto property call must not exit the
    /// process; each reload must issue its property write, and every
    /// `failed to sync blur=` record it logs must come from a write the X error trap
    /// reported as failed.
    ///
    /// The scenario is the measured one: map one of our windows on X11 with blur on,
    /// let a SECOND X client destroy its toplevel (`xdotool windowclose`), pump the
    /// loop so GDK processes the DestroyNotify, then deliver two config reloads whose
    /// `syncAppearance` makes property requests against the dead window:
    /// - (a) blur OFF: `syncBlur` must DELETE the property;
    /// - (b) blur ON again, with the cached region first replaced by a sentinel so
    ///   the region differs: `syncBlur` must CHANGE the property.
    ///
    /// `XDeleteProperty`/`XChangeProperty` always return 1, so for both the trap is
    /// the ONLY failure signal. For each reload the child checks and reports:
    /// - how many `failed to sync blur=RequestFailed` records (`gtk_x11`, error)
    ///   the reload's synchronous `syncAppearance` logged. Zero means no write was
    ///   made (e.g. `syncBlur` skipped, or the region unchanged), or the write was
    ///   made and not reported;
    /// - (asserted in the child) that this count equals the number of
    ///   `XChangeProperty` / `XDeleteProperty` calls the X error trap reported as
    ///   failed during the reload (the test-only `TRAPPED_WRITES` counter,
    ///   incremented where `trap_x_errors` returns `trapped`). This proves the
    ///   write was ATTEMPTED: a `RequestFailed` returned without issuing the
    ///   request (e.g. an early return on `is_destroyed()`) logs the same record
    ///   but leaves the counter unchanged;
    /// - whether the window's cached blur region survived the reload unchanged,
    ///   which a write that was attempted and failed must leave it.
    ///
    /// Together these show each write was made against the dead window, was
    /// reported by the trap as failed, that failure is the one logged, and it did
    /// not exit the process. (Task 20 round 5: round 4 counted only the records,
    /// which its reviewer kept green with both traps removed and an early
    /// `Err(RequestFailed)` on `is_destroyed()`. Round 4: the round-3 test checked
    /// only the cache, which also survives when no write is made at all; the
    /// round-3 review kept it green with both traps removed by skipping the
    /// writes three ways. Round 3 itself corrected an earlier version that relied
    /// on a `failed to sync` line from `syncDecorations`' `XGetWindowProperty`,
    /// which that call produces from its own reply code.)
    ///
    /// GDK's default X error handler calls `exit(1)`, which no in-process assertion
    /// can observe, so the scenario runs in a CHILD copy of this test binary. The
    /// parent asserts the child exited 0 and reported one failed write per reload
    /// with the cache kept. Without a display the child fails
    /// (`init_for_display_test`), so this test fails too.
    #[test]
    #[ignore = "requires an X server; run with GDK_BACKEND=x11 DISPLAY=... cargo test -p rustty-apprt --features gtk,x11,wayland --lib x11_error_on_a_destroyed -- --ignored"]
    fn x11_error_on_a_destroyed_window_does_not_exit_the_process() {
        const CHILD: &str = "RUSTTY_M110_CHILD";
        const MARKER: &str = "M110_CHILD_RETURNED";
        if std::env::var_os(CHILD).is_some() {
            let [delete, change] = destroyed_window_config_reload_scenario();
            println!(
                "{MARKER} delete: failed_writes_logged={} cache_kept={}; \
                 change: failed_writes_logged={} cache_kept={}",
                delete.failed_writes_logged,
                delete.cache_kept,
                change.failed_writes_logged,
                change.cache_kept
            );
            return;
        }

        let name = format!(
            "{}::x11_error_on_a_destroyed_window_does_not_exit_the_process",
            module_path!()
                .split_once("::")
                .map_or(module_path!(), |(_, rest)| rest)
        );
        let out = std::process::Command::new(std::env::current_exe().expect("current_exe"))
            .args(["--exact", &name, "--ignored", "--nocapture"])
            .env(CHILD, "1")
            .env("RUST_BACKTRACE", "0")
            .output()
            .expect("spawn child test process");
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            out.status.success(),
            "the child must survive the X errors; status={:?}\nstdout={stdout}\nstderr={stderr}",
            out.status
        );
        let line = stdout
            .lines()
            .find(|l| l.starts_with(MARKER))
            .unwrap_or_else(|| panic!("child never returned; stdout={stdout}\nstderr={stderr}"));
        assert_eq!(
            line,
            format!(
                "{MARKER} delete: failed_writes_logged=1 cache_kept=true; \
                 change: failed_writes_logged=1 cache_kept=true"
            ),
            "each reload must attempt its property write on the dead window, have the \
             trap report it failed, and leave the cached region unchanged; stderr={stderr}"
        );
    }

    /// What one reload in [`destroyed_window_config_reload_scenario`] did.
    struct ReloadOutcome {
        /// `failed to sync blur=RequestFailed` records logged by the reload.
        failed_writes_logged: usize,
        /// The cached blur region is what it was before the reload.
        cache_kept: bool,
    }

    /// The child half of the test above: (a) the DELETE reload, (b) the CHANGE
    /// reload on the destroyed window.
    fn destroyed_window_config_reload_scenario() -> [ReloadOutcome; 2] {
        use gtk4::prelude::GtkWindowExt as _;

        crate::gtk::init_for_display_test(
            "x11_error_on_a_destroyed_window_does_not_exit_the_process",
        );
        let _ = libadwaita::init();
        crate::gtk::register_resources();
        let display = gdk4::Display::default().expect("a default GDK display");
        assert!(
            display.downcast_ref::<gdk4_x11::X11Display>().is_some(),
            "this test needs the X11 GDK backend (GDK_BACKEND=x11)"
        );

        let app = crate::gtk::class::application::Application::build_default_for_test();
        let x11_app = App::init(
            &display,
            "io.airely.rustty.test",
            &CoreConfig::default_config(),
        )
        .expect("x11 App::init")
        .expect("the X11 arm");
        app.imp()
            .winproto
            .replace(crate::gtk::winproto::App::X11(x11_app));
        let config_with_blur = |blur: bool| {
            let mut cfg = CoreConfig::default_config();
            cfg.background_blur = if blur {
                BackgroundBlur::True
            } else {
                BackgroundBlur::False
            };
            crate::gtk::class::config::Config::new(&cfg)
        };
        let config = config_with_blur(true);
        app.imp().config.replace(Some(config.clone()));
        let win = crate::gtk::class::window::Window::new(&app, None);
        win.imp().config.replace(Some(config));
        win.set_default_size(400, 300);
        win.present();

        let ctx = glib::MainContext::default();
        let pump = |n: usize| {
            for _ in 0..n {
                ctx.iteration(false);
                std::thread::sleep(std::time::Duration::from_millis(1));
            }
        };
        for _ in 0..3000 {
            if win.is_mapped() {
                break;
            }
            pump(1);
        }
        assert!(
            win.is_mapped(),
            "the window must map before its X window is destroyed"
        );
        pump(300);

        let cached = || -> Vec<Slice> {
            match &*win.imp().winproto.borrow() {
                crate::gtk::winproto::Window::X11(w) => w.blur_region.slices().to_vec(),
                _ => panic!("the window must be on the X11 winproto arm"),
            }
        };
        let before = cached();
        assert!(
            !before.is_empty(),
            "precondition: blur was applied while the window was alive, so a blur-off \
             reload must DELETE the property"
        );

        // A second X client destroys our toplevel; GDK processes the DestroyNotify.
        let xid = win
            .surface()
            .expect("surface")
            .downcast::<gdk4_x11::X11Surface>()
            .expect("an X11 surface")
            .xid();
        let xl = xlib::Xlib::open().expect("libX11");
        unsafe {
            let other = (xl.XOpenDisplay)(std::ptr::null());
            assert!(!other.is_null(), "a second X connection");
            (xl.XDestroyWindow)(other, xid);
            (xl.XSync)(other, 0);
            (xl.XCloseDisplay)(other);
        }
        // GDK's OWN requests against the dead window (UPSTREAM-DEFECTS UD56) are
        // not what this test is about, and one of them, an `XIQueryPointer`
        // (`request_code 131` XInputExtension, minor 40) issued while GDK
        // processes the unmap, made the child exit 1 in about one run in three on
        // the VM's headless sway Xwayland (task L-2). So every pump after the
        // destroy runs under a GDK error trap of its own. The port's property
        // writes never run inside one: they happen in `set_property` below, and
        // `display.sync()` flushes any error they leave BEFORE the next trapped
        // pump, so an untrapped write still exits the child.
        let x11_display = display
            .downcast_ref::<gdk4_x11::X11Display>()
            .expect("the X11 GDK backend")
            .clone();
        let pump_trapped = |n: usize| {
            // Dropped, so popped with `error_trap_pop_ignored`.
            let _trap = XErrorTrap::push(Some(&x11_display));
            pump(n);
        };
        pump_trapped(1000);

        // Each reload's `syncAppearance` runs synchronously inside `set_property`
        // (`notify::config`), and the trap pops (and syncs) before `syncBlur`
        // returns, so the reload's failed write is logged before `set_property`
        // returns. Only records from inside that call are counted, so a
        // `resizeEvent` during a later pump cannot stand in for the reload's write.
        let capture = crate::log_capture::start();
        let failed_blur_writes = || {
            capture
                .messages()
                .iter()
                .filter(|(level, target, message)| {
                    *level == log::Level::Error
                        && target == LOG_SCOPE
                        && message == "failed to sync blur=RequestFailed"
                })
                .count()
        };
        let trapped_writes = || TRAPPED_WRITES.with(std::cell::Cell::get);
        let reload = |blur: bool| {
            let logged_before = failed_blur_writes();
            let trapped_before = trapped_writes();
            win.set_property("config", config_with_blur(blur));
            let logged = failed_blur_writes() - logged_before;
            let trapped = trapped_writes() - trapped_before;
            // Each logged failure must be a property write the trap reported as
            // failed: a `RequestFailed` produced without issuing the write (for
            // example an early return on `is_destroyed()`) logs the same record.
            assert_eq!(
                trapped, logged,
                "each `failed to sync blur=` record must come from a property write \
                 the X error trap reported as failed (trapped writes vs records)"
            );
            display.sync();
            pump_trapped(200);
            logged
        };

        // (a) Blur OFF: `syncBlur` deletes the property on the dead window.
        let delete = ReloadOutcome {
            failed_writes_logged: reload(false),
            cache_kept: cached() == before,
        };

        // (b) Blur ON with a sentinel cache: `syncBlur` changes the property. The
        // sentinel must differ from the region blur-on computes (`before`), or
        // `syncBlur` sees no change and makes no write.
        let sentinel = vec![Slice {
            x: 1,
            y: 2,
            width: 3,
            height: 4,
        }];
        assert_ne!(
            sentinel, before,
            "the sentinel must differ from the blur-on region, or reload (b) writes nothing"
        );
        if let crate::gtk::winproto::Window::X11(w) = &mut *win.imp().winproto.borrow_mut() {
            w.blur_region = BlurRegion::from_slices(sentinel.clone());
        }
        let change = ReloadOutcome {
            failed_writes_logged: reload(true),
            cache_kept: cached() == sentinel,
        };

        [delete, change]
    }
}
