# PREPORT_GTK.md — Phase-5 pre-port analysis: `apprt-gtk` tier

> Oracle: `/Users/gael/projects/ghostty` pinned @ `4749c4e9` (read-only). Every `file:line` resolves against this pin.
> Scope: `src/apprt/gtk/**` (55 `.zig` = 22,045 LOC + 21 `.blp` + 4 `.css`) plus the tier-facing shims `src/apprt/gtk.zig`, `App.zig`, `Surface.zig`. Largest unported chunk. macOS-first ordering means this lands **LAST**, but `PORT_RULES` R11 pre-port checklist is required **before** the first line is ported — this document is that gate.
> Generated read-only (grep/read/count only; no build/test/miri — sibling holds build locks).
> Cross-checks: no gtk rows exist in `.port/audits/GHOSTTY_LIFETIMES.tsv` today (`grep -c apprt/gtk` = 0) — all rows below are net-new. Nothing here re-raises `VALIDATION_REPORT.md §5` (that appendix is z2d / input / cfg-fmt / SIGWINCH / onig / tmux-DCS / port-status — all unrelated to gtk).

---

## 0. Tier shape & the one structural fact that drives everything

Every user-visible class is a **GObject subclass** authored with ghostty's own thin subclassing helper `class.zig::Common(Self, Private)` (`class.zig:16-363`). The idiom, repeated 22×:

- The instance is a `pub const X = extern struct { parent: Parent, private: *Private }`-shaped type with `pub const Parent = ...` and a module-level `pub var offset: c_int = 0` inside `Private` (e.g. `application.zig:116,134,221`).
- Pointer-typed state lives in `const Private = struct { ... }`. Template widgets are wired by `Class.bindTemplateChildPrivate("name", .{})` from a compiled `.ui` (Blueprint) file; those are **GTK-template-owned**, released en masse by `gtk.Widget.disposeTemplate` (`surface.zig:1911`), never unref'd by field.
- Non-template GObject refs created in `init`/lazily are unref'd explicitly in `dispose`/`finalize`.
- Two virtual hooks decide ownership: `dispose` (GObject refs; may run >1×) and `finalize` (owned heap: `alloc.destroy`, `glib.free`, boxed frees; runs once).

