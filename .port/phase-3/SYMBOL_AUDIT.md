# Symbol-level audit — accepted absences

Companion to `.port/tools/audit_symbol_coverage.py` (pass 2 of the systematic
oracle→port audit). The tool checks every oracle `pub fn` for a counterpart token
in a Rust file that belongs to it (since P8-W10 T4: a counterpart file of the
oracle file, or a file naming the function's container; see "Note on the
matcher"). At the `88f57ee66` pin it checks **2831 symbols across 530
files**; every absence is recorded below with its reason, because an absence that
is merely *believed* fine is indistinguishable from one nobody looked at.

Format: `src/path.zig::symbolName` — reason. The tool parses the leading
backticked token, so keep that shape.

Two of the rows below (`DBus.zig::addParameter`, `hash_map.zig::keyIterator`) no
longer reach this ledger: the port NAMES each in a comment — a PORT NOTE at
`gtk/app.rs:218` and the ported `// var it = self.placements.keyIterator();` line
at `graphics_storage.rs:1023` — and the tool's index is a token scan that cannot
tell a mention from a definition. They stay listed because the absence each one
explains is still real; deleting them would leave the next reader to re-derive it
from scratch the first time either comment is reworded.

## Generated, not written — GObject properties (8)

`#[derive(glib::Properties)]` + `#[property(get, set)]` on
`crates/rustty-apprt/src/gtk/class/surface.rs:187-198` **generates** the accessor
pair for each declared property. The generated methods are real and callable;
they simply do not exist as literal source tokens, so a text index cannot see
them. This is a better port than hand-writing the pairs, not a worse one.

- `src/apprt/gtk/class/surface.zig::getFocused` — generated from `focused: Cell<bool>` (`surface.rs:219`)
- `src/apprt/gtk/class/surface.zig::getMapped` — generated from `mapped: Cell<bool>` (`surface.rs:223`)
- `src/apprt/gtk/class/surface.zig::getMinSize` — generated from `min_size: RefCell<Option<Size>>` (`surface.rs:237`)
- `src/apprt/gtk/class/surface.zig::getHAdjustment` — generated property accessor
- `src/apprt/gtk/class/surface.zig::setHAdjustment` — generated property accessor
- `src/apprt/gtk/class/surface.zig::setVAdjustment` — generated property accessor
- `src/apprt/gtk/class/surface.zig::getMouseHidden` — generated; **0 callers upstream**
- `src/apprt/gtk/class/surface.zig::getBellRinging` — generated; **0 callers upstream**

## Collapsed into the call site, deliberately (6)

- `src/apprt/gtk/class/split_tree.zig::initReused` — realized inline in
  `build_tree`'s reuse arm (`split_tree.rs:860-865`). The reference's
  `BuildTreeResult { widget, release_ref }` collapses because every gtk4-rs
  widget value is already a sunk strong reference; the ref-then-detach ordering
  that makes `initReused` correct is preserved. PORT NOTE at `split_tree.rs:842`.
- `src/apprt/gtk/ipc/DBus.zig::addParameter` — the whole `DBus.zig` type is
  flattened into `app.rs`'s `ipc_send`/`ipc_bus_target` (WS20, `12c951c`);
  `addParameter`'s job is the inline `Vec<glib::Variant>` build at
  `app.rs:216-227`.
- `src/terminal/hash_map.zig::getOrPutAdapted` — `hash_map.rs:631` records that
  this port's `*_adapted` forms were already identity wrappers (no adapted form
  exists here), so `fedd42e8d`'s consolidation removed the alias rather than
  porting a wrapper that wraps nothing.
- `src/apprt/gtk/class/split_tree.zig::initNew` — the other `BuildTreeResult`
  constructor, collapsed with `initReused` above: `build_tree`'s no-ancestor arm
  returns the freshly built `SurfaceScrolledWindow` as an owned `Widget`
  (PORT NOTE at `split_tree.rs:1122`). Until P8-W10 T4 the bare-name match found
  the word in a `surface_scrolled_window.rs` comment.
- `src/terminal/hash_map.zig::getKey` — the context-inferring wrapper
  (`@sizeOf(Context) != 0` is a compile error); this port's maps always take the
  context, so the function is `get_key_context` (`hash_map.rs:922`), as
  `getKeyPtr` is `get_key_ptr_context`. **0 callers** in the oracle. Until
  P8-W10 T4 the bare-name match took `rustty-vt-c` `key_event::get_key`.
- `src/unicode/lut.zig::hash` — the hasher of the anonymous `BlockMap` context
  struct; `lut.rs` derives `Hash`/`Eq` on `Block` instead (PARITY note at
  `lut.rs:47`). Until P8-W10 T4 any `hash` in the tree matched it.

## A comptime type the model has no place for (1)

