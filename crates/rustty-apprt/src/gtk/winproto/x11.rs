//! rustty-apprt::gtk::winproto::x11
//!
//! PORTS: ghostty `src/apprt/gtk/winproto/x11.zig` (499 loc) — the X11
//! window-protocol backend. Two types: [`App`] (per-application: the xlib
//! `Display` GDK owns, plus the interned [`Atoms`]) and [`Window`] (per-surface:
//! the `X11Surface` XID plus the CSD/blur property machinery).
//!
//! Gated `#[cfg(feature = "x11")]` at the [`super`] module; pulls the `gdk4-x11`
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
//! HINGE (`x11.zig:38-41`): under a non-X11 display this makes [`super::App::init`]
//! fall through to the noop arm. It then reaches the xlib `Display*` via
//! `unsafe { x11d.xdisplay() }` (`x11.zig:43`). A window's XID comes from
//! downcasting the GDK surface to [`gdk4_x11::X11Surface`] and calling `.xid()`
//! (`x11.zig:187-193`).
//!
//! # Ownership
//!
//! The xlib `Display` is GDK-OWNED — [`App::deinit`] does NOT `XCloseDisplay` it
//! (`x11.zig:110-112`). X11 window state is property-based (no per-surface proxy
//! objects to destroy, unlike Wayland), so [`Window::deinit`] only clears the
//! cached blur region (`x11.zig:203-205`).
//!
//! # DEFERRALS
//!
//! * **XKB modifier snapshot.** The XKB extension IS initialized in [`App::init`]
//!   (`x11.zig:71-101`); [`App::event_mods`] is ported in the next task.
//! * **Quick terminal.** [`App::supports_quick_terminal`] is `false` and
//!   [`App::init_quick_terminal`] returns unsupported — matching the reference,
//!   whose X11 `supportsQuickTerminal` is unconditionally `false` (`x11.zig:161-166`;
//!   the quick terminal is a Wayland layer-shell feature).
//! * **`addSubprocessEnv` (`WINDOWID`).** The reference exports `WINDOWID` into the
//!   child env (`x11.zig:320-329`); that seam is threaded through the surface
//!   subprocess spawn, which is out of scope for this arm and not on the
//!   [`super::Window`] dispatch surface.

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
/// off its `*ApprtWindow` (`x11.zig:225-230,299,331`).
type ApprtWindow = crate::gtk::class::window::Window;

/// `_MOTIF_WM_HINTS` flag bit: decorations field is meaningful (`Xm/MwmUtil.h`;
/// the reference's `MotifWMHints.flags.decorations` bit, `x11.zig:482-484`).
const MWM_HINTS_DECORATIONS: c_ulong = 1 << 1;
/// `_MOTIF_WM_HINTS` decorations bit: enable ALL decorations (the reference's
/// `MotifWMHints.decorations.all` bit, `x11.zig:490-492`).
const MWM_DECOR_ALL: c_ulong = 1 << 0;

/// X11 property `format` value: 32-bit-logical items, stored as `c_long` words in
/// the client API (`x11.zig:459-461`). Both properties this arm writes
/// (`_MOTIF_WM_HINTS`, `_KDE_NET_WM_BLUR_BEHIND_REGION`) use format 32.
const FORMAT_32: c_int = 32;

/// `XkbUseCoreKbd` (`X11/XKBlib.h`, via `X11/extensions/XKB.h`) — the core-keyboard
/// device spec (`x11.zig:94`). `x11-dl` does not export it, so it is spelled here.
const XKB_USE_CORE_KBD: c_uint = 0x0100;
/// `XkbMajorVersion` (`X11/XKBlib.h`) — the XKB version rustty asks for
/// (`x11.zig:77`). Not exported by `x11-dl`.
const XKB_MAJOR_VERSION: c_int = 1;
/// `XkbMinorVersion` (`X11/XKBlib.h`) (`x11.zig:78`). Not exported by `x11-dl`.
const XKB_MINOR_VERSION: c_int = 0;

