# vt-12 Plan — Terminal ↔ stream_terminal (the last giant)

Terminal.zig (13,259 loc, 66 fns, 376 tests) + stream_terminal.zig (2,247 loc, 65 tests).
Mutual cycle → struct-first. All cluster deps ported (Screen/ScreenSet/Selection/
formatter-prod/StringMap/modes/charsets/Tabstops/sgr/color/osc/kitty/Stream-Handler-trait).

## vt-12a — structs (both files compile, todo!() bodies)
- **terminal.rs:** `Terminal` (14 fields) + 8 nested (`Colors, Dirty, ScrollingRegion,
  Options, SemanticPrompt, ScrollViewport[plain enum], DeccolmMode, SwitchScreenMode`) +
  `Flags` + 3-state `mouse_shift_capture` enum + `GlyphGlossary` ZST stub + 66 method sigs
  + `active_mut()` helper (`unsafe { &mut *self.screens.active }`).
- **stream_terminal.rs:** `Handler{ terminal:*mut Terminal, effects, apc_handler[STUB], default_cursor* }`
  + `Effects`(9 Option<fn ptr>, default readonly=all None) + `type Stream<'a>=stream::Stream<'a,Handler<'a>>`
  + `Handler::{init,deinit,vt}` sigs + `impl stream::Handler for Handler` (~93 methods, todo!()).
- `Handler.terminal` is `*mut Terminal` (raw ptr, NOT &mut — Stream owns Handler which aliases
  Terminal; mirrors screen_set `*mut Screen`). Gate: build + clippy -D + fmt. No tests.

## vt-12b — methods (376+65 tests), 8 sub-batches
1. **12b-1 print foundation** (init/deinit/vtStream/vtHandler/printString/print[379]/printCell[168]/
   printWrap/plainString) — TENTPOLE, ~70 tests, gates everything. Top Miri/ASan exposure.
2. **12b-2 cursor/charset/tabs** (cursorUp/Down/Left[104]/Right, CR/LF/BS, hTab, tabClear/Set/Reset,
   charset, setCursorPos, save/restoreCursor) ~58t.
3. **12b-3a erase** (eraseChars/eraseLine/eraseDisplay[132]/decaln/deleteChars) — tracked-pin RAII.
4. **12b-3b scroll/lines/margins** (index[135]/reverseIndex/scrollUp/Down/Viewport/insertLines[201]/
   deleteLines[179]/insertBlanks[101]/margins/rowWillBeShifted) — heaviest, tracked-pin RAII.
5. **12b-4 SGR/protected** (setProtectedMode/setAttribute/printAttributes[93]) ~8t.
6. **12b-5 modes/resize/screen-switch** (deccolm/resize/switchScreen/switchScreenMode[96]/fullReset) ~26t.
7. **12b-6 semantic/pwd/title/dirty** (semanticPrompt[121]/cursorIsAtPrompt/set+getPwd/Title[Vec<u8>
   +NUL sentinel]/isDirty/clearDirty) ~19t.
8. **12b-7 stream_terminal Handler** — the 156-arm vtFallible dispatch + 21 helpers (writePty/bell/
   reportDeviceAttributes/deviceStatus/windowTitle/setMode/colorOperation/kittyColorOperation/...)
   + Effects callbacks. ~65t. apcEnd/apc_handler STUBBED. Lands after Terminal methods exist.

Raw-ptr hotspots (Miri+ASan+tracking-alloc gates): print/printCell/printWrap/rowWillBeShifted/
insertLines/deleteLines/scrollUp/insertBlanks/deleteChars/eraseChars/eraseLine/eraseDisplay.

## vt-12c — reactivated deferrals (+169 tests; DEFERRALS.md)
- **12c-kitty** (+41): graphics_storage→ImageStorage(19, needs Screen✓) → graphics_exec→execute(8,
  needs Terminal✓) → graphics_unicode render_placement/grid(14 + 58 comments) + wire Group G wrappers.
- **12c-fmt** (+105): TerminalFormatter(formatter.rs:285) + 100 formatter tests (Terminal::vtStream-gated)
  + 5 TerminalFormatter Screen tests. Can start once 12b-1 lands.
- **12c-screen** (+23): the 23 deferred Screen.zig test bodies (Terminal/ImageStorage-gated) →
  drives Screen test-region G3b → 0.

## Seams
apc/glyph: ZST/no-op stub (continuing font-block deferral past vt-12). kitty graphics: feature-gate;
Screen.kitty_images is real storage, Terminal forwards. Effects: Option<fn ptr>, apprt fills later.
ScrollViewport TaggedUnion C-ABI → rustty-vt-c (deferred); plain enum in pure-VT. unicode width/
grapheme: reuse existing Screen/print seam. pwd/title: Vec<u8> + trailing-0 sentinel, getter Option<&[u8]>.

## Process (per the lesson + restored Miri)
EACH 12b sub-batch: SCOPED G3b of just-filled methods (diff missing vs prior batch) + build/test +
clippy -D + fmt + Miri (small raw-ptr-test subset, isolated, warm cache) + ASan + tracking-alloc on
page-touching batches. Per-step commits; adversarial review per sub-batch. Whole-Terminal G3b at finale.