- `src/lib/packed.zig::PackedTaggedUnionOptions` — a comptime **type
  constructor**: given the union and tag types it *builds* a struct with one
  `?Arm` field per tag enum field (`@Struct(.auto, null, &names, &types,
  &attrs)`), which is then the `options` parameter of `PackedTaggedUnion`. Both
  halves of that only exist to carry the tag→arm mapping into a comptime
  reflection pass. `crates/rustty-lib/src/packed.rs` models the whole family as
  `'static` descriptor tables (the file header states why: "Rust has no
  reflection over the port's hand-rolled bitfields"), so the mapping the
  generated options struct exists to hold is written directly as
  `PackedTaggedUnionLayout.arms: &'static [PackedTaggedUnionArm]`, one entry per
  tag in declaration order, `source: None` for the reference's `null` arm. The
  observables are identical — `page.rs`'s `CONTENT` and `packed.rs`'s
  `VALUE_LAYOUT` are the two live instances — and the reference's
  `@compileError` checks survive as the const-evaluable `validate()`. A Rust
  counterpart for the *generator* would have nothing to generate.

## Renamed or with no Rust form, uncovered by the stem fallback's removal (3)

P8-W10 FX10 scoped a top-level `pub fn` (no inner container) to the oracle
file's counterparts alone. Each of these had matched through an unrelated file
that only mentioned the module stem.

- `src/terminal/hash_map.zig::getPtr` — the context-inferring wrapper, like
  `getKey` above: this port's maps always take the context, so it is
  `get_ptr_context` (`hash_map.rs:933`). The one oracle caller,
  `page.zig:1593` `map.getPtr(cell_offset)`, is `map.get_ptr_context(cell_offset,
  &actx)` at `page_impl.rs:2886`. (The tool sees no container here because
  `HashMapUnmanaged`'s `fn ... type {` signature spans several lines.) Until
  P8-W10 FX10 any `get_ptr` in a file mentioning `hash_map` matched it.
- `src/apprt/gtk/Surface.zig::cgroup` — ported as `cgroup_path`
  (`gtk/class/surface/core_callbacks.rs:1021`, `PORTS: Surface.zig:40-42`), a
  placeholder leaf. `cgroup` is under 8 characters, so the prefix rule does not
  reach `cgroup_path`, and that file cites the bare `Surface.zig`, not the full
  path. Until P8-W10 FX10 any `cgroup` in a file mentioning `Surface` matched it.
- `src/lib/compat/testing.zig::refAllDeclsRecursive` — a comptime helper that
  references every declaration of a type so the Zig compiler analyses it. Rust
  compiles every item anyway, so it has no Rust form. Its two oracle callers,
  the root tests of `apprt/action.zig:1083` and `terminal/mouse.zig:168`, are
  ported as tests at `action_tests.rs:443` (`ref_all_decls_recursive`) and with
  a PORT NOTE at `mouse.rs:400`.

## Dead upstream — absence is correct (2)

Porting an uncalled function would add an untested code path with no behaviour to
verify, which is worse than not porting it.

- `src/terminal/hash_map.zig::keyIterator` — **0 callers** in the oracle.
- `src/apprt/gtk/winproto/noop.zig::updateConfigEvent` — **0 callers**; the noop
  backend's arm is `false` by construction in rustty's dispatch.

## Note on the matcher

The first run reported 16 absences; three (`putAssumeCapacityNoClobber`,
`fetchPutAssumeCapacity`, `getKeyPtr`) were false, because this port routinely
**suffixes** a ported name — `put_assume_capacity_no_clobber_context` — and exact
token matching missed all three. The tool now prefix-matches names of 8+
characters. That bias is deliberate: a false ABSENT wastes a reader's time and,
on this project's evidence, tempts someone to "fix" the audit instead of the code.

P8-W10 T4 made the match container-aware. The token used to count from any file
in `crates/`, so `modes.zig` `ModeState.getDefault` read as present at the
`246f70287` pin through `rustty-font-shaper` `hb.rs` `Language::get_default`.
The token (or its 8+ prefix) now has to be in a counterpart file of the oracle
file (G-FILES' mapping: filename or citation) or in a file naming the function's
container (`ModeState`, `Terminal`, or the module stem). The first run moved
three more rows here (`initNew`, `getKey`, lut `hash`), each a common word found
in an unrelated file; `--self-test` rebuilds the getDefault case.

P8-W10 FX10 dropped the module-stem fallback. A top-level `pub fn` (a method of
a file-struct module such as `Terminal.zig`, or a free function of `modes.zig`)
now counts only in a counterpart file, because "a file naming `terminal`" was
most of `crates/`. The re-run moved three rows here (`getPtr`, `cgroup`,
`refAllDeclsRecursive`); `--self-test` arm [d] is the negative control.