/// PORTS: the inferred error set of `x11.zig`'s `!App` / `!Window` returns
/// (`X11Error` + `GetWindowPropertyError`, `x11.zig:423-431`). Wrapped by
/// [`super::WinprotoError::X11`].
#[derive(Debug)]
pub enum Error {
    /// `error.NotX11Surface` (`x11.zig:188,193`).
    NotX11Surface,
    /// `x11-dl` could not load `libX11`. rustty-specific: the reference links Xlib
    /// statically via `@cImport`, so it has no analogue (there is nothing to load).
    XlibLoadFailed,
    /// `error.XkbInitializationError` (`x11.zig:88,100`) — `XkbQueryExtension` or
    /// `XkbSelectEventDetails` failed.
    XkbInitializationError,
    /// `error.RequestFailed` (`x11.zig:424`) — an Xlib call reported failure.
    RequestFailed,
    /// `error.PropertyTypeMismatch` (`x11.zig:373,429`).
    PropertyTypeMismatch,
    /// `error.PropertyFormatMismatch` (`x11.zig:374,430`).
    PropertyFormatMismatch,
    /// The quick terminal is not supported on X11 (`x11.zig:161-166`).
    QuickTerminalUnsupported,
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
            Error::QuickTerminalUnsupported => {
                write!(f, "quick terminal is not supported on X11")
            }
        }
    }
}

impl std::error::Error for Error {}

impl From<Error> for WinprotoError {
    fn from(e: Error) -> Self {
        WinprotoError::X11(e)
    }
}

/// PORTS: `x11.zig:433-449` `Atoms`. The interned X11 atoms the property writes
/// target. The reference interns exactly these two (`x11.zig:437-448`); everything
/// else it needs (`XA_CARDINAL`, the None atom) is a compile-time constant.
#[derive(Clone, Copy)]
struct Atoms {
    /// `_KDE_NET_WM_BLUR_BEHIND_REGION` (`x11.zig:439-442`).
    kde_blur: xlib::Atom,
    /// `_MOTIF_WM_HINTS` (`x11.zig:443-446`).
    motif_wm_hints: xlib::Atom,
}

impl Atoms {
    /// PORTS: `x11.zig:437-448` `Atoms.init`. Uses GDK's per-display atom cache
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

/// PORTS: `x11.zig:481-498` `MotifWMHints`. See `Xm/MwmUtil.h`. `#[repr(C)]` of
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

/// PORTS: `x11.zig:26-167` `App`. Application-wide X11 state.
pub struct App {
    /// PORTS: `x11.zig:27` `display: *xlib.Display`. GDK owns it (`x11.zig:110-112`
    /// deinit is a no-op) — this is a BORROW, never `XCloseDisplay`d here.
    display: *mut xlib::Display,
    /// The `x11-dl` function table (`XChangeProperty`/`XGetWindowProperty`/
    /// `XDeleteProperty`/`XFree`). rustty-specific: the reference calls these
    /// through the static `@cImport`. `Rc` so each [`Window`] shares the one
    /// process-cached table without re-`open`ing.
    xlib: Rc<xlib::Xlib>,
    /// PORTS: `x11.zig:28` `base_event_code: c_int`. The XKB extension's base event
    /// code, returned by `XkbQueryExtension` — [`Self::event_mods`] compares a
    /// peeked event's `type` against it (`x11.zig:141`).
    base_event_code: c_int,
    /// PORTS: `x11.zig:29` `atoms: Atoms`.
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
    /// PORTS: `x11.zig:31-108` `App.init`. Probe the GDK display for X11; if it is
    /// not X11, return `Ok(None)` — THE ORDERED-INIT FALLBACK HINGE
    /// (`x11.zig:38-41`). Otherwise reach the xlib `Display`, set the program class
    /// (WM_CLASS), load the `libX11` function table, initialize the XKB extension
    /// (`x11.zig:71-101`), and intern the atoms.
    pub fn init(
        gdk_display: &gdk4::Display,
        app_id: &str,
        config: &CoreConfig,
    ) -> Result<Option<App>, WinprotoError> {
        // `gobject.ext.cast(gdk_x11.X11Display, gdk_display) orelse return null`
        // (`x11.zig:38-41`).
        let Some(x11d) = gdk_display.downcast_ref::<gdk4_x11::X11Display>() else {
            return Ok(None);
        };

        // `gdk_x11_display.getXdisplay()` (`x11.zig:43`). `unsafe` per the gdk4-x11
        // binding; the pointer is a live `Display` GDK owns for its lifetime.
        let display = unsafe { x11d.xdisplay() };

        // Set WM_CLASS. The instance name is derived from `g_set_prgname`
        // (`x11.zig:45-69`): `x11-instance-name` if set, else the debug/release
        // program name. Matches the reference's structure with rustty's identity.
        let program_name: std::borrow::Cow<'_, str> = match &config.x11_instance_name {
            Some(pn) => String::from_utf8_lossy(pn),
            None if cfg!(debug_assertions) => std::borrow::Cow::Borrowed("rustty-debug"),
            None => std::borrow::Cow::Borrowed("rustty"),
        };
        glib::set_prgname(Some(program_name.as_ref()));
        // `gdk_x11.X11Display.setProgramClass(gdk_display, app_id)` (`x11.zig:69`).
        x11d.set_program_class(app_id);

