# rustty-vt — Tracked Deferrals (must be completed by the named sub-batch)

Hard tracker for code/tests/comments deferred from an earlier sub-batch because a
dependency wasn't ported yet. Each MUST be completed by its target sub-batch — the
target workflow consumes this file. Nothing here is "dropped" (R1); it is scheduled.

## Deferred to vt-12 (Terminal/Screen cluster)

From **vt-7 (kitty)** — these need `Screen`/`Terminal`/`ImageStorage`:

1. **`kitty/graphics_unicode.rs` — `render_placement()` + `grid()` methods.**
   Production logic + the iterator are ported; these two methods are deferred (need
   `ImageStorage` from `graphics_storage` + `Image`). Reference:
   `ghostty/src/terminal/kitty/graphics_unicode.zig`. Carries **58 inner comments**
   (the coordinate-space / scaled-rect / grid-fit logic) that G3b reports MISSING
   on `graphics_unicode` until done — EXPECTED until vt-12. Signatures documented
   in-source at graphics_unicode.rs:170-179.
   - **14 deferred tests** (graphics_unicode.rs:448-452): 11 placement-iterator
     (`unicode placement: …`) + 3 render (`unicode render placement: dog …`).

2. **`kitty/graphics_storage.zig` (1364 loc, 19 tests)** — full file. Needs `Screen`.
   Compiling stub at `kitty/graphics_storage.rs` with PORT NOTE.

3. **`kitty/graphics_exec.zig` (575 loc, 8 tests)** — full file. Needs `Terminal`.
   Compiling stub at `kitty/graphics_exec.rs` with PORT NOTE.

4. **`kitty/graphics.rs` re-exports** — `ImageStorage` (from `graphics_storage`) +
   `execute` (from `graphics_exec`) held back with PORT NOTE.

**vt-12 acceptance gain:** +~41 tests (19+8+14) and graphics_unicode G3b → 0 missing.

## Pre-existing deferred seams (lower priority, noted)
- `sgr.rs` local `SepList` — can be retargeted to re-export `parser::SepList` (vt-5
  defined the canonical one). Cosmetic; tracked PORT NOTE in sgr.rs.
- `sys.decode_png` — PNG codec hook intentionally null in the VT crate (faithful;
  Ghostty tolerates null). A real decoder rides a later FFI crate.

## Rule
A sub-batch's verify MUST treat G3b "missing" that maps to an entry here as
EXPECTED (not RED), and MUST NOT add new deferrals without recording them here.
