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
- reference:       src/apprt/gtk/winproto/x11.zig:346-431 (`getWindowProperty` from :346, `changeProperty` :392-422, `deleteProperty` :424-431), reached from `resizeEvent` / `syncAppearance` (:218-234) via `syncBlur` (:243-287) and `syncDecorations` (:289-), at 20abdb50a
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
  4. The external destroy with NOTHING PUMPED, then `maximize`: EXITS 1, `request_code 8` (`MapWindow`). This is a race, and the request is GDK's own. After a pump, `maximize`/`unmaximize` SURVIVE, but a hide/show after a pump exits on GDK's own request 18. (Round 4: round 3 cited the survival for the port, but it had been measured only in a plain GTK 4.22.4 window. Now measured in the port too, by a temporary uncommitted probe in the M110 test's child on the VM: external destroy, pump 1000, then `maximize`, pump 500, `unmaximize`, pump 500 survived 3 of 3 runs; `set_visible(false)`/`set_visible(true)` exited 1 on `request_code 18` in 2 of 2.) Both reproduce in a plain GTK window with no app X calls, so they are GTK's (UD56), not this entry's, and **the port's maximize and hide/show paths remain exposed**: the trap here covers only the winproto's three calls.
  5. The external destroy, pump, then a CONFIG RELOAD delivered to the window (`notify::config` → `syncAppearance`): EXITS 1, `request_code 20`. This is the realistic trigger: another client on the display closes the window, and the user later reloads the config.
  6. The same, but the reload turns blur OFF (found by the fix-round-3 reviewer): EXITS 1 on `request_code 19` (`DeleteProperty`, `syncBlur`). This is the case where a failed write is invisible to the status check, because `XDeleteProperty` returned 1.
- port handling:   `crates/rustty-apprt/src/gtk/winproto/x11.rs` `Window::trap_x_errors` (R13 PORT NOTE) brackets each of the three calls with `gdk_x11_display_error_trap_push`/`_pop` (gdk4-x11's `X11Display::error_trap_push`/`error_trap_pop`, GDK's own scoped trap, never a process-wide `XSetErrorHandler`), held by an `XErrorTrap` drop guard that pops with `error_trap_pop_ignored` if the call panics. A trapped error takes the reference's `error.RequestFailed` path, which `resizeEvent`/`syncAppearance` already log as `failed to sync blur=`/`failed to sync decorations=`. The dead `status == 0` checks stay, faithfully (S60-F1). The port does NOT also check `gdk_surface_is_destroyed` before the calls (decided in round 4, stated in the PORT NOTE). First, the trap already covers every case that check would. Second, under R13 item 1 the trap takes the failure path the reference's own code implies (`error.RequestFailed`, then `resizeEvent`/`syncAppearance` log `failed to sync …`), whereas an early `Ok` on a destroyed window would invent a silent skip. (On a destroyed window the reference never logs those errors: its first request exits the process in GDK's error handler, scenarios 2, 3, 5 and 6. Round 5 corrected round 4's claim that the check "would silence the reference's `failed to sync` errors".) A secondary, test-side reason: the check would keep the M110 test's reloads from reaching the trap.
- parity row(s):   S60-F1, S60-F6; inventory item M110.
- test:            `x11_error_on_a_destroyed_window_does_not_exit_the_process` runs, in a child copy of the test binary, the external destroy + pump followed by (a) a blur-OFF reload, which forces `XDeleteProperty`, and (b) a blur-ON reload with the cached region replaced by a sentinel, which forces `XChangeProperty`; the child asserts the sentinel differs from the blur-on region. For each reload the child reports (i) how many `failed to sync blur=RequestFailed` records (`gtk_x11`, error, captured through the log facade) were logged inside the reload's `set_property` call, and (ii) whether the cached region survived unchanged. Inside the same call the child also asserts that the record count equals the change in a test-only thread-local counter (`TRAPPED_WRITES`, `#[cfg(test)]`, incremented in `change_property`/`delete_property` when the trap reports an error), so each record is shown to come from a property write that was issued and that the trap reported failed; a `RequestFailed` returned without a request leaves the counter unchanged and fails the assertion. The kept cache shows the failure took the commit-on-success path. The parent asserts exit 0 and `delete: failed_writes_logged=1 cache_kept=true; change: failed_writes_logged=1 cache_kept=true`. On the VM under Xwayland it passes (round 4, at the committed code: 20 of 20 runs under one sway, 12 of 12 with a fresh sway each, then 3 of 3 through `cargo test`; round 5, with the trap counter: 6 of 6 through `cargo test`, a fresh sway each); with no display it FAILS. The counter cannot reach a production build: with `#[cfg(test)]` removed from only the increment in `change_property`, a non-test `cargo check -p rustty-apprt --features gtk,x11,wayland` fails with E0425 (`TRAPPED_WRITES` not found). (Round 5: round 4 said one record per reload "shows that the write was attempted"; its reviewer kept the round-4 test GREEN with both write traps removed and `syncBlur` returning `Err(RequestFailed)` on `is_destroyed()`, which logs the same record with no request made. The trap counter closes that. Round 4: rounds 2–3 said survival of the cache "can only come from the trap result". The round-3 reviewer kept that test GREEN with both write traps removed, because the cache also survives when no write is made: `syncAppearance` skipping `syncBlur`, `syncBlur` returning early on `is_destroyed()`, or a sentinel equal to the real region. Round 3 had itself corrected an earlier version that asserted a `failed to sync` line which came from `XGetWindowProperty`'s own reply code.) Mutations, VM 2026-09-22, one run each, each reverted before the next:
  - (round 5) both write traps removed + `syncBlur` returns `Err(Error::RequestFailed)` when `x11_surface.is_destroyed()`: RED, the child exits 101 on the trap-counter assertion (`left: 0, right: 1`: one record logged, no trapped write); GREEN under the round-4 test. The round-4 rows below were re-run at the round-5 code with the same results;
  - both write traps removed + `syncAppearance` without `syncBlur`: RED, `delete: failed_writes_logged=0 …; change: failed_writes_logged=0 …`;
  - both write traps removed + `syncBlur` returns early when `x11_surface.is_destroyed()`: RED, the same (so `is_destroyed()` is `true` after the pump);
  - sentinel = `before`: with both write traps removed, RED because reload (a)'s untrapped delete exits 1 on `request_code 19` (2 runs; the first exited earlier, on GDK's own `XIQueryPointer`, see UD56); with only the change trap removed, RED at the sentinel `assert_ne`; with the change trap removed and that assertion also removed, RED on `change: failed_writes_logged=0`; with both traps kept and the assertion removed, RED the same way;
  - both write traps removed, no other change: child exits 1, `request_code 19`, RED;
  - the trap removed from `XChangeProperty` only: exit 1, `request_code 18`; from `XDeleteProperty` only: exit 1, `request_code 19`; both RED;
  - `trap_x_errors` pops with `error_trap_pop_ignored` and reports `false`: the child survives but reports `failed_writes_logged=0 cache_kept=false` for both reloads, RED.
  `sync_blur_keeps_the_cached_region_when_the_property_write_fails` still covers the status path with a stub `XDeleteProperty`.
- proposed fix:    Wrap the property calls in `gdk_x11_display_error_trap_push (display)` / `if (gdk_x11_display_error_trap_pop (display) != 0) return error.RequestFailed;` (zig-gobject `gdkx114.zig:205-206` `errorTrapPop`, `:212-213` `errorTrapPopIgnored`, `:220-221` `errorTrapPush`, on the window's display). Replace the `status == 0` checks, which cannot fire, with that trap result, so `syncBlur`'s commit-on-success works as intended. Note that these functions are DEPRECATED since GTK 4.18, along with GDK's X11 backend API as a whole. A partial, non-deprecated mitigation exists: `gdk_surface_is_destroyed` (`GDK_AVAILABLE_IN_ALL`) reads `true` once GDK has processed the DestroyNotify (the round-4 `is_destroyed()` mutation above proves it for the pumped reload), so returning early on it would avoid scenarios 5 and 6 without a trap. It cannot cover a destroy GDK has not processed yet (scenario 2) or one that lands between the check and the request, so the trap is the complete fix. (Round 4: this said GDK 4 offers no non-deprecated way and that the trap was "the only fix available".) This fixes the winproto's own requests only; GDK's own requests against an externally destroyed window are UD56.
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
- port handling:   faithful (R1, a non-crash defect). `crates/rustty-apprt/src/gtk/portal/open_uri.rs` `response_received` makes the same `g_variant_get` call and reads code 0, which also removed the port's former PANIC on `()` (S59-F3). Controller ruling (task S-6b, under R13 as extended 2026-09-23): stays faithful, because reaching it needs a hostile session-bus peer and choosing the fallback for a malformed body would be new behaviour.
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
- port handling:   no check, as ReleaseFast — `crates/rustty-vt/src/tmux/viewer.rs:1787` (PORT NOTE). Under R13 as extended 2026-09-23 the remaining history damage (the port writes it as Ghostty's release build does) is a fix candidate: fix assigned to S-12 (controller ruling in task S-6b).
- parity row(s):   NEW-FINDINGS N43 (no LEDGER row).
- port handling (task S-12): fixed (R13, as extended 2026-09-23), at the cause. `received_pane_state` skips a pane whose `capture-pane` commands are still queued and leaves it to its own `list-panes` reply, which `sync_layouts` always queues after that pane's captures: the order the reference means (its `syncLayouts` comment "If we added any panes, then we also want to resync the pane state", and the state diagram at the top of `viewer.zig`, where the captures run before `pane_state`). The captures are then written into the pane in its initial state, so the history reaches the scrollback in order and the active area is empty afterwards (the condition the reference asserts), and the visible capture is not scrolled inside the early region. The early reply still applies to every pane whose captures are done, and nothing it carries is lost: the pane's own later reply carries the same fields. `crates/rustty-vt/src/tmux/viewer.rs:1519` (PORT NOTE); the no-check PORT NOTE in `received_pane_history` (`:1822`) now points to it. Resetting the scroll region around the history write was considered and not chosen: it leaves the cursor on the early reply's row (so, from reading, not run, 23 blank rows would go to the scrollback ahead of the history), does not cover the visible capture or the other modes the reply sets, and adds state save/restore the reference has no counterpart for. MEASURED damage without the fix (the port with the skip disabled, which is the reference's behaviour): a three-line history `h1`/`h2`/`h3` ends as `h1h2h3` on row 23 with nothing in the scrollback (a capture's bare `\n` keeps the column, UD272, so short lines run together on that row; the ReleaseFast line's "overwrites" holds once a line wraps), and a 12-line visible capture loses its first line (`v0` scrolls out of rows 0-10). NEW-FINDINGS N251; listed in `docs/ghostty-bug-fixes.md`.
- test:            `pane_state_for_a_pane_with_captures_queued_waits_for_its_own_reply` (replaces `pane_history_below_the_scroll_region_does_not_trip_the_empty_active_area_check`; bytes through the VT stream: list-windows with pane 7, a `%layout-change` adding pane 8, pane 7's captures, a `list-panes` giving pane 7 cursor row 5 and region 2-20 and pane 8 cursor row 23 and region 0-10, pane 8's three-line history and 12-line visible capture, its alternate captures, then pane 8's own `list-panes`). Asserts: pane 7's state applied from the early reply; the history lines in the screen in order with the active area empty; the visible capture on rows 0-11; after pane 8's own reply its region is 0-10 and its primary cursor row 23; the viewer never defunct. Negative controls (macOS, task S-12, each run alone): skip disabled, fails at "the history reached the scrollback in order" with `right: ["", …(23 blank rows)…, "h1h2h3"]`; skip disabled and the history assertions removed, fails at "the visible capture is on rows 0-11, unscrolled" with `right: ["v1", …, "v11"]`. Negative control of the former test, macOS 2026-09-11: with the port's former `debug_assertions` check the test panicked in `inlineAssert` from `received_pane_history` (backtrace confirmed).
- proposed fix:    Drop the assert, or make the intent true: reset the scroll region (and origin mode) before the `index()` loop, or clear the active area with `eraseDisplay(.complete, false)` instead of scrolling it. Better, at the cause (what the port does): in `receivedPaneState`, skip a pane that still has a `pane_history`/`pane_visible` command queued; its own `pane_state` follows its captures.
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
- kind:            non-crash (evident intent broken): lost tmux session
- upstream project: ghostty
- severity:        low
- reference:       src/terminal/tmux/viewer.zig:1317 and :1331 (`capture-pane -p -e …`, the pane history and visible captures), src/terminal/parse_table.zig:71 (the "anywhere" `0x1B => escape` transition, which `dcs_passthrough` does not override), src/terminal/Parser.zig:280 (leaving `dcs_passthrough` emits `dcs_unhook`), at 20abdb50a
- defect:          Ghostty receives tmux control mode inside the VT parser's DCS state. The viewer asks for pane contents with `capture-pane -p -e`, and `-e` includes SGR escape sequences. tmux writes that reply to a control client raw (tmux `cmd-capture-pane.c:447-448` at tmux `b7c5030004`, `:446-447` on later master; confirmed from tmux's source by the task R13-A coordinator). The first ESC in the reply moves the parser from `dcs_passthrough` to `escape`, which unhooks the DCS: the tmux handler emits `.exit` and the viewer is freed. The rest of the tmux output is then parsed as ordinary terminal output in the surface.
- ReleaseSafe:     no crash; the tmux session ends as soon as any captured pane has coloured content, and later control-mode lines are processed as normal output.
- ReleaseFast:     same.
- reachability:    ordinary use: `tmux -CC attach` to a session with any coloured text in a pane's history or visible area.
- port handling:   fixed (R13, task S-12; see the `port handling (task S-12)` line below). Before S-12: faithful (R13 non-crash defect): the port's parser and DCS handler behave the same. Not changed; the right handling (for example a line-based control-mode reader outside the VT parser, or dropping `-e`) is a design question for upstream. Re-decided under R13 as extended 2026-09-23: fix assigned to S-12 (controller ruling in task S-6b).
- parity row(s):   none.
- port handling (task S-12): fixed (R13, as extended 2026-09-23: the reference means the `-e` escapes to reach the pane, "-e = output escape sequences for SGR", and `receivedPaneHistory` sends the reply "as-is" into the pane's VT stream; its control parser also calls block payload "raw data"). The viewer adds `-C` to both capture commands (`crates/rustty-vt/src/tmux/viewer.rs:2209`, `:2232`) and decodes the reply with `unescape_capture` (`:2438`, PORT NOTE; decode sites `:1791`, `:1887`) before writing it into the pane. tmux (`capture-pane -C`, tmux 1.8 onward: the release that added control mode; `grid.c` at master `72e83b00`) then writes every ESC of an SGR or OSC 8 sequence as the four characters `\033`, SO/SI as `\016`/`\017`, and a one-byte `\` cell as `\\`, so the reply holds no ESC and the DCS stays hooked; the decoder restores exactly the `-e` bytes for the grid text, and for hyperlink strings the application's own bytes (below). No parser or DCS change: an ESC still ends the DCS everywhere, so a tmux that dies mid-reply cannot leave the terminal swallowing output (which a "keep ESC inside a `%begin` block" parser rule would risk, up to the control parser's 1 MiB limit), the parser's hot path is untouched, and no handler API is added. The upstream proposal's redesign (a line reader outside the VT parser) is not needed for this defect. No residual: tmux stores hyperlink URIs and ids vis-encoded (`hyperlinks.c:148`, `:154`, `utf8_stravis(..., VIS_OCTAL|VIS_CSTYLE)`, tmux 3.4 onward), so a `\` in them is already `\\` and a non-printable byte `\ooo`. The decoder therefore yields the application's original URI and id, where `-e` alone would have given the pane the vis form. Read from tmux's source, not run. (Review of task S-12, F1: this sentence first claimed a misdecoding residual.) NEW-FINDINGS N250; listed in `docs/ghostty-bug-fixes.md`. Verified against synthetic control-mode transcripts built from tmux's source (no tmux on the host), not a live tmux.
- test:            `coloured_pane_captures_keep_the_control_session_and_reach_the_pane` (bytes through the VT stream, DCS handler, control parser and viewer; each capture reply is built by `test_host::capture_reply` from the command the viewer actually sent, raw without `-C` and escaped with it, as `cmd-capture-pane.c` and `grid.c` do): bold red history text, a `\` and a `\033`-lookalike in the text, a green visible capture with an OSC 8 hyperlink (ST `ESC \`), then `list-panes` and a later `%output`. Asserts the session survives every capture, the later `%output` reaches the pane, the text and backslashes round-trip, the styles match what the raw `-e` bytes give a fresh terminal (VT formatter), and the hyperlink is on the cell (HTML formatter). `unescape_capture_inverts_tmux_capture_escaping` covers the decoder. Negative controls (macOS, task S-12, each run alone): `-C` removed from the commands, fails at capture 0 with "the control session ended" and notifications ending `"exit"`; `-C` kept and the decode skipped, fails on the text with `\033[1m\033[31mred…` printed literally.
- proposed fix:    Consume tmux control mode outside the VT parser once `ESC P 1000 p` is seen (reading raw lines until `%exit` and ST, as other tmux control-mode clients do), or request captures without `-e` and apply styles another way. Smaller (what the port does): add `-C` to both `capture-pane` commands and decode `\ooo` and `\\` in `receivedPaneHistory`/`receivedPaneVisible` before `nextSlice`.
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
  - **MEASURED on the current two-stage pipeline (task L-1, Linux VM rvm (Fedora 44 aarch64), 2026-09-26)**: a copy of the port's `live_shell` KAT whose child is `/bin/sh -c 'printf RUSTTY_OK'` (no trailing sleep), run 40 times per build: with the reference's decision (return on any `POLLHUP`) the output was lost in 23 of 40 runs (the screen stayed empty); with the port's fix it arrived in 40 of 40. The loss at the pin is inferred from the identical decision, not measured in a Zig build. The code path above is confirmed by reading.
- ReleaseSafe:     no crash; the last output of a short-lived command can be missing from the terminal.
- ReleaseFast:     same.
- reachability:    ordinary use on Linux: a command that prints and exits immediately (`ghostty -e …`, or a shell's final output at exit), most visible with `wait-after-command`. Timing-dependent: only when the output lands while the reader is parked in the outer poll.
- port handling:   fixed (R13, extended 2026-09-23; task tail-s6; Linux end-to-end run done by task L-1, above): `crates/rustty-termio/src/exec.rs` `ReadThread::outer_poll_hup_exits` (PORT NOTE: reference defect (R13)), called by the gather stage's outer poll, returns only on a hangup without `POLLIN`, or on the second hangup after the loop went back to read once (a flag set the first time a hangup came with `POLLIN`). So bytes queued with the hangup are read, and the thread still ends: after the data a pipe or a macOS pty master keeps reporting `POLLIN | POLLHUP` while `read` returns 0 (MEASURED on macOS 2026-09-23 for both, `revents == 0x11`), and on Linux the read after the data fails with EIO, which the gather loop already treats as the end. A macOS pty master whose slave closes with bytes still queued returned 0 on read in the same probe, so the drain changes nothing there; the port's `live_shell` KAT keeps its trailing `sleep 1` (its comment now points here). The end-to-end Linux run is the 40-run measurement above (task L-1, Linux VM rvm (Fedora 44 aarch64), 2026-09-26). Before task tail-s6 the port was faithful ("PRESERVED REFERENCE QUIRK"). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   none (phase-2 DEFERRALS reclassified entry; `.port/phase-3/DEFERRAL_REAUDIT.md:255`); inventory M14.
- test:            `exec::tests::gather_stage_reads_output_that_arrives_with_the_hangup` (rustty-termio) runs the production `gatherMainPosix` on a non-blocking Unix socket pair holding `first`; a test-only hook (`#[cfg(test)]`, keyed on the fd) writes `second` and shuts down the writing side right before the outer poll (`shutdown(SHUT_RDWR)` since task L-1: Linux reports `POLLHUP` on a Unix stream socket only when both directions are shut, so with `SHUT_WR` the poll saw `POLLIN` alone and the test timed out on Linux; macOS reports `0x11` for either, measured), so that poll reports `POLLIN | POLLHUP` with `second` unread (MEASURED on macOS: `revents == 0x11`). A socket pair rather than a pipe, because the first version closed a pipe's write end and failed twice in nine runs of the whole suite: other tests in the binary fork children that briefly hold a copy of the write end, so the poll saw the data before the hangup (three outer polls instead of two). It asserts the published batches read `firstsecond` and that exactly two outer polls ran (one drained, the next ended the thread), with a 10 s timeout in case the thread never ends. `exec::tests::outer_poll_hangup_with_data_reads_once_more` pins the decision's truth table. Negative control (macOS, task tail-s6, run alone): with the reference's decision (return on any `POLLHUP`) the first test fails `the output that came with the hangup must be delivered` with `left: [102, 105, 114, 115, 116]` (`first` only).
- proposed fix:    On wake, if `POLLIN` is set, loop back into the gather stage before honouring `POLLHUP`; return only when `POLLHUP` is set and `POLLIN` is clear (or a read returns 0 or EIO), mirroring the bridge-poll arm.
- upstream status: not reported (the Linux measurement it waited for is done: task L-1; negative control on Linux, the reference's decision, fails the unit test with `left: [102, 105, 114, 115, 116]`, `first` only)

### UD20 — custom-shader uniforms `iSelectionForegroundColor` and `iSelectionBackgroundColor` are swapped
- kind:            latent (functional: wrong colours)
- upstream project: ghostty
- severity:        low
- reference:       src/renderer/shaders/shadertoy_prefix.glsl:29-30 (`iSelectionForegroundColor` then `iSelectionBackgroundColor`) vs src/renderer/shadertoy.zig:40-41 (`selection_background_color` then `selection_foreground_color`); written at src/renderer/generic.zig:2275-2293 and uploaded as raw bytes by `state.uniforms.sync(&.{self.custom_shader_uniforms})` (:1892); documented at src/config/Config.zig:3163-3165, at 20abdb50a
- defect:          The `Globals` std140 block and the Zig `extern struct` are laid out member by member and uploaded byte for byte, so each GLSL name receives whatever Zig field sits at its offset. The last two members are declared in opposite orders: `iSelectionForegroundColor` occupies the slot Zig fills with `selection_background_color`, and `iSelectionBackgroundColor` occupies `selection_foreground_color`. A shader reading `iSelectionForegroundColor` gets the `selection-background` colour, and vice versa. The Metal path compiles the same GLSL prefix to MSL (shadertoy.zig:139,150) and the uniforms are written by the shared generic renderer, so both renderers are affected. Introduced with the uniforms in `8d2eb280d` ("custom shaders: add colorscheme information to shader uniforms"). By reading; not rendered.
- ReleaseSafe:     no crash; the two selection colours are exchanged in custom shaders. Visible whenever either option is set: a set colour appears under the other name, and the name the user expects reads the zero default (both slots start `@splat(0)`, generic.zig:776-777).
- ReleaseFast:     same.
- reachability:    config: `custom-shader` plus `selection-foreground` and/or `selection-background`, with a shader that reads either uniform.
- port handling:   fixed (R13, extended 2026-09-23; task tail-s6): the port declares `Uniforms.selection_foreground_color` before `selection_background_color`, the GLSL order, so each uniform receives the colour it is named for (`crates/rustty-renderer/src/shadertoy.rs`, PORT NOTE: reference defect (R13) at the two fields; the layout pins now read 4464 for the foreground and 4480 for the background). The GLSL prefix stays byte-identical to the reference (`cmp` against the pin), so user shaders, which bind by name, need no change. Both renderers share the struct. Before task tail-s6 the port was faithful (task R13-R's PORT NOTE). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   none; inventory M44.
- test:            `shadertoy::tests::globals_block_members_receive_the_uniform_fields_they_are_named_for` (rustty-renderer) compiles the prefix with the production `spirv_from_glsl` (glslang), reads every `Globals` member's `Offset` decoration from the SPIR-V, and asserts that each of the 27 members sits at the offset of the `Uniforms` field of the same name. Negative control (macOS, task tail-s6, run alone): with the reference's field order it fails `GLSL member iSelectionForegroundColor sits at std140 offset 4464, but its Uniforms field is at 4480` (the 25 members before it match).
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
- port handling:   fixed (R13 as extended 2026-09-23: a non-crash defect that visibly breaks the reference's own evident intent, here an inverted condition contradicting its comment, is fixed in the port). `crates/rustty-crash/src/sentry.rs` `cacheDir` (PORT NOTE: reference defect (R13) at the `#[cfg(target_os = "macos")]` block) implements the comment: `XDG_CACHE_HOME` set and non-empty gives the XDG cache; unset or empty gives `NSCachesDirectory`. Listed in `docs/ghostty-bug-fixes.md`. History: phase 4 implemented this under escalation ruling P4-E35; parity tail T-E briefly ported the inversion (under R13 as it stood before the 2026-09-23 extension); parity tail T-E fix round (tail-e2) reverted to the intended behaviour.
- parity row(s):   LEDGER S79-F1 (the row this entry previously said did not exist); phase-4 P4-U1 / assessment P4-E35; inventory M65.
- test:            `cache_dir_follows_the_intended_condition` (`sentry.rs`, `--features sentry`, macOS). Negative control: reinstating the reference's condition (NSCaches when set and non-empty) fails the first assertion with the XDG and NSCaches paths swapped.
- proposed fix:    Restore the intended condition: `const xdg = environ_map.get("XDG_CACHE_HOME"); if (xdg == null or xdg.?.len == 0) return try internal_os.macos.cacheDir(alloc, "sentry");`, then fall through to `xdg.cache`.
- upstream status: not reported

### UD22 — `+list-colors --plain` (and piped) output is truncated: the buffered stdout writer is never flushed
- kind:            latent (functional: lost output)
- upstream project: ghostty
- severity:        low
- reference:       src/cli/list_colors.zig:53-74 (plain branch: `var buffer: [4096]u8`, `stdout.writer(global.io(), &buffer)`, prints in a loop, then `return 0` with no flush), at 20abdb50a
- defect:          The plain branch writes one line per X11 colour through a 4096-byte `std.Io.File.Writer` and never calls `flush()`. Bytes reach stdout only when the buffer fills, so whatever is still buffered at `return 0` (less than 4096 bytes) is discarded. Every other buffered CLI in `src/cli` flushes (`version.zig:78`, `list_keybinds.zig:85`, `list_fonts.zig:168`, `explain_config.zig:112`, `list_themes.zig:200`). The map comes from `src/terminal/res/rgb.txt` (782 lines) and each output line is at least 14 bytes, so the listing spans several buffers and its tail is lost. MEASURED (task T-C, Zig 0.16.0, a probe writing through the same 4096-byte `std.Io.File.Writer` and never flushing): 782 lines of 22 bytes deliver 16399 of 17204 bytes, and 10 lines deliver none. The reference binary itself was not run; the exact number of colour lines lost depends on the listing's byte count.
- ReleaseSafe:     no crash; the end of the colour list is missing.
- ReleaseFast:     same.
- reachability:    CLI: `ghostty +list-colors --plain`, or any `+list-colors` whose stdout is not a tty.
- port handling:   fixed (R13 as extended 2026-09-23: output silently lost because a buffer is never flushed visibly breaks the reference's evident intent). `crates/rustty-cli/src/list_colors.rs` `run_opts` flushes its `BufWriter` after the loop (PORT NOTE: reference defect (R13) at the plain branch), so the whole listing is printed.
- parity row(s):   S68-F3 (CLOSED by task T-C as this R13 fix); inventory M66.
- test:            `list_colors::r13_flush_tests::plain_listing_reaches_stdout_in_full` (rustty-cli; a re-executed child runs the plain branch with stdout on a pipe and the parent requires the whole listing). Negative control (macOS, task T-C): discarding the buffered bytes instead of flushing (`BufWriter::into_parts`, the reference's unflushed buffer) fails it with 8378 bytes of stdout against a 15434-byte listing.
- proposed fix:    Add `try writer.flush();` after the loop.
- upstream status: not reported

### UD23 — `+list-themes` never flushes its buffered stderr, so its two error messages are never shown
- kind:            latent (functional: lost diagnostics)
- upstream project: ghostty
- severity:        low
- reference:       src/cli/list_themes.zig:106-202 (`run`): buffered stderr at :124-126, "Could not find the Ghostty resources directory…" at :128-130, "No themes found…" plus `return 1` at :173-176; only `stdout.flush()` at :200, at 20abdb50a
- defect:          `run` creates a 4096-byte buffered stderr writer and prints to it in two places, but no path flushes it. Both messages are far shorter than the buffer, so neither reaches the terminal (MEASURED, task T-C, Zig 0.16.0: the "No themes found" line through an unflushed 4096-byte `std.Io.File.Writer` delivers 0 bytes; the reference binary itself was not run). In particular a user with no installed themes sees `+list-themes` exit with status 1 and no output. (`std.debug.print` at :148, used for directory-open errors, is unbuffered and unaffected.) Minor companion: the `arena` created at :116 is never deinited, which is harmless at process exit.
- ReleaseSafe:     no crash; the diagnostics are silently dropped.
- ReleaseFast:     same.
- reachability:    CLI: a broken install (no resources directory, or no themes).
- port handling:   fixed (R13 as extended 2026-09-23, as UD22). `crates/rustty-cli/src/list_themes.rs` `run_opts` flushes stderr before `return 1`, and the `BufWriter`'s drop flushes the resources-directory warning on the other paths (PORT NOTE: reference defect (R13) at both sites).
- parity row(s):   S69-F4 (CLOSED by task T-C as this R13 fix); inventory M67.
- test:            `list_themes::r13_flush_tests::no_themes_messages_reach_stderr` (rustty-cli; a re-executed child with an empty XDG config directory and no resources directory must exit 1 with both messages on stderr). Negative control (macOS, task T-C): discarding the buffered bytes instead of flushing (`BufWriter::into_parts`) fails it with an empty stderr ("the resources-directory warning is missing").
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
- port handling:   (1) fixed (R13, extended 2026-09-23; task tail-s6) at both copies: `.numpad_page_down` maps to `ImGuiKey_PageDown`, as `.page_down` does, in `crates/rustty-vt/src/input/key.rs` `Key::imgui_key` (the GTK inspector's mapping, `imgui` feature) and `crates/rustty-apprt/src/embedded.rs` `key_to_imgui_key` (the embedded/macOS inspector's), each with a PORT NOTE: reference defect (R13). Listed in `docs/ghostty-bug-fixes.md`. (2) fixed (R13; controller ruling in task S-6c, which accepted the review's challenge): `.numpad_begin` is unmapped (`None`) at both copies, the reference's own answer (its `=> null` arm) for keys imgui lacks; `ImGuiKey_NamedKey_BEGIN` is the start-of-range marker of imgui's named keys and equals `ImGuiKey_Tab`, so the reference sent keypad Begin (keypad 5, NumLock off) as Tab. PORT NOTE: reference defect (R13) at both arms. (3) stays faithful (prose). Before task tail-s6 all three were faithful. (Task S-6b first ruled (2) faithful; task S-6c reversed that.)
- parity row(s):   none; inventory M74.
- test:            `input::key::tests::imgui_key_numpad_page_down_is_page_down` (rustty-vt, `--features imgui`) and `embedded::imgui_key_tests::numpad_page_down_maps_to_page_down` (rustty-apprt): keypad Page Down maps to `ImGuiKey_PageDown` and to the same value as Page Down, and keypad Page Up still maps to `ImGuiKey_PageUp`. Negative control (macOS, task tail-s6, each run alone before the fix, i.e. with the reference's mapping): both fail `left: Some(517), right: Some(518)` (`ImGuiKey_PageUp` where `ImGuiKey_PageDown` is expected). (2): `input::key::tests::imgui_key_numpad_begin_is_unmapped` (`--features imgui`) and `embedded::imgui_key_tests::numpad_begin_is_unmapped`; negative control (macOS, task S-6c, each run alone with the reference's arm): both fail `left: Some(512), right: None` (512 is `ImGuiKey_Tab`).
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
- defect:          The guard has no upper compiler bound, but the attribute has been removed from newer compilers. **Measured (task R13-R, macOS host):** Homebrew's `x86_64-w64-mingw32-gcc` **16.2.0** rejects both `__attribute__((target("avx512f,evex512")))` and `target("avx512f,no-evex512")` with "attribute 'target' argument 'evex512' is unknown" (and `'no-evex512'`). Zig 0.16.0's bundled Clang (`zig cc -target x86_64-linux-gnu`) accepts both, so Ghostty's default build is unaffected today. highway master already bounds the guard (fetched 2026-09-11): `HWY_HAVE_EVEX512` is 1 only for `1400 <= GCC < 1600` or `1800 <= Clang < 2200`, citing llvm/llvm-project#157034 ("evex512 has been removed from clang 22"). The rustty build audit first hit this cross-building the byte-identical vendored header with mingw GCC 16 (`docs/superpowers/specs/2026-08-25-build-system-gap-audit.md:663-671`). **Upstream fixes (task R13-D, checked against google/highway on GitHub 2026-09-22):** `7cde540171` (2025-05-22, "Do not add evex512 to HWY_TARGET_STR with GCC 16 or later") bounds the GCC side and is in release **1.3.0**; the Clang bound (`HWY_COMPILER_CLANG < 2200`, citing llvm/llvm-project#157034) is `4201022df1` (2025-09-15), and `31ea1cfdf8` (2025-09-22) makes it overridable as `HWY_HAVE_EVEX512`; both are in **1.4.0**, not 1.3.0 (GitHub compare against the tags). Neither 1.3.0's nor 1.4.0's `set_macros-inl.h` contains a `no-evex512` string any more. **UNVERIFIED:** the first GCC release that rejects the attribute (only 16.2.0 was tested; highway's commit says "GCC 16 or later").
- ReleaseSafe:     build failure of highway's x86 AVX3/AVX10 targets on an affected compiler (GCC 16, or Clang 22 per the upstream comment).
- ReleaseFast:     same.
- reachability:    build: an x86 target compiled with GCC 16 (mingw, or a future Linux CI image), or with a Zig whose bundled Clang is 22 or later.
- port handling:   byte-faithful vendor: `crates-sys/highway-sys/vendor/hwy/ops/set_macros-inl.h`, identical to the pinned copy (`cmp`) and deliberately not patched (audit :669-671).
- parity row(s):   none; inventory M85.
- test:            n/a. Probe: two one-line C files with the `target` attributes above, compiled with mingw GCC 16.2.0 and `zig cc` (Zig 0.16.0).
- proposed fix:    bump Ghostty's highway pin (`pkg/highway/build.zig.zon`) to **1.4.0**, the first release with both the GCC 16 and the Clang 22 bounds (1.3.0 has only the GCC one). Required before Ghostty's Zig moves to Clang 22.
- upstream status: fixed in highway 1.4.0 (`7cde540171`, `4201022df1`, `31ea1cfdf8`); Ghostty's pin (1.2.0) does not have it

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
  - destroy, pump, then `set_visible(False)` / `set_visible(True)`: EXITS 1, `request_code 18` (GDK's own `ChangeProperty`). Before the X error, GLib and GDK print three criticals, in this order (added in round 4; re-measured 2026-09-22, 2 of 2 runs, in the plain window and, identically, in the port's window): `GLib-GObject-CRITICAL: invalid (NULL) pointer instance`, `GLib-GObject-CRITICAL: g_signal_handler_disconnect: assertion 'G_TYPE_CHECK_INSTANCE (instance)' failed`, `Gdk-CRITICAL: gdk_frame_clock_end_updating: assertion 'GDK_IS_FRAME_CLOCK (frame_clock)' failed`. (PyGObject prints the two GLib-GObject criticals as Python `Warning:` lines.) GTK is running its hide/show path on a surface without a frame clock, which suggests a GTK bug rather than an exit GTK intends;
  - destroy, pump, then `set_title` and a resize: survive.
  The reviewer additionally found `present`, `fullscreen` and `minimize` survive.
  The port shows the same: in the port's own window (a temporary probe in the M110 test's child, round 4), `maximize`/`unmaximize` after the pump survived 3 of 3 runs and hide/show exited 1 on `request_code 18` in 2 of 2.
  UNVERIFIED, seen once: in one of about 50 runs of the M110 test's child (round 4, a mutation run with both winproto write traps removed), the process exited 1 on `request_code 131` (XInputExtension) `minor_code 40` (`XIQueryPointer`, GDK's own request) within 0.7 s of starting, before the config reloads. Its timing relative to the external destroy is UNKNOWN: in that child the destroy follows the map wait and a 300-iteration pump, so the exit may have come before any window was destroyed, in which case it does not fit this entry's title and is filed here only provisionally (round 5). The same mutation re-run exited on the expected `request_code 19`, and 35 unmutated runs passed. Trigger unknown.
- port handling:   none possible in the port. The failing requests are GDK's, so the port's maximize and hide/show paths remain exposed. rustty traps only its own winproto property calls (UD3).
- parity row(s):   none; found while measuring M110 / UD3. (Round 4: moved here from "Candidates pending verification", where round 3 had filed it although it was already measured.)
- test:            none in the port; the reproduction is the plain-GTK script above.
- proposed fix:    Report to GTK. Either trap errors around GDK's own requests on a surface marked destroyed, or stop issuing requests once "unexpectedly destroyed" has been handled (for example, unrealize the `GtkWindow`). GTK may instead document an externally destroyed toplevel as fatal; the report should ask which.
- upstream status: not reported

### UD57 — custom-shader colour uniforms read `.color` on a `TerminalColor` that can be `cell-foreground` or `cell-background`
- kind:            crash (ReleaseFast: ub)
- upstream project: ghostty
- severity:        medium (a documented config value plus any custom shader)
- reference:       src/renderer/generic.zig:2266-2293 (`updateCustomShaderUniformsFromState`: `cursor_text.color.r/g/b` at :2268-2270, `selection_bg.color` at :2278-2280, `selection_fg.color` at :2288-2290); `TerminalColor` at src/config/Config.zig:5703-5706; the compatibility handlers `compatCursorInvertFgBg` (:4981) and `compatSelectionInvertFgBg` (:5008), which set the cell-relative tags; at 20abdb50a
- defect:          The uniform update reads the `color` payload of `cursor-text`, `selection-background` and `selection-foreground` without checking the tag. `cell-foreground` and `cell-background` are documented values of all three, and `cursor-invert-fg-bg = true` / `selection-invert-fg-bg = true` set them. The update runs only when a custom shader is loaded.
- ReleaseSafe:     panic "access of union field 'color' while field 'cell-foreground' is active"; the terminal process aborts when the uniforms are updated. Measured with a Zig 0.16.0 probe that mirrors the union and the read (Debug and ReleaseSafe, macOS arm64).
- ReleaseFast:     undefined behaviour. In the probe the read returned 1.0 for `r`, a value no config set, and did not crash.
- reachability:    config: `custom-shader = <any shader>` together with `cursor-text`, `selection-background` or `selection-foreground` set to `cell-foreground` or `cell-background`, or with either `*-invert-fg-bg = true` compatibility key.
- port handling:   each uniform is written only for a `.color` value; a cell-relative value leaves the uniform unchanged, which is what the reference does when the option is unset. The alternative, resolving the value the way the cell path does (generic.zig:2726-2750 for the cursor, :2995-2999 and :3039-3043 for the selection), needs a specific cell's style and inverse flag, and these uniforms are one colour for the whole surface; the reference's own `TerminalColor.toTerminalRGB` (Config.zig:5715-5720) returns null for the cell-relative tags. Sites: `crates/rustty-renderer/src/generic.rs:5688` (Metal) and `crates/rustty-renderer/src/generic/linux_impl.rs:4127` (OpenGL), PORT NOTEs.
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
- NaN:             a NaN content scale (an embedder's `ghostty_surface_set_content_scale` or `scale_factor`) does NOT trap at `@intFromFloat(@ceil(px / scale))`: it yields 0, so `recomputeInitialSize` sends 0 plus the padding. Measured by task R13-C with Zig 0.16.0: the `:1879-1886` expression (`final_width` at `:1879-1882`) gives `final_width = 8` for 800 px and 4 px padding a side, Debug, ReleaseSafe and ReleaseFast, on arm64 and on x86_64 (Rosetta 2). The port takes the skip path instead (`u32_from_ceil` returns `None`): the conversion has no `u32` result, which the language makes illegal behaviour that the safety check misses, and the 0 is how LLVM lowers a poison `fptoui` on these two targets, not a size. Test `initial_size_px_with_a_nan_content_scale_takes_the_skip_path` (rustty-core); negative control, macOS 2026-09-22: NaN mapped to `Some(0)` (the reference's observed value) fails with `Some((8, 488))` against `None`. Restored.
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
- port handling:   `crates/rustty-core/src/surface.rs` `isize_from_float` reports the values the reference traps on instead of converting them — and NaN, which has no `isize` either — and each caller takes the handling its own value means. `scroll_page_fractional` saturates (`isize_from_float_saturating`): the fraction means "scroll as far as it goes", and `scroll_viewport` clamps it to the top or the active area. The three gesture casts scroll nothing for such a value — the `.{}` arm the same function already takes for an offset too small to act on — and the guard runs BEFORE the pending-offset store, so a non-finite offset can never leave `pending_scroll_x/y` holding a NaN that poisons every later scroll, and before the `delta` assert, which holds for every value that gets past it. For NaN (re-checked by task R13-P2, since the handling had been argued from a NaN trap the reference does not have): `scroll_page_fractional` yields `0`, and the gestures scroll nothing. On the discrete x path that is exactly arm64's delta-0 result and is safer than x86_64's `isize::MIN`. On the y and precision-x paths the reference has no delta-0 result to match: a safe build fails `assert(@abs(amount) >= 1)` first, and a ReleaseFast build also latches the NaN into `pending_scroll_*`, because its store precedes the cast (corrected by task R13-C; this field first said "the delta-0 result arm64 produces" for all three). The port keeps that `debug_assert!`, but since task R13-C it runs AFTER the guard: a NaN from the C ABI failed it, and a `debug_assert!` panic inside the `extern "C"` `rustty_surface_mouse_scroll` aborts the process in debug and `release-safe` builds. Input from outside can violate it, so under R13 it is not an invariant ahead of the guard; after it, it holds for every value. The gesture blocks are `scroll_amount_y` / `scroll_amount_x`, lifted verbatim out of `scrollCallback` by task R13-P2 so they can be driven by a test.
- parity row(s):   no LEDGER row. Inventory: none (M27-M29 are other float casts). The two port tests that pinned the trap (`isize_from_float_nan_traps_in_checked_builds`, `isize_from_float_out_of_range_traps_in_checked_builds`) are REVERSED, as S114-F2's was.
- test:            `isize_from_float_reports_the_values_the_reference_traps_on`, `isize_from_float_reports_nan_whose_reference_value_is_target_dependent` and `isize_from_float_saturating_takes_the_end_in_each_direction` (rustty-core), plus `a_saturated_scroll_delta_is_clamped_by_the_viewport`, which drives the saturated delta into `Terminal::scroll_viewport` and shows it clamps without a panic and without walking one row at a time. Negative control, macOS 2026-09-11: with the range check removed (an unconditional `Some(v as isize)`), `isize_from_float_reports_the_values_the_reference_traps_on` fails. Restored. The gesture call sites (task R13-P2): `y_gesture_cast_out_of_range_scrolls_nothing`, `x_discrete_gesture_cast_out_of_range_scrolls_nothing` and `x_precision_gesture_cast_out_of_range_scrolls_nothing` drive `scroll_amount_y` / `scroll_amount_x` with mods decoded by `ScrollMods::from_int`, asserting the no-scroll arm, the untouched pending offset, and exact in-range deltas. Negative controls, macOS 2026-09-22, one site at a time: each cast made to saturate fails its own test (`ScrollAmount { delta: 0 }` expected, `9223372036854775807` got) and only that one; each cast made to trap (`.expect(..)`) fails its own test with that panic; all three saturating at once fails all three. NaN on the y and precision-x paths (task R13-C): both tests now drive `NaN` and `-NaN` in precision mode and assert the no-scroll arm with the pending offset kept; negative controls, macOS 2026-09-22, one path at a time: the `amount` assert moved back ahead of the guard fails its own test with "assertion failed: amount.abs() >= 1.0". The discrete y path is not driven with NaN: on Darwin `@min(nan, -1)` is -1 in both trees (measured, Zig 0.16.0), an ordinary tick. The call site (task R13-C): `mouse_scroll_routes_each_offset_to_its_own_axis` (rustty-apprt-c, `tests/macos_window_host.rs`) runs the REAL `scrollCallback` on a live surface through `rustty_surface_mouse_scroll`, with precision offsets below one cell so they only accumulate, and asserts `pending_scroll_x`/`pending_scroll_y` are `(3.0, 7.0)`; negative control: `yoff` passed to `scroll_amount_x` fails with `(3.0, 7.0)` against `(7.0, 7.0)`.
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
- ReleaseSafe:     panic "reached unreachable code" at the assertion, `PageList.zig:4566`, called from `screen.zig:384`; the embedding process aborts. MEASURED by the R13-S review (2026-09-22, Zig 0.16.0 probe on macOS arm64, decoding port-encoded snapshot bytes with the first PAGE record's `string_capacity_bytes` patched and its CRC left wrong; the unpatched bytes decode in both modes).
- ReleaseFast:     `assert` is `unreachable`, so this is undefined behaviour. MEASURED on 64-bit by the same probe: the process dies with SIGBUS. By reading, the mechanism is that an `alignedAlloc` above 4 GiB can succeed and every `getOffset` in `initBuf` then `@intCast`s to the `u32` `OffsetInt` (`size.zig:149-158`), truncating, so page offsets alias. On a 32-bit target (wasm32, which libghostty-vt builds for) `Page.layout`'s `usize` arithmetic (`page.zig:1757`) wraps BEFORE the assertion, so a small `total_size` passes it, the buffer is undersized, and `initBuf` and the grid decode write out of bounds; that 32-bit outcome is UNVERIFIED (not probed).
- reachability:    the libghostty-vt / rustty-vt C ABI snapshot decoder — `decoder_new`/`decoder_new_buf`, then `decoder_ready`/`decoder_next`/`decoder_decode` — i.e. an embedder decoding snapshot bytes that came from outside (a file, a peer process, the network). The decoder's own comments say that input is untrusted. NEITHER the Ghostty app nor the rustty app decodes snapshots itself, so the C ABI is the whole reachability.
- port handling:   `crates/rustty-vt/src/snapshot/page.rs` `check_native_page_size`, called from `Header::page_capacity`, rejects such a capacity with that function's OWN existing `CapacityError::InvalidDimensions`, which surfaces as the decoder's ordinary malformed-input error. One site covers both the SCREEN and HISTORY routes, because both take their capacity from `Decoder::capacity`. The shared assertion in `pagelist/mod.rs:2391` is deliberately LEFT AS IS: it is a true invariant for the native callers (R7), and what R13 forbids is letting decoded input reach it. On a 64-bit `usize` the check is the exact `Page::layout(cap).total_size` against `max_page_size`, which cannot overflow there (`page.zig:2708-2720`, "Page.layout can take a maxed capacity"), so it accepts every capacity native growth can produce: a terminal wide enough to leave the standard capacity (in Ghostty 47,932 columns or more in a shipped macOS arm64 build, 46,908 on Linux aarch64, 48,047 in a macOS test build, as measured for UD80; the port's thresholds were 48,036 and 48,151 on macOS, higher because its style-set item was smaller, and since NEW-FINDINGS N88 (task R13-L) they equal Ghostty's, re-measured by a sweep of the port's `Capacity::adjust`: 47,932 and 48,047 on macOS, 46,908 and 46,511 on Linux aarch64) gets `std_capacity` with only `cols` changed, and `increaseCapacity` can double its grapheme capacity to 512 MiB (a ~1.27 GiB layout) and 1 GiB (~2.46 GiB); `Header.init` copies such a capacity verbatim and the reference decoder accepts it. Only on a 32-bit `usize` (wasm32) does a conservative `u64` pre-gate bound the layout before the exact computation, so that computation cannot overflow; there it can also refuse a capacity whose exact layout would have fit, a deliberate over-rejection taken only where the exact check is not safe to run. (The first R13-S version applied the pre-gate on every target, which rejected those legal large capacities and left the exact check dead; corrected by task R13-S2.) The exact check's own boundary is tested by `decode_rejects_a_capacity_whose_exact_layout_is_one_past_the_limit` (`snapshot/page.rs`, task R13-W): a 48,151-column header whose `grapheme_bytes` is the smallest that passes the limit lays out at exactly 2^32 and is rejected, and the largest grapheme budget below it is accepted. That budget is found by a binary search over the platform's own `Page::layout`, because the layout is rounded to `page_size_min` (task R13-W2; the first version hard-coded macOS arm64's 1,382,748,161, which with Linux's 4 KiB pages lays out 12,288 bytes short of 2^32, so the test never reached the boundary there). The test asserts its own precondition, that the result lays out at exactly 2^32, so a platform where the construction is wrong fails loudly rather than passing vacuously; on macOS arm64 it also asserts 1,382,747,137, the reference's own value: a Zig probe of `Page.layout` at the pin (test build, 48,151 x 215, 128 styles) lays that budget out at exactly 2^32, and a budget one byte smaller (1,382,747,136) at 2^32 - 16,384, one 16 KiB page lower (task R13-L2). The test's first macOS constant, 1,382,748,161, was the port's before NEW-FINDINGS N88 laid the style set out at the reference's item geometry, which moved the grapheme section's start by 1,024 bytes; the rebase of N88 onto the platform-derived test turned it red, and the search now finds the reference's value. On Linux aarch64 (4 KiB pages) the search finds 1,382,759,425, which lays out at exactly 2^32. That is a real Linux run after N88 (the R13-L2 review, on `rvm` at `r13-l2` `0e805945`, the whole `rustty-vt` lib passing 3,089 / 0; NEW-FINDINGS N90, now closed), and it is the reference's own Linux value: the same Zig probe of `Page.layout`, cross-built at the pin for aarch64-linux and run on `rvm`, lays 1,382,759,425 out at exactly 2^32, and a budget one byte smaller (1,382,759,424) at 2^32 - 4,096, one 4 KiB page lower. (A first figure, before N88, came from an emulation that forced a 4 KiB `page_size_min` on macOS; it gave the same number.) Loosening the check to `> MAX_PAGE_SIZE + 1` turns it red (the 2^32 header is accepted, which would reach `pagelist/mod.rs:2391`) on macOS arm64 (16 KiB) and on Linux aarch64 (4 KiB, the same review run); `>=` for `>` is an equivalent change, since the layout is a multiple of the page size and `u32::MAX` is odd.
- parity row(s):   none (no LEDGER row; found by the R13-G part A sweep as A1).
- test:            `decoder_rejects_a_page_capacity_that_would_abort_the_library` (rustty-vt-c) drives the real C ABI: it encodes a live terminal, decodes those bytes unpatched as a control, then patches the first PAGE record's `string_capacity_bytes` to `0xFFFFFFFF` and asserts `decoder_decode` answers `invalid_value` and writes no terminal. `decode_rejects_a_capacity_the_native_page_allocation_cannot_hold` (rustty-vt) covers the three shapes at the payload level AND asserts positively that a capacity a page really can hold is NOT rejected (it proceeds to `EndOfStream` on the absent grid), so the test cannot pass by rejecting everything. `the_page_size_bound_is_an_upper_bound_on_the_real_layout` sweeps the extremes of all six capacity fields and pins the 32-bit pre-gate as a real upper bound on `Page::layout`. `decode_accepts_a_large_capacity_native_ghostty_can_produce` (rustty-vt) asserts that the 512 MiB and 1 GiB grapheme doublings of a 48,151-column page are accepted and that the 2 GiB doubling native growth refuses is rejected, each checked against the exact layout too. Negative controls, macOS 2026-09-11: with the check removed, the C-ABI test dies at `assertion failed: layout.total_size <= size::MAX_PAGE_SIZE` (`crates/rustty-vt/src/pagelist/mod.rs:2391`) — the ported assertion, reached exactly as predicted — and the payload-level test returns `EndOfStream` where `InvalidDimensions` was expected. 2026-09-22, one at a time: the pre-gate made unconditional again fails the large-capacity test (the 512 MiB header returns `Err(InvalidDimensions)` where `Ok` was expected; the test was also red before the fix); the exact check disabled fails it on the 2 GiB header, which is then accepted. All restored.
- proposed fix:    in `pageCapacity`, after the zero check, `if (Page.layout(cap).total_size > size.max_page_size) return error.InvalidDimensions;`, with the product computed through `std.math.mul`/`add` first so the bound also holds on 32-bit targets.
- upstream status: not reported

### UD65 — more than 65534 cells sharing one style in a decoded PAGE overflow the `u16` style refcount
- kind:            crash (ReleaseFast: refcount corruption)
- upstream project: ghostty
- severity:        medium (same reachability as UD64)
- reference:       `src/terminal/snapshot/grid.zig:1262` (`applyCell`'s `page.styles.use(page.memory, wire.style_id)`) → `src/terminal/ref_counted_set.zig:365-379` (`use`, an unguarded `item.meta.ref += 1`), where `ref: RefCountInt` is `size.CellCountInt` = `u16` for the style set (`src/terminal/style.zig:681-684`, `src/terminal/size.zig:22`); at 20abdb50a
- defect:          `applyCell` takes one reference per styled cell and nothing bounds the count. A snapshot PAGE declares its own `columns` x `rows`, so a 256 x 256 page — about half a megabyte — holds 65,536 cells, and if they share one style the 65,535th `use` overflows. Duplicate style-table entries with the same value each take a reference too, which only lowers the number of cells needed. (The first version of this entry said native pages cannot reach the ceiling. That is FALSE: a terminal wide enough to leave the standard capacity — in Ghostty 47,932 columns or more in a shipped macOS arm64 build, 46,908 on Linux aarch64, 48,047 in a macOS test build (the port's thresholds equal these since NEW-FINDINGS N88; see UD80) — gets `std_capacity` with only `cols` changed, 215 rows x cols, over ten million cells, and ordinary styled output overflows the same refcount through `printSliceFill` and `printCell`. That native defect is UD80. This entry is the decoder's own route to it.)
- ReleaseSafe:     panic "integer overflow" at `ref_counted_set.zig:378`, called from `grid.zig:1262`; the process aborts. MEASURED by the R13-S review (2026-09-22, Zig 0.16.0 probe on macOS arm64, decoding port-encoded bytes); the equivalent port overflow was also measured (see the negative control).
- ReleaseFast:     `ref` wraps to zero at the 65,535th cell, so a live style reads as dead while cells still reference it. The very next `use` (the following cell) then violates `use`'s own `assert(item.meta.ref > 0)` (`ref_counted_set.zig:376`), which is undefined behaviour, before the post-grid release loop (`snapshot/page.zig:457-461`) ever runs; that loop and `addContext`'s dead-item trimming (`ref_counted_set.zig:246-249`) then work from the corrupt count. MEASURED by the same probe: the decode returns success with `styles.count() == 0` and refcount 0 while the first and last cells both carry style 1. A memory-unsafe outcome is UNVERIFIED: the IDs themselves stay below `layout.cap`.
- reachability:    as UD64 — the C ABI snapshot decoder, on a page the sender describes.
- port handling:   `crates/rustty-vt/src/snapshot/grid.rs` `apply_cell` asks `ref_count_saturated` before taking a reference and, at the ceiling, stores the cell with the DEFAULT style instead. This is the degrade the same function already applies to a style ID it cannot map, and the one the hyperlink half applies when `set_hyperlink` fails, so it invents no new behaviour and costs only that cell's style; the reference decoder itself degrades the same way when a style cannot be installed (`applyCell`, `grid.zig:1259-1276`). It was preferred to rejecting the page outright, which would refuse pages that decode correctly today, including snapshots of the wide terminals UD80 describes. The native write path's own answer to a style it cannot store is the same family: `Screen.manualStyleUpdate` (`src/terminal/Screen.zig:2499-2514`) treats a full style set as an out-of-capacity condition, growing via `increaseCapacity` and splitting the page on `OutOfSpace` — neither of which a decoder filling a fixed, sender-declared page can do. LIMIT: the decoder fix is sound, but the page it produces inherits the native defect. A degraded 256 x 256 page is left with the style at refcount 65,534 (the style table's own reference is released after the grid), so one cursor-style `add` of that style plus one styled print over a degraded cell overflows it through the native path — measured at the primitive level on the decoded page, 2026-09-22: 65,534, then 65,535 after one `use`, then "attempt to add with overflow" at `crates/rustty-vt/src/ref_counted_set.rs:110` on the next. That is UD80's exposure, fixed with it.
- parity row(s):   none (no LEDGER row; found by the R13-G part A sweep as A2).
- test:            `decode_degrades_cells_past_the_style_refcount_ceiling` (rustty-vt) hand-builds a 256 x 256 PAGE payload whose every cell carries one style ID, decodes it, and asserts POSITIVELY that the style was really installed and really applied (the set holds one style; the first cell carries it) and that the cells past the ceiling carry the default style, with the surviving refcount at its maximum. Negative control, macOS 2026-09-11: with the guard disabled the decode panics "attempt to add with overflow" at `crates/rustty-vt/src/ref_counted_set.rs:110`. Restored.
- proposed fix:    the same degrade in `applyCell` — `if (page.styles.refCount(page.memory, id) == std.math.maxInt(RefCountInt)) { store the cell with style 0; }` — or make `use` saturate and report. Rejecting a PAGE whose `columns * rows` exceeds `maxInt(CellCountInt)` in `pageCapacity` would also refuse snapshots of wide native terminals, so it is not proposed. Fixing UD80 at its root (a refcount wide enough for a page's cell capacity, or a width limit) would cover this route too.
- upstream status: not reported

### UD66 — TERMINAL strings and the cursor hyperlink allocate a `u32` length the record cannot contain
- kind:            latent (resource)
- upstream project: ghostty
- severity:        low (same reachability as UD64)
- reference:       `src/terminal/snapshot/terminal.zig:887-895` (pwd and title) and `src/terminal/snapshot/hyperlink.zig:121-158`, the standalone decoder reached from `src/terminal/snapshot/screen.zig:345` via `decodeCursorHyperlink` (`:1117-1142`); at 20abdb50a
- defect:          each length is read from the payload and allocated before it is compared with the bytes left in the record, although the record reader always knows that bound (`payload.bufferedLen()` when borrowed, `limited.remaining` when streaming). A sub-kilobyte record therefore makes the decoder request a 4 GiB heap block per string. On a memory-constrained or no-overcommit host the request fails and TERMINAL decode reports `out_of_memory` instead of malformed input; for the cursor hyperlink `decodeCursorHyperlink` swallows the failure as "no cursor hyperlink", so the misreport is silent.
- ReleaseSafe:     no crash. MEASURED by the R13-S review (2026-09-22, Zig 0.16.0 probe on macOS arm64, a TERMINAL record with `pwd_len = 0xFFFFFFFF`): the decode returns `EndOfStream`, because macOS overcommits the 4 GiB reservation and the read then runs out of record; the honest control decodes. The misleading out-of-memory result is by reading, and applies only on a host that refuses the reservation (memory-constrained or no overcommit).
- ReleaseFast:     the same; MEASURED by the same probe: `EndOfStream`.
- reachability:    as UD64 — the C ABI snapshot decoder.
- port handling:   a `payload_remaining()` accessor on the snapshot reader trait projects the two bounds the reference reaches directly (only `record::Reader` implements it; a callback source honestly answers "unknown" and is not checked). `crates/rustty-vt/src/snapshot/terminal.rs` (`read_string`) and `crates/rustty-vt/src/snapshot/hyperlink.rs` (`check_declared_len`) reject a declared length greater than the record's remaining payload with `EndOfStream` — the error the read itself would return a moment later, so callers that already swallow it, such as `decodeCursorHyperlink`, keep their behaviour exactly.
- parity row(s):   NEW-FINDINGS N54, which is the port-only half (the zero-fill).
- test:            `terminal_decode_rejects_a_string_length_the_record_cannot_contain` (rustty-vt) and `decode_rejects_a_string_length_the_record_cannot_contain` (rustty-vt, hyperlink), each framing a real record so the bound exists, each asserting the malformed-input error AND — via the allocation tripwire armed on the string allocation — that the allocation was NEVER REACHED. The error alone is not sufficient proof: the unbounded code also ends at `EndOfStream`, just 4 GiB later. Both tests also decode a truthful record, so the bound cannot be rejecting everything. Negative controls, macOS 2026-09-11, one at a time: with either bound removed the test fails with `Err(OutOfMemory)` where `Err(EndOfStream)` was expected, i.e. the allocation was attempted. Restored. Strengthened by task R13-S2 after the R13-S review found the streaming route (`decoder_new`) unproven: every case now runs both borrowed and streaming (a 7-byte `LimitedReader`), always with a second record behind the first; the hyperlink test covers all three lengths (implicit URI, explicit ID, explicit URI) and the TERMINAL test both strings; each has a one-byte-over boundary case, an honest record whose last string's length equals the bytes left, and an assertion of the tripwire's REACH count (so a lie cannot pass by failing before its allocation). `callback_decoder_restores_strings_whatever_the_read_size` (rustty-vt-c) decodes an honest 300-byte pwd and a title through the real `decoder_new` callback at 1- to 8-byte reads. Mutations, macOS 2026-09-22, one at a time, all RED: the streaming arm dropping the already-buffered bytes (the honest streaming decodes fail with `EndOfStream`; in rustty-vt-c the 3-byte-read decode answers `invalid_value`); the streaming arm answering `None` (`Err(OutOfMemory)` on the streaming lies); the explicit `id_len` check removed; either bound loosened to `> remaining + 1`; the borrowed arm bounded by the whole source buffer instead of the record; either bound removed; `>` changed to `>=`. All restored.
- proposed fix:    compare the declared length with the record's remaining payload before allocating and return `error.EndOfStream`, which the code already returns a moment later.
- upstream status: not reported

### UD67 — a child that exits before its kqueue watch is registered hits `catch unreachable` in `processExit`
- kind:            crash (safety-checked trap) / undefined behaviour
- upstream project: ghostty (the error is libxev's documented `ProcError.NoSuchProcess`; the defect is Ghostty not handling it)
- severity:        medium (kernel timing on ordinary use; kqueue platforms only)
- reference:       `src/termio/Exec.zig:296-305` (`processExit`: `const exit_code = r catch unreachable;` at `:302`), with the watch started at `:159-165`; libxev `src/backend/kqueue.zig:157-168` (`add` only queues the completion; the kevent change is submitted on the next tick), `:1389-1393` (the proc op maps `ESRCH` to `ProcError.NoSuchProcess`), at 20abdb50a
- defect:          `threadEnter` spawns the child and only queues its `EVFILT_PROC` registration; the kevent change is submitted on the io loop's next tick. If the child has exited by then it is a zombie, and XNU refuses `EV_ADD` for a zombie with `EV_ERROR`/`ESRCH`. libxev reports that as `error.NoSuchProcess`, which `processExit` declares unreachable.
- ReleaseSafe:     panic "attempt to unwrap error: NoSuchProcess" on the io thread; the app aborts. By reading of `catch unreachable` (no Zig probe was built); the kernel half was measured, see reachability.
- ReleaseFast:     undefined behaviour (`unreachable` on the error arm); the exit is never reported as intended.
- reachability:    kernel timing on ordinary use: any child that exits before the io thread's first tick after spawn (a command that fails at once, a very short `command`). MEASURED on macOS 26.5.1 (arm64) with a C probe: `EV_ADD` of `EVFILT_PROC` with `NOTE_EXIT | NOTE_EXITSTATUS` for an exited, unreaped child returns `EV_ERROR` with `data = 3` (`ESRCH`), with or without `EV_RECEIPT`; registering straight after `fork` of a child that calls `_exit(0)` hit it 2 times in 200. On macOS Ghostty's child is `login(1)`, which narrows the window; FreeBSD (also kqueue) has no such wrapper. Not reachable on Linux (the pidfd backends report exit through `waitid`).
- port handling:   `crates/rustty-termio/src/exec.rs` `processExit` takes the reference's own exit path, `processExitCommon`, for `NoSuchProcess`, with exit code 0 — the value libxev's kqueue backend itself reports when it has no decodable exit status (`kqueue.zig:1270-1281` returns 0 unless `WIFEXITED`). The child has exited, so the surface gets its `child_exited` exactly as it would have. On macOS the exit code never decides abnormal-exit detection (`Surface.zig:1256-1261` skips it on Darwin, and `login(1)` exits 0 anyway). FreeBSD is different (REASONED, not measured: no FreeBSD host; added by task R13-C2): the same arm compiles there, the child is not wrapped in `login(1)`, and the non-Darwin branch treats exit code 0 as a good exit (`if (info.exit_code == 0) break :runtime;`, `Surface.zig:1258-1261`). A command that fails at once is exactly the child that can exit before the watch registers, so on FreeBSD such a failure reports 0 and loses its abnormal-exit message (`show_child_exited`), where the reference would have aborted. Recovering the real status needs the pid in `processExit` (`waitpid(pid, WNOHANG)`, or libxev's proc op reaping on `ESRCH`), which the port does not have without a `rustty-xev` accessor. The other wait errors keep the reference's `unreachable`: the completion is never canceled and the proc op always delivers a kevent. PORT NOTE at the site.
- parity row(s):   NEW-FINDINGS N55. Inventory M7.
- test:            `process_exit_reports_a_child_that_exited_before_the_watch_was_registered` (rustty-termio, macOS) drives the REAL path with no seam: a real `threadEnter` on a pty and a kqueue loop, a wait with `WNOWAIT` until the child is a zombie, a check on a private kqueue that the kernel refuses its registration with `ESRCH`, then the loop's first tick. It asserts that `processExitCommon` ran (`exited`) and that exactly one `child_exited` was pushed. Negative controls, macOS 2026-09-22, one at a time: the original `r.unwrap()` restored fails with "called `Result::unwrap()` on an `Err` value: NoSuchProcess" at the cast site; the `NoSuchProcess` arm replaced by `unreachable!()` fails at that arm. Restored.
- proposed fix:    in `processExit`, handle `error.NoSuchProcess` by calling `processExitCommon(td, 0)` (or reap with `waitpid` to recover the status); alternatively libxev's kqueue proc op could `waitpid(WNOHANG)` on `ESRCH` and deliver the status.
- upstream status: not reported

### UD68 — every inherited tab, split or window leaks the working-directory string `newSurfaceOptions` hands the host
- kind:            leak
- upstream project: ghostty
- severity:        low (a path-sized allocation per inherited new surface)
- reference:       `src/apprt/embedded.zig:1160-1178` (`newSurfaceOptions`: `break :wd self.app.core_app.alloc.dupeZ(u8, cwd) catch null;` at `:1171`), returned by `ghostty_surface_inherited_config`; consumer `macos/Sources/Ghostty/Surface View/SurfaceView.swift:606-607` (`String.init(cString: workingDirectory, encoding: .utf8)`), called from `macos/Sources/Ghostty/Ghostty.App.swift:937`, `:973`, `:1001`; at 20abdb50a
- defect:          `newSurfaceOptions` allocates a NUL-terminated copy of the pwd on the core app's GPA and returns it inside the by-value `ghostty_surface_config_s`. The C ABI has no function to free it, the macOS host only copies it into a Swift `String`, and nothing on the Zig side keeps the pointer, so the allocation is never freed.
- ReleaseSafe:     a leak (the GPA reports it at exit in a Debug build).
- ReleaseFast:     a leak.
- reachability:    ordinary use: every new tab, split or window that inherits the working directory (`window-inherit-working-directory`, default true) from a surface whose shell reported a pwd.
- port handling:   `crates/rustty-apprt/src/embedded.rs` keeps the C ABI unchanged (the same `const char*` in the same by-value struct; `check_embed_h_abi.sh` and `check_rustty_h_abi.sh` PASS) but keeps ownership on the surface: `InheritedWorkingDirectory` holds the string, frees the previous one on each `newSurfaceOptions`, and frees the last in `Surface::deinit`. The pointer stays valid until the next call on the same surface or the surface's destruction; the host copies it at once. PORT NOTEs at the field, the call and the type.
- parity row(s):   NEW-FINDINGS N56. Inventory M15.
- test:            `the_inherited_working_directory_is_freed_by_the_next_call_and_by_deinit` (rustty-apprt) watches the allocation with a test-only forwarding `#[global_allocator]`: the string is readable and not freed until the next `hand_out`, is freed by it, is freed by a `hand_out(None)` too, and the last is freed by `clear`. The probe's own free detection is checked first. Negative controls, macOS 2026-09-22, one at a time: `hand_out` reinstating the reference's hand-over (`into_raw`, nothing kept) fails with "the next call frees the previous string"; `clear` forgetting instead of freeing fails with "deinit frees the last string". Restored. Task R13-C2 added `surface_free_frees_the_last_inherited_working_directory` (rustty-apprt-c, macOS integration test) through the REAL path: a live surface with a pwd, `rustty_surface_inherited_config` (→ `newSurfaceOptions` → `hand_out`), then `rustty_surface_free` (→ `Surface::deinit`). Negative controls, one at a time: removing the `clear()` call, and `hand_out` restored to `into_raw`, both fail with "Surface::deinit frees the last inherited working directory" (re-run by the R13-C2 re-review). It skips, with a message, on a host with no Metal device. The C header now states the pointer's lifetime (`rustty_surface_inherited_config`).
- proposed fix:    keep the string on the `Surface` and free it on the next call and in `deinit`, as the port does; or add a `ghostty_surface_config_free`-style function to the C ABI for the host to call after copying.
- upstream status: not reported

### UD69 — KeymapDarwin leaks the input source when `init` fails, and keeps a released one when `reload` fails
- kind:            leak + ub (use after release / double release)
- upstream project: ghostty
- severity:        low (the OS must return no current keyboard layout, or one without `uchr` data)
- reference:       `src/input/KeymapDarwin.zig:60-64` (`init`), `:66-68` (`deinit`), `:70-75` (`reload`), `:96-119` (`reinit`); caller `src/apprt/embedded.zig:231-235` (`reloadKeymap`) from `ghostty_app_keyboard_changed` (`:1749-1754`, which logs the error and keeps going); at 20abdb50a
- defect:          `reinit` stores the +1 `TISCopyCurrentKeyboardLayoutInputSource()` result into `self.source` and then returns `GetInputSourceFailed` if the source has no `kTISPropertyUnicodeKeyLayoutData`. (1) `init` then discards the keymap without releasing it: a leak. (2) `reload` releases `self.source` BEFORE calling `reinit`. If the copy returns null, `orelse return` leaves the released pointer in `self.source`; if the layout lookup fails, `self.source` is the new source but `unicode_layout` still points into the released one's `CFData`. Either way the keymap stays installed (`ghostty_app_keyboard_changed` only logs), so the next `keyboardLayout` (`sourceId`) or `translate` reads released memory, and `deinit` releases the stale source a second time.
- ReleaseSafe:     no safety check covers it: a leak in (1); in (2) use of a released CF object and a double `CFRelease`, which typically crashes in CoreFoundation later. By reading; not reproduced, since the OS answers cannot be forced.
- ReleaseFast:     the same.
- reachability:    the OS (HIToolbox): a keyboard-layout change (`ghostty_app_keyboard_changed`) at a moment `TISCopyCurrentKeyboardLayoutInputSource` returns null, or a current layout without `uchr` data (legacy resource-based layouts). Not measured on a real system; no such layout is installed here.
- port handling:   `crates/rustty-input/src/keymap_darwin.rs`: `reinit` copies into a local, releases it if the layout lookup fails, and commits `source` and `unicode_layout` together only on success; `reload` reinits first and releases the previous source only after the new one is committed. A failed reload therefore leaves the keymap on its previous, still-owned layout, and `deinit` releases exactly once. The error still propagates as in the reference. The port's previous code stored the null copy, so its `deinit` called `CFRelease(NULL)`, which crashes. PORT NOTEs at `reload` and `reinit`.
- parity row(s):   NEW-FINDINGS N57. Inventory M19 (records F13, LIFETIMES L14).
- test:            through a function-parameter seam (`reinit_with` / `reload_with`; production passes the real TIS calls, so nothing test-only is compiled in), with CFData objects standing in for sources so retain counts are exact: `a_source_without_layout_data_is_released_on_the_error_path`, `a_failed_reload_keeps_the_old_source_owned` (then `deinit` releases it exactly once), `a_reload_to_a_source_without_layout_data_keeps_the_old_layout`, and `a_successful_reload_replaces_the_source` through the real TIS calls (rustty-input, macOS). Each asserts the lookup or copy was reached. Negative controls, macOS 2026-09-22, one at a time: the error-arm release removed fails the two layout-less tests ("the +1 copy was released, not leaked" / "the new copy was released", count 2 against 1); the reference's release-before-reinit order restored fails the two failed-reload tests ("the old source was not released", 1 against 2). Restored. Real TIS calls are serialized in the tests: HIToolbox aborts when two libtest threads call them at once.
- proposed fix:    in `reinit`, copy into a local and `CFRelease` it on the layout error before returning; in `reload`, reinit first and release the old source only on success (or clear the fields and make `deinit` tolerate null).
- upstream status: not reported

### UD70 — the Metal layer's display callback can run with a null renderer while the render thread installs it
- kind:            crash (null dereference) / data race
- upstream project: ghostty
- severity:        low (a one-store window at render-thread start, against a main-thread `display`)
- reference:       `src/renderer/metal/IOSurfaceLayer.zig:127-139` (`setDisplayCallback`: `display_cb` stored, then `display_ctx`), `:151-162` (the `display` override reads both and calls `cb(ctx)`), `:41-47` (`release` clears with `setDisplayCallback(null, null)`); the install runs on the render thread: `src/renderer/Thread.zig:227-228` → `src/renderer/generic.zig:902-906` → `src/renderer/Metal.zig:164-170`; `display` fires on the main thread, e.g. on a bounds change (`Metal.zig:146`, `needsDisplayOnBoundsChange`); `displayCallback` (`Metal.zig:172-176`) takes `*Renderer`, installed through `@ptrCast`; at 20abdb50a
- defect:          the two ivars are written by the render thread with no synchronization while CoreAnimation reads them on the main thread. Between the two stores a `display` reads the new callback and a still-null context and calls `displayCallback(null)`, which dereferences it (`renderer.drawFrame`). The ivar access is also a formal data race.
- ReleaseSafe:     a null dereference (SIGSEGV): the cast is of the function pointer, so no safety check sees the null. By reading (triage `r13-verify-triage.md` M91); not reproduced against a real CoreAnimation schedule.
- ReleaseFast:     the same.
- reachability:    thread timing on ordinary use: a window appearing or resizing (a bounds change) while its surface's render thread is starting.
- port handling:   `crates/rustty-renderer/src/metal/iosurface_layer.rs`. `set_display_callback` stores the context before the callback when installing, and the callback before the context when clearing (the reference's own `release` order), so a reader sees either no callback or a callback with its context; `display` also skips a callback whose context is null, the same as its no-callback arm. The `AtomicPtr` ivars (already in the port) remove the data race. The Ivars PORT NOTE, which said both sides run on the main thread, is corrected. PORT NOTEs at `Ivars`, `display` and `set_display_callback`.
- parity row(s):   NEW-FINDINGS N58. Inventory M91 (records F16; triage outcome FIX).
- test:            through a SEAM, because a real CoreAnimation schedule cannot be driven: `set_display_callback` calls `tests::between_display_stores` between its two stores, compiled only into the crate's own test build (`#[cfg(all(test, target_os = "macos"))]` at the call site and on the module; with the call site's `cfg` removed, a non-test `cargo check -p rustty-renderer` fails with E0433, so the seam cannot reach a production build). The hook plays the main thread: it reads the ivars and sends `-display` at the moment of the race. `a_display_while_the_callback_is_installed_never_gets_a_null_context`, `a_display_while_the_callback_is_cleared_calls_nothing`, and `display_skips_a_callback_whose_context_is_null` (rustty-renderer, macOS) each assert the hook ran once and that the callback runs with its context once installed. Negative controls, macOS 2026-09-22, one at a time: the reference's behaviour (callback first, no null check) fails with "display between the stores must not call the callback; it got [0]" — the callback ran with a null context — and fails the null-context test; the store order alone reverted fails "the callback is published last, after its context"; the null check alone removed fails "no call with a null context". Restored.
- proposed fix:    in `setDisplayCallback`, store `display_ctx` before `display_cb` when installing (keep `release`'s clear order), and skip the call in `display` when the context is null; or install the callback on the main thread.
- upstream status: not reported

### UD71 — a huge `window-padding-x`/`-y` traps in `scaledPadding` or overflows the padding sums
- kind:            crash (safety-checked trap) / undefined behaviour
- upstream project: ghostty
- severity:        low (a config value far outside any real window)
- reference:       `src/Surface.zig:437-455` (`DerivedConfig.scaledPadding`: `@intFromFloat(@floor(padding * dpi / 72))` per side), called at `:543-546` (`init`, before `balancePadding` at `:548`), `:2527` (`balancePaddingIfNeeded`, reached from `setCellSize` at `:2412` and `resize` at `:2501`) and `:3662` (`contentScaleCallback`, which stores the UNBALANCED scaled padding directly: `self.size.padding = self.config.scaledPadding(x_dpi, y_dpi)`); `src/renderer/size.zig:199-203` (`ScreenSize.subPadding`: `self.width -| (padding.left + padding.right)`), `:210-214` (`blankPadding`: `grid_width + (padding.left + padding.right)`), `:72` (`balancePadding`: `explicit.left + explicit.right + self.cell.width`); the four `window_padding_*` are `u32` from `window-padding-x`/`-y` (`:408-411`); at 20abdb50a
- defect:          the padding config is any `u32` and nothing bounds it. The cast in `scaledPadding` traps when the scaled value leaves `u32`; a value that converts can still overflow the unchecked `u32` sums of two paddings downstream. The saturating `-|` in `subPadding` shows the intent — a padding larger than the screen just leaves no room, which `resize` already reports ("Is your padding reasonable?") — but the sum inside it is unchecked.
- ReleaseSafe:     `window-padding-x = 4294967295` (72 dpi) or `3000000000` (144 dpi, Retina): panic "integer part of floating point value out of bounds" in `scaledPadding` at startup. `window-padding-x = 2147483648` (72 dpi): the cast gives 2147483648 and `subPadding` panics "integer overflow". Measured with Zig 0.16.0 (probe of the two expressions, Debug and ReleaseSafe identical).
- ReleaseFast:     the cast yields 4294967295 (measured); the sum wraps — for 2147483648 a side it wraps to 0, so the "huge" padding silently becomes no padding at all (`subPadding(800)` = 800, measured).
- reachability:    config or CLI: `window-padding-x = 2147483648` (or `window-padding-y`), at any content scale.
- port handling:   the port's `scaledPadding` already converted with a saturating `as u32` (4294967295, the reference's own ReleaseFast value: "as large as it goes"), but the sums then overflowed in the port too, since release keeps overflow checks. `crates/rustty-render-types/src/lib.rs` `ScreenSize::sub_padding`, `blank_padding` and `Size::balance_padding` now add the paddings with `saturating_add`, so such a padding leaves a zero-size terminal area, the grid keeps its one-cell minimum (`GridSize::update`'s `@max(1, …)`), and `resize` warns as it does for any unreasonable padding. PORT NOTEs at `scaledPadding` (`crates/rustty-core/src/surface.rs`) and at the three sums.
- parity row(s):   NEW-FINDINGS N59. Not in the inventory: found by task R13-P (its review confirmed the cast site) and decided by task R13-C, which found the downstream sum.
- test:            `an_oversized_window_padding_saturates_and_leaves_a_one_cell_grid` (rustty-core) loads `window-padding-x = 4294967295` / `window-padding-y = 2147483648` through the config loader into a real `DerivedConfig`, and asserts: `scaledPadding(144, 72)` saturates x to 4294967295 and keeps y at 2147483648; the terminal area is 0x0; the grid is 1x1; `blank_padding` and the `balance_padding(.true)` path run. A positive control scales an ordinary padding (2/3 → 4/6 at 144 dpi). Negative controls, macOS 2026-09-22, one at a time: the cast made to trap fails "UD71 trap reinstated"; the reference's unchecked `+` restored in `sub_padding`, in `blank_padding`, or in `balance_padding` each fails "attempt to add with overflow" at that line. Restored.
- proposed fix:    `std.math.lossyCast(u32, @floor(padding * dpi / 72))` in `scaledPadding`, and `+|` for the padding sums in `size.zig` (or clamp `window-padding-*` in `Config.finalize`).
- upstream status: not reported

### UD72 — libvaxis's mouse decode panics on a button mask with both wheel bits, and on an X10 byte below 32
- kind:            crash (safety-checked trap) / undefined behaviour
- upstream project: libvaxis
- severity:        low (tty input to a CLI TUI)
- reference:       libvaxis 0.6.0 (the version Ghostty pins) `src/Parser.zig:688-746` (`parseMouse`): `const button: Mouse.Button = @enumFromInt(button_mask & mouse_bits.buttons);` at `:714`, with `buttons = 0b11000011`, into the exhaustive `Mouse.Button` (`src/Mouse.zig:16-29`); and the X10 branch `button_mask = full_input[3] - 32; px = full_input[4] - 32; py = full_input[5] - 32;` (`:697-699`, `u8` arithmetic)
- defect:          (1) the mask keeps bits 0-1 and 6-7, so 192..=195 (both wheel bits set) are values `Mouse.Button` does not name, and `@enumFromInt` of one is illegal behaviour. (2) An X10 mouse sequence (`ESC [ M` plus three raw bytes) with any of those bytes below 32 underflows the `u8` subtraction.
- ReleaseSafe:     (1) panic "invalid enum value"; (2) panic "integer overflow". Measured with Zig 0.16.0 on the two expressions (mask 192; byte 16), Debug and ReleaseSafe identical.
- ReleaseFast:     (1) a `Button` holding 192, which no `switch` on the type expects (undefined behaviour); (2) the byte wraps (16 - 32 = 240, measured), giving a bogus coordinate or mask.
- reachability:    tty input to a libvaxis TUI (in Ghostty: the CLI's TUIs, such as `+list-themes` and `+boo`): a buggy or hostile host terminal, or the bytes typed or pasted, e.g. `ESC [ < 192 ; 1 ; 1 M`.
- port handling:   `crates/rustty-vaxis/src/mouse.rs` `Button::from_u8` returns `None` for a value no button has, and `crates/rustty-vaxis/src/parser.rs` `parse_mouse` skips such an event, and an X10 event with a byte below 32 (`checked_sub`), returning no event while consuming exactly what the event would have (`6` for X10, the sequence length for SGR — the function's own `n` for a mouse event), so the bytes are not re-read as keys. Skipping rather than mapping to `.none` invents no event. PORT NOTEs at both sites.
- parity row(s):   NEW-FINDINGS N60. Inventory M23 (records F7); the X10 underflow is not in the inventory (found by task R13-C in the same function).
- test:            `a_mouse_event_with_an_invalid_button_or_byte_is_skipped` (rustty-vaxis): SGR masks 192, 195 and 228 (modifier bits on top) and four X10 sequences (button byte 0xE0, and a byte below 32 in each of the three positions) give no event and are consumed whole; the neighbours 131 and 67 (SGR) and 0xA3 (X10) still decode to `button_11`/`wheel_left`/`button_11`. Negative controls, macOS 2026-09-22, one at a time: `unreachable!()` restored in `from_u8` panics "entered unreachable code"; the plain `- 32` restored panics "attempt to subtract with overflow". Restored.
- proposed fix:    decode with `std.meta.intToEnum(Mouse.Button, …) catch return null_event` (consuming the sequence), or make `Mouse.Button` non-exhaustive; and use `std.math.sub(u8, full_input[i], 32) catch return null_event` in the X10 branch.
- upstream status: not reported

### UD73 — the renderer thread's timer and cancel callbacks hit `unreachable` on an error the event loop can return
- kind:            crash (ReleaseFast: undefined behaviour)
- upstream project: ghostty (the errors are libxev's documented sets; libxev maps io_uring's `-EALREADY` on a timer cancel to `Unexpected`)
- severity:        low (Linux io_uring, kernel timing)
- reference:       `src/renderer/Thread.zig:690-714` (`cursorCancelCallback`: `else => { log.warn("error in cursor cancel callback err={}", .{err}); unreachable; }` at `:707-710`); siblings `renderCallback` `else => unreachable` (`:559`), `animationTimerCallback` (`:621`), `cursorTimerCallback` `else => { log.warn("error in cursor timer callback err={}", …); unreachable; }` (`:664-667`); at 20abdb50a
- defect:          the callbacks declare every error other than `Canceled` (and, for the cancel, `NotFound`) unreachable, but they are not guaranteed by construction. On Linux io_uring `Timer.cancel` is an `IORING_OP_TIMEOUT_REMOVE` (libxev `watcher/timer.zig:156-173`), and libxev maps its result as `NOENT => NotFound`, `BUSY => ExpirationInProgress`, `TIME => ok`, anything else `Unexpected` (`backend/io_uring.zig:764-777`); a remove that races the timer firing completes with `-EALREADY` (the kernel's `io_timeout_extract` when `hrtimer_try_to_cancel` reports the callback running), which arrives as `Unexpected`. The timer SQEs themselves can complete with other errnos that also map to `Unexpected`. On kqueue the arms are dead (timers never reach a syscall; the cancel set is only `Canceled`).
- ReleaseSafe:     panic "reached unreachable code" on the render thread (after the warning, for the two callbacks that have one). By reading.
- ReleaseFast:     undefined behaviour.
- reachability:    kernel timing on Linux io_uring: the surface LOSING FOCUS while the cursor-blink timer fires. `cursorCancelCallback` is passed at exactly one place, the `.focus = false` arm (`Thread.zig:357-370`, the `cursor_h.cancel` at `:362-369`). A cursor-blink reset does NOT reach it (corrected by task R13-C2; the earlier text named it): `reset_cursor_blink` (`Thread.zig:388-400`) calls `Timer.reset` (libxev `watcher/timer.zig:94-129`), whose io_uring `timer_reset` (`backend/io_uring.zig:289-324`) queues its own `TIMEOUT_REMOVE` on `c_cancel` with a fresh completion whose callback is the loop's no-op (`:320-322`, default at `:625`), so no user callback sees its result. The `cursorTimerCallback` arm is reached only if the timer SQE itself completes with an errno libxev does not name (`-ETIME` is the expiration, `-ECANCELED` the cancel), which nothing here was found to produce. The `-EALREADY` path is from reading the kernel's `io_uring/timeout.c` (from memory, not re-read against this kernel's source). NOT reproduced: a probe on the VM (Fedora, kernel 6.19.10 aarch64; a temporary, uncommitted `rustty-xev` test) armed a 1 ms io_uring timer, spun to 0.90-1.10 ms and then 0.99-1.02 ms, and cancelled it, 170,000 times: 160,576 cancels succeeded and 9,424 answered `NotFound`, none `ExpirationInProgress` or `Unexpected`. The window is the hrtimer callback's own run time.
- port handling:   `crates/rustty-core/src/renderer_seam.rs` (task R13-C). `cursorCancelCallback`'s Linux arm keeps the reference's warning and then disarms, as its success arms do (the timer has fired or is firing; nothing is left to cancel). `renderCallback` and `animationTimerCallback` take the `Canceled` arm (disarm) for any error, with no diagnostic because the reference has none; `cursorTimerCallback` keeps the reference's warning and then disarms. What each disarm leaves behind (corrected by task R13-C2): (a) after a failed focus-loss cancel the timer was not removed, so its own completion arrives as an expiration and `cursorTimerCallback` re-runs it: the blink timer keeps ticking while the surface is unfocused, which is invisible — an unfocused cursor is a hollow block whatever the blink state (`src/renderer/cursor.zig:58-60`) — and costs one wakeup per blink interval until the next focus loss cancels it again; on focus gain `cursor_c` is still active, so blinking simply continues. (b) after the `cursorTimerCallback` error arm, `cursor_c` is dead, and `reset_cursor_blink` re-runs the timer only while it is active (`Thread.zig:390`; the port's `renderer_seam.rs` `ResetCursorBlink` arm), so a keypress only makes the cursor visible (`cursor_blink_visible = true`) and it stays steady; blinking resumes on the next focus GAIN (`Thread.zig:371-384`, which runs the timer when `cursor_c` is not active). A stopped timer while unfocused is exactly the reference's intended focus-loss state, so the only degraded case is a focused cursor that stops blinking (and, if the error lands in the hidden phase, stays hidden until the next keypress or focus change). Neither loses input or output, which keeps the severity low. PORT NOTEs at each arm.
- parity row(s):   LEDGER S114-F2 (closed by 14d-1 on the ported `unreachable`; reopened and re-closed by task R13-C). Inventory M16 (UD candidate C3, records F5, sessions D3).
- test:            `timer_callbacks_disarm_on_an_unexpected_error` (rustty-core; runs on every target) injects `TimerRunError::Unexpected` into the three timer callbacks with a null thread — the kernel error cannot be forced, and the null thread proves the error arm returns before the thread is touched — and asserts `.disarm` from each, silence from the first two and exactly the reference's cursor-timer warning from the third. `cursor_cancel_callback_warns_and_disarms_on_a_cancel_error` (rustty-core, Linux only; it replaces `cursor_cancel_callback_traps_on_an_impossible_error`, which pinned the trap) injects `ExpirationInProgress` and `Unexpected` and asserts `.disarm` plus exactly the reference's warning for each, which also proves the error arm ran. Injected, because the kernel race could not be forced (see reachability). Negative controls, one at a time: `unreachable!()` restored in each timer callback (macOS 2026-09-22) panics "entered unreachable code" at that callback's line; restored after the cancel warning (VM, 2026-09-22) panics the same at `renderer_seam.rs:2033`. Restored.
- proposed fix:    replace each `unreachable` with the `Canceled` arm's `return .disarm` (keeping the existing warnings), since the loop can deliver these errors.
- upstream status: not reported

### UD74 — libvaxis's SIGWINCH handler takes a mutex and runs callbacks in signal context
- kind:            deadlock
- upstream project: libvaxis
- severity:        low (a resize must land on a thread holding the handlers mutex or a lock a callback takes)
- reference:       libvaxis 0.6.0 (the version Ghostty pins) `src/tty.zig:132-140` (`handleWinch`: `handler_mutex.lock(handler_io) catch @panic(…)`, then every registered callback), installed at `:62-73` (`posix.sigaction(posix.SIG.WINCH, …)` in `PosixTty.init`); the same non-recursive `handler_mutex` (`:49`) guards `notifyWinsize` (`:123-130`); the callback Ghostty's CLI TUIs register is the `Loop`'s winsize callback (an `ioctl` and an event-queue push)
- defect:          neither a mutex lock nor the queue push is async-signal-safe. If SIGWINCH is delivered to a thread that already holds `handler_mutex` (inside `notifyWinsize`) or the event queue's lock, the handler blocks on a lock its own thread holds, forever. The inventory recorded the scenario as unverified, and a validation report called it overstated: the window is the few instructions of a registration or a queue operation.
- ReleaseSafe:     the thread hangs (and with it the TUI, if it is the main or the input thread). `@panic` only if the lock reports cancelation. By reading: `std.Io.Mutex` is not recursive; no Zig probe was built.
- ReleaseFast:     the same.
- reachability:    a terminal resize (SIGWINCH from the kernel) while a libvaxis TUI registers its handler or pushes an event — e.g. resizing the window as `+list-themes` or `+boo` starts. In the port the window was wider: rustty's own `deregister_winsize` (called from `Loop::drop`) takes the same mutex.
- port handling:   `crates/rustty-vaxis/src/tty.rs`: the handler now only writes one byte to a non-blocking, close-on-exec self-pipe, saving and restoring `errno` around the `write(2)`. A process-lifetime dispatcher thread (`vaxis-winch`), created with the pipe the first time the handler is installed, reads it and runs the registered callbacks under the mutex in ordinary thread context, once per wake (signals that arrive before it wakes coalesce, as the kernel already coalesces pending SIGWINCHes). The `sigaction` itself is unchanged. A failure to create the pipe or the thread is returned from `PosixTty::init` as an errno, as its other syscalls are. PORT NOTEs at the module header, `install_winch_handler` and `handle_winch`. Two limits of the new design, recorded by task R13-C2 and not changed: (1) a `fork()` WITHOUT `exec` (the pipe is close-on-exec, so an `exec` is unaffected) leaves the child sharing the self-pipe but not the dispatcher thread, so a SIGWINCH to the child wakes the PARENT's dispatcher, which runs the parent's callbacks once (a spurious size re-read, harmless), and the child's own callbacks never run; upstream's child would run them in signal context against a `Loop` whose input thread did not survive the fork. No rustty process forks without `exec` while a libvaxis TUI is up (the CLI TUIs spawn nothing). An `atfork` child handler that sets the write end to -1 would stop the cross-wake but not give the child a dispatcher, so it is not clearly right and is not applied. (2) A callback that panics does NOT silently end the dispatcher: callbacks are `extern "C" fn`, and a panic that would unwind out of one aborts the process ("panic in a function that cannot unwind"; measured with a probe on rustc 1.93.1, a thread named `vaxis-winch` calling a panicking `extern "C" fn`: exit 134), which is what a panic in upstream's signal-context call did too. The dispatcher only ends silently if `read(2)` on its pipe reports EOF or an error other than `EINTR`, which needs the pipe to be closed, and nothing closes it.
- parity row(s):   NEW-FINDINGS N61. Inventory M24 (records F11).
- test:            `a_sigwinch_while_the_handlers_are_locked_still_reaches_the_callback` (rustty-vaxis, macOS and the Linux VM). A positive control first: `kill(getpid(), SIGWINCH)` reaches a registered callback. Then the deadlock shape, deterministically: a helper thread locks the handlers mutex and sends SIGWINCH to ITSELF (`pthread_kill`), then unlocks; the test asserts the thread finishes within 5 s, that the resize still reaches the callback afterwards, and that every callback ran on the `vaxis-winch` thread, never in signal context. Negative control, 2026-09-22, on macOS and on the VM: the reference's handler body restored (lock the mutex, run the callbacks) fails with "the SIGWINCH handler deadlocked on the handlers mutex" after the 5 s bound. Restored. `the_sigwinch_handler_neither_blocks_on_a_full_pipe_nor_clobbers_errno` (rustty-vaxis, task R13-C2) holds the handlers mutex so the dispatcher cannot drain, calls the handler 262144 times (more than a 64 KiB pipe holds) on a helper thread, checks the pipe is full (a direct `write` answers `EAGAIN`), then sends that thread a real SIGWINCH with `EDOM` in `errno`, and asserts the thread finished within 5 s and `errno` is still `EDOM`. Negative controls, macOS 2026-09-22, one at a time: the write end left blocking (the `O_NONBLOCK` `fcntl` removed) fails "the SIGWINCH handler blocked on a full pipe (write end not O_NONBLOCK): Timeout"; the `errno` restore removed fails with 35 (`EAGAIN`) against 33 (`EDOM`). Restored.
- proposed fix:    make `handleWinch` async-signal-safe: set an atomic flag or write to a self-pipe, and run the callbacks from the event loop's own thread (the input thread can `poll` the pipe alongside the tty).
- upstream status: not reported

### UD75 — the renderer thread's `stopCallback` hits `catch unreachable` on an `Async` wait error
- kind:            crash (safety-checked trap) / undefined behaviour
- upstream project: ghostty (the error is libxev's documented `Async.WaitError`; the defect is Ghostty not handling it)
- severity:        low (the epoll fallback backend with the user's epoll watch limit exhausted, or the kernel out of memory)
- reference:       `src/renderer/Thread.zig:722-731` (`stopCallback`: `_ = r catch unreachable;` at `:728`), registered once at `:239` (`self.stop.wait(...)`, after the wakeup's at `:238`); the sibling `wakeupCallback` (`:485-494`) handles the same `xev.Async.WaitError` with `log.err` and `.rearm`; `Surface.deinit` (`src/Surface.zig:804-808`) notifies the stop and joins, logging "may stall" if the notify fails; `src/global.zig:30` selects `xev.Dynamic`; libxev (9ce8e8e6) `src/watcher/async.zig:135-190` (`waitPoll`: the eventfd wait is a `.poll` whose error reaches the callback at `:184`), `src/backend/epoll.zig:821-834` (the `.poll` start: `epoll_ctl(EPOLL_CTL_ADD)` failing breaks out with the error), `:36-48` (`ENOMEM` → `SystemResources`, `ENOSPC` → `UserResourceLimitReached`); at 20abdb50a
- defect:          the wait is declared infallible, but on Linux the stop `Async` is an eventfd `poll`, and when Ghostty's dynamic loop runs on the epoll backend (io_uring unavailable: disabled by `kernel.io_uring_disabled`, a seccomp filter, or an old kernel) the poll starts with an `epoll_ctl` that can fail. The error is delivered to `stopCallback`, which traps.
- ReleaseSafe:     panic "attempt to unwrap error" on the render thread when the stop wait's registration fails (it is registered once, at thread start); the app aborts. By reading in the reference; the port's handling was reproduced by fault injection (below).
- ReleaseFast:     undefined behaviour.
- reachability:    the kernel, in a narrow window, on a host where the dynamic loop fell back to epoll. If the user's `fs.epoll.max_user_watches` is already exhausted when the renderer thread starts, the stop callback is NOT reached: the epoll loop's own eventfd registration fails first, on the first tick, `run` returns an error and the thread exits with "error in renderer" (measured by fault injection in the port, review of task R13-C2, 2026-09-22). The trap needs `epoll_ctl(EPOLL_CTL_ADD)` for the stop wait to fail after the loop's own registration and the wakeup's succeeded: the per-user limit (shared by every process of that user, so another program can use it up) crossed within those few syscalls (one or two watches left), or a transient `ENOMEM`. The wakeup's callback re-arms on the same error; the stop's traps. Not measured on a real limit: the limit is a global sysctl counted per user, so it cannot be lowered for one process, and exhausting it on the shared Linux VM would have failed other agents' processes. On io_uring the poll SQE can also complete with an error, and on macOS the mach-port kevent registration can fail (`MachPortError`); neither was found reachable in practice.
- port handling:   `crates/rustty-core/src/renderer_seam.rs` `stopCallback` returns `.rearm` on an error, the sibling `wakeupCallback`'s handling of the same error type, without that callback's message (it names the wakeup; the reference has none here). Disarming would leave the thread with no stop watcher, so `Surface::deinit`'s join would hang for good; stopping the loop would end rendering on an error that is not a stop request. The eventfd (a mach port on macOS) keeps a pending notification, so a stop notified while the wait cannot register is delivered once it does. While the registration keeps failing the loop retries once per tick, as the reference's own `wakeupCallback` already does in the same state. What paces a tick is the loop's other work: in the renderer the wakeup wait or the cursor timer is always active, so each retry waits for it (fault injection with a timer active: 80 retries in 2 s, then the pending stop was delivered once the fault was lifted). With nothing else active the retry would be a busy spin (44 million retries in 1 s in an isolated probe), which the renderer does not reach. PORT NOTE at the site.
- parity row(s):   NEW-FINDINGS N62. Not in the inventory (review of task R13-C, observation 7).
- test:            `stop_callback_rearms_on_a_wait_error` (rustty-core, every target) injects `WaitError::Unexpected` with a null thread (the kernel error cannot be forced; the null thread proves the error arm returns before the thread is touched) and asserts `.rearm` and no log record. Negative controls, macOS 2026-09-22, one at a time: the reference's `unreachable!()` restored fails with "internal error: entered unreachable code" at the arm; `.disarm` instead of `.rearm` fails with `Rearm` against `Disarm`. Restored.
- proposed fix:    `_ = r catch |err| { log.err("error in stop wait err={}", .{err}); return .rearm; };`, as `wakeupCallback` does.
- upstream status: not reported

### UD76 — the IME point and the custom-shader cursor uniforms overflow `u32` with a huge padding
- kind:            crash (safety-checked trap) / undefined behaviour
- upstream project: ghostty
- severity:        low (a config value far outside any real window, and a transient window)
- reference:       `src/Surface.zig:2134` and `:2147` (`imePoint`: `cursor.x * self.size.cell.width + self.size.padding.left`, and the `y`/`top` twin); `src/renderer/generic.zig:2351` and `:2356` (`updateCustomShaderUniformsForFrame`, `:2307`: the cursor's `grid_pos[0] * cell.width + padding.left` and `grid_pos[1] * cell.height + padding.top`); the padding is stored unbalanced by `contentScaleCallback` (`src/Surface.zig:3662`); at 20abdb50a
- defect:          a sibling of UD71. The sums are unchecked `u32`. A `window-padding-x`/`-y` that scales to the top of `u32` (UD71) leaves a one-cell grid, so in the steady state the cursor is at 0 and nothing overflows. But a config reload that sets such an unbalanced padding takes effect only on the next DPI change: `contentScaleCallback` stores it at `:3662`, and until the IO thread's resize shrinks the grid the cursor can still be off the first column or row. An `imePoint` call or a custom-shader frame in that window overflows.
- ReleaseSafe:     panic "integer overflow" (by reading; the same expression shape UD71 measured with Zig 0.16.0).
- ReleaseFast:     the sum wraps, giving a wrong IME position or cursor uniform for that frame.
- reachability:    config reload plus a content-scale change (moving the window to a display with another scale), then an IME query or a custom-shader frame before the resize lands. Found by the review of task R13-C (observation 3); not reproduced as a race.
- port handling:   `crates/rustty-core/src/surface.rs` (`imePoint`), `crates/rustty-renderer/src/generic.rs` and `crates/rustty-renderer/src/generic/linux_impl.rs` (`update_custom_shader_uniforms_for_frame`) compute the products and sums with `saturating_mul`/`saturating_add`, the handling UD71 gave the padding sums. A saturated value only places the IME window or the shader's cursor far off-screen for the transient frames. The port had panicked there too (release keeps overflow checks). PORT NOTEs at each site; the `scaledPadding` note, which credited only the `rustty-render-types` sums, now names these.
- parity row(s):   NEW-FINDINGS N63. Not in the inventory.
- test:            `ime_point_saturates_with_a_saturated_padding` (rustty-apprt-c `tests/macos_window_host.rs`) drives the real `rustty_surface_ime_point` on a live surface whose child is a silent `sleep`, with the cursor placed at column 4, row 3 and `padding.left`/`top` = `u32::MAX`, and asserts both coordinates come from the saturated sums. `custom_shader_cursor_uniform_saturates_with_a_saturated_padding` (rustty-renderer; the Metal renderer on macOS and the OpenGL one on Linux) sets the same padding and a cursor glyph at `[3, 2]`, runs the per-frame uniform update, and asserts the saturated cursor edge. Negative controls, macOS 2026-09-22, one at a time: the unchecked `+` restored in `imePoint`'s x sum, then its y sum, panics "attempt to add with overflow" at that line (inside the `extern "C"` entry, so the test process aborts); restored in the Metal uniform's x sum, then its y sum, fails "attempt to add with overflow" at that line. Restored.
- proposed fix:    `+|` (and `*|`) in the four expressions, or clamp `window-padding-*` in `Config.finalize` (UD71's alternative), which removes the input for both.
- upstream status: not reported

### UD80 — styled output on a terminal about 47,900 or more columns wide overflows the `u16` style refcount
- kind:            crash (ReleaseFast: refcount corruption)
- upstream project: ghostty
- severity:        medium (an embedder sets the width; ordinary VT output then triggers it)
- reference:       `src/terminal/PageList.zig:535-540` (`initialCapacity`: when `std_capacity.adjust(.{ .cols = cols })` fails, the page becomes `std_capacity` with only `cols` changed) → two print routes: ordinary VT output (`ghostty_terminal_vt_write`) takes `src/terminal/stream_terminal.zig:362` → `printSliceFast` (`src/terminal/Terminal.zig:642`) → `printSliceFill`, which overflows in `page.styles.useMultiple` (`Terminal.zig:1114` → `src/terminal/ref_counted_set.zig:385-398`, an unguarded `item.meta.ref += n` at `:397`); the `Terminal.print` API takes `printCell`'s `page.styles.use` (`Terminal.zig:1722` → `ref_counted_set.zig:365-379`, `assert(item.meta.ref > 0)` at `:376`, then an unguarded `item.meta.ref += 1` at `:378`). The same unguarded increments sit in `add` / `addWithId` (`:258`, `:329`, `:353`) and are reached by DECALN (`Terminal.zig:3756`, `useMultiple`), reflow (`PageList.zig:2057`, `:2076`, `:2448`), page clones (`page.zig:1014`, `:1021`) and a cursor style change (`Screen.manualStyleUpdate`'s `add`). The style set's `RefCountInt` is `size.CellCountInt` = `u16` (`src/terminal/style.zig:681-684`, `src/terminal/size.zig:22`); at 20abdb50a
- defect:          every styled cell holds one reference to its style, so a page must never hold more cells sharing a style than a `u16` can count. Standard pages respect that: `adjust` keeps the byte budget, so a standard page holds fewer than 65,535 cells. But once the width no longer fits the standard budget, `initialCapacity` keeps `std_capacity`'s 215 rows and sets `cols`, so the page holds 215 x cols cells, over ten million. The first width that leaves the standard capacity in Ghostty, MEASURED by the R13-S2 review: **47,932** in a shipped (non-test) ReleaseSafe `libghostty-vt` on macOS arm64 (16 KiB pages; 47,931 survives), **46,908** in the same build cross-compiled for Linux aarch64 (4 KiB pages; 46,907 survives), and **48,047** in a macOS test build (512-byte grapheme budget; 48,046 survives). Method: a C-ABI probe, `ghostty_terminal_new(cols, 3)`, then `ESC[1m` and 70,000 `A` through `ghostty_terminal_vt_write`, plus a sweep of `std_capacity.adjust` over every width, which agrees in Debug, ReleaseSafe and ReleaseFast and never succeeds again above the threshold. The threshold depends on the platform's `page_size_min`. The PORT's thresholds were different — 48,036 shipped and 48,151 in a test build on macOS, from a sweep of the port's `Capacity::adjust` — because the port's style-set item is 28 bytes against the reference's 36 (its `Color` is 4 bytes, Zig's 8). That divergence is NEW-FINDINGS N88, fixed by task R13-L: the style set is now laid out with the reference's item geometry, and the same sweep gives the port 47,932 shipped and 48,047 in a test build on macOS, 46,908 shipped and 46,511 in a test build on Linux aarch64, equal to Ghostty's. With the cursor's own reference, the 65,535th cell printed in one style overflows the refcount (MEASURED by the R13-S2 review, blocking item 2: 65,534 prints leave it at 65,535, the next one panics). Nothing bounds the width below that: `CellCountInt` allows 65,535 columns.
- ReleaseSafe:     panic "integer overflow" at `ref_counted_set.zig:378`, called from `Terminal.zig:1722` (`printCell`); the process aborts. MEASURED 2026-09-22 by a Zig 0.16.0 probe on macOS arm64 against a copy of the pin (`zig build test-lib-vt -Doptimize=ReleaseSafe`): `Terminal.init(cols = 48151, rows = 3)` (page capacity 48,151 x 215), `setAttribute(.bold)`, then 70,000 `print('A')`. The R13-S review measured the same. The VT route, MEASURED by the R13-S2 review on a shipped ReleaseSafe build through the C ABI (above): 47,932 columns aborts "integer overflow" in `printSliceFill` / `useMultiple` (`ref_counted_set.zig:397`).
- ReleaseFast:     the refcount wraps silently. MEASURED by the same probe with `-Doptimize=ReleaseFast`: the cursor style's refcount reads 4465 after the 70,000 prints (70,001 mod 65,536). By reading, the wrap passes through 0, so the next `use` violates `use`'s own `assert(item.meta.ref > 0)` (`:376`), which is undefined behaviour; from then on the count is meaningless, and releasing cells can drive a live style to zero, after which the set's dead-item trimming (`ref_counted_set.zig:246-249`) can reuse a slot cells still point at (wrong styles; a memory-unsafe outcome is UNVERIFIED).
- reachability:    a libghostty-vt / rustty-vt embedder that creates or resizes a terminal past the threshold (in Ghostty 47,932 columns on macOS arm64, 46,908 on Linux aarch64) through the C ABI (`ghostty_terminal_new` / `ghostty_terminal_resize`; `rustty_terminal_*` in the port), followed by ordinary VT output: one SGR attribute and enough printable characters. The Ghostty and rustty apps size the grid from the window, so a real window would need tens of thousands of columns; the C ABI is the practical route. A snapshot decode is a second route to the same overflow (UD65, including a page UD65's degrade leaves at the ceiling).
- port handling:   `crates/rustty-vt/src/ref_counted_set.rs:115` (PORT NOTE above `bump_ref` / `drop_ref`): every increment in `RefCountedSet` saturates at `maxInt(RefCountInt)`, and a count at the maximum is never decremented again, so the item is PINNED: it stays live, and every cell keeps its style, in that page's memory; a clone of the page recounts it from the cells (exact, or pinned again if 65,535 or more cells still use it). The seven increment and decrement sites carry short PORT NOTEs (`add`'s existing-item arm, `add_with_id`'s two existing-item arms, `use_id`, `use_multiple`, `release`, `release_multiple`, at `:728`, `:834`, `:871`, `:939`, `:963`, `:1040`, `:1063`), so every route shares the one handling: the VT stream's `print_slice` (the reference's `printSliceFill` / `useMultiple`), `printCell`, DECALN (`use_multiple(cols)`), a cursor style change (`manualStyleUpdate`'s `add`), and the page copies in reflow, `increaseCapacity` / `splitForCapacity` and screen or page clones (`cloneFrom`'s `add_with_id` / `use` / batched `use_multiple`). The page integrity check accepts a pinned count below the cells seen (`crates/rustty-vt/src/page_impl.rs:767`, `verify_integrity`); a pinned style is never freed, which is what that check protects. Why this and not the alternatives. The reference's own recovery for a style it cannot store is `Screen.manualStyleUpdate` (`Screen.zig:2499-2514`): grow the style set (`increaseCapacity`) or split the page on `OutOfSpace` (`splitForCapacity`). Neither fits a saturated COUNT: growing adds slots, not count width; a split cannot bound the count, because one row of 65,535 styled columns plus the cursor's reference is already 65,536, and `PageList.split` refuses a one-row page (`PageList.zig:3730`); and the copy routes fill a fixed destination page with no split to fall back on. Re-adding under a new id is not possible either, because the set is keyed by value and `add` returns the existing id. Falling back to the default style (the snapshot decoder's degrade, UD65) would visibly drop bold or colour from cells the reference means to show styled. Pinning keeps every cell exactly as the reference intends. Its cost, and the port's mitigation (task R13-W2, after the R13-W review measured it): a pinned style stays counted after its cells are overwritten or erased (an ORPHAN), one style slot each, until the page is next cloned. Every clone of the page — capacity growth (`increaseCapacity`), a rehash (`increaseCapacity(null)`), a split's new page, a screen clone — recounts from the cells and drops orphans, but `increaseCapacity`'s growth projection counts them (`page.styles.count()`), so repeated pin-then-overwrite inflates the style capacity. MEASURED by the R13-W review (release build, 50,000 x 3, a new SGR 48;2 background plus DECALN per iteration, each DECALN pinning its style with 150,000 cells and orphaning the previous one): at iteration 103 the 104 usable slots of `styles = 128` fill, growth reclaims the orphans (count 103 -> 2) but projects the capacity to 4,096; at 3,429 it goes to 65,535, the `u16` maximum; at 56,675 the set is full at that ceiling, `increaseCapacity` returns `OutOfSpace`, and `manualStyleUpdate`'s fallback splits the page (3 rows -> 1 + 2). DECALN had homed the cursor, row 0 stayed on the full page, the `add` failed again, and `set_attribute` kept the previous style; every later SGR failed the same way (23,325 in a row, to the end of the run). Not a crash and not corruption — it is the reference's own "cannot store the style" degrade — but pinning is what opens that path (about 3.7 billion styled cells, some 10 s of output). Mitigation: before that split, `Screen::manual_style_update` re-clones the cursor's page once at the SAME capacity (`crates/rustty-vt/src/screen.rs:3869`, `reclaim_pinned_styles_before_split`, through `Screen::increase_capacity(node, null)`) when the set holds a pinned style (`crates/rustty-vt/src/pagelist/grow.rs:1004`, `styles_hold_pinned`); the clone recounts real users, which is exactly what growth already does below the ceiling. If the style count fell, the `add` is retried on the re-cloned page; if it did not (the styles are genuinely in use) or the clone could not allocate, the reference's split runs as before. It cannot loop: it is straight-line code, at most one re-clone per `manual_style_update` call, with no retry. The two row-copy retry loops, whose reference `OutOfSpace` arm panics ("increaseCapacity OutOfSpace": `PageList.cloneRowGrowCapacity`, `PageList.zig:5076-5108`, reached by `eraseRow` / `eraseRowBounded`; the Screen analog `clone_partial_row_grow_capacity`, reached by IL/DL and scrolling across a page boundary), take the same step once per call (`grow.rs:566`, `screen.rs:1460`, a `reclaimed_pinned_styles` flag): a style-set `OutOfSpace` with a pinned style re-clones once and retries the copy, and a second `OutOfSpace` reaches the reference's panic, so these loops gain at most one iteration. Without a pinned style every count is exact, a re-clone could reclaim nothing, and none of the three sites does anything the reference does not; the common path (a style add that fits, or growth below the ceiling) is untouched, and the scan for a pinned style runs only on the `OutOfSpace` path. The ported reference test "restoreCursor uses default style on OutOfSpace" guards that gate: its fabricated references are unpinned, and with the gate removed the re-clone reclaims them and the test fails. So the normal paths now handle a style set that fills with orphans, below the ceiling by growth and at it by this re-clone. The hyperlink set shares the type but cannot reach its ceiling (the per-page cell map holds at most 26,214 entries). Before this fix the port overflowed from a higher width than Ghostty (48,036 against 47,932 columns shipped on macOS), so between those widths Ghostty aborts and the port did not; with the fix, no width overflows in the port. Since NEW-FINDINGS N88 (task R13-L) the port also leaves the standard capacity at exactly Ghostty's widths, so the pinning starts at the same width at which Ghostty overflows. Performance: the change is a saturating add on the increment and one compare on the decrement. Measured on macOS arm64 with `rustty-bench +terminal-stream`, base (`r13-s2`) against fix, 15 alternating runs each: a 64 MiB SGR-heavy corpus (bold, colour and underline runs, every styled cell taking a reference) gave medians of 300.5 ms and 297.2 ms (ratio 0.989), and a repeat gave 320.0 ms and 309.1 ms (0.966); the ascii corpus gave 60.8 ms and 60.4 ms (0.993). That is within run-to-run noise, so there is no measurable cost.
- parity row(s):   NEW-FINDINGS N66 (CLOSED by task R13-W); no LEDGER row. Found by the R13-S review while checking UD65's premise.
- test:            six tests in `crates/rustty-vt/src/terminal.rs`, all on a 50,000 x 3 terminal: 50,000 columns leaves the standard capacity in Ghostty and in the port in every build measured (Ghostty 47,932 / 46,908 / 48,047; the port 48,036 / 48,151 when written, and the same as Ghostty since NEW-FINDINGS N88), and the tests' scan asserts that the page holds more than 65,535 cells (`cap.rows * cap.cols > u16::MAX`), which no standard page does, so they cannot go vacuous if those thresholds move (it does not assert the exact 215 x 50,000). Only the rows written are touched, so each test runs in well under a second. The cursor holds one reference and each styled cell one more, so the 65,535th styled cell is the first past the ceiling; the tests use 70,000 (the reference probe's count), 110,000, or DECALN's 150,000. Each asserts it does not panic, that every written cell carries the cursor's style id and that the style resolves to the expected attribute, and that the count is pinned at 65,535: `ud80_wide_terminal_vt_stream_past_the_refcount_ceiling_keeps_every_cell_styled` (SGR 1 and 70,000 bytes through `vt_stream`, the route an embedder's `vt_write` takes, which hands runs to `print_slice`: the reference's `printSliceFill` / `useMultiple`), `..._one_style_...` (the `print` API, `printCell`: 65,534 prints leave the count at exactly 65,535, the 65,535th print does not overflow, then on to 70,000; then 10 overwrites and an ED 2 release references on the pinned style, which stays pinned and is reused by the next bold `add`), `..._print_slice_...` (the print batch called directly), `..._decaln_...` (an RGB background, SGR 48;2, which DECALN keeps; it fills 150,000 cells), `..._reflow_...` (70,000 prints, then a resize to 50,049 columns re-wraps them into new pages) and `..._capacity_clone_...` (110,000 prints, then 300 distinct styles grow the page's style capacity, which clones every row). Before the fix all of them failed with "attempt to add with overflow". Negative controls, 2026-09-22, each applied by a script that refuses a pattern matching anything but once and restores the file: `bump_ref` made an unchecked add (the reference's `+= n`) turns all six red at that overflow; the pin removed from `drop_ref` turns the one-style test red ("a pinned style stays pinned on release", 65,524 against 65,535); the integrity check without its pin exemption turns the reflow, DECALN and clone tests red at the `verify_integrity` assertion (`page_impl.rs:552`); and each increment site reverted alone is red too (`use_id`: one-style, reflow, clone; `use_multiple`: vt-stream, print-slice, DECALN, clone; `add`'s existing arm: one-style, reflow, clone; `add_with_id`'s equal-id arm: clone). All restored. The dead-id arm of `add_with_id` (a dead id requested for a value live under another id), which the R13-W review found could be reverted alone with everything green, is covered by `every_increment_arm_saturates_and_pins_at_the_ceiling` (`crates/rustty-vt/src/ref_counted_set.rs`, task R13-W2): on a 32-slot set, each increment arm (that one, `add`'s existing arm, `add_with_id`'s equal-id arm, `use_id`, `use_multiple`) lands exactly on 65,535 from one below and stays there one past, and `release` / `release_multiple` leave a pinned count alone; that arm made an unchecked add turns it red ("attempt to add with overflow"). The capacity-ceiling mitigation (task R13-W2) has four tests, all reaching the state in a few iterations rather than the review probe's 56,675: they grow the style capacity straight to 65,535 with the reference test's own loop ("restoreCursor uses default style on OutOfSpace"), skipping the two projection jumps, and fill the rest of the set with pinned styles no cell uses — what the probe accumulates — until `add` fails; once it has failed with `OutOfMemory` (the ids are used up, or a probe sequence reached the last `psl_stats` bucket), every `add` of a new value fails the same way. `ud80_wide_terminal_orphaned_pinned_styles_at_the_capacity_ceiling_are_reclaimed_not_split` (`terminal.rs`) is the review's probe as a test: 50,000 x 3, a new RGB background plus DECALN per iteration, four iterations at the ceiling (each orphaning the last), then the fill, then six more; it asserts that every SGR succeeds, the cursor takes each new background, every DECALN cell carries it pinned at 65,535, the page is never split and stays at the ceiling, and that the reclaim happened exactly at iteration 4, after which fewer than 10 styles are left (1.9 s in a debug build). `ud80_wide_terminal_live_styles_at_the_capacity_ceiling_still_split` is the no-reclaim case: 70,000 bold cells (a pinned style that is genuinely used, so the re-clone runs) and one live style per remaining cell until the set is full, checked by `assert_integrity`; the next SGR re-clones, reclaims nothing and splits (two pages), no live style is lost, and the add fails on the full half as in the reference (the cursor keeps bold with no background); a second SGR returns too. `ud80_delete_lines_into_a_page_full_of_orphaned_pinned_styles_reclaims_them` (`terminal.rs`, `clone_partial_row_grow_capacity`) and `ud80_erase_row_into_a_page_full_of_orphaned_pinned_styles_reclaims_them` (`pagelist/tests.rs`, `clone_row_grow_capacity`) pull a bold row across a page boundary into a page whose set is full of orphans at the ceiling (4,000 columns keeps those pages at 12 rows, so the clones are cheap); the row arrives bold and the orphans are gone. Negative controls, 2026-09-22, by script, one at a time, all restored: the re-clone removed from `manual_style_update` turns the probe test red ("iteration 4: SGR failed: OutOfMemory", the unmitigated failure); removed from either retry loop, the matching row-copy test panics "increaseCapacity OutOfSpace"; the helper reporting a reclaim when the count did not fall turns the no-reclaim test red ("the page was split": the add is retried on the full page and fails without the split); the pinned-style gate removed turns the ported reference test "restoreCursor uses default style on OutOfSpace" red (its fabricated unpinned references are reclaimed, so the cursor keeps bold where the reference falls back to the default style).
- proposed fix:    bound the cells that can share one style on a page: either a `RefCountInt` wide enough for the page's cell capacity (for example `u32` for the style set), or have `printCell` treat a saturated refcount as `manualStyleUpdate` treats a full style set (split the page), or cap the column count so the non-standard page stays below `maxInt(u16)` cells.
- upstream status: not reported

### UD90 — a 128-byte input-method commit during a keypress overflows the `u7` `im_len`
- kind:            crash (safety-checked trap) / silent data loss
- upstream project: ghostty
- severity:        medium (an input method decides the commit)
- reference:       `src/apprt/gtk/class/surface.zig:662-663` (`im_buf: [128]u8`, `im_len: u7`), `:3269-3280` (`imCommit`, `.not_composing` arm: `if (str.len > priv.im_buf.len)` then `priv.im_len = @intCast(str.len)` at `:3277`), read back at `:1430-1451` (`keyEvent`), at 20abdb50a
- defect:          the guard admits a commit of exactly `im_buf.len` = 128 bytes, but `im_len` is a `u7` (0..=127), so `@intCast(128)` does not fit.
- ReleaseSafe:     panic "integer does not fit in destination type" (measured, Zig 0.16.0, the cast alone).
- ReleaseFast:     128 truncates to 0 (measured), so the committed text is silently dropped from the key event.
- reachability:    an input method (IBus, fcitx, XCompose) that commits exactly 128 bytes while a key event is in progress. The IM decides the commit; which real IM configurations produce 128 bytes is UNVERIFIED.
- port handling:   `im_len` is a `u8`, so the 128-byte commit is buffered whole; 129 bytes still take the reference's own "not enough buffer space" warning. PORT NOTE at `crates/rustty-apprt/src/gtk/class/surface.rs:4147`. The port's comment that called 128 "the reference's `u7` bound" is corrected.
- parity row(s):   inventory M8.
- test:            `a_128_byte_commit_during_a_keypress_is_buffered_whole` (rustty-apprt, display-gated, run on the VM under a private headless sway): 64 two-byte characters committed during a keypress give `im_len` 128 and the same bytes in `im_buf`; 129 bytes are not buffered. Negative control, VM 2026-09-22: the `u7` range reinstated as a check fails with "integer does not fit in destination type" at the store. Restored.
- proposed fix:    make `im_len` a `u8` (or `std.math.IntFittingRange(0, im_buf.len)`), or change the guard to `str.len >= priv.im_buf.len`.
- upstream status: not reported

### UD91 — a `global:` keybind on a key GTK has no keysym for hits `orelse unreachable` in the Wayland hotkeys
- kind:            crash (safety-checked trap) / undefined behaviour
- upstream project: ghostty
- severity:        medium (a config line, on a compositor that offers the protocol)
- reference:       `src/apprt/gtk/winproto/wayland/Hotkeys.zig:32-60` (`Entry.bind`: `const keysym = key.keysymFromTrigger(self.trigger) orelse unreachable;` at `:40`, with the comment "The only time this will return an error is when `trigger.key` is `catch_all`"), the public `bind` filter at `:95-136` (skips only `catch_all`, `:111`); `src/apprt/gtk/key.zig:62-68` (`keysymFromTrigger`: `.physical => |k| keyvalFromKey(k)`), `:319-332` (`keyvalFromKey` returns null for a key absent from `keymap`), at 20abdb50a
- defect:          `keysymFromTrigger` is also null for every `input.Key` the GTK `keymap` does not list: 44 values at the pin, counting `unidentified` (176 `input.Key` values, 132 of them mapped; counted by script, and every mapped name checked to be an enum value), e.g. `audio_volume_up`, `browser_back`, `media_play_pause`, `intl_backslash`, `numpad_comma`. `keybind = global:audio_volume_up=…` is not a `catch_all`, so it passes the filter and reaches the `unreachable`.
- ReleaseSafe:     panic "reached unreachable code" while binding the hotkeys. By reading.
- ReleaseFast:     undefined behaviour.
- reachability:    config: a `global:` keybind on one of those keys, under a compositor that advertises `vicinae_hotkey_manager_v1` (the Vicinae hotkey protocol). Which compositors ship that protocol is UNVERIFIED.
- port handling:   `Entry::bind` returns `BindError::NoKeysym`, which the reference's own `bind` loop reports (`log.warn("failed to request hotkey trigger={f} err={}", …)`, `:127-133`) and skips; the other binds still go out. PORT NOTE at `crates/rustty-apprt/src/gtk/winproto/wayland/hotkeys.rs:133`.
- parity row(s):   inventory M9.
- test:            `hotkeys_bind_skips_a_trigger_with_no_keysym` (rustty-apprt, Linux, no display: the fake `wayland-server` compositor of the existing wire test): `global:audio_volume_up` and `global:super+q` are both selected, exactly one has no keysym, `bind` does not panic, and only `q`'s keysym reaches the compositor. Negative control, VM 2026-09-22: `unreachable!()` restored in place of the `NoKeysym` return panics "entered unreachable code" at that line. Restored.
- proposed fix:    `const keysym = key.keysymFromTrigger(self.trigger) orelse return error.NoKeysym;` (the caller already logs a bind error), or skip such triggers in the public `bind` filter; and correct the comment.
- upstream status: not reported

### UD92 — the OpenType `SVG ` table parser reads past the table on a short or empty document list
- kind:            ub (out-of-bounds read) / crash (safety-checked trap)
- upstream project: ghostty
- severity:        medium (a font file)
- reference:       `src/font/opentype/svg.zig:25-50` (`SVG.init`: `const records: [*]const [12]u8 = @ptrCast(data[reader.seek..]);` at `:42`, `glyphRange(&records[0])` at `:43`, `records[(len - 1)]` at `:44`, `.records = records[0..len]` at `:49`), `:53-72` (`hasGlyph` binary search over `records`), at 20abdb50a
- defect:          `records` is a many-item pointer with no length. `records[0]` is read before `len` is considered, `len - 1` is computed on a `u16` 0, and nothing checks that `len` records fit in the table. So an `SVG ` table whose `numEntries` is 0, or larger than the bytes left, is read out of bounds.
- ReleaseSafe:     `numEntries = 0`: panic "integer overflow" on `len - 1`, after `records[0]` has already read past a table that ends there (the pointer read is unchecked). An overrunning count: no trap; `glyphRange(&records[len - 1])` and later `hasGlyph` read past the table. By reading (the `u16` underflow is the same shape as UD72's, measured there).
- ReleaseFast:     `len - 1` wraps to 65535 and `records[65535]` is read 786,420 bytes (about 768 KiB) past the start of the records; an overrunning count reads past it the same way.
- reachability:    a font file with an `SVG ` table (for example one named by `font-family`, or picked by fallback discovery).
- port handling:   `crates/rustty-font/src/opentype/svg.rs` returns the reference's own `EndOfStream` when the records do not fit (`len > (data.len() - pos) / 12`) and when `len == 0`. PORT NOTE at `svg.rs:76`.
- parity row(s):   LEDGER S92-F2 (closed by task R13-D), S92-F3 (WAIVED, re-grounded on R13); inventory M10.
- test:            `svg_document_list_with_no_records_is_end_of_stream` (new; positive control: the same table with one real record parses and answers `has_glyph`) and `svg_document_list_record_count_past_end` (rustty-font). Negative controls, macOS 2026-09-22, one at a time: the `len == 0` guard removed fails with "index out of bounds: the len is 0 but the index is 0" (the `records[0]` read); the count guard removed makes `init` return `Ok` for a two-record count with no record bytes (the port then reads past the table too). Restored.
- proposed fix:    slice the records with a length: `if (data.len - reader.seek < @as(usize, len) * 12) return error.EndOfStream; if (len == 0) return error.EndOfStream;` then `const records = std.mem.bytesAsSlice([12]u8, data[reader.seek..][0 .. len * 12]);`.
- upstream status: not reported

### UD93 — `killPid` waits forever for a child that survives SIGHUP
- kind:            deadlock (a surface that never finishes closing)
- upstream project: ghostty
- severity:        medium (ordinary use; see reachability)
- reference:       `src/termio/Exec.zig:1176-1215` (`killPid`: `while (true)` { `killpg(pgid, SIGHUP)`; `waitpid(pid, WNOHANG)`; `if (res_pid != 0) break;` sleep 10 ms }), at 20abdb50a
- defect:          the loop's only exit is reaping the child. A child that survives SIGHUP keeps it spinning: one that ignores or handles SIGHUP and does not exit, or a macOS `login(1)` session leader wedged in the kernel exit path (state `?Es`, SIGKILL-immune) while output is still queued on the pty with no reader.
- ReleaseSafe:     the thread that stops the subprocess spins forever (10 ms sleeps); the surface never finishes closing. By reading.
- ReleaseFast:     the same.
- reachability:    ordinary use: a `command` whose process ignores SIGHUP (the port's test uses `SIG_IGN` inherited across `exec`). The `?Es` wedge was reproduced on the port side, 12/12, at an older pin (`.port/tools/repro_subprocess_hang.sh`); it was not re-measured at this pin (UNVERIFIED at the pin). Its trigger is UD102.
- port handling:   `crates/rustty-termio/src/exec.rs` bounds the same loop with `KILL_ABANDON` (10 s) and returns `KillError::Abandoned`, which `stop` treats as non-fatal; the child is left running, never SIGKILLed (the rationale is in the `[R1-DIVERGENCE: bounded kill wait]` note). The deadline is a parameter of `kill_pid_until` (production passes `KILL_ABANDON`). PORT NOTE at `exec.rs:3952`.
- parity row(s):   `.port/phase-2/DEFERRALS.md` "killPid bounded wait"; inventory M13; LEDGER S211-F4 (closed by T-G, 2026-09-23, which re-ran the test and the negative control below on macOS with the same result).
- test:            `kill_pid_abandons_a_child_that_ignores_sighup` (rustty-termio, macOS and the VM): a real child in its own session that ignores SIGHUP from before `exec`; a 300 ms deadline returns `Abandoned` after at least 300 ms with the child still alive; a 10 s watchdog SIGKILLs the child if the loop is stuck. Negative control, macOS 2026-09-22: the deadline check disabled (the reference's loop) fails with "killPid did not return within 10 s for a child that ignores SIGHUP". Restored.
- proposed fix:    bound the loop (the port abandons after 10 s without escalating), and drain the pty before stopping on the `threadEnter` error path (UD102).
- upstream status: not reported

### UD94 — `+list-themes` indexes an empty theme list after a search that matches nothing
- kind:            crash (safety-checked trap) / undefined behaviour
- upstream project: ghostty
- severity:        low (interactive CLI)
- reference:       `src/cli/list_themes.zig:470` (`self.current = self.filtered.items.len - 1;`, the End / `kp_end` / `G` arm of `update`; the same expression at `:443` is inside `down`, which returns early on an empty list at `:437`), `:486` and `:492` (`self.themes[self.filtered.items[self.current]]` for `c` and `C`), `:461-462` (Enter / `kp_enter` in normal mode sets `.save`) and `:838-839` (the `.save` arm of `draw`: `const theme = self.themes[self.filtered.items[self.current]];`, after `draw` has reset `current = 0` for an empty list at `:656-658`), `:334-338` and `:414` (`updateFiltered`: `var selected: []const u8 = undefined;`, set only when the old list is non-empty, then `if (selected.len == 0)`), at 20abdb50a
- defect:          after a search that matches nothing, `filtered.items` is empty: End computes `0 - 1` on a `usize`; `c`/`C` index `filtered.items[current]` out of bounds; Enter switches to `.save`, and the next `draw` indexes `filtered.items[0]` on the empty list in the save pane; and a later `updateFiltered` that finds matches reads the `undefined` `selected`. (`down` and `saveSelectedTheme`, `:1769`, check for an empty list; the search arm only calls `updateFiltered`.)
- ReleaseSafe:     End: panic "integer overflow"; `c`/`C` and the save-pane draw: panic "index out of bounds". By reading (the same trap shapes are measured elsewhere in this file).
- ReleaseFast:     End wraps `current` to `maxInt(usize)`; `c`/`C` and the save-pane draw read past the list (undefined behaviour); `selected.len` reads uninitialised memory.
- reachability:    interactive: in `+list-themes`, type a search that matches nothing, leave the search (Escape or Enter), then press `c`, `C`, End, or Enter (Enter opens the save pane, whose draw traps). The list can also be empty with no search: `--color=light` or `--color=dark`, or the `f` filter, when no theme has that colour scheme; then a single Enter, End, `c` or `C` traps (unlikely with the bundled themes; review of task R13-D2).
- port handling:   `crates/rustty-cli/src/list_themes.rs`: `c`/`C` do nothing on an empty list (nothing is selected), End wraps harmlessly (every index is guarded and `draw` resets `current`), the save pane omits the `theme = …` line on an empty list (the rest of its instructions still draw), and `selected` is an `Option`. PORT NOTEs at `list_themes.rs:921`, `:966`, `:1417`, `:2373`.
- parity row(s):   LEDGER S69-F7 (closed by task R13-D); inventory M25.
- test:            `keys_on_a_filter_that_matches_nothing_do_not_index_it` (rustty-cli, macOS and the VM). `update` needs the real `Tty`, which opens `/dev/tty`, so the test re-executes its own binary with a fresh pty as the controlling terminal; the child drives End (asserting `current` wrapped), `c`, `C`, the four navigation keys, a draw, and a search clear that must list both themes with `current` 0, then prints a marker the parent requires. Negative control, macOS 2026-09-22: the `c` guard removed makes the child panic "index out of bounds: the len is 0 but the index is 18446744073709551615". Restored. The save-pane draw is covered by `draw_smoke_save_empty_guard` (rustty-cli): `draw` in `.save` mode on an empty list renders the save instructions without panicking. Negative control, macOS 2026-09-22 (task R13-D2): the save-pane guard removed fails that test with "index out of bounds: the len is 0 but the index is 0" at the `filtered[current]` index. Restored.
- proposed fix:    return early from the `c`/`C`/End handlers when `filtered.items.len == 0`; in the `.save` arm of `draw`, skip the theme line when the list is empty (or do not enter `.save` from an empty list); and initialise `selected` to `""`.
- upstream status: not reported

### UD95 — `click-repeat-interval` above 4294 ms overflows the `u32` nanosecond product
- kind:            crash (safety-checked trap) / wrong value
- upstream project: ghostty
- severity:        low (a config value)
- reference:       `src/Surface.zig:395` (`.mouse_interval = config.@"click-repeat-interval" * 1_000_000`), field `mouse_interval: u64` at `:315`; `click-repeat-interval: u32` (`src/config/Config.zig:2578`), at 20abdb50a
- defect:          the product is computed in `u32` (the operand's type) and only then widened to the `u64` field.
- ReleaseSafe:     panic "integer overflow" at surface creation for any value above 4294 (measured, Zig 0.16.0, the expression alone).
- ReleaseFast:     wraps: 5000 ms becomes 705032704 ns (measured), a 0.7 s repeat interval.
- reachability:    config: `click-repeat-interval = 5000`.
- port handling:   `crates/rustty-core/src/surface.rs` widens before multiplying (`u64::from(…) * 1_000_000`). PORT NOTE at `surface.rs:1273`.
- parity row(s):   LEDGER S40-F1 (closed by task R13-D); inventory M26.
- test:            `a_click_repeat_interval_above_u32_nanoseconds_is_widened_not_trapped` (rustty-core): 500 ms (positive control), 5000 ms and `u32::MAX` ms through the config loader into a real `DerivedConfig`. Negative control, macOS 2026-09-22: the product computed in `u32` fails "attempt to multiply with overflow". Restored.
- proposed fix:    `@as(u64, config.@"click-repeat-interval") * std.time.ns_per_ms`.
- upstream status: not reported

### UD96 — `quick-terminal-size` accepts percentages whose pixel size traps, and pixel sizes that do not fit `c_int`
- kind:            crash (safety-checked trap)
- upstream project: ghostty
- severity:        low (a config value)
- reference:       `src/config/Config.zig:9457-9465` (`Size.toPixels`: `return @intFromFloat(v / 100.0 * dim);`), `:9467-9490` (`Size.parse`: `parseFloat`, rejects only `percentage < 0`); `src/apprt/gtk/winproto/wayland.zig:441-449` (`window.setDefaultSize(@intCast(dims.width), @intCast(dims.height))` into `c_int`), at 20abdb50a
- defect:          the parser accepts `inf%`, `1e10%` and `nan%` (`nan < 0` is false), and a pixel size above `maxInt(c_int)`. The percentage conversion and the `c_int` cast are both unchecked.
- ReleaseSafe:     `inf%` or `1e10%`: panic "integer part of floating point value out of bounds"; `3000000000px`: panic "integer does not fit in destination type" in the Wayland quick terminal (all measured, Zig 0.16.0, the expressions alone). `nan%` gives 0 without a trap.
- ReleaseFast:     `inf%`/`1e10%` give 4294967295, `nan%` 0; `3000000000px` becomes -1294967296, which GTK rejects: `gtk_window_set_default_size: assertion 'width >= -1' failed`, the size is left unchanged (measured, GTK 4.22 under headless sway, Python GI).
- reachability:    config: `quick-terminal-size = inf%` (or `3000000000px`) on the GTK Wayland quick terminal. Whether the macOS quick terminal has the same conversion is UNVERIFIED (not read).
- port handling:   `crates/rustty-config/src/value_types.rs` `to_pixels` converts with Rust's saturating `as`, which yields the reference's ReleaseFast values without the trap; the Wayland `set_default_size` casts with `as i32`, which also gives the reference's ReleaseFast value, so a pixel size above `i32::MAX` draws the same GTK critical and is ignored (not a crash; not changed by this task). PORT NOTE at `value_types.rs:6994`.
- parity row(s):   inventory M27.
- test:            `quick_terminal_size_out_of_range_percentages_do_not_trap` (rustty-config): `50%` (positive control), `inf%`, `1e10%` and `nan%` parse as percentages and give 4294967295/4294967295/0, directly and through `calculate`. Negative control, macOS 2026-09-22: the reference's range check reinstated fails "integer part of floating point value out of bounds". Restored. The `c_int` cast is not driven by a test.
- proposed fix:    reject non-finite percentages in `Size.parse`, and use `std.math.lossyCast` (or clamp to the monitor) in `toPixels` and before `setDefaultSize`.
- upstream status: not reported

### UD97 — an `adjust-*` percentage that pushes a metric out of range traps in `Modifier.apply`
- kind:            crash (safety-checked trap)
- upstream project: ghostty
- severity:        low (a config value)
- reference:       `src/font/Metrics.zig:460-478` (`Modifier.parse`: `parseFloat(f64, …)`; any parseable value is accepted, including `inf%`, and a value ≤ -100% becomes `.percent = 0` (`:470`)), `:515-526` (`apply`; the integer `.percent` arm is `:520-526`, `:523-524`: `const applied_f64: f64 = @round(v_f64 * p_clamped);` then `const applied_T: T = @intFromFloat(applied_f64);`), at 20abdb50a
- defect:          the percentage is unbounded, and the product is converted to the metric's integer type unchecked. The `.absolute` arm of the same function saturates on purpose (`:536-537`: `const applied_T: T = std.math.cast(T, clamped_i64) orelse` / `std.math.maxInt(T) * @as(T, @intCast(std.math.sign(clamped_i64)));`), so the percentage arm is the inconsistent one.
- ReleaseSafe:     `adjust-cell-width = inf%` (or `1e300%`): panic "integer part of floating point value out of bounds" (measured, Zig 0.16.0, the expression alone). `nan%` gives 0 without a trap (`@max(0, nan)` is 0, measured).
- ReleaseFast:     the metric becomes the type's maximum (4294967295 for a `u32`, measured).
- reachability:    config: any `adjust-*` metric modifier given as a huge or infinite percentage.
- port handling:   `crates/rustty-font/src/metrics.rs` `apply_u32`/`apply_i32` convert with Rust's saturating `as`, the same values as the reference's ReleaseFast and the same saturation the `.absolute` arm applies. What the rest of the app does with a 4294967295 px cell (the same state `adjust-cell-width = 2147483647` reaches through the `.absolute` arm) is not covered here; see UD59. PORT NOTEs at `metrics.rs:775`, `:804`.
- parity row(s):   inventory M28.
- test:            `Modifier_percent_out_of_range_saturates` (rustty-font): `inf%`, `1e300%` and `nan%` parsed from text give the type's max / max / 0 for `u32` and `i32`, and `Metrics::apply` with `cell_width = inf%` gives 4294967295. Negative control, macOS 2026-09-22: the range check reinstated in `apply_u32` fails "integer part of floating point value out of bounds". Restored.
- proposed fix:    reject non-finite percentages in `Modifier.parse`, and saturate the `.percent` arm as the `.absolute` arm does (`std.math.lossyCast`).
- upstream status: not reported

### UD98 — a `font-variation` value outside the 16.16 range traps in the FreeType face
- kind:            crash (safety-checked trap)
- upstream project: ghostty
- severity:        low (a config value; FreeType builds)
- reference:       `src/font/face/freetype.zig:278-316` (`setVariations`: `coords[i] = @intFromFloat(v.value * 65536);` at `:310`, into `FT_Fixed` = `c_long`); `font-variation` values are parsed with `std.fmt.parseFloat(f64, …)` (`src/config/Config.zig:6441-6445`), at 20abdb50a
- defect:          the parsed value is any `f64`, including `inf`, and `v.value * 65536` is converted to `c_long` unchecked.
- ReleaseSafe:     `font-variation = wght=inf` (or `1e20`): panic "integer part of floating point value out of bounds" (measured, Zig 0.16.0, the expression alone). NaN gives 0.
- ReleaseFast:     the `c_long` maximum (measured).
- reachability:    config: `font-variation` on a FreeType build (Linux; the CoreText face passes the `f64` to CoreText and has no such cast).
- port handling:   `crates/rustty-font-shaper/src/face/freetype.rs` converts with Rust's saturating `as` (the reference's ReleaseFast values). FreeType stores the saturated design coordinate as given, and the face still renders (both measured in the test). PORT NOTE at `freetype.rs:592`.
- parity row(s):   inventory M29.
- test:            `set_variations_with_an_out_of_range_value_saturates` (rustty-font-shaper, FreeType): JetBrains Mono `wght` 600 (positive control), then `inf`, `1e20`, `-inf` and NaN give design coordinates `c_long` max / max / min / 0, and a glyph renders after each. Negative control, macOS 2026-09-22: the range check reinstated fails "integer part of floating point value out of bounds". Restored.
- proposed fix:    clamp the value to the axis range from `mm.axis[i]` (or at least `std.math.lossyCast`) before converting.
- upstream status: not reported

### UD99 — `+crash-report` traps on a crash file whose mtime is more than 584 years old
- kind:            crash (safety-checked trap) / wrong output
- upstream project: ghostty
- severity:        low (a file in the user's crash directory)
- reference:       `src/cli/crash_report.zig:83-91` (`const diff = now - report.mtime;` then `if (diff <= 0) "now" else … @intCast(diff)` into `Config.Duration.duration: u64`), `mtime` from `stat.mtime.toNanoseconds()` (`src/crash/dir.zig:58`, `i128`), at 20abdb50a
- defect:          an age of more than 2^64 ns (about 584.5 years) does not fit the `u64` duration. A future mtime is not affected: it takes the `diff <= 0` "now" arm. (The inventory described the trigger as a future-dated file; that is wrong.)
- ReleaseSafe:     panic "integer does not fit in destination type" (measured, Zig 0.16.0, for an mtime in the year 1400).
- ReleaseFast:     the age wraps: the year-1400 file is listed as about 42 years old (measured: 1330699126290448384 ns).
- reachability:    a file in the crash directory (`$XDG_STATE_HOME/ghostty/crash`) with an mtime before about 1441. Linux tmpfs and btrfs store an mtime in the year 1400 (measured on the VM with `touch -d`); macOS APFS clamps any earlier mtime to 1677 (measured), whose age still fits, so macOS is not affected.
- port handling:   `crates/rustty-cli/src/crash_report.rs` saturates the age to the largest `Duration` ("584y 49w 23h 34m 33s ago", a lower bound on the true age) instead of trapping. The port had truncated with `as u64` (the reference's ReleaseFast value, inventory P3); that is fixed in this task. PORT NOTE at `crash_report.rs:196`.
- parity row(s):   NEW-FINDINGS N70 (the port's truncation); inventory M30, P3.
- test:            `a_report_too_old_for_a_duration_saturates_instead_of_trapping` (rustty-cli) runs the real `run_inner` over a real crash directory with a year-1400, a one-minute-old and a year-2200 report: "now" for the future one, "… ago" for the recent one, and on Linux (where the precondition asserts the filesystem kept the year-1400 mtime) the saturated label for the ancient one. Negative controls, VM 2026-09-22, one at a time: the old `as u64` fails with "42y 10w 3d 3h 51m 6s" against "584y 49w 23h 34m 33s"; the reference's checked cast fails "integer does not fit in destination type". Restored.
- proposed fix:    `const d = Config.Duration{ .duration = std.math.lossyCast(u64, diff) };`.
- upstream status: not reported

### UD100 — grid arithmetic traps on a size or position more than 65535 cells wide
- kind:            crash (safety-checked trap)
- upstream project: ghostty
- severity:        low (embedder sizes and positions)
- reference:       `src/renderer/size.zig:253-261` (`GridSize.update`: `const calc_cols: Unit = @intFromFloat(screen_width / cell_width);`, `Unit` = `CellCountInt` = `u16`), `:138-147` (`Coordinate.convert` to `.grid`: `const col: GridSize.Unit = @intFromFloat(clamped_x / cell_width);`), at 20abdb50a
- defect:          the quotient is converted to `u16` unchecked. A screen or a position more than 65535 cells from the origin, or a zero cell size (`inf`), traps. The conversion's own next line clamps to the grid (`@min(col, grid.columns - 1)`), so a saturating conversion is the evident intent.
- ReleaseSafe:     panic "integer part of floating point value out of bounds" (measured, Zig 0.16.0: 100000 px of 1 px cells, and a zero cell width).
- ReleaseFast:     100000 wraps to 34464 columns; `inf` gives 65535 (measured).
- reachability:    the C ABI (`ghostty_surface_set_size`, `ghostty_surface_mouse_pos`) and the apprt: a size or mouse position far outside any real screen, or a zero cell size. Whether a real host produces one is UNVERIFIED.
- port handling:   `crates/rustty-render-types/src/lib.rs` converts with Rust's saturating `as`; the grid-position path then clamps to the grid as the reference does. PORT NOTEs at `lib.rs:392`, `:549`.
- parity row(s):   inventory M31.
- test:            `grid_arithmetic_outside_u16_saturates` (rustty-render-types): 80x30 (positive control); 100000 px of 1 px cells gives 65535 columns, a zero cell gives 65535x65535; mouse positions `1e12` and `inf` clamp to the last cell and NaN to the first. Negative controls, macOS 2026-09-22, one at a time: the range check reinstated in `update`, then in the grid conversion, each fails "integer part of floating point value out of bounds". Restored.
- proposed fix:    `std.math.lossyCast(Unit, …)` in both places.
- upstream status: not reported

### UD101 — `log.err` between `fork` and `exec` can deadlock the child
- kind:            deadlock (the child hangs before `exec`)
- upstream project: ghostty
- severity:        low (thread timing, plus a pre-exec failure)
- reference:       `src/termio/Exec.zig:1041-1051` (`os_pre_exec` callback: `sp.childPreExec() catch |err| log.err("error initializing child: {}", .{err});` at `:1045`); `src/apprt/gtk/pre_exec.zig:40` and `:56` (`log.err` in `preExec`, the cgroup move; older records cite `:39,58`); both run in the forked child through the global `logFn`, which takes `std.debug.lockStderr` (`src/main_ghostty.zig:158-161`), at 20abdb50a
- defect:          after `fork` only the forking thread exists in the child. If another thread of the (multi-threaded) parent held the stderr lock, or an allocator lock the formatting needs, at the moment of the fork, the child blocks on it forever. POSIX allows only async-signal-safe calls between `fork` and `exec`.
- ReleaseSafe:     the child hangs before `exec`; the terminal shows no process. By reading; not reproduced against the Zig logger. Whether Zig's stderr lock or allocator registers an `atfork` handler that would prevent this is UNVERIFIED.
- ReleaseFast:     the same.
- reachability:    thread timing, when `childPreExec` fails, or on the Linux cgroup path (`linux-cgroup` config) when the `gtk-single-instance = detect` arm is taken (`:40`), or when the move takes over 250 ms with `linux-cgroup-hard-fail` set (`:55-56`; without it the loop `break`s silently).
- port handling:   both sites write the pre-rendered line with a raw `write(2)` on a stack buffer (`log_err_child_raw` in `crates/rustty-termio/src/exec.rs`, `log_err_raw` in `crates/rustty-apprt/src/gtk/pre_exec.rs`), the same bytes `logFn` would write. PORT NOTEs at `exec.rs:2080`, `pre_exec.rs:339`.
- parity row(s):   ADJUDICATION fork/exec sections (tasks 14d-3, 14e-1; no reversal); UD candidate C4; inventory M32.
- test:            `log_err_child_raw_completes_in_a_child_forked_while_stderr_is_locked` (rustty-termio, macOS and the VM): a thread holds std's stderr lock across a real `fork`, the child runs `log_err_child_raw` and must `_exit(0)` within 5 s. Negative control, macOS 2026-09-22: the helper's `write(2)` replaced by a write through `std::io::stderr().lock()` hangs the child ("the forked child hung on the stderr lock another thread held at the fork"). Restored. The existing `log_err_child_raw_writes_exactly_what_logfn_would` pins the bytes.
- proposed fix:    format into a stack buffer and `write(2)` it in the child (as the port does), or pass the error back to the parent through the existing exec-failure pipe.
- upstream status: not reported

### UD102 — `threadEnter`'s error path stops a freshly spawned child that has no pty reader
- kind:            deadlock (with UD93)
- upstream project: ghostty
- severity:        low (a `threadEnter` failure after the spawn)
- reference:       `src/termio/Exec.zig:86-104` (`threadEnter`: `errdefer self.subprocess.stop();` at `:104`), followed by the fallible `xev.Process.init` (`:107-109`), and the later pipe, timer and read-thread spawn; the read thread is only started at the end of `threadEnter`, at 20abdb50a
- defect:          every error after the spawn runs `stop()` while nobody reads the pty master. A SIGHUP'd `login(1)` with output still queued on the slave then wedges in the kernel exit path (`?Es` on macOS), and `stop()`'s `killPid` waits for it forever (UD93).
- ReleaseSafe:     the io thread hangs in `killPid`. The mechanism was reproduced on the port side at an older pin (records: "verified twice"; the 12/12 repro of UD93); it was not re-measured at this pin (UNVERIFIED at the pin).
- ReleaseFast:     the same.
- reachability:    a `threadEnter` failure after the child is spawned: `xev.Process.init`, pipe or timer creation, or the read-thread spawn (fd or thread exhaustion).
- port handling:   `crates/rustty-termio/src/exec.rs` drains the pty master once, bounded (`drain_pty_master_once`, `DRAIN_BUDGET` 50 ms), before `stop()` on each of these error arms, and `killPid` is bounded anyway (UD93). PORT NOTE at `exec.rs:3860`.
- parity row(s):   `.port/phase-3/assessments/E-os-termio-platform.md` §5.3; inventory M33; LEDGER S211-F5 (closed by T-G, 2026-09-23).
- test:            `drain_pty_master_once_consumes_pending_and_returns_on_a_quiet_master` (rustty-termio) proves the drain; the error arms themselves are not driven (they need a failing `xev.Process.init` or thread spawn). No new test in this task. ADDED by T-G (2026-09-23): `exec::tests::threadenter_read_thread_spawn_failure_drains_before_stop` drives the read-thread spawn arm for real (a test-only switch gives the spawn a stack no thread can get) with a `/bin/sh` child that writes to the pty and keeps running, and asserts `threadEnter` fails, the drain ran once, and `stop()` cleared the process; 0.66 s. Negative control (macOS): that arm's drain removed fails on the drain count, and the run takes 10.03 s, because `killPid` could not reap the undrained (login-wrapped) child and hit `KILL_ABANDON`, the wedge this entry describes, observed at this pin on the port side (one run). AMENDED by T-G2 (2026-09-23): the test also asserts the ORDER, which is the fix: `#[cfg(test)]` thread-locals count `Subprocess::stop` calls (`STOP_CALLS`) and record that count when `drain_pty_master_once` runs (`STOPS_BEFORE_DRAIN`); the test requires `stop()` called once and the drain to have seen 0 stops. Negative control (macOS, 2026-09-23): the drain moved after `self.subprocess.stop()` in the read-thread spawn arm (the review's U1, which the count-only test let pass) fails on "the drain runs before subprocess.stop(), not after it" (`Some(1)` vs `Some(0)`); that run also took 10.03 s (the wedge).
- proposed fix:    start the reader before anything that can fail after the spawn, or drain (or close) the pty master before `stop()` on the error path; and bound `killPid` (UD93).
- upstream status: not reported

### UD103 — a closed window's pending tab-overview focus timer fires on the freed window
- kind:            ub (use after free)
- upstream project: ghostty
- severity:        low (a window closed within 500 ms of leaving the tab overview)
- reference:       `src/apprt/gtk/class/window.zig:1397-1406` (`dispose` removes only `handle_active_state_source`), `:267` (`tab_overview_focus_timer: ?c_uint`), `:1543-1553` (`tabOverviewOpen` arms `glib.timeoutAdd(500, tabOverviewFocusTimer, self)`), `:1556-1571` (`tabOverviewFocusTimer` casts the userdata back to `*Self` and uses it), at 20abdb50a
- defect:          `dispose` does not remove the 500 ms focus timer, and the timer's userdata is the raw window pointer. If the window is finalized first, the timer runs on freed memory.
- ReleaseSafe:     a use after free when the timer fires (no safety check covers a dangling userdata). By reading; not reproduced against the reference.
- ReleaseFast:     the same.
- reachability:    ordinary use: close a window within 500 ms of closing its tab overview.
- port handling:   `crates/rustty-apprt/src/gtk/class/window.rs` `dispose` also removes `tab_overview_focus_timer` (invariant 11: a source lives no longer than its object). PORT NOTE at `window.rs:336` (updated to cite R13).
- parity row(s):   LEDGER S58-F5 (WAIVED, re-grounded on R13); inventory M34.
- test:            `window_dispose_removes_source_ids` (rustty-apprt, display-gated; run on the VM under a private headless sway): arms both timers, finalizes the window, and asserts neither source is still registered. Negative control, VM 2026-09-22: the focus-timer removal deleted from `dispose` fails "dispose must remove tab_overview_focus_timer (invariant 11)". Restored.
- proposed fix:    in `dispose`, `if (priv.tab_overview_focus_timer) |v| { _ = glib.Source.remove(v); priv.tab_overview_focus_timer = null; }`.
- upstream status: not reported

### UD104 — `Surface.init`'s error path frees the renderer and io state while their threads still run
- kind:            ub (use after free / data race)
- upstream project: ghostty
- severity:        low (a thread-spawn failure during surface creation)
- reference:       `src/Surface.zig:582` (`errdefer render_thread.deinit();`), `:586` (`errdefer io_thread.deinit();`), the `self.io` errdefer in the io block; the renderer thread spawned at `:725-730`, then `try std.Thread.spawn` for the io thread at `:733-737`, then the `try rt_app.performAction(.set_title …)` calls at `:748-795` (first at `:754`), at 20abdb50a
- defect:          once the renderer thread is running, a failure of the io thread's spawn (or of a later `try`) unwinds through errdefers that deinit the renderer thread's loop and the terminal it reads, without stopping or joining the thread.
- ReleaseSafe:     a use after free on the renderer thread. By reading; not reproduced.
- ReleaseFast:     the same.
- reachability:    `std.Thread.spawn` failing for the io thread (thread or memory exhaustion). The `set_title` action cannot fail in either upstream apprt at the pin (GTK `setTitle` returns `void`; embedded `performAction` only calls the host), so the spawn is the practical trigger.
- port handling:   `crates/rustty-core/src/surface.rs` `initErrdefers` now stops and joins the renderer and io threads first, in `deinit`'s order, before any teardown. **The port was not safe**: it joined the renderer only after `self.io.deinit()` (so a failed io spawn freed the terminal under the running renderer), and it never stopped the io thread. Fixed by task R13-D. PORT NOTE at `surface.rs:3020` (task R13-D2 adds that, unlike `deinit` at `Surface.zig:811`, this path does not call `renderer.threadEnter` before `renderer.deinit()`, which is harmless at the pin: OpenGL's `threadEnter` does nothing for GTK or embedded, and Metal defines none).
- parity row(s):   NEW-FINDINGS N72 (the port's own race); inventory M35.
- test:            `init_error_path_joins_both_threads_before_freeing_their_state` (rustty-apprt-c, `tests/macos_window_host.rs`, macOS, added by task R13-D2). Task R13-D recorded "none: `Surface::init` cannot run in a unit test"; that was wrong, since six non-ignored tests in that file already run the full `rustty_surface_new` → `Surface::init` with both threads spawned. What was missing was a way to fail the io-thread spawn. The test uses rustty-core's test-only `init-fault-inject` feature (`src/init_fault.rs`, enabled only from rustty-apprt-c's `[dev-dependencies]`; shown absent from `cargo build -p rustty` and from the `librustty_apprt_c` dylib and staticlib), armed per thread. It runs two cases in child processes through the real `rustty_surface_new`: the io spawn fails (renderer running), and the step after it fails where the `set_title` `try` sits (both running). It requires the exact sequence: every spawned thread stopped and joined, then the frees. At each free a probe checks that no spawned thread is still unjoined and, if one is, prints why and `_exit`s before the free, so a violation is reported without touching freed memory and the parent's assertion fails instead of the runner crashing. Negative controls, macOS 2026-09-22 (task R13-D2), one at a time: the pre-R13-D order (the renderer joined only before its own deinit, the io thread never stopped) fails "io-spawn: Surface::init's error path freed state under a running thread: UD104 probe: `io` is about to be freed while the renderer thread is still running"; the io join alone removed fails the same way for the `after-io-spawn` case ("… while the io thread is still running"). Restored.
- proposed fix:    stop and join each spawned thread in an `errdefer` placed right after its spawn (`errdefer { self.renderer_thread.stop.notify() catch {}; self.renderer_thr.join(); }`), before the deinit errdefers run.
- upstream status: not reported

### UD105 — a window's pending activation token outlives the window and calls back into it
- kind:            ub (use after free) + leak
- upstream project: ghostty
- severity:        low (a window closed while an urgency request is pending)
- reference:       `src/apprt/gtk/winproto/wayland.zig:216-221` (`Window.deinit`: destroys `bg_effect`, releases `decoration` and `slide`, and leaves `activation_token`), `:261-274` (`setUrgent`: `token.setListener(*Window, onActivationTokenEvent, self)`), `:450-472` (`onActivationTokenEvent` reads `self.globals` and `self.activation_token`), at 20abdb50a
- defect:          a token requested by `setUrgent(true)` whose `done` has not arrived when the window is torn down keeps the window as its listener userdata. The late `done` runs `onActivationTokenEvent` on the freed window, and the token proxy leaks.
- ReleaseSafe:     a use after free when the reply arrives. By reading against the reference; the late reply itself is measured (see test).
- ReleaseFast:     the same.
- reachability:    ordinary use under Wayland: a window closed right after it asked for attention (e.g. a bell with urgency), before the compositor answered.
- port handling:   `crates/rustty-apprt/src/gtk/winproto/wayland.rs` `deinit` destroys a pending token, so libwayland drops the late reply. PORT NOTE at `wayland.rs:707`. (The inventory's other half, a `wl_region` never destroyed, is REFUTED at the pin: `syncBlur` has `defer wl_region.destroy()` at `:308-309`.)
- parity row(s):   LEDGER S60-F2 (WAIVED, re-grounded on R13), S60-F3; inventory M36.
- test:            `a_pending_activation_token_is_destroyed_with_its_window` (rustty-apprt, `tests/gtk_wayland_deinit_token.rs`, headed; run on the VM under a private headless sway): a positive control first (with the window alive, the compositor's `done` arrives); then `set_urgent(true)` immediately followed by `deinit`, and no `done` may be dispatched within 3 s. Negative control, VM 2026-09-22: the token destroy removed from `deinit` fails with "a `done` reached the token of a window that was already torn down (before=1, after=2)". Restored.
- proposed fix:    in `Window.deinit`, `if (self.activation_token) |token| token.destroy();`.
- upstream status: not reported

### UD106 — the wasm `face_new` export narrows its presentation argument into `enum(u1)` unchecked
- kind:            latent (unbuilt code)
- upstream project: ghostty
- severity:        low
- reference:       `src/font/face/web_canvas.zig:494-511` (`face_new(ptr, len, pts, p: u16)` → `face_new_` → `@enumFromInt(presentation)` into `font.Presentation = enum(u1)`), at 20abdb50a
- defect:          any value other than 0 or 1 is illegal behaviour. The only caller is the JS glue, which passes 0 or 1. The file is not compiled by any build step at the pin (UD37, UD38), so this is not reachable today.
- ReleaseSafe:     panic "invalid enum value" (measured, Zig 0.16.0, the expression alone).
- ReleaseFast:     2 truncates to `.text` (measured).
- reachability:    none today (the wasm font tree is not built); the JS glue if it were.
- port handling:   `crates/rustty-font-shaper/src/face/web_canvas.rs` maps 0 to text and any other value to emoji, which agrees with the reference on both values it defines. The export's own error path (`catch null`) would be the stricter handling; not changed. PORT NOTE at `web_canvas.rs:934`.
- parity row(s):   LEDGER S91-F6 (WAIVED, re-grounded on R13); inventory M38.
- test:            none (no reference behaviour to diverge from on 0/1).
- proposed fix:    `std.meta.intToEnum(font.Presentation, presentation) catch return error.InvalidPresentation`, alongside UD37/UD38.
- upstream status: not reported

### UD107 — libvaxis queues key text that points into the read thread's 8 KiB ring
- kind:            ub (data race / dangling slice)
- upstream project: libvaxis
- severity:        low (keyboard input in the CLI TUIs)
- reference:       libvaxis 0.6.0 (the version Ghostty pins) `src/Loop.zig:152` (`var cache: GraphemeCache = .{};` on the read thread's stack in `_ttyRun`), `:240`, `:250`, `:306`, `:316` (`mut_key.text = cache.put(text);` then `postEvent`); `src/GraphemeCache.zig:5-21` (an 8 KiB buffer that `put` wraps to index 0 when full)
- defect:          each queued key event's `text` is a slice into the read thread's ring. When the read thread has put another 8 KiB through the ring before the main thread handles an event, the event's text is overwritten while it is read (a data race); after `stop` the ring's stack frame is gone and any event still queued points into it.
- ReleaseSafe:     wrong key text or a read of a dead stack frame; no safety check covers either. By reading; not reproduced against libvaxis.
- ReleaseFast:     the same.
- reachability:    more than 8 KiB of key text queued faster than the TUI consumes it (for example a large paste without bracketed paste into `+list-themes`' search while it reloads themes). Realistic timing UNVERIFIED.
- port handling:   `crates/rustty-vaxis/src/loop_.rs` copies the text into the queued event (`KeyOwned`, `Vec<u8>`); the `LoopEvent: Send + 'static` bound rejects any event type that borrows the read thread, so the reference's shape cannot be written in safe Rust. PORT NOTE at `loop_.rs:447` (module note at `:32-41`).
- parity row(s):   inventory M39.
- test:            `queued_key_text_is_owned_not_a_view_of_the_read_buffer` (rustty-vaxis): 100 key events of 100 bytes each (more than 8 KiB) are posted through `handle_event_generic` from one buffer that is rewritten after each post, as the ring would be, and every queued event must still hold its own text. The reference's borrow cannot be reinstated in safe Rust (compile error); as a sensitivity check, dropping the text on post fails "event 0 kept its own text".
- proposed fix:    copy the text into the event (allocate, or give each event its own small inline buffer), or have the consumer own the cache.
- upstream status: not reported

### UD108 — libvaxis's `Loop.should_quit` is a plain `bool` shared between threads
- kind:            ub (data race)
- upstream project: libvaxis
- severity:        low
- reference:       libvaxis 0.6.0 `src/Loop.zig:25` (`should_quit: bool = false`), written by `stop` on the caller's thread (`:66`, `:73`), read by the read thread's loop conditions (`:157`, `:175`)
- defect:          an unsynchronized write and read from two threads. In practice the read thread is woken by the device status report `stop` sends, but nothing orders the store before that read.
- ReleaseSafe:     a data race; no safety check covers it. By reading.
- ReleaseFast:     the same; the compiler may hoist the load out of the loop.
- reachability:    every TUI stop.
- port handling:   `crates/rustty-vaxis/src/loop_.rs` uses an `AtomicBool`. PORT NOTE at `loop_.rs:129`.
- parity row(s):   inventory M40.
- test:            none beyond `loop_lifecycle`; the property is by construction (an atomic).
- proposed fix:    `should_quit: std.atomic.Value(bool)`.
- upstream status: not reported

### UD109 — the fontconfig `fc-match` test destroys every pattern twice
- kind:            test-weakness (a double free in test code)
- upstream project: ghostty
- severity:        low (test code only)
- reference:       `pkg/fontconfig/test.zig:24-66` (`test "fc-match"`: `defer fs.destroy();` then `defer for (fs.fonts()) |font| font.destroy();` at `:39-40`); `FontSet.destroy` is `FcFontSetDestroy` and `Pattern.destroy` is `FcPatternDestroy` (`pkg/fontconfig/font_set.zig:11-13`, `pattern.zig:21-23`), at 20abdb50a
- defect:          `FcFontSetDestroy` already destroys each pattern in the set. The deferred loop runs first (LIFO) and frees every pattern; `fs.destroy()` then destroys them again.
- ReleaseSafe:     a double free inside fontconfig. **Re-verified at the pin** (task R13-D) with the same call sequence in the port's test: glibc aborts with "corrupted size vs. prev_size while consolidating" (SIGABRT), Fedora 44 aarch64, fontconfig 2.17.0. The Zig test itself was not run on this host (no Zig on the VM); the call sequence is the one the wrappers above produce.
- ReleaseFast:     the same.
- reachability:    running the fontconfig package tests.
- port handling:   `crates/rustty-font-shaper/src/fc.rs` `fc_match` destroys the set only (the fontconfig-documented teardown). PORT NOTE at `fc.rs:1274` (the existing UPSTREAM BUG note, P4-U3, now citing R13).
- parity row(s):   `.port/phase-4/UPSTREAM_BUGS.md` P4-U3; inventory M41.
- test:            `fc::tests::fc_match` (rustty-font-shaper). Negative control, VM 2026-09-22: the reference's per-pattern destroy loop reinstated before `fs.destroy()` aborts the test binary with the glibc message above. Restored.
- proposed fix:    delete the `defer for (fs.fonts()) |font| font.destroy();` line.
- upstream status: not reported

### UD110 — CoreText name lookups never release the copied name string
- kind:            leak
- upstream project: ghostty
- severity:        low (the lowest: a retain per `Face.name` call on a face loaded from data, i.e. an embedded font; in a release build `+show-face` is the only caller known to reach it, once per codepoint)
- reference:       `src/font/DeferredFace.zig:143-169` (`familyName`: `ct.font.copyAttribute(.family_name)`), `:173-203` (`name`: `ct.font.copyDisplayName()`); `src/font/face/coretext.zig:204-212` (`Face.name`: `self.font.copyFamilyName()`), at 20abdb50a
- defect:          each is a +1 CoreFoundation "Copy" result, and no path releases it: neither the `cstringPtr` return (which hands out a pointer into the string, so it cannot be released there) nor the `cstring(buf)` copy. The unreleased retain is only a real leak when the name string is not an immortal constant, which in practice means `Face.name` on a face loaded from data.
- ReleaseSafe:     one leaked retain per call on a face loaded from data. **Measured** (macOS 26.5.1, Python ctypes against CoreText): for a font created from data (JetBrains Mono), `CTFontCopyFamilyName`, `CTFontCopyDisplayName` and `CTFontCopyAttribute(family)` return the same string object on every call, and 1000 unreleased copies raise its retain count by 1000. For discovered fonts the names are immortal constants (retain count 1152921504606846975, unchanged after 1000 unreleased copies, the same object each time), so nothing leaks: measured for Menlo, Helvetica, Apple Color Emoji and Arial Unicode MS installed in `/Library/Fonts` (not a system font), created with `CTFontDescriptorCreateWithNameAndSize` then `CTFontCreateWithFontDescriptor` (task R13-D review), and re-measured by task R13-D2 for Menlo and Arial Unicode MS both directly and after `CTFontCreateCopyWithAttributes` (the `Face.initFontCopy` a loaded discovered face goes through).
- ReleaseFast:     the same.
- reachability:    `Face.name` on a face loaded from data. At the pin the only data-loaded CoreText faces are the embedded fonts (`SharedGridSet` built-in fallbacks and the default JetBrains Mono, `Face.init` with `font.embedded.…`); every other face is a discovered font loaded through `Face.initFontCopy`, and `DeferredFace.ct` only ever comes from discovery (`discovery.zig:444`, `:945`), so the `DeferredFace.familyName` / `name` half of this record does not leak in practice. The `Face.name` callers that can see a data-loaded face: `+show-face` when the looked-up codepoint resolves to an embedded face (`show_face.zig:232`, once per codepoint in a short-lived process); the Debug-only variation-axes log (`coretext.zig:124`); and the `getMetrics` CJK-width warning (`coretext.zig:833`), which fires only for a face whose '水' is wider than its advance (not checked for the embedded fonts). Collection's synthetic bold/italic log (`Collection.zig:487`, `:512`) would leak only for a data-loaded regular face, which does not happen at the pin: `completeStyles` runs (`SharedGridSet.zig:258`) before any embedded face is added (`:261`), and returns when there is no regular face (`Collection.zig:342`), so its regular face is always discovered (by reading). Fallback discovery, `+list-fonts` and the `SharedGridSet` discovery log (`:201`, `:232`) name discovered faces only, so they do not leak.
- port handling:   `crates/rustty-font-shaper/src/deferred_face.rs` and `face/coretext.rs` hold each copy in a `CFRetained`, copy the bytes into `buf`, and release it at scope end (correct whether or not the string is immortal). The cost is that a name longer than `buf` is an error rather than a zero-copy pointer (LEDGER S85-F6 / S90-F3). PORT NOTEs at `deferred_face.rs:320`, `:367`, `face/coretext.rs:640`. CORRECTION (task T-F, 2026-09-23, closing S85-F6 / S90-F3): the port now copies each name once per face, holds the copy in the face (`Face.family_name`, `CoreTextData.family_name_str` / `display_name_str`) and releases it with the face; the lookups return the `cstringPtr` pointer into that copy, as the reference does, and fall back to `cstring(buf)`. Still one retain per face, never one per call; the test below now checks exactly that (first call +1, 100 more +0, and the retain returns when the held copy is dropped).
- parity row(s):   LEDGER S85-F6, S90-F3 (narrowed to the `buf`-length contract by task R13-D); UD candidate C5; inventory M42.
- test:            `coretext_name_lookups_release_the_copied_name_string` (rustty-font-shaper, macOS), on the embedded JetBrains Mono loaded from data: preconditions that each copy returns the one object held and that it is not immortal; then 100 calls each of `DeferredFace::family_name`, `DeferredFace::name` and the CoreText `Face::name` leave the retain counts unchanged. The `Face::name` half is the reachable leak. The `DeferredFace` half builds a data-backed `DeferredFace` the reference never produces (only there is a leaked retain visible); it proves the port releases its copies, not that the reference's discovered `DeferredFace` leaks. Negative controls, macOS 2026-09-22, one at a time: an extra unreleased retain in `family_name`, in `name`, and in `Face::name` each fail with a count of 102 against 2 at that assertion. Restored.
- proposed fix:    `defer family_name.release();` after copying into `buf`, and return the `cstring(buf)` copy on every path (the `cstringPtr` fast path cannot outlive the release); or return an owned string the caller releases.
- upstream status: not reported

### UD111 — a built-in fallback face leaks when `Collection.add` fails
- kind:            leak
- upstream project: ghostty
- severity:        low
- reference:       `src/font/SharedGridSet.zig:261-405` (every built-in fallback face is built inline: `_ = try c.add(self.alloc, try .init(self.font_lib, font.embedded.…, …), .{…})`); `src/font/Collection.zig:107-108` ("If no error is encountered then the collection takes ownership of the face"), `:122-124` (`CollectionFull`), `:139` (`SetSizeFailed`), and `:142` (`list.append`, `OutOfMemory`), at 20abdb50a
- defect:          `add` keeps its documented contract (on error the caller still owns the face), but these callers hold no reference to the temporary face, so when `add` fails nothing deinits it: the FT face or CTFont, its HarfBuzz font, and on FreeType the mutex leak.
- ReleaseSafe:     a leak per failing add, then the grid key build fails. By reading.
- ReleaseFast:     the same.
- reachability:    `SetSizeFailed` from `setSize` at the configured size (whether a finite `font-size` can make FreeType or CoreText fail here is UNVERIFIED), `CollectionFull` (about 8190 faces in one style, unrealistic), or allocation failure.
- port handling:   `crates/rustty-font-shaper/src/collection.rs` `add` takes the face by value and deinits it on both error arms before returning (W9F-02). PORT NOTE at `collection.rs:208`.
- parity row(s):   inventory M92 (resolved: the leak is at these callers, not in `add`); R13-G part C, C2.
- test:            `add_set_size_failure_deinits_face` and the `CollectionFull` test beside it (rustty-font-shaper, existing W9F-02 tests with a deinit probe and a `set_size` fail-point). Negative control, macOS 2026-09-22: the `SetSizeFailed` arm's `deinit` replaced by `mem::forget` fails the deinit count (0 against 1). Restored.
- proposed fix:    bind each face to a local with `errdefer face.deinit()` before `c.add`, in all the built-in fallback adds.
- upstream status: not reported

### UD112 — the Wayland `Globals.init` leaks itself and its registry when a roundtrip fails
- kind:            leak
- upstream project: ghostty
- severity:        low (once per process, on a failing compositor connection)
- reference:       `src/apprt/gtk/winproto/wayland/Globals.zig:75-96` (`init`: `alloc.create(Globals)` at `:78`, `try display.getRegistry()` at `:85`, `return error.RoundtripFailed` at `:87` and `:93`, with no `errdefer`); the caller's `errdefer globals.deinit()` (`wayland.zig:52-53`) only arms after success; `src/apprt/gtk/class/application.zig:359-370` catches the error and continues on the noop winproto, at 20abdb50a
- defect:          on a roundtrip failure the `Globals` allocation and the `wl_registry` proxy are never freed; a failing `getRegistry` (`:85`) likewise leaks the `Globals` allocation. The leaked `Globals` stays the registry listener's userdata, so later registry events still reach it (no use after free, since it is never freed).
- ReleaseSafe:     a one-time leak. By reading.
- ReleaseFast:     the same.
- reachability:    the compositor: a `wl_display_roundtrip` failure during GTK startup (usually a dead connection).
- port handling:   `crates/rustty-apprt/src/gtk/winproto/wayland.rs` holds `Globals` in a `Box` and returns with `?`, so the box and the event queue are dropped. Whether wayland-rs also frees the client-side `wl_registry` proxy (the interface has no destructor request) is UNVERIFIED. PORT NOTE at `wayland.rs:217`.
- parity row(s):   R13-G part C, C3.
- test:            none (a roundtrip failure cannot be forced in the fake-compositor harness without a new seam).
- proposed fix:    `errdefer alloc.destroy(self);` after the create and `errdefer registry.destroy();` after `getRegistry`.
- upstream status: not reported

### UD113 — libghostty-vt range-checks enum arguments only in safety-checked builds
- kind:            ub (hardening proposal)
- upstream project: ghostty
- severity:        low (a C caller passing an undeclared enum value)
- reference:       `src/terminal/c/terminal.zig:1503-1514` (`scroll_viewport`: `switch (behavior.tag)` on the C-supplied tag, no check at all); the `if (comptime std.debug.runtime_safety) { _ = std.enums.fromInt(E, @intFromEnum(x)) orelse return .invalid_value; }` pattern, e.g. `src/terminal/c/row.zig:70-74`, 28 sites in 15 files under `src/terminal/c/` (counted by `grep`), at 20abdb50a
- defect:          a C enum can carry any integer. The library's own code shows the intended handling of an undeclared value (`return .invalid_value`), but compiles the check out of ReleaseFast, the build it ships, and `scroll_viewport` has no check in any build. The library's code sweep called this a caller contract; it is recorded here as a hardening proposal, because the handling the library already has is one comparison away.
- ReleaseSafe:     the checked functions return `invalid_value`; `scroll_viewport` panics "switch on corrupt value" (measured, Zig 0.16.0, the switch alone).
- ReleaseFast:     undefined behaviour: the probe's `switch` on 99 took an arm that does not exist (returned 100, measured).
- reachability:    a C-ABI embedder passing an undeclared value (a bug, or a header from a newer library version).
- port handling:   every rustty-vt-c export takes such enums as a plain `c_int` and narrows with `from_c_int` (doctrine D11), returning `invalid_value`, or silently returning for `scroll_viewport`, which has no result to report. `scroll_viewport` also carried a `debug_assert!` that mirrored the trap and aborted across `extern "C"` in debug and `release-safe` builds (inventory P4); task R13-D removed it. PORT NOTE at `crates/rustty-vt-c/src/terminal.rs:3511`.
- parity row(s):   NEW-FINDINGS N71 (the port's debug-build abort); `.port/phase-3/README.md` D11; inventory M37, P4.
- test:            `scroll_viewport_invalid_tag` (rustty-vt-c), inverted by this task: `c_int::MAX`, -1 and the first value past `row` must not panic in any build and leave the viewport unchanged, and a declared tag still acts afterwards. Negative control, macOS 2026-09-22: the `debug_assert!` reinstated fails with "corrupt RusttyTerminalScrollViewportTag: 2147483647". Restored.
- proposed fix:    make the existing `std.enums.fromInt(…) orelse return .invalid_value` checks unconditional (drop the `runtime_safety` gate; the cost is one comparison per call), and add one to `scroll_viewport`.
- upstream status: not reported

### UD114 — CoreText `Face.name` traps on a font with no family name
- kind:            crash (safety-checked trap) / undefined behaviour
- upstream project: ghostty
- severity:        low (a font file; see reachability)
- reference:       `src/font/face/coretext.zig:203-212` (`Face.name`: `const family_name = self.font.copyFamilyName();` then `family_name.cstringPtr(.utf8)`); `pkg/macos/text/font.zig:166-168` (`copyFamilyName(self: *Font) *foundation.String` returns `@ptrFromInt(@intFromPtr(c.CTFontCopyFamilyName(…)))`, non-optional), at 20abdb50a
- defect:          `CTFontCopyFamilyName` returns NULL for a font with no family name, but the wrapper's return type is a non-optional pointer, so NULL becomes a `*foundation.String`. `DeferredFace.familyName` (`src/font/DeferredFace.zig:156-158`) asks CoreText the same question through the nullable `copyAttribute(.family_name)` and handles NULL with `orelse return "unknown"`.
- ReleaseSafe:     the `@ptrFromInt(0)` into a non-optional pointer traps ("cast causes pointer to be null"). By reading; the NULL itself is measured: `CTFontCopyFamilyName` is NULL for JetBrains Mono with its `name` table's directory tag renamed, which `Face.init` (`CTFontManagerCreateFontDescriptorFromData`) accepts (macOS 26.5.1, task R13-D review and task R13-D2).
- ReleaseFast:     `cstringPtr` (`CFStringGetCStringPtr`) is called on a null `CFStringRef`: undefined behaviour (CoreFoundation does not accept NULL there). By reading.
- reachability:    a CoreText `Face` for a font with no family name, then any `Face.name` caller: `+show-face` (`show_face.zig:232`), the `getMetrics` CJK-width warning (`coretext.zig:833`), the Debug-only variation-axes log (`coretext.zig:124`). Collection's synthetic bold/italic log (`Collection.zig:487`, `:512`) is NOT reachable: it names the regular face at `completeStyles` time, which is always found by family-name discovery (which cannot match a font whose family name is NULL), and `completeStyles` runs before any fallback face is added (review of task R13-D2). Whether such a font reaches a CoreText `Face` in practice is UNVERIFIED. What is established (task R13-D2): the data-loaded faces are only the embedded fonts, which have names; everything else is discovered. A nameless font registered for the process (`CTFontManagerRegisterFontsForURL`, process scope) is registered and listed by `CTFontCollectionCreateFromAvailableFonts` with a NULL family name (measured, macOS 26.5.1), so an embedding app that registers fonts for its process could expose one to fallback discovery. A family-name search cannot match it (it has no family), and whether a user-installed nameless font is served by the system the same way was not measured (installing one changes system state).
- port handling:   `crates/rustty-font-shaper/src/face/coretext.rs` `Face::name` calls `CTFontCopyFamilyName` through a nullable binding and returns "unknown" on NULL, the reference's own answer for the same NULL in `DeferredFace.familyName`. The port used objc2's non-null `CTFont::family_name()`, which panicked ("function was marked as returning non-null, but actually returned NULL"). PORT NOTE at `face/coretext.rs:646`.
- parity row(s):   NEW-FINDINGS N74; LEDGER S85-F2 (the `DeferredFace` half, fixed by task R13-C).
- test:            `a_face_without_a_family_name_is_unknown` (rustty-font-shaper, macOS): positive control, the embedded JetBrains Mono's `Face::name` is "JetBrains Mono"; then the S85-F2 fixture (the same font with its `name` table's tag renamed) loads, a precondition asserts `CTFontCopyFamilyName` really returns NULL for it, and `Face::name` returns "unknown". Negative control, macOS 2026-09-22 (task R13-D2): objc2's `family_name()` restored panics "function was marked as returning non-null, but actually returned NULL" (`objc2-core-text` `CTFont.rs:983`). Restored.
- proposed fix:    make `copyFamilyName` return `?*foundation.String` (as `copyDisplayName` already does) and in `Face.name` write `const family_name = self.font.copyFamilyName() orelse return "unknown";` (with the release UD110 proposes).
- upstream status: not reported

### UD115 — a failed `Surface.init` keeps its font-grid reference, and the app's teardown then traps
- kind:            leak / crash (safety-checked trap)
- upstream project: ghostty
- severity:        low (a surface-creation failure, then quitting)
- reference:       `src/Surface.zig:523-526` (`const font_grid_key, const font_grid = try app.font_grid_set.ref(…)` with no `errdefer app.font_grid_set.deref(font_grid_key)`); the only release is in `deinit` (`:840`); `src/App.zig:132-143` (`deinit`: `assert(self.font_grid_set.count() == 0)` at `:141`), reached from `defer app.destroy()` (`src/main_ghostty.zig:101`) and `ghostty_app_free` (`src/apprt/embedded.zig:1707`), at 20abdb50a
- defect:          every `try` in `Surface.init` after the `ref` (`getSize` `:532`, `Renderer.init` `:558`, the thread inits `:574`, `:585`, the io block `:659-681`, the thread spawns `:725`, `:733`, `set_title` `:754`) unwinds without releasing the grid reference, so the grid (its font faces and atlases) is never freed. `setFontSize`, which also takes a reference, has the errdefer (`:2446`), so the omission here is an oversight.
- ReleaseSafe:     the grid leaks; when the app is torn down, `assert(self.font_grid_set.count() == 0)` panics ("reached unreachable code"). The port reached the same state: its `debug_assert!` fired at `rustty_app_free` after a failed `rustty_surface_new` (measured, task R13-D2).
- ReleaseFast:     the grid leaks until the process exits; the assert is compiled out.
- reachability:    any `Surface.init` failure after the `ref`, followed by quitting. The failures are resource exhaustion (fds for the event loops, threads, memory) or a renderer init failure; which one a real host hits is UNVERIFIED.
- port handling:   `crates/rustty-core/src/surface.rs` releases the reference on every error path after the `ref`, in the missing errdefer's LIFO slot (after the renderer teardown, before `derived_config.deinit()`): the five early returns and `initErrdefers`. `App::deinit`'s assertion stays; it now holds. PORT NOTEs at `surface.rs:2176` and `:3099`.
- parity row(s):   NEW-FINDINGS N75.
- test:            `init_error_path_joins_both_threads_before_freeing_their_state` (rustty-apprt-c, the UD104 test) also requires `font_grid_set.count() == 0` on the app after each failed `rustty_surface_new`, before `rustty_app_free`. Negative control, macOS 2026-09-22 (task R13-D2): `initErrdefers` without the release fails "the failed Surface::init released its font-grid reference" (left 0, right 1). Restored. The five early returns before the thread spawns are not driven (no fault hook reaches them).
- proposed fix:    `errdefer app.font_grid_set.deref(font_grid_key);` right after the `ref` at `Surface.zig:523`.

- upstream status: not reported

### UD131 — the row-copy retry loops `@panic` when a destination page's style set cannot grow — reachable on an ORDINARY 80×24 terminal from about 1.2 KB of output (DoS)
- kind:            crash (`@panic`)
- upstream project: ghostty
- severity:        HIGH. About 1.2 KB of ordinary program output crashes Ghostty at a standard terminal size (80×24): about 550 bytes of crafted SGR, 591 line feeds and one DL. That is a denial-of-service reachable from ANY program's output — `cat` of a crafted file, or text arriving over SSH — not only from a program the user runs deliberately. (Task R13-X's original "needs a very wide terminal and about 1 MB of styled output" bound covered only ONE of the style set's two out-of-memory arms; see reachability.)
- reference:       `src/terminal/Screen.zig:747` (`clonePartialRowGrowCapacity`, `@panic("increaseCapacity OutOfSpace")`) and `src/terminal/PageList.zig:5108` (`cloneRowGrowCapacity`, same). The `OutOfSpace` comes from `RefCountedSet.addContext` reporting `OutOfMemory`, which has TWO arms: (a) `psl_stats[31] > 0` — the PSL (probe-sequence-length) overflow at `src/terminal/ref_counted_set.zig:268` ("the hash function is not perfect … we claim to be out of memory"), which fires as soon as ONE item sits 31 slots from its home bucket; and (b) the set genuinely full at the `u16` id ceiling. The style hash is `std.hash.int(wide[0] ^ wide[1])` (`src/terminal/style.zig:664`), unseeded and deterministic, so arm (a) is trivially reachable: 32 foreground colours whose hashes share their low 16 bits stay clustered at every capacity doubling and drive `increaseCapacity(.styles)` from 128 up to 65,535 and then to `OutOfSpace`, on a page holding only ~32 live styles. The loops are entered from: `deleteLines` (`Terminal.zig:3215`) and `insertLines` (`:3047`); `cursorScrollAboveRotate` (`Screen.zig:1201`) via `cursorScrollAbove` (`:1142`); `eraseRow` (`PageList.zig:5194`) via `cursorDownScroll` (`Screen.zig:958`) and `scrollUp` (`Terminal.zig:2728`); `eraseRowBounded` (`PageList.zig:5360`) via the scroll-region slow path (`Screen.zig:1380`) and `Terminal.index` (`:2428`). At 20abdb50a.
- defect:          the loop copies a row into the destination page; on a managed-memory error (`clonePartialRowFrom` returning `StyleSetOutOfMemory` etc.) it grows the page's capacity and retries. When `increaseCapacity(.styles)` cannot make room it returns `error.OutOfSpace` and the loop `@panic`s: the caller has already rotated or shifted rows, so it cannot return an error, and the reference chooses a crash over a half-mutated page list. `increaseCapacity` hits `OutOfSpace` whenever the style set keeps reporting `OutOfMemory` at the ceiling, which happens by EITHER of the two `addContext` arms above: (a) a PSL overflow — 32 hash-colliding styles cluster past probe length 31 at every capacity, so the set claims OOM though it holds only ~32 live styles; or (b) a genuinely full live set at the `u16` id ceiling (65,535). Arm (a) needs no wide terminal and no full set, only 32 specific colours; it is the reachable route. Refcount pinning is not involved in either — the W2 orphaned-pinned-style path (UD80) is a distinct trigger; UD131 fires with a PSL-overflowed set or a genuinely full one (or after the port's UD80 re-clone reclaims nothing).
- ReleaseSafe:     the process aborts with `thread N panic: increaseCapacity OutOfSpace` and a stack trace (SIGABRT). MEASURED, PSL route on an 80×24 terminal (task R13-X3, macOS arm64, scratch `libghostty-vt` ReleaseSafe build, C ABI and a Zig test): `ghostty_terminal_new(NULL, 80, 24)`, scroll until the active area straddles a page boundary, print the 32 colliding colours (see the recipe below) on page 1, put one new-style cell on page 2's first row, then DL (`ESC[2;1H ESC[M`) — @panic at `Screen.zig:747` ← `deleteLines` (`Terminal.zig:3215`). A sweep of `lfs`/`y` positions killed 44 of 29,923 child runs (e.g. lfs=568 y=24, lfs=572 y=20); a narrow sweep killed 8 of 253. Also MEASURED, distinct-live-styles route on a wide terminal (task R13-X): `ghostty_terminal_new(50000, 216)`, print 53,247 cells each with a distinct SGR 38;2 colour (~1 MB), then one of DL / IL / alternate-screen LF / alternate-screen SU / a full-screen scroll region — crash sites `Screen.zig:747` (from `deleteLines`/`insertLines`/`cursorScrollAboveRotate`) and `PageList.zig:5108` (from `eraseRow`(`scrollUp`), `eraseRowBounded`(`index`)).
- ReleaseFast:     the process still aborts — `panic: increaseCapacity OutOfSpace` (Zig retains `@panic` in ReleaseFast), SIGTRAP, no stack trace. MEASURED, PSL route on 80×24 (task R13-X3, ReleaseFast build): 8 of 253 sweep runs die (SIGTRAP). Also MEASURED, distinct-live-styles route on a wide terminal (task R13-X). Not silent UB.
- reachability:    EVERY width is reachable, because the primary route is the PSL-overflow arm, which does NOT depend on how many cells a page holds.
                   PRIMARY ROUTE (PSL overflow) — reachable on an ORDINARY 80×24 terminal, MEASURED (task R13-X3, both reference build modes, C ABI and a Zig test). Recipe, about 1.2 KB of output (about 550 bytes of colliding SGR, then 591 line feeds — page 1's row capacity at 80 columns, which is required: a compact `CSI 591 S` scroll-up does not substitute, 0 of 4,824 runs — then about 30 bytes including the DL; measured 1,157–1,175 bytes, R13-X3 re-review):
                     1. Put 32 cells on page 1 with the colours (as SGR 38;2): (0,3,183) (0,36,22) (0,212,121) (1,39,95) (1,89,203) (1,98,209) (1,196,183) (3,10,143) (3,27,63) (3,136,88) (3,177,19) (4,4,80) (4,38,52) (5,170,136) (6,82,67) (6,211,183) (6,221,6) (6,247,172) (7,62,169) (7,87,247) (7,177,15) (8,49,218) (8,179,161) (9,140,163) (9,205,72) (10,83,2) (10,93,46) (10,248,1) (11,112,126) (11,210,50) (13,123,81) (14,56,135). Their style hashes all share the low 16 bits 0x2e2a, the table mask at every capacity up to the 65,536 ceiling, so they stay clustered at every doubling.
                     2. Put one cell with any NEW style on the first row of the next page (the active area must already straddle a page boundary, e.g. after ~573 line feeds at 80×24).
                     3. Send DL (`ESC[2;1H ESC[M`) — the delete pulls page 2's first row into page 1, whose PSL-overflowed set cannot store the new style, so `increaseCapacity(.styles)` climbs 128→…→65,535→`OutOfSpace` → @panic.
                   Controls (MEASURED): 31 colliding colours never crash; 32 colours with no DL never crash; DL at row 1 never crashes (it deletes the colliding row, trimming the ids before the add). DL reaches this at 80 columns (IL did NOT crash at 80×24 with this recipe, 0 of 1,944 runs in the R13-X3 re-review: DL shifts rows up into the overflowed earlier page, IL shifts them down into a fresh later page); the PageList-loop routes need the PSL-overflowed set to be the erase DESTINATION page, so alternate-screen SU/LF do NOT reach it at 80 columns (the alt screen is bounded to a single page — MEASURED, 0 deaths over 32,200 sweep runs) and the primary scroll-region route needs a wide primary screen.
                   SECOND ROUTE (a full set of distinct LIVE styles) — needs a page off the standard capacity, so ≥ 47,932 columns on a shipped macOS arm64 build (46,908 on Linux aarch64, as measured for UD80); at 47,931 columns it survives (MEASURED). It then needs 53,247 distinct live styles to fill the set (53,246 survives, MEASURED). That threshold (47,932 / 47,931 columns) belongs to THIS route only, not to the defect: a standard-capacity page at 215 columns holds 221 rows = 47,515 cells (NOT the 215 rows / 46,225 cells that "215 × 215" would give — 46,225 is only the nominal `std_capacity` product), and over every width up to `maxCols` the most any standard page holds is 47,931 cells (at 47,931 columns, one row), MEASURED by a sweep of the port's `Capacity::adjust` (task R13-X2/R13-X3, macOS arm64, shipped grapheme budget; the port's page geometry equals the reference's since N88). Because the PSL route reaches every width regardless, the earlier "18 widths between 9,585 and 47,931 columns are UNVERIFIED" question is moot and is dropped: every width is reachable. (Task R13-X's first wording, "a standard page holds at most ~48,000 cells, fewer than the ~47,923 live ids", contradicted itself; corrected by task R13-X2; task R13-X2's "a NORMAL-width terminal CANNOT reach it" and "narrower than 9,585 columns cannot reach it" were WRONG — they missed the PSL arm — and are corrected here by task R13-X3.)
                   Linux: the PORT's 80-column degrade test was RUN on Linux aarch64 (task R13-X3, VM) and passes. The reference PSL crash was NOT re-run on Linux (only cross-platform-independent by construction: the style hash is unseeded and deterministic, and the PSL route does not depend on `maxCols`, so the same 32 colours cluster and crash on any platform).
- output needed:   the panic message and process abort; the terminal dies mid-mutation with no way to recover the rotated rows.
- port handling:   the port degrades PER DIMENSION instead of panicking (task R13-X2; task R13-X's first version degraded every dimension at once). When a capacity increase in the loop returns `OutOfSpace` (after the UD80 one-shot re-clone, for the style dimension), `pagelist/grow.rs` `degrade_dimension` marks THAT dimension (styles, grapheme bytes, hyperlink bytes or string bytes) as degraded and the loop retries with `crates/rustty-vt/src/page_impl.rs` `clone_partial_row_degrade`. That copy is the reference's per-cell body, except that a failure in a degraded dimension leaves that part of the cell at its default (no style / no grapheme / no hyperlink, keeping the codepoint), while a failure in any other dimension is returned as usual so the loop grows that dimension and retries. So a full style set degrades only the cells that bring a NEW style; a style already in the set is still stored, and graphemes, hyperlinks and their strings still grow (MEASURED, the R13-X review's probe re-run on the fix: a new-style cell followed by 3,000 grapheme cells loses 0 of 3,000 graphemes and the grapheme capacity grows from 512 to 524,288 bytes, as in the control; under task R13-X's version it lost 2,968 and stayed at 512). Row flags are copied as the reference copies them. TERMINATION: a dimension enters the degrade set only in `degrade_dimension`, and a degraded dimension's errors are never returned again, so each of the four dimensions flips at most once (`debug_assert!`ed); every other retry either grows a dimension strictly (finitely many times before its own ceiling or the page-size limit returns `OutOfSpace`, which flips it), is the one-shot UD80 re-clone, or is the reference's own rehash arm, which this does not change. The UD80 re-clone arm now also counts its runs and `debug_assert!`s at most one ("UD80 re-clone ran twice: one-shot flag regressed"), so a regression of the one-shot flag fails in under a second in a debug build instead of re-cloning forever. LEAKS: graphemes never leak (`set_graphemes` frees its slice when the map put fails). A hyperlink's URI is duplicated into the string allocator before its set add, as in the reference (`page.zig:979-1002`, no errdefer), so a hyperlink degraded after that point orphans its string. MEASURED by the R13-X review on task R13-X's version (hyperlink set full at its CURRENT, non-ceiling capacity, one new 64-byte-URI link copied 400 times into one row): the page's 62 free string chunks fell by 2 per copy to 0 by about copy 31, then stayed at 0 (the next `dupe` fails with `StringAllocOutOfMemory` and nothing more leaks), and a same-capacity clone restored all 62. With the per-dimension degrade that probe no longer orphans anything: the hyperlink set grows instead, the link is stored on all 400 copies, and the free chunks stay at 60 (the stored link's own 2) (MEASURED, task R13-X2). An orphan now needs the hyperlink dimension itself at its ceiling; it is then bounded by the page's string capacity, and the next growth of the string dimension (a page clone, which copies only live links) reclaims it. This mirrors the reference's own precedent for an unstorable style: `Screen.increaseCapacity` and `manualStyleUpdate` degrade to the default style, and UD65's decoder degrades. The degrade is logged once per copy (`log::warn!`, "row copy at a page capacity ceiling: N cell part(s) degraded to default (styles=…, hyperlinks=…, graphemes=…); see UD131"), never once per cell or per retry. The count says "cell part(s)", not "cell(s)": it sums the degraded style, hyperlink and grapheme SLOTS, so a cell that loses both its style and its grapheme counts twice (corrected task R13-X3, review N4). CAVEAT (review N3): a cell whose hyperlink already exists is kept only while the hyperlink MAP is below capacity; once the map itself is full (`hyperlink_count >= capacity`, checked before the lookup) an existing link is dropped too when the hyperlink dimension is degraded — noted in the `clone_partial_row_degrade` doc comment. PORT NOTEs at `screen.rs` (the `clone_partial_row_grow_capacity` `OutOfSpace` arm), `pagelist/grow.rs` (the `clone_row_grow_capacity` `OutOfSpace` arm and `degrade_dimension`), and `page_impl.rs` (`clone_partial_row_degrade`) all name BOTH reachability routes (PSL overflow on any width; a full live set on a wide terminal). W2's one-shot re-clone flag is kept.
- parity row(s):   NEW-FINDINGS N80.
- test:            `ud131_erase_row_loop_full_live_set_degrades_new_style_to_default` (`pagelist/tests.rs`, the `PageList::clone_row_grow_capacity` loop) and `ud131_clone_partial_row_loop_full_live_set_degrades_new_style_to_default` (`screen.rs`, the `Screen::clone_partial_row_grow_capacity` loop). Each fabricates a full LIVE style set at the ceiling on a wide (50,000-col) destination page whose two pages are made with `split` to keep `size.rows` small (so the capacity-growth clones stay cheap; ~1.3 s each), with the first style genuinely pinned so the one re-clone reclaims nothing and W2's one-shot flag is exercised. The source row carries a new style, a grapheme, a hyperlink, a plain cell and row flags; the tests assert the copy completes, the unstorable style degrades to the default, the grapheme/hyperlink/codepoints/row flags are kept, the live set stays full, and every page passes `assert_integrity`. Task R13-X2 added the mixed-row twins `ud131_erase_row_loop_degrades_only_new_styles_and_grows_the_other_dimensions` (`pagelist/tests.rs`) and `ud131_clone_partial_row_loop_degrades_only_new_styles_and_grows_the_other_dimensions` (`screen.rs`), which share `page_impl.rs` `ud131_test_support`: the source row mixes a wide character and its spacer on a style the full set already has and on a new one, other existing- and new-style cells, graphemes (including a 300-cell run that needs grapheme growth), an implicit and an explicit hyperlink, and a 200-cell run over 12 links with 200-byte URIs that needs hyperlink and string growth. The copy into the full page is compared cell by cell against a control copy into a page with an ordinary style set: only the five new-style cells differ, each on the default style; codepoints, wide flags, grapheme codepoints, hyperlink ids and URIs and the row flags equal the control's; the grapheme, hyperlink and string capacities grew; the style set gained nothing. Both also read the log through `log_capture`: exactly one record for one copy ("5 cell part(s) … styles=5, hyperlinks=0, graphemes=0") although the copy retried once per grown dimension, and two for two copies; the control copy logs none. (~3.6 s each in a debug build.) Release-only VT tests (`terminal.rs`, `#[ignore]`d, `cargo test -p rustty-vt --release --lib -- --ignored ud131_vt_release`; they return at once in a debug build, where a case does not finish in ten minutes): the reference probe's recipe through `vt_stream` on a 50,000-column terminal, one test per route — DL and IL (the Screen loop), alternate-screen LF, alternate-screen SU and a primary-screen scroll region `ESC[2;216r` + LF (the PageList loop) — each asserting that 'Y''s new style degrades, one log record, and page integrity; plus 53,246 fills with DL, where 'Y' keeps its style and nothing is logged. All six pass in 0.5 s (macOS arm64, 2026-09-23). (Review N1, task R13-X3: `Page::assert_integrity` is a no-op without `debug_assertions`, so in a release build the old integrity loop checked nothing; the release tests now call `Page::verify_integrity` — whose body runs in every build — and assert it returns `Ok`, so the integrity claim is real in release.)
                   Task R13-X3 also added an ORDINARY-80×24 debug test of the PSL route, in the DEFAULT suite (not `#[ignore]`d, unlike the wide-terminal release tests): `ud131_delete_lines_degrades_on_an_ordinary_80_column_terminal` (`terminal.rs`) scrolls an 80×24 terminal so the active area straddles a page boundary, prints the 32 colliding colours on page 1, a new-style 'Y' on page 2's first row, and DL — asserting the copy completes ('Y' survives), only the unstorable cell degrades ('Y' → default style; the storable colliding cell keeps its style), exactly one log record ("1 cell part(s) … styles=1"), and integrity holds (real in the debug build; it also calls the PageList's `assert_integrity`). ~9 s in debug. The control `ud131_thirty_one_colours_is_the_80_column_control` (31 colliding colours) degrades nothing and logs nothing. RUN on Linux aarch64 too (task R13-X3, VM): both pass. NEGATIVE CONTROL (task R13-X3, macOS): restoring the reference `@panic` in the Screen loop's `OutOfSpace` arm fails the 80-column test with "increaseCapacity OutOfSpace" (`screen.rs:1551`). A pure table test `ud131_degrade_dimension_mapping_is_coherent` (`pagelist/tests.rs`) checks, for every `CloneFromError`, that the mask the loop builds (via `IncreaseCapacity::for_clone_error` → `degrade_dimension`) degrades exactly that error and no other, and that the rehash errors are never degraded — covering the non-style degrade arms the mixed-row tests leave untested; NEGATIVE CONTROL: mis-mapping the hyperlink errors to the `strings` flag in `RowDegradeMask::degrades` fails it ("the flag set for HyperlinkSetOutOfMemory is the one that degrades it"). Negative controls (macOS 2026-09-22, task R13-X): restoring the `@panic` makes each unit test fail with "increaseCapacity OutOfSpace" (`grow.rs:613`, `screen.rs:1515`); removing the one-shot re-clone flag made each test HANG (unbounded re-clone, since the pinned style keeps `styles_hold_pinned` true), confirmed by a >45 s / >50 s no-finish run. Task R13-X2 (macOS 2026-09-23, each mutation applied alone and the file restored): "degrade every style" (the degrade copy never stores a style) turns both mixed-row tests red (style-differing columns [0, 1, 2, 3, 4, 5, 6, 7, 10, 11] against [0, 1, 3, 4, 10]) while both first tests stay green, which is what the review found they could not tell apart; "degrade every dimension when the styles are full" (task R13-X's behaviour) turns both mixed-row tests red (the first grapheme-run cell lost its grapheme) while both first tests stay green; resetting the one-shot flag whenever the node changed (a clippy-clean regression) fails both first tests in under a second (0.98 s each, whole test) with "UD80 re-clone ran twice: one-shot flag regressed"; in release, restoring the `@panic` in the Screen loop fails exactly the DL and IL VT tests (`screen.rs`), and in the PageList loop exactly the alternate-screen LF, SU and scroll-region ones (`grow.rs`).
- proposed fix:    in both loops, on `increaseCapacity` returning `error.OutOfSpace`, mark that one dimension as degraded and retry with a `cloneRowFrom` variant that leaves a cell's part in a degraded dimension at its default (as `manualStyleUpdate` / `increaseCapacity` already do for a style) and still returns the other dimensions' errors so they keep growing, rather than `@panic`; log once per copy. Each dimension degrades at most once, so the loop still terminates. The rows are already rotated, so this completes the mutation without corruption. (A hardening of the PSL arm itself — a seeded style hash, or a larger `psl_stats` window — would raise the bar for arm (a), but the `OutOfSpace` panic is the crash and the degrade above is the direct fix.)

- upstream status: NOT YET REPORTED. Report PRIVATELY through Ghostty's GitHub security advisory process (a repository Security Advisory / private vulnerability report), NOT a public issue: this is a remotely reachable denial-of-service — about 1.2 KB of ordinary terminal output crashes Ghostty at a standard 80×24 size — so it should be disclosed privately and given time to fix before any public description.

### UD140 — a kitty graphics file (or zlib payload) of exactly `max_size` bytes is rejected, one byte short of the inclusive limit used everywhere else
- kind:            functional (off-by-one against the file's evident intent)
- upstream project: ghostty
- severity:        low
- reference:       src/terminal/kitty/graphics_image.zig:21 (`max_size = 400 * 1024 * 1024`), :440 (`readFile`, no `s=`: `reader.appendRemaining(alloc, &managed, .limited(max_size))`), :608 (`decompressZlib`: `stream.reader.appendRemaining(alloc, &list, .limited(max_size))`); the inclusive checks at :323 (`data_size > max_size`), :428 (`size > max_size`), :506 (`len + data.len > max_size`), :642 (`result.data.len > max_size`). zig 0.16.0 `std/Io/Reader.zig` `appendRemainingAligned` doc: "If `limit` is reached or exceeded, `error.StreamTooLong` is returned instead". At 20abdb50a.
- defect:          `.limited(n)` fails once `n` bytes have been read, so both `appendRemaining` sites reject a payload of exactly `max_size` bytes, while every other `max_size` check in the file accepts exactly `max_size` and rejects more. The same 400 MiB file therefore loads when sent with `s=` and fails with `InvalidData` without it. The limit semantics were read in the pinned stdlib and pinned by the port's faithful `append_remaining_limited` test; not measured on a Ghostty build.
- ReleaseSafe:     no crash; the transmission fails (`InvalidData` for the file path, `DecompressionFailed` for zlib), answered with an error response.
- ReleaseFast:     same.
- reachability:    any program that sends a file-medium image of exactly 419,430,400 bytes without `s=`, or a zlib payload that inflates to exactly that size.
- port handling:   fixed (R13, extended 2026-09-23): `readFile` reads with a limit of `max_size + 1` (`crates/rustty-vt/src/kitty/graphics_image.rs`, PORT NOTE: reference defect (R13) at the call), so exactly `max_size` loads and one byte more is still `InvalidData`. The zlib path was already inclusive: `zlib_sys::inflate_zlib` accepts exactly its limit and rejects more (MEASURED on the 128x96 fixture at limit = its 36,864-byte output, and limit - 1); a PORT NOTE at `decompressZlib` now records that this differs from the reference. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   LEDGER S189-F2 (T-A ported the off-by-one faithfully; review-T-A "extended R13" item 1).
- test:            `image_load_file_of_max_size_without_size_loads` (negative control: the limit back to `max_size` fails, "a file of exactly max_size bytes must load"); `image_load_file_over_max_size_without_size_is_invalid_data` (negative control: limit `max_size + 2` fails); `zlib_inflate_accepts_exactly_the_limit` (negative control: `inflate_zlib` made to reject a stream ending at the limit fails on the exact-limit `unwrap`). `append_remaining_limited_matches_zig_limit_semantics` still pins the faithful Zig semantics of the helper itself.
- proposed fix:    use `.limited(max_size + 1)` at both sites (or check `list.items.len > max_size` after an unlimited-but-bounded read), matching the file's other checks.
- upstream status: not reported

### UD141 — the glyph APC, kitty graphics, OSC 4/104, OSC 66, OSC 133 and OSC 9 read `1_0` as 10 (Zig's digit separator on the wire)
- kind:            functional (a Zig source-literal feature leaks into terminal protocols, against Ghostty's stated position)
- upstream project: ghostty
- severity:        low
- reference:       src/terminal/apc/glyph/request.zig:106, 217-222, 372 (`std.fmt.parseInt(u21, value, 16)` for `cp`, `u32` base 10 for `upm`/`aw`/`lh`); src/terminal/kitty/graphics_command.zig:321-322 (every integer key value); src/terminal/osc/parsers/color.zig:177, 250 (OSC 4 / OSC 104 color index, `u9`); src/terminal/osc/parsers/kitty_text_sizing.zig:50 (OSC 66 values, `u4`); src/terminal/osc/parsers/semantic_prompt.zig:171 (OSC 133 `D` exit code, `i32`); src/terminal/osc/parsers/osc9.zig:28, 196 (OSC 9;1 sleep, OSC 9;4 progress, `parseUnsigned`). Ghostty's own position: src/terminal/osc/parsers/kitty_desktop_notification.zig:210-211 ("Zig's integer parser allows '_', we don't", then `std.mem.indexOfScalar(u8, value, '_')` before `parseInt`) and src/terminal/osc/parsers/context_signal.zig:165-167 (digits only before `parseInt`). zig 0.16.0 `std/fmt.zig:394-460` (`parseIntWithSign`; `if (c == '_') continue;` at :442). At 20abdb50a.
- defect:          `std.fmt.parseInt` and `parseUnsigned` skip interior `_` (Zig's source-literal digit separator), so each site above reads `1_0` as 10, `E0_A0` as U+E0A0, `4_2` as 42. None of these protocols defines `_` in a number (the glyph spec's `cp=<hex>` / `upm=<int>`, kitty's integer key values, xterm's color number, the ConEmu and FinalTerm numbers), and Ghostty itself filters `_` out in its OSC 99 parser with a comment saying it does not want Zig's behaviour, and accepts only digits in its OSC 3008 parser. The sites above carry no filter. By reading the pinned sources and stdlib; the port's faithful behaviour (T-A) was pinned by tests before this change.
- ReleaseSafe:     no crash; a malformed number is accepted as a different, valid one (for example a kitty graphics `s=1_0` is a width of 10, an OSC 4 `1_0` sets palette entry 10).
- ReleaseFast:     same.
- reachability:    any program writing to the terminal.
- port handling:   fixed (R13, extended 2026-09-23): `crates/rustty-vt/src/protocol_int.rs` `reject_digit_separators` (PORT NOTE: reference defect (R13)) is called before the `rustty_zigstd::fmt` parse at each site (glyph `parse_u21`/`parse_u32`, kitty graphics `parse_int_i32`/`parse_int_u32`, `color.rs` `parse_int_u9`, `kitty_text_sizing.rs` `update`, `semantic_prompt.rs` `EXIT_CODE`, both `osc9.rs` arms), each with a PORT NOTE, and takes that site's existing failure path (`null` / `InvalidFormat` / `InvalidValue` / the default), as the OSC 99 filter takes `key.default()`. Only `_` is refused: the OSC 99 filter leaves signs to `parseInt` (and then range-checks), so `+10` and `-0` stay accepted where the reference accepts them. The tmux control-mode parsers (`tmux/layout.zig`, `output.zig`, `control.zig`, `viewer.zig`, N100) stay faithful: their input is tmux's own generated output rather than a protocol programs write numbers into, and no Ghostty code states a position there. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   LEDGER S170-F1, S186-F1, S194-F3, S195-F2, S197-F1; NEW-FINDINGS N100 (osc9 arms); review-T-A "extended R13" item 2.
- test:            `integer_options_follow_zig_parse_int_but_refuse_digit_separators` (glyph), `integer_values_refuse_zig_digit_separators`, `parse_int_u32_matches_reference_parse_int_with_sign`, `invalid_format_cases_for_negative_u32` (kitty graphics), `osc_4_104_color_index_follows_zig_parse_int_but_refuses_digit_separators`, `osc_66_values_follow_zig_parse_int_but_refuse_digit_separators`, `option_read_exit_code_follows_zig_parse_int_but_refuses_digit_separators`, `osc_9_numbers_use_zig_parse_unsigned`; each also asserts the digit-only (and signed) form still parses. Negative control: the filter disabled in `reject_digit_separators`, 8 of those tests fail on their `_` assertions.
- proposed fix:    apply the OSC 99 filter (`if (std.mem.indexOfScalar(u8, value, '_') != null) <failure path>`) at these sites, or add a small `parseProtocolInt` helper that refuses `_` and use it for every wire-protocol integer.
- upstream status: not reported

### UD150 — `GitVersion.detect` turns the branch's trailing newline into `-`, so every dev version reads `X.Y.Z-<branch>-+<hash>`
- kind:            latent (functional: a mangled value that visibly breaks the code's evident intent; no crash). RECLASSIFIED 2026-09-23 (review T-B) from "latent, ported faithfully" to fixed under the extended R13.
- upstream project: ghostty
- severity:        low
- reference:       `src/build/GitVersion.zig:22-41` (the sanitizing loop over `runAllowFail`'s raw stdout) and `:84` (`.branch = std.mem.trimEnd(u8, branch, "\r\n ")`), at 20abdb50a. Introduced by upstream `ad9225a4c` ("build: sanitize all invalid chars in branch name for version", 2026-04-09, fixing #11990), read with `git show`.
- defect:          `b.runAllowFail` returns `git rev-parse --abbrev-ref HEAD`'s raw stdout, trailing `\n` included (zig 0.16.0 `std/Build.zig:1862-1900`, read). The loop that rewrites every non-`[0-9A-Za-z-]` byte to `-` runs BEFORE the `trimEnd` at `:84`, so the newline has already become `-` and the trim finds nothing. The pre-release identifier ends in a stray `-`. The `trimEnd` can only be meant to strip git's newline, and it did so before `ad9225a4c`: that commit replaced a `/`-only `std.mem.replaceScalar` (which left the newline for the trim) with the full loop, whose stated purpose was making dotted branch names valid semver, not changing every version. MEASURED: the reference's own cached build options at `.zig-cache` carry `app_version_string = "1.3.2-HEAD-+4540d499a"`.
- ReleaseSafe:     no trap; the version string, `+version`, XTVERSION, `TERM_PROGRAM_VERSION`, the Sentry release and dist tarball names carry the extra `-`.
- ReleaseFast:     same.
- reachability:    every untagged build from a git checkout.
- port handling:   fixed (R13, extended 2026-09-23): `crates/rustty-version/src/git_version.rs` `branch_from_rev_parse` trims `"\r\n "` first and then sanitizes (PORT NOTE: reference defect (R13) there), so the port reports `0.1.0-<branch>+<hash>` (this worktree's build: `0.1.0-tail-b2+<hash>`). Listed in `docs/ghostty-bug-fixes.md`. (Batch T-B first ported it faithfully, in `crates/rustty-build-config/src/git_version.rs`; the git-derived version has since moved to the `rustty-version` crate.)
- parity row(s):   LEDGER S64-F1 / S63-F2 (batch T-B); review-T-B blocking item 5.
- test:            `rustty-version` `tests::git_branch_is_trimmed_before_it_is_sanitized` (`feature/x_y.2\n` -> `feature-x-y-2`, `main\r\n` -> `main`, interior characters still sanitized, and the built `VERSION_STRING` carries no `-+`). Negative control: `branch_from_rev_parse` changed to sanitize and then trim (the reference's order) fails with `left: "feature-x-y-2-"`, `right: "feature-x-y-2"`.
- proposed fix:    trim before sanitizing: `const raw = std.mem.trimEnd(u8, tmp, "\r\n ");` and run the loop over `raw` (or move the `trimEnd` from `:84` to just before the loop).
- upstream status: not reported

### UD160 — the ssh-cache host check traps on a scoped IPv6 address longer than 255 bytes
- kind:            crash (safety-checked trap)
- upstream project: zig (the standard library's IPv6 parser), reached through ghostty
- severity:        low (a CLI process the user runs, on input the user supplies)
- reference:       `src/cli/ssh-cache/DiskCache.zig:428-443` (`isValidHost` calls `std.Io.net.IpAddress.parseIp6(host, 0)` once `HostName.validate` has rejected the text), at 20abdb50a; the trap is in Zig 0.16.0 `lib/std/Io/net.zig` `Ip6Address.Unresolved.parse`, `%` arm: `text_i = @intCast(text.len);` with `var text_i: u8`
- defect:          `Unresolved.parse` indexes the text with a `u8`. After a `%` (a scope id) it casts the whole text length into that index, which does not fit once the text is longer than 255 bytes. Nothing before it bounds the length: `isValidCacheKey` checks only that the key is non-empty, and `HostName.validate` fails on the `%` and `:` without stopping the fallback.
- ReleaseSafe:     panic "integer does not fit in destination type" at `net.zig:577` (MEASURED with Zig 0.16.0 ReleaseSafe, calling `std.Io.net.IpAddress.parseIp6("fe80::1%" ++ "a" ** 300, 0)`: exit 134). Not measured through a ghostty binary built at the pin.
- ReleaseFast:     undefined behaviour (the `@intCast` is unchecked). MEASURED with Zig 0.16.0 ReleaseFast (task T-C review, re-measured in T-C3 with the reference's `isValidHost` body): total lengths 255, 256 and 308 all return `false`, with no crash. The verdict happens to match ReleaseSafe's non-crashing answer, but the cast is still undefined behaviour, so this outcome is not guaranteed by the language and may change with the optimizer.
- reachability:    CLI: `ghostty +ssh-cache --add=fe80::1%<more than 247 bytes>` (also `--remove=`, and a positional query), and any caller that hands a destination to `DiskCache.add`/`remove`/`contains` (the shell-integration ssh wrapper and `+ssh`, which pass the destination the user typed or `ssh -G` resolved). The caller is the user, so this is a self-inflicted crash of a short-lived CLI process.
- port handling:   `crates/rustty-cli/src/ssh_cache/disk_cache.rs` `ip6_parse` (a port of `Unresolved.parse`) keeps a `usize` index, so any scoped address, long or short, is rejected the way the reference rejects a short one (`Ip6Address.parse` returns `error.UnresolvedScope` for any scope id, so `isValidHost` returns false). PORT NOTE at the `%` arm.
- parity row(s):   S71-F3 (the port now uses the reference's IPv6 grammar, which is what brings this code path in).
- test:            `long_scoped_ipv6_is_rejected_without_crashing` (rustty-cli). Negative control (macOS, task T-C): casting the length through `u8::try_from(..).unwrap()` as the reference's `@intCast` does makes it panic (`TryFromIntError`).
- proposed fix:    upstream in Zig, make `text_i` a `usize` (or reject text longer than 255 bytes up front). In ghostty, until then, reject a key longer than a hostname can be (`HostName.max_len` plus the `user@` prefix) in `isValidCacheKey` before calling `parseIp6`.
- upstream status: not reported

### UD170 — `+new-window -e` / `+new-tab -e` with a non-UTF-8 argument aborts the CLI process inside GLib's D-Bus serializer
- kind:            crash (GLib `g_assert` → SIGABRT)
- upstream project: ghostty
- severity:        low (the sending CLI process dies; the running instance is unaffected)
- reference:       `src/apprt/gtk/ipc/new_window.zig:45-56`, `src/apprt/gtk/ipc/new_tab.zig:44-61` (each argument becomes `glib.Variant.newFromBytes(s, Bytes(arg ++ NUL), trusted = true)`), sent by `src/apprt/gtk/ipc/DBus.zig:137-188` (`send` → `callSync` at `:165`), at 20abdb50a
- defect:          `newFromBytes` with `trusted = true` does not validate, so a non-UTF-8 argument builds an `s` value that is not UTF-8. `g_dbus_connection_call_sync` serializes the message with `g_dbus_message_to_blob`, whose `append_value_to_blob` asserts that every `s` is valid UTF-8.
- ReleaseSafe:     the process aborts: `GLib-GIO:ERROR:../gio/gdbusmessage.c:2691:append_value_to_blob: assertion failed: (g_utf8_validate (v, -1, &end) && (end == v + len))`, `Bail out!`, SIGABRT (exit 134). MEASURED on the VM (Fedora, GLib 2.88.2) with a C probe that builds the `(sava{sv})` payload exactly as `newWindow` does (the argument `caf\xe9`) and calls `g_dbus_message_to_blob`. The abort is GLib's, so it does not depend on the Zig build mode.
- ReleaseFast:     the same abort (GLib's assertion, not Zig's).
- reachability:    the command line: `ghostty +new-window -e <arg>` or `+new-tab -e <arg>` where `<arg>` is not UTF-8, e.g. a file name in a Latin-1 locale or on a non-UTF-8 file system.
- port handling:   `crates/rustty-apprt/src/gtk/app.rs` `ipc_arguments_variant` checks each argument; a non-UTF-8 one fails the IPC with `IPCFailed` (the reference's own failure return for these functions) after printing `D-Bus IPC argument is not valid UTF-8: <the argument's bytes>`, because the CLI caller treats `IPCFailed` as "the apprt should have printed a more specific error message already" (`src/cli/new_window.zig:249-252`). That line is NEW text; the reference has none for this case. UTF-8 arguments build the byte-identical `s` values. The check runs where the reference builds the parameter, after the bus-name checks and the session-bus connection, so earlier diagnostics keep their order. The port previously `String::from_utf8_lossy`'d each argument, which forwarded U+FFFD-mangled arguments and reported success. PORT NOTE at `ipc_arguments_variant`.
- parity row(s):   LEDGER S59-F1.
- test:            `ipc_arguments_variant_rejects_non_utf8` (rustty-apprt, Linux, display-free): `[-e, caf\xe9]` fails with `IPCFailed` and exactly that stderr line, for both `new-window-command` and `new-tab`'s `(tas)`. `ipc_arguments_variant_matches_the_reference_bytes`: for UTF-8 arguments (including non-ASCII and an empty one) the port's `as` equals the reference's own `newFromBytes` construction, built through the same C calls in the test (`g_variant_equal` and serialized bytes), and `(tas)` likewise. NEGATIVE CONTROL (VM, 2026-09-23): restoring the lossy conversion fails the first test at its `IPCFailed` assertion. The reference's own behaviour (the abort) is shown by the C probe above, not by a Rust test.
- proposed fix:    validate before building: `if (!std.unicode.utf8ValidateSlice(argument)) { try stderr.print("argument is not valid UTF-8: {s}\n", .{argument}); return error.IPCFailed; }`, or build the value with `glib.Variant.newString`, which validates.
- upstream status: not reported

### UD171 — opening a non-UTF-8 URL through the OpenURI portal violates `g_variant_new_string`'s UTF-8 precondition
- kind:            ub (a GLib API precondition violation; GLib documents the function as having undefined behaviour after a failed `g_return_*_if_fail`)
- upstream project: ghostty
- severity:        low
- reference:       `src/apprt/gtk/portal/OpenURI.zig:147-187` (`start`: `entry.uri = try alloc.dupeZ(u8, value.url)`, no validation), `:259-353` (`sendRequest`: `builder.add("s", entry.uri.ptr)` at `:279`), at 20abdb50a; called from `src/apprt/gtk/class/application.zig:2832-2850` (`openUrl`)
- defect:          `builder.add("s", ptr)` is `g_variant_builder_add(&b, "s", ptr)`, i.e. `g_variant_new_string(ptr)`, whose precondition is a valid UTF-8 string. A URL's bytes are not validated anywhere before it.
- ReleaseSafe:     GLib logs `GLib-CRITICAL: g_variant_new_string(): requires valid UTF-8` and substitutes the string `[Invalid UTF-8]`, so the portal is asked to open `[Invalid UTF-8]`. MEASURED on the VM (GLib 2.88.2) with a C probe that builds `sendRequest`'s `(ssa{sv})` payload the reference's way (printed: `('', '[Invalid UTF-8]', {'handle_token': <'0000000000000042'>})`). What the portal then does with that URI was NOT measured (the VM's portal did not start: `NameHasNoOwner ... startup job failed`); UNVERIFIED. GLib's contract for a failed `g_return_*_if_fail` (the `g_return_if_fail` documentation in `glib/gmessages.h`, GLib main fetched 2026-09-23): "the current function should be considered to have undefined behaviour (a programmer error)"; with the checks compiled out (`G_DISABLE_CHECKS`) the invalid string would reach the D-Bus serializer, whose UTF-8 `g_assert` aborts the process (MEASURED for the IPC path, UD170; not measured for this path).
- ReleaseFast:     the same (GLib's behaviour, not Zig's).
- reachability:    the VT stream: a terminal hyperlink (OSC 8) whose URL bytes are not UTF-8, opened by the user; also any other `open_url` whose bytes are not UTF-8 and do not start with `/` or `file://` (those two go straight to the OS opener).
- port handling:   `crates/rustty-apprt/src/gtk/portal/open_uri.rs` `OpenUri::start` returns `Error::UriNotUtf8` before any state is created (no entry, no subscription, no cleanup timer, no portal call), and the caller takes the reference's own failure path for `start`: `log.err("unable to open uri err={}")` then `openUrlFallback` (`application.zig:2845-2849`), so the URL is still opened by the OS opener with its raw bytes. The guard predates this task (it was a "DELIBERATE, DOCUMENTED DIVERGENCE"); task T-D re-grounds it on R13, records the measurement, and adds the first test that drives it. PORT NOTE on `Error::UriNotUtf8`.
- parity row(s):   LEDGER S59-F5.
- test:            `open_uri_rejects_a_non_utf8_uri_before_any_state` (rustty-apprt, Linux, no display): a private `dbus-daemon` (started by the test, connected without touching the process environment) supplies a real message-bus connection; `https://example.invalid/caf\xe9` returns `UriNotUtf8` with no entry in flight, no cleanup timer and nothing observed, and the positive control on the same object and connection, `https://example.invalid/cafe`, starts a request (one entry in flight). CORRECTION (task L-2): this said "NOT YET RUN: written by task T-D, but the Linux VM went down before this test was built; no negative control yet." RUN on the VM (Fedora 44 aarch64, GLib 2.88.2, 2026-09-26): passes; NEGATIVE CONTROL (the URI converted with `from_utf8_lossy` instead of refused): RED, `start` returns `Ok(())` where `Err(UriNotUtf8)` is expected.
- proposed fix:    validate in `start` and fail into the existing fallback: `if (!std.unicode.utf8ValidateSlice(value.url)) return error.InvalidUri;` (the caller already falls back to `openUrlFallback` on any error).
- upstream status: not reported

### UD172 — an OSC 52 write with a non-UTF-8 payload that needs confirmation previews as EMPTY, and confirming clears the clipboard
- kind:            ub (a GTK API precondition violation; see UD171 for GLib's contract) with a data-loss consequence
- upstream project: ghostty
- severity:        low (needs `clipboard-write = ask`)
- reference:       `src/apprt/gtk/class/surface.zig:4045-4132` (`Clipboard.set`: `text` is the payload's raw `text/plain` bytes), `:4232-4271` (`showClipboardConfirmation`: `contents_buf.insertAtCursor(str, @intCast(str.len))`), `:4273-4309` (`clipboardConfirmationConfirm`: reads the buffer's `text` property back and completes the request with it), at 20abdb50a
- defect:          `gtk_text_buffer_insert_at_cursor`'s precondition is valid UTF-8. A non-UTF-8 OSC 52 payload violates it.
- ReleaseSafe:     GTK logs `Gtk-CRITICAL: gtk_text_buffer_emit_insert: assertion 'g_utf8_validate (text, len, NULL)' failed` and inserts nothing, so the dialog previews an EMPTY payload, and confirming completes the write with the buffer's text, `""`, which clears the clipboard. MEASURED on the VM (GTK 4.22.4) with a C probe: after `gtk_text_buffer_insert_at_cursor(buf, "caf\xe9!", 5)` the buffer's `text` is empty, while the U+FFFD-substituted `caf\xef\xbf\xbd!` is kept (7 bytes). The dialog-level consequence (empty preview, cleared clipboard) is by reading `clipboardConfirmationConfirm`.
- ReleaseFast:     the same (GTK's behaviour).
- reachability:    the VT stream: any program can send an OSC 52 write with arbitrary base64-decoded bytes; the dialog appears when `clipboard-write = ask`.
- port handling:   `crates/rustty-apprt/src/gtk/class/surface.rs` `set_clipboard` previews `String::from_utf8_lossy(payload)`: each invalid sequence becomes U+FFFD, and confirming writes that text, so the user confirms exactly what is written. The `clipboard-write` signal (the non-confirm path) carries the raw bytes, as the reference's does (S56-F3). PORT NOTE at the `show_clipboard_confirmation` call in `set_clipboard`.
- parity row(s):   LEDGER S56-F3.
- test:            `clipboard_confirmation_preview_substitutes_invalid_utf8` (rustty-apprt, Linux, needs a display): `set_clipboard(Standard, [text/plain: caf\xe9], confirm = true)` builds a dialog whose preview buffer reads `caf\u{FFFD}`. CORRECTION (task L-2): this said "NOT YET RUN: written by task T-D, but the Linux VM went down before this test was built; no negative control yet." RUN on the VM (GTK 4.22.4, private headless sway, 2026-09-26): passes; NEGATIVE CONTROL (the preview given the reference's measured result, the empty text): RED, the buffer reads `""` where `caf\u{FFFD}` is expected.
- proposed fix:    validate before inserting and show a replacement-character copy, or refuse the write: `if (!std.unicode.utf8ValidateSlice(str)) { ... }`; and let confirm write the ORIGINAL bytes rather than the buffer's text.
- upstream status: not reported

### UD173 — `loadCustomCss`'s failure warning says "no custom CSS applied" when earlier stylesheets stay applied
- kind:            latent (a misleading diagnostic)
- upstream project: ghostty
- severity:        low
- reference:       `src/apprt/gtk/class/application.zig:1123-1199` (`loadCustomCss`: each stylesheet is added to the display as it loads; a non-`FileTooBig` read error `return`s), `:1309-1314` (`propConfig`: `log.warn("failed to load custom CSS, no custom CSS applied, err={}")`), at 20abdb50a
- defect:          when entry N fails its read, entries 0..N-1 are already registered on the display and stay applied, but the warning says none is.
- ReleaseSafe:     a wrong message; the earlier stylesheets are in effect. By reading (`loadCustomCss` adds each provider to the display before the next file is read). The port test that exercises the same shape (`load_custom_css_stops_on_a_read_error_and_loads_raw_bytes`) is written but NOT YET RUN (the Linux VM went down).
- ReleaseFast:     the same.
- reachability:    config: a `gtk-custom-css` entry that opens but cannot be read (MEASURED: a directory opens and its first read fails `error.IsDir`, Zig 0.16 on the VM), listed after a readable one.
- port handling:   faithful (message and behaviour), as R13 leaves non-crash defects to R1.
- parity row(s):   LEDGER S49-F3.
- test:            as above.
- proposed fix:    say what happened: `"failed to load custom CSS, later stylesheets not applied, err={}"`, or unload the partial set before returning.
- upstream status: not reported

### UD174 — `globalShortcutBindFailed` silently drops the notification when the compositor's message is long
- kind:            latent (a silent failure)
- upstream project: ghostty
- severity:        low
- reference:       `src/apprt/gtk/class/application.zig:1753-1784` (`globalShortcutBindFailed`: `var body_buf: [512]u8`, `std.fmt.bufPrintZ(&body_buf, "{s}: {s}", ...) catch return;` at `:1773-1778`), at 20abdb50a
- defect:          a "global keybind unavailable" notification whose body (trigger label, `": "`, the compositor's message) is 512 bytes or more is not raised at all, and nothing is logged, so a keybind the compositor denied does nothing with no sign of why: the silent no-op this notification exists to prevent.
- ReleaseSafe:     no notification, no log. By reading.
- ReleaseFast:     the same.
- reachability:    a peer process: the compositor's (or portal's) denial/revocation message.
- port handling:   faithful (the same 512-byte limit, measured on the raw bytes, and the same 128-byte label buffer with its `"?"` fallback), as R13 leaves non-crash defects to R1. `crates/rustty-apprt/src/gtk/class/application.rs` `bind_failed_notification`.
- parity row(s):   LEDGER S49-F4, S50-F3.
- test:            `application_bind_failed_notification_buffer_limits` (rustty-apprt, Linux, display-free).
- proposed fix:    format into an allocating writer, or truncate the message to fit instead of returning.
- upstream status: not reported


### UD180 — `+list-keybinds --docs` never prints a keybind action's help
- kind:            latent (functional: documentation output silently empty)
- upstream project: ghostty
- severity:        low
- reference:       src/config/Config.zig:7549-7580 (`Keybinds.formatEntryDocs` from :7549, the `docs` branch :7562-7580; CORRECTION: first cited as `:7560-7576`); src/cli/list_keybinds.zig:50-51 (the `--docs` flag's help: "currently does nothing, intended to print out documentation about the action associated with the keybinds"); src/input/Binding.zig:2138-2148 (`Set.Value = union(enum) { leader, leaf, leaf_chained }`); src/helpgen.zig:119-136 (`genKeybindActions` emits one `KeybindAction` decl per `Binding.Action` union field). At 20abdb50a.
- defect:          the docs branch does `const name = @tagName(v);` with `v` a `Binding.Set.Value`, so `name` is `leader`, `leaf` or `leaf_chained`, and then looks for a `help_strings.KeybindAction` decl of that name. Those decls are named after ACTIONS (`new_window`, `copy_to_clipboard`, ...), and no action is called `leader`, `leaf` or `leaf_chained`, so the lookup never matches. `ghostty +list-keybinds --docs` therefore prints one blank line before each root binding and never the `# <action>` / `#   <help>` block the code is written to print. Evidently the intent was the tag of the binding's ACTION (`v.leaf.action`, and some choice for chained leaves). By reading, not measured on a Ghostty build.
- ReleaseSafe:     no crash; the `--docs` output lacks every action description.
- ReleaseFast:     same.
- reachability:    any user running `ghostty +list-keybinds --docs`.
- port handling:   faithful: `crates/rustty-config/src/value_types.rs` `v_tag_name` returns the `Set.Value` tag, so the port prints the same blank lines. Before LEDGER S76-F1 the port returned the action tag and printed the help blocks. Re-decided under R13 as extended 2026-09-23 (non-crash defects that visibly break evident intent are fixed), and kept faithful, because the intent is not unambiguous: (1) the reference's own user-facing help for the flag (`list_keybinds.zig:50-51`, shipped in the port's `+list-keybinds --help` too) says `--docs` "currently does nothing", so the observed behaviour matches the documented behaviour, and the dead branch is an unfinished feature, not code contradicting its documentation; (2) finishing it needs choices the reference never made: which action to document for a `leaf_chained` binding (the first, or every one), what to print for a `leader` (a chord prefix with no action), and whether table bindings (`formatEntryDocs`'s second loop, which has no docs branch at all) get help too. Any choice is new user-visible behaviour, which R13 step 1 rules out. (This line first said "R13 covers crash-class defects only", which the 2026-09-23 extension made wrong.)
- parity row(s):   LEDGER S76-F1.
- test:            `keybinds_format_entry_docs_writes_only_the_blank_line` (`value_types.rs`) pins the reference output.
- proposed fix:    in the docs branch take the action tag: `const name = switch (v) { .leaf => |leaf| @tagName(leaf.action), .leaf_chained => |c| @tagName(c.actions.items[0]), .leader => continue-without-docs };` (or print one help block per chained action).
- upstream status: not reported

### UD181 — `RepeatableCodepointMap.equal` compares font families by pointer, so a clone is never equal to its source
- kind:            latent
- upstream project: ghostty
- severity:        low
- reference:       src/config/Config.zig:8152-8161 (`RepeatableCodepointMap.equal`, `std.meta.eql(a, b)` per entry; CORRECTION: first cited as `:8199-8210`, copied from the audit); src/font/CodepointMap.zig:36-43 (`clone` dupes each descriptor, `discovery.zig:109` `dupeZ`s `family`); zig 0.16.0 `std/meta.zig:660` (`.slice => a.ptr == b.ptr and a.len == b.len`). At 20abdb50a.
- defect:          `std.meta.eql` on an entry compares the `descriptor.family` slice by address. Any two maps whose families live in different allocations compare unequal even with identical content: a clone and its source, or the same `font-codepoint-map` parsed twice. So `Config.changed(.@"font-codepoint-map")` would report a change on every reload with a non-empty map. By reading; `std.meta.eql`'s slice rule was read in the pinned stdlib.
- ReleaseSafe:     no product effect at the pin: `changeIterator` has no production caller, and the formatter compares against a default config whose map is empty (see ADJUDICATION.md S76-F5).
- ReleaseFast:     same.
- reachability:    none today; it becomes live as soon as a caller compares two loaded configs.
- port handling:   compares `family` by content (`crates/rustty-config/src/value_types.rs` `RepeatableCodepointMap::equal`, PORT NOTE there); unobservable, so LEDGER S76-F5 is WAIVED rather than reproduced.
- parity row(s):   LEDGER S76-F5.
- test:            `repeatable_codepoint_map_clone_equal` (`value_types.rs`) pins the port's content equality.
- proposed fix:    compare fields explicitly: ranges, then `std.mem.eql` on the optional family/style strings and the variations, then the scalar fields (or use `std.testing`-style deep equality).
- upstream status: not reported

### UD182 — `Envelope.serialize` panics on a header nested deeper than 256 levels, which `Envelope.parse` accepts
- kind:            latent (crash-class, no production caller)
- upstream project: ghostty (the panic itself is zig 0.16.0 `std.json.Stringify`'s documented fixed-depth assertion)
- severity:        low
- reference:       src/crash/sentry_envelope.zig:221-247 (`Envelope.serialize`, `std.json.fmt(std.json.Value{ .object = ... }, json_opts)` at :226 and :240); :63-90 (`parseHeader`, `parseFromSliceLeaky` at :79) and :105 (`parseOneItem`, `parseFromSliceLeaky` at :130). zig 0.16.0 `std/json/Stringify.zig:69-79` (`safety_checks_hint = .{ .checked_to_fixed_depth = 256 }`, "In ReleaseFast and ReleaseSmall mode ... always treated as .assumed_correct"), `:572` ("The maximum nesting depth of the output JSON document is 256"), `std/BitStack.zig:50`. At 20abdb50a.
- defect:          the parser accepts any depth (the `Scanner`'s `BitStack` is allocator-backed), but the stringifier supports only 256 levels and enforces it with an assertion, not an error. So an `Envelope` whose headers parse fine cannot be serialized: `serialize` panics instead of returning an error. MEASURED on zig 0.16.0 with a probe (`parseFromSliceLeaky(std.json.Value, ..)` then `std.json.fmt(v, .{ .whitespace = .minified })`): 256 levels round-trip byte-identically, for arrays and objects; 257 levels, and 100,000 levels, parse and then panic `index out of bounds: index 32, len 32` (`BitStack.zig:50` for arrays, `Stringify.zig:421` for objects), exit 134.
- ReleaseSafe:     panic (process abort).
- ReleaseFast:     the depth check is compiled out and the fixed 32-byte nesting stack in the `Stringify` struct is written past its end: undefined behaviour. By reading `Stringify.zig:69-79`; not measured.
- reachability:    none in production at the pin: `Envelope.serialize` is called only by the file's tests. `Transport.sendInternal` (`sentry.zig:312-363`) parses sentry-native's own envelope and writes the ORIGINAL bytes, never re-serializing. It becomes live as soon as something serializes a parsed envelope (for example a crash-report reader that rewrites one).
- port handling:   fixed (R13): `crates/rustty-crash/src/json.rs` `stringify` / `stringify_object` (PORT NOTE: reference defect (R13) in the module header and at `stringify`) compute the depth iteratively first and return an `std::io::ErrorKind::InvalidInput` error, writing nothing, past 256; `Envelope::serialize` passes it on as `Error::Io`. Parsing stays unlimited, as in the reference. The port's own depth hazards are closed as well: `json::Value` has an iterative `Drop` (the derived drop glue overflowed the 2 MB test-thread stack at about 20,000 levels, measured by the T-E reviewer), and `stringify` recurses at most 256 levels. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   LEDGER S79-F3 / S79-F4 (the std.json port); review-T-E blocking item 1.
- test:            `json::tests::stringify_rejects_depth_257_with_an_error` (negative control: the depth check removed from `stringify`, then separately from `stringify_object`, each fails on `unwrap_err`); `json::tests::stringify_round_trips_the_documented_256_levels`; `json::tests::parse_and_drop_any_depth_without_stack_overflow` (200,000 levels, arrays and objects; negative control: the iterative drop disabled aborts the test binary with "has overflowed its stack", SIGABRT); `sentry_envelope::tests::serialize_rejects_a_header_deeper_than_256_levels` (256-level header round-trips; 257 parses, `serialize` errors with nothing written; negative control: the `stringify_object` check removed fails on `unwrap_err`).
- proposed fix:    in `sentry_envelope.zig`, reject headers nested deeper than 256 levels when parsing (a depth count over the parsed `Value`, or a token-level count), so every parsed `Envelope` can be serialized; or check the depth in `serialize` and return an error. Upstream zig could also make `Stringify` return an error rather than assert on the documented limit.
- upstream status: not reported

### UD190 — FreeType `Face.name` overflows its buffer on a long UTF-16 family name
- kind:            crash (safety-checked trap) / undefined behaviour
- upstream project: ghostty
- severity:        low (a font file with an unusual family name)
- reference:       `src/font/face/freetype.zig:158-183` (`Face.name`: for a Windows Unicode family record of even length and at most 1024 bytes, `std.unicode.utf16LeToUtf8(buf, tmp[0..max]) catch return string`); zig 0.16.0 `std/unicode.zig:1000-1052` (`utf16LeToUtf8Impl` writes `utf8[dest_index..]` with no room check; `utf8Encode` asserts the room), at 20abdb50a
- defect:          `utf16LeToUtf8` reports decode errors but not a full output buffer: running out of room is a safety check, not an error. The record may hold 512 UTF-16 units, whose UTF-8 form can reach 1536 bytes (3 per BMP character from U+0800). Callers pass 1024 bytes, or 256: Collection's synthetic bold/italic log (`Collection.zig:486`, `:511`), Windows discovery's `familyMatches` (`discovery.zig:1207`), and the `DeferredFace.name` callers, which reach `Face.name` on `freetype_windows` (`SharedGridSet.zig:190`, `CodepointResolver.zig:202`, and the test `shaper/harfbuzz.zig:2205`). Measured (Zig 0.16.0, aarch64-macos, a probe with `freetype.zig:170-178` copied verbatim and 1024 canary bytes after the buffer): a 256-byte buffer overflows from 86 x U+6C34 (258 bytes as UTF-8), 129 x U+00E9 (258) or 257 ASCII characters (257); a 1024-byte buffer overflows from 342 x U+6C34 (1026), and no ASCII or two-byte name can overflow it. One character fewer fits in each case. The function's own answer for a name it cannot decode is the raw record (`catch return string`).
- ReleaseSafe:     panic, the same in Debug. Measured: "reached unreachable code" (`utf8Encode`'s assertion) for 86 x U+6C34, 129 x U+00E9 and 257 to 263 ASCII characters into 256 bytes, and for 342 or 400 x U+6C34 into 1024 bytes; from 264 ASCII characters into 256 bytes, "index out of bounds: index 264, len 256" (the vectorized ASCII fast path, 8 units per chunk on this target, whose slice fails before the scalar loop is reached).
- ReleaseFast:     the call returns a length past the buffer after writing past it, and `name` then returns `buf[0..len]`, a slice past the end of the caller's stack buffer: undefined behaviour. Measured: 256 bytes, 86 x U+6C34 returns 258 and writes 2 bytes past; 257 ASCII returns 257 and writes 1 byte past; 1024 bytes, 342 x U+6C34 returns 1026 and writes 2 bytes past, 400 x U+6C34 returns 1200 and writes 176 bytes past.
- reachability:    a font whose first family (`nameID` 1) record is Windows / Unicode BMP, loaded on a FreeType backend (Linux, FreeBSD, Windows), then a `Face.name` call whose buffer is too small for it: 86 or more CJK characters, 129 or more two-byte characters, or 257 or more ASCII characters for the 256-byte callers (Collection's synthetic bold/italic log, Windows discovery, the `DeferredFace.name` callers on Windows); 342 or more CJK characters for the 1024-byte ones (`+show-face`, `+list-fonts` on Windows, the CJK-width warning `freetype.zig:1072`, the variation-axes log of Debug builds `freetype.zig:121`). The name is read from the font file.
- port handling:   fixed (R13): `crates/rustty-font-shaper/src/face/freetype.rs` `Face::name` decodes with a bounded helper that fails when the output does not fit, and answers that failure as the reference answers a decode error, with the raw record. PORT NOTE: reference defect (R13) at the decode. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   LEDGER S91-F9.
- test:            `face::freetype::tests::face_name_utf16_record_too_long_for_the_buffer_is_returned_raw` (rustty-font-shaper; JetBrains Mono with a rebuilt `name` table), each measured boundary on both sides: into 256 bytes, 85 x U+6C34, 128 x U+00E9 and 256 ASCII are decoded, and 86, 129 and 257 return the raw record; into 1024 bytes, 341 x U+6C34 is decoded and 342 returns the raw record. Negative control, macOS 2026-09-23 (task T-F2): the no-room check panicking instead of returning None fails the test at the first overflowing case. Restored.
- proposed fix:    size `tmp` and the output together: decode into a buffer of `max * 3` bytes (or check `std.unicode.calcUtf8Len` against `buf.len` first) and `return string` when the name does not fit.
- upstream status: not reported

### UD200 — `openThread` spins forever logging empty lines once the opener writes a line to stderr, and never reaps it
- kind:            hang (busy loop) and leak (unreaped child)
- upstream project: ghostty
- severity:        medium
- reference:       `src/os/open.zig:79-101` (`openThread`, the `takeDelimiterExclusive('\n')` loop at :88-99), at 20abdb50a; zig 0.16.0 `std/Io/Reader.zig:852-876` (`takeDelimiterExclusive` "advancing the seek position up to (but not past) the delimiter", `toss(result.len)` with the delimiter excluded).
- defect:          `takeDelimiterExclusive` leaves the `'\n'` unread. The loop never tosses it, so every call after the first line finds the same `'\n'` at the read position and returns an empty slice. The loop logs `open stderr=` forever, never reads the rest of stderr, and never reaches `exe.wait(io)`: the thread spins at full CPU, floods the log, and the opener stays a zombie. MEASURED with zig 0.16.0 (`zig run`): the reference loop body over a 256-byte-buffered reader on `"hello\nworld\n" ++ "x" ** 300 ++ "\nend"` yields line 0 `hello`, then lines 1 to 11 empty (capped at 12 iterations by the probe). Not measured through a Ghostty binary.
- ReleaseSafe:     no crash; an endless busy loop in the detached thread, an endless stream of `warning(os-open): open stderr=` lines, and a zombie per such `open` call.
- ReleaseFast:     same.
- reachability:    every `internal_os.open` call (the fallback URL opener used by the `open_url` action when the apprt does not handle it: Linux/FreeBSD `xdg-open`, Windows `rundll32`, macOS `open` for non-OSC 8 kinds) whose opener writes a newline-terminated line to stderr, for example `xdg-open` reporting a missing handler or `open` reporting a missing file.
- port handling:   fixed (R13, extended 2026-09-23): `crates/rustty-os/src/open.rs` `log_stderr_lines` (PORT NOTE: reference defect (R13)) consumes the delimiter, so each line is logged once and the loop ends at end of stream, then `openThread` waits for the child. It keeps the reference's 256-byte buffer semantics: a line that fills the buffer with no newline is logged as a 256-byte piece (`StreamTooLong => take(buffer.len)`). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   LEDGER S112-F7.
- test:            `open::open_thread_tests::stderr_lines_are_logged_once_each_in_256_byte_pieces`, `stderr_line_at_the_buffer_boundary`, `open_thread_reaps_an_opener_that_writes_stderr` (rustty-os). Negative control (macOS, 2026-09-23): the delimiter left unconsumed (`consume(i)`, the reference's behaviour) fails all three, the first two on "stderr loop does not advance past a delimiter" and the third on "the opener was not reaped within 10 s".
- proposed fix:    use `reader.takeDelimiter('\n')` (which consumes the delimiter and returns `null` at end of stream) with `orelse break`, or `toss(1)` after each `takeDelimiterExclusive` that did not end at end of stream.
- upstream status: not reported

### UD201 — zig's `wtf8ToWtf16Le` panics on an overlong 3-byte sequence (`E0 80..9F xx`)
- kind:            crash (`catch unreachable`)
- upstream project: zig (the standard library), reached through ghostty on Windows
- severity:        low
- reference:       zig 0.16.0 `std/unicode.zig:235-300` (`utf8ValidateSliceImpl`: in `.can_encode_surrogate_half` mode the `E0` lead byte's class `s2` is `0x03`, "accept 0", so any continuation byte `80..BF` passes, where UTF-8 mode's `0x13` restricts it to `A0..BF`), `:1738` (`Wtf8Iterator.nextCodepoint`: `wtf8Decode(slice) catch unreachable`), `:155` (`utf8Decode3AllowSurrogateHalf` returns `error.Utf8OverlongEncoding`), `:1812` (`wtf8ToWtf16Le`). Reached from ghostty at 20abdb50a wherever a byte path is handed to the Windows file API, for example `src/os/path.zig:39-43` (`expand`'s `openFile`).
- defect:          the WTF-8 validator accepts the overlong form `E0 80..9F 80..BF`, and the decoder that runs after it rejects that form with an error the caller turns into `unreachable`. MEASURED with zig 0.16.0 Debug (`zig run`): `std.unicode.wtf8ToWtf16Le(&buf, "\xe0\x9f\xbf")` panics "attempt to unwrap error: Utf8OverlongEncoding".
- ReleaseSafe:     panic, the same as Debug. MEASURED (T-G2, 2026-09-23, zig 0.16.0 aarch64-macos, a probe calling `std.unicode.wtf8ToWtf16Le` on `E0 9F BF`, which `wtf8ValidateSlice` accepts and `utf8ValidateSlice` rejects): "attempt to unwrap error: Utf8OverlongEncoding" at `std/unicode.zig:1738` (`wtf8Decode(slice) catch unreachable`), in both Debug and ReleaseSafe.
- ReleaseFast:     undefined behaviour (`unreachable` reached). Not measured.
- reachability:    Windows only: any byte path or argument containing those three bytes that is converted for a Windows API, for example a `PATH` entry or command name in `os/path.zig` `expand`.
- port handling:   `crates/rustty-os/src/path.rs` `wtf8_to_wtf16le` (the Windows conversion for `expand`'s candidate paths and `open`'s argv, LEDGER S112-F4) rejects overlong forms as `error.InvalidWtf8`, which the reference's error arms already handle. Windows-only code: compile-checked for `x86_64-pc-windows-msvc`, never run (no Windows lane); the decoder itself is tested on every target.
- parity row(s):   LEDGER S112-F4.
- test:            `path::tests::wtf8_to_wtf16le_matches_zig` (last row), `wtf8_to_wtf16le_accepts_what_wtf8_validate_slice_accepts` (zig's own `testValidateWtf8Slice` table). Negative control (macOS, 2026-09-23): the overlong check (`cp < min`) removed fails `wtf8_to_wtf16le_matches_zig` with `Some([2047])` for `E0 9F BF`.
- proposed fix:    in zig, give `E0` its own class in the `.can_encode_surrogate_half` table (accept `A0..BF`, as in UTF-8 mode) so the validator and decoder agree.
- upstream status: not reported

### UD202 — `systemd.notify.send` writes one byte past `sun_path` when `NOTIFY_SOCKET` is exactly 108 bytes
- kind:            crash (bounds-check trap)
- upstream project: ghostty
- severity:        low
- reference:       `src/os/systemd.zig:108-117` (`notify.send`: `if (socket_address.path.len < socket_path.len) { log.warn("NOTIFY_SOCKET path is too long!") ... }` then `socket_address.path[socket_path.len] = 0;`), at 20abdb50a. Found by T-G while working LEDGER S112-F8; outside that row's cited anchors (`:37-53, 136-149`).
- defect:          the length check lets through a value exactly `path.len` (108 on Linux) bytes long, and the NUL terminator is then written at index 108 of a 108-byte array.
- ReleaseSafe:     panic. MEASURED (T-G2, 2026-09-23, zig 0.16.0 aarch64-macos, the reference's address setup, `systemd.zig:106-117`, run over `std.os.linux.sockaddr.un`, whose `path` is 108 bytes): a 107-byte path is accepted, a 109-byte path logs "NOTIFY_SOCKET path is too long!", and a 108-byte path panics "index out of bounds: index 108, len 108" at the NUL write, in both Debug and ReleaseSafe. The `socket`/`connect` calls after it were not run (they need Linux).
- ReleaseFast:     undefined behaviour: a one-byte write past `sun_path`, into whatever follows the stack `sockaddr.un`. Not measured.
- reachability:    Linux, whenever Ghostty calls `notify.ready()` or `notify.reloading()` with `NOTIFY_SOCKET` set to a 108-byte value in its environment (systemd sets the variable for `Type=notify` units; any launcher can set it).
- port handling:   fixed (R13): `crates/rustty-os/src/systemd.rs` `notify_address` (PORT NOTE: reference defect (R13)) rejects a value that leaves no room for the NUL with the reference's own "NOTIFY_SOCKET path is too long!" warning. The port had copied the reference's index and panicked the same way.
- parity row(s):   LEDGER S112-F8 (found alongside it).
- test:            `systemd::tests::notify_socket_as_long_as_sun_path_is_too_long` (runs on macOS, where `sun_path` is 104 bytes, and Linux). Negative control (macOS, 2026-09-23): the reference's `<` restored panics "index out of bounds: the len is 104 but the index is 104". Linux (task L-1, Linux VM rvm (Fedora 44 aarch64), 2026-09-26): the test passes, and end to end (a scratch test through `notify.ready()`) 107-, 108- and 109-byte values return without a panic (108 and 109 log the warning); with the reference's `<` restored the 108-byte value panics at `systemd.rs:352`.
- proposed fix:    `if (socket_address.path.len <= socket_path.len)`.
- upstream status: not reported

### UD203 — `systemd.notify.send` accepts an abstract-namespace `NOTIFY_SOCKET` (`@...`) but connects to a filesystem path named `@...`
- kind:            functional (a documented case that does not work)
- upstream project: ghostty
- severity:        low
- reference:       `src/os/systemd.zig:100-104` (the check and its warning: "only AF_UNIX sockets with path or abstract namespace addresses are supported!"), `:115-116` (the `@` is copied into `path` as is), `:136-141` (`connect` with `@offsetOf(sockaddr.un, "path") + path.len`, the whole struct), at 20abdb50a. `sd_notify(3)`: "If the first character of $NOTIFY_SOCKET is "@", the string is understood as Linux abstract namespace socket". Found by T-G alongside LEDGER S112-F8.
- defect:          an abstract address is written with a leading NUL byte in place of the `@`, and its length is the name's own. The reference does neither: it connects to the pathname socket `@name` relative to its working directory, which does not exist, so the notification is lost with "unable to connect to notify socket". By reading; not measured.
- ReleaseSafe:     no crash; `READY=1` / `RELOADING=1` are never delivered, so a `Type=notify` service whose manager hands out an abstract socket times out waiting for readiness.
- ReleaseFast:     same.
- reachability:    Linux, any `NOTIFY_SOCKET` starting with `@`.
- port handling:   fixed (R13, extended 2026-09-23): `crates/rustty-os/src/systemd.rs` `notify_address` (PORT NOTE: reference defect (R13)) writes the leading NUL and uses the name's length for an `@` address; path addresses keep the reference's whole-struct length. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   LEDGER S112-F8 (found alongside it).
- test:            `systemd::tests::notify_address_lengths` (macOS and Linux). Negative control (macOS, 2026-09-23): the NUL write removed fails with `left: 64` (the `@`) `right: 0`. Linux (task L-1, Linux VM rvm (Fedora 44 aarch64), 2026-09-26): the test passes, and end to end `READY=1` reaches a socket bound to the abstract name; with the reference's handling (the `@` kept, the whole-struct length) connect fails ENOENT and nothing is delivered.
- proposed fix:    `if (socket_path[0] == '@') socket_address.path[0] = 0;` and connect with `@offsetOf(sockaddr.un, "path") + socket_path.len` for that case.
- upstream status: not reported

### UD204 — repeating `--norc` or `--noprofile` in a bash `command` fails the surface: the inject flag buffer is 32 bytes
- kind:            functional (a terminal that fails to start)
- upstream project: ghostty
- severity:        low
- reference:       `src/termio/shell_integration.zig:302-326` (`setupBash`: `var buf: [32]u8`, `var inject: std.Io.Writer = .fixed(&buf)`, `try inject.writeAll(" --norc")` / `(" --noprofile")` once per occurrence of the flag); `src/termio/Exec.zig:800` (`const integration = try shell_integration.setup(...)`), at 20abdb50a.
- defect:          the buffer fits every distinct flag (`1 --norc --noprofile` is 20 bytes), but a repeated flag is appended again, and once the text passes 32 bytes `writeAll` fails. The error goes through `setup` and `Exec.zig:800`'s `try`, so the subprocess, and with it the surface, fails to start instead of starting bash with integration. MEASURED with zig 0.16.0: the fifth ` --norc` after `"1"` returns `error.WriteFailed` with `end=32`.
- ReleaseSafe:     no crash; the new window, tab or split fails to open with an error.
- ReleaseFast:     same.
- reachability:    a `command` (or default shell command) such as `bash --norc --norc --norc --norc --norc`, i.e. a user's own configuration, with automatic shell integration on (the default).
- port handling:   fixed (R13, extended 2026-09-23): `crates/rustty-termio/src/shell_integration.rs` `setupBash` (PORT NOTE: reference defect (R13)) accumulates the flags in a `String`, so every flag is passed on (the script splits them into words; a repeat is harmless) and setup succeeds. The port already did this; T-G recorded it as a deliberate fix. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   LEDGER S213-F2.
- test:            `shell_integration::tests::bash_repeated_inject_flags_do_not_fail_setup` (rustty-termio): 41 bytes of flags, setup succeeds with all of them. Negative control (macOS, 2026-09-23): a 32-byte cap on the accumulator returning an error (the reference's shape; the port's `Error` has no `WriteFailed`, so the control used its only variant) fails the test with "repeated flags must not fail setupBash".
- proposed fix:    skip a flag already present (append `--norc` / `--noprofile` at most once each), or size the buffer for the longest distinct combination and ignore repeats.
- upstream status: not reported

### UD205 — an OSC 4/10-19 color query of about 25 or more colors gets no answer, and the requests after it are skipped
- kind:            functional (output silently lost)
- upstream project: ghostty
- severity:        low
- reference:       `src/termio/stream_handler.zig:1545-1772` (`colorOperation`: `var buffer: [1024]u8`, `FixedBufferAllocator`, `var response: std.Io.Writer.Allocating = .init(alloc)` at :1554-1557; every report `try response.writer.print(...)` / `try response.writer.writeAll(terminator.string())`; the write request at :1764-1770), at 20abdb50a; zig 0.16.0 `std/Io/Writer.zig:2615-2644, 2698-2723` (`Allocating.ensureTotalCapacity` grows to `growCapacity(n)` = `n + n/2 + init_capacity`; `drain` asks for `bytes.len + splat_len + 1`).
- defect:          the reports go into an allocating writer backed by a 1024-byte buffer. When the writer cannot grow, the `try` returns `error.WriteFailed` from `colorOperation`: no report is written to the pty, and the rest of the request list, including any set or reset after the failing query, is never processed. Because of the growth policy the failure comes well before 1024 bytes. MEASURED with zig 0.16.0 (aarch64-macos, and x86_64-macos under Rosetta), the reference's own accumulation of 16-bit palette reports (CORRECTED by T-G2, 2026-09-23, after the T-G review; re-measured with a zig 0.16.0 probe, Debug and ReleaseFast identical): for indices 0, 1, 2, ... 25 reports fit (640 bytes with BEL, 665 with ST) and the 26th fails; the number depends on the indices' digit count: index 255 repeated, the 25th fails with BEL (648 bytes fit) and the 24th with ST (644); index 1 repeated, the 27th fails with BEL (650) and the 26th with ST (650). Measured range: the 24th to the 27th report; plain writes fail after 627 (1-byte writes) to 912 (24-byte writes) bytes. Not measured through a Ghostty binary.
- ReleaseSafe:     no crash; the query is unanswered (a program waiting for the reply stalls or times out), and later requests in the same sequence are dropped.
- ReleaseFast:     same.
- reachability:    any program writing an OSC 4 (or OSC 10-19) with about 25 or more `?` queries, e.g. one that reads the whole 256-color palette in a single sequence.
- port handling:   fixed (R13, extended 2026-09-23): `crates/rustty-termio/src/stream_handler.rs` `colorOperation` (PORT NOTE: reference defect (R13)) accumulates in a `Vec`, so every report is sent and every request is applied. The port already did this; T-G recorded it as a deliberate fix (the audit had suggested porting the cap). The request list is bounded by the OSC parser's input limit. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   LEDGER S214-F1.
- test:            `stream_handler::tests::color_operation_reports_every_query_and_applies_later_requests` (rustty-termio): 256 queries then a set; one write carrying all 256 reports, and the set applied. Negative control (macOS, 2026-09-23): a 1024-byte cap on the accumulator returning an error (the reference's shape, at the nominal size) fails the test with "colorOperation: Error".
- proposed fix:    accumulate with the surface's allocator (`self.alloc`) instead of the fixed buffer, or send the reports in chunks as each one is formatted; at the least, do not let a failed report stop the remaining requests.
- upstream status: not reported

### UD206 — `termio.Backend.childExitedAbnormally` delegates to a method `termio.Exec` does not have
- kind:            latent (uncompilable but unused code)
- upstream project: ghostty
- severity:        low
- reference:       `src/termio/backend.zig:88-100` (`.exec => |*exec| try exec.childExitedAbnormally(gpa, t, exit_code, runtime_ms)`), at 20abdb50a. `src/termio/Exec.zig` declares no `childExitedAbnormally`; the only other declaration is `Surface.zig`'s own private one (`:1277`, called at `:1330`), which does not go through the backend.
- defect:          dead code that would not compile: nothing calls `Backend.childExitedAbnormally`, so Zig's lazy analysis never checks it; the first caller would fail to build. By reading (a grep of `src/` for the name).
- ReleaseSafe:     no effect (never compiled).
- ReleaseFast:     same.
- reachability:    none.
- port handling:   the port has no `Backend::childExitedAbnormally`, so a Rust caller gets the same compile error (it was a public method whose body was `unimplemented!()`, which compiled and would have panicked). The reference is preserved as a comment with a PORT NOTE at `crates/rustty-termio/src/backend.rs`.
- parity row(s):   LEDGER S212-F3.
- test:            the `compile_fail,E0599` example on `rustty_termio::Backend` (a doctest). Negative control (macOS, 2026-09-23): the method restored, the doctest fails with "Test compiled successfully, but it's marked `compile_fail`".
- proposed fix:    delete `Backend.childExitedAbnormally` (Surface handles abnormal exits itself), or add the Exec method it expects.
- upstream status: not reported

### UD210 — the shell fallback for the home directory fails on a home path of 361 bytes or more, well inside its own 512-byte limit
- kind:            non-crash (evident intent broken)
- upstream project: ghostty
- severity:        low
- reference:       src/os/homedir.zig:49-67 (`homeUnix`: `var tempBuf: [1024]u8` at :49, `fba.reset()` at :61, `std.process.run(fba.allocator(), io, .{ ..., .stdout_limit = .limited(fba.buffer.len / 2), .stderr_limit = .limited(fba.buffer.len / 2) })`); :119-130 (`expandHomeUnix` maps any `home` error to `HomeDetectionFailed`). zig 0.16.0 `std/process.zig:496-530` (`run`: a `MultiReader` over stdout and stderr, allocating through the same allocator). At 20abdb50a.
- defect:          the call sets a limit of 512 bytes per stream (`fba.buffer.len / 2`), but `run` allocates the growing stream buffers from the same 1024-byte `FixedBufferAllocator`, which runs out first. MEASURED with a probe over the exact call (zig 0.16.0, ReleaseSafe, macOS, `HOME` set in the child's environment map): a home path of 300 and 360 bytes comes back (361 bytes of `pwd` output); 361, 362, 400, 450, 511, 512, 513 and 600 bytes fail with `error.OutOfMemory`. Stderr follows the limit exactly: 512 bytes of stderr return exit status 1, 513 bytes fail with `StreamTooLong` (MEASURED in task T-H3 with the same `run` options and a command writing exactly N bytes to stderr and exiting 1, the shape of a failing `cd`: 300, 511 and 512 OK; 513 and 514 `StreamTooLong`; the review of T-H2 found the same edge with the exact `cd && pwd` call). So on stdout the 512-byte limit is never reached.
- ReleaseSafe:     `home` returns `error.OutOfMemory`; `expandHome` returns `HomeDetectionFailed`, so `~/` paths are not expanded.
- ReleaseFast:     the same (no safety check is involved). MEASURED, identical to ReleaseSafe: 361 bytes of stdout OK and 362 or 512 bytes `OutOfMemory`; 512 bytes of stderr OK and 513 `StreamTooLong` (task T-H3's probe; the review of T-H2 measured the same with the exact call).
- reachability:    Linux and FreeBSD, when `HOME` is unset and passwd has no home directory for the user, and the shell's `cd` lands in a directory whose path is 361 to 511 bytes long. Rare. The Linux threshold is the same, MEASURED (task L-1, Linux VM rvm (Fedora 44 aarch64), 2026-09-26: a probe of the exact call, zig 0.16.0 cross-built for aarch64-linux-gnu, ReleaseSafe and ReleaseFast: 300 and 360 bytes OK, 361, 362, 400, 450, 511, 512 and 513 `OutOfMemory`); the port's `home_from_shell_returns_a_400_byte_home_the_reference_runs_out_of_memory_on` passes on Linux.
- port handling:   fixed (R13): `crates/rustty-os/src/homedir.rs` `home_from_shell` reads each pipe to at most 513 bytes and fails with `StreamTooLong` only past 512, so a home path up to 511 bytes is returned (PORT NOTE: reference defect (R13) on the function). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   LEDGER S111-F11; review-T-H blocking item 1.
- test:            `homedir::tests::home_from_shell_returns_a_400_byte_home_the_reference_runs_out_of_memory_on` (a real 400-byte directory; negative control: the stdout cap lowered to 361 bytes, the reference's measured failure point, fails with `Err(StreamTooLong)` where `Ok(Some(path))` is expected). `homedir::tests::home_from_shell_limits_stdout_and_stderr_to_512_bytes` still covers the limit itself.
- proposed fix:    give `run` an allocator that can hold both streams at their limit (a larger `tempBuf`, for example 4096 bytes, or the general-purpose allocator), or lower the limits so they fit: the buffers need more than `limit` bytes each while growing.
- upstream status: not reported

### UD211 — `TMPDIR=/` becomes an empty temp-dir path that the reference's own temp-dir code cannot open
- kind:            non-crash (evident intent broken)
- upstream project: ghostty
- severity:        low
- reference:       src/os/file.zig:58-66 (doc comment: "Any trailing path separator is stripped so callers can safely join with their own separator"), :85-86 (`return std.mem.trimEnd(u8, tmpdir, &.{std.fs.path.sep});`); src/os/TempDir.zig:32-34 (`cwd.openDir(global.io(), tmp_dir, .{})`). At 20abdb50a.
- defect:          `trimEnd` removes every trailing `/`, including the root directory's only one, so `TMPDIR=/` (or `TMP=/`, or `//`) yields `""`. MEASURED with zig 0.16.0 on macOS: `trimEnd("/")` has length 0; `Dir.cwd().openDir("")` fails with `FileNotFound`, while `openDir("/")` succeeds. So `TempDir.init` fails for a valid temp directory.
- ReleaseSafe:     `TempDir.init` returns `FileNotFound`. `randomTmpPath` still builds `"/<name>"`, which happens to work.
- ReleaseFast:     the same.
- reachability:    a user or environment that sets `TMPDIR=/` or `TMP=/`. Unusual, but a legal value.
- port handling:   fixed (R13): `crates/rustty-os/src/file.rs` `allocTmpDir` and `crates/rustty-global/src/environ.rs` `posix_tmp_dir_path` return `/` when the value is made only of separators (PORT NOTE: reference defect (R13) at both). A present-but-empty value still returns `""`, as in the reference (its intent is ambiguous; S111-F1), and `TempDir.init` then fails with `NotFound` as the reference's fails with `FileNotFound` (N172, task T-H3: `Dir::openDir`/`openDirPath` in `crates/rustty-os/src/tempdir.rs` reject an empty path; before, the port created the directory under the cwd). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   LEDGER S111-F1; review-T-H R13 section, controller ruling.
- test:            `file::tests::alloc_tmp_dir_keeps_the_root_directory` and `environ::tests::posix_tmp_dir_path_keeps_the_root_directory` (`/`, `///`, `TMP=/` all give `/`; empty stays `""`). Negative controls, one copy at a time: the root check disabled fails with `left: []`, `right: [47]`.
- proposed fix:    in `allocTmpDir`, keep one separator when the trimmed value would be empty and the original was not: `const t = std.mem.trimEnd(u8, tmpdir, "/"); return if (t.len == 0 and tmpdir.len > 0) "/" else t;`.
- upstream status: not reported

### UD230 — `std.Uri` percent-decodes `%-0` to a NUL byte and `%+X` to the byte `X`, in an OSC 7 path or host
- kind:            non-crash (evident intent broken)
- upstream project: zig (the standard library's URI decoder), reached through ghostty
- severity:        low
- reference:       Zig 0.16.0 `lib/std/Uri.zig:86-104` (`Component.formatRaw`: `std.fmt.parseInt(u8, percent_encoded[index..][0..2], 16) catch continue;`), also `:172-190` (`percentDecodeBackwards`, doc comment "Percent decodes all %XX where XX is a valid hex number", not reached from Ghostty); Ghostty `src/termio/stream_handler.zig:1493-1537` (`reportPwd`: `internal_os.uri.parse`, then `uri.getHost` and `uri.path.toRawMaybeAlloc`, both of which format through `formatRaw`). At 20abdb50a.
- defect:          RFC 3986's `pct-encoded` is `"%" HEXDIG HEXDIG`, and `formatRaw` passes every other invalid escape through unchanged (`catch continue`), but `parseInt` also takes a leading sign: `+` followed by one hex digit, and `-0`, are read as numbers. MEASURED with a probe over `Component.toRawMaybeAlloc` (Zig 0.16.0 ReleaseSafe and ReleaseFast, macOS, identical): `/a%-0b` -> `/a\x00b`, `/a%+Fb` -> `/a\x0fb`, `/a%+b` -> `/a\x0b`; `%_1`, `%1_`, `%-1`, `%zz` pass through. Over the task's 1940-input probe corpus placed after a `%`, 116 inputs decode through a sign.
- ReleaseSafe:     a malformed escape becomes a byte the sender never wrote: a NUL or a control character in the terminal's pwd (MEASURED up to the decoded path; the downstream chain is code-traced, not measured: then in the title while none was set, in the surface's pwd, and in a new tab's or split's `working-directory`; for a NUL the port's `exec.rs` test `osc7_percent_00_pwd_is_ignored_as_a_cwd_instead_of_panicking` records the reference's `cannot access cwd, ignoring` arm, UNVERIFIED here through a Ghostty binary). A `%-0` in the host makes the host not local, as `%00` would.
- ReleaseFast:     the same (no safety check is involved; MEASURED identical).
- reachability:    any program that writes OSC 7 with a malformed escape (`ESC ] 7 ; file://<local host>/tmp/a%-0b BEL`); a correct sender writes a literal `%` as `%25`. `kitty-shell-cwd://` paths are not decoded (`raw_path`), only their host.
- port handling:   fixed (R13): `crates/rustty-os/src/uri.rs` `parse_int_u8_hex` decodes an escape only when both bytes are hex digits, then parses them with `rustty_zigstd::fmt::parse_int::<u8>` (PORT NOTE: reference defect (R13) on the function). Every escape of two hex digits decodes as in the reference. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   NEW-FINDINGS N206 (the separate R13 question it raised); LEDGER S112-F9 had kept this faithfully.
- test:            `uri::tests::percent_escapes_decode_only_two_hex_digits` and `uri::tests::osc7_path_keeps_a_signed_escape` (the OSC 7 `parse` options, then the path formatted as `toRawMaybeAlloc` does; `%41` in the same path proves decoding ran). Negative control (macOS, task tail-s9b): the hex-digit check disabled, both fail with `left: [.., 97, 0, 98, ..]` (the NUL) where `[.., 97, 37, 45, 48, 98, ..]` (`a%-0b`) is expected.
- proposed fix:    in `formatRaw` and `percentDecodeBackwards`, decode only when both bytes pass `std.ascii.isHex` (or parse with `std.fmt.charToDigit` per byte instead of `parseInt`).
- upstream status: not reported

### UD245 — libvaxis's Windows key table sends F8 for F7
- kind:            latent (functional: a wrong key event)
- upstream project: libvaxis
- severity:        low
- reference:       `zig-pkg/vaxis-0.6.0-BWNV_CrbCQCscGpzsAlR402rYQ_tV3aAl081c2iRRkka/src/tty.zig:437` (`0x76 => Key.f8`, next to `0x77 => Key.f8` at :438), in the Windows console-input translation, the vaxis version Ghostty pins at 20abdb50a
- defect:          `0x76` is `VK_F7`, but the arm returns `Key.f8`; every other function key from `VK_F1` (`0x70`) to `VK_F24` (`0x87`) maps to its own key. By reading.
- ReleaseSafe:     no crash; on Windows a vaxis TUI receives F8 when the user presses F7, and cannot tell the two apart.
- ReleaseFast:     same.
- reachability:    keyboard input to a libvaxis TUI on Windows. Ghostty's own TUIs (`+list-themes`, `+boo`) bind neither F7 nor F8 (their `key.matches` calls, `src/cli/list_themes.zig:449-530`, `src/cli/boo.zig:59-60`, name letters, `/` and Escape), so neither Ghostty's nor Rustty's TUIs see a difference.
- port handling:   fixed (R13; controller ruling in task S-6b): `crates/rustty-vaxis/src/tty.rs` maps `0x76` to `key::f7` (PORT NOTE: reference defect (R13) at the arm). **Windows run pending**, and the Windows compile was not run either: the arm is inside the `#[cfg(windows)]` `event_from_record`, and a `--target x86_64-pc-windows-msvc` check of rustty-vaxis stops in `wuffs-sys`'s C build on this host (no Windows `stdlib.h`); `key::f7` is the same `u32` constant family as the neighbouring arms. Applied on the port's libvaxis 0.6.0 key table (after merging `tail-s10b`); the typo is in the pin's 0.6.0 source (`tty.zig:437`). Task tail-s6 had first kept it faithful. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   none; found by task tail-s6's sweep of PORT NOTEs.
- test:          none runnable here: the key table is inside Windows-only code that is not plain data, so no cross-platform test reaches it (controller ruling: mark Windows run pending).
- proposed fix:    `0x76 => Key.f7`.
- upstream status: not reported

### UD246 — libvaxis's `Loop.stop` waits forever when the terminal does not answer a status report
- kind:            deadlock (a TUI that never exits)
- upstream project: libvaxis
- severity:        low (the host terminal must not answer `ESC[5n`)
- reference:       `zig-pkg/vaxis-0.6.0-BWNV_CrbCQCscGpzsAlR402rYQ_tV3aAl081c2iRRkka/src/Loop.zig:63-75` (`stop`: set `should_quit`, write `deviceStatusReport`, then `thread.await`), the reader at :174-176 (`read_loop: while (!self.should_quit) { const n = try self.tty.read(...)`), `Vaxis.zig:1209-1212` (`ESC[5n`); callers `src/cli/list_themes.zig:308` (`defer loop.stop()`) and vxfw `App.zig:58` (`defer loop.stop()`, used by `src/cli/boo.zig`), at 20abdb50a
- defect:          the reader thread is blocked in a blocking `read` of the tty. `stop` wakes it only by asking the terminal for a device status report and relying on the reply to make `read` return; it then waits for the thread with no timeout. A terminal that does not answer `ESC[5n` never unblocks the read, so `stop` never returns.
- ReleaseSafe:     the TUI hangs when it exits, until more input reaches the tty (a keypress makes the `read` return and the reader sees `should_quit`). By reading at the pin; MEASURED on the port, whose loop is a faithful port: `crates/rustty-cli/tests/boo_pty_e2e.rs` arm B suppresses only the `ESC[0n` reply and the process is still alive 4.4 s after Escape, against 0.04 s to exit with the reply (`crates/rustty-cli/src/boo.rs:26-33`).
- ReleaseFast:     same.
- reachability:    the host terminal: running a Ghostty CLI TUI in a terminal or pty harness that does not implement DSR 5 (every mainstream terminal does). Any later input unblocks the read, so the hang ends at the next keypress.
- port handling:   fixed (R13; controller ruling in task S-6b): `crates/rustty-vaxis/src/loop_.rs`. `Loop::start` creates a close-on-exec wake pipe; the read thread (`tty_run`, POSIX arm) waits with `select` on the tty and the pipe's read end before each blocking `read`, and returns when the pipe is readable. `Loop::stop` still sets `should_quit` and writes the reference's `ESC[5n` first, then waits up to `STOP_DSR_GRACE` (1 s) for the read thread to end; only if it has not does it write one byte to the pipe; then it joins and closes the pipe. So a terminal that answers within the grace shuts down exactly as in the reference (the reader reads the `ESC[0n`), and one that never answers makes `stop` return after about 1 s instead of never. `select`, not `poll`: macOS `poll` reports `POLLNVAL` for `/dev/tty` (MEASURED 2026-09-24 with a pty child opening `/dev/tty`: `poll` revents 32, `select` works); the first version used `poll` and the reader still blocked. An fd at or above `FD_SETSIZE` keeps the reference's plain blocking read. The Windows arm (`ReadConsoleInputW`) is unchanged. A terminal that answers more than 1 s late (a slow remote link, for example) has its `ESC[0n` left on the tty for the next program, usually the shell, where the reference would have waited for it; this is the cost of any bounded wait. `EINTR` retries the `select` itself (task S-6c, review n3), so the loop condition is tested only where the reference tests it. The wait and the grace are the helpers `wait_tty_or_wake` and `wake_reader_after_grace` since task S-6c. PORT NOTEs: reference defect (R13) at the field, `start`, `stop`, the helpers and the read loop. Task tail-s6 had first kept it faithful. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   none; the port's `boo_pty_e2e` KAT used the hang as its negative control and now measures the bounded stop.
- test:            Unit (not ignored, task S-6c): `loop_::stop_wake_tests::a_terminal_that_never_answers_is_woken_after_the_grace` runs `wait_tty_or_wake` on a thread over a pipe standing in for a terminal that never answers, calls `wake_reader_after_grace` with a 200 ms grace, and asserts it used the pipe and the reader ended within 5 s; `a_terminal_that_answers_ends_the_read_without_the_wake` writes `ESC[0n` and asserts the pipe is not used. Negative control (macOS, task S-6c, run alone, `wake_reader_after_grace` not writing the pipe, i.e. the reference's plain wait): fails `the reader is still blocked 5 s after the wake: stop would hang (UD246)`. (`tty_run` returns at once in test builds, as the reference's does, so the unit tests drive the helpers.) KAT (manual, `#[ignore]`d, no CI lane runs it): `crates/rustty-cli/tests/boo_pty_e2e.rs` (`#[ignore]`d for runtime; run with `--ignored` after `cargo build -p rustty --bin rustty`). Arm A (harness answers `ESC[5n`): exits 0 with `ELAPSED_AFTER_QUIT < 0.5` s (measured 0.018 s), so an answering terminal is not delayed by the grace. Arm B, renamed `boo_without_dsr_reply_exits_after_the_grace` (reply suppressed): exits 0 with `ELAPSED_AFTER_QUIT >= 0.9` s (measured 1.011 s) and at least 10 frames, which shows the wake pipe ended the read. Negative control (macOS, task S-6b, arm B run alone with `loop_.rs` reverted to the reference behaviour and the binary rebuilt): FAILS `with the ESC[5n reply suppressed +boo did not exit` (`EXITED=0`, still alive 4.402 s after ESC). Re-run after merging `tail-s10b` (the port's move to libvaxis 0.6.0, whose `Loop` this fix is applied to; the defect is verified in the pin's 0.6.0 source, `Loop.zig:63-75`): the fixed tree passes both arms 3 of 3 (arm B 1.003-1.049 s, arm A 0.024-0.052 s), and the reverted tree fails arm B the same way (4.397 s). `.port/tools/boo_pty_kat.py`'s docstring notes the change.
- proposed fix:    have the reader thread wait on the tty together with a self-pipe that `stop` writes to (with `select` or kqueue on macOS, where `poll` does not support `/dev/tty`), instead of relying only on the terminal's reply; the port keeps the reply as the first wake and uses the pipe after a grace.
- upstream status: not reported

### UD247 — `pkg/opengl` `Program.createVF` leaks its shader objects when compilation fails, and the program when linking fails
- kind:            leak
- upstream project: ghostty
- severity:        low (a driver-side compile or link failure)
- reference:       `pkg/opengl/Program.zig:29-44` (`createVF`), reached from `src/renderer/opengl/Pipeline.zig:42` for every pipeline, including custom shaders, at 20abdb50a
- defect:          each `defer x.destroy()` is registered only after that shader's `try setSourceAndCompile` succeeds, so a vertex-shader compile failure leaks `vs`, and a fragment-shader failure leaks `fs`. There is no `errdefer p.destroy()`, so a failing `attachShader` or `link` leaks the program object. By reading.
- ReleaseSafe:     no crash; GL shader and program objects leak in the GL context, once per failing pipeline creation.
- ReleaseFast:     same.
- reachability:    a GLSL shader that glslang accepts but the GL driver fails to compile or link (a driver limit or bug), for example a custom shader recompiled on each config reload. Whether a real driver rejects a shader glslang accepted is UNVERIFIED. The objects belong to the surface's GL context and go away with it.
- port handling:   fixed (R13; controller ruling in task S-6b): `crates/rustty-renderer/src/opengl/gl/program.rs` `Program::create_vf` destroys `vs` when its compile fails, `fs` when its compile fails, and the program when an attach or the link fails; the success path is unchanged (PORT NOTE: reference defect (R13) above the function). **Run on the GL lane by task L-1** (task L-1, Linux VM rvm (Fedora 44 aarch64), 2026-09-26, Mesa 26.1.4 llvmpipe, `RUSTTY_REQUIRE_GL=1 LIBGL_ALWAYS_SOFTWARE=1`): the test below passes. Task S-6b's host had no GL context. Checked here only by a temporary, reverted macOS build of the module (the target-gated `gl`/`khronos-egl` deps and `pub mod opengl` enabled for macOS, the build-generated GLSL stubbed): `cargo check --tests` and `cargo clippy --all-targets -D warnings` of rustty-renderer were clean. Task tail-s6 had first kept it faithful. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   none; found by task tail-s6's sweep of PORT NOTEs.
- test:          `opengl::gl::program::create_vf_leak_tests::create_vf_failures_leave_no_shader_or_program_behind` (live GL, `test_support::offscreen_gl`, so it runs on the Ubuntu GL lane under `RUSTTY_REQUIRE_GL=1` and does not exist on macOS): records the shader and program names GL reports live (`glIsShader` / `glIsProgram` over 1..=512), checks a valid pair links and its destroy restores that set, then asserts a vertex compile failure, a fragment compile failure and a link failure (a fragment input no vertex stage writes) each leave it unchanged. Passes on Linux/llvmpipe (task L-1), which does reject the test's link. Negative controls (Linux, run alone): without the `vs.destroy()` on a vertex compile failure it fails `vertex compile failure: create_vf left GL shader or program objects behind` (`left: ([1], [])`); without the `p.destroy()` on a link failure it fails `link failure: ...` (`left: ([1, 2], [3])`: the program, and the two shaders it still holds).
- proposed fix:    register `defer vs.destroy()` / `defer fs.destroy()` right after each `Shader.create`, and add `errdefer p.destroy()` after `create()`.
- upstream status: not reported

### UD248 — synthetic bold and italic faces compute the regular face's size and then ignore it
- kind:            latent (a comment the code contradicts; unobservable at the pin)
- upstream project: ghostty
- severity:        low
- reference:       `src/font/Collection.zig:480-484` (`syntheticBold`) and `:505-509` (`syntheticItalic`), at 20abdb50a
- defect:          both functions say "Inherit size from regular; it may be different than opts.size due to scaling adjustments", build `face_opts` with `face_opts.size = regular.size`, and then pass `opts.faceOptions()` instead of `face_opts` to `regular.syntheticBold` / `syntheticItalic`, so the synthetic face gets the collection's size, not the regular face's.
- ReleaseSafe:     none at the pin: `completeStyles` synthesizes only from style faces discovered before any fallback is added, and those are added with `size_adjustment = .none` (`src/font/SharedGridSet.zig:208`, `:239`), so their scale factor is 1 and `regular.size` equals the collection's size (by reading `Collection.add`, `:113-150`, and `scaleFactor`, `:593-599`). A synthetic face would get the wrong size only if a scaled face were ever synthesized from.
- ReleaseFast:     same.
- reachability:    none today.
- port handling:   faithful: `crates/rustty-font-shaper/src/collection.rs` keeps the dead `face_opts` in both functions with PORT NOTEs (`let _ = face_opts;`). Decided by task tail-s6: unobservable, so not changed.
- parity row(s):   none; found by task tail-s6's sweep of PORT NOTEs.
- test:            n/a (record only).
- proposed fix:    pass `face_opts` to `syntheticBold` / `syntheticItalic`, or delete it and the comment.
- upstream status: not reported

### UD249 — libvaxis `Key.matchShiftedCodepoint` clears the lock modifiers on a copy it never reads
- kind:            latent (a dead store; unobservable in Ghostty)
- upstream project: libvaxis
- severity:        low
- reference:       `zig-pkg/vaxis-0.6.0-BWNV_CrbCQCscGpzsAlR402rYQ_tV3aAl081c2iRRkka/src/Key.zig:90-101`, the vaxis version Ghostty pins at 20abdb50a
- defect:          the function builds `tgt_mods` from the caller's `mods` with `caps_lock` and `num_lock` cleared, then compares `self_mods` against the original `mods` (`std.meta.eql(self_mods, mods)`), so a pattern that names a lock modifier can never match a shifted codepoint. By reading.
- ReleaseSafe:     none in Ghostty: its TUIs never pass a lock modifier in a pattern (`src/cli/list_themes.zig:449-530`, `src/cli/boo.zig:59-60`); the event's own lock bits are cleared on `self_mods`, as intended.
- ReleaseFast:     same.
- reachability:    a libvaxis application that matches a shifted key with `caps_lock` or `num_lock` set in the pattern. None in Ghostty.
- port handling:   faithful: `crates/rustty-vaxis/src/key.rs` `Key::match_shifted_codepoint` keeps the dead store with a PORT NOTE. Decided by task tail-s6: unobservable in the product.
- parity row(s):   none; found by task tail-s6's sweep of PORT NOTEs.
- test:            n/a (record only).
- proposed fix:    compare against `tgt_mods`: `std.meta.eql(self_mods, tgt_mods)`.
- upstream status: not reported

### UD250 — `pkg/opengl` `Framebuffer.bind` saves the draw binding whatever target it binds, and `present` binds the read target
- kind:            latent (unobservable where Ghostty calls it)
- upstream project: ghostty
- severity:        low
- reference:       `pkg/opengl/Framebuffer.zig:23-31` (`bind` queries `GL_FRAMEBUFFER_BINDING`, the draw binding, for every target) and `:66-71` (`unbind` restores that value on the bound target); caller `src/renderer/OpenGL.zig:312` (`present`: `target.framebuffer.bind(.read)`), at 20abdb50a
- defect:          binding `.read` saves the DRAW framebuffer name and `unbind` then writes it to the READ target, so after `present` the read binding is whatever the draw binding was, not what it was before.
- ReleaseSafe:     no visible effect expected: whenever the read and draw bindings are the same framebuffer before `present`, the restored value is the right one. That they are the same on every apprt and driver is UNVERIFIED (by reading; not measured on a driver). The other callers (`src/renderer/opengl/Target.zig:45`, `Pipeline.zig:53`, `RenderPass.zig:72,74`) bind `.framebuffer`, where the two coincide.
- ReleaseFast:     same.
- reachability:    none known.
- port handling:   faithful: `crates/rustty-renderer/src/opengl/gl/framebuffer.rs` `Framebuffer::bind`, and the port's `present` binds `Target::READ` as the reference does (`crates/rustty-renderer/src/opengl/opengl_impl.rs`). Task tail-s6 corrected the PORT NOTE there, which said every reference caller binds `.framebuffer`: `present` binds `.read`.
- parity row(s):   none; found by task tail-s6's sweep of PORT NOTEs.
- test:            n/a (record only).
- proposed fix:    query `GL_READ_FRAMEBUFFER_BINDING` for `.read` and `GL_DRAW_FRAMEBUFFER_BINDING` for `.draw` (and `.framebuffer`).
- upstream status: not reported

### UD251 — the OpenGL `Pipeline.deinit` destroys the program but not the framebuffer and vertex array it created
- kind:            leak
- upstream project: ghostty
- severity:        low (two GL object names per pipeline, per shader rebuild)
- reference:       `src/renderer/opengl/Pipeline.zig:40-70` (`init` creates `program`, `fbo` at :51 and `vao` at :56, each with an `errdefer ... destroy()`) and `:72-74` (`deinit` destroys only `program`); callers `src/renderer/opengl/shaders.zig:137-153` (`Shaders.deinit`), which `src/renderer/generic.zig:1695-1699` runs when `reinitialize_shaders` is set, i.e. after a config change of the blending mode (`:2106-2112`) or of the custom shaders (`:2118-2119`), at 20abdb50a
- defect:          `init` owns three GL objects, as its errdefers show, but `deinit` releases one. Every time the shaders are rebuilt in the same GL context, each old pipeline's framebuffer and vertex array names stay allocated.
- ReleaseSafe:     no crash; GL objects leak in the surface's GL context on each rebuild. By reading; not measured on a driver.
- ReleaseFast:     same.
- reachability:    config reload on the OpenGL renderer (Linux, FreeBSD): changing `alpha-blending` or `custom-shader`. The objects go away when the surface's GL context is destroyed.
- port handling:   fixed (R13; controller ruling in task S-6b): `crates/rustty-renderer/src/opengl/pipeline.rs` `Pipeline::deinit` destroys `vao` and `fbo` after the program (PORT NOTE: reference defect (R13) at `deinit`). **Run on the GL lane by task L-1** (task L-1, Linux VM rvm (Fedora 44 aarch64), 2026-09-26, Mesa llvmpipe, `RUSTTY_REQUIRE_GL=1`); task S-6b had checked it only by the same temporary macOS build as UD247. Task tail-s6 had first kept it faithful. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   none; found by task tail-s6's sweep of PORT NOTEs.
- test:          `opengl::pipeline::deinit_leak_tests::deinit_releases_the_framebuffer_and_vertex_array` (live GL, runs on the Ubuntu GL lane): builds a pipeline from a minimal valid shader pair, asserts `glIsFramebuffer`/`glIsVertexArray` are true for its names, calls `deinit`, and asserts the program, framebuffer and vertex array are all gone. Passes on Linux/llvmpipe (task L-1). Negative control (Linux, the reference's program-only `deinit`): fails `deinit must destroy the framebuffer init created` (`left: 1 right: 0`).
- proposed fix:    destroy `fbo` and `vao` in `deinit` as well (after the program, the reverse of `init`'s order).
- upstream status: not reported

### UD252 — libvaxis's read loop carries a partial sequence one byte too far and then parses only the new bytes
- kind:            crash (safety-checked trap) / undefined behaviour; and non-crash (input lost or garbled)
- upstream project: libvaxis
- severity:        low (tty input to a CLI TUI)
- reference:       `zig-pkg/vaxis-0.6.0-BWNV_CrbCQCscGpzsAlR402rYQ_tV3aAl081c2iRRkka/src/Loop.zig:172-196` (`_ttyRun`: `var buf: [1024]u8`, `read_start`; `const n = try self.tty.read(buf[read_start..])` at :176; `parser.parse(buf[seq_start..n], ...)` at :179; the carry and `read_start = seq_start - initial_start + 1` at :181-188), the libvaxis version the pin's `build.zig.zon` names (20abdb50a)
- defect:          when the parser needs more input (`result.n == 0`) the loop copies the unparsed tail to the front of `buf` ("copy the read to the beginning") and sets `read_start` to ONE PAST the carried length. The next `read` fills `buf[read_start..]` and returns `n`, the count of NEW bytes, and parsing then covers `buf[0..n]`: the carried bytes, one stale byte, and only the first `n - carried - 1` new ones. Two consequences. (1) A sequence split across two reads is parsed with a stale byte in it and its last bytes cut off, so the key or report is lost or garbled. (2) A full 1024-byte read that is all one unfinished sequence (the parser answers `n: 0` for an OSC, DCS or APC until its terminator arrives) carries 1024 bytes and sets `read_start = 1025`, and the next `buf[read_start..]` is out of range. By reading the pinned source; measured on the port's line-for-line translation (see test).
- ReleaseSafe:     (2) panics on the slice ("index out of bounds") on the TUI's read thread and the process aborts; (1) no crash, lost or garbled input.
- ReleaseFast:     (2) undefined behaviour (a slice start past the end); (1) the same as ReleaseSafe.
- reachability:    tty input to a libvaxis TUI (`+list-themes`, `+boo`): a key or report sequence that the kernel delivers in two reads (a slow link, or a large paste), or more than 1 KiB inside an unterminated `ESC ]` / `ESC P` / `ESC _` from the terminal or pasted by the user.
- port handling:   fixed (R13; controller ruling in task S-6c): `crates/rustty-vaxis/src/loop_.rs` `ReadCarry` (the pair `buf` / `read_start`) with `read_buf` (where the next read goes) and `process` (the reference's inner parse loop): it parses the carried bytes plus the new ones (`buf[0..read_start + n]`) and carries exactly the unparsed tail (`read_start = carried`). A carried sequence that fills the whole buffer cannot grow, so it is dropped (`read_start = 0`) and parsing restarts with the next read: its bytes are lost and the rest of that sequence is parsed as ordinary input, where the reference crashes. The parser itself answers a UTF-8 character split between reads with U+FFFD rather than asking for more (measured in the port's test; not changed here). PORT NOTE: reference defect (R13) at `ReadCarry` and the read loop. The Windows arm has no carry and is unchanged. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   none; found by the review of task S-6 (item 6.2).
- test:            `loop_::read_carry_tests::a_sequence_split_across_reads_parses_whole` (an Up arrow split after `ESC[`, and Ctrl+Up split inside its parameters, each followed by a letter: every key arrives) and `a_full_buffer_of_one_unfinished_osc_does_not_panic` (1024 bytes of an unterminated OSC, then its end, then `ESC[A`: no panic, the buffer is whole again, and the Up key arrives). Negative control (macOS, task S-6c, each run alone, `process` given the reference's arithmetic: parse `[0..n]`, `read_start = carried + 1`): the first fails `left: [], right: [57352, 98]` (the Up key and the `b` after it lost), the second panics `range start index 1025 out of range for slice of length 1024`.
- proposed fix:    set `read_start = seq_start - initial_start` (no `+ 1`) and parse `buf[seq_start..read_start + n]` on the next pass (or keep a separate `end = read_start + n`); decide what to do when a partial sequence fills the whole buffer (grow the buffer, or drop the sequence) instead of slicing past it.
- upstream status: not reported

### UD260 — libvaxis's grapheme start is the re-encoded length of U+FFFD back from the cursor: an invalid byte that starts a cluster underflows or re-includes the bytes before it
- kind:            crash (safety-checked trap) / undefined behaviour; and non-crash (a wrong value)
- upstream project: libvaxis
- severity:        low (non-UTF-8 text drawn by a CLI TUI)
- reference:       libvaxis 0.6.0 (pinned by ghostty 20abdb50a `build.zig.zon:25-30`) `src/gwidth.zig:58-63` (`gwidth`, `.unicode`: `const cp_len: usize = std.unicode.utf8CodepointSequenceLength(result.code_point) catch 1; grapheme_start = grapheme_iter.i - cp_len;`) and `src/unicode.zig:28-35` (`GraphemeIterator.next`: `self.start = self.inner.i - cp_len;`), over uucode 2826a37a `src/utf8.zig:64-78` (an invalid sequence yields `0xFFFD` after consuming the bytes up to the one that rejected). Both already so in libvaxis 0.5.1 (`result.cp`).
- defect:          the start of a multi-code-point cluster is recomputed as the iterator position minus the UTF-8 length of the DECODED code point, which is not the number of bytes the decoder consumed when the input is not UTF-8: a lone invalid byte decodes to U+FFFD (3 bytes when re-encoded) after consuming 1. When such a byte starts a cluster (it is followed by a combining mark, a ZWJ, a variation selector, …) at offset 0 or 1 the subtraction underflows; later in the string the start lands two bytes early, inside the previous grapheme.
- ReleaseSafe:     `panic: integer overflow` at `gwidth.zig:62`, MEASURED in a Debug build (the same safety checks as ReleaseSafe; Zig 0.16.0, a probe built against libvaxis 0.6.0 + uucode 2826a37a with ghostty's `uucode_config.zig`; e.g. `FF CC 81`). `GraphemeIterator` yields `{start 0, len 5}` for the third grapheme of `a b FF CC 81` after `{0,1}` and `{1,1}`, so a caller that prints graphemes prints "ab" twice (MEASURED: 35 such inputs and 36 traps in 20,000 random byte strings; `gwidth`'s total was not changed by the early start on any of them, because it takes the widest code point of the cluster and the two extra bytes are at most a narrow character).
- ReleaseFast:     the subtraction wraps; `gwidth` then slices `str[grapheme_start..grapheme_end]` with a start past the end (undefined behaviour with safety off), and `GraphemeIterator` returns a grapheme whose `start` is near `usize` max (by reading).
- reachability:    any text a libvaxis TUI measures or splits into graphemes that is not UTF-8: in Ghostty's CLI, a theme file name drawn by `+list-themes` (file names are arbitrary bytes on Linux), and key text or pastes read from the tty. `Window.print` and `TextInput` go through `GraphemeIterator`; `gwidth` is called per grapheme and on whole strings.
- port handling:   fixed (R13): `crates/rustty-vaxis/src/gwidth.rs` (`gwidth`, `.unicode` arm) and `crates/rustty-vaxis/src/unicode.rs` (`GraphemeIterator::next`) read the iterator's `i` before `next_code_point` and use it as the cluster start, which is the position the reference means and equals `i - cp_len` for every UTF-8 input. PORT NOTE: reference defect (R13) at both sites. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   NEW-FINDINGS N240 (found by the S-10b gwidth/grapheme differential).
- test:            `gwidth::tests::gwidth_invalid_byte_starting_a_cluster` (`FF CC 81` is width 1, `a FF CC 81` width 2, the NoZwj arm too, and a valid cluster after `中` unchanged) and `unicode::tests::invalid_byte_starting_a_cluster` (`FF CC 81` is one grapheme `(0,3)`; `a b FF CC 81` is `(0,1) (1,1) (2,3)`; a valid cluster after `中` unchanged). Written first; before the fix (the reference's `i - cp_len`) both failed with "attempt to subtract with overflow" at the subtraction (`gwidth.rs:107`, `unicode.rs:45`), macOS 2026-09-23. Differential after the fix, 1,334,878 inputs (every code point alone, the 766 `GraphemeBreakTest.txt` sequences, 200,000 random sequences over emoji / Hangul / Indic / control code points, 20,000 random byte strings): `gwidth` identical to the Zig oracle for all three methods except the 36 inputs on which the reference traps; `GraphemeIterator` identical except those 36 and the 35 overlapping graphemes above.
- proposed fix:    take the start from the iterator before advancing (`const start = grapheme_iter.i;` before `nextCodePoint`), or use `grapheme_iter.i - (bytes consumed)`; the same in `unicode.zig`.
- upstream status: not reported

### UD261 — libvaxis's render treats "the secondary cursors are unchanged" as a change, and a real change as none
- kind:            non-crash (evident intent broken)
- upstream project: libvaxis
- severity:        low (a repaint on every frame while the cursor is shown; secondary cursors never drawn by a render that changes nothing else)
- reference:       libvaxis 0.6.0 (pinned by ghostty 20abdb50a `build.zig.zon:25-30`) `src/Vaxis.zig:390-391` (`const cursor_secondary_changed = self.screen.cursor_vis and std.meta.eql(self.screen.cursor_secondary, self.state.cursor_secondary);`), used for `needs_render` at `:392-397` and to record the drawn slice at `:820-823`. `std.meta.eql` on two slices is `a.ptr == b.ptr and a.len == b.len` (Zig 0.16.0 `std/meta.zig:657-661`).
- defect:          the comparison is the wrong way round. The name, its use in `needs_render` (the new "only write when something changed" render) and the `if (cursor_secondary_changed)` block that records the screen's slice as the drawn one all mean "the secondary cursors differ from the last frame". As written: (a) with the cursor visible and no secondary-cursor change, every `render` repaints (the frame prologue, the cursor, `show_cursor`), which defeats the 0.6.0 optimisation that `test "render: no output when no changes"` covers only for a hidden cursor; (b) after `addTerminalSecondaryCursor`, a render that changes nothing else writes nothing, so the new secondary cursor is not shown, and the state never records the new slice.
- ReleaseSafe:     MEASURED (Zig 0.16.0, a probe over libvaxis 0.6.0 + ghostty's uucode): with a visible cursor and nothing changed, a render writes the same 40 bytes again; with `caps.multi_cursor` and one secondary cursor added, the render writes 0 bytes. With the one-character fix the same probe writes 0 bytes and then the 61-byte frame with `ESC [ > 0 ; 4 SP q ESC [ > 29 ; 2 : 1 : 4 SP q`.
- ReleaseFast:     the same (no safety check is involved).
- reachability:    any libvaxis application that shows the cursor (a repaint per frame); secondary cursors are used only by applications that call `addTerminalSecondaryCursor` on a terminal that answers the multi-cursor query. In Ghostty's CLI, `+list-themes` shows the cursor in its search box.
- port handling:   fixed (R13): `crates/rustty-vaxis/src/vaxis.rs` `render_diff` negates the comparison (`!ptr_eql`) and gates it on `caps.multi_cursor` (task tail-s10c, review F1): the drawn slice is recorded only under `cursor_vis and caps.multi_cursor` (reference `Vaxis.zig:816-824`; port `vaxis.rs` `render_diff`), so without the gate a secondary cursor added on a terminal without the capability stays "changed" and every render repaints (measured by the review: 40 bytes per render). PORT NOTE: reference defect (R13). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   NEW-FINDINGS N240.
- test:            `vaxis::tests::render_scenarios_match_the_reference` replays the probe's eleven renders and compares every byte with the Zig oracle's output (with the fix applied to the oracle; steps 5, 6, 7 and 11 are the ones the unpatched reference renders differently). Negative control (macOS 2026-09-23): the reference's comparison restored, step 5 fails with the 40-byte repaint where nothing is expected. `vaxis::tests::secondary_cursor_without_multi_cursor_does_not_repaint` (tail-s10c): cursor shown, `caps.multi_cursor` false, one secondary cursor added, three renders write nothing. Negative control (macOS 2026-09-24): the `caps.multi_cursor` gate removed, the first render writes the 40-byte repaint.
- proposed fix:    `const cursor_secondary_changed = self.screen.cursor_vis and self.caps.multi_cursor and !std.meta.eql(self.screen.cursor_secondary, self.state.cursor_secondary);` (the capability gate matches the block that records the slice; or record the slice outside that block).
- upstream status: not reported

### UD262 — libvaxis's `translateMouse` traps when a cell is wider or taller than 32767 pixels
- kind:            crash (safety-checked trap) / undefined behaviour
- upstream project: libvaxis
- severity:        low (a buggy or hostile terminal)
- reference:       libvaxis 0.6.0 `src/Vaxis.zig:916-917` (`const xcell: i16 = @intCast((self.screen.width_pix - xextra) / self.screen.width);`, and `ycell` from `height_pix`), in `translateMouse` (`:906-924`), called from `Loop.handleEventGeneric` for every mouse event while pixel mouse reporting is on.
- defect:          0.6.0 made the mouse coordinates `i16` and casts the cell size to `i16` with `@intCast`, which is not guaranteed: the pixel and cell sizes are the terminal's (the kernel winsize it sets, or its in-band resize report, both `u16`), so a report of, e.g., 40000 pixels for one column gives a cell width of 40000.
- ReleaseSafe:     a safety-check panic at the `@intCast` (by reading the Zig `@intCast` semantics; not run against a terminal).
- ReleaseFast:     undefined behaviour.
- reachability:    a terminal that reports such sizes while a libvaxis TUI has pixel mouse reporting on (`setMouseMode` with `caps.sgr_pixels`): `+list-themes` enables mouse reporting and uses pixel mode when the terminal answers the SGR-pixels query.
- port handling:   fixed (R13): `crates/rustty-vaxis/src/vaxis.rs` `translate_mouse` takes the reference's own exit for a degenerate cell size (it already returns the event untranslated when a cell is 0 pixels) when the size does not fit `i16`. PORT NOTE: reference defect (R13). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   NEW-FINDINGS N240.
- test:            `vaxis::tests::translate_mouse_pixel_coordinates` (negative pixel positions give negative cells with a non-negative offset, as `@divFloor`/`@mod` do; then a 40000-pixel column returns the event unchanged). Negative control (macOS 2026-09-23): the casts made `try_from(..).unwrap()` (the reference's trap) panic at the cast.
- proposed fix:    `std.math.cast(i16, ...) orelse return mouse` for `xcell` and `ycell`.
- upstream status: not reported

### UD263 — vxfw's mouse handler traps on a negative mouse position
- kind:            crash (safety-checked trap) / undefined behaviour
- upstream project: libvaxis
- severity:        low (terminal input to a vxfw application)
- reference:       libvaxis 0.6.0 `src/vxfw/App.zig:358-361` (`updateMouse`) and `:417-420` (`handleMouse`): `const mouse_point: vxfw.Point = .{ .row = @intCast(mouse.row), .col = @intCast(mouse.col) };`, with `vxfw.Point` `u16` (`src/vxfw/vxfw.zig:314-317`) and `Mouse.col`/`row` `i16` (`src/Mouse.zig:44-45`).
- defect:          0.6.0 widened the mouse coordinates to `i16` so that negative positions can be reported (libvaxis PR 276), but vxfw still casts them to the unsigned `Point` with `@intCast`. The parser produces a negative position from any SGR report with a coordinate of 0 or below: `CSI < 0 ; 0 ; 0 M` decodes to column -1, row -1 (MEASURED with libvaxis 0.6.0, S-10b probe). vxfw's `App.run` turns SGR-pixel reporting off before enabling the mouse (`App.zig:78-80`), so it receives cell reports, which conforming terminals clamp to the grid (Ghostty's encoder: `src/input/mouse_encode.zig:256-266` `posToCell`; only pixel reports go unclamped, `:271-285`).
- ReleaseSafe:     a safety-check panic at the `@intCast` in the event loop (by reading the Zig `@intCast` semantics; the vxfw handler is private, so no probe reached it).
- ReleaseFast:     undefined behaviour.
- reachability:    an SGR cell report with a coordinate of 0 or below reaching a vxfw application with mouse reporting on (Ghostty's `+boo`: `App.run` calls `setMouseMode(.., true)`): from a non-conforming terminal, or bytes injected into the tty.
- port handling:   fixed (R13): `crates/rustty-vaxis/src/vxfw/app.rs` `mouse_point` returns `None` for a negative position and the handlers hit-test nothing, which is what the reference's own `containsPoint` guard decides for any other point outside the frame (the widgets last under the mouse get their `mouse_leave`). PORT NOTE: reference defect (R13). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   NEW-FINDINGS N240.
- test:            `vxfw::app::tests::a_negative_mouse_position_hits_nothing` (column or row -1, and `i16::MIN`, give no point; an on-screen position and column 32767 convert). Negative control (macOS 2026-09-23): the reference's cast restored (`try_from(..).unwrap()`) panics at the conversion.
- proposed fix:    `const mouse_point: vxfw.Point = .{ .row = std.math.cast(u16, mouse.row) orelse return, .col = ... };` in both handlers (for `handleMouse`, after recording `self.mouse` and sending the `mouse_leave` events for an empty hit list).
- upstream status: not reported

### UD264 — libvaxis TextInput's word motions reach `unreachable` in `std.unicode.utf8Decode` on invalid UTF-8
- kind:            crash (safety-checked trap) / undefined behaviour
- upstream project: libvaxis
- severity:        low (typed or pasted bytes in a TUI search box)
- reference:       libvaxis 0.6.0 `src/widgets/TextInput.zig:275-288` (`decodeCodepointAt`: the slice is capped at the end of the buffer) and `:290-299` (`decodeCodepointBefore`: the slice runs back to the first non-continuation byte), both `std.unicode.utf8Decode(slice) catch {...}`; Zig 0.16.0 `std/unicode.zig:101-109` (`utf8Decode`: `switch (bytes.len) { 1 => .., 2 => utf8Decode2(..), 3 => .., 4 => .., else => unreachable }`) and `:116`, `:144`, `:166` (`utf8Decode2/3/4` begin with `assert(` the first byte has that length's lead prefix `)`).
- defect:          `utf8Decode` is only defined for a slice that is one encoded code point's length. The new word-motion helpers pass it a slice of five or more bytes (a run of continuation bytes), a slice whose first byte is not a lead byte of that length (`a` followed by `0x80`; a space followed by continuation bytes), and a lead byte cut short by the end of the buffer (`E2 82`). Its error return, which the helpers `catch`, does not cover these: they hit `unreachable` / `assert`.
- ReleaseSafe:     `panic: reached unreachable code`. MEASURED (Zig 0.16.0 Debug, libvaxis 0.6.0 via a probe): Ctrl+W's `deleteWordBeforeWhitespace` on `x SP 80 80 80` (the `assert` in `utf8Decode4`), `moveBackwardWordwise` on `a b SP 80 80 80 80 80` (`else => unreachable`, `unicode.zig:107`) and on `- SP a 80` (`utf8Decode2`'s assert, `:116`), `moveForwardWordwise` on `E2 82` (`:116`); a valid `hello-world` control moves normally.
- ReleaseFast:     undefined behaviour (the assert is not checked; `unreachable` is undefined).
- reachability:    the TextInput buffer holds key text from the tty: the parser turns each stray byte such as `0x80` into a key whose text is that byte (MEASURED), and TextInput inserts it. In Ghostty, `+list-themes`'s search box (Alt+B, Alt+F, Alt+D, Alt+Backspace, Ctrl+W).
- port handling:   fixed (R13): `crates/rustty-vaxis/src/widgets/text_input.rs` `utf8_decode` returns `None` for the inputs the reference traps on, as for its error returns, and both helpers take their existing decode-failure path (one byte is the code point). PORT NOTE: reference defect (R13). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   NEW-FINDINGS N240.
- test:            `widgets::text_input::tests::word_motion_over_invalid_utf8_does_not_crash` (the four measured inputs, each with the resulting buffer). Negative controls (macOS 2026-09-23, one at a time): the lead-byte check made `unreachable!()` panics at it; the `len > 4` arm made `unreachable!()` panics at it.
- proposed fix:    validate before decoding: in `decodeCodepointAt`, fall back when `capped_len != len`; in `decodeCodepointBefore`, fall back when `end - start > 4` or `utf8ByteSequenceLength(bytes[start]) != end - start` (or decode with `std.unicode.Utf8View`).
- upstream status: not reported

### UD265 — vxfw Text's soft wrap splits a word that fits a line by itself (`trimEnd` where `trimLeft` was meant)
- kind:            non-crash (evident intent broken)
- upstream project: libvaxis
- severity:        low (layout)
- reference:       libvaxis 0.6.0 `src/vxfw/Text.zig:240` (`const trimmed = std.mem.trimEnd(u8, word, " \t");` in `SoftwrapIterator.next`); 0.5.1 had `std.mem.trimLeft` there (Zig 0.15 renamed `trimLeft`/`trimRight` to `trimStart`/`trimEnd`; the line above, `:226`, correctly became `trimEnd` from `trimRight`).
- defect:          `word` is the separators before the next word plus the word (`nextWrap` ends it at the first separator after the word), so it never ends in whitespace. `trimEnd` therefore removes nothing, `trimmed_bytes` is always 0, and a word that would fit a line by itself once its leading space is dropped is judged too long and split mid-word. The comment ("Trim the word to see if it can fit on a line by itself") and the byte arithmetic after it describe removing the leading separators.
- ReleaseSafe:     MEASURED (Zig 0.16.0, a test in a copy of libvaxis 0.6.0): `"ab hello"` at width 5 wraps as `"ab he"` / `"llo"` instead of `"ab"` / `"hello"`.
- ReleaseFast:     the same.
- reachability:    any vxfw `Text` with soft wrap on (the default), narrower than its text; in Ghostty, `+boo`'s "Screen must be at least 100w x 41h" message (`src/cli/boo.zig:76`) in a narrow window.
- port handling:   fixed (R13): `crates/rustty-vaxis/src/vxfw/text.rs` keeps trimming the start (`trim_left`). PORT NOTE: reference defect (R13). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   NEW-FINDINGS N240.
- test:            `vxfw::text::tests::softwrap_word_that_fits_alone_after_a_space_wraps_whole`. Negative control (macOS 2026-09-23): `trim_right` (the reference's `trimEnd`) gives `"ab he"`.
- proposed fix:    `std.mem.trimStart(u8, word, " \t")`.
- upstream status: not reported

### UD266 — `+list-themes` no longer installs the resize handler, so a window resize is ignored on terminals without in-band resize
- kind:            non-crash (evident intent broken)
- upstream project: ghostty
- severity:        low (the picker keeps its old size until restarted)
- reference:       `src/cli/list_themes.zig:306` at 20abdb50a (`var loop: vaxis.Loop(Event) = .init(global.io(), &self.tty, &self.vx);`, then `start`, and nothing else); the handler of the resulting events is `:538` (`.winsize => |ws| try self.vx.resize(..)`). Before `e8525c0fd` ("Update to Zig 0.16.0") the code was `var loop: vaxis.Loop(Event) = .{ .tty = &self.tty, .vaxis = &self.vx }; try loop.init(); try loop.start();`, and libvaxis 0.5.1's `Loop.init` installed the SIGWINCH handler. libvaxis 0.6.0 moved that to `installResizeHandler` and made `init` a constructor; its own `vxfw.App.run` calls `installResizeHandler` when the terminal has no in-band resize.
- defect:          the migration replaced the two lines by the constructor alone, so the SIGWINCH callback that turns a resize into a `.winsize` event is never registered. The picker still handles `.winsize` (and the read thread posts the initial size), but on a terminal without in-band resize reports (mode 2048) nothing reports later sizes.
- ReleaseSafe:     after a resize the picker keeps drawing at the size it started with (by reading; the rustty test below measures the port's side: without the handler no event arrives).
- ReleaseFast:     the same.
- reachability:    running `ghostty +list-themes` in a terminal without in-band resize (for example macOS Terminal) and resizing its window. In Ghostty itself (which supports mode 2048) the in-band reports still arrive.
- port handling:   fixed (R13): `crates/rustty-cli/src/list_themes.rs` `with_preview_loop` installs the resize handler right after constructing the Loop, where the pre-migration code did. PORT NOTE: reference defect (R13). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   NEW-FINDINGS N240.
- test:            `list_themes::picker_tests::the_preview_loop_delivers_a_resize`: a re-executed child whose controlling terminal is a fresh pty builds the picker's Loop with `with_preview_loop`, sets the pty to 33x77, raises SIGWINCH and receives `Winsize { rows: 33, cols: 77 }`. Negative control (macOS 2026-09-23): without the `install_resize_handler` call no event arrives in 10 s ("no winsize event reached the Loop").
- proposed fix:    `try loop.installResizeHandler();` after constructing the loop (or, as vxfw's `App.run` does, after `queryTerminal` when `!self.vx.state.in_band_resize`).
- upstream status: not reported

### UD267 — libvaxis's queue signals `not_full` / `not_empty` only on the full / empty edge, so a second blocked producer (or consumer) can miss its wakeup
- kind:            latent
- upstream project: libvaxis
- severity:        low (needs two threads blocked on the same full, or empty, queue at once)
- reference:       libvaxis 0.6.0 `src/queue.zig:145-162` (`pushAndSignalLH` signals `not_empty` only when the queue was empty; `popAndSignalLH` signals `not_full` only when it was full), used by `pop` / `push` / `tryPush` / `tryPop` (`:30-88`) and `drain` (`:91-102`); 0.5.1 had the same conditions inline.
- defect:          each wakeup is a single `signal` sent only on the edge. With two producers waiting on a full queue, a consumer that pops twice before the first woken producer runs sends one `signal` (on the pop from full) and none on the second pop (the queue is no longer full): the second producer keeps waiting although there is room, until the queue fills up again and a later pop from full signals. The symmetric case holds for two consumers waiting on an empty queue. Reasoned from the code (S-10b review observation); not run.
- ReleaseSafe:     no crash; a producer (or consumer) stays blocked while the queue has room (or items).
- ReleaseFast:     the same.
- reachability:    not reached by Ghostty's CLI in practice. Each `Loop` queue has one consumer (the TUI's main thread); its producers are the read thread, the SIGWINCH callback and the main thread's own `postEvent`s. The loss needs two of them blocked on a full 512-slot queue at the same time (the main thread stalled with 512 events pending while both input and a resize arrive), so this is recorded, not fixed.
- port handling:   faithful (R1): `crates/rustty-vaxis/src/queue.rs` `push_and_signal_lh` / `pop_and_signal_lh` / `drain` keep the reference's edge-triggered `notify_one`. No docs row: behaviour is unchanged.
- parity row(s):   NEW-FINDINGS N240 (found by the S-10b review).
- test:            none beyond the ported queue tests (`queue.rs`, 7 reference tests + `drain_wakes_a_blocked_producer`), which pin the faithful signalling; no test reproduces the two-waiter case.
- proposed fix:    signal on every push and pop (`not_empty.signal()` after each push, `not_full.signal()` after each pop), or `broadcast` on the edge.
- upstream status: not reported

### UD270 — tmux `%output` data is written into the pane still octal-escaped
- kind:            latent (non-crash, evident intent broken, not visible at the pin)
- upstream project: ghostty
- severity:        low
- reference:       src/terminal/tmux/control.zig:213-243 (`%output`: the regex capture is returned as `data` unchanged), :522 (`data: []const u8, // unescaped`), src/terminal/tmux/viewer.zig:1105-1120 (`receivedOutput`: `stream.nextSlice(data)`), at 20abdb50a
- defect:          tmux writes a pane's output in `%output` with every byte below `' '` and every `\` as octal `\ooo` (tmux(1): "value escapes non-printable characters and backslash as octal \\xxx"; `control.c:830-831` at master `72e83b00`). The notification's own field comment says the data is unescaped, but nothing decodes it, so the pane's terminal prints the escapes as text. MEASURED on the port (faithful, task S-12 probe): `%output %7 x\033[1my\015\012z` leaves the active area reading `x\033[1my\015\012z` literally; no line break, no style.
- ReleaseSafe:     no crash; every pane's live output shows `\015\012`, `\033[…` and `\134` as text instead of line breaks, colours and backslashes.
- ReleaseFast:     same.
- reachability:    every `tmux -CC` session whose pane writes a control character (any newline). At the pin the tmux panes are not displayed (termio's `.windows` action is `// TODO`, `stream_handler.zig:468-470`), so the damage is to the viewer's pane terminals, not yet to anything on screen.
- port handling:   faithful, not changed (found by task S-12 while fixing UD12; not in that task's assignment). Ruled (controller, review of task S-12, 2026-09-24): faithful and latent. Not visible at the pin: termio's `.windows` action is `// TODO` in both trees (Ghostty `stream_handler.zig:468-470`, port `crates/rustty-termio/src/stream_handler.rs:647-649`), and nothing a user sees reads the pane terminals. Reopen trigger: becomes visible, and must be fixed, when `.windows` is implemented (upstream or port); UD270, UD271 and UD272 are fixed together then. The decoder is the `\ooo` half of `unescape_capture` (UD12). The fix must also extend UD9's guard: once `%output` is decoded, a pane's own output can set its scroll region before its captures arrive, which UD9's fix (`received_pane_state` waiting) does not cover.
- parity row(s):   none.
- test:            none yet (recorded only; the probe was not committed).
- proposed fix:    decode `\ooo` in the `%output` data (in `parseNotification`, or in `receivedOutput` before `nextSlice`).
- upstream status: not reported

### UD271 — after attaching, every tmux pane is left on its alternate screen
- kind:            latent (non-crash, evident intent broken, not visible at the pin)
- upstream project: ghostty
- severity:        low
- reference:       src/terminal/tmux/viewer.zig:680-685 (a new pane's captures: primary history and visible, then alternate history and visible), :1053 and :1093 (`receivedPaneHistory`/`receivedPaneVisible`: `_ = try t.switchScreen(screen_key)`), :935-951 (`receivedPaneState`: `alternate_on` picks the screen whose cursor is set, and nothing else), at 20abdb50a
- defect:          Each capture switches the pane's terminal to the screen it captures, the alternate captures run last, and the `list-panes` reply that follows reads `alternate_on` but never switches back. So a pane whose program is on the primary screen ends up showing its (usually empty) alternate screen, and its live output (UD270 aside) is written there. MEASURED on the port (faithful, task S-12): after the four captures and a `list-panes` with `alternate_on` 0, `screens.active` is the alternate screen, and a later `%output` lands there while the captured history and visible contents sit on the primary screen.
- ReleaseSafe:     no crash; the pane shows the wrong screen.
- ReleaseFast:     same.
- reachability:    every attach. Not yet on screen at the pin (the `.windows` action is `// TODO`).
- port handling:   faithful, not changed (found by task S-12; not in its assignment). Ruled (controller, review of task S-12, 2026-09-24): faithful and latent. Not visible at the pin: termio's `.windows` action is `// TODO` in both trees (Ghostty `stream_handler.zig:468-470`, port `crates/rustty-termio/src/stream_handler.rs:647-649`), and nothing a user sees reads the pane terminals. Reopen trigger: becomes visible, and must be fixed, when `.windows` is implemented (upstream or port); UD270, UD271 and UD272 are fixed together then. The likely fix is `switchScreen` to `alternate_on`'s screen in `receivedPaneState`.
- parity row(s):   none.
- test:            none of its own (recorded only). The S-12 UD12 test `coloured_pane_captures_keep_the_control_session_and_reach_the_pane` asserts the current placement (captures on the primary screen, the later `%output` on the active, alternate, screen) and names this entry; a fix updates those two assertions.
- proposed fix:    in `receivedPaneState`, switch the pane's terminal to the screen `alternate_on` names.
- upstream status: not reported

### UD272 — a multi-line tmux capture is written as a staircase: its bare `\n` does not return the carriage
- kind:            latent (non-crash, evident intent broken, not visible at the pin)
- upstream project: ghostty
- severity:        low
- reference:       src/terminal/tmux/viewer.zig:1061 and :1102 (`stream.nextSlice(content)` of the capture reply), src/terminal/Terminal.zig:1836-1838 (`linefeed` returns the carriage only in linefeed mode, off by default); the reference test's own hedge at viewer.zig:2183 ("History has 2 lines with "prompt %" (padded to screen width)", asserting only that the text occurs twice), at 20abdb50a
- defect:          tmux joins a capture's lines with a bare `\n` (`cmd-capture-pane.c:404`), which is how text is written to a file, not to a terminal. Written into the pane's VT stream as-is, each line starts in the column where the previous one ended, and a line that no longer fits wraps. MEASURED on the port (faithful, task S-12): a history `a\nb` and visible `c\nd` leave the primary screen reading `a` / ` b` / `c` / ` d`; `h1\nh2\nh3` gives `h1` / `  h2` / `    h3`.
- ReleaseSafe:     no crash; pane history and visible contents are skewed and rewrapped.
- ReleaseFast:     same.
- reachability:    every capture of more than one line. Not yet on screen at the pin (the `.windows` action is `// TODO`).
- port handling:   faithful, not changed (found by task S-12; not in its assignment; the S-12 tests compare lines with leading blanks removed so that they neither depend on nor pin this). Ruled (controller, review of task S-12, 2026-09-24): faithful and latent. Not visible at the pin: termio's `.windows` action is `// TODO` in both trees (Ghostty `stream_handler.zig:468-470`, port `crates/rustty-termio/src/stream_handler.rs:647-649`), and nothing a user sees reads the pane terminals. Reopen trigger: becomes visible, and must be fixed, when `.windows` is implemented (upstream or port); UD270, UD271 and UD272 are fixed together then. The likely fix is writing `\r\n` for each `\n` of a capture (or setting linefeed mode around the write).
- parity row(s):   none.
- test:            none yet (recorded only).
- proposed fix:    in `receivedPaneHistory` and `receivedPaneVisible`, feed a carriage return before each `\n` of the capture.
- upstream status: not reported

### UD273 — a tmux capture's last SGR state leaks into the pane's next capture and later output
- kind:            latent (non-crash, not visible at the pin)
- upstream project: ghostty
- severity:        low
- reference:       src/terminal/tmux/viewer.zig:1040-1077 (`receivedPaneHistory`: `stream.nextSlice(content)`, then `index()` × rows) and :1080-1103 (`receivedPaneVisible`: `eraseDisplay(.complete, false)`, then `stream.nextSlice(content)`); src/terminal/Screen.zig:1995-1998 (`blankCell` takes the pen's background), at 20abdb50a. tmux side: `cmd-capture-pane.c` passes one `gc` pointer to every `grid_string_cells` call of a capture and writes no closing reset, and each capture command starts again from the default cell (tmux master `72e83b00`).
- defect:          A capture ends with whatever SGR its last styled cell set, and the next capture's leading default-styled cells get no SGR, because tmux starts each capture from the default cell. The pane's terminal keeps the pen the history left behind: `index()` fills the new rows with its background, `eraseDisplay(.complete)` does the same, and the visible capture's unstyled text is drawn in the history's colours. MEASURED on the port (faithful, task S-12b probe, not committed; first found by the S-12 reviewer): raw history `ESC[41m bg` and visible `plain`, through `test_host::capture_reply` with `-C`, give the pane `…ESC[48;5;1mplain<padded to 80 columns>`, where a fresh terminal given `plain` has no style.
- ReleaseSafe:     no crash; pane contents after a styled capture carry the wrong colours.
- ReleaseFast:     same.
- reachability:    any pane whose history (or visible capture) ends in a styled cell. In the reference the session already ends at the first coloured capture (UD12), so this became reachable in state only with the port's UD12 fix; it is not visible at the pin in either tree (termio's `.windows` action is `// TODO`).
- port handling:   faithful, not changed. Ruled (controller, review of task S-12, 2026-09-24): latent. Reopen trigger: becomes visible, and must be fixed, when termio's `.windows` is implemented (upstream or port), together with UD270-UD272.
- parity row(s):   none.
- test:            none yet (recorded only).
- proposed fix:    write `ESC[0m` into the pane's stream before and after each capture reply (or reset the pen in `receivedPaneHistory`/`receivedPaneVisible`).
- upstream status: not reported

### UD275 — libxev's kqueue backend has no loop wakeup on FreeBSD: `Wakeup.wakeup` is a `@panic` stub
- kind:            latent
- upstream project: libxev
- severity:        low (unreachable today; a crash on FreeBSD the moment anything calls it)
- reference:       libxev `9ce8e8e6` (pinned by ghostty@20abdb50a `build.zig.zon:19-22`) `src/backend/kqueue.zig:1488-1510` (the non-Darwin `Wakeup`: `init`/`deinit`/`setup` do nothing, `wakeup` is `@panic("wakeup not implemented on this platform")`, under `// TODO: We should use eventfd for FreeBSD. Until this is implemented, loop wakeup will crash on BSD.`); its only caller `Loop.wakeup`, `:973-975`, which is not `pub`; that function's only call site, `thread_perform`, `:965-968`, inside `if (comptime builtin.target.os.tag.isDarwin())`.
- defect:          on FreeBSD the kqueue loop has no way to be woken from another thread. libxev keeps the stub from firing by not calling it: thread-pool completions are pushed to `thread_pool_completions` without a wakeup, so a loop blocked in `kevent` collects them only when some other event arrives. Any future caller of `Loop.wakeup` on FreeBSD (or a `thread_perform` that dropped the Darwin gate) would abort the process.
- ReleaseSafe:     no effect today (no caller reaches it); if reached, the panic aborts the process.
- ReleaseFast:     the same (`@panic` is not optimized out).
- reachability:    none, verified at the pin: `Loop.wakeup` is private to `kqueue.zig`; its one call site is Darwin-only; `xev.Async` on FreeBSD is `AsyncEventFd` (`watcher/async.zig:22-25`), its own eventfd, and never calls it; neither the static kqueue `Api()` nor `dynamic.zig` exposes it; and Ghostty neither calls a loop wakeup nor creates an `xev.ThreadPool` (a grep of ghostty@20abdb50a `src/` for `ThreadPool` and `thread_pool` finds nothing). The latency cost of the pushed-only pool completions is likewise unreached, since Ghostty has no pool.
- port handling:   faithful (R1): `crates/rustty-xev/src/kqueue.rs` keeps the FreeBSD `Wakeup` as the same panic stub and `thread_perform`'s wakeup Darwin-gated; its PORT NOTE records the reachability argument. `Loop::wakeup` is private in the port too and has no caller on any target. No docs row: behaviour is unchanged.
- parity row(s):   NEW-FINDINGS N255; `.port/phase-2/DEFERRALS.md` "FreeBSD — the platform's explicit ledger row", item 3.
- test:            `kqueue::tests::freebsd_wakeup_stub_is_the_reference_panic` (`cfg(target_os = "freebsd")`, `should_panic`): `init` and `setup` succeed and `wakeup` panics with the reference's message. Type-checked from macOS (`cargo check -p rustty-xev --tests --target x86_64-unknown-freebsd`; a planted type error in its body fails that check and not the macOS one, so it is compiled for FreeBSD); not run (no FreeBSD host, `VERIFICATION_DEBT.md` V23).
- proposed fix:    what the TODO names: on FreeBSD (13+ has `eventfd(2)`), give `Wakeup` an eventfd created with `EFD_CLOEXEC | EFD_NONBLOCK`, register it in `setup` as `EVFILT_READ` with `udata = 0` (the tick loop already skips `udata == 0` events, as it does for the Darwin mach port), write `1` to it in `wakeup`, and drain it on each tick; then drop the `isDarwin()` gate on `thread_perform`'s wakeup. (`EVFILT_USER` with `NOTE_TRIGGER` is the eventfd-free alternative on every BSD.)
- upstream status: not reported

### UD276 — `test "face metrics"` in font/Collection.zig never checks anything: its backend guard is inverted
- kind:            test-weakness
- upstream project: ghostty
- severity:        low
- reference:       src/font/Collection.zig:1396-1523 at 20abdb50a (comment and guard at :1397-1398, expected-value switches at :1438-1502)
- defect:          The comment says "The web canvas backend doesn't calculate face metrics, only cell metrics", but the guard is `if (options.backend != .web_canvas) return error.SkipZigTest;`, so the test is skipped on every backend except the one the comment excludes. On `web_canvas` each expected-value switch takes its `.web_canvas => unreachable` prong. The switches also have no `freetype_windows` prong, so they would not compile if analyzed on that backend. The face-metrics expectations therefore never run in upstream CI.
- ReleaseSafe:     no product effect; the test is inert.
- ReleaseFast:     same.
- reachability:    none (test code).
- port handling:   faithful skip (R13 keeps test weaknesses faithful; R14 form 1: `cfg_attr(not(wasm32), ignore)` in `crates/rustty-font-shaper/src/collection.rs` `face_metrics`), with the body ported live. MEASURED (task tail-s8c): run with `--ignored` on macOS (coretext) the body passes against the reference's own coretext expected values for CozetteVector and Geist Mono; a perturbed `cell_width` (7.3828125 → 7.5) fails it. On Linux (`fontconfig_freetype`, task L-1, Linux VM rvm (Fedora 44 aarch64), 2026-09-26) the body run with `--ignored` fails at the narrow font's first metric, `expected approximately 8, found 7`: the stale FreeType expectation of UD281, as measured under `coretext_freetype`.
- parity row(s):   NEW-FINDINGS N265.
- test:            `face_metrics` (`crates/rustty-font-shaper/src/collection.rs`), ignored by default as upstream.
- proposed fix:    `if (options.backend == .web_canvas) return error.SkipZigTest;`, plus a `freetype_windows` prong (or an `else`) in the four switches.
- upstream status: not reported

### UD280 — with `-Dfont-backend=coretext_freetype`, a font shipped as a collection (`.ttc`) loads the collection's first face
- kind:            non-crash (evident intent broken)
- upstream project: ghostty
- severity:        low (a non-default, self-compiled backend; the reference's own comment admits it)
- reference:       `src/font/DeferredFace.zig:301-304` at 20abdb50a (`loadCoreTextFreetype`: "Face index 0 is not always correct. We don't ship this configuration in a release build." then `Face.initFile(lib, path, 0, opts)`)
- defect:          CoreText discovery yields the right face (for example `Menlo-Bold`), but the FreeType load opens face index 0 of its font FILE. For a collection file that is another face: `Menlo-Bold` is in `/System/Library/Fonts/Menlo.ttc`, whose face 0 is `Menlo-Regular`, so bold text (measured; italic takes the same path) of a collection-shipped family renders with the regular face.
- ReleaseSafe:     wrong face (no crash).
- ReleaseFast:     the same.
- reachability:    a `coretext_freetype` build (the reference's `build-macos-freetype` CI job; "people who want to self-compile Ghostty and prefer the freetype aesthetic") with any `font-family` whose styles ship in one `.ttc` (Menlo, and many system families). MEASURED on macOS 26 by the rustty test below before the fix.
- port handling:   fixed: `crates/rustty-font-shaper/src/deferred_face.rs` `load_coretext_freetype` opens the face whose PostScript name equals the CoreText font's (`collection_face_index`), keeping index 0 when the file holds one face or none matches. PORT NOTE: reference defect (R13). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   NEW-FINDINGS N288 (and N266, which made the arm live).
- test:            `deferred_face::tests::load_coretext_freetype_opens_the_discovered_face_of_a_collection` (`-Dfont-backend=coretext_freetype`): CoreText `Menlo-Bold` loads a FreeType face whose PostScript name is `Menlo-Bold`, with preconditions that the file has more than one face and the chosen index is not 0. Negative control (the reference's index 0, run before the fix, 2026-09-24): "left: Menlo-Bold, right: Menlo-Regular".
- proposed fix:    pick the face index by matching `FT_Get_Postscript_Name` of each face in the file against `CTFontCopyPostScriptName` (or use `kCTFontURLAttribute` together with the descriptor's collection index where CoreText exposes it).
- upstream status: not reported

### UD281 — `test "face metrics"`'s FreeType expectations predate the light-hinting default, so they would fail if the test ran
- kind:            test-weakness
- upstream project: ghostty
- severity:        low
- reference:       `src/font/Collection.zig:1439-1502` at 20abdb50a (the `.freetype`/`.fontconfig_freetype`/`.coretext_freetype` prongs: narrow `cell_width` 8.0, `ascii_height` 18.0625; wide `cell_width` 10.0, `ascii_height` 16.0)
- defect:          The values were written by `03a707b2c` while the test ran; `4af4e1872` (the next day) inverted the test's guard (UD276), and `ad9f9dc11` later made `light = true` the default `FreetypeLoadFlags` (`FT_LOAD_TARGET_LIGHT`), updating a running FreeType test but not this skipped one. Under today's default the FreeType face measures narrow `cell_width` 7.0 and wide `ascii_height` 15.5; with `light = false` it reproduces all four expectations. The inverted guard (UD276) hides the drift.
- ReleaseSafe:     no product effect; the test is inert.
- ReleaseFast:     same.
- reachability:    none (test code); it would surface as a failure the day UD276's guard is fixed.
- port handling:   faithful (R13 keeps test weaknesses faithful): `crates/rustty-font-shaper/src/collection.rs` `face_metrics` keeps the reference's values and its skip. MEASURED (S-14 review, `-Dfont-backend=coretext_freetype`, reverted probe): default flags 7.0 / 18.0625 / 10.0 / 15.5; `light = false` 8.0 / 18.0625 / 10.0 / 16.0. The port's measuring code is line-for-line the reference's over the same FreeType 2.13.2, so Ghostty would measure the same (inferred, not run in Zig).
- parity row(s):   NEW-FINDINGS N285 (and N265 / UD276).
- test:            `face_metrics` run with `--ignored` under a FreeType backend fails on exactly those two values.
- proposed fix:    update the FreeType prongs to 7.0 (narrow `cell_width`) and 15.5 (wide `ascii_height`) together with fixing UD276's guard, or pin `.freetype_load_flags = .{ .light = false }` in the test.
- upstream status: not reported

### UD290 — an `input` that fails after the shell started leaves the IO loop holding completions in a dead stack frame
- kind:            crash (use-after-free) and leak
- upstream project: ghostty
- severity:        medium
- reference:       src/termio/Termio.zig:343-400 (`threadEnter`: `try self.backend.threadEnter(...)`, `errdefer self.backend.threadExit(data)`, then the `input` loop returning `error.InputFailed` at :382, :391 and :397); src/termio/Exec.zig:159-190 (`threadEnter` registers `process_wait_c`, `flatpak_wait_c` and `termios_timer_c`, all fields of `td`, on `td.loop`); src/termio/Thread.zig:262-270 (`var cb: CallbackData` in `threadMain_`'s frame; `try io.threadEnter(self, &cb.data)` precedes `defer cb.data.deinit()`) and :224-234 (`threadMain` runs `self.loop.run(.until_done)` to drain after `threadMain_` returned), at 20abdb50a
- defect:          When an `input` file cannot be read, is larger than 10 MiB, or cannot be queued, `Termio.threadEnter` fails after `Exec.threadEnter` has started the shell and registered its completions on the thread's loop. The `errdefer` stops the read thread only. The completions live in `cb.data`, a local of `threadMain_`, which returns the error; `threadMain` then paints the `input` failure screen and runs the loop "to drain", and the loop's first tick starts the queued completions through pointers into the returned frame. `cb.data.deinit()` is registered only after `threadEnter` succeeds, so the read-thread pipe, the process watcher, the write stream and the termios timer are also never released. By reading at the pin; the port, which had the same shape, was MEASURED.
- ReleaseSafe:     undefined behaviour (a use-after-free of stack memory in the event loop). Not measured in Zig. MEASURED on the port (Linux aarch64, epoll backend, task L-1): SIGSEGV in the loop's submission-queue pop (`rustty_xev::queue::Intrusive::pop`, reached from `Thread::threadMain`'s drain run). On macOS (kqueue) the same port test passed: the freed frame had not yet been overwritten.
- ReleaseFast:     same.
- reachability:    config: an `input` entry naming a file over 10 MiB or one that cannot be read, on any surface start.
- port handling:   fixed (R13): `crates/rustty-termio/src/termio_impl.rs` `Termio::threadEnter` (PORT NOTE: reference defect (R13) at the input-failure branch), after the faithful `backend.threadExit(data)`, stops the thread's loop, which has not run yet, so none of the queued completions was started and a stopped loop never ticks again (`threadMain` then skips its drain, which had nothing to drain: the mailbox and stop waits are registered only after `threadEnter` succeeds), and releases `data` as the normal exit does (`deinit` + drop). The only visible difference from the reference is that the "abrupt io thread exit detected" warning is not logged on this path. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   none; found by task L-1 (the Linux lane) when `io_thread::oversized_input_file_paints_the_input_failure_screen` (LEDGER S212-F1's test) crashed the `io_thread` test binary on Linux. NEW-FINDINGS N297.
- test:            `io_thread::oversized_input_file_paints_the_input_failure_screen` (rustty-termio): an `input` file of 10 MiB + 1 byte, the real `Thread::threadMain` on an io thread, the failure screen asserted, then stop and join. With the fix it passes on Linux and macOS (`io_thread`: 3 passed on each). Negative control (Linux, task L-1, run twice): the three statements removed, the binary dies with SIGSEGV (signal 11) in that test both times.
- proposed fix:    In `Termio.threadEnter`, add `errdefer data.deinit()` after the backend starts, and make sure nothing is left queued on the loop: for example `errdefer thread.loop.stop()`, or allocate `CallbackData` in `threadMain` so it outlives the drain.
- upstream status: not reported

### UD291 — Highway's Linux CPU detection never sees a feature bit: `std.os.linux.getauxval` returns 0 in a libc-linked program
- kind:            functional (performance: SIMD dispatch stuck at the baseline target)
- upstream project: ghostty
- severity:        low
- reference:       pkg/highway/src/detect/linux.zig:1-5 (`getauxval` = `std.os.linux.getauxval`, "Does not call into libc"), used by detect/aarch64_linux.zig:9-10, ppc.zig, s390x.zig, riscv.zig and loongarch.zig; consumed by pkg/highway/src/cpp/targets.cpp:28-38 (`DetectTargets`, runtime dispatch); zig 0.16.0 std/os/linux.zig:517-545 (`getauxvalImpl` reads `elf_aux_maybe`, which only std/start.zig:550, Zig's own `_start`, sets; with `link_libc` that implementation is the one used). At 20abdb50a.
- defect:          In a program that links libc, the C runtime's `_start` runs instead of Zig's, `elf_aux_maybe` stays null, and `getauxval` returns 0 for every key. Ghostty on Linux always links libc, and libghostty-vt is a library (no Zig `_start` at all), so on every non-x86 Linux target the HWCAP tests all fail and Highway only ever dispatches to the baseline (`neon_without_aes` on aarch64). The code and its doc comment ("lightweight syscalls (... getauxval on Linux)", detect.zig:18-21) clearly mean to read the kernel's values. MEASURED (task L-1, Linux aarch64 VM, HWCAP 0xefb3ffff, HWCAP2 0x126181, no SVE): the reference's `ghostty_hwy_detect_targets` built with zig 0.16.0 for aarch64-linux-gnu with `-lc` returns 0x20000000 (NEON_WITHOUT_AES), and for aarch64-linux-musl with `-lc` also 0x20000000; the same code built without libc returns 0x34000000 (NEON_WITHOUT_AES, NEON, NEON_BF16).
- ReleaseSafe:     no crash; the SIMD kernels run their baseline variant on ARM, POWER, s390x, RISC-V and LoongArch Linux.
- ReleaseFast:     same.
- reachability:    every Ghostty run on non-x86 Linux (x86 uses CPUID and is unaffected).
- port handling:   fixed (R13, evident intent): `crates-sys/highway-sys/cpp/detect.c` reads `getauxval` from libc (PORT NOTE: reference defect (R13) at the aarch64 Linux detector). The port always did; the difference was found only now, by task L-1's comparison against the reference. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   LEDGER S34-F3 (found while verifying it). NEW-FINDINGS N299.
- test:            measured by probes, not a unit test: the port's `rustty_hwy_detect_targets` (the committed `detect.c` compiled on the VM) returns 0x34000000 on the host above, equal to the reference's own code when its `getauxval` works (the no-libc build). Negative control: the reference's behaviour (libc-linked Zig build) returns 0x20000000 on the same host.
- proposed fix:    call libc's `getauxval` when `builtin.link_libc` (for example `std.c.getauxval`), keeping the Zig implementation for libc-free builds.
- upstream status: not reported

### UD295 — the surface drag-and-drop `drop` handler is a `void` function on a `gboolean` signal
- kind:            ub (a C function-type mismatch: GTK calls the handler as returning `gboolean` and reads an undefined return value)
- upstream project: ghostty
- severity:        low (the drop is accepted or rejected at random; the split move itself runs)
- reference:       `src/apprt/gtk/class/surface.zig:3759-3786` (`surfaceDrop`, `callconv(.c) void`), connected in `src/apprt/gtk/ui/1.2/surface.blp` (`surface_drop_target`: `drop => $surface_drop()`), at 20abdb50a. `GtkDropTarget::drop` is declared `gboolean (*drop) (GtkDropTarget*, const GValue*, double, double, gpointer)`: "Returns: whether the drop was accepted at the given pointer position".
- defect:          the handler returns nothing, so GTK reads whatever the return register holds and passes it to `gdk_drop_finish` as the accepted/rejected answer. The sibling handler `surfaceDropMotion` does return its `gdk.DragAction`.
- ReleaseSafe:     UNVERIFIED (not measured on a Ghostty build): the answer GTK sees is the register left by the last call in `surfaceDrop` (`setDropOverlayDirection`, or `findSurfaceByID`/`moveSplit` on the early returns), so the drag source is told accepted or rejected arbitrarily. No crash is expected in C.
- ReleaseFast:     the same (C calling convention, not a Zig safety check).
- reachability:    every drag of a split onto another surface (the surface drag source), from the user.
- port handling:   `crates/rustty-apprt/src/gtk/class/surface.rs` `surface_drop` returns `bool`: `true` after `move_split`, `false` on each early return, the signal's documented meaning. A `()` handler, the direct translation, is worse in Rust: glib-rs checks the return type and panics inside the C callback, which aborts the process (MEASURED on the VM, task L-2: "Closure returned no value but the caller expected a value of type gboolean", then "panic in a function that cannot unwind"). PORT NOTE: reference defect (R13) at the handler.
- parity row(s):   none (found by task L-2 running the headed KAT `gtk_parity_callbacks`); NEW-FINDINGS N300.
- test:            `surface_drop_answers_the_gboolean_drop_signal` (rustty-apprt, Linux, display): emitting `drop` on `surface_drop_target` with a value that is not a surface id returns `false` and the process survives; the headed KAT `parity_callbacks_do_not_abort` emits `drop` on every drop target. NEGATIVE CONTROL (VM, task L-2): the `()` handler restored aborts the test process.
- proposed fix:    declare `surfaceDrop` as `callconv(.c) c_int` and return `1` after `moveSplit`, `0` on each early return.
- upstream status: not reported

### UD300 — three GTK "surface is not in a window" warnings name `new_tab` for another action
- kind:            latent (copy-paste in a message)
- upstream project: ghostty
- severity:        low (a log line names the wrong action)
- reference:       `src/apprt/gtk/class/application.zig` at 20abdb50a: `gotoTab` :2462, `moveTab` :2603 and `toggleTabOverview` :3276 each log `surface is not in a window, ignoring new_tab`, the text of `newTab`'s own warning (:2683); the sibling `moveTabToNewWindow` (:2626) names its own action.
- defect:          when the surface has no enclosing window, the warning names `new_tab` although the ignored action is `goto_tab`, `move_tab` or `toggle_tab_overview`.
- ReleaseSafe:     misleading diagnostic, no behavioural effect.
- ReleaseFast:     the same.
- reachability:    any of the three actions targeting a surface that is not (or no longer) inside a `Window`.
- port handling:   faithful (R13: message text stays faithful). The port had used the "corrected" names; task S-1c restored the reference text in `crates/rustty-apprt/src/gtk/class/application.rs` `goto_tab`, `move_tab` and `toggle_tab_overview`, each with a comment citing this entry. Not in `docs/ghostty-bug-fixes.md` (no fix in the port).
- parity row(s):   NEW-FINDINGS N310.
- test:            none (GTK widget tree; the text is a string literal, compile-checked on Linux only). UNVERIFIED on Linux.
- proposed fix:    `ignoring goto_tab`, `ignoring move_tab`, `ignoring toggle_tab_overview`.

- upstream status: not reported

### UD301 — `wasm_patch_growable_table`'s usage line names a `wasm_growable_table` binary
- kind:            latent (copy-paste in a message)
- upstream project: ghostty
- severity:        low (a usage line names the wrong program)
- reference:       `src/build/wasm_patch_growable_table.zig:9` (doc) and `:23` `std.log.err("usage: wasm_growable_table <input.wasm> <output.wasm>", .{})` at 20abdb50a; the build step names the executable `wasm_patch_growable_table` (`src/build/GhosttyLibVt.zig:73`).
- defect:          the usage error names a program that does not exist; the binary is `wasm_patch_growable_table`.
- ReleaseSafe:     misleading diagnostic, no behavioural effect.
- ReleaseFast:     the same.
- reachability:    running the build helper with the wrong argument count (only the build graph runs it, with two arguments).
- port handling:   faithful (R13: message text stays faithful): xtask's `report_usage_error` prints the reference's line (task S-1d). Not in `docs/ghostty-bug-fixes.md`.
- parity row(s):   NEW-FINDINGS N313.
- test:            none for the text (xtask's `wasm_patch_rejects_*` tests assert the usage path); the line was measured (`cargo run -p xtask -- wasm-patch-growable-table onlyone`: the line, exit 1).
- proposed fix:    `usage: wasm_patch_growable_table <input.wasm> <output.wasm>` in both places.
- upstream status: not reported

### UD310 — an OSC 10-13 whose seventh color request fails to allocate leaks the list it grew
- kind:            leak
- upstream project: ghostty
- severity:        low (reachable only with a failing caller allocator)
- reference:       src/terminal/osc/parsers/color.zig `parseGetSetDynamicColor` (`var result: List = .{};` with no `errdefer result.deinit(alloc);`, unlike its three siblings `parseGetSetAnsiColor`, `parseResetAnsiColor`, `parseResetDynamicColor`), at 20abdb50a
- defect:          `result` is a `SegmentedList(Request, 2)`: the third request grows the shelf array and shelf 0, the seventh a new shelf array and shelf 1. When that seventh `try result.addOne(alloc)` fails, the error returns without freeing `result`, and the caller's `catch` (color.zig `parse`) substitutes an empty list. Only OSC 10, 11, 12 and 13 reach a seventh request: each value advances with `Dynamic.next()`, which ends at 19, so OSC 14 takes at most six values and OSC 15-19 fewer.
- ReleaseSafe:     the shelf array and shelf 0 leak (the testing allocator reports them); the OSC is otherwise handled as the reference intends (an empty request list).
- ReleaseFast:     the same leak.
- reachability:    libghostty-vt embedders whose `GhosttyAllocator` can fail, through the OSC parser (`ghostty_osc_*`) or the terminal stream, on an OSC 10-13 with seven or more values from the program in the terminal.
- port handling:   fixed (R13): the port's list frees itself when the error drops it; `crates/rustty-vt/src/osc/parsers/color.rs` (PORT NOTE: reference defect (R13), UD310). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   none; found by task S7-7 (LEDGER S174-F3). NEW-FINDINGS N321.
- test:            `osc::s7_7_tests::dynamic_color_list_growth_failure_frees_what_it_grew` (rustty-vt): `FailingAllocator` refusing allocation 2 on `10;?;?;?;?;?;?;?`; allocated == freed and the catch's log ran. Negative control (the list forgotten on the error path): allocated 28 vs freed 0.
- proposed fix:    add `errdefer result.deinit(alloc);` after `var result: List = .{};` in `parseGetSetDynamicColor`.
- upstream status: not reported

### UD311 — `SegmentedList.growCapacity` cleans up a failed multi-shelf growth by freeing an undefined pointer
- kind:            latent
- upstream project: ghostty (vendored Zig 0.15.2 `std.segmented_list`)
- severity:        low (latent)
- reference:       src/datastruct/segmented_list.zig `growCapacity`: `errdefer while (i > old_shelf_count) : (i -= 1) { allocator.free(new_dynamic_segments[i][0..shelfSize(i)]); };`, at 20abdb50a
- defect:          when the allocation of shelf `i` fails, the cleanup frees `new_dynamic_segments[i]`, the slot of the shelf that failed, which was never written, and stops before `old_shelf_count`, so the first new shelf leaks. It runs only when one call adds two or more shelves and a later one fails.
- ReleaseSafe:     `new_dynamic_segments[i]` is `undefined` (0xaa… bytes): an invalid free (the general-purpose allocator panics; a C embedder's allocator does whatever it does with a wild pointer), plus the leak.
- ReleaseFast:     the free of an uninitialized pointer is undefined behaviour, plus the leak.
- reachability:    none today. The callers only `addOne`/`append`, which add at most one shelf per call: src/terminal/osc/parsers/color.zig (`List`) and src/font/Collection.zig:698 (`SegmentedList(EntryOrAlias, 0)`). A future `setCapacity`/`growCapacity` with a large capacity would reach it.
- port handling:   the port frees exactly the shelves the call allocated: `crates/rustty-datastruct/src/segmented_list.rs` `grow_capacity` (PORT NOTE, UD311). No behaviour change on any reachable path, so not listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   none; found by task S7-7 porting the file. NEW-FINDINGS N322.
- test:            `segmented_list::tests::a_refused_shelf_in_a_multi_shelf_growth_frees_the_new_shelves` (rustty-datastruct), exact call log; negative control (cleanup removed): the shelves leak and the log differs.
- proposed fix:    `errdefer while (i > old_shelf_count) { i -= 1; allocator.free(new_dynamic_segments[i][0..shelfSize(i)]); };` (free indices `old_shelf_count..i`).
- upstream status: not reported

### UD312 — `ghostty_osc_command_data` unwraps a NULL command, which `ghostty_osc_end` returns
- kind:            crash
- upstream project: ghostty
- severity:        medium (C API)
- reference:       src/terminal/c/osc.zig `commandDataTyped`: `const command = command_.?;`; `end` (osc.zig `Parser.end`) returns null for `.start`, `.invalid` and every parser that rejects its input; at 20abdb50a
- defect:          `ghostty_osc_command_data` dereferences its command without a NULL check. The header contradicts itself: `ghostty_osc_command_data` documents `command` as "may be NULL", while `ghostty_osc_end` is documented to never return NULL ("Invalid commands will return a command with type GHOSTTY_OSC_COMMAND_INVALID"), yet it does. A caller that trusts the header passes `end`'s result straight to `command_data`.
- ReleaseSafe:     panic (null unwrap).
- ReleaseFast:     a NULL dereference (undefined behaviour).
- reachability:    libghostty-vt embedders using the OSC parser API: `ghostty_osc_end` on a parser fed nothing (or any rejected OSC from the program in the terminal), then `ghostty_osc_command_data(cmd, GHOSTTY_OSC_DATA_CHANGE_WINDOW_TITLE_STR, &out)`.
- port handling:   fixed (R13): a NULL command answers `false` and leaves `out` untouched, as `commandType` answers NULL with `.invalid`; `crates/rustty-vt-c/src/osc.rs` (PORT NOTE: reference defect (R13), UD312). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   none; found by task S7-7's C-API failure sweep. NEW-FINDINGS N323.
- test:            `osc::tests::command_data_on_a_null_command_is_false` (rustty-vt-c); negative control (guard removed): the test process aborts on the NULL dereference.
- proposed fix:    `const command = command_ orelse return false;` in `commandDataTyped`; alternatively make `end` honour its header (return a command of type `.invalid` instead of null).
- upstream status: not reported

### UD313 — a second `ghostty_osc_end` on the same color or kitty-color OSC leaks the first request list
- kind:            leak
- upstream project: ghostty
- severity:        low (C API)
- reference:       src/terminal/osc/parsers/color.zig `parse` and src/terminal/osc/parsers/kitty_color.zig `parse` assign `parser.command = .{ ... }` without freeing the list the previous command holds; `Parser.end` leaves `parser.state` as it was, so a second `end` reparses; only `reset`/`deinit` free the list; at 20abdb50a
- defect:          calling `ghostty_osc_end` twice on the same OSC (the header does not forbid it, and `end` changes no state) builds a second list and drops the first one's allocation: for OSC 4/5/10-19/104/110-119 the shelf array and shelf 0 once there are three or more requests, for OSC 21 the `ArrayList` buffer.
- ReleaseSafe:     the first list leaks.
- ReleaseFast:     the same leak.
- reachability:    libghostty-vt embedders using the OSC parser API (`ghostty_osc_end` twice without `ghostty_osc_reset`). The terminal stream is not affected: it resets the parser on every OSC start.
- port handling:   fixed (R13): the parser's `color_requests`/`kitty_color` storage is replaced with `insert`, which frees the previous list; `crates/rustty-vt/src/osc/parsers/color.rs` and `kitty_color.rs` (PORT NOTE: reference defect (R13), UD313). Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   none; found by the S7-7 review (F4).
- test:            `osc::tests::end_twice_frees_the_first_list` (rustty-vt-c): OSC 4 with three requests and OSC 21, `end` twice over a counting allocator; the first list is freed by the second `end` and `free` returns everything. Negative controls (the previous list forgotten before each `insert`, the reference's behaviour): the test fails for each.
- proposed fix:    in both `parse` functions, free the previous command's list before assigning `parser.command` (for example call the `reset` cleanup switch first), or have `end` move the state to `.invalid` so a second `end` returns null.
- upstream status: not reported

### UD315 — after a render-state update fails on an allocation, the next update leaves the rows it did not rebuild stale or empty
- kind:            functional (evident intent: a successful update must describe the current terminal), and, by reading, UB (UNVERIFIED)
- upstream project: ghostty
- severity:        medium (UB-class by reading, UNVERIFIED; reachable only with a failing caller allocator)
- reference:       src/terminal/render.zig:373-745 (`beginUpdate`: `self.rows`/`cols`/`viewport_pin` are stored at :417-419 before the `row_data.resize` `try` at :469; each page's dirty flag is cleared at :580 and each row's at :630/:638/:646 before `try builder.row(...)`; `t.flags.dirty` and the screen's dirty flags are cleared only at the end, :743-744), :1140-1344 (`RowBuilder.row`: `try cells.resize` at :1181, `try b.pending_styles.append` at :1260/:1275/:1326, `try arena_alloc.dupe` at :1285, `try ensureTotalCapacity` at :1340), and src/terminal/c/render.zig:214-233 (`update`/`begin_update` return `.out_of_memory` and nothing else), at 20abdb50a
- defect:          When an allocation fails part way through `beginUpdate`, the call returns `error.OutOfMemory` after it has already consumed dirty state it did not act on: the new dimensions and viewport pin are stored, the pages and rows it reached have their dirty flags cleared (the failing row included, since its flag is cleared before `builder.row` runs), and rows added by a resize exist with no cells. The next `beginUpdate` compares against those stored values, so unless the terminal changed again it is not a full redraw: it rebuilds only rows whose page or row is still dirty, and leaves the others stale (their previous frame) or empty (0 cells), and it can report `.false` or `.partial` for them. By reading at the pin; the port, which is line-for-line the same, was MEASURED (below). Worse, by reading, UNVERIFIED (review S7-5 F1): when the failure happens INSIDE `RowBuilder.row`, the failing row has already had its raw cells copied (`fastmem.copy`, render.zig:1190-1194) and its arena reset (:1155, `reset(.retain_capacity)` frees every node but the last, ArenaAllocator.zig:153-160), so its flagged cells past the failure point keep grapheme slices into freed or reused arena memory; on a row whose cells were just allocated (first update or a resize) they keep the undefined grapheme slices and `Style` values `MultiArrayList.resize` leaves. The row's dirty flag was consumed, so the successful retry does not rebuild it, and `ghostty_render_state_row_cells_get` reads `cells.styles[x]` (c/render.zig:758) and `cells.graphemes[x]` (:767, :772) for those cells.
- ReleaseSafe:     a caller that retries after `GHOSTTY_OUT_OF_MEMORY` gets `GHOSTTY_SUCCESS` and a render state whose unreached rows are missing or out of date, until they happen to be written again. By reading, UNVERIFIED: reading a flagged cell of a row that failed part way reads an undefined or dangling grapheme slice and `Style`; a possible panic (invalid union tag in the style switch) or garbage graphemes.
- ReleaseFast:     the stale/empty rows as above; by reading, UNVERIFIED: the same reads are undefined behaviour (a wild or dangling read of the grapheme slice).
- reachability:    libghostty-vt embedders whose `GhosttyAllocator` can fail (a limited arena, a memory cap), through `ghostty_render_state_update`/`begin_update`. Ghostty's own renderer uses the process allocator.
- port handling:   fixed (R13, evident intent): `crates/rustty-vt/src/render.rs` `RenderState::begin_update` (PORT NOTE: reference defect (R13, UD315)) records a failed update, and the next one takes the reference's own full-redraw path (`redraw`), which rebuilds every row, re-stores every pin and reports `.full`; the flag clears when that redraw completes. On success nothing changes. The port has no counterpart of the undefined reads: new cell slots are initialized, grapheme spans are read through a checked slice, and the forced redraw rebuilds the row. Listed in `docs/ghostty-bug-fixes.md`.
- parity row(s):   none; found by task S7-5 while making the render state's allocations fallible (LEDGER S175-F1). NEW-FINDINGS N316.
- test:            `render::tests::failed_update_is_followed_by_a_full_rebuild` (rustty-vt): a failing allocator refuses row 1's first cell column; the retried update must rebuild rows 1 and 2 and equal a fresh state. `render_state_allocation_failure_sweep` (rustty-vt) and `render_state_allocation_failure_sweep_through_the_c_api` (rustty-vt-c) fail every allocation of their scenarios in turn and compare each retried update with a fresh state. Negative control (the flag's check removed, the reference's behaviour): the first test fails with `row 1 is rebuilt` (10 expected, 0 cells), the Rust sweep with `alloc n=0 first update: row 2 raw` (an empty row where the fresh state has text).
- proposed fix:    make a failed `beginUpdate` force the next one to redraw: a `needs_redraw: bool` field set by an `errdefer` at the top of `beginUpdate`, checked first in the `redraw` block and cleared when a redraw completes (or `errdefer self.rows = 0;`, which reuses the dimension check but also changes what `rows` reports until then).
- upstream status: not reported

## Candidates pending verification

Assembled from this plan's progress notes on 2026-09-11. **Not verified** — do not cite as fact.

| # | reference | suspected defect | kind | port today | source |
|---|---|---|---|---|---|
| C3 | `src/renderer/Thread.zig:698-723` `cursorCancelCallback` | superseded by UD73 (task R13-C) | crash | see UD73 | task 14d-1 |
| C4 | `src/termio/Exec.zig:1045`; `src/apprt/gtk/pre_exec.zig:39,58` | superseded by UD101 (task R13-D) | deadlock | see UD101 | tasks 14d-3, 14e-1 |
| C5 | `src/font/DeferredFace.zig:154-157, 186-197` | superseded by UD110 (task R13-D) | leak | see UD110 | task 16 |
| ~~C11~~ | ~~`src/font/face/coretext.zig:1023-1036`; `src/terminal/StringMap` tests~~ | ~~assertions cannot distinguish the fixture under test~~ STRUCK (task R13-D): refuted as a reference defect by the R13 verify pass (those tests assert what they are named for; the insensitivity only limits the port's negative controls) | ~~test-weakness~~ | ported verbatim (S90-F5, S140-F1) | task 17 |