        // Load libX11's function table (process-cached in x11-dl). rustty-specific:
        // stands in for the reference's static `@cImport` of Xlib.
        let xlib = xlib::Xlib::open().map_err(|_| Error::XlibLoadFailed)?;

        // XKB (`x11.zig:71-101`). `XkbQueryExtension` reports the extension's base
        // event code; `XkbSelectEventDetails` then asks the server to deliver
        // `XkbStateNotify` events whose MODIFIER-state bits changed, which is what
        // `event_mods` peeks for. Both are FATAL on failure in the reference.
        let mut opcode: c_int = 0;
        let mut base_event_code: c_int = 0;
        let mut base_error_code: c_int = 0;
        let mut major: c_int = XKB_MAJOR_VERSION;
        let mut minor: c_int = XKB_MINOR_VERSION;
        // `if (XkbQueryExtension(...) == 0) return error.XkbInitializationError`
        // (`x11.zig:79-89`).
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
            eprintln!("err(gtk-x11): error initializing Xkb extension: XkbQueryExtension failed");
            return Err(Error::XkbInitializationError.into());
        }

        // `if (XkbSelectEventDetails(display, XkbUseCoreKbd, XkbStateNotify,
        // XkbModifierStateMask, XkbModifierStateMask) == 0) return
        // error.XkbInitializationError` (`x11.zig:92-101`).
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
            eprintln!(
                "err(gtk-x11): error initializing Xkb extension: XkbSelectEventDetails failed"
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
    /// [`Self::init`] captured (`x11.zig:105`) so the live X11 KAT can prove
    /// `XkbQueryExtension` actually ran.
    #[doc(hidden)]
    #[must_use]
    pub fn xkb_base_event_code(&self) -> c_int {
        self.base_event_code
    }

    /// PORTS: `x11.zig:110-112` `App.deinit`. Nothing to release — the xlib
    /// `Display` is GDK-owned (NOT closed here), and X11 state is property-based
    /// (no proxies).
    #[allow(clippy::unused_self)]
    pub fn deinit(&mut self) {}

    /// PORTS: `x11.zig:124-159` `App.eventMods`. DEFERRED — returns `None` (the XKB
    /// modifier snapshot is not ported; see module DEFERRALS), so
    /// [`super::App::event_mods`] falls back to `key::translate_mods`.
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn event_mods(
        &self,
        _device: Option<&gdk4::Device>,
        _gtk_mods: gdk4::ModifierType,
    ) -> Option<rustty_vt::input::key_mods::Mods> {
        None
    }

    /// PORTS: `x11.zig:161-164` `App.supportsQuickTerminal`. The quick terminal is
    /// a Wayland layer-shell feature; the reference's X11 arm returns `false`.
    #[allow(clippy::unused_self)]
    #[must_use]
    pub fn supports_quick_terminal(&self) -> bool {
        false
    }

    /// PORTS: `x11.zig:166` `App.initQuickTerminal`. Unsupported on X11 — returns
    /// [`Error::QuickTerminalUnsupported`]. Only reachable if
    /// [`Self::supports_quick_terminal`] is `true` (never), so effectively
    /// unreachable; the reference's body is empty.
    #[allow(clippy::unused_self)]
    pub fn init_quick_terminal(
        &mut self,
        _window: &impl gtk4::prelude::IsA<gtk4::Window>,
    ) -> Result<(), WinprotoError> {
        Err(Error::QuickTerminalUnsupported.into())
    }
}

/// PORTS: `x11.zig:169-421` `Window`. Per-surface X11 state.
pub struct Window {
    /// PORTS: `x11.zig:171` `apprt_window: *ApprtWindow`. A WEAK ref (the window
    /// owns this winproto `Window`, so a strong ref is a GObject refcount cycle);
    /// upgraded at sync time to read config. Non-owning, like the reference's raw
    /// pointer.
    apprt_window: glib::WeakRef<ApprtWindow>,
    /// PORTS: `x11.zig:172` `x11_surface: *gdk_x11.X11Surface`. Cloned (bumps the
    /// GObject refcount) rather than borrowed, to avoid a lifetime tie to `App`;
    /// GDK owns the underlying surface, so `deinit` just drops this ref.
    x11_surface: gdk4_x11::X11Surface,

