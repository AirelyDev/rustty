# Upstream defects — Ghostty bugs found while porting

Defects **in the Ghostty reference itself** (pin `20abdb50a`), recorded so fixes can be
proposed upstream. Governed by `.port/PORT_RULES.md` **R13**: a defect reachable from real
input is never ported; the port handles it safely and the defect is recorded here.

This is not the parity ledger. `LEDGER.tsv` tracks where the **port** diverges from the
reference; this file tracks where the **reference** is wrong. A row here usually has a matching
parity row, cross-referenced both ways.

## Status of this file

Created 2026-09-11 when the user ruled that reference crashes are not ported. **The Candidates
section below is unverified.** Each candidate was assembled from this plan's progress notes and
must be re-verified against the pin, and triaged, before it becomes an entry. A retroactive sweep
of past work — code, findings, adjudications, and session logs — is in progress to find every
instance this plan and earlier phases ported faithfully.

## Entry format

```
### UD<n> — <one-line title>
- kind:            crash | ub | leak | deadlock | latent | test-weakness
- severity:        high | medium | low   (reachability-weighted, see below)
- reference:       <path>:<lines> at 20abdb50a
- defect:          what the reference does wrong
- ReleaseSafe:     consequence in a safety-checked build
- ReleaseFast:     consequence in the shipped build
- reachability:    who or what can trigger it (peer process / file / VT stream / config / kernel / none)
- port handling:   what rustty does instead, with the file:line of the PORT NOTE
- parity row(s):   LEDGER ids and/or NEW-FINDINGS ids
- test:            the test proving the port's safe handling, and its negative control
- proposed fix:    a sentence or two suitable for an upstream issue or PR
- upstream status: not reported | reported <link> | fixed upstream <commit>
```

**Severity** weighs reachability first: a crash any same-user process can trigger outranks a
crash that needs a corrupted config file, which outranks one that needs kernel misbehaviour.
`latent` and `test-weakness` entries are `low` unless they hide a live defect.

## Entries

### UD1 — GlobalShortcuts `Response` handler null-unwraps the results of a malformed body
- kind:            crash
- severity:        medium (a same-user session-bus peer watching the bus can trigger it)
- reference:       src/apprt/gtk/class/global_shortcuts.zig:442-469 (`callbacks.responded`, `.?` at :462), subscription at :481-491, at 20abdb50a
- defect:          `params_.get("(u@a{sv})", &response, &vardict)` is `g_variant_get`. On a body that is not `(ua{sv})` — `()`, `(u)`, `(sa{sv})` — GLib emits a critical and writes neither out-parameter (measured, GLib 2.88.2: "the GVariant format string '(u@a{sv})' has a type of '(ua{sv})' but the given value has a type of '()'"), so `response` stays 0 and `vardict` stays null. Arm 0 then calls `method.onResponse(self_cb, vardict.?)`.
- ReleaseSafe:     panic on the null unwrap — the terminal process aborts.
- ReleaseFast:     undefined behaviour: `onResponse` runs with a null `*glib.Variant` (`vardict.lookup(...)` on null).
- reachability:    peer process, same user. The `Response` subscription passes a NULL sender, so a process on the user's session bus that emits `org.freedesktop.portal.Request.Response` with a malformed body on the request path triggers it. Two limits: the path embeds a random 28-bit token (`generateToken`, `:687-698`) as well as our unique name, and the subscription exists only while a request is in flight. In practice the peer must be watching the bus (for example via `BecomeMonitor`) during that window. (Fix round 1: this line first said "any process on the user's session bus", which omitted both limits.)
- port handling:   code 0 with no results vardict takes the reference's own `2` arm (`log.warn("request ended unexpectedly")`) and `onResponse` is not called — `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs:613` (PORT NOTE in `GlobalShortcuts::responded`). The body is still read with the same `g_variant_get` call (`response_params`).
- parity row(s):   S51-F7 (LEDGER); task 20 briefly ported the panic in `dae5118b` and reversed it in `d9de0339` under R13.
- test:            `global_shortcuts_malformed_response_does_not_panic` (drives a well-formed `(ua{sv})` to `onResponse`, then `()`, asserting the full log vector). Negative control, VM 2026-09-11: reinstating `vardict.expect(..)` on arm 0 turns it RED with the panic at the unwrap. `global_shortcuts_response_params_follow_g_variant_get` pins the `(0, null)` reading of `()`, `(u)` and `(sa{sv})`.
- proposed fix:    In `responded`, treat a null `vardict` on code 0 as a failed response (`orelse { log.warn(...); return; }`) instead of `vardict.?`, and pass the portal's bus name (`org.freedesktop.portal.Desktop`) as the `signalSubscribe` sender so other peers cannot reach the handler at all. The sender filter is only spoof-proof on GLib 2.78.5 (the 2.78 backport) or 2.80.1 and later: earlier GDBus matched a well-known sender name in a way another connection could impersonate (CVE-2024-34397). (Fix round 2: this said 2.80.1 only.) The null-vardict check is the part that fixes the crash on every GLib.
- upstream status: not reported

