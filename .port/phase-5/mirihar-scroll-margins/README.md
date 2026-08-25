# mirihar-scroll-margins

Isolated, non-FFI Miri proof artifact for the **P5-W2.3 ScrollMargins guard**
(upstream `90bce0d2d` "terminal/kitty: scroll and clip image placements within
margins"; phase-5 plan SB/UB hazard #1; ported in
`crates/rustty-vt/src/kitty/graphics_storage.rs` `scroll_margins_begin` /
`ScrollMargins` + the `crates/rustty-vt/src/terminal.rs` scroll call sites).

See the module doc in `src/main.rs` for the exact aliasing structure modeled
and the candidate list.

## Results (2026-08-25, this machine)

```
cargo +nightly miri run -q -- ported                                 # ok (Stacked Borrows)
MIRIFLAGS=-Zmiri-tree-borrows cargo +nightly miri run -q -- ported   # ok (Tree Borrows)
cargo +nightly miri run -q -- ub_end_after_untrack                   # UB report:
#   error: Undefined Behavior: memory access failed: allocN has been freed,
#   so this pointer is dangling   (the guard's end() writing a recorded pin
#   that a storage mutation untracked between begin and end)
```

The foil proves Miri catches the exact violation class the guard's
no-storage-mutation-between-begin-and-end contract prevents (the `*Pin`
dangling hazard named by the assessment). The real-code counterpart is also
covered directly: the 11 `scroll margins` + 5 `animation tick` storage tests
run clean under `cargo +nightly miri test -p rustty-vt --lib` filters.