    /// Borrow of [`App::display`] (a Copy pointer; GDK-owned, never closed here).
    display: *mut xlib::Display,
    /// Shared clone of [`App::xlib`] — the libX11 function table.
    xlib: Rc<xlib::Xlib>,
    /// Copy of [`App::atoms`].
    atoms: Atoms,

    /// PORTS: `x11.zig:175` `blur_region`.
    blur_region: BlurRegion,
    /// PORTS: `x11.zig:180` `last_applied_decoration_hints`. Cache the last applied
    /// `_MOTIF_WM_HINTS` to avoid redundant property writes, which glitch some WMs
    /// (ghostty PR #8075, `x11.zig:177-180`).
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
    /// PORTS: `x11.zig:182-201` `Window.init`.
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
        // (`x11.zig:187-189`).
        let Some(gdk_surface) = gtk_window.surface() else {
            return Err(Error::NotX11Surface.into());
        };
        // `gobject.ext.cast(gdk_x11.X11Surface, surface) orelse return error.NotX11Surface`
        // (`x11.zig:190-193`).
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

    /// PORTS: `x11.zig:203-205` `Window.deinit`. Clear the cached blur region; X11
    /// state is property-based so there are no proxies to destroy, and the xlib
    /// `Display`/`X11Surface` are GDK-owned.
    pub fn deinit(&mut self) {
        self.blur_region.clear();
    }

    /// PORTS: `x11.zig:207-212` `Window.resizeEvent`. The blur region must track
    /// window resizes.
    pub fn resize_event(&mut self) -> Result<(), WinprotoError> {
        // The reference logs and swallows the sync error; we do the same (the
        // winproto dispatch expects a best-effort result).
        self.sync_blur();
        Ok(())
    }

    /// PORTS: `x11.zig:214-223` `Window.syncAppearance`. Re-derive both the blur
    /// region (CSD toggles change its offset) and the decoration hints.
    pub fn sync_appearance(&mut self) -> Result<(), WinprotoError> {
        self.sync_blur();
        self.sync_decorations();
        Ok(())
    }

    /// PORTS: `x11.zig:225-230` `Window.clientSideDecorationEnabled`. Returns the
    /// ACTUAL present decoration intent (the noop contract).
    #[must_use]
    pub fn client_side_decoration_enabled(&self) -> bool {
        matches!(
            self.window_decoration(),
            WindowDecoration::Auto | WindowDecoration::Client
        )
    }

