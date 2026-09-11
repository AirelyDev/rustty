> **SUPERSEDED — 2026-09-11, user ruling, `.port/PORT_RULES.md` R13.**
> The policy this file states — reproduce the reference's defects faithfully and do not fix
> them locally — is **no longer in force**. A reference defect that real input can reach is now
> handled safely in the port and recorded in `.port/parity-closure/UPSTREAM-DEFECTS.md` for an
> upstream fix proposal. The entries below are kept as historical record and are being
> re-verified and folded into that file; any entry describing a *faithfully ported* crash, UB,
> leak or deadlock is now work to re-address, not a closed decision. **Do not follow this file's
> "do not fix locally" instruction.**

# Bugs that are FAITHFULLY PORTED — report upstream, do not fix locally

A full-fidelity port reproduces the reference's defects along with its behaviour.
When rustty and Ghostty are wrong in the same way, patching rustty alone is a
**divergence**, not a fix: it silently forks the two implementations and the next
person to diff them finds an unexplained difference. So these are recorded here
and left in place.

That is not the same as ignoring them. Each entry needs an upstream report, and
if upstream fixes it the fix comes back here as a normal port.

Distinguish this file from `NON_PORTS.md` (things deliberately not ported) and
`LOOSE_ENDS.md` (rustty's own unfinished work). Everything here is *correctly*
ported.

---

## U1 — `GstPlay` teardown use-after-free / mutex UB (GTK apprt, Linux)

**Found:** 2026-07-31, Linux lane, on Fedora 44 / aarch64 / GStreamer 1.28.1.

`GtkMediaFile` is backed by `GstPlay`, which runs its own `gst_play_main` thread.
Dropping the **last** reference within microseconds of construction races that
thread's startup: the play thread is still inside `gst_play_set_uri_internal` /
`on_error` → `api_bus_post_message`, and the bus message it posts ends up holding
the final `GstPlay` reference. `gst_play_finalize` therefore runs **on the play
thread** and calls `g_mutex_clear()` on a mutex that thread still holds.

Two measured signatures, both ending inside `GstPlay`:

```
SIGABRT: g_mutex_clear() called on uninitialised or locked mutex
  g_mutex_clear <- gst_play_finalize <- g_object_unref <- _gst_message_free
  <- gst_bus_post <- api_bus_post_message <- gst_play_set_uri_internal

SIGSEGV: gst_object_ref on a freed object
  g_type_check_instance_is_fundamentally_a <- g_object_ref <- gst_object_ref
  <- gst_message_new_custom <- api_bus_post_message <- on_error
  <- gst_play_pause_internal
```

**Where rustty is exposed in production:** `crates/rustty-apprt/src/gtk/media.rs:63`,
`bell_media_file`, drops the old handle when `bell-audio-path` changes. If that
drop is the last reference and the pipeline is young, this is the identical UAF.

**Why it is not fixed here:** the oracle does exactly the same thing —
`media.zig:66` `media_file.unref()`. There is no GTK-level API that closes the
window; `clear()` before unref only narrows it (measured: still 2/60). Adding a
workaround would be a deviation from the reference at the one line where the two
are currently identical.

In practice the window is small, because a cached `MediaFile` has normally lived
across at least one config reload before being replaced.

**What WAS changed:** only the *test*
(`media.rs:156`), which reproduced the race ~30% of the time and made
`cargo test --workspace` red at random on Linux. It now leaks the final handle
deliberately, with the reasoning inline. No assertion was weakened.

**Reduction status:** a minimal pure-GTK4 C reproducer
(`gtk_media_file_new_for_filename` + immediate `g_object_unref`, on the main
thread and on a `GThread`) did **not** reproduce — 0/20 and 0/25. The race needs
tighter timing than the C reproducer produces, so this is not yet reducible to a
standalone upstream report against GStreamer.

**Also worth knowing:** the oracle's own test (`media.zig:135-158`) never creates
the files, so `accessAbsolute` fails and it hits `error.SkipZigTest`. **That
regression guard has never actually executed upstream.** The Rust port created
real temp files and called `gtk4::init()`, which is what turned a permanent skip
into a test that spins up two real GStreamer pipelines — and thereby found this.

---

## U2 — `cursor-text = cell-foreground` panics in ReleaseSafe

**Found:** 2026-07-31, `unreachable!()` audit.

`crates/rustty-renderer/src/generic.rs:5006`, `:5021`, `:5036` faithfully
reproduce `generic.zig:2067`, where the `cell-foreground` arm of `cursor-text`
reaches an `unreachable`. In Zig's ReleaseSafe this panics; in ReleaseFast it is
UB. The configuration that triggers it is a documented, user-settable value.

Ported as-is. Fixing it locally would mean inventing behaviour the reference does
not have, and the correct resolution is an upstream decision about what that arm
should do.

---

## U3 — tmux control-mode integer overflow on unbounded `([0-9]+)`

**Found:** 2026-07-31, untrusted-input panic sweep.

`control.zig:233-237` parses tmux control-mode notification fields with
`std.fmt.parseInt(...) catch unreachable`, and the regexes that produce those
captures use an **unbounded** `([0-9]+)`. So a line like

```
%output %99999999999999999999999999 x
```

overflows `usize`. Upstream that is `unreachable` — UB in ReleaseFast, a panic in
ReleaseSafe. rustty reproduces it at `crates/rustty-vt/src/tmux/control.rs:384`
and seven siblings, where `parse_int_usize(...).unwrap()` panics.

**Reachability is proven, not assumed:** a throwaway probe drove the malformed
line through the real parser and panicked at `control.rs:384:83`. The probe was
removed afterwards.

Left faithful. rustty's behaviour matches the reference's ReleaseSafe behaviour
exactly; making it recover would invent semantics the oracle does not define, and
the right fix (bound the capture, or return an error) is an upstream decision.

Note the contrast with the tmux `unreachable!()` defect fixed the same day
(`control.rs:315`): there the oracle **recovered** (`catch |err| { log.warn;
break :cmd; }`) and rustty panicked, which is a divergence. Here both panic. Same
file, same subsystem, opposite verdicts — which is why each site has to be read
against the oracle rather than pattern-matched.
