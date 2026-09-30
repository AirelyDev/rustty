# N465: `Surface::close(&mut self)` across the runtime's close callback

This is an isolated, **non-FFI** reproduction of the borrow shape behind NEW-FINDINGS **N465**,
fixed in P7-W12 T1 (ruling 61). It is used to PROVE the fix. It follows the doctrine in memories
`rust-mut-args-strongly-protected` and `shaper-hook-sb-clean-interleave`, and the layout of
`mirihar-n456`.

## The bug

The core `Surface::close(&mut self)` ran `self.rt_surface.close(self.needsConfirmQuit())`. A
`&mut self` argument is a **protector** over the core surface for the whole call, and the runtime's
close callback re-enters that surface synchronously:

- **macOS** (when a clipboard confirmation is pending on the closing surface): `close_surface_cb`
  posts `rusttyCloseSurface`. The observer runs synchronously: `closeSurface(withConfirmation:
  false)` → `removeSurfaceNode` → the `surfaceTree` didSet → `surfaceTreeDidChange` →
  `cancelPendingClipboardConfirmation` → `pendingClipboardConfirmation = nil` → its didSet →
  `previous.cancel(from:)` → the completion → `rustty_surface_deny_clipboard_request` → embedded
  `denyClipboardRequest(&mut self)` → core `denyClipboardRequest`, a write.
- **GTK** (every close): `close` emits `close-request`, and `SplitTree.surfaceCloseRequest` takes
  `&mut *core` for `needsConfirmQuit`.

The seam's `fn close(&self, ..)` had the same problem on the embedded runtime, whose surface holds the
core surface inline: a shared protector over those bytes for the whole callback. The routes are
`handleMessage(.close / .child_exited)` (`childExited(&mut self)` and `handleMessageArms(&mut self)`
above `close`), the keybind and binding `close_surface`, `rustty_surface_request_close`'s fallback,
and `App::closeSurface`.

Zig's `*Surface` carries no protector: port soundness only, no UD.

## The fix

- Core `close` and `childExited` are raw-self (`unsafe fn(self_: *mut Surface, ..)`), and
  `handleMessage` dispatches `.close` and `.child_exited` raw. `needsConfirmQuit`'s reborrow ends
  before the callback.
- The seam is `unsafe fn close(self_: *const Self, process_alive)`. The embedded impl reads `app` and
  `userdata` by place; the GTK impl's `&Self` ends with `gobj()`.
- The keybind, binding, C-fallback and `App::closeSurface` callers go through `Surface::close(self_)`.

## Results (`bash run-all.sh`, 16 seeds each, foreground, 2026-09-30)

| candidate | SB | TB |
|---|---|---|
| `n465_shipped_macos_child_exited` | UB | UB |
| `n465_shipped_gtk_child_exited` | UB | UB |
| `n465_core_only_macos_child_exited` (negative: core raw-self, seam still `&self`) | UB | UB |
| `n465_fixed_macos_child_exited` | CLEAN | CLEAN |
| `n465_fixed_gtk_child_exited` | CLEAN | CLEAN |
| `n465_fixed_macos_all` (`.close`, `.child_exited`, the C request_close fallback) | CLEAN | CLEAN |
| `n465_fixed_gtk_all` (`.close`, `.child_exited`) | CLEAN | CLEAN |

- Shipped, SB: `not granting access to tag .. because that would remove [Unique for ..] which is
  strongly protected`, at the host's `deny_clipboard_request` (macOS) and at the split tree's
  `&mut *core` (GTK). TB (GTK): `reborrow through .. is forbidden`.
- The negative row: with the core half fixed but the seam still `&self`, SB reports `.. would remove
  [SharedReadOnly for ..] which is strongly protected` at the host's `&mut` embedded surface. Both
  halves are needed.

The live route is exercised by
`crates/rustty-apprt-c/tests/macos_window_host.rs::close_callback_reenters_the_closing_surface`
(Miri cannot see across that FFI).