    /// PORTS: `x11.zig:331-333` `Window.setUrgent`. Drives urgency via GDK's
    /// `X11Surface::set_urgency_hint` (the WM_HINTS urgency flag) — the SAME
    /// mechanism the reference uses (`self.x11_surface.setUrgencyHint(...)`), NOT a
    /// `_NET_WM_STATE_DEMANDS_ATTENTION` client message.
    pub fn set_urgent(&mut self, urgent: bool) -> Result<(), WinprotoError> {
        self.x11_surface.set_urgency_hint(urgent);
        Ok(())
    }

    /// PORTS: `x11.zig:232-276` `Window.syncBlur`. Sets/clears the KDE blur region
    /// property from the window's `background-blur` config.
    fn sync_blur(&mut self) {
        let Some(apprt) = self.apprt_window.upgrade() else {
            return;
        };
        let Some(config) = apprt.get_config() else {
            return;
        };
        let blur = config.get().background_blur;

        // `if (blur.enabled()) calcForWindow(..., true) else .empty` (`x11.zig:238-246`).
        // The X11 path requests DEVICE coordinates (`to_device_coordinates = true`).
        let region = if background_blur_enabled(blur) {
            let csd = self.client_side_decoration_enabled();
            let native: &gtk4::Native = apprt.upcast_ref();
            BlurRegion::calc_for_window(native, csd, true)
        } else {
            BlurRegion::empty()
        };

        // Only touch X11 when the region actually changes (`x11.zig:250-253`).
        if region.eql(&self.blur_region) {
            return;
        }

        if region.slices().is_empty() {
            // `deleteProperty(kde_blur)` (`x11.zig:271`).
            let _ = self.delete_property(self.atoms.kde_blur);
        } else {
            // `changeProperty(Slice, kde_blur, XA_CARDINAL, ._32, .replace, slices)`
            // (`x11.zig:262-269`). Each `Slice` is four `c_long` words => nelements
            // = 4 * len. `Slice` is `#[repr(C)]` of `i64`(=`c_long`) so the slice
            // buffer is already a contiguous long array.
            let slices = region.slices();
            let nelements = (slices.len() * 4) as c_int;
            let _ = unsafe {
                self.change_property(
                    self.atoms.kde_blur,
                    xlib::XA_CARDINAL,
                    slices.as_ptr().cast::<c_uchar>(),
                    nelements,
                )
            };
        }

        self.blur_region = region;
    }

    /// PORTS: `x11.zig:278-318` `Window.syncDecorations`. Read the existing
    /// `_MOTIF_WM_HINTS` (to preserve other bits), set the decorations flag + `all`
    /// bit from the window-decoration config, and write it back — skipping the
    /// write when the hints are unchanged.
    fn sync_decorations(&mut self) {
        // `getWindowProperty(...)` catching PropertyNotFound (`x11.zig:281-296`).
        let mut hints = match self.get_motif_hints() {
            Ok(Some(h)) => h,
            // PropertyNotFound => default (already-zeroed) hints (`x11.zig:289-290`).
            Ok(None) => MotifWmHints::default(),
            // RequestFailed / type / format mismatch: the reference propagates;
            // here syncAppearance is best-effort, so bail (matches its log+return).
            Err(_) => return,
        };

        // `hints.flags.decorations = true` (`x11.zig:298`).
        hints.flags |= MWM_HINTS_DECORATIONS;
        // `hints.decorations.all = switch (decoration) { .server => true, else => false }`
        // (`x11.zig:299-302`).
        if matches!(self.window_decoration(), WindowDecoration::Server) {
            hints.decorations |= MWM_DECOR_ALL;
        } else {
            hints.decorations &= !MWM_DECOR_ALL;
        }

        // Only update when the hints actually change (`x11.zig:305-307`).
        if self.last_applied_decoration_hints == Some(hints) {
            return;
        }

        // `changeProperty(MotifWMHints, motif_wm_hints, motif_wm_hints, ._32, .replace, &.{hints})`
        // (`x11.zig:309-316`). One element of five `c_long` words => nelements = 5.
        let data = std::ptr::addr_of!(hints).cast::<c_uchar>();
        if unsafe {
            self.change_property(self.atoms.motif_wm_hints, self.atoms.motif_wm_hints, data, 5)
        }
        .is_ok()
        {
            self.last_applied_decoration_hints = Some(hints);
        }
    }