### UD2 — GlobalShortcuts `Activated` handler aborts on a short body and reads an undefined shortcut id on a mistyped one
- kind:            crash (short body) + ub (wrong-typed second child)
- severity:        medium (any same-user session-bus peer that knows the session handle)
- reference:       src/apprt/gtk/class/global_shortcuts.zig:587-606 (`shortcutActivated`), subscribed in `onResponse` at :352-362, at 20abdb50a
- defect:          `var shortcut_id: [*:0]const u8 = undefined; params.getChild(1, "&s", &shortcut_id);` fails two different ways, depending on the body.
  - **No second child** (`(o)`, `()`): `g_variant_get_child` reaches GLib's fatal error "Attempt to access item 1 in a container with only 1 items" and the process aborts. Measured by the task-20 reviewer on GLib 2.88.2, both on a directly built variant and after a real D-Bus message round-trip.
  - **Second child of the wrong type** (`(oo)`): GLib emits a critical and writes nothing. The handler then runs `log.debug("activated={s}", .{shortcut_id})` and `map.get(std.mem.span(shortcut_id))` on the undefined pointer.
  (Fix round 1: this entry first said the no-second-child case also "writes nothing".)
- ReleaseSafe:     short body — abort (exit 134) inside GLib. Wrong-typed child — a DETERMINISTIC segfault: Zig fills `undefined` with `0xaa` bytes in Debug and ReleaseSafe (the task-20 reviewer measured `shortcut_id == 0xaaaaaaaaaaaaaaaa` on zig 0.16.0, the pin's minimum), and `std.mem.span` dereferences it. `log.debug("activated={s}")` is compiled out outside Debug (`main_ghostty.zig:208-211`, `.log_level = .info`), so in release builds the `map.get(std.mem.span(shortcut_id))` read is the one that faults; in Debug the log line faults first.
- ReleaseFast:     short body — the same GLib abort, since it does not depend on the Zig build mode. Wrong-typed child — undefined behaviour: `undefined` is not filled, so `std.mem.span` reads whatever the stack slot held.
  (Fix round 2: ReleaseSafe first said "reads uninitialised memory ... in practice a segfault or a garbage lookup".)
- reachability:    peer process, same user. The `Activated` subscription passes a NULL sender and matches only `arg0` as a path against the session handle, so an `(o)` or `(oo)` body carrying that handle reaches it. The session handle is not secret to a bus monitor, and the subscription lives as long as the session.
- port handling:   `activated_shortcut_id` reads child 1 only if it exists and is type `s`; otherwise the handler returns without logging or looking anything up — `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs:850`. Before task 20 the port PANICKED here (glib-rs `child_value(1)` asserts the index).
- parity row(s):   none — found by task 20 while fixing S51-F7; NEW-FINDINGS N38 records the unclaimed port site.
- test:            `global_shortcuts_activated_shortcut_id_is_safe_on_malformed_bodies` (`(os)` yields the id; `(o)`, `()` and `(oo)` yield none). Negative control, VM 2026-09-11: restoring `params.child_value(1).str()` turns it RED with glib's index assertion (`variant.rs:314`).
- proposed fix:    Check `params.nChildren() >= 2` and the child's type (or initialise `shortcut_id` to null and use a checked `get`) before using it, and filter the subscription's sender to the portal's bus name (spoof-proof only on GLib 2.78.5 or 2.80.1+, see UD1). Simpler alternative, suggested by the task-20 reviewer: check the whole body against the documented `Activated` signature before reading any child — in the zig-gobject binding `isOfType` takes a `*const glib.VariantType` and returns `c_int` (`glib2.zig:14474-14475`), so: `const ty = glib.VariantType.new("(osta{sv})"); defer ty.free(); if (params.isOfType(ty) == 0) return;` (in C, `if (!g_variant_is_of_type (params, G_VARIANT_TYPE ("(osta{sv})"))) return;`). (Fix round 2: the Zig here first read `params.isOfType("(osta{sv})")`, which does not compile against the binding.)
- upstream status: not reported

### UD3 — X11 winproto property writes run with no X error trap, so a `BadWindow` exits the terminal (and their status checks can never fire)
- kind:            crash
- severity:        medium (reachable from another X client on the same display)
- reference:       src/apprt/gtk/winproto/x11.zig:346-431 (`getWindowProperty` from :346, `changeProperty` :392-422, `deleteProperty` :424-431), reached from `resizeEvent` / `syncAppearance` (:218-235) via `syncBlur` (:243-287) and `syncDecorations` (:289-), at 20abdb50a
- defect:          Two faults.
  - **No error trap.** `XChangeProperty`, `XDeleteProperty` and `XGetWindowProperty` are called with no `gdk_x11_display_error_trap_push`/`_pop`, so an X error on them reaches GDK's default X error handler. That handler prints "The program '…' received an X Window System error … BadWindow" and calls `exit(1)`.
  - **A dead status check.** `if (status == 0) return error.RequestFailed` can never fire: libX11 1.8.13's `XChangeProperty`/`XDeleteProperty` have a single return path after `mov w0, #1` (task-20 reviewer's disassembly), and a real failure arrives asynchronously. The cache-preserving logic in `syncBlur` behind that check is therefore dead too.
  (Fix rounds 1–2: this entry was first `latent`, and said a failed request was "no crash; the cache disagrees with the server". That was wrong. The reviewer's GTK 4.22.4 probe showed an untrapped `BadWindow` exits the process with status 1, and the same call inside the GDK trap returns error 3 and survives.)
- ReleaseSafe:     the process exits with status 1 inside GDK's error handler (not a Zig safety check, so every build mode).
- ReleaseFast:     same.
- reachability:    MEASURED by task 20 on the VM (GTK 4.22.4, GDK_BACKEND=x11, headless sway's Xwayland), one scenario per process, with blur enabled:
  1. A second X client destroys our mapped toplevel (`XDestroyWindow`, as `xdotool windowclose` does), and the GTK loop is pumped for 1000 iterations: SURVIVES. GDK logs "GdkSurface 0x… unexpectedly destroyed" and keeps the window realized and mapped; nothing issues a winproto request.
  2. The same destroy, then `syncAppearance` directly: EXITS 1, `BadWindow … request_code 20` (`GetProperty`, from `syncDecorations`).
  3. GTK's own `gtk_window_destroy`, then `syncAppearance`: EXITS 1, `request_code 19` (`DeleteProperty`, from `syncBlur`). No natural in-app sequence was found that calls `syncAppearance` on a window after GTK destroyed it, so this is exposure, not a verified trigger.
  4. The external destroy, then `maximize`: EXITS 1, `request_code 8` (`MapWindow`). That is GDK's own request, so the trap below cannot fix this path; any GTK app is exposed to it.
  5. The external destroy, pump, then a CONFIG RELOAD delivered to the window (`notify::config` → `syncAppearance`): EXITS 1, `request_code 20`. This is the realistic trigger: another client on the display closes the window, and the user later reloads the config.
- port handling:   `crates/rustty-apprt/src/gtk/winproto/x11.rs` `Window::trap_x_errors` (R13 PORT NOTE) brackets each of the three calls with `gdk_x11_display_error_trap_push`/`_pop` (gdk4-x11's `X11Display::error_trap_push`/`error_trap_pop`, GDK's own scoped trap, never a process-wide `XSetErrorHandler`). A trapped error takes the reference's `error.RequestFailed` path, which `resizeEvent`/`syncAppearance` already log as `failed to sync blur=`/`failed to sync decorations=`. The dead `status == 0` checks stay, faithfully (S60-F1).
- parity row(s):   S60-F1, S60-F6; inventory item M110.
- test:            `x11_error_on_a_destroyed_window_does_not_exit_the_process` runs scenario 5 in a child copy of the test binary and asserts the child exits 0 and logged at least one trapped `failed to sync` line. On the VM under Xwayland it passes 3/3; with no display it FAILS. Negative control (trap disabled): the child exits with status 1 after GDK's "BadWindow" message, and the test is RED. `sync_blur_keeps_the_cached_region_when_the_property_write_fails` still covers the status path with a stub `XDeleteProperty`.
- proposed fix:    Wrap the property calls in `gdk_x11_display_error_trap_push (display)` / `if (gdk_x11_display_error_trap_pop (display) != 0) return error.RequestFailed;` (zig-gobject `gdkx114.zig:205-213`: `gdk_x11.X11Display.errorTrapPush` / `errorTrapPop` on the window's display). Replace the `status == 0` checks, which cannot fire, with that trap result, so `syncBlur`'s commit-on-success works as intended.
- upstream status: not reported

### UD4 — `gotoWindow(.previous)` dereferences a null toplevel list
- kind:            crash
- severity:        low (no verified trigger)
- reference:       src/apprt/gtk/class/application.zig:2476-2519 (`gotoWindow`), at 20abdb50a; the binding `zig-pkg/gobject-0.3.2-…/src/gtk4/gtk4.zig:59024` `extern fn gtk_window_list_toplevels() *glib.List`
- defect:          The binding declares the result non-optional, but `gtk_window_list_toplevels` returns NULL when there are no toplevels. `findCustom` on NULL returns NULL, so `starting` is null. The forward loop over the now-null optional does nothing, and in the wrap-around `.previous` arm `var end: *glib.List = glist; while (end.f_next) ...` dereferences it.
- ReleaseSafe:     segfault (Zig does not check a non-optional pointer returned from `extern`).
- ReleaseFast:     same, undefined behaviour.
- reachability:    UNVERIFIED. `goto_window` is a surface-targeted action, and a registered surface lives inside a toplevel, so the list is not empty in normal operation. The only conceivable trigger is a race in which the last window is torn down between the action being queued and performed.
- port handling:   `crates/rustty-apprt/src/gtk/class/application.rs` `goto_window_walk` returns `false` on an empty list (task 20, S50-F1). Treated as an invariant rather than an R13 site because no trigger is verified.
- parity row(s):   S50-F1.
- test:            `goto_window_walk_matches_the_reference_list_walk` covers the empty list in both directions (display-free); the `.previous` case — the one that crashes in the reference — was added in fix round 2, whose line first overstated coverage when only `.next` was tested. Negative control: removing the empty-list guard in `goto_window_walk` panics on the `.previous` slice.
- proposed fix:    Type `listToplevels` as returning `?*glib.List` in the bindings, and `orelse return false` in `gotoWindow`.
- upstream status: not reported

### UD5 — OpenURI treats a malformed `Response` body as success and never opens the URL
- kind:            latent
- severity:        low
- reference:       src/apprt/gtk/portal/OpenURI.zig:479-499 (`responseReceived`), subscription at :202-212, at 20abdb50a
- defect:          `params.get("(u@a{sv})", &response, &results)` on a body that is not `(ua{sv})` writes nothing (GLib critical), so `response` stays 0. The switch then logs `open uri successful` and skips `openUrlFallback`, so the URL the user asked to open is silently never opened.
- ReleaseSafe:     no crash; a lost URL open.
- ReleaseFast:     same.
- reachability:    peer process, same user: the subscription has a null sender, but the request path embeds a random `usize` token (`portal.generateToken`) and exists only while a request is in flight, so the peer must be watching the bus.
- port handling:   faithful (R1, a non-crash defect). `crates/rustty-apprt/src/gtk/portal/open_uri.rs` `response_received` makes the same `g_variant_get` call and reads code 0, which also removed the port's former PANIC on `()` (S59-F3).
- parity row(s):   S59-F3.
- test:            `malformed_response_bodies_read_as_code_zero_without_panicking` pins the faithful behaviour (code 0, no fallback, `open uri successful`).
- proposed fix:    Check the body's type first — `const ty = glib.VariantType.new("(ua{sv})"); defer ty.free(); if (params.isOfType(ty) == 0) { ... }` (C: `g_variant_is_of_type (params, G_VARIANT_TYPE ("(ua{sv})"))`) — and on a mismatch take the `2` ("ended unexpectedly") path so `openUrlFallback` still opens the URL; filter the subscription to the portal's bus name (spoof-proof on GLib 2.78.5 or 2.80.1+, see UD1). (Fix round 2: the Zig first read `params.isOfType("(ua{sv})")`, which does not compile against the binding, and the GLib floor said 2.80.1 only.)
- upstream status: not reported

### UD6 — an oversized tmux control-mode block leaves the viewer alive, so the next attach asserts
- kind:            crash (ReleaseFast: ub + leak)
- severity:        high (anything written to the pty; also a real `tmux -CC` re-attach)
- upstream project: ghostty
- reference:       src/termio/stream_handler.zig:410 (`assert(self.tmux_viewer == null)` on `.enter`); the missing `.exit` is src/terminal/dcs.zig:114-121 (`put`'s `catch`) with :164-166 (`unhook` of `.ignore` returns null; the `.exit` at :168-170 is only for a live `.tmux` state), fed by src/terminal/tmux/control.zig:71-74 (the 1 MiB `max_bytes` check), at 20abdb50a
- defect:          When a notification or `%begin` block grows past the control parser's 1 MiB `max_bytes`, `Parser.put` marks itself broken and returns `error.OutOfMemory`. `dcs.Handler.put` catches that, discards the tmux state, sets `.ignore` and returns null. `unhook` of `.ignore` also returns null, so `dcsCommand` never sees `.exit` and never frees `tmux_viewer`. The next `ESC P 1000 p` hits the assert. `control.zig:26-29` documents the limit as "the control mode session will be forcibly exited", which is not what happens.
- ReleaseSafe:     panic on the assert at the second `ESC P 1000 p`; the terminal process aborts.
- ReleaseFast:     `inlineAssert` is `if (!ok) unreachable`, so undefined behaviour. If the optimizer just drops the check, `.enter` overwrites `tmux_viewer` and the old viewer leaks (inferred from the code, not measured).
- reachability:    VT stream: `ESC P 1000 p`, then a notification line or a `%begin` block longer than 1 MiB, then `ESC \`, then `ESC P 1000 p`. A real tmux reaches it too: `tmux -CC attach` to a pane whose plain-text history exceeds 1 MiB (the viewer's `capture-pane -S -` reply is one block), detach, attach again in the same surface.
- port handling:   `dcs::Handler::put` emits `Command::Tmux(Exit)` when the state it discards is the tmux state, so the stream handler's existing `.exit` arm frees the viewer — `crates/rustty-vt/src/dcs.rs:204` (PORT NOTE). The `.enter` assert stays as an invariant, with a PORT NOTE saying why — `crates/rustty-termio/src/stream_handler.rs:539`.
- parity row(s):   NEW-FINDINGS N39 (no LEDGER row).
- test:            `tmux_oversized_control_block_ends_the_viewer` and `tmux_reattach_after_an_oversized_control_block_does_not_crash` (rustty-termio, `--features oniguruma`; real bytes through the VT stream into `StreamHandler`). Negative control, macOS 2026-09-11: with `put` returning null as the reference does, the first fails on "a control-mode block over the 1 MiB limit must end the session and free the viewer" and the second panics at `stream_handler.rs:538` with `assertion failed: self.tmux_viewer.is_none()`.
- proposed fix:    In `dcs.Handler.put`'s `catch`, return `.{ .tmux = .exit }` when the discarded state was `.tmux` (as the `max_bytes` doc comment already promises), so `dcsCommand` frees the viewer. Replacing the assert with "deinit any live viewer on `.enter`" would also stop the crash, but would leave the viewer alive for the rest of the broken DCS.
- upstream status: not reported

### UD7 — tmux control-mode ids with 20 or more digits hit `catch unreachable`
- kind:            crash (ReleaseFast: ub)
- severity:        high (anything written to the pty)
- upstream project: ghostty
- reference:       src/terminal/tmux/control.zig:238 (`%output`), :269 (`%session-changed`), :309 (`%layout-change`), :347 (`%window-add`), :377 (`%window-renamed`), :408 and :413 (`%window-pane-changed`), :470 (`%client-session-changed`), at 20abdb50a
- defect:          Each arm matches its id with an unbounded `([0-9]+)` capture and then parses it with `std.fmt.parseInt(usize, ..) catch unreachable`. A capture of 20 or more digits overflows `usize`, so `parseInt` returns `error.Overflow` and the `catch` reaches `unreachable`. The same arms already handle a failed regex match with `log.warn("failed to match notification ...")` and `break :cmd`.
- ReleaseSafe:     panic ("reached unreachable code"); the terminal process aborts.
- ReleaseFast:     undefined behaviour.
- reachability:    VT stream: `ESC P 1000 p` then, for example, `%output %99999999999999999999 x` and a newline.
- port handling:   the capture goes through `parse_capture_usize`, which on a parse error logs the arm's own `failed to match notification cmd=… line="…" err=Overflow` and makes the arm take its match-failure path (no notification; buffer cleared; back to idle) — `crates/rustty-vt/src/tmux/control.rs:970` (PORT NOTE), used at all eight sites.
- parity row(s):   NEW-FINDINGS N40 (no LEDGER row). `.port/phase-3/UPSTREAM_BUGS.md` U3 recorded this as "left faithful", which R13 reverses.
- test:            `an_overflowing_notification_id_warns_and_the_parser_continues` (rustty-vt, `--features oniguruma`): all eight captures, through the VT stream; each asserts the single warning, no notification from the bad line, and that the following `%output` still parses. Negative control, macOS 2026-09-11: restoring the unwrap makes every one of the eight cases panic with `called Result::unwrap() on an Err value: Overflow`; before the fix the test panicked at the original `control.rs:433` unwrap.
- proposed fix:    Replace each `catch unreachable` with `catch |err| { log.warn("failed to match notification cmd={s} line=\"{s}\" err={}", .{ cmd, line, err }); break :cmd; }`, or bound the captures (for example `([0-9]{1,19})`) so an over-long id fails the match instead.
- upstream status: not reported

### UD8 — tmux layout pane dimensions are `@intCast` to `u16`, and a zero crashes `Terminal.init`
- kind:            crash (ReleaseFast: ub)
- severity:        high (anything written to the pty)
- upstream project: ghostty
- reference:       src/terminal/tmux/viewer.zig:1160-1161 (`.cols = @intCast(layout.width)`, `.rows = @intCast(layout.height)` in `initLayout`, under a TODO saying the cast is not safe), at 20abdb50a. The zero-dimension traps are src/terminal/page.zig:274 (`&cells_ptr[start]` on an empty cells slice, zero width) and src/terminal/PageList.zig:647 (`page_list.first.?`, zero height). (Candidate C2 spelled the path `Viewer.zig`, which resolves only on a case-insensitive filesystem.)
- defect:          One root cause, two symptoms, both covered by this entry. The layout's `usize` width and height come from tmux and are narrowed with `@intCast`. Above 65535 the cast itself traps. A zero passes the cast, and `Terminal.init` then traps. Which check fires first was measured on the port's faithful mirror of `Terminal.init`: a zero width trips the cells index in `Page.initBuf`; a zero height (and 0x0) trips `first.?` in `PageList.init`. The `rows - 1` / `cols - 1` in `Terminal.init` (Terminal.zig:336,338) is never reached first.
- ReleaseSafe:     panic ("integer does not fit in destination type", or the index / null-unwrap panics for zero); the terminal process aborts.
- ReleaseFast:     undefined behaviour (truncated sizes, an out-of-bounds pointer, a null node).
- reachability:    VT stream: after `ESC P 1000 p` and the startup exchange, a `list-windows` reply or a `%layout-change` whose layout (with a correct checksum) has a pane of width or height 0 or above 65535.
- port handling:   an out-of-range or zero dimension is treated as a failed `Terminal.init`: `sync_layouts` runs its errdefer and returns an error, and the viewer becomes defunct with the reference's own "failed to process command output" / "failed to handle layout change" warning — `crates/rustty-vt/src/tmux/viewer.rs:1006` (PORT NOTE). Task 15 had ported the cast as a panic; this reverses it. The port's errdefer also now keeps carried-over panes, as the reference's does (N41).
- parity row(s):   S209-F1 (LEDGER); NEW-FINDINGS N41 for the zero half and the errdefer.
- test:            `list_windows_pane_dimensions_out_of_range_or_zero_make_the_viewer_defunct` and `layout_change_pane_dimensions_out_of_range_or_zero_make_the_viewer_defunct` (bytes through the VT stream; 70000 and 0 in each dimension, each with an in-range positive control), and `sync_layouts_pane_dimensions_out_of_range_or_zero_fail_without_a_pane`, which replaces the `should_panic` test `sync_layouts_pane_width_overflow_traps`. Negative controls, macOS 2026-09-11: restoring the checked cast panics with `TryFromIntError`; restoring it without the zero guard panics at `page_impl.rs:450` (index out of bounds) for the zero width, and before the fix a zero height panicked at `pagelist/mod.rs:1928` (`!page_list.first.is_null()`); restoring the old deinit-everything errdefer fails "pane 7 survives, as in the reference's errdefer".
- proposed fix:    In `initLayout`, use `std.math.cast(size.CellCountInt, layout.width) orelse return error.InvalidLayout` (same for height) and reject 0, so `syncLayouts` fails and the viewer goes defunct through its existing error handling. Optionally reject such dimensions in `Layout.parse`.
- upstream status: not reported

### UD9 — the "active area should be empty" assert after a pane's history is not guaranteed
- kind:            crash (safety-checked builds only)
- severity:        low
- upstream project: ghostty
- reference:       src/terminal/tmux/viewer.zig:1070-1077 (`receivedPaneHistory`: `if (comptime std.debug.runtime_safety) { ... assert(discarding.count == 0); }`), at 20abdb50a
- defect:          After writing the history, the viewer does `carriageReturn()` and `rows` × `index()` and asserts the active area is empty. `index()` scrolls only at the bottom margin; below the scroll region it just moves the cursor down. If the pane's scroll region and cursor are already set, text written below the region stays in the active area. `receivedPaneState` (the `list-panes` reply) sets both, and it can arrive before a pane's history: when a second pane is added while the first pane's captures are still queued, the queue holds the first pane's `list-panes` before the second pane's captures, and that reply covers both panes. (History bytes cannot set the region themselves: an ESC ends the DCS.)
- ReleaseSafe:     panic on the assert (Debug too); the terminal process aborts.
- ReleaseFast:     no check, and the history is damaged, not merely left in the active area: with the cursor stuck on the last row below the scroll region, `index()` never scrolls, so each history line overwrites the previous one on that row and the scrollback the capture was meant to fill is garbled or lost. The pane's `capture-pane` visible reply then runs `eraseDisplay(.complete)` over the active area. (Fix round 1, task R13-A review: this line first said only that the active area "keeps the text".)
- reachability:    VT stream (a forged exchange), and plausibly a real tmux: a split created during attach, in a pane running a program with a scroll region above the cursor row (vim's command line, for example).
- port handling:   no check, as ReleaseFast — `crates/rustty-vt/src/tmux/viewer.rs:1768` (PORT NOTE).
- parity row(s):   NEW-FINDINGS N43 (no LEDGER row).
- test:            `pane_history_below_the_scroll_region_does_not_trip_the_empty_active_area_check` (bytes through the VT stream: list-windows with pane 7, a `%layout-change` adding pane 8, pane 7's captures, a `list-panes` giving pane 8 cursor row 23 and region 0-10, then pane 8's history; asserts the viewer is not defunct and the history text is in pane 8's active area). Negative control, macOS 2026-09-11: with the port's former `debug_assertions` check the test panics in `inlineAssert` from `received_pane_history` (backtrace confirmed).
- proposed fix:    Drop the assert, or make the intent true: reset the scroll region (and origin mode) before the `index()` loop, or clear the active area with `eraseDisplay(.complete, false)` instead of scrolling it.
- upstream status: not reported

### UD10 — tmux layout parsing recurses without a depth limit
- kind:            crash (stack overflow)
- severity:        medium (anything written to the pty)
- upstream project: ghostty
- reference:       src/terminal/tmux/layout.zig:87-230 (`parseNext`; the recursive call is at :180), at 20abdb50a
- defect:          `parseNext` recurses once per `{` or `[`. The layout string comes from tmux, bounded only by the control parser's 1 MiB limit, so it can nest about 116,000 levels (9 bytes per level: `1x1,0,0{` plus `}`).
- ReleaseSafe:     stack overflow; the process dies (SIGABRT on macOS).
- ReleaseFast:     stack overflow; the process dies (SIGBUS on macOS).
- reachability:    VT stream: a `list-windows` reply or `%layout-change` with a deeply nested layout and a correct checksum. Measured, macOS arm64, Zig 0.16, `layout.zig` copied into a probe calling `Layout.parse` on a default `std.Thread` (16 MiB, as the io reader thread is spawned): overflow at about 9,200 levels in Debug and about 39,000 in ReleaseSafe and ReleaseFast, all well inside the 116,000 the input allows. Linux aarch64 (the VM, the same probe cross-compiled): identical, about 9,200 / 39,000 (SIGABRT / SIGABRT / SIGSEGV).
- port handling:   `Layout::parse` rejects nesting deeper than `MAX_DEPTH` (1000) with the existing `SyntaxError`, which the viewer turns into its defunct state; `Layout`'s `Drop` uses an explicit stack — `crates/rustty-vt/src/tmux/layout.rs:157` (PORT NOTE). The port needs the limit more than the reference: its reader thread has Rust's default 2 MiB stack, and several walkers recurse over the tree. Measured on macOS with the same kind of probe, the first to overflow did so at about 1,500 levels in a debug build (the parser) and about 2,400 in a release build (the derived `Debug` used by the action log), roughly six times sooner than the reference, not the eight an earlier estimate assumed. Linux aarch64 (the VM, 2026-09-11): parser about 1,480 (debug) / 6,270 (release), derived `Debug` about 1,625 / 2,415, `Clone` about 1,890 / 10,000, full viewer path about 1,480 / 6,150; every walker passes at `MAX_DEPTH` in both builds. The limit matches upstream tmux: commit `b7c5030004` (2026-09-09) added `LAYOUT_V1_MAX_DEPTH 1000` to `layout_construct_v1`, counting depth the same way (root is 0, reject when depth > 1000, checked before every cell). Older tmux has no cap, so a hand-written `select-layout` string could nest deeper there, but a layout built by splitting cannot realistically reach 1000. (Fix round 1: this sentence first said the limit was not checked against tmux's source.)
- parity row(s):   NEW-FINDINGS N42 (no LEDGER row).
- test:            `deeply_nested_layout_is_a_syntax_error_beyond_max_depth` (116,508 levels and `MAX_DEPTH + 1` rejected, `MAX_DEPTH` accepted, on a default thread), `dropping_a_very_deep_layout_does_not_recurse` (200,000 levels), and the viewer's `a_layout_nested_deeper_than_max_depth_makes_the_viewer_defunct` (the deepest 1 MiB `list-windows` reply as bytes, plus `MAX_DEPTH` accepted end to end). All three also pass on Linux aarch64 (VM), run individually in debug and release. Negative controls, macOS 2026-09-11: without the depth check the parse and viewer tests abort the test binary with "has overflowed its stack"; with recursive drop glue the drop test does the same.
- proposed fix:    Pass a depth to `parseNext` and return `error.SyntaxError` beyond a fixed limit, or parse with an explicit stack.
- upstream status: not reported

### UD11 — a tmux `list-panes` scroll region outside the pane is stored unchecked, and the next line feed crashes
- kind:            crash (ReleaseFast: ub)
- severity:        medium (anything written to the pty)
- upstream project: ghostty
- reference:       src/terminal/tmux/viewer.zig:1013-1024 (`receivedPaneState`, the `scroll:` block; the assignments are at :1022-1023), at 20abdb50a. The traps are src/terminal/Screen.zig:828 (`cursorDown`: `assert(self.cursor.y + n < self.pages.rows)`) and :1265 (`cursorScrollRegionUp`: `assert(limit >= 1)`).
- defect:          The `scroll:` block only checks that `scroll_region_upper`/`lower` fit `CellCountInt`, then stores them in `t.scrolling_region`. Nothing checks them against the pane's rows or each other. With a bottom at or past `rows`, `Terminal.index` treats a cursor on the last row as inside the region and not at its bottom, so it calls `cursorDown(1)` and trips the assert. With a one-row region away from the top (`top == bottom != 0`) and the cursor on it, `index` scrolls the region in place with `cursorScrollRegionUp(bottom - top)`, which is 0. The same function already ignores its other out-of-screen geometry: the cursor and the alternate saved cursor `break` out when `>= cols`/`>= rows` (:941-950, :968-982), and a tab stop `>= cols` is skipped (:1033). `setTopAndBottomMargin` (DECSTBM, Terminal.zig:2594-2602) refuses `top >= bottom`.
- ReleaseSafe:     panic on the assert at the next line feed or line wrap in that pane (or at the viewer's own `index()` loop after a history capture); the terminal process aborts.
- ReleaseFast:     undefined behaviour: the cursor row moves past the pane's rows.
- reachability:    VT stream: the `list-panes` reply to the viewer's pane-state command, for example `%7;0;23;1;;;;1;…;0;100;8` on a 24-row pane, then `%output` for that pane long enough to wrap. Found by the task R13-A reviewer.
- port handling:   the region is ignored unless `top < bottom < rows`, leaving the pane's current region, the way the same reply ignores bad cursor and tab-stop geometry — `crates/rustty-vt/src/tmux/viewer.rs:1665` (PORT NOTE). Inventory M111.
- parity row(s):   NEW-FINDINGS N44 (no LEDGER row).
- test:            `pane_state_scroll_region_outside_the_pane_is_ignored` (bytes through the VT stream; the reply sets the cursor on row 23, then 100 bytes of `%output` wrap): 0..10 is applied, while 0..100, 0..24, 23..23, 5..5 and 10..5 leave the region at 0..23. Negative control, macOS 2026-09-11: without the guard the test panics at `screen.rs:1577` (`assertion failed: self.cursor.y + n < self.pages.rows`); measured before the fix, 23..23 panicked at `screen.rs:2088` (`limit >= 1`), and 5..5 and 10..5 did not crash with this input.
- proposed fix:    In the `scroll:` block, `if (scroll_top >= scroll_bottom or scroll_bottom >= t.rows) break :scroll;` before assigning, matching the cursor and tab-stop checks beside it.
- upstream status: not reported

### UD12 — coloured pane history ends the tmux control-mode DCS, so the viewer disconnects
- kind:            latent (functional: lost tmux session)
- severity:        low
- upstream project: ghostty
- reference:       src/terminal/tmux/viewer.zig:1317 and :1331 (`capture-pane -p -e …`, the pane history and visible captures), src/terminal/parse_table.zig:71 (the "anywhere" `0x1B => escape` transition, which `dcs_passthrough` does not override), src/terminal/Parser.zig:280 (leaving `dcs_passthrough` emits `dcs_unhook`), at 20abdb50a
- defect:          Ghostty receives tmux control mode inside the VT parser's DCS state. The viewer asks for pane contents with `capture-pane -p -e`, and `-e` includes SGR escape sequences. tmux writes that reply to a control client raw (tmux `cmd-capture-pane.c:446-447`, confirmed from tmux's source by the task R13-A coordinator). The first ESC in the reply moves the parser from `dcs_passthrough` to `escape`, which unhooks the DCS: the tmux handler emits `.exit` and the viewer is freed. The rest of the tmux output is then parsed as ordinary terminal output in the surface.
- ReleaseSafe:     no crash; the tmux session ends as soon as any captured pane has coloured content, and later control-mode lines are processed as normal output.
- ReleaseFast:     same.
- reachability:    ordinary use: `tmux -CC attach` to a session with any coloured text in a pane's history or visible area.
- port handling:   faithful (R13 non-crash defect): the port's parser and DCS handler behave the same. Not changed; the right handling (for example a line-based control-mode reader outside the VT parser, or dropping `-e`) is a design question for upstream.
- parity row(s):   none.
- test:            none (recorded only).
- proposed fix:    Consume tmux control mode outside the VT parser once `ESC P 1000 p` is seen (reading raw lines until `%exit` and ST, as other tmux control-mode clients do), or request captures without `-e` and apply styles another way.
- upstream status: not reported

## Candidates pending verification

Assembled from this plan's progress notes on 2026-09-11. **Not verified** — do not cite as fact.

| # | reference | suspected defect | kind | port today | source |
|---|---|---|---|---|---|
| C3 | `src/renderer/Thread.zig:698-723` `cursorCancelCallback` | `unreachable` on cancel errors other than Canceled/NotFound; reachability depends on what the event loop can return | crash? | **ported as `unreachable!()`** (task 14d-1, S114-F2) — reachability must be decided | task 14d-1 |
| C4 | `src/termio/Exec.zig:1045`; `src/apprt/gtk/pre_exec.zig:39,58` | `log.err` between fork and exec: the log function allocates and locks, so a child forked from a multi-threaded parent can deadlock silently | deadlock | handled safely (raw `write`, tasks 14d-3 and 14e-1) | tasks 14d-3, 14e-1 |
| C5 | `src/font/DeferredFace.zig:154-157, 186-197` | zero-copy `cstringPtr` path returns into a +1 CFString that is never released | leak | rows S85-F6 / S90-F3 left OPEN | task 16 |
| C6 | `src/apprt/gtk/noop.zig` `updateConfigEvent` | references `ApprtWindow.DerivedConfig`, which does not exist; compiles only because nothing calls it | latent | N32 | task 18 |
| C7 | `src/renderer/Thread.zig:672` `cursorTimerCallback` | log message says "render callback" — copy-pasted from the render callback | latent | kept verbatim (R1) | task 14d-1 review |
| C8 | `src/renderer/OpenGL.zig:316-327` `present` | `BlitFramebuffer` unchecked; its GL error surfaces later inside the SRGB re-enable, misattributed | latent | faithful; restored log is the only report | task 14c-3 |
| C9 | `src/font/shaper/coretext.zig`, `harfbuzz.zig` `debugPositions` | dead private functions, only callers commented out | latent | N22 dead-site ratchet | task 14e-2 |
| C10 | `src/terminal/PageList.zig` reset test (~`:18963`) | the `!nodeIsValid` assertion is defended by two guards, so breaking either alone stays green | test-weakness | ported verbatim (S131-F1) | task 17 |
| C11 | `src/font/face/coretext.zig:1023-1036`; `src/terminal/StringMap` tests | assertions cannot distinguish the fixture under test | test-weakness | ported verbatim (S90-F5, S140-F1) | task 17 |
| C12 | `src/lib/enum.zig:126` `checkGhosttyHEnum` | iterates only the enum's fields, so a header enumerator the enum lacks is never caught | test-weakness | faithful (N36) | task 19 review |
| C13 | `src/apprt/gtk/media.zig` `bellMediaFile` | a GstPlay teardown race required `mem::forget` in the port's test; whether production code races is unknown | crash? | test-only workaround (S48-F1) | task 19 |
