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
- proposed fix:    In `responded`, treat a null `vardict` on code 0 as a failed response (`orelse { log.warn(...); return; }`) instead of `vardict.?`, and pass the portal's bus name (`org.freedesktop.portal.Desktop`) as the `signalSubscribe` sender so other peers cannot reach the handler at all. The sender filter is only spoof-proof on GLib 2.80.1 or later: earlier GDBus matched a well-known sender name in a way another connection could impersonate (CVE-2024-34397). The null-vardict check is the part that fixes the crash on every GLib.
- upstream status: not reported

### UD2 — GlobalShortcuts `Activated` handler aborts on a short body and reads an undefined shortcut id on a mistyped one
- kind:            crash (short body) + ub (wrong-typed second child)
- severity:        medium (any same-user session-bus peer that knows the session handle)
- reference:       src/apprt/gtk/class/global_shortcuts.zig:587-606 (`shortcutActivated`), subscribed in `onResponse` at :352-362, at 20abdb50a
- defect:          `var shortcut_id: [*:0]const u8 = undefined; params.getChild(1, "&s", &shortcut_id);` fails two different ways, depending on the body.
  - **No second child** (`(o)`, `()`): `g_variant_get_child` reaches GLib's fatal error "Attempt to access item 1 in a container with only 1 items" and the process aborts. Measured by the task-20 reviewer on GLib 2.88.2, both on a directly built variant and after a real D-Bus message round-trip.
  - **Second child of the wrong type** (`(oo)`): GLib emits a critical and writes nothing. The handler then runs `log.debug("activated={s}", .{shortcut_id})` and `map.get(std.mem.span(shortcut_id))` on the undefined pointer.
  (Fix round 1: this entry first said the no-second-child case also "writes nothing".)
- ReleaseSafe:     short body — abort (exit 134) inside GLib. Wrong-typed child — reads uninitialised memory (Zig does not check `undefined`); in practice a segfault or a garbage lookup.
- ReleaseFast:     short body — the same GLib abort, since it does not depend on the Zig build mode. Wrong-typed child — undefined behaviour.
- reachability:    peer process, same user. The `Activated` subscription passes a NULL sender and matches only `arg0` as a path against the session handle, so an `(o)` or `(oo)` body carrying that handle reaches it. The session handle is not secret to a bus monitor, and the subscription lives as long as the session.
- port handling:   `activated_shortcut_id` reads child 1 only if it exists and is type `s`; otherwise the handler returns without logging or looking anything up — `crates/rustty-apprt/src/gtk/class/global_shortcuts.rs:850`. Before task 20 the port PANICKED here (glib-rs `child_value(1)` asserts the index).
- parity row(s):   none — found by task 20 while fixing S51-F7; NEW-FINDINGS N38 records the unclaimed port site.
- test:            `global_shortcuts_activated_shortcut_id_is_safe_on_malformed_bodies` (`(os)` yields the id; `(o)`, `()` and `(oo)` yield none). Negative control, VM 2026-09-11: restoring `params.child_value(1).str()` turns it RED with glib's index assertion (`variant.rs:314`).
- proposed fix:    Check `params.nChildren() >= 2` and the child's type (or initialise `shortcut_id` to null and use a checked `get`) before using it, and filter the subscription's sender to the portal's bus name (spoof-proof only on GLib 2.80.1+, see UD1). Simpler alternative, suggested by the task-20 reviewer: check the whole body with `params.isOfType("(osta{sv})")` (the documented `Activated` signature) and return early otherwise, before reading any child.
- upstream status: not reported

### UD3 — X11 `changeProperty`/`deleteProperty` check a status libX11 never returns
- kind:            latent
- severity:        low
- reference:       src/apprt/gtk/winproto/x11.zig:392-431 (`changeProperty` `if (status == 0) return error.RequestFailed` at :421, `deleteProperty` at :430), at 20abdb50a
- defect:          Both wrappers treat an `XChangeProperty`/`XDeleteProperty` return of 0 as `error.RequestFailed`, and `syncBlur` (`:243-287`) relies on that to keep its cached `blur_region` when a write fails. libX11 cannot return 0 from either call. The task-20 reviewer disassembled libX11 1.8.13 on the VM: each function has a single return path preceded by `mov w0, #1`. A real failure (for example `BadWindow`) arrives asynchronously through the error handler. Measured by task 20 with a C probe under headless sway's Xwayland: both calls on a bogus window returned 1, and two `BadWindow` errors (requests 18 and 19) arrived later. So the failure branch, and the cache-preserving logic behind it, is dead code. A failed write still updates the cache.
- ReleaseSafe:     no crash. After an asynchronous X error the cache disagrees with the server until the region changes.
- ReleaseFast:     same.
- reachability:    none for the status branch. The cache/server mismatch needs an X request to fail, for example against a window destroyed concurrently.
- port handling:   faithful (R1, a non-crash defect). `crates/rustty-apprt/src/gtk/winproto/x11.rs` `sync_blur` keeps the reference's `?` before `self.blur_region = region`, which is S60-F1, fixed in `7862eab7`. The test `sync_blur_keeps_the_cached_region_when_the_property_write_fails` swaps in a stub `XDeleteProperty` that returns 0, through the public function-pointer fields of x11-dl's `Xlib`, because libX11's own function cannot fail this way.
- parity row(s):   S60-F1 (and S60-F6).
- test:            as above, under GDK_BACKEND=x11 + Xwayland. Negative control, VM 2026-09-11: committing the cache before the fallible delete turns it RED.
- proposed fix:    Detect X request failure the way Xlib reports it: install a scoped error handler (or use `XSync` plus an error-trapping handler around the property writes) and only commit `blur_region` when no error was reported. Drop the `status == 0` checks, which cannot fire.
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
- test:            `goto_window_walk_matches_the_reference_list_walk` covers the empty list (display-free).
- proposed fix:    Type `listToplevels` as returning `?*glib.List` in the bindings, and `orelse return false` in `gotoWindow`.
- upstream status: not reported

