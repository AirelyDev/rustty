# Symbol-level audit — accepted absences

Companion to `.port/tools/audit_symbol_coverage.py` (pass 2 of the systematic
oracle→port audit). The tool checks every oracle `pub fn` for a counterpart token
anywhere in `crates/`. **2554 symbols across 485 files were checked; 13 had no
counterpart token, and all 13 resolved.** Each is recorded below with the reason,
because an absence that is merely *believed* fine is indistinguishable from one
nobody looked at.

Format: `src/path.zig::symbolName` — reason. The tool parses the leading
backticked token, so keep that shape.

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

## Collapsed into the call site, deliberately (3)

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