    /// The window-decoration intent, read live from the apprt window
    /// (`x11.zig:226,299`). Falls back to `.auto` if the weak ref is dead.
    fn window_decoration(&self) -> WindowDecoration {
        self.apprt_window
            .upgrade()
            .map_or(WindowDecoration::Auto, |w| w.get_window_decoration())
    }

    /// PORTS: `x11.zig:335-379` `Window.getWindowProperty`, specialized to
    /// [`MotifWmHints`] (format 32, five words). Returns `Ok(None)` for the
    /// reference's `error.PropertyNotFound` (the caller treats it as "use
    /// defaults"). The `XFree` of the returned buffer mirrors `x11.zig:378`.
    fn get_motif_hints(&self) -> Result<Option<MotifWmHints>, Error> {
        let mut actual_type: xlib::Atom = 0;
        let mut actual_format: c_int = 0;
        let mut nitems: c_ulong = 0;
        let mut bytes_after: c_ulong = 0;
        let mut prop: *mut c_uchar = std::ptr::null_mut();

        // `XGetWindowProperty(display, xid, name, offset=0, length=maxInt, delete=0,
        // type, &actual_type, &actual_format, &nitems, &bytes_after, &prop)`
        // (`x11.zig:356-369`).
        let code = unsafe {
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
        };
        // `if (code != Success) return error.RequestFailed` (`x11.zig:370`).
        if code != c_int::from(xlib::Success) {
            return Err(Error::RequestFailed);
        }
        // `if (actual_type == None) return error.PropertyNotFound` (`x11.zig:372`) —
        // the X11 `None` atom is 0. Modelled as `Ok(None)`. Nothing was allocated.
        if actual_type == 0 {
            return Ok(None);
        }

        // Validate, then free unconditionally (buffer WAS allocated).
        let result = (|| {
            // `if (typ != actual_type) return error.PropertyTypeMismatch` (`x11.zig:373`).
            if actual_type != self.atoms.motif_wm_hints {
                return Err(Error::PropertyTypeMismatch);
            }
            // `if (format != actual_format) return error.PropertyFormatMismatch` (`x11.zig:374`).
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
            // (`x11.zig:376-377`). `read_unaligned` is defensive; the format-32
            // buffer is in fact `c_long`-aligned.
            Ok(Some(unsafe {
                std::ptr::read_unaligned(prop.cast::<MotifWmHints>())
            }))
        })();

        // `_ = XFree(prop_return)` (`x11.zig:378`).
        if !prop.is_null() {
            unsafe { (self.xlib.XFree)(prop.cast::<c_void>()) };
        }
        result
    }

    /// PORTS: `x11.zig:381-411` `Window.changeProperty`, concrete over a raw
    /// (format-32) byte buffer. `nelements` is the count of 32-bit-logical items
    /// (i.e. `c_long` words). Returns [`Error::RequestFailed`] when Xlib reports
    /// failure — note Xlib's status is `0 = failure` here (`x11.zig:407-410`).
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
        let status = unsafe {
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
        };
        if status == 0 {
            Err(Error::RequestFailed)
        } else {
            Ok(())
        }
    }

    /// PORTS: `x11.zig:413-420` `Window.deleteProperty`.
    fn delete_property(&self, name: xlib::Atom) -> Result<(), Error> {
        let status = unsafe {
            (self.xlib.XDeleteProperty)(self.display, self.x11_surface.xid(), name)
        };
        if status == 0 {
            Err(Error::RequestFailed)
        } else {
            Ok(())
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
