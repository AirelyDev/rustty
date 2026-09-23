# Upstream defects — Ghostty bugs found while porting

Defects **in the Ghostty reference itself** (pin `20abdb50a`), recorded so fixes can be
proposed upstream. Governed by `.port/PORT_RULES.md` **R13**: a defect reachable from real
input is never ported; the port handles it safely and the defect is recorded here.

Some entries are in a dependency Ghostty pins (libxev, libvaxis, GStreamer, google-highway)
rather than in Ghostty's own code. Their `upstream project` field names that project, because
the report goes to its tracker, not Ghostty's.

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
- upstream project: ghostty | <dependency>   (the project whose tracker gets the report, e.g. libxev, libvaxis, GStreamer, google-highway)
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
- upstream project: ghostty
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
- upstream project: ghostty
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
- upstream project: ghostty
- severity:        medium (reachable from another X client on the same display)
- reference:       src/apprt/gtk/winproto/x11.zig:346-431 (`getWindowProperty` from :346, `changeProperty` :392-422, `deleteProperty` :424-431), reached from `resizeEvent` / `syncAppearance` (:218-235) via `syncBlur` (:243-287) and `syncDecorations` (:289-), at 20abdb50a
- defect:          Two faults.
  - **No error trap.** `XChangeProperty`, `XDeleteProperty` and `XGetWindowProperty` are called with no `gdk_x11_display_error_trap_push`/`_pop`, so an X error on them reaches GDK's default X error handler. That handler prints "The program '…' received an X Window System error … BadWindow" and calls `exit(1)`.
  - **A dead status check.** `if (status == 0) return error.RequestFailed` can never fire: libX11 1.8.13's `XChangeProperty`/`XDeleteProperty` have a single return path after `mov w0, #1` (task-20 reviewer's disassembly), and a real failure arrives asynchronously. The cache-preserving logic in `syncBlur` behind that check is therefore dead too.
  (Fix rounds 1–2: this entry was first `latent`, and said a failed request was "no crash; the cache disagrees with the server". That was wrong. The reviewer's GTK 4.22.4 probe showed an untrapped `BadWindow` exits the process with status 1, and the same call inside the GDK trap returns error 3 and survives.)
- ReleaseSafe:     the process exits with status 1 inside GDK's error handler (not a Zig safety check, so every build mode).
- ReleaseFast:     same.
- reachability:    MEASURED by task 20 on the VM (GTK 4.22.4, GDK_BACKEND=x11, headless sway's Xwayland), one scenario per process, with blur enabled. "Pump" means 1000 main-loop iterations after the destroy, which lets GDK process the DestroyNotify:
  1. A second X client destroys our mapped toplevel (`XDestroyWindow`, as `xdotool windowclose` does), and the GTK loop is pumped for 1000 iterations: SURVIVES. GDK logs "GdkSurface 0x… unexpectedly destroyed" and keeps the window realized and mapped; nothing issues a winproto request.
  2. The same destroy with NOTHING PUMPED, then `syncAppearance` directly: EXITS 1, `BadWindow … request_code 20` (`GetProperty`, from `syncDecorations`).
  3. GTK's own `gtk_window_destroy`, then `syncAppearance`: EXITS 1, `request_code 19` (`DeleteProperty`, from `syncBlur`). No natural in-app sequence was found that calls `syncAppearance` on a window after GTK destroyed it, so this is exposure, not a verified trigger.
  4. The external destroy with NOTHING PUMPED, then `maximize`: EXITS 1, `request_code 8` (`MapWindow`). This is a race, and the request is GDK's own. After a pump, `maximize`/`unmaximize` SURVIVE in the port (fix-round-3 reviewer) — but a hide/show after a pump exits on GDK's own request 18. Both reproduce in a plain GTK window with no app X calls, so they are GTK's (UD56), not this entry's, and **the port's maximize and hide/show paths remain exposed**: the trap here covers only the winproto's three calls.
  5. The external destroy, pump, then a CONFIG RELOAD delivered to the window (`notify::config` → `syncAppearance`): EXITS 1, `request_code 20`. This is the realistic trigger: another client on the display closes the window, and the user later reloads the config.
  6. The same, but the reload turns blur OFF (found by the fix-round-3 reviewer): EXITS 1 on `request_code 19` (`DeleteProperty`, `syncBlur`). This is the case where a failed write is invisible to the status check, because `XDeleteProperty` returned 1.
- port handling:   `crates/rustty-apprt/src/gtk/winproto/x11.rs` `Window::trap_x_errors` (R13 PORT NOTE) brackets each of the three calls with `gdk_x11_display_error_trap_push`/`_pop` (gdk4-x11's `X11Display::error_trap_push`/`error_trap_pop`, GDK's own scoped trap, never a process-wide `XSetErrorHandler`), held by an `XErrorTrap` drop guard that pops with `error_trap_pop_ignored` if the call panics. A trapped error takes the reference's `error.RequestFailed` path, which `resizeEvent`/`syncAppearance` already log as `failed to sync blur=`/`failed to sync decorations=`. The dead `status == 0` checks stay, faithfully (S60-F1).
- parity row(s):   S60-F1, S60-F6; inventory item M110.
- test:            `x11_error_on_a_destroyed_window_does_not_exit_the_process` runs, in a child copy of the test binary, the external destroy + pump followed by (a) a blur-OFF reload, which forces `XDeleteProperty`, and (b) a blur-ON reload with the cached region replaced by a sentinel, which forces `XChangeProperty`. The child reports whether the cached region survived each write unchanged, which happens only when the TRAP RESULT turned the write into `RequestFailed`. The parent asserts exit 0 and `delete_trapped=true change_trapped=true`. On the VM under Xwayland it passes 3/3; with no display it FAILS. Negative controls, VM 2026-09-11: (1) popping with `error_trap_pop_ignored` so `trapped` is always false: the child survives but reports `delete_trapped=false change_trapped=false`, RED; (2) the trap removed from `XChangeProperty`/`XDeleteProperty` only: the child exits with status 1 on `BadWindow … request_code 19`, RED. (Fix round 3: this line first said the test asserted a trapped `failed to sync` log line. That line comes from `XGetWindowProperty`'s own reply code, so it proved nothing about the trap; the reviewer showed both mutations above left the old test GREEN.) `sync_blur_keeps_the_cached_region_when_the_property_write_fails` still covers the status path with a stub `XDeleteProperty`.
- proposed fix:    Wrap the property calls in `gdk_x11_display_error_trap_push (display)` / `if (gdk_x11_display_error_trap_pop (display) != 0) return error.RequestFailed;` (zig-gobject `gdkx114.zig:205-206` `errorTrapPop`, `:212-213` `errorTrapPopIgnored`, `:220-221` `errorTrapPush`, on the window's display). Replace the `status == 0` checks, which cannot fire, with that trap result, so `syncBlur`'s commit-on-success works as intended. Note that these functions are DEPRECATED since GTK 4.18, along with GDK's X11 backend API as a whole. GDK 4 offers no non-deprecated way to trap X errors, so this is the only fix available short of moving off Xlib. This fixes the winproto's own requests only; GDK's own requests against an externally destroyed window are UD56.
- upstream status: not reported

### UD4 — `gotoWindow(.previous)` dereferences a null toplevel list
- kind:            crash
- upstream project: ghostty
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
- upstream project: ghostty
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
- upstream project: ghostty
- severity:        high (anything written to the pty; also a real `tmux -CC` re-attach)
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
- upstream project: ghostty
- severity:        high (anything written to the pty)
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
- upstream project: ghostty
- severity:        high (anything written to the pty)
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
- upstream project: ghostty
- severity:        low
- reference:       src/terminal/tmux/viewer.zig:1070-1077 (`receivedPaneHistory`: `if (comptime std.debug.runtime_safety) { ... assert(discarding.count == 0); }`), at 20abdb50a
- defect:          After writing the history, the viewer does `carriageReturn()` and `rows` × `index()` and asserts the active area is empty. `index()` scrolls only at the bottom margin; below the scroll region it just moves the cursor down. If the pane's scroll region and cursor are already set, text written below the region stays in the active area. `receivedPaneState` (the `list-panes` reply) sets both, and it can arrive before a pane's history: when a second pane is added while the first pane's captures are still queued, the queue holds the first pane's `list-panes` before the second pane's captures, and that reply covers both panes. (History bytes cannot set the region themselves: an ESC ends the DCS.)
- ReleaseSafe:     panic on the assert (Debug too); the terminal process aborts.
- ReleaseFast:     no check, and the history is damaged, not merely left in the active area: with the cursor stuck on the last row below the scroll region, `index()` never scrolls, so each history line overwrites the previous one on that row and the scrollback the capture was meant to fill is garbled or lost. The pane's `capture-pane` visible reply then runs `eraseDisplay(.complete)` over the active area. (Fix round 1, task R13-A review: this line first said only that the active area "keeps the text".)
- reachability:    VT stream (a forged exchange), and plausibly a real tmux: a split created during attach, in a pane running a program with a scroll region above the cursor row (vim's command line, for example).
- port handling:   no check, as ReleaseFast — `crates/rustty-vt/src/tmux/viewer.rs:1787` (PORT NOTE).
- parity row(s):   NEW-FINDINGS N43 (no LEDGER row).
- test:            `pane_history_below_the_scroll_region_does_not_trip_the_empty_active_area_check` (bytes through the VT stream: list-windows with pane 7, a `%layout-change` adding pane 8, pane 7's captures, a `list-panes` giving pane 8 cursor row 23 and region 0-10, then pane 8's history; asserts the viewer is not defunct and the history text is in pane 8's active area). Negative control, macOS 2026-09-11: with the port's former `debug_assertions` check the test panics in `inlineAssert` from `received_pane_history` (backtrace confirmed).
- proposed fix:    Drop the assert, or make the intent true: reset the scroll region (and origin mode) before the `index()` loop, or clear the active area with `eraseDisplay(.complete, false)` instead of scrolling it.
- upstream status: not reported

### UD10 — tmux layout parsing recurses without a depth limit
- kind:            crash (stack overflow)
- upstream project: ghostty
- severity:        medium (anything written to the pty)
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
- upstream project: ghostty
- severity:        medium (anything written to the pty)
- reference:       src/terminal/tmux/viewer.zig:1013-1024 (`receivedPaneState`, the `scroll:` block; the assignments are at :1022-1023), at 20abdb50a. The traps are src/terminal/Screen.zig:828 (`cursorDown`: `assert(self.cursor.y + n < self.pages.rows)`) and :1265 (`cursorScrollRegionUp`: `assert(limit >= 1)`).
- defect:          The `scroll:` block only checks that `scroll_region_upper`/`lower` fit `CellCountInt`, then stores them in `t.scrolling_region`. Nothing checks them against the pane's rows or each other. With a bottom at or past `rows`, `Terminal.index` treats a cursor on the last row as inside the region and not at its bottom, so it calls `cursorDown(1)` and trips the assert. With a one-row region away from the top (`top == bottom != 0`) and the cursor on it, `index` scrolls the region in place with `cursorScrollRegionUp(bottom - top)`, which is 0. The same function already ignores its other out-of-screen geometry: the cursor and the alternate saved cursor `break` out when `>= cols`/`>= rows` (:941-950, :968-982), and a tab stop `>= cols` is skipped (:1033). `setTopAndBottomMargin` (DECSTBM, Terminal.zig:2594-2602) refuses `top >= bottom`.
- ReleaseSafe:     panic on the assert at the next line feed or line wrap in that pane (or at the viewer's own `index()` loop after a history capture); the terminal process aborts.
- ReleaseFast:     undefined behaviour, differing by case. A bottom at or past `rows`: `cursorDown` moves the cursor row past the pane's rows. A one-row region away from the top: from reading `Screen.zig:1264-1360` (not run), `cursorScrollRegionUp(0)` takes its fast path (`pin.y < 0` can never hold), clears only the cursor row instead of scrolling (the rotated slice is that one row, so `rotateOnce` is a no-op), and still shifts the viewport's cached row offset and every other tracked pin on that row up one row, although nothing moved. (Fix round 1, task R13-A review: this line first described only the first case.)
- reachability:    VT stream: the `list-panes` reply to the viewer's pane-state command, for example `%7;0;23;1;;;;1;…;0;100;8` on a 24-row pane, then `%output` for that pane long enough to wrap. Found by the task R13-A reviewer.
- port handling:   the region is ignored unless `top < bottom < rows`, leaving the pane's current region, the way the same reply ignores bad cursor and tab-stop geometry — `crates/rustty-vt/src/tmux/viewer.rs:1665` (PORT NOTE). Inventory M111.
- parity row(s):   NEW-FINDINGS N44 (no LEDGER row).
- test:            `pane_state_scroll_region_outside_the_pane_is_ignored` (bytes through the VT stream; the reply sets the cursor on row 23, then 100 bytes of `%output` wrap): 0..10 is applied, while 0..100, 0..24, 23..23, 5..5 and 10..5 leave the region at 0..23. Negative control, macOS 2026-09-11: without the guard the test panics at `screen.rs:1577` (`assertion failed: self.cursor.y + n < self.pages.rows`); measured before the fix, 23..23 panicked at `screen.rs:2088` (`limit >= 1`), and 5..5 and 10..5 did not crash with this input.
- proposed fix:    In the `scroll:` block, `if (scroll_top >= scroll_bottom or scroll_bottom >= t.rows) break :scroll;` before assigning, matching the cursor and tab-stop checks beside it.
- upstream status: not reported

### UD12 — coloured pane history ends the tmux control-mode DCS, so the viewer disconnects
- kind:            latent (functional: lost tmux session)
- upstream project: ghostty
- severity:        low
- reference:       src/terminal/tmux/viewer.zig:1317 and :1331 (`capture-pane -p -e …`, the pane history and visible captures), src/terminal/parse_table.zig:71 (the "anywhere" `0x1B => escape` transition, which `dcs_passthrough` does not override), src/terminal/Parser.zig:280 (leaving `dcs_passthrough` emits `dcs_unhook`), at 20abdb50a
- defect:          Ghostty receives tmux control mode inside the VT parser's DCS state. The viewer asks for pane contents with `capture-pane -p -e`, and `-e` includes SGR escape sequences. tmux writes that reply to a control client raw (tmux `cmd-capture-pane.c:447-448` at tmux `b7c5030004`, `:446-447` on later master; confirmed from tmux's source by the task R13-A coordinator). The first ESC in the reply moves the parser from `dcs_passthrough` to `escape`, which unhooks the DCS: the tmux handler emits `.exit` and the viewer is freed. The rest of the tmux output is then parsed as ordinary terminal output in the surface.
- ReleaseSafe:     no crash; the tmux session ends as soon as any captured pane has coloured content, and later control-mode lines are processed as normal output.
- ReleaseFast:     same.
- reachability:    ordinary use: `tmux -CC attach` to a session with any coloured text in a pane's history or visible area.
- port handling:   faithful (R13 non-crash defect): the port's parser and DCS handler behave the same. Not changed; the right handling (for example a line-based control-mode reader outside the VT parser, or dropping `-e`) is a design question for upstream.
- parity row(s):   none.
- test:            none (recorded only).
- proposed fix:    Consume tmux control mode outside the VT parser once `ESC P 1000 p` is seen (reading raw lines until `%exit` and ST, as other tmux control-mode clients do), or request captures without `-e` and apply styles another way.
- upstream status: not reported

### UD13 — GstPlay can finalize on its own play thread when the last `GtkMediaFile` reference drops right after construction
- kind:            ub (crash inside GStreamer)
- upstream project: GStreamer (gst-plugins-bad `gst-libs/gst/play`), reached through GTK4's GStreamer media backend; owning component UNVERIFIED (see defect)
- severity:        low
- reference:       Ghostty trigger sites at 20abdb50a: src/apprt/gtk/media.zig:64-72 (`bellMediaFile` unrefs the cached `MediaFile` at :71 when the path changes), its production caller src/apprt/gtk/class/application.zig:2167 (`priv.bell_media = media.bellMediaFile(priv.bell_media, path, required)`), dispose at application.zig:1570-1573, and the test's final `current.unref()` at media.zig:161. No GStreamer source was read for this entry.
- defect:          `GtkMediaFile` (GTK4 GStreamer backend) is backed by `GstPlay`, which runs a `gst_play_main` thread. The rustty port measured that dropping the last reference within microseconds of construction races that thread's startup: the bus message the play thread posts ends up holding the final `GstPlay` reference, so `gst_play_finalize` runs on the play thread and clears a mutex that thread still holds. Two measured signatures (Fedora 44 / aarch64 / GStreamer 1.28.1, 2026-07-31, `.port/phase-3/UPSTREAM_BUGS.md` U1):
  ```
  SIGABRT: g_mutex_clear() called on uninitialised or locked mutex
    g_mutex_clear <- gst_play_finalize <- g_object_unref <- _gst_message_free
    <- gst_bus_post <- api_bus_post_message <- gst_play_set_uri_internal
  SIGSEGV: gst_object_ref on a freed object
    g_type_check_instance_is_fundamentally_a <- g_object_ref <- gst_object_ref
    <- gst_message_new_custom <- api_bus_post_message <- on_error
    <- gst_play_pause_internal
  ```
  The port's unref-ing test crashed in about 30% of runs; calling `gtk_media_stream_clear()` first narrowed it to 2/60. **UNVERIFIED:** a minimal pure-GTK4 C reproducer (`gtk_media_file_new_for_filename` then immediate `g_object_unref`, on the main thread and on a `GThread`) did not reproduce (0/20 and 0/25, same record), so which component owns the fault (GstPlay, or GTK's GStreamer media backend) is not established. The GTK version in that measurement was not recorded.
- ReleaseSafe:     SIGABRT or SIGSEGV inside GStreamer; the terminal process dies. Independent of the Zig build mode.
- ReleaseFast:     same.
- reachability:    timing. In Ghostty production the last reference drops only when `bell-audio-path` has changed and the bell then rings (the old `MediaFile` is unref'd at media.zig:71), or at application dispose. Both normally happen long after that pipeline started. Production reach is **UNVERIFIED** (no trigger measured outside tests). Ghostty's own test would expose it on every run that gets past its skip, which today is none (UD14).
- port handling:   faithful in production: `crates/rustty-apprt/src/gtk/media.rs:59-64` drops the replaced handle exactly as media.zig:71 does. Only the port's test leaks its final handles (`media.rs:212` and `:217`, `std::mem::forget`, reasoning inline). No R13 PORT NOTE: the defect is not in the reference's code.
- parity row(s):   S48-F1 (CLOSED, test only); phase-3 U1; inventory M43.
- test:            none proving safety (no GTK-level API closes the window). The measurements came from the port's `bell_media_file_reuses_one_per_path` before its `forget` was added.
- proposed fix:    To GStreamer, once the owning component is confirmed: `GstPlay` must not let a bus message posted from `gst_play_main` hold the last reference to the `GstPlay` object, or `gst_play_finalize` must tolerate running on the play thread (for example, take an extra reference across `api_bus_post_message`). The report needs the two backtraces and a reproducer that actually reproduces; the C reproducer above did not, so reduction is the first step. No Ghostty-side change is proposed while production reach is unproven.
- upstream status: not reported (search GStreamer GitLab for GstPlay finalize-on-play-thread reports first)

### UD14 — `bellMediaFile reuses one MediaFile per path` always skips: its paths never exist
- kind:            test-weakness (plus a wrong comment)
- upstream project: ghostty
- severity:        low
- reference:       src/apprt/gtk/media.zig:136-162 (test; comment at :144, paths at :145-146, skips at :148 and :157), `fromFilename` at :12-23, at 20abdb50a
- defect:          The test comment says "The files need not exist: MediaFile only records the path until played", and the test uses `/tmp/ghostty-bell-test-a.oga` and `-b.oga` without creating them. `bellMediaFile` calls `fromFilename`, which first runs `std.Io.Dir.accessAbsolute(..., .{ .read = true })` and returns null when that fails (:14-21). So wherever those files are absent, `bellMediaFile(null, path_a, false) orelse return error.SkipZigTest` (:148) returns `SkipZigTest` before any assertion, and none of the reuse assertions (:150, :153-154, :158-159) runs. The commit that added the test (`0708f932a`) says it "Runs in the existing test-gtk CI job"; that job (`.github/workflows/test.yml:1552-1590`) runs `zig build -Dapp-runtime=gtk … test` and creates no such files.
  - **Change it fails to catch** (by reading): make `bellMediaFile` rebuild every time, for example by deleting `if (isForPath(media_file, path)) return media_file;` at :70. That is the per-bell pipeline leak the test was added to guard (`0b6d91e53`, `0708f932a`). The test still reports skipped, not failed.
  - **UNVERIFIED:** whether Ghostty's test build analyses `media.zig`'s tests at all. `media.zig` is imported only as a private `const media` in `class/application.zig:48`, and `src/apprt/gtk.zig:12-17` uses non-recursive `refAllDecls`. Either way the assertions do not run in CI.
- ReleaseSafe:     no product effect; the regression guard is inert.
- ReleaseFast:     same.
- reachability:    none (test code).
- port handling:   the port runs the test for real: it creates both files (`crates/rustty-apprt/src/gtk/media.rs:150-155`), turns the skip into a loud failure (`:157-165`), and is `#[ignore]`d because gtk4-rs needs a display (`:132`). Running it is how UD13 was found.
- parity row(s):   S48-F1 (context); inventory M45.
- test:            n/a (record only).
- proposed fix:    Create the two files in a `std.testing.tmpDir` and pass their absolute paths; delete the "files need not exist" sentence; replace `orelse return error.SkipZigTest` with a hard failure once the files exist. When enabling it, avoid dropping the last `MediaFile` reference immediately after construction because of UD13. If the test cannot run without a display, gate on that explicitly rather than on file existence.
- upstream status: not reported

### UD15 — PageList reset test cannot fail when either `nodeIsValid` guard is removed
- kind:            test-weakness
- upstream project: ghostty
- severity:        low
- reference:       src/terminal/PageList.zig:18921-18967 (`test "PageList reset invalidates stale untracked refs even if node memory is reused"`; comment :18958-18960, assertion :18963); `nodeIsValid` at :5701-5716 with the epoch guard at :5708 and the serial comparison at :5712; `reset` sets the epoch at :955, at 20abdb50a
- defect:          The test forces node-address reuse across resets and asserts `!s.nodeIsValid(old_node, old_serial)` where `old_node == new_node` (:18961-18963). Its comment says reset rejects "the stale generation before inspecting its pointer", which is the epoch guard's job (:5708). But `old_serial` is rejected twice:
  1. `serial < self.page_serial_epoch` at :5708 returns false; and
  2. if that line is removed, the list scan finds `node == target` and returns `node.serial == serial` at :5712, which is false because the reset node has a fresh serial (the test itself asserts `new_serial >= s.page_serial_epoch` at :18965).
  - **Changes it fails to catch** (by reading): delete :5708 (green; `nodeIsValid` silently becomes an O(n) scan for pre-reset refs), or make :5712 `return true` (green, because :5708 still rejects). It goes red only when both are broken. The rustty port measured exactly this on its line-for-line translation (LEDGER S131-F1: "breaking either one alone leaves it green").
  - Since reset always re-serials nodes, the epoch guard is behaviourally a shortcut, so no pure `nodeIsValid` result can distinguish its removal; the comment claims more than a black-box test can show.
- ReleaseSafe:     no product effect.
- ReleaseFast:     same.
- reachability:    none (test code).
- port handling:   faithful: `crates/rustty-vt/src/pagelist/tests.rs:1728` (`pagelist_reset_invalidates_stale_untracked_refs_even_if_node_memory_is_reused`).
- parity row(s):   S131-F1 (CLOSED, unchanged); inventory M46.
- test:            n/a (record only).
- proposed fix:    Test each guard on its own: (a) call `nodeIsValid` on a node still in the list with a serial below the epoch, so only :5708 can reject; (b) a non-reset test in which a destroyed node's address is reused by a new node, so only :5712 can reject. (`:8742` does not cover (b): it asserts the replacement has a different address, so the scan simply misses. Whether another existing test covers (b) was not checked.) Or reword the comment so it does not claim to test the epoch shortcut.
- upstream status: not reported

### UD16 — `checkGhosttyHEnum` never notices a header enumerator that has no Zig field
- kind:            test-weakness (enhancement; the doc comment scopes the check one way)
- upstream project: ghostty
- severity:        low
- reference:       src/lib/enum.zig:105-163 (`checkGhosttyHEnum`; doc :105-107, field loop :126-146, final set check :149-162); callers src/apprt/action.zig:26,433,514,530,541,557,592,602,612,623,663,672,681,692,959,989,1065, src/apprt/ipc.zig:28,182, src/renderer.zig:52, src/terminal/mouse.zig:96, src/terminal/osc.zig:246; header include/ghostty.h, at 20abdb50a
- defect:          The function iterates `@typeInfo(T).@"enum".fields` and, for each Zig field, compares its value with `@field(c, PREFIX ++ UPPER(name))` from the translated `ghostty.h`; a missing header value leaves the field in `set` and fails at :149. It never enumerates the header side, so a `GHOSTTY_<PREFIX>_*` enumerator with no Zig field is never examined.
  - **Changes it fails to catch** (by reading): (1) delete the last variant of a checked enum while leaving its `GHOSTTY_…` enumerator in `ghostty.h`: every remaining field still matches, so the test stays green and the header advertises a value libghostty never produces. (2) append `GHOSTTY_IPC_ACTION_FOO` to `ghostty_ipc_action_tag_e` in the header with no Zig field: green, although a C caller can now pass a value the exhaustive `ipc.Action.Key` cannot represent. (An insertion in the middle is caught, because it shifts later values.)
  - The doc comment says "for every key in enum T", so upstream may consider this by design.
- ReleaseSafe:     no product effect by itself; a stale or extra header enumerator ships unnoticed.
- ReleaseFast:     same.
- reachability:    none (test code).
- port handling:   faithful one-directional check: `crates/rustty-lib/src/enum.rs:162` (`check_ghostty_henum`), callers such as `crates/rustty-apprt-api/src/action_tests.rs`. The port's own variant of this blind spot (hand-typed key lists) is NEW-FINDINGS N36.
- parity row(s):   NEW-FINDINGS N26, N36 (unchanged); inventory M47.
- test:            n/a (record only).
- proposed fix:    After the field loop, iterate `@typeInfo(c).@"struct".decls` and fail on any decl that starts with `prefix` and matched no field. A plain prefix scan is not enough, because prefixes overlap in the header: `GHOSTTY_ACTION_` also matches `GHOSTTY_ACTION_OPEN_URL_KIND_*`, `GHOSTTY_ACTION_CLOSE_TAB_MODE_*` and `GHOSTTY_ACTION_OPEN_CONFIG_*`, and `GHOSTTY_MOUSE_` (used for `MouseVisibility`, action.zig:681) also matches `GHOSTTY_MOUSE_SHAPE_*`, the mouse buttons (`GHOSTTY_MOUSE_LEFT`, include/ghostty.h:135) and `GHOSTTY_MOUSE_MOMENTUM_*` (:149-155). So either pass an explicit list of excluded sub-prefixes, or give each C enum a `_COUNT`/`_MAX` sentinel and compare it with `enum_fields.len`.
- upstream status: not reported

### UD17 — snapshot decode allocation-failure test cannot see a site that swallows OOM
- kind:            test-weakness
- upstream project: ghostty
- severity:        low
- reference:       src/terminal/snapshot/snapshot.zig:1637-1697 (`test "complete snapshot decode allocation failures are transactional"`, `saw_out_of_memory` check at :1676); Zig 0.16 `std/testing/FailingAllocator.zig:58-78` (`alloc`); the heap-OOM-degrading site src/terminal/snapshot/screen.zig:1117-1140 (`decodeCursorHyperlink`, `error.OutOfMemory` at :1133), at 20abdb50a
- defect:          For each `fail_index` below the baseline allocation count, the test accepts either `error.OutOfMemory` or a successful decode, and only requires that some index produced OOM. Two properties make a swallowed failure invisible:
  1. `FailingAllocator` is sticky: once `alloc_index == fail_index` it returns null for that and every later allocation, because `alloc_index` only advances on success (`FailingAllocator.zig:65-76`). If a decode site catches its OOM and continues, the next allocation fails too and a later site propagates `OutOfMemory`, so that index still counts as OOM.
  2. When no later allocation exists, the decode returns successfully and the test deinits the result without comparing it to the baseline.
  - **Change it fails to catch** (by reading): turn a propagating allocation into a silent degrade, for example make `grid.StyleRemap.init(alloc) catch return error.OutOfMemory` (snapshot/page.zig:374) fall back to an empty remap. The decoded terminal loses its styles under OOM, and the test stays green.
  - The reference does contain one deliberate heap-allocator degrade (`decodeCursorHyperlink` maps `error.OutOfMemory` to "no cursor hyperlink", screen.zig:1133), so "every index must be OOM" is not the right assertion either. The rustty port measured the "still reached but stops propagating" blind spot on its own translation (LEDGER S206-F1); on the reference it is by reading, **UNVERIFIED** (not run).
- ReleaseSafe:     no product effect; OOM data-loss regressions in the decoder would pass CI.
- ReleaseFast:     same.
- reachability:    none (test code).
- port handling:   faithful acceptance of `Ok`, with the limit documented at `crates/rustty-vt/src/snapshot/codec.rs:2629-2640` (that note's cited reason was corrected in task R13-R: the reference's own swallowing site is `decodeCursorHyperlink`, not `decodePageString`, which allocates from the page string allocator the `FailingAllocator` never sees).
- parity row(s):   S206-F1 (CLOSED, unchanged); inventory M48.
- test:            n/a (record only).
- proposed fix:    Use a one-shot failure (fail exactly allocation `i` and let later ones succeed, via a small wrapper allocator), and on success re-encode the decoded terminal and compare with the baseline bytes. Allow a mismatch only for an explicit list of documented degradations (today the cursor hyperlink). That catches both a masked swallow and a silent data loss.
- upstream status: not reported

### UD18 — Config clone tests cannot catch a field that `clone` stops copying
- kind:            test-weakness
- upstream project: ghostty
- severity:        low
- reference:       src/config/Config.zig:5120-5127 (`cloneEmpty` starts from `default()`), :5133-5171 (`clone`, field loop :5143-5150); tests `clone default` :10596, `clone preserves conditional state` :10613, `clone can then change conditional state` :10632, `clone preserves conditional set` :10706, `changed` :10750, at 20abdb50a
- defect:          `clone` begins with `cloneEmpty`, which calls `default()`, then overwrites each field with a deep copy. Every source config in these tests is `Config.default(alloc)` plus, at most, a light/dark theme (`clone can then change conditional state`) or `--theme`/`--window-theme` (`clone preserves conditional set`). So for any field those fixtures leave at its default, a `clone` that skips the field produces a destination equal to the source anyway, and `changeIterator`/`changed` report no difference.
  - **Change it fails to catch** (by reading): add `if (comptime std.mem.eql(u8, field.name, "keybind")) continue;` inside the loop at :5143. The clone keeps the default keybinds instead of the source's, and all five tests stay green. The same holds for most fields: the only field observed on a clone with a non-default value is `background` (`cfg_clone.background` in `clone can then change conditional state`). Also green by reading: replacing the deep copy with a shallow `@field(result, name) = @field(self, name)`, since each test deinits the clone before the source.
  - The rustty port measured the fixture problem on its translation (LEDGER S77-F2); on the reference it is by reading.
- ReleaseSafe:     no product effect; a clone regression would ship. `clone` feeds `changeConditionalState` (light/dark theme switching).
- ReleaseFast:     same.
- reachability:    none (test code).
- port handling:   faithful: `crates/rustty-config/src/config_struct.rs:11738` (`clone_default`) through `:11799` (`changed`) and the conditional-state tests after it.
- parity row(s):   S77-F2 (CLOSED, unchanged); inventory M49.
- test:            n/a (record only).
- proposed fix:    Add a clone test whose source sets a non-default value for each category of field, at least `keybind` (for example `--keybind=ctrl+a=new_window`), a repeatable string, a `RepeatablePath` and an optional, then assert `source.changeIterator(&dest).next() == null` and, for the keybind, that `dest.keybind.set` contains the custom trigger.
- upstream status: not reported

### UD19 — ReadThread drops the child's final output when `POLLHUP` arrives with `POLLIN`
- kind:            latent (functional: lost output)
- upstream project: ghostty
- severity:        medium (if the loss is confirmed at the pin; see reachability)
- reference:       src/termio/Exec.zig:1716-1731 (outer `poll`, quit check, then `if (pollfds[0].revents & posix.POLL.HUP != 0) { …; return; }` at :1727-1730) in `ReadThread.threadMainPosix` (:1411); contrast the bridge-poll arm at :1657-1662, at 20abdb50a
- defect:          After the gather stage drains the pty to `WouldBlock` and publishes the batch, the thread blocks in `poll(pollfds[0..2], -1)`. When it wakes it returns as soon as `POLLHUP` is set, without checking `POLLIN` and without a final read. If the child writes and then exits while the thread is parked in that poll, `revents` can carry both, and the buffered bytes are never read. The same function's bridge poll gets this right ("HUP without IN means no more data is coming", :1657-1662, which stops gathering only when `POLLIN` is clear).
  - Measured on the rustty port (Linux, before the two-stage pipeline): `revents == 0x11` (`POLLIN|POLLHUP`), and at the HUP return the pty still held the unread `"RUSTTY_OK\r\n"` (`.port/phase-2/DEFERRALS.md:7149-7215`). macOS was not affected: a dying child surfaced as `read() == 0`, which the loop handles (the reference's comment at :1693-1695 says so).
  - **UNVERIFIED at the pin: the loss has not been re-measured on the current two-stage pipeline code. Needs a Linux VM run before filing** (for example a `/bin/sh -c 'printf done'` child, repeated, checking whether `done` reaches the terminal). The code path above is confirmed by reading.
- ReleaseSafe:     no crash; the last output of a short-lived command can be missing from the terminal.
- ReleaseFast:     same.
- reachability:    ordinary use on Linux: a command that prints and exits immediately (`ghostty -e …`, or a shell's final output at exit), most visible with `wait-after-command`. Timing-dependent: only when the output lands while the reader is parked in the outer poll.
- port handling:   faithful ("PRESERVED REFERENCE QUIRK"): `crates/rustty-termio/src/exec.rs:5088-5092`. The port's live-shell test sidesteps it with a trailing `sleep 1` (`crates/rustty-termio/tests/live_shell.rs:297-332`).
- parity row(s):   none (phase-2 DEFERRALS reclassified entry; `.port/phase-3/DEFERRAL_REAUDIT.md:255`); inventory M14.
- test:            n/a (record only).
- proposed fix:    On wake, if `POLLIN` is set, loop back into the gather stage before honouring `POLLHUP`; return only when `POLLHUP` is set and `POLLIN` is clear (or a read returns 0 or EIO), mirroring the bridge-poll arm.
- upstream status: not reported (blocked on the Linux measurement above)

### UD20 — custom-shader uniforms `iSelectionForegroundColor` and `iSelectionBackgroundColor` are swapped
- kind:            latent (functional: wrong colours)
- upstream project: ghostty
- severity:        low
- reference:       src/renderer/shaders/shadertoy_prefix.glsl:29-30 (`iSelectionForegroundColor` then `iSelectionBackgroundColor`) vs src/renderer/shadertoy.zig:40-41 (`selection_background_color` then `selection_foreground_color`); written at src/renderer/generic.zig:2275-2293 and uploaded as raw bytes by `state.uniforms.sync(&.{self.custom_shader_uniforms})` (:1892); documented at src/config/Config.zig:3163-3165, at 20abdb50a
- defect:          The `Globals` std140 block and the Zig `extern struct` are laid out member by member and uploaded byte for byte, so each GLSL name receives whatever Zig field sits at its offset. The last two members are declared in opposite orders: `iSelectionForegroundColor` occupies the slot Zig fills with `selection_background_color`, and `iSelectionBackgroundColor` occupies `selection_foreground_color`. A shader reading `iSelectionForegroundColor` gets the `selection-background` colour, and vice versa. The Metal path compiles the same GLSL prefix to MSL (shadertoy.zig:139,150) and the uniforms are written by the shared generic renderer, so both renderers are affected. Introduced with the uniforms in `8d2eb280d` ("custom shaders: add colorscheme information to shader uniforms"). By reading; not rendered.
- ReleaseSafe:     no crash; the two selection colours are exchanged in custom shaders. Visible whenever either option is set: a set colour appears under the other name, and the name the user expects reads the zero default (both slots start `@splat(0)`, generic.zig:776-777).
- ReleaseFast:     same.
- reachability:    config: `custom-shader` plus `selection-foreground` and/or `selection-background`, with a shader that reads either uniform.
- port handling:   faithful, byte-identical on both sides: `crates/rustty-renderer/src/shaders/shadertoy_prefix.glsl:29-30` and `crates/rustty-renderer/src/shadertoy.rs:158-159`, with a PORT NOTE at the Rust fields (added in task R13-R).
- parity row(s):   none; inventory M44.
- test:            n/a (record only).
- proposed fix:    Swap lines 29 and 30 of `shadertoy_prefix.glsl` so the GLSL order matches `shadertoy.zig` and the documentation order. User shaders bind by name, so no user shader changes. A test pairing each GLSL `Globals` member with the Zig field at the same offset would stop a recurrence.
- upstream status: not reported

### UD21 — sentry `cacheDir` on macOS inverts the `XDG_CACHE_HOME` check its comment describes
- kind:            latent (functional)
- upstream project: ghostty
- severity:        low
- reference:       src/crash/sentry.zig:188-207 (`cacheDir`; comment :189-191, condition :192-199), at 20abdb50a; introduced by `15c50c1db` ("crash: do not use global state"), an ancestor of the pin
- defect:          The comment says: prefer `NSCachesDirectory` on macOS, "But if XDG env vars are set we will respect them." The code does the opposite. If `XDG_CACHE_HOME` is unset it `break :macos`es to `internal_os.xdg.cache(… "ghostty/sentry")`; if it is set and non-empty it returns `internal_os.macos.cacheDir(alloc, "sentry")`. Before `15c50c1db` the block read `if (global.environ().containsUnemptyConstant("XDG_CACHE_HOME")) break :macos;` followed by the `NSCachesDirectory` return, which matched the comment (confirmed in `git show 15c50c1db`).
- ReleaseSafe:     no crash. On macOS the Sentry database (`sentry_options_set_database_path_n`, :164-168) lands in the XDG cache for users who did not set `XDG_CACHE_HOME`, and in `~/Library/Caches` for users who did. Crash reports are written by the custom transport to `crash.defaultDir` (`xdg.state … "ghostty/crash"`, sentry.zig:122, crash/dir.zig:7-10), so only the database location changes.
- ReleaseFast:     same.
- reachability:    environment (macOS builds with Sentry enabled).
- port handling:   diverges (escalation ruling P4-E35): `crates/rustty-crash/src/sentry.rs:450-484` implements the comment-consistent, pre-`15c50c1db` behaviour, with an UPSTREAM BUG note quoting the reference.
- parity row(s):   none in LEDGER; phase-4 P4-U1 / assessment P4-E35; inventory M65.
- test:            n/a (record only).
- proposed fix:    Restore the intended condition: `const xdg = environ_map.get("XDG_CACHE_HOME"); if (xdg == null or xdg.?.len == 0) return try internal_os.macos.cacheDir(alloc, "sentry");`, then fall through to `xdg.cache`.
- upstream status: not reported

### UD22 — `+list-colors --plain` (and piped) output is truncated: the buffered stdout writer is never flushed
- kind:            latent (functional: lost output)
- upstream project: ghostty
- severity:        low
- reference:       src/cli/list_colors.zig:53-74 (plain branch: `var buffer: [4096]u8`, `stdout.writer(global.io(), &buffer)`, prints in a loop, then `return 0` with no flush), at 20abdb50a
- defect:          The plain branch writes one line per X11 colour through a 4096-byte `std.Io.File.Writer` and never calls `flush()`. Bytes reach stdout only when the buffer fills, so whatever is still buffered at `return 0` (less than 4096 bytes) is discarded. Every other buffered CLI in `src/cli` flushes (`version.zig:78`, `list_keybinds.zig:85`, `list_fonts.zig:168`, `explain_config.zig:112`, `list_themes.zig:200`). The map comes from `src/terminal/res/rgb.txt` (782 lines) and each output line is at least 14 bytes, so the listing spans several buffers and its tail is lost. By reading; the exact number of missing lines is **UNVERIFIED** (not run).
- ReleaseSafe:     no crash; the end of the colour list is missing.
- ReleaseFast:     same.
- reachability:    CLI: `ghostty +list-colors --plain`, or any `+list-colors` whose stdout is not a tty.
- port handling:   diverges: `crates/rustty-cli/src/list_colors.rs:140-167` flushes a `BufWriter` (PORT NOTE at :145-148). Whether that divergence is right under R1 is the open question in S68-F3; R13 does not decide it.
- parity row(s):   S68-F3 (OPEN, an R1 question); inventory M66.
- test:            n/a (record only).
- proposed fix:    Add `try writer.flush();` after the loop.
- upstream status: not reported

### UD23 — `+list-themes` never flushes its buffered stderr, so its two error messages are never shown
- kind:            latent (functional: lost diagnostics)
- upstream project: ghostty
- severity:        low
- reference:       src/cli/list_themes.zig:106-202 (`run`): buffered stderr at :124-126, "Could not find the Ghostty resources directory…" at :128-130, "No themes found…" plus `return 1` at :173-176; only `stdout.flush()` at :200, at 20abdb50a
- defect:          `run` creates a 4096-byte buffered stderr writer and prints to it in two places, but no path flushes it. Both messages are far shorter than the buffer, so neither reaches the terminal. In particular a user with no installed themes sees `+list-themes` exit with status 1 and no output. (`std.debug.print` at :148, used for directory-open errors, is unbuffered and unaffected.) Minor companion: the `arena` created at :116 is never deinited, which is harmless at process exit.
- ReleaseSafe:     no crash; the diagnostics are silently dropped.
- ReleaseFast:     same.
- reachability:    CLI: a broken install (no resources directory, or no themes).
- port handling:   diverges: `crates/rustty-cli/src/list_themes.rs:291` and `:369-375` flush stderr. As with UD22, R13 does not decide the divergence.
- parity row(s):   S69-F4 (OPEN, an R1 question); inventory M67.
- test:            n/a (record only).
- proposed fix:    `defer stderr.flush() catch {};` right after creating the writer (or explicit flushes after each print and before `return 1`).
- upstream status: not reported

### UD24 — Metal `Texture` reports 128 bytes per pixel for 128-bit pixel formats
- kind:            latent
- upstream project: ghostty
- severity:        low
- reference:       src/renderer/metal/Texture.zig:120-202 (`bppOf`), `.rgba32uint, .rgba32sint, .rgba32float => 128` at :196-200; consumers `init` :73,78 and `replaceRegion` :114 (`bytesPerRow = bpp * width`), at 20abdb50a
- defect:          `bppOf` returns bytes per pixel (8-bit formats → 1, 16 → 2, 32 → 4, 64 → 8), but the "128-bit pixel formats" arm returns 128 instead of 16. For those formats `Texture.init` would assert `data.len == width * height * 128` on correctly sized data, and `replaceRegion` would pass a `bytesPerRow` eight times too large, making Metal read past the supplied buffer.
- ReleaseSafe:     none today. If a 128-bit format were used: an assertion failure in `init`, or an out-of-bounds read by Metal in `replaceRegion`.
- ReleaseFast:     same, with the assertion compiled out.
- reachability:    none known: Ghostty creates Metal textures only with `r8unorm[_srgb]`, `rgba8unorm[_srgb]` and `bgra8unorm[_srgb]` (`textureOptions` Metal.zig:292-295, `imageTextureOptions` :339-341, `initAtlasTexture` :372-374), plus `bgra8unorm[_srgb]` render targets (:242-245) backed by `32BGRA` IOSurfaces (metal/Target.zig:57).
- port handling:   faithful ("Reproduced verbatim, NOT corrected"): `crates/rustty-renderer/src/metal/texture.rs:230-234`.
- parity row(s):   none; inventory M71.
- test:            n/a (record only).
- proposed fix:    Change `=> 128` to `=> 16` at :200.
- upstream status: not reported

### UD25 — OpenGL `Buffer.len` is an element count after `init` but a byte count after `initFill`
- kind:            latent
- upstream project: ghostty
- severity:        low
- reference:       src/renderer/opengl/buffer.zig:28-30 (doc: "the number of `T`s, not the size in bytes"), `init` `.len = len` at :46, `initFill` `.len = data.len * @sizeOf(T)` at :62; `sync` :76-91 and `syncFromArrayLists` :96-124 treat `len` as a count; the Metal twin src/renderer/metal/buffer.zig:62 stores `.len = data.len`, at 20abdb50a
- defect:          `initFill` overstates `len` by a factor of `@sizeOf(T)`. A later `sync` with `count < data.len <= count * @sizeOf(T)` elements would skip the reallocation (`data.len > self.len` is false) and call `glBufferSubData` past the end of the buffer's storage, which GL rejects with `GL_INVALID_VALUE`, surfaced by `setSubData`'s `getError` as `error.InvalidValue`; the contents stay stale.
- ReleaseSafe:     none today; if reached, a failed upload returned as `error.InvalidValue`.
- ReleaseFast:     same.
- reachability:    none known: the only renderer `initFill` caller (src/renderer/image.zig:139-167) builds a one-element vertex buffer, draws it and deinits it without syncing.
- port handling:   faithful ("FAITHFULNESS QUIRK (R1)"): `crates/rustty-renderer/src/opengl/buffer.rs:82` and `:178-180`.
- parity row(s):   none; inventory M72.
- test:            n/a (record only).
- proposed fix:    Set `.len = data.len` in `initFill`, matching the field doc and the Metal implementation.
- upstream status: not reported

### UD26 — GTK `postFork` reads `linux_cgroup_hard_fail` from the pre-exec info in one arm
- kind:            latent (unobservable)
- upstream project: ghostty
- severity:        low
- reference:       src/apprt/gtk/post_fork.zig:91-99 (`if (cmd.rt_pre_exec_info.linux_cgroup_hard_fail)` at :93); every other read in `postFork` uses `cmd.rt_post_fork_info` (:40-50, :66, :81); both infos are built from the same config at src/Surface.zig:669-670, at 20abdb50a
- defect:          In the scope-transition timeout arm, `postFork` consults `rt_pre_exec_info` instead of `rt_post_fork_info`. Both structs copy `cfg.@"linux-cgroup-hard-fail"` (pre_exec.zig:21, post_fork.zig:27) from the same `Config` at the same time, so the values are always equal today. It is a copy-paste slip that would become a real bug if the two infos were ever derived differently.
- ReleaseSafe:     none.
- ReleaseFast:     none.
- reachability:    none.
- port handling:   collapsed to a single `info` with a PORT NOTE: `crates/rustty-apprt/src/gtk/post_fork.rs:190-198`.
- parity row(s):   none; inventory M73.
- test:            n/a.
- proposed fix:    Read `cmd.rt_post_fork_info.linux_cgroup_hard_fail` at :93.
- upstream status: not reported

### UD27 — inspector key mapping: keypad Page Down sends Page Up, and keypad Begin sends Tab
- kind:            latent (functional, inspector)
- upstream project: ghostty
- severity:        low
- reference:       src/input/key.zig:543 (`imguiKey`), `.numpad_page_down => cimgui.c.ImGuiKey_PageUp` at :654, `.numpad_begin => cimgui.c.ImGuiKey_NamedKey_BEGIN` at :659, orphaned comment at :647-648; consumers src/apprt/gtk/class/imgui_widget.zig:180, src/apprt/embedded.zig:1447; GTK producers src/apprt/gtk/key.zig:634,639 (`KEY_KP_Page_Down`, `KEY_KP_Begin`); pinned dear imgui (`pkg/dcimgui/build.zig.zon:16-20`) `imgui.h:1613-1615`, at 20abdb50a
- defect:          Three mistakes in the numpad block of `imguiKey`:
  1. `.numpad_page_down` maps to `ImGuiKey_PageUp` (the line above maps `.numpad_page_up` to the same value), so keypad Page Down in the inspector acts as Page Up.
  2. `.numpad_begin` maps to `ImGuiKey_NamedKey_BEGIN`, which is the start-of-range sentinel, not a key. In the pinned imgui, `ImGuiKey_NamedKey_BEGIN = 512` and `ImGuiKey_Tab = 512` ("== ImGuiKey_NamedKey_BEGIN"), so keypad Begin (KP5 with NumLock off) reaches imgui as Tab and moves keyboard focus.
  3. The comment at :647-648 ("We map KP_SEPARATOR to Comma…") has no arm under it; `.numpad_separator` is in the unmapped list at :690.
  By reading; not exercised in the inspector.
- ReleaseSafe:     no crash; wrong key events in the inspector UI.
- ReleaseFast:     same.
- reachability:    keyboard in the inspector: on GTK, keypad navigation keys with NumLock off produce these keyvals. macOS reachability not checked.
- port handling:   faithful at both copies: `crates/rustty-vt/src/input/key.rs:970,975` and `crates/rustty-apprt/src/embedded.rs:3159,3164`.
- parity row(s):   none; inventory M74.
- test:            n/a (record only).
- proposed fix:    Map `.numpad_page_down => ImGuiKey_PageDown`. Map `.numpad_begin` to `null` (imgui has no Begin key) or to `ImGuiKey_Keypad5`. Either restore `.numpad_separator => ImGuiKey_Comma` under the comment or delete the comment. A test that `imguiKey` is injective over mapped keys, except intentional aliases such as `numpad_left`/`arrow_left`, would catch (1).
- upstream status: not reported

### UD28 — terminal `hash_map.removeByPtrContext` assumes a zero-sized key lives at index 0
- kind:            latent (dead in product)
- upstream project: ghostty
- severity:        low
- reference:       src/terminal/hash_map.zig:901-912 (`removeByPtrContext`; `else 0` at :909, comment :902-905), `removeByIndexContext` :841-873, `AutoContext.hash` :72-116; test `HashMap removeByPtr 0 sized key` at :1744-1766 (uses `AutoHashMapUnmanaged(i32, u64, …)`), at 20abdb50a
- defect:          For `@sizeOf(K) == 0` the index is hard-coded to 0 on the theory that the single item "must be at index 0". This map uses linear probing with backward-shift deletion, so the single entry lives at its home slot `hash(key) & (capacity - 1)`. `removeByIndexContext(0)` then clears slot 0, decrements `size`, and leaves the real entry's metadata in place, so the count and the table disagree from then on. The test named for this case uses an `i32` key, so the zero-sized branch is never analysed.
  - The home slot is not 0 (by reading Zig 0.16 std): `std.meta.hasUniqueRepresentation(void)` is false (`meta.zig:961-963`, `else => false`), so `AutoContext(void).hash` takes the `Wyhash.init(0)` + `std.hash.autoHash` path, and `autoHash` returns without hashing anything for `.void` (`hash/auto_hash.zig:89`). The result is Wyhash of empty input, `0x409638ee2bde459` (std's own expected value, `hash/wyhash.zig:210`), which is slot 9 at capacity 16. (The port had computed the same value, `crates/rustty-vt/src/hash_map.rs:1131`.)
- ReleaseSafe:     none today. With a zero-sized key: `count()` reports 0 while the entry is still present, and a later remove of that entry underflows `size` (a safety panic).
- ReleaseFast:     same, with a silent wrap instead of the panic.
- reachability:    none in the product: the terminal map's users key on non-zero-sized types (the grapheme and hyperlink maps in `page.zig`).
- port handling:   diverges (scans for the one used slot): `crates/rustty-vt/src/hash_map.rs:1119-1150`, PORT NOTE W24-B03-ZST-01.
- parity row(s):   S155-F3 (WAIVED, unchanged); inventory M76.
- test:            n/a (record only).
- proposed fix:    For zero-sized `K`, find the entry's index by scanning `metadata` for the one used slot (or probe from `hash(key) & mask`), and change the "0 sized key" test to use `void` as the key type so it exercises the branch it is named for.
- upstream status: not reported

### UD29 — GTK `winproto/noop.zig` `updateConfigEvent` names a type that does not exist
- kind:            latent (compile)
- upstream project: ghostty
- severity:        low
- reference:       src/apprt/gtk/winproto/noop.zig:64-67 (`pub fn updateConfigEvent(_: *Window, _: *const ApprtWindow.DerivedConfig) !void {}`), `ApprtWindow` imported at :8 from `../class/window.zig`, at 20abdb50a
- defect:          `class/window.zig` declares no `DerivedConfig`: a search of `src/apprt/gtk` finds only this line, a comment in `class/config.zig:18`, and an unrelated `font.SharedGridSet.DerivedConfig` in `class/surface.zig:2206`. Nothing calls it: the `winproto.zig` dispatcher's `Window` has no `updateConfigEvent`, and neither `wayland.zig` nor `x11.zig` defines one. Zig never analyses the declaration, so the build passes. It is a leftover from the pre-GObject apprt.
- ReleaseSafe:     none; a compile error on first reference.
- ReleaseFast:     same.
- reachability:    none.
- port handling:   not ported (the parameter type does not exist): `crates/rustty-apprt/src/gtk/winproto/noop.rs:117-123` explains, citing N32.
- parity row(s):   NEW-FINDINGS N32; S60-F4, S60-F9 (unchanged); inventory M52.
- test:            n/a.
- proposed fix:    Delete `updateConfigEvent` from `winproto/noop.zig`.
- upstream status: not reported

### UD30 — renderer `cursorTimerCallback` logs "render callback fired without data set"
- kind:            latent (copy-paste in a log message)
- upstream project: ghostty
- severity:        low
- reference:       src/renderer/Thread.zig:654-683 (`cursorTimerCallback`), message at :672; the original in `renderCallback` at :563; the correctly renamed sibling "animation callback fired without data set" at :625, at 20abdb50a
- defect:          The null-userdata warning in the cursor blink timer callback was copied from `renderCallback` without renaming, so a missing-userdata event in the cursor timer is logged as a render-callback problem. The animation callback shows the intended pattern.
- ReleaseSafe:     misleading log only.
- ReleaseFast:     same.
- reachability:    none in practice (the userdata is always set when the timer is armed).
- port handling:   faithful: `crates/rustty-core/src/renderer_seam.rs:1924` (reference quoted at :1915).
- parity row(s):   none; inventory M53.
- test:            n/a.
- proposed fix:    Change the message at :672 to "cursor timer callback fired without data set".
- upstream status: not reported

### UD31 — copy-pasted log messages: `ghostty_surface_split_focus` and GTK `setWindowTitle`
- kind:            latent (copy-paste in log messages)
- upstream project: ghostty
- severity:        low
- reference:       at 20abdb50a:
  - src/apprt/embedded.zig:2182-2195 (`ghostty_surface_split_focus`, performing `.goto_split`) logs "error creating new split err={}" at :2192, copied from `ghostty_surface_split` (:2177).
  - src/apprt/gtk/class/application.zig:3111-3133 (`setWindowTitle`, dispatched for `.set_window_title` at :773) logs "set_tab_title to app is unexpected" at :3117, copied from `setTabTitle` (:3093).
- defect:          Both messages name the wrong operation, so a `goto_split` failure is reported as a split-creation failure, and an app-targeted `set_window_title` as `set_tab_title`.
  - Two further sub-sites in the source records are **refuted at the pin**: "mouse over link does nothing with an app target" and "set mouse shapes nothing" are not in `src/` (removed upstream in the embedded action rework). The `ghostty_app_set_color_scheme` message (inventory M103) is refuted too: the pin logs "error setting color scheme" (embedded.zig:1791).
- ReleaseSafe:     misleading logs only.
- ReleaseFast:     same.
- reachability:    embedder (a split focus failure) and GTK (an app-targeted `set_window_title`).
- port handling:   faithful: `crates/rustty-apprt-c/src/lib.rs:2201-2211` (PORT NOTE naming the copy-paste) and `crates/rustty-apprt/src/gtk/class/application.rs:4682-4684`.
- parity row(s):   none; inventory M54.
- test:            n/a.
- proposed fix:    embedded.zig:2192 → "error focusing split err={}"; application.zig:3117 → "set_window_title to app is unexpected".
- upstream status: not reported

### UD32 — OpenGL `present` does not check `BlitFramebuffer`; its error is reported as a failure to re-enable sRGB
- kind:            latent
- upstream project: ghostty
- severity:        low
- reference:       src/renderer/OpenGL.zig:299-331 (`present`): `defer gl.enable(GL_FRAMEBUFFER_SRGB) catch |err| log.err("Error re-enabling GL_FRAMEBUFFER_SRGB, err={}", …)` at :307-309, unchecked `BlitFramebuffer` at :316-327, `self.last_target = target` at :330; pkg/opengl/draw.zig:56-59 (`enable` calls `errors.getError()` after `glEnable`); pkg/opengl/Framebuffer.zig:66-71 (`unbind` does not check); pkg/opengl/errors.zig:16-26 (`getError` reads one `glGetError`), at 20abdb50a
- defect:          `glBlitFramebuffer` records its error (for example `GL_INVALID_OPERATION` for mismatched formats, or `GL_INVALID_FRAMEBUFFER_OPERATION` for an incomplete read framebuffer) and nothing checks it. `present` returns success and records the target as presented. On scope exit the framebuffer unbind runs unchecked, then the deferred `gl.enable` calls `glGetError`, receives the blit's pending error, and logs "Error re-enabling GL_FRAMEBUFFER_SRGB" although `glEnable` itself succeeded. By reading.
- ReleaseSafe:     no crash; a failed present is invisible to the caller and its log line blames the wrong call.
- ReleaseFast:     same.
- reachability:    GL driver or framebuffer state errors.
- port handling:   faithful: `crates/rustty-renderer/src/opengl/opengl_impl.rs:476-485` and `:577-588` (the restored log is the only report).
- parity row(s):   none; inventory M55.
- test:            n/a.
- proposed fix:    Call `try gl.errors.getError();` immediately after `BlitFramebuffer`, so `present` fails and `last_target` is not updated on failure; that also stops the error leaking into the deferred `enable`.
- upstream status: not reported

### UD33 — dead diagnostic helpers whose only callers are commented out
- kind:            latent (dead code)
- upstream project: ghostty
- severity:        low
- reference:       at 20abdb50a:
  - src/font/shaper/coretext.zig:690 `fn debugPositions` (only call commented out at :503)
  - src/font/shaper/harfbuzz.zig:302 `fn debugPositions` (commented call at :236)
  - pkg/macos/text/font_collection.zig:41 `fn debugDumpList` (commented calls at :114, :138)
  - src/terminal/bitmap_allocator.zig:212 `fn dumpBitmaps` (no caller)
  - src/inspector/widgets/termio.zig:435 `pub fn eventTable` (commented call at :120; no other caller in `src/`; `inspector/main.zig`'s test uses non-recursive `refAllDecls`)
- defect:          These functions are never referenced, so Zig never analyses them. They are not type-checked against current APIs and can bit-rot unnoticed. The source records also mention "eight gate log sites"; those were not located, are **UNVERIFIED**, and are not included.
- ReleaseSafe:     none.
- ReleaseFast:     none.
- reachability:    none.
- port handling:   faithful, tracked by the N22 dead-site ratchet (`.port/parity-closure/LOG-DEAD-SITES.tsv`).
- parity row(s):   NEW-FINDINGS N22; S109-F1, S149-F3 (WAIVED, unchanged); inventory M56.
- test:            n/a.
- proposed fix:    Either delete them, or keep them type-checked by referencing them from a test or behind a comptime debug flag. Upstream may prefer to keep them as debugging aids, so the issue should present both options.
- upstream status: not reported

### UD34 — `highlight.Flattened.init` cannot compile (wrong field name and wrong chunk type)
- kind:            latent (compile)
- upstream project: ghostty
- severity:        low
- reference:       src/terminal/highlight.zig:141-160 (`Flattened.init`); `const PageChunk = PageList.PageIterator.Chunk` at :16; `Flattened` fields at :116-122 (`chunks: std.MultiArrayList(Chunk)`, `top_x`, `bot_x`) and `Flattened.Chunk` at :128-133; `PageIterator.Chunk` at src/terminal/PageList.zig:6617-6624, at 20abdb50a
- defect:          Three compile errors, all in a function nothing calls (no `Flattened.init` or decl-literal `.init` caller in `src/`; `Flattened` values are built from `.empty` elsewhere, e.g. `search/sliding_window.zig:327`):
  1. `.end_x = end.x` at :158: `Flattened` has `bot_x`, not `end_x`.
  2. `result` is a `std.MultiArrayList(PageChunk)` (:146), and `PageIterator.Chunk` has only `node`, `start` and `end`, so appending `.serial = chunk.node.serial` (:151) names a nonexistent field.
  3. `.chunks = result` (:156) assigns a `MultiArrayList(PageChunk)` to a `MultiArrayList(Flattened.Chunk)` field.
  By reading; not compiled.
- ReleaseSafe:     none; a compile error on first use.
- ReleaseFast:     same.
- reachability:    none.
- port handling:   ported with the evident intent (`bot_x`, chunks carrying `serial`): `crates/rustty-vt/src/highlight.rs:168-199`, PORT NOTE at :193-197.
- parity row(s):   S155-F6 (OOM only, unchanged); inventory M57.
- test:            n/a.
- proposed fix:    Declare `result` as `std.MultiArrayList(Chunk)` and write `.bot_x = end.x`; add a small test that flattens a two-page highlight so the function is analysed. Or delete `init` if it is not meant to be used.
- upstream status: not reported

### UD35 — IPC `NewTab.C.deinit` and `NewTab.cval` are copies of `NewWindow`'s, and neither struct's `cval`/`deinit` compiles
- kind:            latent (compile)
- upstream project: ghostty
- severity:        low
- reference:       src/apprt/ipc.zig:124-171 (`NewTab`): `pub fn deinit(self: *NewWindow.C, …)` at :151 and `pub fn cval(self: *NewWindow, …) … !NewWindow.C` at :156-170; the `NewWindow` originals at :100-102 and :105-119; the generic dispatcher `Action.cval` at :238-251, at 20abdb50a
- defect:          `NewTab.C.deinit` takes `*NewWindow.C`, and `NewTab.cval` takes `*NewWindow` and returns `NewWindow.C`, so neither can be called on a `NewTab`, and `cval` never sets `surface_id`, the one field `NewTab.C` adds. The shared body is also ill-typed in both structs. **Measured** by compiling a verbatim copy of `NewWindow` with Zig 0.16.0 and a test that calls `cval` and `C.deinit`:
  - `cval` fails with "expected type '?[*]?[*:0]const u8', found '[][*:0]const u8'": it assigns `alloc.alloc([*:0]const u8, n)` (a slice) to the many-pointer field (and would then index the optional).
  - `C.deinit` fails inside `Allocator.free`'s `comptime assert(slice_info.size == .slice)`, because `arguments` is a many-pointer.
  Nothing reaches these: the only caller of a per-action `cval` is `Action.cval` (:243), and nothing in `src/` calls `Action.cval`. By reading, `Action.cval` would not compile either (it passes the error union `cval` returns straight into a union field, and calls a `*NewWindow` method on a by-value capture).
- ReleaseSafe:     none; compile errors on first use.
- ReleaseFast:     same.
- reachability:    none.
- port handling:   corrected with UPSTREAM QUIRK notes: `crates/rustty-apprt-api/src/ipc.rs:224-232` (`NewTabC::deinit`) and `:250-256` (`NewTab::cval`, passes `surface_id` through).
- parity row(s):   none in LEDGER; phase-4 P4-U2; inventory M58.
- test:            n/a. Probe: a verbatim copy of `NewWindow` under `zig test` (Zig 0.16.0), task R13-R.
- proposed fix:    Change the `NewTab` receivers to `*NewTab.C` / `*NewTab` and the return type to `NewTab.C`, and set `result.surface_id = self.surface_id`. In both structs, marshal through a slice and store its pointer (`const args = try alloc.alloc(?[*:0]const u8, n + 1); … result.arguments = args.ptr;`), and free with the reconstructed slice length. Make `Action.cval` `try` the payload's `cval` and return an error union. Add a test that calls `Action.cval` for both actions. Or remove the unused `cval`s.
- upstream status: not reported

### UD36 — `termio.Backend.childExitedAbnormally` calls a function `Exec` does not define
- kind:            latent (compile)
- upstream project: ghostty
- severity:        low
- reference:       src/termio/backend.zig:88-103 (`Backend.childExitedAbnormally`, calling `exec.childExitedAbnormally(gpa, t, exit_code, runtime_ms)` at :96), at 20abdb50a
- defect:          `src/termio/Exec.zig` defines no `childExitedAbnormally` (a search of `src` finds only this wrapper and `Surface.zig`'s own private function). Nothing calls `Backend.childExitedAbnormally`: the abnormal-exit message is built in `Surface.childExitedAbnormally` (src/Surface.zig:1277, :1330). Zig never analyses the wrapper, so the build passes.
- ReleaseSafe:     none; a compile error on first use.
- ReleaseFast:     same.
- reachability:    none.
- port handling:   a tripwire `unimplemented!` with an explanation: `crates/rustty-termio/src/backend.rs:172-185`.
- parity row(s):   none; inventory M59.
- test:            n/a.
- proposed fix:    Delete `Backend.childExitedAbnormally` from `backend.zig`.
- upstream status: not reported

### UD37 — wasm `web_canvas` shaper refers to APIs that no longer exist
- kind:            latent (compile)
- upstream project: ghostty
- severity:        low
- reference:       src/font/shaper/web_canvas.zig at 20abdb50a:
  - :188 `run.group.group.faceFromIndex(run.font_index)`
  - :264 and :273 `group: *font.GroupCache`
  - :275 `terminal.Terminal.init(alloc, self.cell_buf.len, 80)`
  - :288 `term.screen.rowIterator(.viewport)`
  - :293 `self.runIterator(group, row, null, null)`
- defect:          `font.shape.TextRun` has `grid: *font.SharedGrid` and no `group` field (src/font/shaper/run.zig:13-38), and no `faceFromIndex` exists in `src/`. `font.GroupCache` is not exported by `src/font/main.zig` (stale `const GroupCache = font.GroupCache;` aliases also remain, unused, in `shaper/coretext.zig:17` and `shaper/noop.zig:8`). `Terminal` has `screens`, not `screen` (Terminal.zig:48), and `Terminal.init` is now `init(io, alloc, opts)` (:311-315). The shaper's own `runIterator` takes one `RunOptions` argument (:63-71). The file is selected only for wasm targets (font/main.zig:34-40, :50), and no build step compiles Ghostty's wasm font tree: `src/main_wasm.zig`, the root that would import it, is referenced by no build file, and the only wasm artifact is libghostty-vt (`src/build/GhosttyLibVt.zig`), whose root does not import `font/main.zig`. So none of this is analysed.
- ReleaseSafe:     none; compile errors if the wasm font tree is built.
- ReleaseFast:     same.
- reachability:    none.
- port handling:   ported by evident intent ("FAITHFUL MODERNIZATION": `faceFromIndex` → `Collection::get_face`): `crates/rustty-font-shaper/src/shaper/web_canvas.rs:7-16` (module note) and `:250-252`; the `shaper_test` rework at `:346` and `:432`.
- parity row(s):   none; inventory M60.
- test:            n/a.
- proposed fix:    Either update the wasm shaper to the `SharedGrid` API (`run.grid.resolver.collection.getFace(run.font_index)`, `Terminal.init(io, alloc, .{ .cols = …, .rows = … })`, `screens.active`, `runIterator(.{ … })`) and add the wasm font tree to a CI build, or delete `shaper/web_canvas.zig` together with `face/web_canvas.zig` (UD38).
- upstream status: not reported

### UD38 — web_canvas `renderGlyph` switches on an `Atlas.Format.rgba` tag that no longer exists
- kind:            latent (compile)
- upstream project: ghostty
- severity:        low
- reference:       src/font/face/web_canvas.zig:199-217 (`renderGlyph`, `.rgba => render.bitmap` at :202), reached from the wasm export `face_render_glyph` (:521-530, via `face_render_glyph_` :553); src/font/Atlas.zig:53-67 `Format = enum(u8) { grayscale = 0, bgr = 1, bgra = 2 }`, at 20abdb50a
- defect:          Commit `41ae32814` ("renderer: fix color glyph rendering under OpenGL", an ancestor of the pin) renamed `Atlas.Format.rgba` to `bgra` and `rgb` to `bgr`, but left this switch arm on `.rgba`. `Format` is an exhaustive enum, so the arm is a compile error whenever `renderGlyph` is analysed. It has not been compiled since the rename, because nothing builds the wasm font tree (see UD37).
  A rename alone would not fix it. `renderGlyphInternal` copies `ctx.getImageData(...).data` (:444-446), which is RGBA byte order; storing it verbatim in a `.bgra` atlas swaps the red and blue channels of every colour glyph.
- ReleaseSafe:     no runtime effect; a compile error if the wasm font tree is built.
- ReleaseFast:     same.
- reachability:    none (no build step compiles it).
- port handling:   deviates, because Rust compiles every item: `Format::Bgra => rgba_to_bgra(&render.bitmap)`, with the reference arm kept as a comment: `crates/rustty-font-shaper/src/face/web_canvas.rs:472-478` (module note :34-42, helper :818-826).
- parity row(s):   none (S91 shard, no LEDGER row); inventory M61.
- test:            n/a (record only).
- proposed fix:    Change the arm to `.bgra` and convert the canvas RGBA bytes to BGRA before `atlas.set` (swap bytes 0 and 2 of each pixel). Also add the wasm font tree to a CI `zig build` check, or delete it (UD37).
- upstream status: not reported

### UD39 — GTK apprt `Surface.cgroup` calls `cgroupPath`, which was removed from the GObject surface
- kind:            latent (compile)
- upstream project: ghostty
- severity:        low
- reference:       src/apprt/gtk/Surface.zig:40-42 (`pub fn cgroup(self: *Self) ?[]const u8 { return self.surface.cgroupPath(); }`), at 20abdb50a
- defect:          `self.surface` is the `class/surface.zig` `Surface`, which has no `cgroupPath`: a search of `src` and `pkg` finds only this call. Commit `1342eb594` ("gtk: revamp cgroup/transient scope handling", an ancestor of the pin) deleted `cgroupPath` from `class/surface.zig` but left this wrapper. Nothing calls `.cgroup(` on an apprt surface, so Zig never analyses the body.
- ReleaseSafe:     none; a compile error if anything calls it.
- ReleaseFast:     same.
- reachability:    none.
- port handling:   a placeholder returning `None`: `crates/rustty-apprt/src/gtk/class/surface.rs:2215-2222` (`cgroup_path`, "Placeholder-leaf").
- parity row(s):   none; inventory M62.
- test:            n/a.
- proposed fix:    Delete `Surface.cgroup` from `src/apprt/gtk/Surface.zig`. Since `1342eb594` the cgroup scope is per command (`post_fork.zig`), so there is no per-surface cgroup to return.
- upstream status: not reported

### UD40 — `pkg/opengl` `Buffer.Binding.attribute` and `draw.pixelStore` use removed `@typeInfo` tags and a method `Buffer.Binding` lacks
- kind:            latent (compile)
- upstream project: ghostty (pkg/opengl)
- severity:        low
- reference:       pkg/opengl/Buffer.zig:142-179 (`Binding.attribute`: `.Array => |ary|` at :159, `try b.enableAttribArray(idx)` at :178); pkg/opengl/draw.zig:80-86 (`pixelStore`: `.ComptimeInt, .Int =>` at :82), at 20abdb50a
- defect:          The pin requires Zig 0.16.0 (`build.zig.zon:6`), whose `std.builtin.Type` tags are lowercase (`.array`, `.int`, `.comptime_int`); `.Array`, `.Int` and `.ComptimeInt` are pre-0.14 spellings. `attribute` also calls `b.enableAttribArray`, which `Buffer.Binding` does not declare; only `VertexArray.Binding` has one (pkg/opengl/VertexArray.zig:33). Neither function has a caller in Zig code (only the definitions and the `main.zig:42` re-export of `pixelStore`). Both take `comptime`/`anytype` parameters, so their bodies are checked only when instantiated, which never happens.
- ReleaseSafe:     none; a compile error on first use.
- ReleaseFast:     same.
- reachability:    none.
- port handling:   `Binding.attribute` is deliberately not ported (PORT NOTE `crates/rustty-renderer/src/opengl/gl/buffer.rs:13-26`); its helpers `attribute_divisor`, `attribute_advanced` and `attribute_i_advanced` are ported.
- parity row(s):   none (S37-F3 is a session id with no LEDGER row); inventory M63.
- test:            n/a.
- proposed fix:    Delete `Buffer.Binding.attribute` and `draw.pixelStore`, which are unused. If kept, update the tags to `.array`/`.int`/`.comptime_int` and enable the attribute through the bound vertex array (`VertexArray.Binding.enableAttribArray`).
- upstream status: not reported

### UD41 — `FeatureList.format` contains a pasted copy of `Feature.format`'s body, and the file's formatters use std APIs Zig 0.16 lacks
- kind:            latent (compile)
- upstream project: ghostty
- severity:        low
- reference:       src/font/shaper/feature.zig:258-284 (`FeatureList.format`), siblings `Feature.toString` :191-196 and `Feature.format` :198-219, at 20abdb50a
- defect:          `FeatureList.format` loops over `self.features.items` but tests `i != std.features.items.len - 1` (:267); `std.features` does not exist (it should be `self.features`). After the loop the whole `Feature.format` body is pasted in (:269-283), using `self.value` and `self.tag`, which are not fields of `FeatureList`.
  - All three functions also use std APIs Zig 0.16.0 does not have: `std.fmt.FormatOptions` (0.16 `std.fmt` defines `Options`, fmt.zig:31), `std.fmt.format`, `std.io` and `std.io.fixedBufferStream` (0.16 `std` exposes `Io`).
  - Their 4-parameter `format(self, comptime layout, opts, writer)` signature is the pre-0.15 convention; current Ghostty formatters are `format(self, writer: *std.Io.Writer)`.
  None of them is referenced: `toString` has no caller, and `FeatureList` is only constructed (shaper/harfbuzz.zig:65, shaper/coretext.zig:153), never formatted.
- ReleaseSafe:     none; a compile error the first time a `Feature` or `FeatureList` is formatted or `toString` is called.
- ReleaseFast:     same.
- reachability:    none.
- port handling:   the live loop is ported and the pasted tail kept as a comment with a PORT NOTE: `crates/rustty-font-shaper/src/shaper/feature.rs:453-489`; `Feature::to_string` ported live at `feature.rs:334-360`.
- parity row(s):   none; inventory M64.
- test:            n/a.
- proposed fix:    Rewrite the formatters in the 0.16 form `pub fn format(self: T, writer: *std.Io.Writer) !void`: `FeatureList.format` iterates `self.features.items`, compares against `self.features.items.len - 1`, and drops the pasted tail; `Feature.format` uses `writer.print`; `toString` writes through `std.Io.Writer.fixed(buf)`. Add a test that formats a `FeatureList` (for example `-liga, kern=2`) so the formatters are analysed.
- upstream status: not reported

### UD42 — typo in the `os` module doc comment ("butos/main.zig")
- kind:            latent (prose)
- upstream project: ghostty
- severity:        low
- reference:       src/os/main.zig:2, at 20abdb50a
- defect:          The module doc reads "These aren't restricted to syscalls or low-level operations, butos/main.zig"; the file's own path was pasted into the sentence, which continues on the next line "also OS-specific features and conventions."
- ReleaseSafe:     none.
- ReleaseFast:     none.
- reachability:    none (generated docs only).
- port handling:   the port's prose reads "…low-level operations, but / also OS-specific features and conventions." (`crates/rustty-os/src/lib.rs:17-19`).
- parity row(s):   none (phase-3 NON_PORTS E28); inventory M69.
- test:            n/a.
- proposed fix:    Replace `butos/main.zig` with `but` (bundle with UD43 as one typo PR).
- upstream status: not reported

### UD43 — typos: "Ghosttty" (two CLI docs), "fever than", and the GType name `GhostttySurfaceScrolledWindow`
- kind:            latent (prose, plus one type name)
- upstream project: ghostty
- severity:        low
- reference:       at 20abdb50a:
  - src/cli/new_tab.zig:28 and src/cli/new_window.zig:24 ("They will be sent to Ghosttty")
  - src/terminal/kitty/graphics_command.zig:16 ("KIP has fever than 52 possible")
  - src/apprt/gtk/class/surface_scrolled_window.zig:23 (`.name = "GhostttySurfaceScrolledWindow"`) and src/apprt/gtk/ui/1.5/surface-scrolled-window.blp:4 (`template $GhostttySurfaceScrolledWindow: Adw.Bin`)
- defect:          The first two are doc-comment typos ("Ghostty", "fewer"). The third is a registered GObject type name with a triple `t`; the other 28 `.name = "Ghostty…"` registrations in `src/apprt/gtk/class/` are spelled correctly. The misspelling is used consistently (the Zig class and the Blueprint template are its only two occurrences), so nothing is broken. The class sets no CSS name, so CSS selectors are unaffected. The name is visible in GtkInspector, in GLib warning output, and to anything that looks types up by name.
- ReleaseSafe:     none.
- ReleaseFast:     none.
- reachability:    none.
- port handling:   "fever than" faithful in `crates/rustty-vt/src/kitty/graphics_command.rs:33`; the GType faithful in `crates/rustty-apprt/src/gtk/class/surface_scrolled_window.rs:73-78` and `crates/rustty-apprt/assets/ui/1.5/surface-scrolled-window.blp:4`. The "Ghosttty" doc prose is a brand token, which the CLI ports retarget to the Rustty product name (R2): task R13-R made `crates/rustty-cli/src/new_tab.rs` and `new_window.rs` consistent ("Rustty" in both, with a note citing this entry); before that, `new_tab.rs` kept "Ghosttty" and `new_window.rs` did not.
- parity row(s):   none; inventory M70.
- test:            n/a.
- proposed fix:    Fix the two prose typos. Rename the GType to `GhosttySurfaceScrolledWindow` in `surface_scrolled_window.zig:23` and `surface-scrolled-window.blp:4` in the same commit (the two must change together or template binding fails at class init). The GType name is not public API, but out-of-tree tooling matching on type names would see the change.
- upstream status: not reported

### UD44 — CLI TUIs leave the terminal raw on a panic: no root panic handler calls `vaxis.recover`
- kind:            latent (functional)
- upstream project: ghostty (with a libvaxis sub-report)
- severity:        low
- reference:       at 20abdb50a:
  - src/main.zig:1-25 (the root re-exports `main` and `std_options` only). The only root `panic` declaration in `src` is `src/lib_vt.zig:500`, the libghostty-vt root.
  - vaxis users: src/cli/boo.zig (`vxfw`), src/cli/list_themes.zig (`tty: vaxis.Tty` at :243, `deinit` at :301), and the pretty-print path of src/cli/list_colors.zig (`vaxis.Tty.init` at :82, `defer tty.deinit()` at :83).
  - libvaxis 0.6.0 (pinned by `build.zig.zon:25-30`) `src/main.zig:50-79` (`Panic`, `panic_handler`, `recover`); `src/tty.zig:27,85,254` (`global_tty`, set in `Tty.init`).
- defect:          libvaxis ships the recovery hook: `panic_handler` calls `recover()`, which writes `csi_u_pop ++ mouse_reset ++ bp_reset ++ rmcup`, flushes, and runs `Tty.deinit` (restoring termios). The hook only takes effect if the root module declares `panic`, and Ghostty's does not. Zig panics do not unwind, so the TUIs' `defer tty.deinit()` never runs. A panic in a vaxis TUI therefore goes to the default handler and exits with the tty in raw mode, the alternate screen active, and mouse and bracketed-paste reporting still enabled.
  - libvaxis sub-report: the ready-made `vaxis.Panic` namespace cannot be used on Zig 0.16: it references `std.debug.FormattedPanic` (`main.zig:52-57`), which does not exist in the 0.16.0 std, although vaxis 0.6.0 declares `minimum_zig_version = "0.16.0"`.
- ReleaseSafe:     any safety panic inside a TUI leaves the user's shell with echo off, no line editing, and the alternate screen active until `reset`. Mechanism by reading; the post-panic terminal state was **not measured (UNVERIFIED)**.
- ReleaseFast:     same for an explicit `@panic`; safety checks become undefined behaviour instead.
- reachability:    any panic in the CLI TUIs (for example the `list_themes` sites recorded as inventory M25).
- port handling:   faithful: the rustty CLI installs no hook. `install_panic_recovery()` exists as an opt-in port (`crates/rustty-vaxis/src/lib.rs:128-146`, PORT NOTE W16-EXPLICIT-01) but no crate calls it. The CLI's release profile unwinds (the workspace sets `panic = "abort"` only for the xcframework profile, `Cargo.toml:426-433`), but the port's Tty has no `Drop` that restores termios.
- parity row(s):   none; inventory M75.
- test:            n/a.
- proposed fix:    In the Ghostty root (`src/main_ghostty.zig`, re-exported through `main.zig` the way `std_options` is), declare `pub const panic = std.debug.FullPanic(struct { fn f(msg: []const u8, ra: ?usize) noreturn { vaxis.recover(); std.debug.defaultPanic(msg, ra); } }.f);` (`FullPanic` and `defaultPanic` exist in the 0.16.0 std, debug.zig:97, :489). `recover` is a no-op while `vaxis.tty.global_tty` is null, so the GUI path is unaffected. Raise two points with libvaxis: `global_tty` is set in `Tty.init` and not cleared by `deinit` (a panic after a TUI exited would `tcsetattr`/`close` a stale fd), and `vaxis.Panic` should move to `std.debug.FullPanic` for Zig 0.16.
- upstream status: not reported

### UD45 — `pkg/libxml2` builds `LIBXML_VERSION` by string concatenation: 2.11.5 becomes `201105`, not `21105`
- kind:            latent (inert)
- upstream project: ghostty (pkg/libxml2 build)
- severity:        low
- reference:       pkg/libxml2/build.zig:37-41 (`-DLIBXML_VERSION=` and `-DLIBXML_VERSION_STRING=`) and :128-146 (`Version.number()` returns `major ++ "0" ++ minor ++ "0" ++ micro` with `minor = "11"`, `micro = "5"`), at 20abdb50a. Pinned libxml2 2.11.5 (`pkg/libxml2/build.zig.zon:3,8-9`).
- defect:          The concatenation matches libxml2's formula (`configure.ac:21`, `major * 10000 + minor * 100 + micro`) only when minor and micro each have one digit. For 2.11.5 it yields `"2" ++ "0" ++ "11" ++ "0" ++ "5" = 201105` instead of `21105`. The wrong value reaches `LIBXML_VERSION` (used by `xmlCheckVersion`, `parserInternals.c:64-82`) and `LIBXML_VERSION_STRING` (the exported `xmlParserVersion`, `globals.c:182,548`). `LIBXML_DOTTED_VERSION` is correct. A client calling `xmlCheckVersion(21105)` would get "Fatal: program compiled against libxml 2 using libxml 20" on stderr (no abort).
- ReleaseSafe:     no effect in Ghostty's link graph.
- ReleaseFast:     same.
- reachability:    none today: the only consumer, fontconfig 2.14.2, references neither `xmlCheckVersion`, `LIBXML_TEST_VERSION`, `LIBXML_VERSION` nor `xmlParserVersion`, and the override `pkg/libxml2/override/include/libxml/xmlversion.h` defines none of them.
- port handling:   faithful with a note: `crates-sys/libxml2-sys/build.rs:145` (`version_number()`) reproduces `"201105"` and argues it is inert (the note mentions only `xmlCheckVersion`, not `xmlParserVersion`; harmless for the same reason).
- parity row(s):   none (session sF10); inventory M84.
- test:            n/a.
- proposed fix:    Compute the number arithmetically at comptime, for example `std.fmt.comptimePrint("{d}", .{major * 10000 + minor * 100 + micro})` with integer components, and derive both strings from it; or parse `configure.ac` as the TODO above `Version` suggests.
- upstream status: not reported

### UD46 — libxev dynamic `test "prefer"` needs io_uring to be usable and cannot catch a broken `prefer`
- kind:            test-weakness
- upstream project: libxev
- severity:        low
- reference:       libxev (pinned by ghostty 20abdb50a `build.zig.zon:19-24`, tarball `libxev-9ce8e8e6ff89e583258a7f8e7adeeeaeae8611bf`) `src/dynamic.zig:218-227` (`test "prefer"`), `prefer` at :109-120, `test "detect"` at :207-216; candidates `src/backend.zig:31-46`; `io_uring.available()` `src/backend/io_uring.zig:18-33`
- defect:          `Xev` returns the static API when there is a single candidate (dynamic.zig:39), so these dynamic tests exist only where there are at least two candidates, which is non-Android Linux with `{ io_uring, epoll }` (backend.zig:36). Two problems:
  1. **It needs io_uring.** `try testing.expect(prefer(bes[0]))` requires io_uring to be usable. `available()` returns false whenever `linux.IoUring.init(256, 0)` fails (:29), and then the test fails instead of skipping. **UNVERIFIED:** not run on a host with io_uring disabled (for example `kernel.io_uring_disabled=2`, or a seccomp-filtered container).
  2. **It cannot catch a broken `prefer`.** It only checks that `prefer(bes[0])` returns true and that the selected backend is available. **Changes it fails to catch** (by reading): change `if (candidate == be)` at :111 to `if (true)` (`prefer` then selects the first candidate whatever `be` is, which here is still `bes[0]`); or return true without assigning `backend` (drop :114), which stays green because the preceding `detect` test already set `backend` to io_uring.
- ReleaseSafe:     no product effect; false failures on io_uring-less Linux hosts and missed regressions.
- ReleaseFast:     same.
- reachability:    none (test code).
- port handling:   faithful but strengthened: `crates/rustty-xev/src/dynamic.rs:4653-4675` (`dynamic_prefer`) keeps `assert!(prefer(CANDIDATES[0]))` (PORT NOTE :4658-4661) and adds `assert_eq!(backend(), CANDIDATES[0])` and `assert!(!prefer(Backend::Kqueue))`.
- parity row(s):   none; inventory M50.
- test:            n/a (record only).
- proposed fix:    Skip (`error.SkipZigTest`) when `bes[0].Api().available()` is false; after `prefer`, assert `backend == subset(bes[0])`; add `prefer(bes[1])` (epoll is always available on Linux) and assert that `backend` switched.
- upstream status: not reported

### UD47 — libxev io_uring `socket read cancellation` test submits its cancel twice, so it cannot detect a `Loop.cancel` that never queues
- kind:            test-weakness
- upstream project: libxev
- severity:        low
- reference:       libxev (pin as UD46) `src/backend/io_uring.zig:1731-1792` (test), `Loop.cancel` at :570-607 (`self.add(c_cancel)` at :606)
- defect:          `loop.cancel(&c_read, &c_read_cancel, …)` already queues the cancel completion (:606). The test then calls `loop.add(&c_read_cancel);` again (:1786), so two `IORING_OP_ASYNC_CANCEL` SQEs with the same `user_data` are submitted and the cancel callback runs twice.
  - **Change it fails to catch** (by reading): delete `self.add(c_cancel);` at :606, so `Loop.cancel` only fills in the completion. The test's own extra `add` still queues it, so the test stays green while every real `cancel` caller silently never cancels.
  - **Kernel dependence:** the second cancel's `r catch unreachable` passes only if the kernel reports success for the repeat. The rustty port measured `cqe.res == 0` for both on Linux 6.19.10 aarch64 (PORT NOTE cited below). **UNVERIFIED** on other kernels; one that returns `-ENOENT` for the second cancel would hit `unreachable` in the test.
- ReleaseSafe:     no product effect; the regression above passes, and on a `-ENOENT` kernel the test panics.
- ReleaseFast:     same (the test's `unreachable` would be undefined behaviour).
- reachability:    none (test code).
- port handling:   faithful: `crates/rustty-xev/src/io_uring.rs:7342-7424` keeps the second `add`, with a PORT NOTE at :7404-7416 recording the double submit and the kernel measurement.
- parity row(s):   none; inventory M51.
- test:            n/a (record only).
- proposed fix:    Remove the redundant `loop.add(&c_read_cancel);`; optionally count cancel-callback invocations and assert exactly one, which pins that `Loop.cancel` queues its completion.
- upstream status: not reported

### UD48 — libvaxis `transmitImage(.png)` sizes the PNG output buffer at the raw pixel size
- kind:            latent (functional)
- upstream project: libvaxis
- severity:        low
- reference:       vaxis 0.6.0 (pinned by ghostty 20abdb50a `build.zig.zon:25-30`) `src/Vaxis.zig:1045-1079` (`transmitImage`; `alloc(u8, img.imageByteSize())` at :1061, `writeToMemory` at :1062), reached from `loadImage` at :1081-1097; zigimg `d695acd9` (pinned by vaxis's `build.zig.zon:7-10`) `src/Image.zig:242-244` (`imageByteSize` is `rawBytes().len`) and :277-285 (`writeToMemory`: "The memory buffer is not grown for you so make sure you pass a large enough buffer"; a `std.Io.Writer.fixed`, `src/io.zig:104-108`)
- defect:          The PNG encoding is written into a fixed buffer exactly as large as the raw pixel data, but PNG adds overhead: the signature and IHDR/IDAT/IEND framing, a zlib header and trailer, and one filter byte per scanline (zigimg `src/formats/png.zig:268-280` deflates at `.level = .default`). Any image whose PNG does not compress below its raw size overflows the writer: tiny images (a 1×1 RGBA image has 4 raw bytes) and high-entropy images of any size. `transmitImage` then returns the writer's error instead of transmitting. **UNVERIFIED:** the threshold was not measured, and the exact error name (`error.WriteFailed` from a fixed writer) is inferred from std, not observed.
- ReleaseSafe:     no crash; `loadImage`/`transmitImage` fail for affected images.
- ReleaseFast:     same.
- reachability:    library API: any vaxis app calling `loadImage` or `transmitImage(.png)` with a small or noisy image. Ghostty's CLI TUIs call neither.
- port handling:   faithful: `crates/rustty-vaxis/src/vaxis.rs:1067` (`vec![0u8; img.image_byte_size()]`), with the resulting `NoSpaceLeft` documented as preserved upstream behaviour at `crates/rustty-zigimg/src/lib.rs:59-64` and `crates/rustty-zigimg/src/png_encode.rs:34-38`.
- parity row(s):   none; inventory M77.
- test:            n/a (record only). The port's codec test deliberately oversizes its buffer (`crates/rustty-zigimg/tests/codec.rs:39-43`).
- proposed fix:    Encode through an allocating writer (`std.Io.Writer.Allocating` on the arena) instead of a fixed buffer, or size the buffer with a deflate worst-case bound (raw bytes + one filter byte per row + deflate stored-block overhead + zlib and PNG framing).
- upstream status: not reported

### UD49 — libvaxis `WindowsTty.read` references a nonexistent `fd` field
- kind:            latent (compile)
- upstream project: libvaxis
- severity:        low
- reference:       vaxis 0.6.0 (pin as UD48) `src/tty.zig:310-312` (`pub fn read(self: *const WindowsTty, buf: []u8) !usize { return posix.read(self.fd, buf); }`), struct fields at :194-210
- defect:          `WindowsTty` declares `stdin`, `stdout`, `initial_codepage`, `initial_input_mode`, `initial_output_mode`, `buf`, `tty_writer` and `last_mouse_button_press`, but no `fd`, so `read` cannot compile. It builds only because Zig analyses declarations lazily: `Loop.ttyRun`'s `.windows` arm (`src/Loop.zig:153-159`) calls `nextEvent`, and only the POSIX arm calls `tty.read`. Any Windows caller of `Tty.read` gets a compile error.
- ReleaseSafe:     compile error if referenced; otherwise none.
- ReleaseFast:     same.
- reachability:    none (compile time, Windows only, only if a consumer calls `read`).
- port handling:   repaired (Rust compiles every arm): `crates/rustty-vaxis/src/tty.rs:490-496` (module PORT NOTE) and `:940-948` implement `read` as `ReadFile` on the stdin handle.
- parity row(s):   none; inventory M78.
- test:            n/a.
- proposed fix:    Implement `read` against `self.stdin` (for example `windows.ReadFile(self.stdin, buf, null)`), or remove it from `WindowsTty` if console input is meant to go only through `nextEvent`.
- upstream status: not reported

### UD50 — libxev epoll `pread`/`pwrite` start errors are tagged `.read`/`.write`
- kind:            latent (wrong union tag; a panic for any user of the path)
- upstream project: libxev
- severity:        low
- reference:       libxev (pin as UD46) `src/backend/epoll.zig` `Loop.start`: `.pread` arm :652-671 with `break :res .{ .read = err }` at :657 and :665; `.pwrite` arm :692-711 with `.{ .write = err }` at :701 and :709; `thread_schedule` at :526-528
- defect:          `Result` is a tagged `union(OperationType)` (:1181-1198) with separate `pread` and `pwrite` fields, documented as "the result tag will ALWAYS match the operation tag". On the start path's error branches (`thread_schedule`, `fd_maybe_dup`, or `epoll_ctl` failing) the two arms build `.read`/`.write` results for a `.pread`/`.pwrite` operation, and :838-846 hands that result to the callback. The perform side tags them correctly (`.pread` :1054, `.pwrite` :1069). `xev.File`'s callbacks read `r.pread` (`src/watcher/file.zig:118`) and `r.pwrite` (:273), and `File.pread`/`pwrite` set `threadpool = true` on epoll (file.zig:131-133, :302-304). So on a loop created without a `thread_pool`, `thread_schedule` returns `error.ThreadPoolRequired` (:528) and produces `.{ .read = error.ThreadPoolRequired }`. **UNVERIFIED:** traced by reading, not executed.
- ReleaseSafe:     panic: access of union field `pread` while field `read` is active.
- ReleaseFast:     undefined behaviour (an inactive-field read; both payloads are `ReadError!usize`, so it likely "works" by accident).
- reachability:    none in Ghostty (it uses `xev.Dynamic`, src/global.zig:30, but never `xev.File` or pread/pwrite operations). For libxev users: `File.pread`/`pwrite` on epoll with no thread pool, or a failing `dup`/`epoll_ctl`.
- port handling:   corrected: `crates/rustty-xev/src/epoll.rs:980-998` (`Operation::PRead`) and `:1110-1125` (`Operation::PWrite`) tag `Result::PRead`/`Result::PWrite`, with a REFERENCE-QUIRK PORT NOTE (which cites older reference lines).
- parity row(s):   none; inventory M79.
- test:            n/a (record only).
- proposed fix:    Tag the four sites `.{ .pread = err }` (:657, :665) and `.{ .pwrite = err }` (:701, :709). Regression test: on epoll with `Loop.init(.{})` (no thread pool), call `File.pread` and expect the callback to receive `error.ThreadPoolRequired` rather than a panic.
- upstream status: not reported

### UD51 — libxev dynamic `backend` doc says "first candidate" but the code defaults to the last
- kind:            latent (doc)
- upstream project: libxev
- severity:        low
- reference:       libxev (pin as UD46) `src/dynamic.zig:25-30` (doc: "The backend to use defaults to the first candidate backend and DOES NOT test that it is available") vs :91 `pub var backend: Backend = subset(bes[bes.len - 1]);`; the zero values of `DynamicCompletion` (:251-255) and `DynamicWriteQueue` (:377-381) also use the last candidate
- defect:          On Linux (`{ io_uring, epoll }`) the default before `detect()`/`prefer()` is epoll, not io_uring as documented. The code is internally consistent; only the comment is wrong.
- ReleaseSafe:     none; a reader who skips `detect()` expecting io_uring gets epoll.
- ReleaseFast:     same.
- reachability:    none (documentation).
- port handling:   faithful: `crates/rustty-xev/src/dynamic.rs:139-149` (`BACKEND_DEFAULT = CANDIDATES[len-1]`, REFERENCE QUIRK note).
- parity row(s):   none; inventory M80.
- test:            n/a.
- proposed fix:    Change the comment to "defaults to the last candidate backend (the most portable fallback)". If "first" is the intent, change :91 together with the two zero-value initialisers.
- upstream status: not reported

### UD52 — libxev dynamic `queueWrite` hands the callback a `*Completion` that points into no Completion
- kind:            latent (ub if dereferenced)
- upstream project: libxev
- severity:        low
- reference:       libxev (pin as UD46) `src/watcher/stream.zig:688-760` (dynamic `Writeable.queueWrite`), `api_cb` at :709-733 with `@fieldParentPtr("value", @as(*xev.Completion.Union, @fieldParentPtr(@tagName(tag), c_inner)))` at :724-727; backend `WriteRequest` at :37-56 (the backend `queueWrite` at :781-876 uses `&req.completion`); dynamic `WriteRequest = Dynamic.Union(&.{"WriteRequest"})` at `src/dynamic.zig:69`
- defect:          In the dynamic `queueWrite`, the backend completion `c_inner` is `&req.<tag>.completion`: the `completion` field of a backend `WriteRequest` stored inside the dynamic `xev.WriteRequest` union (:740-752, :801). The trampoline treats `c_inner` as the `<tag>` field of an `xev.Completion.Union` and then as the `value` field of an `xev.Completion`, but neither encloses it, so the `*xev.Completion` passed to the user's `cb` does not point at any `xev.Completion`. The other dynamic trampolines (for example :188-195) receive `&c.value.<tag>` from a real `xev.Completion` and are correct. Upstream's test already avoids the mismatch: `test "pty: queued writes"` checks `xev.WriteRequest.from(c_result.?) == &write_req[1]` only `if (!xev.dynamic)` (:1373-1377). **UNVERIFIED:** whether computing (not dereferencing) such a `@fieldParentPtr` is itself illegal behaviour under Zig's rules, and whether the dynamic-mode skip "this test fails on x86_64 with a strange error" (:1279-1281) is related.
- ReleaseSafe:     none unless the callback dereferences `c`; then it reads unrelated memory with no safety check.
- ReleaseFast:     same.
- reachability:    none in Ghostty: its only dynamic `queueWrite` callback, `ttyWrite` (src/termio/Exec.zig:471-485), ignores that parameter. Any libxev user whose dynamic `queueWrite` callback touches `c`.
- port handling:   faithful: `crates/rustty-xev/src/dynamic.rs:2264-2280` (REFERENCE QUIRK PORT NOTE plus a `# Safety` contract that `c` must not be dereferenced); the Rust layout makes the up-cast an identity cast, so no out-of-object pointer is computed.
- parity row(s):   none; inventory M81.
- test:            n/a (record only).
- proposed fix:    Do not synthesise an `*xev.Completion` in the dynamic `queueWrite` trampoline: either give the dynamic callback a `*xev.WriteRequest` (recovered with `@fieldParentPtr("completion", c_inner)` then `@fieldParentPtr(@tagName(tag), …)`), or document `c` as invalid. Adding `from` on the dynamic `WriteRequest` would let the test drop its `if (!xev.dynamic)` guard.
- upstream status: not reported

### UD53 — libxev io_uring `.noop` add consumes an SQE without filling it
- kind:            latent (ub if reached)
- upstream project: libxev
- severity:        low
- reference:       libxev (pin as UD46) `src/backend/io_uring.zig:362-403` (`add_`: `self.ring.get_sqe()` at :371, then the `.noop` arm at :399-403 marks the completion dead and returns without preparing the SQE); CQE dispatch :193-197 (`orelse continue` on `user_data == 0`); `user_data` set only after a real prep at :564; default `Completion.op = .noop` (:621); Zig 0.16 std `os/linux/IoUring.zig:139-148` (`get_sqe`) and :221-236 (`flush_sq`)
- defect:          `get_sqe` advances `sq.sqe_tail` and returns `sqes[tail & mask]` without clearing it, and `flush_sq` later submits every SQE between head and tail. So the `.noop` slot is submitted on the next flush with whatever it already holds. On a never-used slot that is presumably zeroed memory, so the kernel sees `IORING_OP_NOP` with `user_data = 0` and `tick` skips its CQE (:194); upstream's own test comments on this ("Due to implementation details, we need to have an ongoing completion…", :1158). Once the ring has cycled, the slot still holds a previous SQE's opcode, fd, buffer and `user_data`, so a `.noop` add resubmits a stale operation (a read into an old buffer, or a `close` of a possibly reused fd number) whose CQE invokes an old completion's callback again. **UNVERIFIED:** not executed; the "fresh SQE memory is zeroed" premise was not checked in kernel source.
- ReleaseSafe:     undefined behaviour after ring wrap (a stale operation re-executed; a callback on a dead or freed completion); no Zig safety check fires.
- ReleaseFast:     same.
- reachability:    libxev's own `test "io_uring: default completion"` (:1152-1180), on a fresh ring. No Ghostty caller adds a `.noop` completion.
- port handling:   faithful: `crates/rustty-xev/src/io_uring.rs:1644-1672` (REFERENCE QUIRK PORT NOTE with the stale-slot caveat); CQE skip note at :1415-1417.
- parity row(s):   none; inventory M82.
- test:            n/a (record only).
- proposed fix:    Handle `.noop` before `get_sqe`: mark it dead and return without touching the ring; or, if a posted CQE is wanted, call `sqe.prep_nop()` (Zig 0.16 `os/linux/io_uring_sqe.zig:23`) and set `sqe.user_data = 0` explicitly.
- upstream status: not reported

### UD54 — libxev `sendmsg` with the documented optional `buffer` panics with a TODO
- kind:            latent (crash with no caller)
- upstream project: libxev
- severity:        low
- reference:       libxev (pin as UD46) `src/backend/io_uring.zig:493-495` and `src/backend/epoll.zig:749-751` (`if (v.buffer) |_| @panic("TODO: sendmsg with buffer");`); the field is documented at `io_uring.zig:971-976` ("Optionally, a write buffer can be specified and the given msghdr will be populated with information about this buffer.")
- defect:          `Operation.sendmsg.buffer` is public and documented as supported, but both Linux backends panic when it is set. The `iov` scratch field meant to back it (:976) is never used.
- ReleaseSafe:     panic.
- ReleaseFast:     panic (`@panic` is unconditional).
- reachability:    none today: libxev's `UDP` watcher builds `sendmsg` without `buffer` (`src/watcher/udp.zig:645-651`), and Ghostty does not use `sendmsg`. Only a direct libxev user setting the documented field.
- port handling:   faithful: `crates/rustty-xev/src/io_uring.rs:1749-1757` and `crates/rustty-xev/src/epoll.rs:1156-1164` (`panic!("TODO: sendmsg with buffer")`).
- parity row(s):   none; inventory M83.
- test:            n/a.
- proposed fix:    Implement it (fill `v.iov[0]` from the buffer and point the `msghdr`'s `iov`/`iovlen` at it before `prep_sendmsg`), or remove the `buffer` field and its doc comment.
- upstream status: not reported

### UD55 — highway 1.2.0 adds the `evex512` target attribute on every GCC ≥ 14 and Clang ≥ 18, which GCC 16 rejects
- kind:            latent (build)
- upstream project: google-highway (already fixed on highway master; the actionable change for Ghostty is its pinned version)
- severity:        low
- reference:       highway 1.2.0 (tarball `highway-66486a10623fa0d72fe91260f96c892e41aceb06`, pinned by ghostty 20abdb50a `pkg/highway/build.zig.zon:8-11`) `hwy/ops/set_macros-inl.h:144-148`:
  ```c
  #if HWY_COMPILER_GCC_ACTUAL >= 1400 || HWY_COMPILER_CLANG >= 1800
  #define HWY_TARGET_STR_AVX3_VL512 ",evex512"
  ```
  appended to the AVX3, AVX3_DL, AVX3_ZEN4 and AVX3_SPR target strings (:154-190); the AVX10.2 strings at :186-194 also emit `no-evex512`.
- defect:          The guard has no upper compiler bound, but the attribute has been removed from newer compilers. **Measured (task R13-R, macOS host):** Homebrew's `x86_64-w64-mingw32-gcc` **16.2.0** rejects both `__attribute__((target("avx512f,evex512")))` and `target("avx512f,no-evex512")` with "attribute 'target' argument 'evex512' is unknown" (and `'no-evex512'`). Zig 0.16.0's bundled Clang (`zig cc -target x86_64-linux-gnu`) accepts both, so Ghostty's default build is unaffected today. highway master already bounds the guard (fetched 2026-09-11): `HWY_HAVE_EVEX512` is 1 only for `1400 <= GCC < 1600` or `1800 <= Clang < 2200`, citing llvm/llvm-project#157034 ("evex512 has been removed from clang 22"). The rustty build audit first hit this cross-building the byte-identical vendored header with mingw GCC 16 (`docs/superpowers/specs/2026-08-25-build-system-gap-audit.md:663-671`). **UNVERIFIED:** the first GCC release that rejects the attribute (only 16.2.0 was tested), the highway commit and release that carry the fix, and whether that fix also bounds the `no-evex512` strings.
- ReleaseSafe:     build failure of highway's x86 AVX3/AVX10 targets on an affected compiler (GCC 16, or Clang 22 per the upstream comment).
- ReleaseFast:     same.
- reachability:    build: an x86 target compiled with GCC 16 (mingw, or a future Linux CI image), or with a Zig whose bundled Clang is 22 or later.
- port handling:   byte-faithful vendor: `crates-sys/highway-sys/vendor/hwy/ops/set_macros-inl.h`, identical to the pinned copy (`cmp`) and deliberately not patched (audit :669-671).
- parity row(s):   none; inventory M85.
- test:            n/a. Probe: two one-line C files with the `target` attributes above, compiled with mingw GCC 16.2.0 and `zig cc` (Zig 0.16.0).
- proposed fix:    Upstream highway has the fix. For Ghostty: move `pkg/highway` to a highway release that includes the bounded `HWY_HAVE_EVEX512` guard (identify the commit first), which will be required before Ghostty's Zig moves to Clang 22.
- upstream status: fixed upstream in highway master (commit not identified); Ghostty's pin (1.2.0) does not have it

### UD57 — custom-shader colour uniforms read `.color` on a `TerminalColor` that can be `cell-foreground` or `cell-background`
- kind:            crash (ReleaseFast: ub)
- upstream project: ghostty
- severity:        medium (a documented config value plus any custom shader)
- reference:       src/renderer/generic.zig:2266-2293 (`updateCustomShaderUniformsFromState`: `cursor_text.color.r/g/b` at :2268-2270, `selection_bg.color` at :2278-2280, `selection_fg.color` at :2288-2290); `TerminalColor` at src/config/Config.zig:5703-5706; the compatibility handlers `compatCursorInvertFgBg` (:4981) and `compatSelectionInvertFgBg` (:5008), which set the cell-relative tags; at 20abdb50a
- defect:          The uniform update reads the `color` payload of `cursor-text`, `selection-background` and `selection-foreground` without checking the tag. `cell-foreground` and `cell-background` are documented values of all three, and `cursor-invert-fg-bg = true` / `selection-invert-fg-bg = true` set them. The update runs only when a custom shader is loaded.
- ReleaseSafe:     panic "access of union field 'color' while field 'cell-foreground' is active"; the terminal process aborts when the uniforms are updated. Measured with a Zig 0.16.0 probe that mirrors the union and the read (Debug and ReleaseSafe, macOS arm64).
- ReleaseFast:     undefined behaviour. In the probe the read returned 1.0 for `r`, a value no config set, and did not crash.
- reachability:    config: `custom-shader = <any shader>` together with `cursor-text`, `selection-background` or `selection-foreground` set to `cell-foreground` or `cell-background`, or with either `*-invert-fg-bg = true` compatibility key.
- port handling:   each uniform is written only for a `.color` value; a cell-relative value leaves the uniform unchanged, which is what the reference does when the option is unset. The alternative, resolving the value the way the cell path does (generic.zig:2726-2750 for the cursor, :2995-2999 and :3039-3043 for the selection), needs a specific cell's style and inverse flag, and these uniforms are one colour for the whole surface; the reference's own `TerminalColor.toTerminalRGB` (Config.zig:5715-5720) returns null for the cell-relative tags. Sites: `crates/rustty-renderer/src/generic.rs:5657` (Metal) and `crates/rustty-renderer/src/generic/linux_impl.rs:4127` (OpenGL), PORT NOTEs.
- parity row(s):   NEW-FINDINGS N46. Parity audit S116-F1 found the port's `unreachable!()` and was refuted on "same defined semantics", so it has no LEDGER row; `.port/phase-3/UPSTREAM_BUGS.md` U2 recorded it as "ported as-is". R13 reverses both. Related: UD20 (the two selection uniforms are also swapped).
- test:            `update_custom_shader_uniforms_from_state_cell_relative_colors` in both twins (Metal on macOS with `RUSTTY_REQUIRE_METAL=1`; OpenGL on the Linux VM with `RUSTTY_REQUIRE_GL=1`, so neither can skip): the three uniforms keep a sentinel, the foreground uniform is written (proves the update ran), and a `.color` value is still written. `cell_relative_cursor_and_selection_colors_reach_the_renderer` (rustty-config) loads the config text, including both compatibility keys, and checks the renderer accessors return the cell-relative tags. Negative controls, 2026-09-11: reinstating `unreachable!()` for a cell-relative `cursor-text` panics with "internal error: entered unreachable code" in the Metal twin (macOS, generic.rs:5671) and in the OpenGL twin (VM, linux_impl.rs:4137).
- proposed fix:    Guard each read with `toTerminalRGB`: `if (self.config.cursor_text) |c| if (c.toTerminalRGB()) |rgb| { … }`, and the same for both selection colours, and say in the `custom-shader` help that these uniforms carry only explicit colours. Resolving the cursor text colour from the cursor cell would also work for `iCursorText`, but not for the selection uniforms.
- upstream status: not reported

### UD58 — `goto_tab` narrows a `usize` into `GotoTab = enum(c_int)` without a range check
- kind:            crash (ReleaseFast: the wrong tab action)
- upstream project: ghostty
- severity:        low (a hand-written keybind or command-palette entry)
- reference:       src/Surface.zig:5317-5331 (`.goto_tab => @enumFromInt(v)` at :5328); `goto_tab: usize` at src/input/Binding.zig:575; `GotoTab = enum(c_int)` with `previous = -1`, `next = -2`, `last = -3` at src/apprt/action.zig:569-574; at 20abdb50a
- defect:          The action parser accepts any `usize` for `goto_tab` (`parseInt(usize, …)`, Binding.zig:1225-1227), but the surface converts it with `@enumFromInt` into a `c_int` enum whose negative values are the previous/next/last sentinels. Nothing checks the range in between.
- ReleaseSafe:     panic "invalid enum value" when the binding fires with an index above 2147483647 (measured: Zig 0.16.0 probe of the same conversion, Debug and ReleaseSafe, with 3000000000, 4294967294 and 4294967295).
- ReleaseFast:     the index is truncated to 32 bits. Measured in a probe that stores the result in an `extern struct` field: 4294967295 → -1 (`.previous`), 4294967294 → -2 (`.next`), 4294967293 → -3 (`.last`); 2147483648 → -2147483648 and 3000000000 → -1294967296, other negative indices. So `keybind = ctrl+1=goto_tab:4294967295` goes to the previous tab.
- reachability:    config: `keybind = …=goto_tab:N` or `command-palette-entry = …,action:goto_tab:N` with N ≥ 2147483648.
- port handling:   both action parsers reject an index above `binding::GOTO_TAB_MAX` (`c_int` max) with the error they already return for an integer that does not fit its type: `Action::parse` returns `InvalidFormat` (the reference's `parseInt … catch return Error.InvalidFormat`), and the tagged-union parser used by `command-palette-entry` returns `InvalidValue` (`cli/args.zig:429-433`). The config reports it as it reports any other bad keybind — `crates/rustty-input/src/binding.rs:1988` (PORT NOTE). The surface conversion is now `i32::try_from(v).expect(…)`, an R7 check that cannot fire — `crates/rustty-core/src/surface.rs:12168`. Before, the port's `v as i32` wrapped exactly like ReleaseFast.
- parity row(s):   LEDGER S42-F4 ("wraps instead of trapping"), closed by task R13-B; R13 reverses its implied fix, a trap.
- test:            `goto_tab_index_above_c_int_max_is_rejected_by_both_parsers` (rustty-input: 2147483647 accepted; 2147483648, 3000000000, 4294967293-4294967295 and `usize` max rejected by both parsers) and `tests/goto_tab_out_of_range.rs` (rustty-config, through `Config::load_iter`: one diagnostic per bad `keybind` or `command-palette-entry` line, none for 2147483647, and the same message an out-of-range `jump_to_prompt:70000`, an `i16`, gets). Negative control, macOS 2026-09-11: with both range checks disabled, the rustty-input test fails on `Ok(GotoTab(2147483648))` where `Err(InvalidFormat)` was expected, and the config test fails with no diagnostic for `goto_tab:2147483648`.
- proposed fix:    Give the field a type its consumer can hold, such as `goto_tab: u31`, so `parseInt` rejects larger values with the existing `InvalidFormat`; or, in `Surface.zig`, `std.math.cast(c_int, v) orelse return false` before `@enumFromInt`.
- upstream status: not reported

### UD59 — initial window size and size-limit arithmetic overflows `u32`
- kind:            crash (ReleaseFast: a wrong window size)
- upstream project: ghostty
- severity:        low (a config value far outside any real window)
- reference:       src/Surface.zig:1856-1886 (`recomputeInitialSize`: the products at :1866-1873, `@intFromFloat(@ceil(px / scale))` plus padding at :1874-1886) and :705-706 (the `size_limit` minimums in `init`); `window-height`/`window-width` are `u32` (src/config/Config.zig:2243-2244), clamped only from below (:4854-4855); `Metrics.Modifier.apply` (src/font/Metrics.zig:528-541) adds an `i32` to the cell size with saturation; at 20abdb50a
- defect:          `@max(window_height, 4) * cell.height` and the width product are unchecked `u32` multiplications, the result goes through `@intFromFloat(@ceil(@as(f32, …) / scale))`, and the padding is added unchecked. `window-height = 1000000000` overflows the product for any cell taller than 4 px. `window-height = 214748364` with a 20 px cell fits the product (4294967280), but `@floatFromInt` rounds it to 2^32, which `@intFromFloat` cannot fit in a `u32`. In `init`, `cell.width * 10` and `cell.height * 4` overflow for a cell wider than 429496729 px or taller than 1073741823 px, which `adjust-cell-width` / `adjust-cell-height` can produce.
- ReleaseSafe:     panic "integer overflow", or "integer part of floating point value out of bounds" for the rounding case (both measured with a Zig 0.16.0 probe of the two expressions, Debug and ReleaseSafe). `recomputeInitialSize` runs from `init` (:747) and after a cell-size change (:2418), so the terminal aborts at startup.
- ReleaseFast:     no crash: the product wraps (1000000000 × 20 → 2820130816, measured) and the apprt is sent a wrong initial size; the float conversion gave 4294967295 in the probe (arm64).
- reachability:    config or CLI: `window-height = 1000000000` with `window-width` set. For :705, `adjust-cell-width = 500000000`; whether `init` survives such a cell up to that line was not measured.
- port handling:   `initial_size_px` computes the size with checked multiplication, a range-checked conversion and checked addition. On overflow `recomputeInitialSize` returns early, the path it already takes when either option is unset, so no `initial_size` is sent — `crates/rustty-core/src/surface.rs:283` (helper PORT NOTE) and :5180. `size_limit_min` checks the two products, and `init` skips the `size_limit` action if either overflows (no minimum is imposed) — :2542. The port had panicked at the product (release keeps overflow checks), and its `as u32` conversion had saturated where the reference traps.
- parity row(s):   NEW-FINDINGS N47 (no LEDGER row).
- test:            `initial_size_px_overflow_takes_the_skip_path` and `size_limit_min_overflow_skips_the_action` (rustty-core; config text through `Config::load_iter` and `finalize`): 80x24 gives the expected size; `window-height = 1000000000`, `window-width = 1000000000`, `window-height = 214748364` with no padding, and a padding overflow each give `None`; `adjust-cell-width = 500000000` applied to a 10 px cell gives 500000010 and `None`. `Surface::init` cannot run without an apprt, so the tests call the two helpers with the parsed values. Negative controls, macOS 2026-09-11: an unchecked height product panics with "attempt to multiply with overflow" (surface.rs:296); the conversion without its range check returns `Some((800, 4294967295))` where `None` was expected; an unchecked `size_limit` product panics with "attempt to multiply with overflow" (:328). The conversion control first stayed green, because 2 px of padding overflowed the saturated value anyway; that case now has no padding.
- proposed fix:    Use `std.math.mul` and `std.math.add` and check the float against `maxInt(u32)` before `@intFromFloat`, returning a new `InitialSizeError` member that both callers' existing `log.warn` reports; use `std.math.mul` at :705-706 and skip `size_limit` on overflow. Alternatively clamp `window-height` and `window-width` from above in `Config.finalize`.
- upstream status: not reported

### UD60 — UTF-8 mouse reports hit `catch unreachable` on a surrogate coordinate
- kind:            crash (ReleaseFast: invalid UTF-8 written)
- upstream project: ghostty
- severity:        low (a grid at least 55264 cells wide or tall)
- reference:       src/input/mouse_encode.zig:134-149 (the `.utf8` arm; `std.unicode.utf8Encode(…) catch unreachable` at :144 and :147); the grid clamp in src/renderer/size.zig:129-149; the C-ABI `size` option at src/terminal/c/mouse_encode.zig:162; at 20abdb50a
- defect:          The UTF-8 (1005) format encodes `x + 33` and `y + 33` as code points. The cell COLUMN is a `u16` (`point.Coordinate.x` is `size.CellCountInt`); the ROW is a `u32` (`point.Coordinate.y`, src/terminal/point.zig:99-107, deliberately wider because screen/history coordinates can exceed one page), bounded in practice by the `u16` grid row count. Either way, for 55263 ≤ x ≤ 57310 (or y) the value is a surrogate (U+D800..U+DFFF), `utf8Encode` returns `error.Utf8CannotEncodeSurrogateHalf`, and the `catch unreachable` fires. `ESC [ M` and the button byte have already been written.
- ReleaseSafe:     panic "attempt to unwrap error: Utf8CannotEncodeSurrogateHalf" (measured with a Zig 0.16.0 probe, Debug and ReleaseSafe).
- ReleaseFast:     undefined behaviour. In the probe the surrogate was written as `ED A0 80` (x = 55263) and `ED BF BF` (x = 57310), which is not valid UTF-8, and nothing crashed.
- reachability:    C ABI (libghostty-vt): the mouse encoder's `size` option with a screen at least 55264 cells wide or tall (for example 60000 px of 1 px cells), the UTF-8 format, and an event in a surrogate column or row. The app's own grid cannot plausibly get that large.
- port handling:   both code points are checked before anything is written; if either is not a Unicode scalar value the event is not reported, as the X10 arm (:121-125) does for a coordinate its format cannot carry. No message is logged, because the X10 arm's text is specific to X10 — `crates/rustty-vt/src/input/mouse_encode.rs:245` (PORT NOTE).
- parity row(s):   NEW-FINDINGS N48 (no LEDGER row).
- test:            `encode_utf8_surrogate_coordinate_is_not_reported` (rustty-vt-c, through the C-ABI encoder with a 60000x60000 px grid of 1 px cells): SGR reports cells 55263 and 57310 (proves the grid and the press); UTF-8 at x = 55262 writes `ESC [ M`, space, `ED 9F BF`, `!`, and at x = 57311 writes `EE 80 80`; x or y of 55263 or 57310 returns success with nothing written. Negative control, macOS 2026-09-11: reinstating the unwrap panics with "called `Option::unwrap()` on a `None` value" (mouse_encode.rs:258).
- proposed fix:    Encode both coordinates into a local buffer before writing anything and `catch return` (or check `0xD800 <= cp <= 0xDFFF`), mirroring the X10 range check.
- upstream status: not reported

### UD61 — kitty shared-memory images are copied out of a mapping the sender can truncate, so the terminal takes SIGBUS
- kind:            crash (SIGBUS)
- upstream project: ghostty
- severity:        low (a program in the terminal must shrink its own object inside a short window)
- reference:       src/terminal/kitty/graphics_image.zig:208-274 (`shm_open` at :227, the `fstat` size at :245-254, `mmap` of `stat_size` at :259-270, `appendSlice(alloc, map[range.start..range.end])` at :273), at 20abdb50a
- defect:          The image data is copied out of a `MAP_SHARED` mapping of an object another process owns, sized by an earlier `fstat`. If the owner shrinks the object with `ftruncate` after that `fstat`, the copy touches pages past the end of the object. The `defer shm_unlink` does not prevent it, because the owner keeps its descriptor.
- ReleaseSafe:     SIGBUS; the terminal process dies. A signal, so the same in every build mode.
- ReleaseFast:     same.
- reachability:    a program running in the terminal (anything that can write to its pty) that sends `a=t,t=s` naming an object it owns, and shrinks the object between the terminal's `fstat` and the end of its copy; Linux. Measured on Linux aarch64 (VM, 2026-09-11) with a C probe of the same syscall sequence: after the owner's `ftruncate(fd, 0)`, `mmap(stat_size)` still succeeds and the first read of the mapping is killed by SIGBUS, while `pread` of the same range returns 0. The port's copy dies the same way (see test). macOS refuses to shrink a POSIX shared-memory object (`ftruncate` returns EINVAL, measured), so it is not affected. FreeBSD was not measured. The crash was triggered with a deterministic hook in the window, not by a racing process.
- port handling:   the range is read with `pread`, which reports a truncated object as a short read; a short read or a read error fails the load with `InvalidData`, so the client gets `EINVAL: invalid data`, and no new message is logged. Where `pread` cannot read POSIX shared memory (macOS answers ESPIPE, measured), the mapping is kept and copied through a raw pointer. The port also had a defect of its own here (inventory P6): it formed a `&[u8]` over the peer-writable mapping, which is undefined behaviour in Rust even without a truncation; no reference to the mapping is formed now — `crates/rustty-vt/src/kitty/graphics_image.rs:482` (PORT NOTE), `pread_range` at :579.
- parity row(s):   NEW-FINDINGS N49 (no LEDGER row; inventory M20 and P6).
- test:            `kitty_graphics_shared_memory_copies_the_requested_range` (macOS and Linux: `O=5,S=12` of a 4096-byte object, through the VT stream, stores exactly those bytes and answers OK) and `kitty_graphics_shared_memory_truncated_after_fstat_is_invalid_data` (Linux: a test-only hook between the `fstat` and the read truncates the object; asserts the hook ran, the response is `EINVAL: invalid data`, and no image is stored). Negative control, VM 2026-09-11: with `pread_range` forced to report Unsupported, so the load takes the reference's map-and-copy path, the test binary is killed by signal 7 ("SIGBUS: access to undefined memory").
- proposed fix:    Read the range with `std.posix.pread` in a loop instead of mapping it, and treat a short read as `error.InvalidData`. Where POSIX shared memory cannot be read with `pread` (macOS), keep the mapping; macOS does not let the owner shrink the object.
- upstream status: not reported

### UD62 — a `scroll_page_fractional` keybind, and any extreme scroll offset, crash on the float-to-int cast
- kind:            crash (safety-checked trap) / undefined behaviour
- upstream project: ghostty
- severity:        low (the user must bind an extreme fraction, or an embedder must send an extreme offset)
- reference:       `src/Surface.zig:5266-5268` (`const rows: f32 = @floatFromInt(self.size.grid().rows); const delta: isize = @intFromFloat(@trunc(fraction * rows));`), and the three gesture casts at `:3520`, `:3529`, `:3543`, at 20abdb50a
- defect:          `@intFromFloat` is safety-checked and the operand is unbounded. `fraction` comes from `std.fmt.parseFloat(f32, ...)` on a keybind argument (`Binding.zig:1229-1230`) with no range check, and `parseFloat` accepts `1e30`, `inf` and `nan` (measured). The gesture casts take an accumulated pixel offset derived from `xoff`/`yoff`, which a C-ABI embedder supplies as raw `f64` (`ghostty_surface_mouse_scroll`).
- ReleaseSafe:     panic "integer part of floating point value out of bounds", on the keypress or the scroll event. Measured with Zig 0.16.0 on macOS arm64 (`r13p-reference-probe.zig`: `fractional 1e30`, `gesture 1e300`, `gesture inf`); Debug is identical.
- ReleaseFast:     no trap; the value saturates to `9223372036854775807` (measured). That is undefined behaviour by the language, and on the gesture path it is arguably worse than the trap: `scrollCallback`'s own consumers loop `for (0..@abs(y.delta))` writing one mouse report or one cursor-key sequence per row (`Surface.zig:3587-3612`), so a saturated delta hangs the surface.
- NaN:             does NOT trap, in any mode: the safety check lets it through (corrected by task R13-P2; this record first listed `nan` with the trapping values). The result is target-dependent. Measured with Zig 0.16.0 for `@intFromFloat(@trunc(v))`, `@intFromFloat(@round(v))` and the f32 `scroll_page_fractional` expression, Debug, ReleaseSafe and ReleaseFast alike: `0` on macOS arm64, `-9223372036854775808` on x86_64 (`-target x86_64-macos`, run under Rosetta 2, not native x86_64 hardware). So on x86_64 the discrete x gesture cast (`:3529`), which no assert precedes, turns a NaN offset into the same `isize` extreme the per-row loops walk, i.e. the ReleaseFast hang above, in every build; and `scroll_page_fractional:nan` queues a `scroll_viewport` delta of `isize::MIN`, a scroll toward the top (what the reference's viewport does with that value was not measured). On the y cast and the precision x cast, `assert(@abs(amount) >= 1)` (`:3516`, `:3541`) precedes the cast, so a NaN trips it first in a safe build.
- reachability:    (a) config: `keybind = ctrl+a=scroll_page_fractional:1e30` (also `inf`; `nan` parses too but does not trap, see above), then the keypress; (b) a C-ABI embedder passing an infinite or astronomically large scroll offset (or, for the x86_64 arm above, a NaN).
- port handling:   `crates/rustty-core/src/surface.rs` `isize_from_float` reports the values the reference traps on instead of converting them — and NaN, which has no `isize` either — and each caller takes the handling its own value means. `scroll_page_fractional` saturates (`isize_from_float_saturating`): the fraction means "scroll as far as it goes", and `scroll_viewport` clamps it to the top or the active area. The three gesture casts scroll nothing for such a value — the `.{}` arm the same function already takes for an offset too small to act on — and the guard runs BEFORE the pending-offset store, so a non-finite offset can never leave `pending_scroll_x/y` holding a NaN that poisons every later scroll, and before the `delta` assert, which holds for every value that gets past it. For NaN (re-checked by task R13-P2, since the handling had been argued from a NaN trap the reference does not have): `scroll_page_fractional` yields `0` and the gestures scroll nothing, which is the delta-0 result arm64 produces and is safer than x86_64's `isize::MIN`; the port keeps `debug_assert!(amount.abs() >= 1.0)` ahead of the y and precision-x casts, as the reference asserts there. The gesture blocks are `scroll_amount_y` / `scroll_amount_x`, lifted verbatim out of `scrollCallback` by task R13-P2 so they can be driven by a test.
- parity row(s):   no LEDGER row. Inventory: none (M27-M29 are other float casts). The two port tests that pinned the trap (`isize_from_float_nan_traps_in_checked_builds`, `isize_from_float_out_of_range_traps_in_checked_builds`) are REVERSED, as S114-F2's was.
- test:            `isize_from_float_reports_the_values_the_reference_traps_on`, `isize_from_float_reports_nan_whose_reference_value_is_target_dependent` and `isize_from_float_saturating_takes_the_end_in_each_direction` (rustty-core), plus `a_saturated_scroll_delta_is_clamped_by_the_viewport`, which drives the saturated delta into `Terminal::scroll_viewport` and shows it clamps without a panic and without walking one row at a time. Negative control, macOS 2026-09-11: with the range check removed (an unconditional `Some(v as isize)`), `isize_from_float_reports_the_values_the_reference_traps_on` fails. Restored. The gesture call sites (task R13-P2): `y_gesture_cast_out_of_range_scrolls_nothing`, `x_discrete_gesture_cast_out_of_range_scrolls_nothing` and `x_precision_gesture_cast_out_of_range_scrolls_nothing` drive `scroll_amount_y` / `scroll_amount_x` with mods decoded by `ScrollMods::from_int`, asserting the no-scroll arm, the untouched pending offset, and exact in-range deltas. Negative controls, macOS 2026-09-22, one site at a time: each cast made to saturate fails its own test (`ScrollAmount { delta: 0 }` expected, `9223372036854775807` got) and only that one; each cast made to trap (`.expect(..)`) fails its own test with that panic; all three saturating at once fails all three. `scrollCallback` itself is still not run, because `Surface::init` needs an apprt.
- proposed fix:    Reject the out-of-range fraction where the binding is parsed (as every other out-of-range binding argument is rejected), or clamp with `std.math.lossyCast`. For the gesture casts, use `std.math.lossyCast` or skip the event when the accumulated offset is not finite.
- upstream status: not reported

### UD63 — a C-ABI Kitty image storage limit above `usize` aborts the library on 32-bit targets
- kind:            crash (safety-checked trap)
- upstream project: ghostty
- severity:        low (32-bit targets only; the embedder must pass a limit above 4 GiB)
- reference:       `src/terminal/c/terminal.zig:1370-1378` (`const limit: usize = if (value) |v| @intCast(v.*) else 0;`), at 20abdb50a
- defect:          The option's declared input type is `?*const u64` (`:1246`), so the value is whatever a C caller passes. `@intCast` is a checked narrowing, and on a target where `usize` is 32 bits (wasm32, which is a Kitty-graphics-enabled target) any value above `u32` max has no destination representation.
- ReleaseSafe:     panic "integer does not fit in destination type", inside a C entry point — the host process aborts. Measured with Zig 0.16.0 at the same cast narrowed to u32 (`r13p-reference-probe.zig`, `limit32 4294967296`); Debug is identical.
- ReleaseFast:     wraps (4294967296 becomes 0, measured), i.e. the caller silently gets a zero storage limit.
- reachability:    any embedder of libghostty-vt on a 32-bit target calling `ghostty_terminal_set` with `GHOSTTY_TERMINAL_OPTION_KITTY_IMAGE_STORAGE_LIMIT` and a `uint64_t` above 4 GiB. Not reachable on 64-bit, where the conversion is the identity.
- port handling:   `crates/rustty-vt-c/src/terminal.rs` converts with `saturating_from_u64`, so a limit larger than the address space becomes `usize::MAX` — "no practical limit", which is what such a request means. The port's previous `usize::try_from(..).unwrap()` was stricter than the reference's own safe builds: it trapped in EVERY profile.
- parity row(s):   no LEDGER row; codex-vet W33-B07 / W33-B07-32BIT-01, whose "traps (like Zig's safety-checked @intCast)" resolution this reverses under R13. The port test that pinned the trap (`kitty_image_storage_limit_over_usize_traps`) is REVERSED.
- test:            `saturating_from_u64_takes_the_maximum` (rustty-vt-c), which drives the same helper at u32 width on this 64-bit host, and `kitty_image_storage_limit_over_usize_saturates`, the reversed 32-bit test, which CANNOT BE RUN here — no 32-bit target is installed and none was built. Negative control, macOS 2026-09-11: with the saturating arm replaced by a panic, `saturating_from_u64_takes_the_maximum` fails. Restored.
- proposed fix:    `std.math.lossyCast(usize, v.*)`, or return `.invalid_value` for a limit the target cannot represent.
- upstream status: not reported

### UD64 — a decoded PAGE header's capacity fields reach `assert(layout.total_size <= max_page_size)`
- kind:            crash (ReleaseFast: ub)
- upstream project: ghostty
- severity:        medium (one PAGE header field; any producer of snapshot bytes can set it)
- reference:       `src/terminal/snapshot/page.zig:565-578` (`Header.pageCapacity`); the capacity reaches `createPageExt` two ways, SCREEN pages via `src/terminal/snapshot/screen.zig:381-385` → `PageList.zig:7541-7553` and HISTORY pages via `src/terminal/snapshot/history.zig:215-219` → `PageList.zig:4370-4383`; the trap is `src/terminal/PageList.zig:4566`, with `max_page_size = maxInt(u32)` (`src/terminal/size.zig:8`); at 20abdb50a
- defect:          `pageCapacity`'s own doc says it validates "native allocation requirements", but it rejects only `columns == 0 or rows == 0`. The header's `columns`/`rows` (u16), `grapheme_capacity_bytes` and `string_capacity_bytes` (u32) then flow unchecked into `createPageExt`, whose assertion is the first thing that looks at them — and whose own comment (`:4563-4565`) says callers are expected to handle the limit themselves. `string_capacity_bytes = 0xFFFFFFFF` alone exceeds the limit, as does 65535 x 65535 cells. Native callers cannot do this: `initialCapacity` proves the bound at comptime (`PageList.zig:518-523`) and `increaseCapacity` returns an error instead (`:4253`, `:4310`) — which is why the assertion is a sound invariant for them and only decoded input violates it. The record CRC is no barrier: the capacity is consumed before the payload checksum is verified (`page.zig:192-205`, `screen.zig:381-385`).
- ReleaseSafe:     panic at `PageList.zig:4566`; the embedding process aborts. By reading — no Zig probe was built for this entry.
- ReleaseFast:     `assert` is `unreachable`, so this is undefined behaviour. If the optimizer drops the check, an `alignedAlloc` above 4 GiB can succeed on 64-bit and every `getOffset` in `initBuf` then `@intCast`s to the `u32` `OffsetInt` (`size.zig:149-158`), truncating, so page offsets alias. On a 32-bit target (wasm32, which libghostty-vt builds for) `Page.layout`'s `usize` arithmetic (`page.zig:1757`) wraps BEFORE the assertion, so a small `total_size` passes it, the buffer is undersized, and `initBuf` and the grid decode write out of bounds. Both outcomes UNVERIFIED (not probed).
- reachability:    the libghostty-vt / rustty-vt C ABI snapshot decoder — `decoder_new`/`decoder_new_buf`, then `decoder_ready`/`decoder_next`/`decoder_decode` — i.e. an embedder decoding snapshot bytes that came from outside (a file, a peer process, the network). The decoder's own comments say that input is untrusted. NEITHER the Ghostty app nor the rustty app decodes snapshots itself, so the C ABI is the whole reachability.
- port handling:   `crates/rustty-vt/src/snapshot/page.rs` `check_native_page_size`, called from `Header::page_capacity`, rejects such a capacity with that function's OWN existing `CapacityError::InvalidDimensions`, which surfaces as the decoder's ordinary malformed-input error. One site covers both the SCREEN and HISTORY routes, because both take their capacity from `Decoder::capacity`. The shared assertion in `pagelist/mod.rs:2391` is deliberately LEFT AS IS: it is a true invariant for the native callers (R7), and what R13 forbids is letting decoded input reach it. A `u64` pre-gate bounds the layout before it is computed, so the exact computation cannot overflow a 32-bit `usize` either.
- parity row(s):   none (no LEDGER row; found by the R13-G part A sweep as A1).
- test:            `decoder_rejects_a_page_capacity_that_would_abort_the_library` (rustty-vt-c) drives the real C ABI: it encodes a live terminal, decodes those bytes unpatched as a control, then patches the first PAGE record's `string_capacity_bytes` to `0xFFFFFFFF` and asserts `decoder_decode` answers `invalid_value` and writes no terminal. `decode_rejects_a_capacity_the_native_page_allocation_cannot_hold` (rustty-vt) covers the three shapes at the payload level AND asserts positively that a capacity a page really can hold is NOT rejected (it proceeds to `EndOfStream` on the absent grid), so the test cannot pass by rejecting everything. `the_page_size_bound_is_an_upper_bound_on_the_real_layout` sweeps the extremes of all six capacity fields and pins the pre-gate as a real upper bound on `Page::layout`. Negative control, macOS 2026-09-11: with the check removed, the C-ABI test dies at `assertion failed: layout.total_size <= size::MAX_PAGE_SIZE` (`crates/rustty-vt/src/pagelist/mod.rs:2391`) — the ported assertion, reached exactly as predicted — and the payload-level test returns `EndOfStream` where `InvalidDimensions` was expected. Restored.
- proposed fix:    in `pageCapacity`, after the zero check, `if (Page.layout(cap).total_size > size.max_page_size) return error.InvalidDimensions;`, with the product computed through `std.math.mul`/`add` first so the bound also holds on 32-bit targets.
- upstream status: not reported

### UD65 — more than 65534 cells sharing one style in a decoded PAGE overflow the `u16` style refcount
- kind:            crash (ReleaseFast: refcount corruption)
- upstream project: ghostty
- severity:        medium (same reachability as UD64)
- reference:       `src/terminal/snapshot/grid.zig:1262` (`applyCell`'s `page.styles.use(page.memory, wire.style_id)`) → `src/terminal/ref_counted_set.zig:365-379` (`use`, an unguarded `item.meta.ref += 1`), where `ref: RefCountInt` is `size.CellCountInt` = `u16` for the style set (`src/terminal/style.zig:681-684`, `src/terminal/size.zig:22`); at 20abdb50a
- defect:          `applyCell` takes one reference per styled cell and nothing bounds the count. Natively that is safe by construction: `std_capacity` is 215 x 215 = 46,225 cells (`page.zig:1829-1834`) and `adjust` preserves the page's byte budget, so a page cannot hold 65,535 cells. A snapshot PAGE, however, declares its own `columns` x `rows`, so a 256 x 256 page — about half a megabyte — holds 65,536 cells, and if they share one style the 65,535th `use` overflows. Duplicate style-table entries with the same value each take a reference too, which only lowers the number of cells needed.
- ReleaseSafe:     integer-overflow panic at `ref_counted_set.zig:378`; the process aborts. By reading — no Zig probe was built for this entry; the equivalent port overflow WAS measured (see the negative control).
- ReleaseFast:     `ref` wraps to zero, so a live style reads as dead while cells still reference it. The post-grid release loop (`snapshot/page.zig:457-461`) then hits `release`'s `assert(item.meta.ref > 0)` (ub) or underflows, and `addContext`'s dead-item trimming (`ref_counted_set.zig:246-249`) can delete or reuse a slot that cells still point at. The result is stale or wrong styles and corrupt set accounting. A memory-unsafe outcome is UNVERIFIED: the IDs themselves stay below `layout.cap`.
- reachability:    as UD64 — the C ABI snapshot decoder, on a page the sender describes.
- port handling:   `crates/rustty-vt/src/snapshot/grid.rs` `apply_cell` asks `ref_count_saturated` before taking a reference and, at the ceiling, stores the cell with the DEFAULT style instead. This is the degrade the same function already applies to a style ID it cannot map, and the one the hyperlink half applies when `set_hyperlink` fails, so it invents no new behaviour and costs only that cell's style. It was preferred to rejecting the page outright, which would refuse pages that decode correctly today. The native write path's own answer to a style it cannot store is the same family: `Screen.manualStyleUpdate` (`src/terminal/Screen.zig:2499-2514`) treats a full style set as an out-of-capacity condition, growing via `increaseCapacity` and splitting the page on `OutOfSpace` — neither of which a decoder filling a fixed, sender-declared page can do.
- parity row(s):   none (no LEDGER row; found by the R13-G part A sweep as A2).
- test:            `decode_degrades_cells_past_the_style_refcount_ceiling` (rustty-vt) hand-builds a 256 x 256 PAGE payload whose every cell carries one style ID, decodes it, and asserts POSITIVELY that the style was really installed and really applied (the set holds one style; the first cell carries it) and that the cells past the ceiling carry the default style, with the surviving refcount at its maximum. Negative control, macOS 2026-09-11: with the guard disabled the decode panics "attempt to add with overflow" at `crates/rustty-vt/src/ref_counted_set.rs:110`. Restored.
- proposed fix:    the same degrade in `applyCell` — `if (page.styles.refCount(page.memory, id) == std.math.maxInt(RefCountInt)) { store the cell with style 0; }` — or make `use` saturate and report, or reject a PAGE whose `columns * rows` exceeds `maxInt(CellCountInt)` in `pageCapacity`.
- upstream status: not reported

### UD66 — TERMINAL strings and the cursor hyperlink allocate a `u32` length the record cannot contain
- kind:            latent (resource)
- upstream project: ghostty
- severity:        low (same reachability as UD64)
- reference:       `src/terminal/snapshot/terminal.zig:887-895` (pwd and title) and `src/terminal/snapshot/hyperlink.zig:121-158`, the standalone decoder reached from `src/terminal/snapshot/screen.zig:345` via `decodeCursorHyperlink` (`:1117-1142`); at 20abdb50a
- defect:          each length is read from the payload and allocated before it is compared with the bytes left in the record, although the record reader always knows that bound (`payload.bufferedLen()` when borrowed, `limited.remaining` when streaming). A sub-kilobyte record therefore makes the decoder request a 4 GiB heap block per string. On a memory-constrained or no-overcommit host the request fails and TERMINAL decode reports `out_of_memory` instead of malformed input; for the cursor hyperlink `decodeCursorHyperlink` swallows the failure as "no cursor hyperlink", so the misreport is silent.
- ReleaseSafe:     no crash; a transient oversized reservation and a misleading out-of-memory result.
- ReleaseFast:     the same.
- reachability:    as UD64 — the C ABI snapshot decoder.
- port handling:   a `payload_remaining()` accessor on the snapshot reader trait projects the two bounds the reference reaches directly (only `record::Reader` implements it; a callback source honestly answers "unknown" and is not checked). `crates/rustty-vt/src/snapshot/terminal.rs` (`read_string`) and `crates/rustty-vt/src/snapshot/hyperlink.rs` (`check_declared_len`) reject a declared length greater than the record's remaining payload with `EndOfStream` — the error the read itself would return a moment later, so callers that already swallow it, such as `decodeCursorHyperlink`, keep their behaviour exactly.
- parity row(s):   NEW-FINDINGS N54, which is the port-only half (the zero-fill).
- test:            `terminal_decode_rejects_a_string_length_the_record_cannot_contain` (rustty-vt) and `decode_rejects_a_string_length_the_record_cannot_contain` (rustty-vt, hyperlink), each framing a real record so the bound exists, each asserting the malformed-input error AND — via the allocation tripwire armed on the string allocation — that the allocation was NEVER REACHED. The error alone is not sufficient proof: the unbounded code also ends at `EndOfStream`, just 4 GiB later. Both tests also decode a truthful record, so the bound cannot be rejecting everything. Negative controls, macOS 2026-09-11, one at a time: with either bound removed the test fails with `Err(OutOfMemory)` where `Err(EndOfStream)` was expected, i.e. the allocation was attempted. Restored.
- proposed fix:    compare the declared length with the record's remaining payload before allocating and return `error.EndOfStream`, which the code already returns a moment later.
- upstream status: not reported

## Candidates pending verification

Assembled from this plan's progress notes on 2026-09-11. **Not verified** — do not cite as fact.

| # | reference | suspected defect | kind | port today | source |
|---|---|---|---|---|---|
| C3 | `src/renderer/Thread.zig:698-723` `cursorCancelCallback` | `unreachable` on cancel errors other than Canceled/NotFound; reachability depends on what the event loop can return | crash? | **ported as `unreachable!()`** (task 14d-1, S114-F2) — reachability must be decided | task 14d-1 |
| C4 | `src/termio/Exec.zig:1045`; `src/apprt/gtk/pre_exec.zig:39,58` | `log.err` between fork and exec: the log function allocates and locks, so a child forked from a multi-threaded parent can deadlock silently | deadlock | handled safely (raw `write`, tasks 14d-3 and 14e-1) | tasks 14d-3, 14e-1 |
| C5 | `src/font/DeferredFace.zig:154-157, 186-197` | zero-copy `cstringPtr` path returns into a +1 CFString that is never released | leak | rows S85-F6 / S90-F3 left OPEN | task 16 |
| C11 | `src/font/face/coretext.zig:1023-1036`; `src/terminal/StringMap` tests | assertions cannot distinguish the fixture under test | test-weakness | ported verbatim (S90-F5, S140-F1) | task 17 |

### UD56 — GDK's own X11 requests exit the process after another client destroys a toplevel
- kind:            crash
- upstream project: gtk
- severity:        low (another X client on the display must destroy the window; GTK may consider that fatal by design)
- reference:       not Ghostty code. GDK's X11 backend (GTK 4.22.4, `gdk/x11`) issues requests against a toplevel whose X window another client destroyed; Ghostty reaches it through ordinary `gtk.Window` calls such as `maximize` and `set_visible` (cross-referenced from UD3).
- defect:          After an external `XDestroyWindow` on a mapped toplevel, GDK logs "GdkSurface 0x… unexpectedly destroyed" but keeps the `GtkWindow` realized and mapped. Later GDK requests against the dead XID are made without an error trap, and GDK's own default X error handler then calls `exit(1)`.
- ReleaseSafe:     the process exits with status 1 ("The program '…' received an X Window System error … BadWindow").
- ReleaseFast:     same (C library behaviour, not a Zig build mode).
- reachability:    another X client on the same display, e.g. `xdotool windowclose`. MEASURED on the VM with a plain Python GTK 4.22.4 window that makes no X calls of its own (GDK_BACKEND=x11, headless sway's Xwayland), one process per case, reproducing the task-20 fix-round-3 reviewer's results:
  - destroy, then an immediate `maximize()` with nothing pumped: EXITS 1, `request_code 8` (`MapWindow`);
  - destroy, pump 1000 iterations, then `maximize()` / `unmaximize()`: survives;
  - destroy, pump, then `set_visible(False)` / `set_visible(True)`: EXITS 1, `request_code 18` (GDK's own `ChangeProperty`);
  - destroy, pump, then `set_title` and a resize: survive.
  The reviewer additionally found `present`, `fullscreen` and `minimize` survive.
- port handling:   none possible in the port. The failing requests are GDK's, so the port's maximize and hide/show paths remain exposed. rustty traps only its own winproto property calls (UD3).
- parity row(s):   none; found while measuring M110 / UD3.
- test:            none in the port; the reproduction is the plain-GTK script above.
- proposed fix:    Report to GTK. Either trap errors around GDK's own requests on a surface marked destroyed, or stop issuing requests once "unexpectedly destroyed" has been handled (for example, unrealize the `GtkWindow`). GTK may instead document an externally destroyed toplevel as fatal; the report should ask which.
- upstream status: not reported
