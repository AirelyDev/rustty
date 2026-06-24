# Screen-SCC Plan (vt-10 structs → vt-11 methods+tests)

The Screen↔Selection↔formatter↔StringMap cluster + ScreenSet. **18,841 loc, 311 tests**
(Screen 188, formatter 100, Selection 18, StringMap 3, ScreenSet 2). Done struct-first
(the PageList discipline) because of the 4-way type cycle.

## vt-10 — ONE structs phase (whole cluster compiles, method bodies `todo!()`)
Define types in this order (cross-module forward refs resolve within the one crate):
1. **Selection** — `Selection{bounds,rectangle}`, `Bounds::{Untracked{start,end:Pin}, Tracked{start,end:*mut Pin}}`, `Order`(4), `Adjustment`(10). Plain enums (no C-ABI). `*mut Pin` is faithful: PinPool gives stable pointers (pin.rs track_pin→*mut Pin).
2. **Screen** — `Screen` struct (ref fields 37–82) + 13 nested (`Dirty, SemanticPrompt(+SemanticClick), Cursor(3 raw page ptrs: page_pin/page_row/page_cell), SavedCursor, CharsetState, Options, Scroll, Resize, SelectionString{sel,map:Option<*mut StringMap>}, SelectLine, LineIterator, PromptClickMove`) + 3 inline param structs. `kitty_images: kitty::graphics::ImageStorage` → use the existing deferred stub field.
3. **formatter** — `ScreenFormatter{screen:*const Screen,...}` + `Content`/`Extra`/`Format`/`Options`/`CodepointMap`/`Replacement(string→Vec<u8>)`/`PinMap`/`PageFormatter`/`PageListFormatter`/`TrailingState`. **`TerminalFormatter` → stub** (needs Terminal, vt-12).
4. **StringMap** — `StringMap{string:Vec<u8> (NUL invariant), map:Vec<Pin>}` always; `SearchIterator`/`Match`/`oni::*` behind `#[cfg(feature="oniguruma")]`.
5. **ScreenSet** — `ScreenSet` + `#[repr(C)] Key{primary=0,alternate=1}`. (active:*mut Screen aliases `all` → consider key/index for borrowck; PORT NOTE.)
Gates: build green (todo! allowed via #[allow]); clippy; fmt; acyclic; existing 819 tests still pass. (G3b deferred to vt-11 — bodies/comments land then.)

## vt-11 — methods+tests, sub-batched (each sub-batch's tests run against landed deps)
1. Selection methods + 18 Selection tests (need Screen::init + testWriteString live).
2. Screen core (init/deinit/clone/reset, cursor-move, clearing, scrolling, resize) + ~120 non-Selection Screen tests.
3. Screen selection methods (select*, selectionString, lineIterator) + formatter PRODUCTION code (PageFormatter/PageListFormatter/ScreenFormatter) + ~40 Selection-dependent Screen tests.
4. Screen semantic/prompt (promptClickMove 18 + osc::semantic_prompt) + the 1 StringMap+tripwire alloc-failure test.
5. StringMap (oniguruma-gated; 3 `#[cfg(feature="oniguruma")]` tests) + ScreenSet (7 methods, 2 tests).

## Deferred from this cluster → vt-12 (record in DEFERRALS.md)
- **ALL 100 formatter tests + 5 TerminalFormatter Screen-tests** — Terminal-gated (parse via Terminal::vtStream). formatter production code still lands (vt-11.3), exercised via Screen dump/selectionString.
- kitty graphics_storage/exec/render_placement — need Screen+ImageStorage+Terminal co-port (Screen embeds ImageStorage but graphics_storage methods take &Screen → mutual → vt-12).

## Seams
oniguruma (libonig FFI, NOT regex crate — backtracking budget differs) `#[cfg(feature="oniguruma")]`; unicode::table::get(cp).width (rustty-unicode); formatter writer → &mut dyn io::Write / Vec<u8> (Discarding→counting sink); fastmem::rotateOnceR; tripwire selectionString_tw (rustty_util::tripwire, reuse pagelist idiom); quirks.inlineAssert→debug_assert!; build_options→crate::options.

## Risks
R1 formatter tests 100% Terminal-gated (defer). R2 Cursor 3 raw page ptrs + Bounds::Tracked *mut Pin (mirror PinPool exactly; clone remap via TrackedPinsRemap access.rs:721). R3 Screen↔Selection co-define in vt-10. R4 ScreenSet active aliases all (key/index). R5 [:0]const u8 NUL invariant on selectionString/StringMap.string.
