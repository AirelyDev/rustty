# Parity audit — src/apprt

Seventeen shards (45–61) audited 76 oracle units against the rustty port at ghostty pin `20abdb50a`, reading 30,175 lines of oracle source with no unit left partially read. 98 candidate findings were raised and double-verified: **63 CONFIRMED**, **19 DISPUTED** (one verification lens refuted), **16 REFUTED** (both lenses refuted, listed in the appendix); no findings exceeded the verification cap, so there are no UNVERIFIED entries. By corrected severity the kept (confirmed + disputed) findings are **0 high, 16 medium, 66 low** — the low tier is dominated by dropped/rewritten log text and GObject introspection metadata.

The single most important gap is **S46-F1**: `App.keymap` is eagerly initialized in `App::init` instead of lazily on first `keyboardLayout()`, so a `Keymap::init()` failure makes `rustty_app_new` return null and the embedded host cannot create the application at all, where the oracle logs a warning and degrades to `Layout::Unknown`. Close behind are **S49-F1** (SIGUSR2 hardcoded to 12, wrong on the gated-in FreeBSD target, so config reload lands on SIGSYS and a real SIGUSR2 kills the process) and **S55-F1** (snap env-path filtering deferred in a build configuration that `snapcraft.yaml` actually ships).

## Coverage

| Oracle unit | Rustty counterpart(s) | Status | Note |
| --- | --- | --- | --- |
| `src/apprt/action.zig:1-1071` | `rustty-apprt-api/src/action.rs`, `action_tests.rs`, `seam.rs`, `rustty-apprt/src/gtk/class/gtypes.rs`, `rustty-apprt-c/tests/abi_layout.rs` | ported | Target/TargetKey/TargetC + cval (rt_surface via `seam.rs` CoreSurfaceHandle, R4 cycle-break); all 69 Action arms in identical order; explicit `ActionCValue` extern union (comptime-synthesized in Zig) with the same per-arm C projection rule and `size_of == 24` const assert; every payload type ported (incl. `InitialSize` boxed GType relocated to `gtypes.rs:71-79`); 17 header enum orderings hand-verified against `include/rustty-embed.h:601-889`. Comptime `Value(key)` has no runtime form. `rustty-cli/src/action.rs` is a false candidate. |
| `src/apprt/browser.zig:1-4` | `rustty-apprt/src/browser.rs` | ported | `resourcesDir` re-exported from `rustty_os`; empty `App`/`Window`; wired at `rustty-apprt/src/lib.rs:32`. |
| `src/apprt/embedded.zig` (App, Options, KeyEvent, KeyTarget, Platform, Surface, Inspector) | `rustty-apprt/src/embedded.rs` | ported | Backend struct and every method ported incl. Surface::init LIFO errdefers, clipboard request/normalization, NaN + `<1` content-scale clamp, phantom-move filter, `defaultTermioEnv` Xcode strip/launch-source/LANGUAGE, Inspector 2-frame Metal loop and arm-by-arm `key_to_imgui_key`. Divergences filed as S46-F1/F2/F4/F5. |
| `src/apprt/embedded.zig` CAPI block (extern structs + 71 exports) | `rustty-apprt-c/src/capi.rs`, `lib.rs`, `include/rustty-apprt.h`, `include/rustty-embed.h` | ported | All extern structs with size/align const-asserts; `Point.pin` tag map/clamp and `Selection.core`; `Text.deinit` frees the len+1 sentinel; all exports present as `rustty_*` incl. the Darwin block and live-CGS `set_window_background_blur`. `ghostty_*` names not aliased (S46-F6, refuted as the locked R2 rename). |
| `src/apprt/gtk.zig:1-17` | `rustty-apprt/src/gtk.rs`, `tests/gtk_seam.rs` | ported | Re-exports App/Surface/WeakRef/resources_dir (flatpak override); `class`/`pre_exec`/`post_fork` as `pub mod`; the Zig `refAllDecls` test block has no runtime analogue beyond the gtk_seam KATs. Module doc header is stale (doc-only). |
| `src/apprt/ipc.zig` | `rustty-apprt-api/src/ipc.rs` | ported | Errors/Target/TargetKey/TargetC/cval, Action + NewWindow/NewTab C structs, ActionKey/ActionCValue/ActionC, the 12/16-byte size asserts and both header-sync tests. `NewTab.cval/deinit` fix the upstream copy-paste receiver bug (documented). Header-sync tests are tautological (S47-F1). |
| `src/apprt/none.zig` | `rustty-apprt/src/none.rs` | ported | `resourcesDir` re-export, `App::perform_ipc -> Ok(false)`, empty Surface. |
| `src/apprt/runtime.zig` | `rustty-apprt-api/src/runtime.rs`, `rustty-build-config/src/apprt.rs` | ported | Runtime enum + `default(target)` (linux/freebsd → Gtk, else None) hoisted to `Runtime::default_for`; reify test ported. |
| `src/apprt/structs.zig` | `rustty-apprt-api/src/structs.rs`, `gtk/class/gtypes.rs`, `gtk/class/clipboard_confirmation_dialog.rs` | ported | All structs/enums present incl. Kitty read/write; `GhosttyApprtClipboard` enum GType at `gtypes.rs:38-49`, `GhosttyClipboardRequest` boxed at `clipboard_confirmation_dialog.rs:48`; arena-owned Kitty structs modelled as owned Vec + `Box::into_raw/from_raw`. |
| `src/apprt/surface.zig` | `rustty-apprt-api/src/surface.rs`, `rustty-core/src/surface.rs`, `rustty-core/src/app.rs`, `gtk/class/surface_child_exited.rs` | ported | Message union (24 variants), WriteReq, DesktopNotification helpers, ReportTitleStyle, ChildExited (boxed at `surface_child_exited.rs:37`), NewSurfaceContext, `shouldInheritWorkingDirectory`; Mailbox relocated to `rustty-core` `SurfaceMailbox`, `newConfig` to `rustty_core::app::new_config`. All 4 tests ported with identical assertions. |
| `src/apprt/gtk/App.zig` | `gtk/app.rs`, `gtk/build_info.rs` | ported | `must_draw_from_app_thread=true` as trait const; application_id/object_path in build_info; init/run/terminate/wakeup/performAction/performIpc/redrawInspector present. |
| `src/apprt/gtk/Surface.zig` | `gtk/surface.rs` | partial | All live methods ported. `cgroup()` (`Surface.zig:40-42`) has no direct counterpart — dead upstream (calls a nonexistent `cgroupPath`); see S47-F5 (refuted). |
| `src/apprt/gtk/adw_version.zig` | `gtk/adw_version.rs` | partial | BASELINE/getRuntimeVersion/logVersion/atLeast/runtimeAtLeast + the versionAtLeast test ported; comptime header fold dropped (S47-F3) and the five `supports*()` helpers deleted per AD-3 (S47-F2). |
| `src/apprt/gtk/cgroup.zig` | `gtk/cgroup.rs` | ported | Options, `fmtScope` (name retargeted per R2, bound asserted, NUL sentinel), `createScope` with identical `(ssa(sv)a(sa(sv)))` payload order, same bus/path/iface/method/reply type, `DbusCallFailed` + log. |
| `src/apprt/gtk/class.zig` | `gtk/class.rs` | replaced | The `Common(Self, Private)` GObject mixin is replaced by the gtk4-rs subclass framework; `class.rs` carries the mapping table + refAllDecls-style test; use-site behaviour lives in the per-class ports. |
| `src/apprt/gtk/ext.zig` | `gtk/ext.rs`, `ext/actions.rs`, `ext/slice.rs` | ported | boxedCopy/boxedFree (null-guarded), listFind, getAncestor, gValueHolds; actions + StringList re-exported; reify test plus an extra list_find test. |
| `src/apprt/gtk/flatpak.zig` | `gtk/flatpak.rs` | ported | comptime gate, isFlatpak, `/app/share/<app>`, `.flatpak-info` load-failure and Instance/app-path fallbacks, host_path, fallthrough to `os` resourcesDir. Product name retargeted per R2. |
| `src/apprt/gtk/gsettings.zig` | `gtk/gsettings.rs` | partial | Key enum + `Type()`/`GValueType()`/`requiresAllocation()` reified; non-allocating `get` as `get_bool`/`get_c_int`; all 4 tests ported. `getAlloc` missing (S47-F4, refuted — no oracle caller). |
| `src/apprt/gtk/gtk_version.zig` | `gtk/gtk_version.rs` | ported | BASELINE, getRuntimeVersion, logVersion, atLeast, runtimeAtLeast, runtimeUntil and both tests ported; `atLeast` lacks the comptime header fold (S47-F3). |
| `src/apprt/gtk/key.zig` | `gtk/key.rs` | ported | All 13 functions plus the 150-entry keymap with identical branches/ordering; all 5 oracle tests ported plus a round-trip test. `keyvalFromKey` is private (both external oracle users go through pub fns). |
| `src/apprt/gtk/media.zig` | `gtk/media.rs` | ported | fromFilename/fromResource/bellMediaFile/playBell/isForPath/mediaFileError; `accessAbsolute(.read)` → `File::open`; notify::error connected only when required. Regression test weakened (S48-F1); trailing unref → `mem::forget` (documented). |
| `src/apprt/gtk/portal.zig` | `gtk/portal.rs`, `portal/open_uri.rs` | ported | token_hex_len, generateToken, formatToken, getRequestPath, buildRequestPath, parseRequestPathToken and all 4 oracle tests plus 3 extra. Hex-parse and RNG-source notes filed as S48-F3/F4. |
| `src/apprt/gtk/post_fork.zig` | `gtk/post_fork.rs`, `rustty-termio/src/command.rs` | ported | `PostForkInfo` relocated to `RtPostForkInfo` (R4); `postFork` body arm-for-arm identical incl. 250 ms poll / 25 ms sleep and last-segment compare; hook installed via `install_rt_hooks`. |
| `src/apprt/gtk/pre_exec.zig` | `gtk/pre_exec.rs`, `rustty-termio/src/command.rs` | ported | `PreExecInfo` → `RtPreExecInfo`; `preExec` identical (127 exit codes, getpid, fmtScope, 250/25 ms loop); async-signal-safe cgroup reader with a byte-equality test against `rustty_os::cgroup::current`. |
| `src/apprt/gtk/weak_ref.zig` | `gtk/weak_ref.rs` | ported | `WeakRef(T)` newtype over `glib::WeakRef<T>`; `deinit` maps clear → `g_weak_ref_set(NULL)` (wrapper Drop owns the clear); the oracle test ported with the same three-step contract on a bare `glib::Object`. |
| `src/apprt/gtk/winproto.zig` | `gtk/winproto.rs` + `noop.rs`/`wayland.rs`/`x11.rs` | ported | Protocol enum, App union (ordered none→wayland→x11 chain) and Window union dispatched to every arm; `build_options.wayland/x11` → default-on cargo features; `*ApprtWindow` → `&impl IsA<gtk4::Window>` with internal downcast. |
| `src/apprt/gtk/build/blueprint.zig:1-146` | `rustty-apprt/build.rs:175-183,226-252,306-371,554-564` | replaced | Zig build-helper binary replaced by the build script; adwaita-version gate, `--version` not-found/non-zero/<0.16.0 gates and `compile --output` with stderr echo all present. Help text rebranded/shortened. |
| `src/apprt/gtk/build/gresource.zig:1-276` | `gtk/gresource.rs:95-244`, `build.rs`, `tests/gtk_assets.rs`, `tests/gtk_resources.rs` | partial | Manifest + comptime `blueprint()` resolver as const fn; XML generation with identical element shapes; existence assertions hoisted to tests. Manifest is 19 entries, not 21 (S49-F2, refuted under AD-3). |
| `src/apprt/gtk/build/info.zig:1-18` | `gtk/build_info.rs:61-130` | ported | base_application_id/application_id/resource_path/object_path; identity forked to `rustty_identity::BUNDLE_ID` (R2); Debug/ReleaseSafe split → `cfg(debug_assertions)`. |
| `src/apprt/gtk/class/application.zig:1-1744` | `gtk/class/application.rs`, `tests/gtk_application.rs` | ported | glibLogWriterFunction, 14-field Private, all lifecycle/perform-action arms, runtime+custom CSS, 26 accelerator entries, all startup/dispose/finalize/handler paths, global shortcuts. Findings S49-F1/F3/F4/F5. |
| `src/apprt/gtk/class/application.zig:1745-3488` | `gtk/class/application.rs` (many ranges), `gtk.rs:160-180` | ported | Every Action-namespace handler with the same target switch, log/return values and leaf calls; gio callbacks that the oracle routes through the core mailbox are collapsed to direct calls (verified pass-throughs). Findings S50-F1..F5. |
| `src/apprt/gtk/class/clipboard_confirmation_dialog.zig` | `gtk/class/clipboard_confirmation_dialog.rs` | ported | Properties, confirm/deny signals, template children, reveal/hide/notify callbacks, 6 verbatim msgids, response mapping. AD-3 collapse: 1.4 blueprint + switch read always live. dispose/finalize discharged by gtk4-rs + RefCell drop. |
| `src/apprt/gtk/class/close_confirmation_dialog.zig` | `gtk/class/close_confirmation_dialog.rs` | ported | Target enum + GType name, title/body strings (R2 rebrand), signals, response mapping, `present()`, blueprint 1.2, ensureType. Trivial `close()` forwarder absent (S50-F6, refuted — inherited `AdwDialogExt::close`). |
| `src/apprt/gtk/class/command_palette.zig` | `gtk/class/command_palette.rs`, `assets/ui/1.5/command-palette.blp` | ported | GhosttyCommandPalette + GhosttyCommand 1:1 incl. collectRegular/collectJump, isActionSupportedOnGtk, compareCommands with colon→tab normalisation and sort_key tiebreak, all 4 template callbacks. Blueprint byte-identical. Stale module doc (S51-F9). |
| `src/apprt/gtk/class/config.zig` | `gtk/class/config.rs` | partial | new/get/hasDiagnostics/diagnosticsBuffer/finalize + both properties + the oracle test ported. `getMut` missing (S51-F1); 4095-byte diagnostic truncation dropped (S51-F2). |
| `src/apprt/gtk/class/config_errors_dialog.zig` | `gtk/class/config_errors_dialog.rs`, `tests/gtk_config_errors_dialog.rs`, `assets/ui/1.2/config-errors-dialog.blp` | ported | config property with unref-old semantics, reload-config signal, response vfunc mapping only `reload`, dispose, present/close; blueprint byte-identical; headed KAT covers the response mapping. |
| `src/apprt/gtk/class/debug_warning.zig` | `gtk/class/debug_warning.rs`, `assets/ui/1.3/debug-warning.blp` | partial | adw.Bin + 1.3 template arm ported (blp differs only by product name). The `!supportsBanner()` gtk.Box/1.2 arm dropped under AD-3 (S51-F4, refuted). |
| `src/apprt/gtk/class/dialog.zig` | `gtk/class/dialog.rs` | partial | GhosttyDialog as subclassable adw.AlertDialog base with the response vfunc forwarded; present/close collapse to adw.Dialog present/forceClose. The adw.MessageDialog arm dropped under AD-3 (S51-F3, refuted). |
| `src/apprt/gtk/class/global_shortcuts.zig` | `gtk/class/global_shortcuts.rs` | ported | Properties, trigger/bind-failed signals, boxed BindFailed, init/close/refresh with the full map-build rules, Method payloads, onResponse, request/subscribe ordering, getRequestPath sanitisation, dispose, generateToken. Findings S51-F5..F8, F10. |
| `src/apprt/gtk/class/imgui_widget.zig` | `gtk/class/imgui_widget.rs`, `assets/ui/1.5/imgui-widget.blp` | ported | setup/render vfuncs as real class-struct vfuncs, queueRender, setCurrentContext, newFrame delta, keyEvent, translateMouseButton, all 13 template callbacks, 2× render loop, clear colour, 30 FPS throttle. Backend name rebranded per the naming transform. |
| `src/apprt/gtk/class/inspector_widget.zig` | `gtk/class/inspector_widget.rs`, `assets/ui/1.5/inspector-widget.blp` | ported | GType name preserved; surface property, init/dispose, imguiSetup/imguiRender, queueRender, setSurface freeze/notify/thaw sequence, surfaceWeakNotify. Pointer-compare replaced by handle disconnect (equivalent). |
| `src/apprt/gtk/class/inspector_window.zig` | `gtk/class/inspector_window.rs`, `gtk/weak_ref.rs`, `assets/ui/1.5/inspector-window.blp` | ported | adw.ApplicationWindow parent, WeakRef-backed surface + debug properties, init (devel css class, icon = bundle id per R2), dispose, new/present/queueRender, notify callback, ensureType. Only the `debug` pspec default differs (S52-F2). |
| `src/apprt/gtk/class/key_state_overlay.zig` | `gtk/class/key_state_overlay.rs`, `gtk/ext/slice.rs`, `assets/ui/1.2/key-state-overlay.blp` | ported | All properties, setters, getters and the four closures with identical join/visibility rules, onDragEnd valign flip, dispose/finalize. Notify ordering differs (S52-F3). |
| `src/apprt/gtk/class/resize_overlay.zig` | `gtk/class/resize_overlay.rs`, `assets/ui/1.2/resize-overlay.blp` | ported | duration/first-delay/halign/valign properties, label child, timers, setLabel, schedule/onIdle/onTimer/onDelayTimer with warn-on-failure removals, dispose + finalize. Property registered as `label-text` (S52-F1); timer armed in `constructed` (S52-F6, refuted). |
| `src/apprt/gtk/class/search_overlay.zig` | `gtk/class/search_overlay.rs`, `assets/ui/1.2/search-overlay.blp` | ported | All 7 properties, 4 signals, grabFocus, setSearchActive/Contents/Total/Selected with early returns and conditional has-* notifies, closureMatchLabel, key handling, onDragEnd, dispose/finalize. |
| `src/apprt/gtk/class/split_tree.zig:1-1088` (SplitTree) | `gtk/class/split_tree.rs:1-1053`, `assets/ui/1.5/split-tree.blp` | ported | Every property, signal, action, handler, idle source, freeze/thaw, empty-tree fast path, previous-then-next focus selection, errdefer restore, cross-tree move and detachWidget fallback present with identical side-effect ordering. |
| `src/apprt/gtk/class/split_tree.zig:1089-1516` (SplitTreeSplit) | `gtk/class/split_tree.rs:1055-1355`, `assets/ui/1.5/split-tree-split.blp` | ported | min==0 debug assert, max==0 early return, 0.001 tolerance, round(max*ratio), widget_to_tree warnings + resizeInPlace, the load-bearing defer ordering in onIdle, setSizeRequest(1,1), idle removal in dispose. Only the f64→f16 rounding path differs (S53-F1, refuted). |
| `src/apprt/gtk/class/surface.zig:1-1489` | `gtk/class/surface.rs`, `gtk/class/application.rs:4759`, `gtk/surface.rs:288` | ported | 24 properties, 9 signals, Private + Default, new/core/rt/setParent/redraw, closures, bindIsSplit, fullscreen/maximize/present, inspector control, key sequence/table actions, scrollbar, progress report, commandFinished, readonly, keyEvent, promptTitle, scaledCoordinates. `toggleCommandPalette` and `redrawInspector` relocated. |
| `src/apprt/gtk/class/surface.zig:1490-2978` | `gtk/class/surface.rs`, `gtk/surface.rs`, `assets/ui/1.2/surface.blp` | partial | Every declaration located and compared; blueprint callback/notify wiring identical. Gaps: `filterSnapPaths` deferred (S55-F1), setBellRinging gating (S55-F2), non-bool action state (S55-F3), mouse-shape GType (S55-F4). |
| `src/apprt/gtk/class/surface.zig:2979-4466` | `gtk/class/surface.rs` (many ranges) | ported | All 48 template callbacks, 16 template children, 24 properties, 9 signals; both oracle tests carried assertion-for-assertion; Request userdata → cloned-GObject closure capture. Findings S56-F1..F6. |
| `src/apprt/gtk/class/surface_child_exited.zig` | `gtk/class/surface_child_exited.rs` | ported | Banner variant fully ported (GType name, adw.Bin parent, 1.3 template, boxed `data` property, close-request, notify title/CSS logic, dispose_template). The noop variant is dead under the adw≥1.5 pin. `set_data(None)` notify differs (S56-F5). |
| `src/apprt/gtk/class/surface_scrolled_window.zig` | `gtk/class/surface_scrolled_window.rs` | ported | GType name preserved with the reference's three-t typo; kinetic-scroll workaround gated on `runtime_until(4,20,1)`; dispose unbind/unref, setSurface freeze/notify ordering, scrollbar-policy closure. No functional differences found. |
| `src/apprt/gtk/class/tab.zig` | `gtk/class/tab.rs`, `assets/ui/1.5/tab.blp`, `gtk/ext.rs`, `gtk/ext/actions.rs` | ported | All 7 properties, close-request signal, overrides struct, setParent(WithContext), 5-action map, promptTabTitle, all getters, prop callbacks, computed-title precedence, dispose/finalize, Class.init. Blueprint byte-identical; default title rebranded. |
| `src/apprt/gtk/class/title_dialog.zig` | `gtk/class/title_dialog.rs`, `assets/ui/1.5/title-dialog.blp`, `gtk/i18n.rs` | partial | Properties, `set` signal, new/present (parent fallback chain, prefill, heading, response), Target enum with GType + i18n titles ported; blueprint byte-identical. `dispose`/`dispose_template` missing (S57-F1). |
| `src/apprt/gtk/class/window.zig` | `gtk/class/window.rs` | ported | All 11 properties, Private fields, init/initActionMap (20 actions), tab lifecycle, syncAppearance/syncActions, all prop callbacks incl. the 50 ms is-active debounce, closures, dispose/finalize, windowRealize, all tab-view callbacks and 20 action handlers, Class.init. Findings S58-F1..F6. |
| `src/apprt/gtk/ext/actions.zig` | `gtk/ext/actions.rs` | ported | gActionNameIsValid as const fn, Action struct, init/initStateful/add/addToMap/addAsGroup with identical branching; both tests ported (the display-gated one as `#[ignore]`). |
| `src/apprt/gtk/ext/slice.zig` | `gtk/ext/slice.rs`, `gtk/ext.rs` | ported | Boxed GType registered under the same `GhosttyStringList` name; deep-copy via Clone, free via Drop; all 3 tests with equal assertions incl. the storage-pointer inequality check. |
| `src/apprt/gtk/ipc/DBus.zig` | `gtk/app.rs:171-244` | ported | Relocated (no gtk/ipc dir). Target switch, id/object-path validity checks with the same stderr messages, session-bus error path, `(sava{sv})` payload, `org.gtk.Actions.Activate` with no reply type, error → stderr + IPCFailed. Lossy conversions filed (S59-F1/F2). |
| `src/apprt/gtk/ipc/new_tab.zig` | `gtk/app.rs:267-310` | ported | Always `new-tab` with `(tas)`: u64 surface id + `as` args (empty when None); returns Ok(true). Argument bytes lossily converted (S59-F1). |
| `src/apprt/gtk/ipc/new_window.zig` | `gtk/app.rs:246-265` | ported | `new-window` vs `new-window-command` selection incl. Some(empty); `as` only when arguments present; Ok(true). Argument bytes lossily converted (S59-F1). |
| `src/apprt/gtk/ipc/toggle_quick_terminal.zig` | `gtk/app.rs:312-316` | ported | Sends `toggle-quick-terminal` with no parameters, returns Ok(true); rustty adds an IPCFailed-path test. |
| `src/apprt/gtk/portal/OpenURI.zig` | `gtk/portal/open_uri.rs`, `gtk/class/application.rs:3493`, `tests/gtk_portal_openuri.rs` | ported | Entry/Errors/init/setDbusConnection/deinit, start, subscribeToResponse, sendRequest, requestCallback, responseReceived and the 30 s cleanup timer all ported; Mutex → RefCell on the main loop, RequestData → closure captures. Findings S59-F3..F6. |
| `src/apprt/gtk/winproto/BlurRegion.zig` | `gtk/winproto/blur_region.rs` | ported | Slice/Pos, empty/deinit/eql/calcForWindow/approxRoundedRect with identical arithmetic, radius 15/0, CSD inset, fullscreen/maximized/tiled check, device-coordinate scaling. `Pos` is `i64` not `c_long` (S60-F7, refuted). |
| `src/apprt/gtk/winproto/noop.zig` | `gtk/winproto/noop.rs` | partial | All App and Window methods ported with the same return values. `Window.updateConfigEvent` absent — dead in the oracle too (S60-F4). Stale PORT NOTE (S60-F9). |
| `src/apprt/gtk/winproto/wayland.zig` | `gtk/winproto/wayland.rs` + `wayland/{globals,hotkeys,queue_source}.rs` | ported | App init/deinit/shortcuts/quick-terminal and Window init/deinit/resize/syncAppearance/decoration/blur/urgency/quick-terminal layout (anchor+margin order, KDE slide, monitor resolution) all present. Findings S60-F2/F3/F8. |
| `src/apprt/gtk/winproto/x11.zig` | `gtk/winproto/x11.rs`, `gtk/winproto.rs` | ported | App.init (prgname, program class, Xkb selection, both fatal paths), eventMods peek/decode, Window init/deinit/resize/syncAppearance, CSD predicate, syncBlur, syncDecorations with the `_MOTIF_WM_HINTS` cache, WINDOWID env, setUrgent, atom lookup. Findings S60-F1/F5/F6. |
| `src/apprt/gtk/winproto/wayland/Globals.zig` | `gtk/winproto/wayland/globals.rs`, `wayland.rs:195-225` | ported | Tag enum (7 members), State, two-phase roundtrip in App::init, typed accessors, registry global/global_remove with re-advertise destroy-old, deco/bg/output-order listeners. Findings S61-F1/F2/F3. |
| `src/apprt/gtk/winproto/wayland/Hotkeys.zig` | `gtk/winproto/wayland/hotkeys.rs`, `wayland/vicinae.rs` | ported | Entry, deinit/bind/fail, init/deinit/clear/bind with leader/global/catch-all/single-action filtering, hotkeyListener bound/denied/revoked/pressed/released. Emissions deferred out of the dispatch borrow (same main-loop turn). Reference has no tests; rustty adds 5. |
| `src/benchmark/ApcParser.zig` | `rustty-bench/src/apc_parser.rs` | ported | Options, Handler callbacks, create/destroy/benchmark, setup/teardown/step with 64 KiB chunks, the `once` test. |
| `src/benchmark/Benchmark.zig` | `rustty-bench/src/benchmark.rs` | ported | init/run with setup, drop-guard teardown, macOS signpost interval, once/duration exit, iteration+duration accounting; RunMode/RunResult/Error/VTable defaults; the oracle test keeps all 4 assertions (S61-F6, refuted). |
| `src/benchmark/CApi.zig` | `rustty-bench/src/capi.rs`, `include/rustty-embed.h:1276`, `macos/Tests/BenchmarkTests.swift:28` | ported | Exported as `rustty_benchmark_cli` (workspace rebrand); unknown-action warn + false, mainAction failure warn + false, true on success. Failure log prints the Rust variant name (S61-F8). |
| `src/benchmark/CodepointWidth.zig` | `rustty-bench/src/codepoint_width.rs` | ported | Options defaults, 4 modes, step dispatch, setup/teardown, all four step bodies incl. the Windows wcwidth warn, the test. |
| `src/benchmark/GraphemeBreak.zig` | `rustty-bench/src/grapheme_break.rs` | ported | Options defaults, both modes, decode-only noop, table step with cp1 carry, the test. |
| `src/benchmark/HyperlinkMap.zig` | `rustty-bench/src/hyperlink_map.rs` | ported | Options defaults, create validation (≥16, power of two, capacity), page init sizing, implicit-1 insert + example URI, load clamp, lookup/churn steps, the test over both modes. |
| `src/benchmark/IsSymbol.zig` | `rustty-bench/src/is_symbol.rs` | ported | Options defaults, both modes, setup/teardown, table step, the test. The `uucode` mode reproduces the same embedded table (R3 — no live uucode dep), so both modes now time the same lookup. |
| `src/benchmark/OscParser.zig` | `rustty-bench/src/osc_parser.rs` | ported | Options, create/destroy, setup, LE length-prefixed step with EOF/ReadFailed/`>4096` handling, BEL end + reset, the test. |
| `src/benchmark/PageCompression.zig` | `rustty-bench/src/page_compression.rs` | ported | 64 MiB cap, Options defaults, 5 modes, create/destroy/benchmark, setup validation, prepareStored/prepareEncoded with round-trip verify, all 5 steps incl. store's ring semantics, PageIterator, percentage; both tests plus one extra. Report goes to stdout (S61-F9). |
| `src/benchmark/ScreenClone.zig` | `rustty-bench/src/screen_clone.rs`, `rustty-bench/src/lib.rs:69-89` | ported | Options defaults, 6 modes, create/destroy, setup (fullReset, SGR + rows of `hello`, data-file streaming), all six step bodies with identical iteration counts and the partial-render pin/markDirty. No test in either tree. |

