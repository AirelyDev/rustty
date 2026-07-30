# mirihar-ws20-flatpak

Isolated Stacked-/Tree-Borrows proof for the **one** new cross-thread
raw-pointer handoff WS20 introduced: `rustty_os::flatpak::FlatpakHostCommand`
handing `self` to a detached worker thread (`spawn` -> `threadMain`) while the
spawning thread keeps using the same object (`wait` / `waitXev` / `signal`), and
the GIO `onExit` callback recovering it from an opaque `gpointer`.

The real module cannot run under Miri (it links GIO/GLib and issues DBus calls —
foreign functions Miri refuses to execute), so the aliasing SHAPE is reproduced
here with the FFI removed and nothing else changed.

## Result (2026-07-30, `nightly-aarch64-apple-darwin`)

| run | result |
|---|---|
| `MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly miri run` | **PASS** — `sound: ok (pid=4321 status=3)` |
| `MIRIFLAGS="-Zmiri-tree-borrows -Zmiri-strict-provenance" cargo +nightly miri run` | **PASS** — `sound: ok (pid=4321 status=3)` |
| `MIRIFLAGS="-Zmiri-strict-provenance" cargo +nightly miri run -- --unsound` | **FAILS as required** — SB: `<2036> was later invalidated ... by a write access` |
| `MIRIFLAGS="-Zmiri-tree-borrows" cargo +nightly miri run -- --unsound` | **FAILS as required** — `Data race detected between (1) non-atomic write on thread main and (2) retag read ... on thread unnamed-1` |

The `--unsound` arm is the shape the FIRST WS20 draft had (`&mut self`
receivers + `threadMain(&mut *ptr)` on the worker). It is the negative control:
a harness whose failure arm never fires proves nothing.

## What the green arm certifies

The shipped shape:

* every `FlatpakHostCommand` method takes `&self` (`spawn`, `wait`, `waitXev`,
  `signal`, `start`, `updateState`), so **no `&mut` to the command exists
  anywhere** and there is no exclusive/protected tag a foreign thread's access
  could invalidate;
* all mutable state lives inside `state_mutex: Mutex<State>` (WS20 moved
  `state` INTO the mutex that the Zig reference only documented as guarding it —
  mutex-by-convention over a sibling field is a data race in Rust);
* the worker receives a `*const FlatpakHostCommand` and reborrows it as `&`;
* the `onExit` GIO callback userdata is `self as *const _ as *mut c_void` and is
  recovered as `&`, never `&mut`.

The switch is an **argv** flag, not an env var: Miri does not forward the host
environment by default, so an env-var switch would silently take the sound path
and the negative control would be vacuous.