**The single most load-bearing pointer**: `Surface.Private.core_surface: ?*CoreSurface` is the ONLY libghostty-core object the tier owns outright (`surface.zig:650`, allocated with `Application.default().allocator()`, `alloc.destroy`'d in `finalize` at `surface.zig:1937`). Everything else is either a GObject (glib refcount discipline) or a raw C windowing handle (FFI, owned by GDK/Wayland/X11).

**OpenGL note (task said "+ OpenGL"):** there is **no GLSL and no shader/renderer code in this tier**. `Surface.Private.gl_area: *gtk.GLArea` (`surface.zig:623`) is a template child that merely supplies a GL context; the actual GL rendering is the already-ported `renderer` crate (`surface.zig:18` imports `../../../renderer.zig`). The only direct GL consumer here is `imgui_widget.zig` (inspector), which drives `cimgui`/`dcimgui`'s `ImGui_ImplOpenGL3` against the widget's own `gl_area` (`imgui_widget.zig:4,54,218`). So the GL risk is (a) the app-thread render contract and (b) cimgui FFI, NOT shader porting.

---

## 1. LIFETIMES rows — every pointer-typed struct field

TSV schema (7 cols): `file · struct · field · zig_type · class · rust_type · evidence`. Rendered as grouped tables. Classes per the ratified 11-class taxonomy (OWNED/SHARED/BORROW_PARAM/BORROW_FIELD/STATIC/BACKREF/INTRUSIVE/FFI/ARENA/GHOSTTY_C/UNKNOWN). "TC" = template child (`bindTemplateChildPrivate`). glib source IDs (`?c_uint`) are non-pointer but are **owned handles** (paired `glib.Source.remove`) and are listed because the port must model the free — marked `OWNED(handle)`.

### 1.1 `class/application.zig` — `Application.Private` (@163)

| field | zig_type | class | rust_type / discipline | evidence |
|---|---|---|---|---|
| rt_app | `*ApprtApp` | BACKREF | `*mut ApprtApp`, never freed here | `application.zig:167`; apprt `App` holds `app:*Application` (`App.zig:32`); no free in dispose/finalize |
| core_app | `*CoreApp` | BACKREF | `*mut CoreApp` (libghostty-core owns) | `application.zig:170`; `alloc = core_app.alloc` (`:247`); not freed here |
| config | `*Config` | SHARED | glib-refcounted `Config` obj; `unref` on drop | created `:368`, `priv.config.unref()` in dispose `:438` |
| winproto | `winprotopkg.App` (embedded union) | — | see §1.11 (contains FFI ptrs) | `application.zig:176` |
| global_shortcuts | `*GlobalShortcuts` | OWNED | glib obj created here; sole ref | `newInstance(GlobalShortcuts)` `:402`, `unref` `:441` |
| config_errors_dialog | `WeakRef(ConfigErrorsDialog)` | SHARED | `glib::WeakRef` wrapper; non-owning | `:203`; `weak_ref.zig` (no refcount on set) |
| css_provider | `*gtk.CssProvider` | OWNED | glib obj `.new()`; `unref` on drop | `.new()` `:372`, `unref` `:456` |
| custom_css_providers | `ArrayListUnmanaged(*gtk.CssProvider)` | OWNED | `Vec<CssProvider>`; each `unref` + `deinit` | `:212`; `for(...) provider.unref(); deinit` `:457-458` |
| saved_language | `?[:0]const u8` | OWNED | `Option<CString>` (alloc) | `:217`; `alloc.free(language)` `:442` |
| open_uri | `OpenURI` (embedded) | — | holds BACKREF rt_app internally | `.init(rt_app)` `:404`; see §1.12 |
| signal_source | `?c_uint` | OWNED(handle) | glib source id; `Source.remove` | `:206` |
| quit_timer.active | `c_uint` (union payload) | OWNED(handle) | glib timeout id | `:194-198` |

### 1.2 `class/window.zig` — `Window.Private` (@218)

| field | zig_type | class | rust_type | evidence |
|---|---|---|---|---|
| tab_bindings | `*gobject.BindingGroup` | OWNED | glib obj; `unref` on drop | `:232` |
| config | `?*Config` | SHARED | refcounted; `unref` | `:235` |
| winproto | `winprotopkg.Window` | — | §1.11 | `:238` |
| command_palette | `WeakRef(CommandPalette)` | SHARED | glib WeakRef | `:256` |
| context_menu_page | `?*adw.TabPage` | BORROW_FIELD | `*mut adw::TabPage` (owned by tab_view); set via `setup-menu` signal | `:260` "setup by setup-menu" |
| tab_overview | `*adw.TabOverview` | BORROW_FIELD | TC — `disposeTemplate` frees | `:263` |
| tab_bar | `*adw.TabBar` | BORROW_FIELD | TC | `:264` |
| tab_view | `*adw.TabView` | BORROW_FIELD | TC | `:265` |
| toolbar | `*adw.ToolbarView` | BORROW_FIELD | TC | `:266` |
| toast_overlay | `*adw.ToastOverlay` | BORROW_FIELD | TC | `:267` |
| handle_active_state_source | `?c_uint` | OWNED(handle) | glib source | `:224` |
| tab_overview_focus_timer | `?c_uint` | OWNED(handle) | glib source | `:253` |

### 1.3 `class/surface.zig` — `Surface.Private` (@570) — the tier's heart

| field | zig_type | class | rust_type | evidence |
|---|---|---|---|---|
| config | `?*Config` | SHARED | refcounted; `unref` | `:572`; `unref` dispose `:1858` |
| default_size | `?*Size` | OWNED | boxed; `ext.boxedFree(Size,v)` | `:575`; finalize `:1946` |
| min_size | `?*Size` | OWNED | boxed; `boxedFree` | `:579`; `:1954` |
| font_size_request | `?*font.face.DesiredSize` | OWNED | `glib.ext.destroy` | `:583`; `:1950` |
| mouse_hover_url | `?[:0]const u8` | OWNED | `glib.free` | `:592`; `:1942` |
| pwd | `?[:0]const u8` | OWNED | `glib.free` | `:601`; `:1958` |
| title | `?[:0]const u8` | OWNED | `glib.free` | `:604`; `:1962` |
| title_override | `?[:0]const u8` | OWNED | `glib.free` | `:607`; `:1966` |
| gl_area | `*gtk.GLArea` | BORROW_FIELD | TC (GL context source) | `:623`; bind `:3620`-region; `disposeTemplate` `:1911` |
| url_left | `*gtk.Label` | BORROW_FIELD | TC | `:626` |
| url_right | `*gtk.Label` | BORROW_FIELD | TC | `:627` |
| resize_overlay | `*ResizeOverlay` | BORROW_FIELD | TC (own GObject subclass) | `:630` |
| search_overlay | `*SearchOverlay` | BORROW_FIELD | TC | `:633` |
| key_state_overlay | `*KeyStateOverlay` | BORROW_FIELD | TC | `:636` |
| rt_surface | `ApprtSurface` (embedded) | — | holds `*Surface` backref | `:639`; `Surface.zig:12` |
| **core_surface** | `?*CoreSurface` | **OWNED** | `Box<CoreSurface>` / owned raw; `alloc.destroy` in finalize | `:650`; `v.deinit(); alloc.destroy(v)` `:1936-1937` |
| im_context | `*gtk.IMMulticontext` | BORROW_FIELD | TC | `:658`; `bindTemplateChildPrivate("im_context")` `:3620` |
| bell_media | `?*gtk.MediaFile` | OWNED | glib obj; `unref` | `:681`; `unref` dispose `:1864` |
| inspector | `?*InspectorWindow` | BACKREF | `*mut InspectorWindow` non-owning; weak-notify nulls it | `:693`; `priv.inspector=null` on notify `:2624` |
| action_group | `?*gio.SimpleActionGroup` | SHARED | glib obj inserted into widget action map (widget owns) | `:699`; `ext.actions.addAsGroup` `:1852` |
| hadj | `?*gtk.Adjustment` | OWNED | glib obj; `unref` | `:702`; `:1875` |
| vadj | `?*gtk.Adjustment` | OWNED | glib obj; `unref` | `:703`; `:1880` |
| vadj_signal_group | `?*gobject.SignalGroup` | OWNED | glib obj; `setTarget(null)+unref` | `:706`; `:1868-1870` |
| child_exited_overlay | `*ChildExited` | BORROW_FIELD | TC | `:713` |
| context_menu | `*gtk.PopoverMenu` | BORROW_FIELD | TC | `:714` |
| drop_target | `*gtk.DropTarget` | BORROW_FIELD | TC | `:715` |
| progress_bar_overlay | `*gtk.ProgressBar` | BORROW_FIELD | TC | `:716` |
| error_page | `*adw.StatusPage` | BORROW_FIELD | TC | `:717` |
| terminal_page | `*gtk.Overlay` | BORROW_FIELD | TC | `:718` |
| key_sequence | `ArrayListUnmanaged([:0]const u8)` | OWNED | `Vec<CString>`; free each + deinit | `:709`; finalize `:1979-1980` |
| key_tables | `ArrayListUnmanaged([:0]const u8)` | OWNED | `Vec<CString>` | `:710`; `:1981-1982` |
| overrides.command | `?configpkg.Command` | OWNED | `.deinit(alloc)` | `:738`; finalize `:1970` |
| overrides.working_directory | `?[:0]const u8` | OWNED | `alloc.free` | `:739`; `:1974` |
| progress_bar_timer | `?c_uint` | OWNED(handle) | glib source; `Source.remove` | `:670`; `:1884` |
| idle_rechild | `?c_uint` | OWNED(handle) | glib source | `:690`; `:1891` |
| pending_horizontal_scroll_reset | `?c_uint` | OWNED(handle) | glib source | `:735`; `:1898` |

### 1.4 `class/tab.zig` — `Tab.Private` (@158)

| field | zig_type | class | rust_type | evidence |
|---|---|---|---|---|
| config | `?*Config` | SHARED | refcounted | `:160` |
| title | `?[:0]const u8` | OWNED | `Option<CString>` (glib.free) | `:163` |
| title_override | `?[:0]const u8` | OWNED | glib.free | `:166` |
| tooltip | `?[:0]const u8` | OWNED | glib.free | `:169` |
| split_tree | `*SplitTree` | BORROW_FIELD | TC | `:172` |

### 1.5 `class/split_tree.zig` — `SplitTree.Private` (@146) + `Split.Private` (@1113)

| field | zig_type | class | rust_type | evidence |
|---|---|---|---|---|
| tree | `?*Surface.Tree` | OWNED | owned tree datastructure | `:148` |
| tree_bin | `*adw.Bin` | BORROW_FIELD | TC | `:151` |
| last_focused | `WeakRef(Surface)` | SHARED | glib WeakRef | `:155` |
| pending_close | `?Surface.Tree.Node.Handle` | — | value handle (index into tree) | `:170` |
| rebuild_source | `?c_uint` | OWNED(handle) | glib source | `:159` |
| restore_focus_source | `?c_uint` | OWNED(handle) | glib source | `:166` |
| (Split) handle | `Surface.Tree.Node.Handle` | — | value | `:1116` |
| (Split) paned | `*gtk.Paned` | BORROW_FIELD | TC | `:1128` |
| (Split) idle | `?c_uint` | OWNED(handle) | glib source | `:1119` |

### 1.6 `class/command_palette.zig` — `CommandPalette.Private` (@66) + `Command.Private` (@532)

| field | zig_type | class | rust_type | evidence |
|---|---|---|---|---|
| config | `?*Config` | SHARED | refcounted | `:68` |
| dialog | `*adw.Dialog` | BORROW_FIELD | TC | `:71` |
| search | `*gtk.SearchEntry` | BORROW_FIELD | TC | `:74` |
| view | `*gtk.ListView` | BORROW_FIELD | TC | `:77` |
| model | `*gtk.SingleSelection` | BORROW_FIELD | TC | `:80` |
| source | `*gio.ListStore` | BORROW_FIELD | TC (data store) | `:84` |
| (Command) config | `?*Config` | SHARED | refcounted | `:533` |
| (Command) arena | `ArenaAllocator` | ARENA | per-command arena | `:534` |
| (Command) data.jump.surface | `WeakRef(Surface)` | SHARED | glib WeakRef | `:551` |
| (Command) data.regular.{action,action_key} | `?[:0]const u8` | ARENA | arena-alloc'd strings | `:546-547` |

### 1.7 `class/global_shortcuts.zig` — `GlobalShortcuts.Private` (@54)

| field | zig_type | class | rust_type | evidence |
|---|---|---|---|---|
| config | `?*Config` | SHARED | refcounted | `:56` |
| dbus_connection | `?*gio.DBusConnection` | SHARED | glib obj (session bus); `unref` | `:59` |
| arena | `?ArenaAllocator` | ARENA | per-refresh arena | `:62` |
| map | `StringArrayHashMapUnmanaged(Binding.Action)` | OWNED | `HashMap<CString,Action>` | `:68` |
| handle | `?[:0]const u8` | OWNED | glib.free (dbus object path) | `:72` |
| response_subscription | `c_uint` (0=null) | OWNED(handle) | dbus signal sub id | `:76` |
| activate_subscription | `c_uint` (0=null) | OWNED(handle) | dbus signal sub id | `:80` |

### 1.8 Small dialog / overlay / widget classes

| file · struct | field | zig_type | class | rust_type | evidence |
|---|---|---|---|---|---|
| config.zig `Config.Private` | config | `CoreConfig` (value) | OWNED | embedded `Config` value | `:69` |
| surface_scrolled_window.zig | config | `?*Config` | SHARED | refcounted | `:60` |
| " | config_binding | `?*gobject.Binding` | OWNED | glib obj; unbind/unref | `:61` |
| " | surface | `?*Surface` | BORROW_FIELD | child widget (widget tree owns) | `:62` |
| " | scrolled_window | `*gtk.ScrolledWindow` | BORROW_FIELD | TC | `:63` |
| clipboard_confirmation_dialog.zig | request | `?*apprt.ClipboardRequest` | BORROW_FIELD | caller-owned request ptr | `:119` |
| " | clipboard_contents | `?*gtk.TextBuffer` | OWNED | glib obj; unref | `:122` |
| " | text_view_scroll/text_view/reveal_button/hide_button | `*gtk.*` | BORROW_FIELD | TC | `:131-134` |
| " | **remember_choice** | `if(can_remember) *adw.SwitchRow else void` | BORROW_FIELD (comptime) | TC, comptime-conditional (see §3) | `:135` |
| close_confirmation_dialog.zig | target | `Target` (union) | — | value | `:69` |
| config_errors_dialog.zig | config | `?*Config` | SHARED | refcounted | `:56` |
| inspector_widget.zig | surface | `?*Surface` | BACKREF | non-owning; weak-notify attached | `:50` "NOT referenced" |
| inspector_window.zig | surface | `WeakRef(Surface)` | SHARED | glib WeakRef | `:71` |
| " | inspector_widget | `*InspectorWidget` | BORROW_FIELD | TC | `:74` |
| imgui_widget.zig | gl_area | `*gtk.GLArea` | BORROW_FIELD | TC (GL context) | `:54` |
| " | im_context | `*gtk.IMMulticontext` | BORROW_FIELD | TC | `:57` |
| " | ig_context | `?*cimgui.c.ImGuiContext` | FFI | cimgui ctx; `DestroyContext` per Dear ImGui | `:61` |
| " | tick_callback_id | `c_uint` | OWNED(handle) | gtk tick cb id | `:67` |
| search_overlay.zig | search_entry | `*gtk.SearchEntry` | BORROW_FIELD | TC | `:209` |
| resize_overlay.zig | label | `*gtk.Label` | BORROW_FIELD | TC | `:129` |
| " | label_text | `?[:0]const u8` | OWNED | glib.free | `:132` |
| " | idler/timer/delay_timer | `?c_uint` | OWNED(handle) | glib sources | `:142-148` |
| key_state_overlay.zig | tables | `?*ext.StringList` | OWNED | boxed StringList | `:122` |
| " | sequence | `?*ext.StringList` | OWNED | boxed StringList | `:125` |
| surface_child_exited.zig | data | `?*apprt.surface.Message.ChildExited` | OWNED | copied message ptr | `:64` |
| " | banner | `*adw.Banner` | BORROW_FIELD | TC | `:67` |
| title_dialog.zig | initial_value | `?[:0]const u8` | OWNED | glib.free | `:79` |
| " | target | `Target` | — | value | `:82` |
| " | entry | `*gtk.Entry` | BORROW_FIELD | TC | `:83` |

### 1.9 Standalone apprt shims

| file · struct | field | zig_type | class | rust_type | evidence |
|---|---|---|---|---|---|
| App.zig `App` | app | `*Application` | OWNED | holds the sole created ref; `unref`/`deinit` on terminate | `App.zig:32,44,57` |
| Surface.zig `Self` | surface | `*Surface` | BACKREF | `*mut Surface` (GObject owns; this wrapper is embedded in it) | `Surface.zig:12,27` |
| weak_ref.zig `WeakRef(T)` | ref | `gobject.WeakRef` | SHARED | `glib::WeakRef<T>`; get() returns strong ref (must unref) | `weak_ref.zig:11,27-31` |

### 1.10 `ext/`, `media.zig`, `cgroup.zig`, `portal/`, `ipc/`, `key.zig`

| file · struct | field | zig_type | class | rust_type | evidence |
|---|---|---|---|---|---|
| ext/slice.zig `StringList` | strings | `[]const [:0]const u8` | OWNED | `Vec<CString>` (boxed elsewhere) | `ext/slice.zig:10` |
| ext.zig (list helper) | list | `*glib.List` | FFI | glib.List; free per glib | `ext.zig:36` |
| ext/actions.zig `Action` | parameter_type | `?*const glib.VariantType` | STATIC | interned VariantType (static) | `ext/actions.zig:48` |
| " | state | `?*glib.Variant` | OWNED | glib.Variant; unref | `:49` |
| media.zig `AudioBell` | current | `?*gtk.MediaFile` | OWNED | glib obj; unref (see surface bell_media) | `media.zig:60` |
| " | path | `[:0]const u8` | OWNED | CString | `:61` |
| cgroup.zig `Options` | dbus | `*gio.DBusConnection` | SHARED | session bus (App-owned) | `cgroup.zig:30` |
| portal/OpenURI.zig `OpenURI` | (see Entry map) | — | — | owns pending Entry list | `portal/OpenURI.zig:79` |
| " `RequestData` | open_uri | `*OpenURI` | BACKREF | non-owning back-ptr | `:38` |
| " `RequestData` | uri/request_path | `[:0]const u8` | OWNED | CString | `:41-42` |
| " `Entry` | uri | `[:0]const u8` | OWNED | CString | `:93` |
| key.zig `Encoder`/helpers | writer | `*std.Io.Writer` | BORROW_PARAM | call-scoped `&mut Writer` | `key.zig:61,241,256` |
| " | event | `*gdk.KeyEvent` / `*gdk.Event` | FFI | GDK-owned event borrow | `key.zig:99,145` |
| " | app_winproto | `*winproto.App` | BORROW_FIELD | non-owning ptr into App | `key.zig:149` |

### 1.11 `winproto/` — windowing-protocol backends (Wayland / X11 / noop)

| file · struct | field | zig_type | class | rust_type | evidence |
|---|---|---|---|---|---|
| winproto.zig `App` (union) | none/wayland/x11 | `noop.App`/`wayland.App`/`x11.App` | — | comptime-selected union (see §3) | `winproto.zig:25-27` |
| winproto.zig `Window` (union) | none/wayland/x11 | `*.Window` | — | union | `:90-92` |
| wayland.zig `App` | display | `*wl.Display` | FFI | GDK-owned wl_display (borrow) | `wayland.zig:28` |
| " | globals | `*Globals` | OWNED | our alloc; destroy in deinit | `wayland.zig:29,58` |
| wayland.zig `Window` | apprt_window | `*ApprtWindow` | BACKREF | non-owning | `:93` |
| " | surface | `*wl.Surface` | FFI | GDK-owned wl_surface | `:96` |
| " | globals | `*Globals` | BORROW_FIELD | App-owned; non-owning alias | `:99` |
| " | bg_effect | `?*ext.BackgroundEffectSurfaceV1` | FFI | wl proxy; `.destroy()` | `:102`; deinit `:190` |
| " | decoration | `?*org.KdeKwinServerDecoration` | FFI | wl proxy; `.destroy()` | `:106` |
| " | slide | `?*org.KdeKwinSlide` | FFI | wl proxy; `.destroy()` | `:110` |
| " | activation_token | `?*xdg.ActivationTokenV1` | FFI | wl proxy; `.destroy()` | `:114`; deinit `:237` |
| wayland/Globals.zig `Binding` | proxy | `*wl.Proxy` | FFI | wl proxy | `Globals.zig:25` |
| wayland/Globals.zig `State` | primary_output_name | `?[:0]const u8` | OWNED | CString | `Globals.zig:53` |
| x11.zig `App` | display | `*xlib.Display` | FFI | Xlib display (GDK-owned) | `x11.zig:27` |
| x11.zig `App` | atoms | `Atoms` (c.Atom values) | — | value struct | `x11.zig:29,433` |
| x11.zig `Window` | app | `*App` | BACKREF | non-owning | `:170` |
| " | apprt_window | `*ApprtWindow` | BACKREF | non-owning | `:171` |
| " | x11_surface | `*gdk_x11.X11Surface` | FFI | GDK obj borrow | `:172` |
| " | last_applied_decoration_hints | `?MotifWMHints` | — | value | `:180` |
| BlurRegion.zig | window | `*ApprtWindow` (param) | BORROW_PARAM | call-scoped | `BlurRegion.zig:43` |

### 1.12 Ownership-summary counts (for the porter)

- **BORROW_FIELD (template children + widget-tree children):** ~40 fields — the dominant class. Rule: never `unref`; released by `disposeTemplate`/parent. In gtk4-rs these are `TemplateChild<T>`.
- **OWNED (glib obj `unref` / boxed `boxedFree` / `alloc.destroy` / `glib.free`):** ~30 fields. Each has an explicit dispose/finalize free-site cited above; the port's `Drop`/`dispose` must reproduce every one.
- **SHARED (Config + WeakRef + dbus + action_group):** ~12 fields.
- **FFI (wl/xlib/gdk-x11/cimgui/glib.List raw handles):** ~13 fields — per-library ownership (wl proxies `.destroy()`, ImGui `DestroyContext`, GDK objects are borrows).
- **BACKREF:** ~9 fields.
- **OWNED(handle) glib source ids:** ~14 fields — model as a `SourceId` newtype whose `Drop`/dispose calls `glib::source_remove`.

---

## 2. Paired-invariant catalog (cross-crate predicates this tier introduces)

Each is a two-sided contract the port must preserve; violating it is UB or a leak/double-free.

1. **App-thread render contract.** `pub const must_draw_from_app_thread = true` (`App.zig:23`) ⇔ the `renderer` crate must NOT draw from the render thread; it sends `redraw_surface` so the app thread calls `drawFrame` (GLArea is single-thread). Cross-crate: `apprt-gtk` ⇔ `renderer`. Port: this const gates a code path already anticipated in the ported renderer; the gtk side must re-assert it.
2. **Singleton-allocator symmetry.** `Application.default()` is a process-global GObject singleton (`Surface.zig:32`, `surface.zig:1923,1929`). `core_surface` is allocated with `Application.default().allocator()` and MUST be `alloc.destroy`'d with the same allocator (`surface.zig:1937`). Invariant: alloc-side ↔ free-side use the identical `CoreApp.alloc`. Port: the Rust `Application` must expose one canonical allocator; a Surface finalize must reach the same one.
3. **`core_surface` realize ordering.** `core_surface` starts null and is only initialized once a `GLArea` is realized (`surface.zig:641-650`); `apprt.Surface.core()` asserts non-null (`Surface.zig:24-28`). Invariant: libghostty-core only calls `core()`-family entry points after realize. Port: keep `Option` + the assert; do not eagerly construct.
4. **WeakRef ref/unref pairing.** `WeakRef(T).get()` returns a **strong** ref that the caller must `unref` (`weak_ref.zig:27-31`, test `:41`). Every `.get()` call site in the tier is paired with an `unref`/`defer unref`. Port: model as `glib::WeakRef::upgrade() -> Option<T>` where `T`'s `Drop` unrefs.
5. **`deleteSurface` before `deinit`.** Finalize removes the surface from the core app's known-surfaces list *before* `core_surface.deinit()` (`surface.zig:1926-1936`) so a callback searching for the surface can't find a half-torn one. Cross-crate ordering invariant with libghostty-core.
6. **Template double-free workaround.** `dispose` sets `self.as(adw.Bin).setChild(null)` before `disposeTemplate` (`surface.zig:1905-1911`) to avoid a GTK double-free when a top-level template child is also the Bin's root child. Port must reproduce this exact ordering in the `dispose` impl (gtk4-rs subclass `dispose`).
7. **Version-gate ⇔ .blp resource-version pairing.** The compiled UI a class loads is versioned (`ui/1.0`, `ui/1.2`, `ui/1.5`, …). The class's comptime `Parent` type and comptime-conditional fields are gated on the same adw/gtk version (`dialog.zig:21` `Parent = if(adw_version.supportsDialogs())`, `clipboard_confirmation_dialog.zig:17,135`). Invariant: the resource version embedded ⇔ the code path compiled must agree. Cross-artifact: `.blp` ↔ `.zig`.
8. **cgroup-before-fork.** The surface's cgroup path (`Surface.zig:40` `cgroupPath()`) must exist before `core_surface` spawns the child (`post_fork.zig`/`pre_exec.zig`). Ordering invariant with termio/child-spawn.
9. **IME buffer consistency.** `in_keyevent`/`im_composing`/`im_buf[128]`/`im_len:u7` form a coherent group mutated across `filterKeypress`/`reset` (`surface.zig:657-661,1303,1450`). Invariant: `im_len ≤ 128`; state transitions gated by `in_keyevent`. Port: keep as one struct, not scattered fields.
10. **`action_group` self-reference.** `ext.actions.addAsGroup(Self, self, "surface", &actions)` (`surface.zig:1852`) installs a GObject action group whose callbacks receive `self` — a `container_of`/userdata backref that must stay valid for the widget's life. Invariant: action group lifetime ⊆ widget lifetime.
11. **glib source ⇔ object lifetime.** Every `?c_uint` source id must be `Source.remove`'d in dispose *before* the object frees, else the callback fires on freed memory (`surface.zig:1884-1903`, `window.zig`, `resize_overlay.zig`). Invariant: no live source outlives its target.
12. **Floating-ref sink.** GObject construction returns a floating ref that must be `refSink`'d exactly once when parented, else leak or premature free (`class.zig:36-38`). Invariant across every `newInstance`/template instantiation.

---

## 3. Comptime-parameter inventory (each → Rust const / generic / macro)

| Zig comptime construct | site | Rust target |
|---|---|---|
| `Common(comptime Self, comptime Private)` class generator | `class.zig:16` | A `subclass!` macro or per-class hand-written `glib::Object` subclass (gtk4-rs `#[glib::object_subclass]`); `Private` → the `ObjectSubclass` impl struct |
| `defineVirtualMethod(comptime name)` | `class.zig:99` | gtk4-rs virtual-method override in the subclass `*Impl` trait |
| `privateShallow/Boxed/Obj/StringFieldAccessor(comptime name)` | `class.zig:165-304` | `glib::Properties` derive + typed getters/setters (transfer semantics per variant) |
| `bindTemplateChildPrivate(comptime name, comptime options)` | `class.zig:313` | `#[template_child]` fields in the `CompositeTemplate` derive |
| `bindTemplateCallback(comptime name, comptime func)` + the bool→c_int compile guard | `class.zig:330-352` | `#[template_callback]` — note the guard "must return c_int not bool" (glib gboolean ABI) survives as a Rust `-> i32` **CORRECTION (2026-09-27, task L-4b, NEW-FINDINGS N336): wrong for gtk4-rs — a `#[template_callback]` returns through glib-rs's closure marshaller, which aborts on an `i32` (`G_TYPE_INT`) for a `gboolean` signal; gboolean callbacks return `bool` (or `glib::Propagation`), see `TemplateCallbackReturn = bool` in `gtk/class.rs`.** |
| `gtk_version.atLeast/runtimeAtLeast/runtimeUntil(comptime major,minor,micro)` | `gtk_version.zig:49,75,90` | `const fn` over a baseline `SemanticVersion` + runtime `gtk::functions::{major,minor,micro}_version()`; the comptime short-circuit → `cfg!`/const eval |
| `adw_version.atLeast` + `supportsDialogs/TabOverview/SwitchRow/ToolbarView/Banner` | `adw_version.zig:47,104-122` | same pattern; the `supportsX()` helpers → `const fn`/inline predicates |
| **Comptime `Parent` type selection** `Dialog.Parent = if(supportsDialogs()) adw.AlertDialog else …`; `DebugWarning.Parent = if(supportsBanner()) adw.Bin else gtk.Box` | `dialog.zig:21`, `debug_warning.zig:14` | **Decision point:** gtk4-rs binds one concrete parent per build. macOS-first lets us pin a single min GTK4/libadwaita baseline (recommend adw ≥ 1.5 / gtk ≥ 4.14) and drop the older branches, OR gate with cargo features. Record the pinned baseline in the crate. |
| **Comptime-conditional FIELD TYPE** `remember_choice: if(can_remember) *adw.SwitchRow else void` where `const can_remember = adw_version.supportsSwitchRow()` | `clipboard_confirmation_dialog.zig:17,135,234,320` | With a pinned baseline this becomes an unconditional `TemplateChild<adw::SwitchRow>`; if features are used, `#[cfg(feature=...)]` on the field + all `if(comptime can_remember)` uses |
| `build_options.wayland` / `build_options.x11` selecting the winproto union arm | `winproto.zig:26-27,91-92` | Cargo features `wayland`/`x11`; the `App`/`Window` union → an enum whose arms are `#[cfg(feature=…)]` (macOS build compiles neither → `noop` only) |
| `gsettings.Key.Type()/GValueType()/requiresAllocation()` comptime dispatch on enum | `gsettings.zig:11-42` | `enum Key` + `match` (associated const/type via trait or a `match` returning the parsed value) |
| `ext.boxedCopy/boxedFree(comptime T)`, `getAncestor(comptime T)`, `getAncestorWhile(comptime T, comptime func)` | `ext.zig:19,25,35,52` | Rust generics `fn boxed_copy<T: BoxedType>` / `fn ancestor<T: IsA<Widget>>` |
| `portal.token_format = comptimePrint("{{x:0>{}}}", .{token_hex_len})` | `portal.zig:8-10` | `const` format width = `size_of::<usize>()*2`; `format!("{:0width$x}")` |
| `gresource.icon_sizes: []const comptime_int` + `blueprints: []const Blueprint` manifest | `build/gresource.zig:24,32` | `const [u32]` + a `const` slice of `(major,minor,name)` — drives the build.rs resource step (§5) |
| `App.performAction(comptime action: Action.Key)` / `performIpc(comptime action)` | `App.zig:68,83` | already handled by the apprt-api dispatch tier; gtk side is the switch arm |

---

## 4. Constants extraction

| constant | value | site | note |
|---|---|---|---|
| `base_application_id` | `"com.mitchellh.ghostty"` | `build/info.zig:4` | verbatim |
| `application_id` | base `++ "-debug"` (Debug/ReleaseSafe) else base | `build/info.zig:7-10` | build-mode split → Rust `cfg!(debug_assertions)` |
| `resource_path` | `"/com/mitchellh/ghostty"` | `build/info.zig:12` | gresource prefix |
| `object_path` | resource `++ "_debug"` (Debug/Safe) else resource | `build/info.zig:14-17` | build-mode split |
| `ui_path` | `"src/apprt/gtk/ui"` | `build/gresource.zig:15` | `{version}/{name}.blp` layout |
| `css_path` | `"src/apprt/gtk/css"` | `build/gresource.zig:18` | |
| `icon_sizes` | `{16,32,128,256,512,1024}` | `build/gresource.zig:24` | embedded icon sizes |
| `blueprints` manifest | 21 `(major,minor,name)` entries | `build/gresource.zig:32+` | mirrors `ui/*/*.blp`; drives build.rs |
| resize-overlay default duration | `750` (ms) | `resize_overlay.zig:44` | `.default = 750` |
| window magic timeout | `500` | `window.zig:1377` | |
| imgui frame throttle | `ns_per_s / 30` (≈33ms, 30 FPS) | `imgui_widget.zig:466`, doc `:450,465` | inspector redraw cap |
| `token_hex_len` | `@sizeOf(usize)*2` (=16 on 64-bit) | `portal.zig:8` | portal request token |
| `TokenBuffer` | `[token_hex_len+1]u8` | `portal.zig:9` | |
| GTK/adw version baselines | GTK `MAJOR/MINOR/MICRO`, ADW `1.3/1.4/1.5` thresholds | `gtk_version.zig:14`, `adw_version.zig:15,104-122` | dialogs≥1.5, tabOverview/switchRow/toolbarView≥1.4, banner≥1.3 |
| Motif WM hints bitfields | `_pad:u1`, decorations flags, `_rest` | `x11.zig:482-497` | packed struct(c_ulong) — port as `#[repr(C)]` bitflags |
| CSS files | `style.css` (183), `style-dark.css` (8), `style-hc.css` (3), `style-hc-dark.css` (3) | `css/` | reuse verbatim (§5) |

---

## 5. Risk notes

### 5.1 gtk4-rs binding-stack risk (HIGH)
The tier is authored against ghostty's own Zig GObject bindings (`@import("gtk")`, `"adw"`, `"gobject"`, `"gio"`, `"glib"`, `"gdk"`) generated from gobject-introspection. The Rust port targets the **gtk4-rs** stack (`glib`, `gtk4`, `libadwaita` crates) plus their `-sys` layers. Hazards:
- **Subclassing model mismatch.** ghostty hand-rolls `Common(Self,Private)`; gtk4-rs uses the `glib::object_subclass` + `CompositeTemplate` + `glib::Properties` macro triad. Every one of the 22 classes must be re-expressed in that idiom — this is mechanical but voluminous and is where fidelity bugs (missing property, wrong transfer mode) hide. The `class.zig` accessor helpers (`privateBoxed/Obj/StringFieldAccessor`) encode exact transfer semantics (`.none`/`.full`) that MUST map 1:1 onto gtk4-rs property flags.
- **No `gobject.ext.as` compile-time upcast in Rust.** ghostty's `as()` (`class.zig:25`) is a static-safe upcast; gtk4-rs uses `.upcast()`/`IsA<T>` bounds. Unsafe casts (`gobject.ext.cast`) → `.dynamic_cast()`.
- **`getClass`/`f_g_class` raw `TypeInstance` poking** (`class.zig:86-89`) and `defineVirtualMethod` (`class.zig:99`) rely on GObject memory layout; gtk4-rs exposes virtual overrides through `*Impl` traits — the raw layout access does not port and must be replaced by the safe trait mechanism (behavior-preserving, not line-preserving; flag for R1 waiver).
- **Wayland/X11 protocol bindings.** `winproto/wayland.zig` uses `wl`, `xdg`, `org` (KDE), `kde`, `ext` protocol modules; `x11.zig` uses `xlib`/`gdk_x11`. Rust equivalents: `wayland-client`/`wayland-protocols`(+wlr/kde) and `x11`/`x11rb` + `gdk4-x11`. These are **feature-gated off on macOS** (`winproto.zig:26-27`) so the milestone can ship `noop` first and defer the real backends.

### 5.2 GObject lifetime hazards (HIGH — the reason this pre-port exists)
- **Floating references.** New GObjects float until `refSink` (`class.zig:36`). A missed sink leaks; a double-sink frees early. Every `newInstance`/template-instantiated object must sink exactly once when parented (invariant §2.12). gtk4-rs handles this for widgets added to containers but NOT for manually-held refs (`css_provider`, `global_shortcuts`, `tab_bindings`).
- **WeakRef finalize races.** `config_errors_dialog`, `command_palette`, `last_focused`, `inspector_window.surface`, jump-command `surface` are `WeakRef` precisely because the referent can finalize out from under us (`application.zig:200-203`). The `inspector` raw `?*InspectorWindow` (`surface.zig:693`) and `inspector_widget.surface` (`inspector_widget.zig:50`) are nulled by weak-notify callbacks (`surface.zig:2624`). Port must wire `glib::WeakRef` + weak-notify closures faithfully; a raw `*mut` without the notify is a use-after-free.
- **Template double-free** (invariant §6) — reproduce `setChild(null)` before `disposeTemplate` exactly.
- **dispose runs ≥1×, finalize runs once.** GObject can call `dispose` multiple times; every field-null-after-unref pattern (`if (priv.x) |v| { v.unref(); priv.x = null; }`) is deliberate idempotency. gtk4-rs `dispose` has the same contract — preserve the null-guard on every field.
- **Source-id leaks** (invariant §11): 14 `?c_uint` glib source ids. Model each as an RAII `SourceId(u32)` that removes on drop / dispose; forgetting one = callback on freed object.
- **cimgui context** (`imgui_widget.zig:61`, FFI): `ImGuiContext` is per-widget and must be `DestroyContext`'d; it interacts with GL current-context state — flag as an isolated-Miri candidate if any raw aliasing appears (per `shaper-hook-sb-clean-interleave` doctrine).

### 5.3 The `.blp` / `.css` / GLSL reuse decision (RATIFIED here)
- **`.blp` (21 Blueprint files) — reuse VERBATIM.** They are declarative XML-precursor UI, GObject-toolkit-agnostic, versioned under `ui/{major.minor}/`. Decision: **copy the `.blp` byte-for-byte** into the rust crate and compile them at build time via `blueprint-compiler` → `.ui`, exactly as ghostty does (`build/blueprint.zig` shells `blueprint-compiler` with min-version checks; `build/gresource.zig:32` lists them). The Rust `build.rs` reproduces: run `blueprint-compiler compile`, then `glib-compile-resources` over the generated `.ui` + `.css` + icons into a `.gresource`, embedded via `gio::Resource`/`gtk4-rs`'s resource macro. The `blueprints` manifest and `icon_sizes` (§4) port to `const` slices driving `build.rs`. This preserves R1 fidelity (the UI is not re-authored) and matches `sys-generated-config-header` build.rs precedent in memory.
- **`.css` (4 files, 197 lines) — reuse VERBATIM**, embedded in the same gresource; loaded through `css_provider` (`application.zig:372`, custom providers `:1119`). No translation.
- **GLSL — N/A for this tier.** Confirmed: zero `.glsl/.vert/.frag` under `src/apprt/gtk/`. Shaders live in the `renderer` crate (already ported per `renderer-config-accessor-trait-acyclic`). `GLArea` here only vends a GL context; the inspector's GL is cimgui's bundled `ImGui_ImplOpenGL3`. **No shader-porting work belongs in this tier** — do not scope it here.

### 5.4 Sequencing risk / recommendation
macOS-first means this tier is genuinely last, so the pre-port artifact's value is (a) locking the LIFETIMES rows now while the oracle is fresh, and (b) the two ratified decisions above (baseline version pin; verbatim `.blp`/`.css` via build.rs). Recommend the embedded-first sub-milestone: land the crate with the `noop` winproto and the GObject class skeletons (behavior-preserving subclass macros) BEFORE wiring real Wayland/X11, mirroring the `placeholder-leaf-doctrine` (INHABITED placeholders, `unimplemented!()` leaves for the protocol backends). The `winproto` union's cargo-feature gating makes this clean: macOS compiles only `noop`, so the first green build needs zero wayland/x11 crates.