**Partially-read units: none.** Every shard reported an empty `units_partial` list; all 76 oracle units in scope were read end to end (shard 46 additionally read past the nominal range to finish `ecMouseMotion`, and shard 49 read to `:1784` to finish `globalShortcutTrigger`). Rows marked *partial* above are counterpart-completeness statuses, not reading gaps, and each names the missing declaration.

## Findings

No findings were graded **high**. 16 findings are **medium** (all CONFIRMED), 66 are **low** (47 CONFIRMED, 19 DISPUTED). Refuted candidates are listed in the final section.

### Medium

#### S45-F1 — checkGhosttyHEnum ports are tautological: the "header" side is synthesized from the Rust enum itself, not read from rustty-embed.h

- **Status:** ✅ CONFIRMED · **Kind:** weakened-assertion · **Severity:** medium
- **Oracle:** `src/apprt/action.zig:25-27, 432-434, 513-515, 529-531, 540-542, 556-558, 591-593, 601-603, 611-613, 622-624, 662-664, 671-673, 680-682, 691-693, 958-960, 988-990, 1064-1066`
- **Rustty:** `crates/rustty-apprt-api/src/action_tests.rs:15-35` (run_henum) and every `ghostty_h_*` test `41-283`; driver + `crates/rustty-lib/src/enum.rs:124-184`

The reference `lib.checkGhosttyHEnum` (`src/lib/enum.zig:108-165`) imports the real `ghostty.h` and asserts, for every enum key, that `GHOSTTY_<PREFIX><KEY>` is declared in the header with the same integer value, and that no key is left unmatched. The rustty `run_henum` helper builds an `EnumType` from the same key list passed by the test and then answers the header lookup closure by finding the name in that very `EnumType`, so the "declared C value" is always equal to the Rust value by construction and the test can never fail regardless of what `include/rustty-embed.h` says. Only `TargetKey` and `ActionKey` receive a real header-mirroring ordinal check elsewhere (`crates/rustty-apprt-c/tests/abi_layout.rs:255-256, 266-267, 302-372, 656-677`). The other 15 enums (SplitDirection, GotoSplit, GotoWindow, ResizeSplit.Direction, Fullscreen, FloatWindow, SecureInput, Inspector, QuitTimer, Readonly, MouseVisibility, PromptTitle, OpenUrl.Kind, CloseTabMode, OpenConfig) have no assertion anywhere in the workspace binding their discriminants to the header. Manual comparison shows the values currently DO match, so this is a test-strength gap, not a live value divergence.

**Evidence:** `action_tests.rs:21-35`: `let opts = keys...; let t = enum_type(EnumTarget::C, &opts); let fields = t.fields.clone(); check_ghostty_henum(&t, prefix, |name| fields.iter().find_map(...))` — the closure answers from `t.fields`, the same data being checked. Reference `enum.zig:118`: `const c = @import("ghostty.h");` then `@hasDecl(c, expected_name)` / `expectEqual(field.value, @field(c, expected_name))`.

**Suggested fix:** Have the test (or a build.rs/xtask gate) parse `include/rustty-embed.h` and feed the parsed `RUSTTY_<PREFIX><KEY> = value` map into `check_ghostty_henum`'s closure, or add hard-coded header-mirroring `const _: () = assert!(Enum::Variant as c_int == N)` pins for the 15 remaining enums in `abi_layout.rs` the way ActionKey/TargetKey are pinned.

#### S46-F1 — App.keymap is eagerly initialized in App::init instead of lazily on first keyboardLayout()

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/apprt/embedded.zig:140-171, 229-266`
- **Rustty:** `crates/rustty-apprt/src/embedded.rs:157-158, 346-387, 473-513`

The oracle at the pin declares `keymap: ?input.Keymap` and initializes it LAZILY inside `keyboardLayout()` (`embedded.zig:249-256`) because the first TIS call is slow; `init` stores `null`, `terminate` deinits only if set, `reloadKeymap` is a no-op when never initialized (`:235`), and a keymap init failure only logs a warning and returns `.unknown` (`:250-253`). rustty's `App` holds a non-optional `keymap: Keymap`, calls `Keymap::init()` inside `App::init` (`embedded.rs:364-372`) and propagates the error, so `rustty_app_new` returns null and the host cannot create the app at all; `reloadKeymap` unconditionally reloads (`embedded.rs:473-476`); `keyboardLayout` has no init-failure arm.

**Evidence:** oracle `embedded.zig:147` `keymap: ?input.Keymap,` / `:164` `.keymap = null` / `:235` `if (self.keymap) |*v| try v.reload();` / `:249-256` lazy init with `log.warn(...); return .unknown;` vs rustty `embedded.rs:158` `pub keymap: Keymap` / `:364` `let keymap = match Keymap::init() { Ok(..) => .., Err(err) => { config_clone.deinit(); return Err(err); } }` / `:475` `self.keymap.reload()`; `lib.rs:441/:514` `AppNewError::Keymap` makes `rustty_app_new` return null.

**Suggested fix:** Make `keymap: Option<Keymap>`; init to None; terminate deinits if Some; reloadKeymap reloads only if Some; keyboardLayout lazily inits (on Err: return `Layout::Unknown`), matching `embedded.zig:229-266`. Drop `KeymapError` from `App::init` / `AppNewError`.

#### S46-F2 — KeyEvent.core does not zero an unshifted_codepoint that does not fit in u21

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/apprt/embedded.zig:116-119`
- **Rustty:** `crates/rustty-apprt/src/embedded.rs:308-315`

The oracle converts the host's u32 `unshifted_codepoint` with `std.math.cast(u21, ...) orelse 0`, so any value > 0x1FFFFF becomes 0 before it reaches keybind matching. rustty models u21 as u32 (`rustty_vt::input::key::u21 = u32`, `key.rs:161`) and assigns the raw value through, so an out-of-range value propagates into the core KeyEvent. Downstream, `crates/rustty-input/src/binding.rs:4699-4700` takes the unicode-trigger branch (which can never match) where the oracle's 0 falls through to the physical-key trigger path, and `key.rs:106` hashes the differing value.

**Evidence:** oracle `:116-119` `const unshifted_codepoint: u21 = std.math.cast(u21, self.unshifted_codepoint) orelse 0;` vs rustty `embedded.rs:315` `let unshifted_codepoint: key::u21 = self.unshifted_codepoint;` with a PORT NOTE claiming the cast is the identity (it is not: u21 max is 0x1FFFFF).

**Suggested fix:** `let unshifted_codepoint = if self.unshifted_codepoint <= 0x1F_FFFF { self.unshifted_codepoint } else { 0 };`

#### S47-F1 — ipc header-sync tests look the C values up in the enum itself, never in the header

- **Status:** ✅ CONFIRMED · **Kind:** weakened-assertion · **Severity:** medium
- **Oracle:** `src/apprt/ipc.zig:27-29, 181-183`
- **Rustty:** `crates/rustty-apprt-api/src/ipc.rs:372-420`

The reference tests `ghostty.h Target.Key` / `ghostty.h Action.Key` call `lib.checkGhosttyHEnum`, which reflects over the generated `ghostty.h` and fails if a key is missing from the header or has a different value. The rustty port passes a lookup closure that searches `t.fields` (the enum descriptor under test), so the closure always returns the enum's own value: the test cannot detect a drifted or missing `RUSTTY_IPC_TARGET_*` / `RUSTTY_IPC_ACTION_*` declaration. `crates/rustty-vt/src/mouse.rs:288-296` shows the intended pattern — `header_decls(HEADER_SRC)` parses the checked-in header text and feeds that to the closure, with an explicit PORT NOTE that declarations must never come from the descriptor under test.

**Evidence:** `ipc.rs:375-387` and `:408-419` clone `t.fields` and re-derive `RUSTTY_IPC_TARGET_<NAME>` from those same fields; `include/rustty-embed.h:1102-1103, 1127-1129` declare the enumerators but are never read.

**Suggested fix:** `include_str!` `include/rustty-embed.h` and parse the `RUSTTY_IPC_TARGET_*` / `RUSTTY_IPC_ACTION_*` enumerators as `mouse.rs`'s `header_decls` does, then pass that map to `check_ghostty_henum`. (The same tautology affects `action_tests.rs` — see S45-F1.)