### UD5 — OpenURI treats a malformed `Response` body as success and never opens the URL
- kind:            latent
- severity:        low
- reference:       src/apprt/gtk/portal/OpenURI.zig:478-499 (`responseReceived`), subscription at :202-212, at 20abdb50a
- defect:          `params.get("(u@a{sv})", &response, &results)` on a body that is not `(ua{sv})` writes nothing (GLib critical), so `response` stays 0. The switch then logs `open uri successful` and skips `openUrlFallback`, so the URL the user asked to open is silently never opened.
- ReleaseSafe:     no crash; a lost URL open.
- ReleaseFast:     same.
- reachability:    peer process, same user: the subscription has a null sender, but the request path embeds a random `usize` token (`portal.generateToken`) and exists only while a request is in flight, so the peer must be watching the bus.
- port handling:   faithful (R1, a non-crash defect). `crates/rustty-apprt/src/gtk/portal/open_uri.rs` `response_received` makes the same `g_variant_get` call and reads code 0, which also removed the port's former PANIC on `()` (S59-F3).
- parity row(s):   S59-F3.
- test:            `malformed_response_bodies_read_as_code_zero_without_panicking` pins the faithful behaviour (code 0, no fallback, `open uri successful`).
- proposed fix:    Check `params.isOfType("(ua{sv})")` first, and on a mismatch take the `2` ("ended unexpectedly") path so `openUrlFallback` still opens the URL; filter the subscription to the portal's bus name (GLib 2.80.1+, see UD1).
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
- ReleaseFast:     no check; the active area keeps the text until the pane's `capture-pane` visible reply runs `eraseDisplay(.complete)`.
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
- reachability:    VT stream: a `list-windows` reply or `%layout-change` with a deeply nested layout and a correct checksum. Measured, macOS arm64, Zig 0.16, `layout.zig` copied into a probe calling `Layout.parse` on a default `std.Thread` (16 MiB, as the io reader thread is spawned): overflow at about 9,200 levels in Debug and about 39,000 in ReleaseSafe and ReleaseFast, all well inside the 116,000 the input allows.
- port handling:   `Layout::parse` rejects nesting deeper than `MAX_DEPTH` (1000) with the existing `SyntaxError`, which the viewer turns into its defunct state; `Layout`'s `Drop` uses an explicit stack — `crates/rustty-vt/src/tmux/layout.rs:157` (PORT NOTE). The port needs the limit more than the reference: its reader thread has Rust's default 2 MiB stack, and several walkers recurse over the tree. Measured on macOS with the same kind of probe, the first to overflow did so at about 1,500 levels in a debug build (the parser) and about 2,400 in a release build (the derived `Debug` used by the action log), roughly six times sooner than the reference, not the eight an earlier estimate assumed.
- parity row(s):   NEW-FINDINGS N42 (no LEDGER row).
- test:            `deeply_nested_layout_is_a_syntax_error_beyond_max_depth` (116,508 levels and `MAX_DEPTH + 1` rejected, `MAX_DEPTH` accepted, on a default thread), `dropping_a_very_deep_layout_does_not_recurse` (200,000 levels), and the viewer's `a_layout_nested_deeper_than_max_depth_makes_the_viewer_defunct` (the deepest 1 MiB `list-windows` reply as bytes, plus `MAX_DEPTH` accepted end to end). Negative controls, macOS 2026-09-11: without the depth check the parse and viewer tests abort the test binary with "has overflowed its stack"; with recursive drop glue the drop test does the same.
- proposed fix:    Pass a depth to `parseNext` and return `error.SyntaxError` beyond a fixed limit, or parse with an explicit stack.
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