#### S48-F1 — bellMediaFile reuse test asserts path equality instead of object identity

- **Status:** ✅ CONFIRMED · **Kind:** weakened-assertion · **Severity:** medium
- **Oracle:** `src/apprt/gtk/media.zig:148-154` (unref at `:161`)
- **Rustty:** `crates/rustty-apprt/src/gtk/media.rs:147-149` (mem::forget at `:181`)

The oracle test captures `const first = current;` and, after the same-path call, asserts `testing.expectEqual(first, current)` — pointer identity of the cached MediaFile, which is exactly the leak regression being guarded (a rebuild for the same path would fail it). The port's same-path step asserts only `is_for_path(&current, &path_a)`, which a rebuilt MediaFile targeting the same path would also satisfy, so the test no longer catches the regression it documents. The oracle's trailing `current.unref()` is also replaced with `std::mem::forget` (documented GstPlay teardown race), so the final-drop path is not exercised.

**Evidence:** `media.zig:149` `const first = current;` / `:153-154` `try testing.expectEqual(first, current);` vs `media.rs:148-149` `let current = bell_media_file(Some(current.clone()), &path_a, false).unwrap(); assert!(is_for_path(&current, &path_a));` — no identity comparison against the first handle anywhere in the test module.

**Suggested fix:** Keep `let first = current.clone();` before the same-path call and `assert_eq!(first, current);` (glib object handles compare by pointer), matching `media.zig:154`.

#### S49-F1 — SIGUSR2 hardcoded to 12; wrong signal number on FreeBSD (a gated target)

- **Status:** ✅ CONFIRMED · **Kind:** different-constant · **Severity:** medium
- **Oracle:** `src/apprt/gtk/class/application.zig:1465-1473`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/application.rs:1712` (const) and `:783-800` (use site)

The oracle installs the config-reload handler on `std.posix.SIG.USR2`, which resolves per-OS. rustty inlines `const SIGUSR2: c_int = 12`, which is only correct on Linux. The GTK apprt is compiled for `any(target_os = "linux", target_os = "freebsd")` (`lib.rs:55`, `build.rs:209-212`, all three `tests/gtk_*.rs`), and its own PORT NOTE (`lib.rs:49-54`) states FreeBSD builds and launches the same GTK application. On FreeBSD SIGUSR2 is 31 while 12 is SIGSYS, so rustty installs the reload handler on SIGSYS and a real SIGUSR2 takes its default action (terminate) instead of reloading the configuration. No per-target cfg variant exists anywhere in the workspace.

**Evidence:** `application.zig:1468-1469` `glibunix.signalAdd(@intFromEnum(std.posix.SIG.USR2), handleSigusr2, self)`; `application.rs:1712` `const SIGUSR2: core::ffi::c_int = 12;` with a comment claiming "all glibc targets rustty builds for … is 12", contradicted by the module's own cfg gate.

**Suggested fix:** Use `libc::SIGUSR2`, or a `#[cfg(target_os = "freebsd")] const SIGUSR2 = 31;` / `#[cfg(target_os = "linux")] const SIGUSR2 = 12;` pair, so the value follows the target.

#### S50-F1 — gotoWindow: start node is skipped / found differently from the reference

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/apprt/gtk/class/application.zig:2476-2519` (the shard's cited 2067-2101 is a stale pin offset)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/application.rs:3725-3757`

The reference starts its forward/backward walk AT the `starting` node inclusive (`var current_ = starting; while (current_) |node| ... gotoWindowMaybe`), where `starting` is the first *active* toplevel of ANY `gtk.Window` type (`glist.findCustom(null, findActiveWindow)`) or, if no toplevel is active, the list head. The active node is only skipped because `gotoWindowMaybe` rejects active windows. The port first filters the toplevel list down to Ghostty `Window`s, then computes `start` as the active Ghostty window else index 0, then unconditionally skips `idx == start`. Two observable divergences: (1) with no active toplevel and exactly one window, the oracle presents the head window and returns true while the port returns false (with several windows it presents a different window); (2) when the active toplevel is a non-Ghostty `gtk.Window` (GTK inspector, native dialogs), the reference anchors the ring walk at that node's list position while the port anchors at index 0.

**Evidence:** `application.zig:2480-2485` `const starting = glist.findCustom(null, findActiveWindow) orelse glist;` and `:2488-2496` first loop begins with `current_ = starting`; `application.rs:3727-3740` filters to `Window` and takes `position(is_active).unwrap_or(0)`; `:3751-3753` `if idx == start { continue; }`.

**Suggested fix:** Keep the full toplevel list, find `start` via `is_active` over that list (default 0), include the start node in the first pass (letting `goto_window_maybe` reject it only when active), then wrap exactly as the reference does, stopping before revisiting `start`.

#### S52-F1 — ResizeOverlay GObject property registered as `label-text` instead of `label`

- **Status:** ✅ CONFIRMED · **Kind:** different-constant · **Severity:** medium (presence lens suggested low; semantic lens kept medium)
- **Oracle:** `src/apprt/gtk/class/resize_overlay.zig:77-91, 349-355`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/resize_overlay.rs:62-65, ~200-212`

The oracle registers the text property under the GObject name `"label"` (`pub const name = "label"`, installed at `:351`) and `setLabel` notifies that pspec. The rustty port derives the property from the field `label_text` with no `name = ...` override, so glib-macros registers it as `"label-text"` and `set_label` notifies `notify::label-text`. Any `g_object_get/set(overlay, "label", ...)`, `notify::label` connection, or GtkBuilder/expression binding to `template.label` would fail on the port. Neighbouring properties in the same struct DO carry explicit `name = "first-delay"` / `name = "overlay-halign"`, so the omission is not a deliberate convention. The vendored `resize-overlay.blp` binds only `overlay-halign`/`overlay-valign` and `surface.rs` uses the `set_label()` wrapper, so no in-tree caller is affected — the public property surface differs.

**Evidence:** oracle `resize_overlay.zig:79` `pub const name = "label";` and `:351` `properties.label.impl`; rustty `resize_overlay.rs:64-65` `#[property(get, set, nullable)] pub label_text: RefCell<Option<String>>`.

**Suggested fix:** Add `name = "label"` to the `#[property(...)]` attribute on `label_text` (keeping the field name to avoid clashing with the `label` TemplateChild) and keep `set_label()` delegating to the generated setter.

#### S54-F1 — setParent inherits pwd from the core terminal instead of the parent widget's `pwd` property

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/apprt/gtk/class/surface.zig:806-815` (getter at `:2125-2128`)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:1318-1328`

The oracle inherits the working directory from `parent.rt_surface.surface.getPwd()`, i.e. the parent GhosttySurface WIDGET's `priv.pwd`, which is populated both by the `.pwd` apprt action (OSC 7) and by a prior inheritance at creation time. rustty calls `parent_ref.pwd(alloc)` — `rustty_core::surface::Surface::pwd` (`rustty-core/src/surface.rs:5196`), which locks the renderer mutex and reads `io.terminal.get_pwd()`. For a parent surface that itself inherited a pwd but whose shell has not yet emitted OSC 7, the oracle inherits the parent widget's pwd while rustty inherits nothing. The pwd is consumed to set the child process working directory, so this is a user-visible cwd difference. Additionally rustty converts via `String::from_utf8(pwd).ok()`, silently dropping a non-UTF-8 pwd (the oracle's `glib.ext.dupeZ` keeps the bytes), and the renderer-mutex lock from the main thread is a new side effect.

**Evidence:** `surface.zig:813` `if (parent.rt_surface.surface.getPwd()) |pwd| { priv.pwd = glib.ext.dupeZ(u8, pwd); notify }`; `surface.rs:1323-1326` `if let Some(pwd) = parent_ref.pwd(alloc) { imp.pwd.replace(String::from_utf8(pwd).ok()); self.notify("pwd"); }` with `parent_ref: &CoreSurface`.

**Suggested fix:** Read the parent widget's `pwd` property (via `parent.rt_surface` → widget backref → `imp().pwd.borrow().clone()`) instead of `CoreSurface::pwd`, and keep raw bytes (or at least `from_utf8_lossy`) rather than dropping non-UTF-8 values.

#### S55-F1 — filterSnapPaths (snap env sanitising in defaultTermioEnv) not ported

- **Status:** ✅ CONFIRMED · **Kind:** missing-function · **Severity:** medium
- **Oracle:** `src/apprt/gtk/class/surface.zig:1643-1650, 1667-1740` (shard cited 1625-1630, 1648-1720)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:2517-2584`

The oracle's `defaultTermioEnv`, when built with `build_config.snap` and `SNAP` is set, calls `filterSnapPaths`, which (a) removes every `SNAP_*` variable (except TERMINFO/GHOSTTY*-prefixed keys), and (b) strips any colon-separated path component of every other variable that starts with the value of `SNAP`/`SNAP_USER_COMMON`/`SNAP_USER_DATA`/`SNAP_DATA`/`SNAP_COMMON`, removing the variable entirely if no components remain. rustty has the `snap` build option (`rustty-build-config/src/build.rs:120`, `lib.rs:226`) but `default_termio_env` carries a PORT NOTE "deferred" and never filters; grep for `SNAP_USER_COMMON`/`filter_snap` across `crates/` returns nothing. This is not unreachable: `rustty-config/Cargo.toml:50` declares the feature and rustty's own `snap/snapcraft.yaml:97` builds with `--features "gtk wayland x11 snap"`, so a shipped configuration leaks `SNAP_*` and snap-prefixed `PATH`/`LD_LIBRARY_PATH` entries to child shells — exactly what the oracle prevents so that further snaps can be launched.

**Evidence:** `surface.zig:1643-1649` `if (comptime build_config.snap) { if (env.get("SNAP") != null) try filterSnapPaths(alloc, &env); }`; `surface.rs:2521-2524` "PORT NOTE (deferred): the snap-path filtering … lands with its own tier".

**Suggested fix:** Port `filterSnapPaths` as `fn filter_snap_paths(env: &mut EnvMap)` gated on `rustty_build_config::snap()` and `env.get(b"SNAP").is_some()`, with the same key skip-list, `SNAP_` removal, and colon-split prefix filtering (join with ':' or remove when empty), applying updates then removals in that order; add a unit test over a synthetic EnvMap.

#### S55-F2 — setBellRinging's change-gated notify and freeze/thaw are lost: bell-ringing notifies on every write

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/apprt/gtk/class/surface.zig:2296-2316` (contract documented at `:61-71`)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:200-202, 1624-1648, 2896-2903, 3296`

Oracle `setBellRinging(ringing)` wraps everything in `freezeNotify`/`thawNotify` and only assigns+notifies when `priv.bell_ringing != ringing` — the property doc explicitly promises a notify only on a full state change. rustty replaced the method with the glib::Properties-derived setter (plain `#[property(get, set)]` on a `Cell<bool>`, no custom setter, no `EXPLICIT_NOTIFY`), which sets unconditionally and lets `g_object_set_property` emit `notify::bell-ringing` every time. `update_focus` (`:2902`) calls `set_bell_ringing(false)` on every focus-in and `mouse_down` (`:3296`) on every button press, so listeners receive spurious notifies while the value is unchanged. `ring_bell` also drops the freeze/thaw, so notifies raised by `bell` signal handlers / ring-bell actions are no longer coalesced around the state write. The in-file comment at `:2896-2899` claiming "set + notify-on-change" is factually wrong. (S54-F2 reports the same defect from the `commandFinished` entry point.)

**Evidence:** `surface.zig:2299-2300` `freezeNotify(); defer thawNotify();`, `:2311-2313` `if (priv.bell_ringing == ringing) return; priv.bell_ringing = ringing; notifyByPspec(...)`; glib-macros-0.22.6 `properties.rs:518-525/684-686` expands the default setter to an unconditional `set_property_from_value`.

**Suggested fix:** Give `bell-ringing` a custom setter (`#[property(get, set = Self::set_bell_ringing_prop)]` or an inherent `set_bell_ringing`) that wraps in `freeze_notify()`/`thaw_notify()`, returns early when the Cell already holds the value, and only then sets + `notify_bell_ringing()`; route `ring_bell` through the same helper so the freeze/thaw covers the signal/action fan-out.

#### S57-F1 — TitleDialog has no dispose override calling dispose_template

- **Status:** ✅ CONFIRMED · **Kind:** missing-function · **Severity:** medium
- **Oracle:** `src/apprt/gtk/class/title_dialog.zig:151-161` (registered at `:210`)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/title_dialog.rs:101-112`

The oracle implements `dispose` for GhosttyTitleDialog, calling `gtk.Widget.disposeTemplate(self, getGObjectType())` before chaining to the parent dispose. The rustty `impl ObjectImpl for TitleDialog` only defines `signals()`; grep for `dispose` in the whole file returns nothing. gtk4-rs 0.11.4 does NOT call `gtk_widget_dispose_template` automatically (`CompositeTemplateDisposeExt::dispose_template` is an explicit-call-only trait method; `window.rs:218-220` in this very crate says so), and 15+ other templated classes in the crate call it explicitly (`tab.rs:136`, `split_tree.rs:251`, `surface.rs:762`, `window.rs:359`, …). The class does bind a `#[template_child] entry`, so its template child is never released by the template machinery on dispose — template teardown differs from the reference (possible GTK "still has children" warnings / delayed release).

**Evidence:** `title_dialog.zig:151-161` `fn dispose(self) { gtk.Widget.disposeTemplate(...); gobject.Object.virtual_methods.dispose.call(Class.parent, ...) }`; `title_dialog.rs:63-77` template + template_child with no dispose anywhere in the file.

**Suggested fix:** Add `fn dispose(&self) { self.dispose_template(); }` to `impl ObjectImpl for imp::TitleDialog` (parent dispose is chained by gtk-rs), mirroring `tab.rs:134-140`.

#### S58-F1 — About dialog drops the `website` and `issue-url` properties

- **Status:** ✅ CONFIRMED · **Kind:** missing-branch · **Severity:** medium (semantic lens medium; presence lens argued low as an R2 consequence)
- **Oracle:** `src/apprt/gtk/class/window.zig:1963-2005` (shard cited 1790-1832)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/window.rs:2174-2209`

`actionAbout` passes six properties to `adw.showAboutDialog`: application-name, developer-name, application-icon, version, issue-url and website. rustty's `action_about` builds `adw::AboutDialog` with only application_name, application_icon, developer_name and version; the website link and the issue-tracker link are absent from the shipped About dialog (user-visible). The PORT NOTE justifies this by R2 (no rustty homepage/issue-url constant), and grep confirms `rustty_identity` has no WEBSITE/ISSUE_URL (only `COMMIT_URL_PREFIX`), so the functionality is genuinely absent workspace-wide rather than relocated.

**Evidence:** `window.zig:1963-1988` supplies `"issue-url"`/`"website"` key-value pairs; `window.rs:2201-2207` `adw::AboutDialog::builder().application_name(..).application_icon(..).developer_name(..).version(..).build()` has no `.website()`/`.issue_url()`; `window.rs:2178-2181` comment: "The reference's `website` … and `issue-url` … are DROPPED".

**Suggested fix:** Add WEBSITE and ISSUE_URL constants to `rustty_identity` (per profile) and pass `.website(..)` / `.issue_url(..)` to the AboutDialog builder.

#### S59-F1 — IPC CLI arguments are converted with String::from_utf8_lossy instead of sent as raw bytes

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** medium
- **Oracle:** `src/apprt/gtk/ipc/new_window.zig:47-56`, `src/apprt/gtk/ipc/new_tab.zig:52-61`
- **Rustty:** `crates/rustty-apprt/src/gtk/app.rs:257-262, 297-302`

The oracle wraps each argument's raw bytes in an `"s"` variant via `glib.Variant.newFromBytes(s, bytes, trusted=true)` with no UTF-8 conversion. rustty maps every argument through `String::from_utf8_lossy`, so a non-UTF-8 argument (e.g. a Latin-1 filename passed to `-e`) is silently rewritten with U+FFFD and the IPC SUCCEEDS, forwarding corrupted arguments to the running instance. Whatever the D-Bus layer does with the oracle's raw bytes, the byte sequence the remote instance receives differs observably, and no PORT NOTE documents the divergence. This also violates the project's byte-slice → `Vec<u8>` fidelity rule (arguments are `&[&[u8]]` in `rustty-apprt-api`).

**Evidence:** `new_window.zig:49-54` `for (arguments) |argument| { const bytes = glib.Bytes.new(argument.ptr, argument.len + 1); ... newFromBytes(s_variant_type, bytes, true) }` vs `app.rs:257-262` `.map(|a| String::from_utf8_lossy(a).into_owned())` (and identically for new-tab).

**Suggested fix:** Keep bytes: build the `as` array with `glib::Variant::from_bytes_with_type` on the raw bytes as the oracle does, or at minimum return IPCFailed with the oracle's stderr message when an argument is not valid UTF-8 rather than silently mangling it.

#### S59-F3 — A Response signal with an empty tuple body panics the GTK main loop (oracle logs and continues)

- **Status:** ✅ CONFIRMED · **Kind:** missing-error-path · **Severity:** medium
- **Oracle:** `src/apprt/gtk/portal/OpenURI.zig:478-482`
- **Rustty:** `crates/rustty-apprt/src/gtk/portal/open_uri.rs:495`

The oracle reads the signal parameters with `params.get("(u@a{sv})", ...)`: on a malformed body `g_variant_get` emits a GLib critical and leaves `response = 0`, so the handler logs "open uri successful" and returns. rustty calls `params.child_value(0)`, which in glib-rs 0.22.8 (`variant.rs:312-316`) asserts `is_container()` and `index < n_children()`; any bus peer that sends a `Response` signal on our unique-name request path with an empty tuple `()` body therefore panics inside the D-Bus signal dispatch on the GTK main loop, aborting the application. The subscription passes `sender = null` (`OpenURI.zig:202-212`), so any peer can reach the handler. The documented `unwrap_or(2)` divergence only covers a wrong-typed first child, not a missing one.

**Evidence:** `OpenURI.zig:478-481` `var response: u32 = 0; ... params.get("(u@a{sv})", &response, &results);` vs `open_uri.rs:495` `let response: u32 = params.child_value(0).get().unwrap_or(2);`.

**Suggested fix:** Guard with `if params.n_children() == 0 { … }` (or use `params.try_child_value(0)`) before reading the response code.

#### S60-F1 — X11 syncBlur commits the new blur_region cache even when XChangeProperty/XDeleteProperty fail

- **Status:** ✅ CONFIRMED · **Kind:** missing-error-path · **Severity:** medium
- **Oracle:** `src/apprt/gtk/winproto/x11.zig:243-287` (assignment at `:285-286`)
- **Rustty:** `crates/rustty-apprt/src/gtk/winproto/x11.rs:562-607`

The oracle uses `try self.changeProperty(...)` / `try self.deleteProperty(...)`: on `error.RequestFailed` the function returns before `self.blur_region = region`, so the cache stays at the OLD region, the next resize/appearance sync re-attempts the write, and the caller logs `failed to sync blur=`. rustty does `let _ = self.delete_property(...)` / `let _ = unsafe { self.change_property(...) }` and then unconditionally assigns `self.blur_region = region`. The helpers really do surface failure (`x11.rs:744-779` return `Err(Error::RequestFailed)` when the Xlib status is 0), so after one failed X request every subsequent sync hits the `region.eql(&self.blur_region)` early-return (`x11.rs:582`) and the blur property is never written or deleted again until the region changes. The `failed to sync blur` error log is dropped too.

**Evidence:** `x11.zig:272-286` `try self.changeProperty(...)` … `self.blur_region = region;` vs `x11.rs:588, 596-603, 606`.

**Suggested fix:** Make `sync_blur` return `Result` (or track success); only assign `self.blur_region = region` when the property write/delete succeeded, and log `err(gtk-x11): failed to sync blur={err}` on failure as the oracle's `resizeEvent`/`syncAppearance` do.

### Low

#### S45-F2 — Display formatters drop the fully-qualified type-name prefix used by the reference `format`

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/action.zig:645-654, 754-760, 778-784, 806-817`
- **Rustty:** `crates/rustty-apprt-api/src/action.rs:982-991, 1096-1103, 1126-1130, 1158-1167`

The reference `format` functions print `@typeName(@This())`, the fully-qualified Zig type name (e.g. `apprt.action.SetTitle{ foo }`). The Rust `Display` impls print the bare type name (`SetTitle{ foo }`, `ExportTerminalIO{ contents: 12 bytes }`, `Pwd{ … }`, `DesktopNotification{ … }`). Only observable in log output that formats these payloads.

**Evidence:** `action.zig:754-760` `try writer.print("{s}{{ {s} }}", .{ @typeName(@This()), value.title });` vs `action.rs:1102` `write!(f, "SetTitle{{ {} }}", String::from_utf8_lossy(self.title))`.

**Suggested fix:** Emit the qualified name (e.g. `apprt.action.SetTitle`) in the four Display impls if log-text parity is required; otherwise document the deliberate divergence.

#### S45-F3 — SearchTotal/SearchSelected cval: `@intCast` (checked) became `as isize` (wrapping)

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/action.zig:1034-1038, 1049-1053`
- **Rustty:** `crates/rustty-apprt-api/src/action.rs:1479-1487, 1502-1510`

The reference converts `?usize` to `isize` with `@intCast`, a safety-checked cast (panics in safe builds above `isize::MAX`). The Rust port uses `t as isize` / `s as isize`, which silently wraps to a negative value. Only observable for match counts/indices > `isize::MAX`.

**Evidence:** `action.zig:1036` `.total = if (self.total) |t| @intCast(t) else -1,` vs `action.rs:1482-1485` `total: match self.total { Some(t) => t as isize, None => -1 }`.

**Suggested fix:** Use `isize::try_from(t).expect(...)` to mirror the checked-cast semantics.

**Refuting lens — semantic (high confidence):** "Read both regions: oracle src/apprt/action.zig:1034-1038 / 1049-1053 use `@intCast` (safety-checked) on `?usize -> isize`; port crates/rustty-apprt-api/src/action.rs:1479-1487 / 1502-1510 use `t as isize` / `s as isize`. The two differ only for values > isize::MAX (>= 2^63 on 64-bit), i.e. a search match count or match index exceeding nine quintillion, which cannot be constructed — every match/index originates from terminal screen contents bounded by addressable memory, and the value would also have to be produced by a Some(..) path. The difference is therefore confined to unreachable input; for every reachable value both produce the identical isize, and the None -> -1 sentinel path is identical. Refuted as a non-observable difference (a `try_from().unwrap()` would be a nicer mirror but changes nothing observable)."

#### S46-F3 — rustty_app_set_color_scheme / rustty_surface_set_color_scheme emit a stderr diagnostic on an invalid scheme; oracle is silent

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/embedded.zig:1788, 1992`
- **Rustty:** `crates/rustty-apprt-c/src/lib.rs:785-791, 1675-1681`

At the pin both exports use `std.enums.fromInt(apprt.ColorScheme, scheme_raw) orelse return;` with no log (the `intToEnum` + `log.warn` form the rustty comment quotes is a pre-pin revision). rustty prints `invalid color scheme to rustty_*_set_color_scheme value=..` via `eprintln!`, the same stale-revision class already fixed for `rustty_surface_mouse_pressure` in W22-B02-DIAG-01.

**Evidence:** oracle `:1788` and `:1992` `orelse return;` vs `lib.rs:789` and `:1679` `eprintln!("invalid color scheme to … value={scheme_raw}")`.

**Suggested fix:** Drop the `eprintln!` in both arms (silent `return`), mirroring the mouse_pressure fix.

#### S46-F4 — Inspector Metal backend is gated on target_os=macos while the oracle (and the rustty CAPI exports) gate on isDarwin

- **Status:** ✅ CONFIRMED · **Kind:** stub-or-unimplemented · **Severity:** low
- **Oracle:** `src/apprt/embedded.zig:1238, 1284, 1310, 1332` (shard cited 1004-1092)
- **Rustty:** `crates/rustty-apprt/src/embedded.rs:2188-2200, 2364-2378, 2572-2586` vs `crates/rustty-apprt-c/src/lib.rs:3074/3100/3136`

Oracle `Backend.deinit` calls `ImGui_ImplMetal_Shutdown` under `isDarwin()`, and `initMetal`/`renderMetal` are compiled for any Darwin target. rustty's `Backend::deinit` and `initMetal`'s `ImGui_ImplMetal_Init` are `#[cfg(target_os = "macos")]` only (with a non-macos arm that discards `device` and still returns true), and `renderMetal` has a `#[cfg(not(target_os = "macos"))]` body that is `unimplemented!()`. The apprt-c exports `rustty_inspector_metal_init/render/shutdown` are `#[cfg(any(macos, ios))]`, so on an iOS build `initMetal` would report success without initializing Metal and `rustty_inspector_metal_render` would panic. Latent (no iOS toolchain in tree), but it is a reachable-by-cfg `unimplemented!()` in production code.

**Evidence:** oracle `:1238` `.metal => if (builtin.target.os.tag.isDarwin()) cimgui.ImGui_ImplMetal_Shutdown()` vs `embedded.rs:2194-2197`; `embedded.rs:2576-2586` `#[cfg(not(target_os = "macos"))] pub fn renderMetal(..) { unimplemented!(..) }` while `lib.rs:3100` is `#[cfg(any(macos, ios))]`.

**Suggested fix:** Gate dcimgui-sys' Metal backend and these three fns on `any(target_os = "macos", target_os = "ios")` (or gate the three CAPI exports on macos only) so the cfg sets agree and no `unimplemented!()` body is reachable.

#### S46-F5 — All backend log.err/log.warn/log.info/log.debug sites in embedded.rs are elided (kept only as comments)

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/embedded.zig:277-281, 313-317, 493-517, 646-650, 764-983, 1027-1029, 1064-1066`
- **Rustty:** `crates/rustty-apprt/src/embedded.rs:641-643, 1157-1185, 1770-1993, 3082-3086`

Every logging statement in the backend half of `embedded.zig` — performAction's debug dispatch line, performPreAction's config-clone error, Surface.init's three working-directory warnings and finalize warning, `close()`'s "runtime embedder does not support closing a surface", the 13 callback `catch |err| log.err(...)` arms, Inspector.init's activate failure, initMetal's warn/debug — is reduced to a comment in rustty (the file's stated "no logging shim" convention). Grep confirms the file contains exactly one runtime diagnostic (`eprintln!` at `:2370`) against 44 comment-only log mentions. The apprt-c wrappers DO emit via `eprintln!`, so diagnostics are inconsistent and the backend-side ones are lost.

**Evidence:** oracle `:493-497` `log.warn("error opening requested working directory dir={s} err={}", ..)` vs `embedded.rs:1157-1160` `Err(_err) => break 'wd,`; oracle `:646-649` `log.info("runtime embedder does not support closing a surface", .{})` vs `embedded.rs:3084` comment only.

**Suggested fix:** Route these through `rustty_core::logging::log` (already used by `rustty_init` in `lib.rs`) with the oracle message text and scope `embedded_window`.

#### S47-F2 — supportsDialogs/supportsTabOverview/supportsSwitchRow/supportsToolbarView/supportsBanner not ported

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** missing-function · **Severity:** low
- **Oracle:** `src/apprt/gtk/adw_version.zig:101-120`
- **Rustty:** `crates/rustty-apprt/src/gtk/adw_version.rs:11-36`

The five `supports*()` predicates (atLeast(1,5,0) / (1,4,0)×3 / (1,3,0)) are deliberately deleted under AD-3 (libadwaita ≥ 1.5 floor), with their call sites in other files hard-wired to the modern arm. On any runtime that satisfies AD-3 they would all be true, so behaviour only diverges on a libadwaita < 1.5 runtime (declared unsupported). Recorded as a documented deviation so the policy exception is visible.

**Evidence:** `adw_version.zig:102-120` defines `pub inline fn supportsDialogs() bool { return atLeast(1, 5, 0); }` etc.; `adw_version.rs:11-36` documents the deletion; grep for `supports_dialogs`/`supports_banner` across `crates/rustty-apprt/src` returns nothing.

**Suggested fix:** Either accept as a ratified AD-3 deviation, or port the five helpers as thin delegates to `at_least(...)` so the call sites can keep the reference's runtime branch.

**Refuting lens — semantic (medium confidence):** "Checked all oracle call sites: grep of ghostty/src shows supportsBanner/supportsDialogs/supportsSwitchRow are used only at debug_warning.zig:14,:42, surface_child_exited.zig:15, dialog.zig:21, clipboard_confirmation_dialog.zig:17 (all comptime type/const selections) plus window.zig:1972 (runtime); supportsTabOverview/supportsToolbarView have zero call sites. Under adw_version.zig:47-63 atLeast folds at comptime against the build headers, so with the AD-3-pinned adw>=1.5 headers every one of those predicates is compile-time true — exactly the arm crates/rustty-apprt/src/gtk/adw_version.rs:11-36 documents inlining. No input/state within the supported build+runtime envelope yields a different observable result, and the helpers themselves are pure predicates with no ported caller. Documented, ratified deviation with equivalent behaviour: refuted for this file."

#### S47-F3 — atLeast drops the compile-time header-version short-circuit (gtk_version and adw_version)

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/gtk_version.zig:47-66` (and `adw_version.zig:45-63`)
- **Rustty:** `crates/rustty-apprt/src/gtk/gtk_version.rs:146-149` (and `adw_version.rs:133-136`)

The reference `atLeast` first compares the header (`comptime_version`) against the threshold and returns false when the headers are older, regardless of the linked runtime; only then does it consult the runtime version. rustty's `at_least` is a pure delegate to `runtime_at_least`, so when the linked library is newer than the compile-time API ceiling (`BASELINE` 4.14.0 / 1.5.0) the reference answers false and rustty answers true. Documented as an AD-3 decision (the `v4_14` Cargo feature is a ceiling, not a version oracle), with the module explaining why a BASELINE fold was rejected.

**Evidence:** `gtk_version.zig:56-60` `if (comptime comptime_version.order(.{...}) == .lt) return false;` before `return runtimeAtLeast(...)`; `gtk_version.rs:147-149` `pub fn at_least(major, minor, micro) -> bool { runtime_at_least(major, minor, micro) }`.

**Suggested fix:** Accept as a ratified AD-3 divergence, or gate `at_least` on `order((major,minor,micro), BASELINE)` only for thresholds above the Cargo feature ceiling where the modern-arm symbols cannot be named.

**Refuting lens — semantic (medium confidence):** "Read gtk_version.zig:45-66 (comptime_version fold then runtimeAtLeast) and adw_version.zig:45-63 against crates/rustty-apprt/src/gtk/gtk_version.rs:146-150 and adw_version.rs:133-136. The oracle's short-circuit is driven by the BUILD BOX's gtk/gtk.h macros, not by a fixed floor; ghostty built on any host with headers >= the threshold behaves exactly as rustty's live runtime delegate. rustty's BASELINE (4.14/1.5) is documented at gtk_version.rs:38-60 as an API-surface ceiling, not a version oracle, and folding it would const-false window.zig's live 4.16 CSS-class check — i.e. the fold would be the divergence from upstream-on-host, not the delegate. The only differing scenario is a hypothetical ghostty built against exactly-4.14 headers on a newer runtime, a build-configuration difference rather than a port behaviour difference. Refuted."

#### S47-F4 — gsettings.getAlloc (allocating string-key reader for gtk-font-name) not ported

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** missing-function · **Severity:** low
- **Oracle:** `src/apprt/gtk/gsettings.zig:50-77`
- **Rustty:** `crates/rustty-apprt/src/gtk/gsettings.rs:112-165`

The reference provides `getAlloc(allocator, key)`, which reads a string-typed GTK setting (the `[]const u8` arm of `getImpl`) and the `Key.@"gtk-font-name"` key exists for it. rustty ports only the non-allocating readers (`get_bool`, `get_c_int`), so there is no way to read `Key::GtkFontName`. No upstream call site uses `getAlloc` at this pin, so this is currently unobservable, but the public API is incomplete.

**Evidence:** `gsettings.zig:54-57` `pub fn getAlloc(allocator, comptime key: Key) !?key.Type()` and the string arm at `:69-74`; `gsettings.rs:18-26` module doc: "the allocating `getAlloc` path lands with its first consumer (none at this pin)".

**Suggested fix:** Add `get_string(key: Key) -> Option<Vec<u8>>` reading `settings.property::<Option<String>>(key.name())`, mirroring `getImpl`'s `[]const u8` arm.

**Refuting lens — semantic (medium confidence):** "Confirmed gsettings.zig:50-57 getAlloc and the []const u8 arm of getImpl (:69-74) have no counterpart in crates/rustty-apprt/src/gtk/gsettings.rs (only get_bool :122 and get_c_int :149). But `grep -rn getAlloc /Users/gael/projects/ghostty/src` shows no caller of gsettings.getAlloc anywhere (the two hits are environ.getAlloc in global.zig:149 and Config.zig:4756, a different API), and Key.@\"gtk-font-name\" is read by nothing. Zig never even semantically analyses the unreferenced pub fn. The Key/KeyType/requires_allocation data including GtkFontName IS ported (gsettings.rs:31-40, :100-110) and all four oracle tests port over it, so no assertion is weakened. Difference is confined to code unreachable in the oracle; refuted."

#### S48-F2 — Unicode-trigger fallback drops unencodable codepoints instead of writing U+FFFD

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/key.zig:79-81, 389-391`
- **Rustty:** `crates/rustty-apprt/src/gtk/key.rs:145-149, 346-350`

When `gdk.keyvalName(cp)` is null the reference prints the codepoint with `{u}`, which for a surrogate half or a codepoint > 0x10FFFF writes the UTF-8 replacement character (`std/Io/Writer.zig:1573-1582` printUnicodeCodepoint). The port does `else if let Some(c) = char::from_u32(cp) { out.push(c) }` and appends nothing for those values, so accel/xdg/label strings for such a trigger differ (empty key vs `\u{FFFD}`) while still returning true/Some.

**Evidence:** `key.zig:80` `try writer.print("{u}", .{cp});` (and `:388` in writeTriggerKeyLabel) vs `key.rs:145-149` / `:346-350`.

**Suggested fix:** `out.push(char::from_u32(cp).unwrap_or(char::REPLACEMENT_CHARACTER))` in both `write_trigger_key` and `write_trigger_key_label`.

**Refuting lens — semantic (medium confidence):** "Read key.zig:75-81 and :373-391 (`try writer.print(\"{u}\", .{cp})`) against key.rs:137-146 and :343-350 (`else if let Some(c) = char::from_u32(cp) { out.push(c); }`). The two differ only for cp values that are not valid Unicode scalars (surrogate halves, >0x10FFFF). Trigger.key.unicode is populated only from validated UTF-8 decodes (Binding.zig:1804-1813 via std.unicode.Utf8View, and :2705-2713) or from an unshifted_codepoint produced by the apprt key layer, so a surrogate/out-of-range value is not reachable through any real input path; the claimed U+FFFD behaviour of Zig's `{u}` was also not verifiable in-tree. Difference confined to effectively unreachable input — refuted per the lens."

#### S48-F3 — parseRequestPathToken hex parsing differs on '_' separators and leading '+'

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/portal.zig:63-67`
- **Rustty:** `crates/rustty-apprt/src/gtk/portal.rs:89-92`

The reference uses `std.fmt.parseUnsigned(usize, token, 16)`, which accepts `_` digit separators inside the token (rejecting only leading/trailing `_`) and rejects a leading `+` (verified in `std/fmt.zig:439-475`). The port uses `usize::from_str_radix(token, 16)`, which is the exact opposite on both characters. Only reachable with malformed portal response paths; the three oracle test vectors pass on both.

**Evidence:** `portal.zig:66` `std.fmt.parseUnsigned(usize, token, 16) catch return null` vs `portal.rs:91` `usize::from_str_radix(token, 16).ok()`.

**Suggested fix:** Reject a leading `'+'` explicitly and (if underscore tolerance is wanted for fidelity) strip interior `'_'` subject to Zig's no-leading/trailing-underscore rule before `from_str_radix`.

#### S48-F4 — generateToken draws from GLib's MT PRNG; PORT NOTE cites a stale entropy source

- **Status:** ✅ CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/apprt/gtk/portal.zig:13-17`
- **Rustty:** `crates/rustty-apprt/src/gtk/portal.rs:31-49`

At the pin the reference draws the token from `std.Random.IoSource{ .io = global.io() }` (OS entropy through the Io layer). The port draws two 32-bit values from `glib::random_int()` (GLib's Mersenne Twister, urandom-seeded), and its PORT NOTE says the reference "reaches std.crypto.random", which is not what this pin does. The per-call observable (a random usize) is the same class; recorded for the stale comment and the different generator.

**Evidence:** `portal.zig:15-16` `const rng_impl: std.Random.IoSource = .{ .io = global.io() }; return rng_impl.interface().int(usize);` vs `portal.rs:34` PORT NOTE and `:44-47` `glib::random_int()` loop.

**Suggested fix:** Update the PORT NOTE to cite `std.Random.IoSource`; optionally use getrandom-backed entropy to match the source class.

#### S48-F5 — fromResource / mediaFileError log lines collapse the reference's domain/code detail and two-branch messages

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/media.zig:29-47, 126-133`
- **Rustty:** `crates/rustty-apprt/src/gtk/media.rs:36-44, 107-116`

`fromResource` in the reference logs two distinct warnings — `failed to find resource {s}: {domain} {code} {message}` when a GError is set and `failed to find resource {s}` when `found == 0` without error — and `mediaFileError` logs `error playing sound from {s}: {domain} {code} {message}`. The port emits a single message-only warning in each case, omitting the quark domain and code. Log text only.

**Evidence:** `media.zig:32-40` and `:128-133` format `glib.quarkToString(err.f_domain), err.f_code, err.f_message` vs `media.rs:41` / `:115` `{err}` (glib::Error Display is the message only).

**Suggested fix:** Format `err.domain()` (quark to string) and `err.code()` alongside the message in both warnings.

#### S49-F3 — loadCustomCss: a non-FileTooBig read error no longer aborts the whole custom-CSS load

- **Status:** ✅ CONFIRMED · **Kind:** missing-error-path · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/application.zig:1166-1176` (caller log at `:1309-1312`, byte load at `:1180, 1196`)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/application.rs:1269-1281, 1310`

In the oracle, `readToEndAlloc` failing with anything other than `error.FileTooBig` is `return e`-propagated out of `loadCustomCss`, which stops processing the remaining `gtk-custom-css` entries and makes `propConfig` log "failed to load custom CSS, no custom CSS applied, err=…". rustty checks metadata size first, and any subsequent `fs::read` error is logged as "error opening gtk-custom-css file" and `continue`s — later files still load, so the applied-stylesheet end state differs. Independently, the oracle loads raw file bytes via `glib.Bytes`/`loadFromBytes` while rustty passes `String::from_utf8_lossy(&contents)` to `load_from_string`, so invalid-UTF-8 bytes in a user stylesheet are replaced with U+FFFD before GTK parses them.

**Evidence:** `application.zig:1170-1176` `catch |err| switch (err) { error.FileTooBig => {…continue}, else => |e| return e }`; `application.rs:1269-1281` `Err(err) => { …eprintln!(…); continue; }` and `:1310` `load_from_string(&String::from_utf8_lossy(&contents))`.

**Suggested fix:** On a read error after a successful open, log the oracle's "failed to load custom CSS" warning and return early; load via `load_from_bytes(&glib::Bytes::from(&contents))` to preserve raw bytes.

#### S49-F4 — globalShortcutBindFailed: fixed 128/512-byte buffers (drop-notification-on-overflow) not reproduced

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/application.zig:1753-1784`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/application.rs:1661-1692`

The oracle renders the trigger label into a 128-byte buffer (falling back to `"?"` on failure) and the body into a 512-byte buffer, and `catch return`s — raising no desktop notification at all — when the body does not fit. rustty uses growable Strings and always raises the notification. Intentional (the PORT NOTE says a silently dropped notification is a defect), but it is an observable difference for very long compositor messages / labels. (S50-F3 reports the same divergence from the second shard's range.)

**Evidence:** `application.zig:1758-1763` `var label_buf: [128]u8`, `:1773-1778` `var body_buf: [512]u8; … bufPrintZ(...) catch return;`; `application.rs:1661-1665` PORT NOTE and `:1669-1692` unbounded Strings.

**Suggested fix:** Leave as-is if the deviation is accepted; otherwise truncate/skip at the same 128/512 thresholds.

#### S49-F5 — Log-only omissions: runtime-CSS byte-count debug line dropped; unimplemented-action warning omits the action name

- **Status:** ✅ CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/application.zig:942, 812`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/application.rs:1197-1201, 3461-3463`

`log.debug("runtime CSS is {d} bytes")` is preserved only as a comment (no emission anywhere in `crates/`), and the `Unimplemented` arm's `log.warn("unimplemented action={}", .{action})` becomes a fixed `"unimplemented action"` string without the action tag. Both are diagnostics-only; control flow and return values match.

**Evidence:** `application.zig:942` vs `application.rs:1197-1201` comment "preserved as a comment rather than emitted"; `application.zig:812` vs `application.rs:3462` `eprintln!("warn(gtk): unimplemented action")`.

**Suggested fix:** Emit `eprintln!("debug(gtk): runtime CSS is {} bytes", css.len())` and include the action discriminant in the unimplemented warning.

#### S50-F2 — parseOverrides trims Unicode whitespace instead of ASCII whitespace

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/application.zig:2022-2046` (shard cited 2073-2097)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/application.rs:1924-1940`

The reference trims `--shell-integration=`, `--working-directory=` and `--title=` values with `std.mem.trim(u8, v, &std.ascii.whitespace)` (space, `\t`, `\n`, `\r`, VT, FF only). The port uses `str::trim()`, which also strips Unicode White_Space code points (U+00A0, U+0085, U+2028, U+3000, …). A title or working directory ending in a non-breaking space is preserved by the reference but stripped by the port.

**Evidence:** `application.zig:2028, 2036, 2043` `std.mem.trim(u8, v, &std.ascii.whitespace)`; `application.rs:1925, 1934, 1939` `v.trim()`.

**Suggested fix:** Use `trim_matches(|c: char| matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0B' | '\x0C'))` (or a shared ascii-whitespace trim helper) at the three sites.

#### S50-F3 — globalShortcutBindFailed: over-long body is dropped by the reference but always notified by the port

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/application.zig:1753-1783`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/application.rs:1651-1692`

The reference formats `"{s}: {s}"` into a fixed 512-byte buffer and `catch return`s — raising NO desktop notification — when the label+detail does not fit, and caps the trigger label at 128 bytes with a `"?"` fallback. The port grows a String and always raises the notification with the full label. Edge-case only (compositor messages > ~500 bytes); the deviation is documented in an explicit PORT NOTE.

**Evidence:** `application.zig:1772-1777` `std.fmt.bufPrintZ(&body_buf, "{s}: {s}", ...) catch return;` and `:1758-1762` 128-byte label buffer; `application.rs:1651-1664` PORT NOTE, `:1667-1692` unbounded `format!`.

**Suggested fix:** If strict fidelity is wanted, return early when the formatted body exceeds 511 bytes and fall back to `"?"` when the label exceeds 127 bytes.

#### S50-F4 — globalShortcutTrigger emits a rustty-only success log line

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/application.zig:1741-1750`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/application.rs:2974-2983`

The reference logs only on failure (`log.warn("failed to perform action={}")`). The port's `perform_all_action_thunk` additionally prints `debug(gtk): global shortcut action performed ok` to stderr on every successful activation (documented as gate instrumentation, asserted by `.port/tools/gates/ws15_globalshortcuts_portal_gate.sh`). Cosmetic stderr difference.

**Evidence:** `application.zig:1746-1748`; `application.rs:2983` `Ok(()) => eprintln!("debug(gtk): global shortcut action performed ok")`.

**Suggested fix:** Drop the success log or gate it behind the same debug-log predicate the rest of the file uses, if stderr parity matters.

#### S50-F5 — actionOpenConfig performs synchronously instead of via the core mailbox

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/application.zig:2115-2151`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/application.rs:2013-2023`

The reference pushes `.{ .open_config = target }` onto the core mailbox (drained on the next core tick, which also emits the core's `mailbox message=open_config` debug log and runs through `App.performAction`). The port calls `action::open_config(app, target)` directly. End state is identical since `App.zig:275-282` is a pure pass-through, but the work happens synchronously inside the GAction activation rather than on the next tick, side-effect ordering relative to other queued mailbox messages differs, and the core-side debug log line is not produced.

**Evidence:** `application.zig:2151` `_ = self.core().mailbox.push(global.io(), .{ .open_config = target }, .forever);`; `application.rs:2023` `let _ = action::open_config(app, target);` with a PORT NOTE.

**Suggested fix:** Optional: push `Message::OpenConfig(target)` on the core mailbox through a bound thunk (as `present_surface_thunk` does) to keep the tick-deferred ordering.

#### S51-F1 — Config.getMut has no counterpart

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** missing-function · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/config.zig:94-100`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/config.rs:143-174`

The oracle exposes `getMut(self) *CoreConfig` alongside `get`. rustty's `Config` only exposes `get(&self) -> &CoreConfig`; there is no `get_mut` anywhere in the file or workspace. No caller in the oracle GTK tree uses `getMut`, so this is API-surface only.

**Evidence:** `config.zig:94-100` defines `pub fn getMut`; the rustty impl block has only `new` and `get`.

**Suggested fix:** Add a `get_mut` (e.g. via `OnceCell::get_mut` on `imp().config`), or document the omission as deliberate.

**Refuting lens — semantic (medium confidence):** "Verified oracle config.zig:94-100 does define `pub fn getMut`, and rustty config.rs:163-172 has only `get` (no `get_mut` anywhere in the file). But `grep -rn 'getMut\\b' /Users/gael/projects/ghostty/src` returns exactly one hit — the definition itself at config.zig:98 — so the function has zero call sites in the entire oracle tree. No input or state reaches it, so its absence produces no observable behavioural difference; it is dead API surface only (the finder concedes this). Under the semantic-equivalence lens this is a difference confined to unreachable code."

#### S51-F2 — diagnosticsBuffer: no 4095-byte truncation / skip-on-overflow of a single diagnostic

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/config.zig:110-134` (buffer at `:117-131`)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/config.rs:108-124`

The oracle formats each diagnostic into a fixed `[4095:0]u8` buffer; if `diag.format` fails (WriteFailed on overflow) it logs a warning and SKIPS that diagnostic entirely. rustty formats into a growable Vec and always inserts the full text, so a >4095-byte diagnostic is shown in full instead of being dropped with a warning. Diagnostic text embeds user-controlled config key/value/file-path text, so the over-long case is reachable. Documented as a PORT NOTE in the file, which records but does not neutralise the divergence.

**Evidence:** `config.zig:117-130` (`var buf: [4095:0]u8`, `diag.format(&writer) catch |err| { log.warn(...); continue; }`) vs `config.rs:110-123`.

**Suggested fix:** Either keep as a recorded divergence (strictly more informative) or cap at 4095 bytes and skip+warn to match.

#### S51-F6 — session_handle lookup accepts D-Bus type `o` as well as `s`

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/global_shortcuts.zig:333-338` (shard cited 286-293)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs:776-786`

The oracle looks up `session_handle` with format `&s`; a portal that types the field as an object path (`o`) makes the lookup fail, the oracle warns "session handle not found" and abandons the session (no handle stored, no Activated subscription). rustty's `session_handle_from_results` uses `Variant::str()`, which accepts `s`, `o` and `g`, so it proceeds to bind shortcuts against such a portal. More lenient than the oracle; documented PORT NOTE plus a test asserting the `o` case is accepted.

**Evidence:** `global_shortcuts.zig:333-338` `vardict.lookup("session_handle", "&s", &handle) == 0` → warn+return; `global_shortcuts.rs:782-786` `dict.lookup_value("session_handle", None)?; value.str()`.

**Suggested fix:** Keep (superset) or restrict to `VariantTy::STRING` to match exactly.

#### S51-F7 — Response signal: unparseable response code defaults to 2 instead of 0

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/global_shortcuts.zig:454-467` (shard cited 408-421)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs:508-522`

The oracle initialises `response = 0` and calls `params_.get("(u@a{sv})", ...)`; a malformed Response leaves 0 (success) and calls `onResponse` with the (possibly null) vardict. rustty reads `child_value(0).get::<u32>().unwrap_or(2)`, treating a malformed payload as "ended unexpectedly". Only reachable with a non-conforming portal; documented as a DELIBERATE DIVERGENCE.

**Evidence:** `global_shortcuts.zig:454-467` vs `global_shortcuts.rs:512-522` `unwrap_or(2)` with the divergence comment.

**Suggested fix:** None; the rustty choice is safer. Record only.

#### S51-F8 — dbus-connection property setter runs close/refresh side effects (oracle: only setDbusConnection does)

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** other · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/global_shortcuts.zig:40-59, 508-540`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs:145-152, 177-210`

In the oracle the `dbus-connection` property uses a plain `privateObjFieldAccessor`, so `g_object_set("dbus-connection")` just swaps the field (no close/refresh/notify), while the explicit `setDbusConnection` method performs close → unref → ref → refresh → notifyByPspec. rustty folds that logic into the property's custom setter, so both the method and `set_property` run it. All reachable oracle callers (`application.zig:1371, :1509`) go through `setDbusConnection`, and `application.rs:465/:864` mirror them.

**Evidence:** `global_shortcuts.zig:42-48` `.accessor = C.privateObjFieldAccessor("dbus_connection")` vs `global_shortcuts.rs:151` `#[property(name = "dbus-connection", get, set = Self::set_dbus_connection, nullable)]`.

**Suggested fix:** None required; note for API-surface fidelity only.

**Refuting lens — semantic (medium confidence):** "Oracle global_shortcuts.zig:52-59 defines `dbus-connection` with `C.privateObjFieldAccessor(\"dbus_connection\")` and the close/unref/ref/refresh/notify logic lives in the standalone `setDbusConnection` (:508-540); rustty global_shortcuts.rs:138 wires the same logic as the property setter (:194-210). I grepped both trees for writers: oracle has only application.zig:1371 (open_uri) and :1509 (`priv.global_shortcuts.setDbusConnection(...)`); rustty has only application.rs:465 and :864 calling `set_dbus_connection`. No g_object_set(\"dbus-connection\") / construct-time write exists in either tree, so the divergence is confined to an unreachable path and every live call performs the identical close/ref/refresh/notify sequence. Refuted per the unreachable-code rule."

#### S51-F9 — Stale module doc claims jump commands are a placeholder no-op

- **Status:** ✅ CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/command_palette.zig:225-243`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/command_palette.rs:24-37` (doc) vs `:290-318, :744-752`

The module-level "PORT NOTE — jump commands are a documented placeholder" says `collect_jump_commands` is an inhabited no-op until surface iteration and `getEffectiveTitle`/`getPwd` land. The code below actually walks `core_app.surfaces` and pushes `Command::new_jump` per surface (a faithful port of `command_palette.zig:225-243`), and `effective_title`/`surface_pwd` exist and are used. The note is wrong and would fabricate a gap in a later audit (cf. memory `stale-deferral-docs-fake-gaps`).

**Evidence:** `command_palette.rs:24-37` (doc) vs `:290-318` and `:744-751`.

**Suggested fix:** Delete or rewrite the stale PORT NOTE paragraph.

#### S51-F10 — rustty-only extra log lines in on_response(BindShortcuts) and dispatch_activation

- **Status:** ✅ CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/global_shortcuts.zig:369, 590-606`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs:410-418, 637-642`

The oracle's `.bind_shortcuts => {}` arm is empty and `shortcutActivated` logs only `activated={s}` before the map lookup. rustty additionally logs "global shortcuts bound results=…" on BindShortcuts success and "global shortcut dispatch map-hit id=…" after the lookup hits. Log-only, self-documented as rustty instrumentation for the live gate.

**Evidence:** `global_shortcuts.zig:369` `.bind_shortcuts => {}`; `global_shortcuts.rs:637-641` and `:410-418` eprintln sites.

**Suggested fix:** None; cosmetic.

#### S52-F2 — `debug` pspec default value is `false` instead of `build_config.is_debug`

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** different-constant · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/inspector_window.zig:47-64`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/inspector_window.rs:58-62, 164-168`

The oracle defines the read-only `debug` property with `.default = build_config.is_debug`, so in a debug build the ParamSpec's default value is true. The rustty derive `#[property(name = "debug", get = Self::debug_prop)]` on a `PhantomData<bool>` supplies no `default = ...`, so the ParamSpecBoolean default is `false` regardless of build. The getter itself matches (`cfg!(debug_assertions)`), so the only difference is the pspec default visible via introspection.

**Evidence:** oracle `inspector_window.zig:54` `.default = build_config.is_debug,` vs rustty `inspector_window.rs:61` with no `default`.

**Suggested fix:** Add `default = cfg!(debug_assertions)` to the property attribute.

**Refuting lens — semantic (medium confidence):** "Read oracle inspector_window.zig:47-64: `debug` is a read-only property (accessor supplies only a getter returning build_config.is_debug) with `.default = build_config.is_debug`. rustty inspector_window.rs:61-62 + :166-168 registers `#[property(name = \"debug\", get = Self::debug_prop)]` whose getter returns `cfg!(debug_assertions)` — identical value. For a getter-backed read-only property the ParamSpec default is never applied to any instance and every g_object_get returns the getter's value, so no input/state yields a different result; the only divergence is g_param_spec_get_default_value introspection, which nothing (including inspector-window.blp's `bind template.debug`, which reads the getter) consults. Not an observable functional difference."

#### S52-F3 — setTables/setSequence notify emission order reversed (`has-*` before `tables`/`sequence`)

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/key_state_overlay.zig:145-171`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/key_state_overlay.rs:129-138`

The oracle setter explicitly emits `notify::tables` then `notify::has-tables` (and `sequence` then `has-sequence`). The rustty custom setter emits only `notify::has-tables` and relies on GObject's automatic post-`set_property` notification for `notify::tables` (no `explicit_notify` anywhere in the file), so handlers observe `has-tables` first. Template bindings are order-insensitive, so this is only observable to external `notify::` handlers.

**Evidence:** `key_state_overlay.zig:155-156` `notifyByPspec(properties.tables…)` then `notifyByPspec(properties."has-tables"…)` vs `key_state_overlay.rs:130-131` `self.tables.replace(value); self.obj().notify("has-tables");`.

**Suggested fix:** Call `self.obj().notify("tables")` before `notify("has-tables")` in the custom setter (optionally with `explicit_notify` to avoid a double emission), same for `sequence`.

#### S52-F4 — closureMatchLabel never returns null on formatting overflow

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/search_overlay.zig:303-318`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/search_overlay.rs:359-372`

The oracle formats `{selected+1}/{total}` into a fixed 32-byte buffer with `bufPrintZ` and returns null (label unset) if the result does not fit. The rustty version uses `format!` and always returns a string. Unreachable with real match counts; noted for completeness.

**Evidence:** oracle `search_overlay.zig:311-316` `var buf: [32]u8 = undefined; … catch return null;` vs rustty `search_overlay.rs:370-371` `format!("{sel}/{total}")`.

**Suggested fix:** None needed in practice; could mirror by returning None when the formatted length exceeds 31 bytes.

**Refuting lens — semantic (medium confidence):** "Read oracle search_overlay.zig:303-317 (bufPrintZ into [32]u8, `catch return null`) and rustty search_overlay.rs:359-372 (`format!(\"{sel}/{total}\")`). The null branch requires the two decimal renderings plus '/' to exceed 31 bytes, i.e. counts in the 10^15+ range for search match indices sourced from the terminal's match count; unreachable with any real state, and the oracle path itself would first hit `selected + 1` overflow semantics. Difference confined to unreachable code — refuted per the lens (the finder themselves note 'unreachable with real match counts')."

#### S54-F2 — commandFinished bell path always emits notify::bell-ringing (no state-change dedup, no freeze/thaw)

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/surface.zig:1165` with `setBellRinging` at `2296-2315` (contract at `:62-68`)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:2043-2046, 1624-1648`

Oracle `commandFinished` calls `setBellRinging(true)`, which freezes notifications, calls `ringBell()`, returns if focused, and then only writes+notifies `bell-ringing` when the value actually changes. rustty's `command_finished` calls `ring_bell()`, which after the focused guard calls the derived-property setter; `g_object_set_property` emits `notify::bell-ringing` on every call, and there is no freeze/thaw around the signal+action fan-out. Current consumers (the `should_border_be_shown` bind, tab title handling) are idempotent, so this is presently an extra notify emission. See S55-F2 for the same defect reported from the setter side.

**Evidence:** `surface.zig:2296-2315` `freezeNotify(); defer thawNotify(); … if (priv.bell_ringing == ringing) return;` vs `surface.rs:1643-1648` `if self.focused() { return; } self.set_bell_ringing(true);` with `#[property(get, set)] bell_ringing: Cell<bool>`.

**Suggested fix:** Give `bell-ringing` a custom setter that returns early when unchanged, and wrap `ring_bell` in `freeze_notify()`.

#### S54-F3 — GObject property/signal type and default mismatches (mouse-shape, focused, clipboard-write)

- **Status:** ✅ CONFIRMED · **Kind:** different-constant · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/surface.zig:222-240, 154-170, 480-492`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:249-259, 224-227, 596-606`

(a) `mouse-shape` is registered in the oracle as the `terminal.MouseShape` GObject enum with pspec default `.text`; rustty registers a plain `i32` (`Cell<i32>`, pspec default 0) — acknowledged in a PORT NOTE. (b) The oracle `focused` pspec default is `false` (private field default true); rustty declares `default = true` (runtime initial values still match). (c) The `clipboard-write` signal's first parameter is `apprt.Clipboard` (a registered GObject enum) in the oracle; rustty registers it as `i32` and `window.rs:1049` extracts an `i32`. None change runtime behaviour of the in-tree consumers, but all are observable via GObject introspection / `g_param_spec_get_default_value` / `g_signal_query`.

**Evidence:** `surface.zig:227-229` `terminal.MouseShape, .{ .default = .text`; `surface.rs:258-259` `#[property(name = "mouse-shape", get, set)] pub mouse_shape: Cell<i32>`; `surface.zig:485-488` `&.{ apprt.Clipboard, [*:0]const u8 }` vs `surface.rs:604-606` `.param_types([i32::static_type(), String::static_type()])`.

**Suggested fix:** Register `MouseShape` and `Clipboard` as glib enums (`#[derive(glib::Enum)]`) and use them as the property/signal types; align the `focused` and `mouse-shape` pspec defaults with the oracle.

#### S54-F4 — setProgressReport drops the `progress_report action blocked by config` debug log

- **Status:** ✅ CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/surface.zig:1070-1075` (shard cited 1045-1051)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:1519-1526`

When `progress-style` is disabled the oracle logs `log.debug("progress_report action blocked by config", .{})` before hiding the bar and returning; rustty hides and returns without logging (grep for the literal across the workspace returns nothing). Cosmetic (log text only).

**Evidence:** `surface.zig:1072` vs `surface.rs:1519-1525`.

**Suggested fix:** Add the equivalent debug log line.

#### S54-F6 — keyEvent error log omits the error value and is logged at warn instead of err

- **Status:** ✅ CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/surface.zig:1453-1456` (shard cited 1436-1439)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:2816-2829` (same at `:4074`)

Oracle: `log.err("error in key callback err={}", .{err})`. rustty: `eprintln!("warn(gtk): error in key callback")` discarding `_err`. Log-text/level-only difference; the `return false` behaviour is identical.

**Evidence:** `surface.zig:1454` vs `surface.rs:2827-2829`.

**Suggested fix:** Include `{err:?}` in the message and restore the `err` level.

#### S55-F3 — actionNotifyOnNextCommandFinish: non-boolean state is toggled to true instead of ignored

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** missing-branch · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/surface.zig:2651-2663` (shard cited 2586-2597)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:3173-3185`

The oracle checks `state.isOfType(bool)` and returns without touching the action when the state is not a boolean variant. rustty does `.and_then(|s| s.get::<bool>()).unwrap_or(false)` then `set_state(!value)`, so a non-boolean state is overwritten with `true` rather than left alone (the null-state case is identical on both sides). Unreachable in practice because `init_action_map` always creates the action with a boolean state.

**Evidence:** `surface.zig:2660` `if (state.isOfType(bool_variant_type) == 0) return;` vs `surface.rs:3181-3184`.

**Suggested fix:** Match on `action.state()`: `Some(s) if s.is_type(VariantTy::BOOLEAN) => toggle`, `Some(_) => return`, `None => set_state(true)`.

**Refuting lens — semantic (high confidence):** "The difference is confined to unreachable code. Oracle actionNotifyOnNextCommandFinish is at surface.zig:2651-2663 (the finder's 2586-2597 is stale — that range is setHAdjustment); it does `if (state.isOfType(bool)==0) return;`. The null-state case is identical in both (oracle `orelse newBoolean(false)` -> sets true; rustty `unwrap_or(false)` -> sets true). The only divergent input is a non-boolean state, which cannot occur: initActionMap (:1874-1884) creates the action via `initStateful(..., glib.Variant.newBoolean(false))` and GSimpleAction enforces the state's variant type on every set_state, so no code path can install a non-boolean state. No input produces a different observable result."

#### S55-F4 — mouse-shape property is registered as an int, not the GhosttyMouseShape enum GType

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/surface.zig:224-241, 2264-2272`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:247-259, 3143-3148, 3197-3242`

The oracle's `mouse-shape` property is a typed `terminal.MouseShape` GObject enum property (default `.text`) with typed getMouseShape/setMouseShape accessors; rustty registers it as `Cell<i32>` (PORT NOTE labelled "concern") carrying the repr discriminant, so the property's GType/ParamSpec and its default differ for introspection, `g_object_set` with the enum type, GTK Inspector, and any blueprint/GSettings binding expecting the enum. The cursor-name mapping itself was verified identical (NAMES[] index order matches `rustty-vt` `mouse::Shape` declaration order and every name equals the oracle switch arm), and the effective initial value matches. Overlaps S54-F3(a) from the adjacent shard range.

**Evidence:** `surface.zig:224-232` `defineProperty(name, Self, terminal.MouseShape, .{ .default = .text, .accessor = typedAccessor(...) })` vs `surface.rs:258-259`; `surface.rs:3143-3148` PORT NOTE deferring the typed accessors.

**Suggested fix:** Register a `GhosttyMouseShape` glib::Enum for `rustty_vt::mouse::Shape` (or a local wrapper), make the property typed with default Text, then drop the i32 lookup table in favour of a match.

#### S55-F5 — Mouse button callback warning text differs and drops the error value

- **Status:** ✅ CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/surface.zig:2898, 2959`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:3335-3340, 3385-3390`

Oracle `gcMouseDown`/`gcMouseUp` log `error in key callback err={}` with the error; rustty logs `warn(gtk): error in mouse button callback` and discards `_err`. Cosmetic log-text difference only; control flow (false / return) is identical.

**Evidence:** `surface.zig:2898` `log.warn("error in key callback err={}", .{err});` and `:2959`; `surface.rs:3339`, `:3389`.

**Suggested fix:** Print the error value (`err={err:?}`) and keep the oracle text if log parity matters.

#### S55-F6 — defaultTermioEnv swallows a winproto addSubprocessEnv error instead of propagating it

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** missing-error-path · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/surface.zig:1655-1660` + `src/Surface.zig:642-646`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:2564-2577`

The oracle uses `try window.winproto().addSubprocessEnv(&env)`, so an error propagates out of `defaultTermioEnv` and core `Surface.init` replaces the whole map with a fresh `global.environMap()` (losing the LANG override / GTK var stripping). rustty logs a warning and returns the partially-built env, and the apprt seam only errors when the widget is gone. Currently unreachable: all three winproto arms return `Ok(())`.

**Evidence:** `surface.zig:1655` `try window.winproto().addSubprocessEnv(&env);` vs `surface.rs:2565-2567` `if let Err(_err) = window.winproto_mut().add_subprocess_env(&mut env) { eprintln!(...) }` with `default_termio_env` returning `EnvMap`.

**Suggested fix:** Make `default_termio_env` return `Result<EnvMap, ApprtError>` and propagate the winproto error so core's fallback branch is exercised like the oracle.

**Refuting lens — semantic (high confidence):** "Difference confined to unreachable code, and the finder concedes as much. I read all three rustty arms — noop.rs:158-163, wayland.rs:736-742, x11.rs:551-558 — each returns Ok(()) with no fallible operation (x11 only does env.put of a formatted xid), matching the oracle arms whose only error source is Zig allocator/bufPrint failure that cannot trigger here. So no input or state produces a different result today: default_termio_env's returned map is byte-identical either way, and core Surface.zig:642-646's fallback branch is never entered in the oracle either. The Result-vs-log shape difference alone is not an observable functional divergence."

#### S56-F2 — clipboardConfirmationDeny: @panic replaced by a warning log for non-rememberable requests

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/surface.zig:4311-4325`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:2483-2492`

Oracle `.paste, .list, .kitty_read, .kitty_write => @panic("request should not be able to be remembered")` when `remember` is true; rustty logs the same message and continues, with an explicit PORT NOTE ("In Rust we log instead of aborting"). Unreachable through the dialog UI (`can-remember` is false for those variants).

**Evidence:** `surface.zig:4324` `@panic(...)` vs `surface.rs:2491` `eprintln!("warn(gtk): request should not be able to be remembered")`.

**Suggested fix:** Use `unreachable!("request should not be able to be remembered")` to keep the oracle's fail-fast semantics.

**Refuting lens — semantic (medium confidence):** "Verified oracle surface.zig:4311-4324 (`.paste, .list, .kitty_read, .kitty_write => @panic(\"request should not be able to be remembered\")`) vs rustty surface.rs:2483-2492 (eprintln warn). The panic arm is dead code: surface.zig:4247-4250 constructs the dialog with `can-remember = false` for exactly those four variants, and clipboard_confirmation_dialog.zig:235-238 reads `remember` from a switch row that ui/1.4/clipboard-confirmation-dialog.blp:91 binds `visible: bind template.can-remember` and nothing ever activates, so `remember` is always false for those requests. The difference is confined to unreachable code — no input produces a different observable result."

#### S56-F3 — set_clipboard converts the text/plain payload lossily to String for the clipboard-write signal

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/surface.zig:4054-4058, 4117-4132`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:2268, 2311, 2317`

The oracle passes the raw `[:0]const u8` text/plain bytes as the clipboard-write signal param and to the confirmation dialog; rustty does `String::from_utf8_lossy(text_content.data).into_owned()` first, so non-UTF-8 bytes (reachable via OSC 52 base64 payloads, which `Surface.zig:2203-2221` hands straight to setClipboard) reach `window.rs:1047 surface_clipboard_write` and the confirmation buffer as U+FFFD instead of the original bytes — and on the confirm path those bytes round-trip back through `completeClipboardRequest`. The GDK content providers themselves still receive the raw bytes, so the actual clipboard contents are unaffected.

**Evidence:** `surface.zig:4056` `break content.data;` and `:4119` `.{ clipboard_type, text.ptr }` vs `surface.rs:2268` and `:2311`.

**Suggested fix:** Carry the signal text as raw bytes (`glib::Bytes` / `Vec<u8>`) per the repo's zig-byteslice-to-rust-rawbytes rule, or document that only valid UTF-8 is representable in the GObject string param.

#### S56-F4 — gl_resize debug log omits the window (GdkSurface) scale factor

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/surface.zig:3452-3470` (shard cited 3352-3369)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface.rs:3902-3908`

The oracle logs `gl resize width={} height={} scale={} window_scale={}` where `window_scale` is `root.as(Native).getSurface().getScaleFactor()` (0 if no root/surface). rustty logs only width/height/widget scale; its own comment states "The window (gdk-surface) scale is elided from the log here". Log-text only.

**Evidence:** `surface.zig:3455-3468` vs `surface.rs:3902-3908`.

**Suggested fix:** Add `window_scale = self.root().and_then(|r| r.surface()).map(|s| s.scale_factor()).unwrap_or(0)` to the debug line.

#### S56-F5 — SurfaceChildExited.setData(null) emits notify::data in rustty but not in the oracle

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/surface_child_exited.zig:76-91`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/surface_child_exited.rs:179-181` (property at `:59`)

Oracle `setData` with a null payload destroys the old data, sets `priv.data = null` and returns WITHOUT `notifyByPspec`, so the banner title is not rewritten to the "This is a bug" fallback. rustty routes through the generated property setter, which always emits `notify::data`, so `on_notify_data` runs and sets the fallback title. The only oracle caller (`surface.zig:1545`) always passes a non-null payload.

**Evidence:** `surface_child_exited.zig:82-85` `const data = data_ orelse { priv.data = null; return; };` vs `surface_child_exited.rs:179-181` delegating to the `#[property(get, set, nullable)]` setter.

**Suggested fix:** In `set_child_exited_data`, write the imp field directly and only call `self.notify("data")` when the payload is Some.

**Refuting lens — semantic (medium confidence):** "Read oracle surface_child_exited.zig:76-91 (null payload: destroy old, set priv.data = null, early return without notifyByPspec) and rustty surface_child_exited.rs:179-181 (`self.set_data(data.map(ChildExited))` on a `#[property(get, set, nullable)]` field, whose generated setter always notifies), plus on_notify_data at :137-148 which sets the 'This is a bug in Rustty' fallback title. The divergence is real in shape but unreachable: the sole caller in the whole workspace is surface.rs:1606 `.set_child_exited_data(Some(value))` (grepped crates/), mirroring the oracle's single non-null caller. Difference confined to unreachable code."

#### S57-F2 — action_close checks the mode string before the tab view, changing which warning is logged

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/tab.zig:396-433`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/tab.rs:386-409`

Oracle order: null param → warn and return; `param.get("&s")`; `getTabView() orelse return` (silent); null string → warn "invalid mode provided to tab.close-tab"; unknown mode → warn with the string. Rust order puts the `args.str()` check before the tab-view lookup, so when the tab has no `adw.TabView` ancestor AND the variant is not a string, rustty logs a warning the oracle would not. Log-only; the `close_page`/`close_other_pages`/`close_pages_after` dispatch and the `this`/`other`/`right` tags match.

**Evidence:** `tab.zig:406-407` `const tab_view = self.getTabView() orelse return;` precedes `:409-423`; `tab.rs:391-394` `let Some(mode_str) = args.str() else { eprintln!(...); return; };` precedes `:395-397`.

**Suggested fix:** Move the `get_tab_view()` early-return above the `args.str()` check so the warning ordering matches the reference.

#### S57-F3 — computed_title converts the config `title` bytes lossily (U+FFFD substitution)

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/tab.zig:496-509` (`Config.zig:1543` `title: ?[:0]const u8`)
- **Rustty:** `crates/rustty-apprt/src/gtk/class/tab.rs:352-355` (`rustty-config/src/config_struct.rs:2484` `Option<Vec<u8>>`)

The oracle uses the config title bytes verbatim as the plain title. rustty maps them through `String::from_utf8_lossy(...).into_owned()`, so a config `title` containing invalid UTF-8 yields replacement characters instead of the original bytes. Only observable for non-UTF-8 config titles; all other title sources are already UTF-8 GTK strings.

**Evidence:** `tab.zig:498-501` vs `tab.rs:352-355`.

**Suggested fix:** Pass the bytes through unchanged (e.g. a `glib::GString` built from the raw bytes), or document that GTK requires UTF-8 and the substitution is intentional.

#### S58-F2 — Legacy `gtk.showAboutDialog` arm (adw < 1.5) not ported

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** missing-branch · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/window.zig:1972-2004`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/window.rs:2174-2209`

The oracle chooses between `adw.showAboutDialog` (`adw_version.supportsDialogs()`) and `gtk.showAboutDialog` with program-name/logo-icon-name/title/version/website at runtime. rustty unconditionally uses `adw::AboutDialog`. This is a ratified collapse (AD-3, `gtk.rs:90-99`, `DEFERRALS.md`) and only observable on an adw < 1.5 runtime, which the crate declares unsupported; recorded so the divergence is on the audit ledger.

**Evidence:** `window.zig:1972` `if (adw_version.supportsDialogs())` … else `gtk.showAboutDialog(...)`; `window.rs:2175` comment "AD-3: `adw_version.supportsDialogs()` collapses to the modern `adw` arm".

**Suggested fix:** None required if AD-3 stands; otherwise gate on `adw_version::at_least(1,5,0)` and add a `gtk::AboutDialog` fallback with the same properties.

**Refuting lens — semantic (high confidence):** "Oracle window.zig:1972-2004 branches on adw_version.supportsDialogs(); rustty (window.rs:2175 comment + gtk.rs:90-99) unconditionally uses adw::AboutDialog. adw::AboutDialog is v1_5-gated and the crate compiles against a pinned adw >= 1.5 floor (gtk.rs:54,90-99), so the legacy gtk.showAboutDialog arm is unreachable in any supported build — a difference confined to unreachable code. The finder itself states no fix is required."

#### S58-F3 — Tab close-confirmation handlers use a captured TabView instead of re-resolving it via ancestor; the "non-existent page" warn path is gone

- **Status:** ✅ CONFIRMED · **Kind:** missing-error-path · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/window.zig:1604-1631`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/window.rs:1866-1882`

`closeConfirmationCloseTab` / `closeConfirmationCancelTab` re-resolve the `adw.TabView` at response time via `ext.getAncestor(adw.TabView, page.getChild())`, logging "close confirmation called for non-existent page" and bailing without calling `closePageFinish` if the page's child is no longer inside a TabView. rustty clones `imp.tab_view` into the two closures and always calls `close_page_finish` on it, so if the tab was reparented/removed while the dialog was up the behaviour differs (no warning, finish issued on the original view).

**Evidence:** `window.zig:1608-1615` and `1623-1630` `ext.getAncestor(...) orelse { log.warn("close confirmation called for non-existent page"); return; }`; `window.rs:1870-1880` `let tab_view = imp.tab_view.get(); … close_page_finish(...)` with no ancestor lookup.

**Suggested fix:** In both closures, resolve `ext::ancestor::<adw::TabView>(&page.child())`, warn and return on None, then call `close_page_finish` on the resolved view.

#### S58-F4 — GParamSpec metadata differs: `quick-terminal` default true→false, `titlebar-style` registered as i32 instead of a GEnum

- **Status:** ✅ CONFIRMED · **Kind:** different-constant · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/window.zig:101-151`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/window.rs:114-141`

The oracle registers `quick-terminal` with `.default = true` (the backing Private field defaults false, so runtime behaviour is unchanged) and `titlebar-style` as a `GtkTitlebarStyle` GObject enum with default `.native`. rustty exposes `quick-terminal` with default false and `titlebar-style` as a plain `i32` (0=native, 1=tabs). The neighbouring `toolbar-style` DOES keep its adw type and builder default, so the divergence is not a convention. Behaviour through the blueprint closure is identical; the difference is visible through introspection / param-spec queries / GtkBuilder.

**Evidence:** `window.zig:135-151` `.default = true`; `window.zig:101-118` typed `TitlebarStyle` with `.default = .native`; `window.rs:115-116` `#[property(name = "quick-terminal", get, set)] Cell<bool>`; `window.rs:140-141` `#[property(name = "titlebar-style", get = Self::titlebar_style)] PhantomData<i32>` with a PORT NOTE.

**Suggested fix:** Add `default = true` to the quick-terminal property attribute; register `GtkTitlebarStyle` as a `glib::Enum` and type the property with it.

#### S58-F5 — dispose additionally removes `tab_overview_focus_timer` (documented soundness divergence)

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/window.zig:1397-1425`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/window.rs:322-341`

The oracle's dispose removes only `handle_active_state_source`; a pending 500 ms `tab_overview_focus_timer` is left to fire after the window is finalized. rustty's dispose also removes `tab_overview_focus_timer`. This is intentional (documented as invariant 11; the oracle's behaviour is a latent use-after-free) and only observable in that undefined scenario, but it is an extra side effect relative to the oracle.

**Evidence:** `window.zig:1400-1405` removes only `handle_active_state_source`; `window.rs:329-341` `remove_source_id(self.tab_overview_focus_timer.take(), "tab overview focus timer")` with a PORT NOTE.

**Suggested fix:** None (keep; it is a soundness fix). Recorded for the ledger only.

**Refuting lens — semantic (high confidence):** "Oracle dispose at window.zig:1397-1425 removes only handle_active_state_source; rustty window.rs:322-341 additionally removes tab_overview_focus_timer with a PORT NOTE. The only scenario in which this differs is one where the oracle's pending timer fires against a finalized window — undefined behaviour in the oracle, and no defined-behaviour input produces a different observable result (the window is being destroyed, so the deferred focus grab has no observable effect either way). Difference confined to an unreachable/UB path; the finder itself recommends no change."

#### S58-F6 — Warn log messages omit the error value and one message text differs

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/class/window.zig:765, 806, 1210, 1287, 1294, 1453, 2182`
- **Rustty:** `crates/rustty-apprt/src/gtk/class/window.rs:549, 650, 677, 746, 760, 2094-2096, 2104, 2371`

Every `log.warn("... error={}", .{err})` site (winproto sync/resize/urgency, performBindingAction, winproto init, quick-terminal init, ring-bell urgency) is ported as an `eprintln!` that discards `_err`, so the diagnostic no longer names the failing error. `propQuickTerminal`'s text is also shortened from "quick terminal property can't be changed after surfaces have been initialized" to "… after surfaces initialized".

**Evidence:** `window.zig:765` `log.warn("failed to sync winproto appearance error={}", .{err})` vs `window.rs:649-651`; `window.zig:1287` vs `window.rs:2094-2096`.

**Suggested fix:** Format the error into the message (`{err:?}`) and restore the exact text.

#### S59-F2 — --class target name is lossy-converted before the bus-name validity check

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/ipc/DBus.zig:42-66`
- **Rustty:** `crates/rustty-apprt/src/gtk/app.rs:181-186`

For `Target::Class(class)` the oracle uses the raw class bytes as the bus name and derives the object path from them; if invalid, it prints the raw bytes in "D-Bus bus name is not valid: {s}". rustty converts with `String::from_utf8_lossy` first, so the diagnostic shows U+FFFD replacements instead of the user's bytes. The validity outcome is the same (`g_application_id_is_valid` requires ASCII), so only the printed text differs.

**Evidence:** `DBus.zig:43-51` and `:63`; `app.rs:182` `let class = String::from_utf8_lossy(class).into_owned();`.

**Suggested fix:** Validate/print the raw bytes and only convert after validation; or accept as cosmetic.

#### S59-F4 — Cleanup staleness threshold differs by up to one second (truncated seconds > 30 vs Duration > 30s)

- **Status:** ⚠️ DISPUTED (presence lens refuted) · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/portal/OpenURI.zig:545-549`
- **Rustty:** `crates/rustty-apprt/src/gtk/portal/open_uri.rs:566-577`

The oracle marks an entry stale when `untilNow().toSeconds() > cleanup_timeout`; `Duration.toSeconds` is `@divTrunc(ns, 1e9)` (zig `std/Io.zig:998`), so an entry is stale only once elapsed >= 31.0 s. rustty uses `now.duration_since(e.start) > Duration::from_secs(30)`, i.e. stale once elapsed > 30.0 s. Observable when a second `start()` restarts the 31 s timer: entries between 30.0 s and 31.0 s old when the timer fires are reaped (fallback opener invoked) by rustty but left in the map by the oracle.

**Evidence:** `OpenURI.zig:546-549` vs `open_uri.rs:567, 575`.

**Suggested fix:** Compare truncated whole seconds: `now.duration_since(e.start).as_secs() > CLEANUP_TIMEOUT_SECS`.

**Refuting lens — presence (medium confidence):** "The claimed window is unreachable. The cleanup timer is armed ONLY from start() (oracle OpenURI.zig:184 `self.startCleanupTimer()`; startCleanupTimer at :521-525 stops and re-arms at cleanup_timeout+1 = 31s), and cleanup() at :529-561 is one-shot (returns SOURCE_REMOVE, never re-arms). rustty mirrors this exactly (open_uri.rs:528-543 start_cleanup_timer with CLEANUP_TIMEOUT_SECS+1 via timeout_add_seconds_local_once; cleanup() clears the id and does not re-arm). Because the timer is restarted by the newest entry's start(), every surviving entry's age at fire time is >= 31s (glib timeouts fire no earlier than requested, and g_timeout_add_seconds only rounds later). trunc(age) > 30 and age > 30s therefore agree on every reachable timing; no entry can be in the (30.0s, 31.0s) band when the callback runs. Difference is theoretical only."

#### S59-F5 — start() rejects non-UTF-8 URIs with a rustty-only UriNotUtf8 error before the portal call

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/portal/OpenURI.zig:147-187, 275-279`
- **Rustty:** `crates/rustty-apprt/src/gtk/portal/open_uri.rs:67-74, 229-231`

The oracle performs no UTF-8 validation: it `dupeZ`s the raw URL bytes and hands them to `builder.add("s", ...)`. rustty adds `Error::UriNotUtf8` and returns it before any state is mutated, routing the caller (`application.rs:4132`) to the OS-opener fallback — so for a non-UTF-8 URL there is no entry, no portal call and no cleanup timer. Documented as deliberate (`open_uri.rs:623-636`).

**Evidence:** `OpenURI.zig:155` and `:279` with no validation; `open_uri.rs:231` `let uri = std::str::from_utf8(value.url).map_err(|_| Error::UriNotUtf8)?;`.

**Suggested fix:** Accept as a documented divergence, or mirror the oracle by attempting the call and letting the D-Bus layer fail into `failRequestAndFallback`.

#### S59-F6 — Log scope/level fidelity: debug messages printed unconditionally, scope `openuri` → `(gtk)`, error code dropped

- **Status:** ✅ CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/apprt/gtk/portal/OpenURI.zig:18, 383-387, 484-491`
- **Rustty:** `crates/rustty-apprt/src/gtk/portal/open_uri.rs:391, 505-508`

The oracle logs via `std.log.scoped(.openuri)`; in release builds ghostty's log_level is `.info` (`main_ghostty.zig:200-211`), so `log.debug("open uri successful")` / "cancelled by the user" / "updating open uri request path" produce no output. rustty uses `eprintln!("debug(gtk): …")`, which always writes to stderr, so every successful portal open prints a line in release; the scope name is `gtk` rather than `openuri` (the crate does preserve scopes elsewhere, e.g. `cgroup.rs:26-33`), and the request-failure log omits the oracle's `({})` `err.f_code`.

**Evidence:** `OpenURI.zig:18, 484-486, 383-387` vs `open_uri.rs:506` and `:391`.

**Suggested fix:** Route through the project's leveled logger with the `openuri` scope so debug lines are suppressed in release, and include the GError code in the failure message.

#### S60-F2 — xdg_activation token is not destroyed / Window.activation_token not cleared after the `done` event

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/winproto/wayland.zig:452-472`
- **Rustty:** `crates/rustty-apprt/src/gtk/winproto/wayland/globals.rs:551-568` (field cleared at `wayland.rs:752-755, 679-683`)

On `.done` the oracle's `onActivationTokenEvent` calls `activation.activate(done.token, self.surface)`, then `token.destroy()` and `self.activation_token = null`. rustty's Globals-routed handler only counts and activates; the `XdgActivationTokenV1` proxy stays alive and `Window.activation_token` stays `Some(token)` until the next `set_urgent` or `deinit`. Observable differences: the token object outlives `done`, and a later `set_urgent(false)` issues an extra `destroy` request that the oracle would not send.

**Evidence:** `wayland.zig:465-470` vs `globals.rs:560-566` with no destroy and no way to reset the Window field.

**Suggested fix:** Have the Dispatch handler destroy the token proxy after activate and give Window a way to observe completion so `activation_token` is cleared to None after done.

**Refuting lens — semantic (medium confidence):** "Read wayland.zig:452-472 and globals.rs:530-568 plus wayland.rs:743-760/668-685. The divergence is object-lifetime bookkeeping only: the oracle destroys the token inside the done handler; rustty destroys it in the single owner (set_urgent take()/deinit take()), documented in the PORT NOTE at globals.rs:530-551. The activate() call and its arguments are identical, so the surface-activation behaviour the user sees is the same. The only protocol delta is one extra xdg_activation_token_v1.destroy request on an already-done token, which is a legal destructor request with no compositor-visible effect, and take() makes double-destroy impossible. No input/state yields a different result."

#### S60-F3 — Wayland Window::deinit additionally destroys the pending activation token and flushes

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/winproto/wayland.zig:216-221`
- **Rustty:** `crates/rustty-apprt/src/gtk/winproto/wayland.rs:668-686`

Oracle deinit only frees `blur_region` and destroys bg_effect / releases decoration / releases slide; it deliberately leaves `activation_token` alone. rustty adds `if let Some(token) = self.activation_token.take() { token.destroy(); }` and a `conn.flush()`, sending an extra `xdg_activation_token_v1.destroy` request on window teardown (arguably a leak fix, but an observable protocol-level difference). An in-code comment acknowledges the divergence.

**Evidence:** `wayland.zig:216-221` (four statements, no token) vs `wayland.rs:680-685`.

**Suggested fix:** Either keep and document as an intentional divergence, or drop the token destroy to match the oracle exactly.

**Refuting lens — semantic (medium confidence):** "wayland.zig:216-221 deinit frees blur_region + destroys bg_effect + releases decoration/slide; rustty wayland.rs:668-685 does the same four plus `if let Some(token) = self.activation_token.take() { token.destroy(); }` and `let _ = self.conn.flush()`. The extra destroy is a destructor request on a proxy the client is discarding at window teardown (the oracle leaks it), and the flush only pushes already-queued requests earlier. Nothing observable to the user or to window/terminal behaviour changes; this is cleanup, not a functional difference."

#### S60-F4 — noop.Window.updateConfigEvent has no counterpart

- **Status:** ⚠️ DISPUTED (semantic lens refuted) · **Kind:** missing-function · **Severity:** low
- **Oracle:** `src/apprt/gtk/winproto/noop.zig:64-67`
- **Rustty:** `crates/rustty-apprt/src/gtk/winproto/noop.rs:119-174`

`pub fn updateConfigEvent(_: *Window, _: *const ApprtWindow.DerivedConfig) !void {}` is absent from `noop.rs` and from the `winproto.rs` Window union. It is dead in the oracle (`winproto.zig` defines no dispatcher and neither `wayland.zig` nor `x11.zig` implements it), so nothing observable changes; reported for completeness.

**Evidence:** grep for `update_config_event|updateConfigEvent` across both trees returns exactly one hit — `noop.zig:64`.

**Suggested fix:** Add an empty `update_config_event(&mut self, _cfg: &DerivedConfig) -> Result<(), WinprotoError>` to `noop::Window` for 1:1 surface parity, or document it as intentionally omitted dead code.

**Refuting lens — semantic (high confidence):** "Confirmed the oracle's noop.zig updateConfigEvent is an empty body and, as the finder itself states, has no dispatcher in winproto.zig and no wayland/x11 implementation — it is unreachable dead code on the oracle side. Per the lens, a difference confined to unreachable code is not an observable functional difference; omitting an empty never-called function changes nothing."

#### S60-F5 — X11 supportsQuickTerminal drops the warn log; initQuickTerminal returns an error instead of Ok

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/winproto/x11.zig:172-177`
- **Rustty:** `crates/rustty-apprt/src/gtk/winproto/x11.rs:413-431`

Oracle `supportsQuickTerminal` logs "quick terminal is not yet supported on X11" then returns false; rustty returns false silently (grep for the literal across `crates/` finds nothing) — and that call IS reachable, so a user-visible diagnostic is dropped. Oracle `initQuickTerminal` is an empty success; rustty returns `Err(Error::QuickTerminalUnsupported)`, which is effectively unreachable because both `application.zig:3186` and `application.rs:4649` gate window creation on `supports_quick_terminal`.

**Evidence:** `x11.zig:173` `log.warn(...)` vs `x11.rs:417-419`; `x11.zig:177` `pub fn initQuickTerminal(...) !void {}` vs `x11.rs:430` `Err(Error::QuickTerminalUnsupported.into())`.

**Suggested fix:** Emit the warn in `supports_quick_terminal` and return `Ok(())` from `init_quick_terminal`.

#### S60-F6 — X11 syncAppearance/syncBlur diagnostics dropped (blur debug line, `failed to sync decorations` error log)

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/winproto/x11.zig:214-234, 266-271, 289-307`
- **Rustty:** `crates/rustty-apprt/src/gtk/winproto/x11.rs:521-525, 613-622`

The oracle logs `set blur={}, window xid={}, region={}` before writing the blur property and logs `failed to sync decorations={}` / `failed to sync blur={}` when the property helpers fail. rustty's `sync_decorations` swallows the Err with a bare `return` and `sync_blur` emits no debug line, so the failure is invisible in the log. (Related: S60-F1 is the state-corruption half of the same code path.)

**Evidence:** `x11.zig:231-233` `self.syncDecorations() catch |err| { log.err("failed to sync decorations={}", .{err}); }` vs `x11.rs:521-525` and `:620-622` `Err(_) => return,`.

**Suggested fix:** Log the swallowed errors and the blur debug line with the same wording (log-only change).

#### S60-F8 — gtk-quick-terminal-namespace (and x11-instance-name) pass through String::from_utf8_lossy

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/winproto/wayland.zig:352`, `src/apprt/gtk/winproto/x11.zig:45-69`
- **Rustty:** `crates/rustty-apprt/src/gtk/winproto/wayland.rs:909-911`, `x11.rs:229`

The oracle hands the raw `[:0]const u8` config bytes to `layer_shell.setNamespace` and to `glib.setPrgname`. rustty converts with `String::from_utf8_lossy`, so a namespace / instance name containing non-UTF-8 bytes is rewritten with U+FFFD instead of being passed verbatim to the compositor / X server. The backing fields really are raw bytes (`config_struct.rs:2514`, `:3789`), and no raw-CString path exists in the winproto tree.

**Evidence:** `wayland.zig:352` `layer_shell.setNamespace(window, config.@"gtk-quick-terminal-namespace");` vs `wayland.rs:909-911`; `x11.rs:229` `Some(pn) => String::from_utf8_lossy(pn)`.

**Suggested fix:** Pass the bytes through the raw C-string entry points (gtk4-layer-shell ffi / `g_set_prgname`) to avoid the lossy re-encode, per the byte-slice fidelity rule.

#### S60-F9 — Stale PORT NOTE in noop.rs claims eventMods/initQuickTerminal are not yet ported

- **Status:** ✅ CONFIRMED · **Kind:** other · **Severity:** low
- **Oracle:** `src/apprt/gtk/winproto/noop.zig:27-38`
- **Rustty:** `crates/rustty-apprt/src/gtk/winproto/noop.rs:104-110`

`noop.rs:104-110` says `eventMods`, `initQuickTerminal` and the Window methods "are added … when those consumers exist; this task lands only init/deinit", but all of them are present just above (`event_mods` at `:55-67`, `supports_quick_terminal` at `:64-69`, `init_quick_terminal` at `:78-88`, plus the Window arm). Doc-only; misleads a reader auditing gaps.

**Evidence:** `noop.rs:104-110` PORT NOTE vs `noop.rs:55-61` and `:79-84`.

**Suggested fix:** Delete the stale PORT NOTE.

#### S61-F1 — Registry bind version is clamped to the advertised version instead of T.generated_version

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/winproto/wayland/Globals.zig:152-162`
- **Rustty:** `crates/rustty-apprt/src/gtk/winproto/wayland/globals.rs:276-341`

The reference always binds each matched global at `T.generated_version`, handling a bind error only by logging a warn and returning. rustty binds at `min(advertised_version, interface.version)` in every arm. On a compositor advertising a lower version than the generated bindings, the reference would issue a bind at a too-high version (a protocol error from the compositor) while rustty silently binds the lower version. Documented as deliberate in the rustty doc comment.

**Evidence:** `Globals.zig:152-156` `registry.bind(v.name, T, T.generated_version)`; `globals.rs:291` `let v = version.min(WlCompositor::interface().version);` (same at `:296, 302, 311, 317, 325, 335`).

**Suggested fix:** Either accept as a deliberate hardening and record it in the parity ledger, or bind at the generated version to match the reference exactly.

#### S61-F2 — onGlobalRemoved clears the blur capability for ext_background_effect; reference only resets output order

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/winproto/wayland/Globals.zig:132-137`
- **Rustty:** `crates/rustty-apprt/src/gtk/winproto/wayland/globals.rs:375-386`

The reference `onGlobalRemoved` switch has a single arm (`.kde_output_order => resetOutputOrder`) and leaves `bg_effect_capabilities` untouched when the background-effect manager is removed or re-advertised. rustty adds a `Tag::ExtBackgroundEffect` arm setting `bg_blur_supported = false` (self-labelled DIVERGENCE). `record_binding` also invokes `on_global_removed` on re-advertisement, so after a rebind the cached blur bit is false until the new capabilities event arrives and blur can be skipped in that window.

**Evidence:** `Globals.zig:132-137` vs `globals.rs:378-383`.

**Suggested fix:** Drop the extra arm (or keep it and record the divergence in the parity ledger).

#### S61-F3 — Globals log lines dropped (found global / matched / bind warn / empty primary output warn / primary output debug)

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/winproto/wayland/Globals.zig:146, 150, 157-161, 235, 239, 242`
- **Rustty:** `crates/rustty-apprt/src/gtk/winproto/wayland/globals.rs:437-443, 502-515`

The reference logs `found global {s}` and `matched {}` for every registry global, warns `error binding interface … error=…` on bind failure, warns `ignoring empty primary output name from kde_output_order_v1` and logs `primary output: {s}`. rustty's registry Dispatch and KdeOutputOrderV1 Dispatch emit none of these (grep across `crates/` finds zero hits for any of the message texts). The functional state handling of the empty output name is equivalent.

**Evidence:** `Globals.zig:146, 235, 242` vs `globals.rs:436-448` / `502-515` (no logging calls).

**Suggested fix:** Add the equivalent `eprintln!`/log calls at the same points.

#### S61-F4 — Hotkeys.bind warn omits the trigger

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/winproto/wayland/Hotkeys.zig:126-133`
- **Rustty:** `crates/rustty-apprt/src/gtk/winproto/wayland/hotkeys.rs:317-323`

The reference warn on a failed hotkey request is `failed to request hotkey trigger={f} err={}`; rustty prints `failed to request hotkey err={err:?}` without the trigger.

**Evidence:** `Hotkeys.zig:128-131` vs `hotkeys.rs:320`.

**Suggested fix:** Include the formatted trigger in the log line.

#### S61-F5 — Hotkey description is never truncated to "" for actions longer than 255 bytes

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/apprt/gtk/winproto/wayland/Hotkeys.zig:42-43`
- **Rustty:** `crates/rustty-apprt/src/gtk/winproto/wayland/hotkeys.rs:136-141`

The reference formats the action into a 256-byte stack buffer and passes an empty description when it does not fit (`bufPrintZ(...) catch ""`). rustty calls the allocating `global_shortcuts::action_description` (`global_shortcuts.rs:862-866`) with no length check, so an over-long action description is sent in full over the wire. The PORT NOTE admits the deviation. Affects only the advisory description string.

**Evidence:** `Hotkeys.zig:42-43` vs `hotkeys.rs:141`.

**Suggested fix:** Substitute an empty description when the formatted action exceeds 255 bytes.

#### S61-F8 — Failure log prints the Rust Debug variant name instead of the action tag name

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/benchmark/CApi.zig:25-31`
- **Rustty:** `crates/rustty-bench/src/capi.rs:62-66`

The reference warns `failed to run action={s} err={}` with `@tagName(action)` (kebab-case, e.g. `codepoint-width`); rustty prints `action={action:?}`, the derived Debug identifier (`CodepointWidth`). No `name()` inverse of `from_name` exists. The adjacent unknown-action path does echo the raw name faithfully, so the divergence is confined to this one diagnostic string.

**Evidence:** `CApi.zig:26-29` vs `capi.rs:64` `eprintln!("[{LOG_SCOPE}] failed to run action={action:?} err={err:?}")`.

**Suggested fix:** Print the kebab-case action name (the inverse of `Action::from_name`).

#### S61-F9 — PageCompression report mode prints to stdout instead of stderr

- **Status:** ✅ CONFIRMED · **Kind:** behavioral-difference · **Severity:** low
- **Oracle:** `src/benchmark/PageCompression.zig:436-453`
- **Rustty:** `crates/rustty-bench/src/page_compression.rs:749-764`

The reference `stepReport` writes both the per-page line and the total line with `std.debug.print`, i.e. to stderr. rustty uses `println!`, i.e. stdout. Observable when the benchmark output is redirected/piped. Note the adjacent error log at `page_compression.rs:740` does use `eprintln!`, so the stream choice is inconsistent within the same function.

**Evidence:** `PageCompression.zig:436-439, 442-453` vs `page_compression.rs:749, 758`.

**Suggested fix:** Use `eprintln!` for both report lines.

## Performance-only differences (not findings)

- `ActionCValue` is written out explicitly rather than comptime-synthesized from the Key enum; identical field set/types/order. `Action::key()/cval()` are explicit 69-arm matches instead of `inline else`.
- `Surface::init`: the oracle's `openDirAbsolute` + `dir.stat()` pair collapses to a single `std::fs::metadata` (same symlink-following semantics and not-a-directory rejection).
- Clipboard paths: oracle `stackFallback(1024)` conversion buffers become owned `Vec`/`CString` allocations; mimes are NUL-duplicated into CStrings for the `[*:0]` C contract (identical bytes reach the host).
- `Options.read_clipboard` returns raw `c_int` narrowed via `ClipboardReadResult::from_c_int` (out-of-range → Unsupported) instead of an `enum(c_int)` return; ABI-identical, fail-closed on host garbage.
- CAPI D11 inbound narrowing: key/mouse/inspector/point entry points take raw `c_int` and reject undeclared values (the oracle would be UB/trap in safe builds); `rustty_surface_quicklook_font` elides the `backend != .coretext` early return because the shaper BACKEND const is Coretext on Darwin with no build-time override.
- `Inspector::newFrame` uses `Instant` + `saturating_duration_since` (infallible) in place of the fallible `std.Io.Timestamp`; `Inspector::initInspector` evaluates `Inspector::init` before boxing (no alloc on failure) vs alloc-then-placement-init.
- `ipc.rs` `NewWindowC`/`NewTabC::deinit` recover the array length by scanning for the NUL terminator instead of Zig's length-tracked slice free.
- `gtk/surface.rs` `get_title` keeps a `CString` cache to lend a stable `&CStr` (reference returns a pointer into the widget's own `[:0]` storage).
- `gtk/cgroup.rs` builds the D-Bus payload with `glib::Variant` tuple/array constructors instead of an incremental `VariantBuilder`; identical value and element order.
- `gtk/ext.rs` `list_find` iterates the typed `glib::List<T>` and returns a cloned strong ref rather than a raw borrowed `*T` from a `g_list_find_custom` trampoline.
- `winproto.rs` boxes the `Wayland` arm of the `Window` union to keep every window's `RefCell<Window>` small; dispatch is by auto-deref.
- `key.rs` `keyval_from_key` is a runtime linear scan of KEYMAP where the reference resolves the lookup at comptime per Key variant.
- `pre_exec.rs` hand-rolls an allocation-free `/proc/<pid>/cgroup` reader for async-signal-safety in the forked child; a test asserts byte-equality with `rustty_os::cgroup::current`.
- `gresource.rs` precomputes each blueprint's resource path at const-eval; `build.rs` derives the gresource XML from the BLUEPRINTS table rather than parsing `.ui` argv paths (making `genUi`'s BlueprintNotFound unreachable by construction).
- `application.rs` `core()`/`rt()` accessors are replaced by per-callsite monomorphized thunks installed by `bind_core_app` (type-erased backrefs); same calls, same ordering. `GlobalShortcuts` is instantiated in `startup_global_shortcuts` rather than `new`; `glib::log_set_writer_func` is wrapped in a `Once`; `find_active_window` uses `.find(is_active)` over toplevels; `load_custom_css` checks size via `fs::metadata` before reading (same 5 MiB threshold).
- `setGtkEnv` builds `GDK_DEBUG`/`GDK_DISABLE` via `Vec<&str>.join(",")` in the same field order and calls `sync_environ()` after each setenv; same resulting env and final environ snapshot. `actionPresentSurface` folds VariantType check + get into `Variant::get::<u64>()`. `exportTerminalIO` models the heap request + destroyData as a closure-owned `Vec<u8>` plus an `Rc<RefCell<Option<FileChooserNative>>>` keep-alive.
- `command_palette`: per-Command ArenaAllocator caches → owned `Option<String>` caches; `accelFromTrigger`'s 64-byte stack buffer → growable String.
- `global_shortcuts`: per-refresh arena + StringArrayHashMap → insertion-ordered `Vec<(String, Action)>` with linear find and explicit last-write-wins; fixed 1024/256-byte buffers → growable Strings (the overflow arms are unreachable for real triggers/actions); gio `SignalSubscription` RAII guards replace `c_uint` ids.
- `config.rs`: `OnceCell<CoreConfig>` replaces the by-value Private field; `imgui_widget.rs` uses `Instant` instead of `std.Io.Timestamp .awake`.
- `inspector_widget.rs`: `get_surface()` returns an owned `Option<Surface>` (+1 ref); `surface_weak_notify` skips the pointer-equality check because the WeakRefNotify handle is disconnected on every surface change.
- `key_state_overlay.rs`/`search_overlay.rs` `on_drag_end` use `width()/height()` instead of the deprecated `get_allocated_*`; closures use `Vec<String>::join`; `resize_overlay.rs` calls `g_source_remove` directly and `take()`s the RefCell so dispose is idempotent.
- `split_tree.rs`: `get_tree()` returns a clone of the boxed tree (all in-place mutations go through `borrow_mut()`); `disconnect_surface_handlers` tracks `(surface, id)` pairs instead of `signalHandlersDisconnectMatched`; `action_zoom` computes the active handle unconditionally (side-effect free); `remove_source_id` is a shared warn-on-failure helper.
- `class/surface.rs`: `core_surface` is an `AtomicPtr` with Acquire/Release instead of a plain optional raw pointer; key-sequence/table getters rebuild a `StringList` per read; `defer`/`block()` pairs are RAII guards; the IME `in_keyevent` reset happens right after `filter_keypress` on a snapshotted state; `estimateInitialSize` inlines the min-cell/DPI constants; the GTK<4.22 blur gate is a cargo feature with the runtime `runtime_until(4,23,3)` guard retained; `resize_overlay_schedule` uses `format!`; the clipboard read request's heap userdata becomes a cloned GObject captured in the async closure; the horizontal-scroll reset timeout closure holds a WeakRef.
- `tab.rs` builds the computed title in a Rust String; `tab.rs`/`title_dialog.rs` use weak self references in signal callbacks; `tab.rs` runs `initActionMap` from `constructed`.
- `window.rs`: `toplevelComputeSize` calls `get_bounds` before the `w<=0||h<=0` check (no side effects); handler disconnection is tracked pair-wise; `BindingGroup` is wrapped via direct libgobject FFI (glib-rs gates it behind v2_72); the compute-size `after` connection uses raw `g_signal_connect_data` with `G_CONNECT_AFTER`; `Window::new` takes `Option<&str>` instead of a single-field overrides struct.
- `ext/actions.rs` replaces raw `*T` userdata with a weak GObject reference; `ext/slice.rs` replaces arena + dupeZ storage with `Vec<String>`.
- DBus IPC builds the `(sava{sv})` payload from `ToVariant` tuples at send time rather than in `VariantBuilder`s created in `init()`; validation/connect/call order and error output are the same.
- OpenURI: `AutoArrayHashMapUnmanaged` + `std.Io.Mutex` → `HashMap` in a `RefCell` (all callers on the GTK main loop); the cleanup sweep collects the stale set then removes and falls back (same set reaped, different stderr/spawn interleaving); `RequestData` → closure captures with an RAII `SignalSubscription`; options `a{sv}` built with `VariantDict::end()`.
- winproto: `wayland.rs` keeps an `Rc<RefCell<Pump>>` plus Cell caches instead of a raw `*Globals`, runs a private wayland-client queue cranked by a GLib GSource with explicit flushes, and resolves the primary output through `App::primary_output_name()`; `x11.rs` loads libX11 through x11-dl and adds a defensive `nitems<5` check and `read_unaligned` on `_MOTIF_WM_HINTS`; `blur_region.rs` uses `Vec<Slice>` with the global allocator.
- Globals stores proxies in typed `Option<T>` fields plus a `HashMap<Tag, u32>` of names instead of a type-erased `EnumMap`; Hotkeys uses a monotonic `HotkeyId` userdata and drains queued emissions after the dispatch borrow is released (same main-loop turn and order), with `Entry.shortcuts` as a `WeakRef`.
- `rustty-bench`: `doNotOptimizeAway` modelled as a register-input inline-asm sink / `black_box`; per-byte `assert` → `debug_assert!`; data files wrapped in a 256 KiB `BufReader`; `IsSymbol`'s `uucode` mode reproduces the same embedded table (R3 — no live uucode dep), so both modes now time the same lookup; `PageIterator` yields `(offset, len)` ranges; `lz4::HashTable` is boxed; `ScreenClone`'s `stepNoop` uses `ptr::read` + `mem::forget` to match the reference's aliasing struct copy.

## Refuted candidates

Sixteen candidates were refuted by both verification lenses. Listed so a reader can see what was checked and cleared.

- **S46-F6 — `ghostty_*` export names are not kept or aliased; only `rustty_*` symbols exist.** Every oracle export has a counterpart under the locked tree-wide R2 rename, and the Swift host calls `rustty_app_new`; the `ghostty_` hits in `include/rustty-apprt.h` are doc comments only. Policy/doc note, not an observable difference.
- **S47-F5 — gtk apprt `Surface.cgroup()` has no counterpart.** The oracle chain is dead (`cgroupPath` does not exist anywhere in ghostty/src, so Zig never analyses `cgroup`), and `class/surface.rs:2147-2154` carries the annotated `cgroup_path()` placeholder anyway. Comment wording only.
- **S49-F2 — Two legacy blueprints dropped from the gresource manifest.** The `1.0/clipboard-confirmation-dialog` and `1.2/debug-warning` entries are selected only by comptime arms requiring adw < 1.4 / < 1.3, unreachable under the locked AD-3 adw ≥ 1.5 baseline; with adw ≥ 1.5 the oracle resolves to the same 1.4 / 1.3 blueprints rustty ships.
- **S50-F6 — `CloseConfirmationDialog.close()` forwarder not ported.** The Rust wrapper `@extends libadwaita::Dialog`, so `AdwDialogExt::close` is the same `adw_dialog_close` call the oracle forwarder makes; the oracle forwarder has no call sites.
- **S51-F3 — Dialog: `adw.MessageDialog` fallback arm dropped.** `supportsDialogs()` is `atLeast(1,5,0)`; under the AD-3-pinned floor the MessageDialog arm is dead across the entire supported envelope, and the decision is recorded in `adw_version.rs:11-37` and `DEFERRALS.md`.
- **S51-F4 — DebugWarning: `gtk.Box` + `ui/1.2/debug-warning` legacy arm dropped.** `supportsBanner()` is `atLeast(1,3,0)`, strictly below the adw ≥ 1.5 floor, so the legacy arm (and hence the missing 1.2 blueprint) is unconditionally dead on every supported runtime; documented under AD-3.
- **S51-F5 — refresh builds the shortcut map even when there is no D-Bus connection.** rustty's `request()` returns immediately with no connection and the only map reader is the Activated handler, which requires a subscription created in the create-session response — so no D-Bus traffic, activation or session handle differs; the map is unreachable internal state.
- **S52-F5 — activateInspector failure warning goes to stderr instead of the scoped log.** The message text and destination match (ghostty's `log.warn` also lands on stderr); `eprintln!("warn(gtk): …")` is the crate-wide convention (157 sites) and the suggested "scoped logging facility" does not exist in this crate.
- **S52-F6 — first-delay timer armed in `constructed` rather than instance-init.** Neither tree marks `first-delay` CONSTRUCT, and GObject applies plain properties after `constructed()`, so a `first-delay` passed to `g_object_new`/`Object::builder()` has no effect in either; both arm a 250 ms timer from the byte-identical blueprint.
- **S53-F1 — f64→f16 split ratio conversion double-rounds via f32.** Both call sites derive the ratio from small integer pixel quantities; for `p/q` with realistic widget dimensions the exact value can never fall within a half-f32-ulp of an f16 rounding midpoint, so the stored f16 and the divider pixel position are identical.
- **S54-F5 — setScrollbar tolerates a missing vadj signal group instead of asserting.** `on_notify_vadjustment` unconditionally creates and stores the group whenever a vadjustment is set, and the group is cleared only in `init`/`dispose`, so no reachable state has `Some` vadjustment with a `None` group; the `configure` call itself matches.
- **S56-F1 — `init_surface` publishes the core_surface pointer BEFORE `CoreSurface::init`.** The post-condition matches (null restored on Err), GTK is single-threaded and init does not iterate the main loop, and none of the apprt entry points core init calls reads the widget's `core_surface`; the pre-publish is the documented compensation for the dropped renderer `surface_mailbox`.
- **S56-F6 — `IMKeyEvent.not_composing` is spelled `ImKeyEvent::True`.** Every use site maps consistently and the mapping is documented; a private enum variant name with identical observable behaviour.
- **S60-F7 — `BlurRegion.Pos` is a hard `i64` instead of `c_long`.** On every target either tree builds (64-bit Linux/FreeBSD) `c_long` is `i64`, so the `_KDE_NET_WM_BLUR_BEHIND_REGION` payload and its `nelements` are byte-identical; the ILP32 concern is unreachable and the choice is documented in-file.
- **S61-F6 — `test Benchmark`: `Simple.step` spins until the monotonic clock advances.** All four reference assertions are present with identical meaning; the spin lives in the test's own fixture and strengthens the `duration > 0` precondition without touching `run`.
- **S61-F7 — C export named `rustty_benchmark_cli` only; no `ghostty_benchmark_cli` alias.** Header declaration, linker `-u` flag, smoke test and the macOS test bundle all use the renamed symbol consistently; the project-wide rebrand leaves no dangling reference and no behaviour differs.
