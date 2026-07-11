# UNSOUND_SAFE — ledger of safe-Rust surfaces whose soundness rests on unchecked invariants

Seeded 2026-07-10 (plan T17 step 3; BUN_PORT_LEARNINGS §4-D7; PORT_RULES R9
draft). This is the class that grep-for-`unsafe` cannot see (Bun: `as_mut(&self)
-> &mut Self` was "invisible to any audit that greps for the unsafe keyword").
Companion to the census (`.port/audits/count-unsafe.sh`,
`unsafe-census-<shortsha>.json`).

**Scope of an entry:** a *safe* `fn`/struct/field whose misuse from safe code
(or whose internal invariant silently breaking) is UB in a release build.
Honest `unsafe fn` / `unsafe extern "C" fn` signatures are NOT entries — the
obligation is in the signature (rustty-vt-c's 154 `pub unsafe extern "C" fn`
exports are the good case).

**Triage taxonomy** (per D7):
- **VT-STREAM** — the deref/alias executes while processing untrusted VT
  input; a discipline bug anywhere upstream becomes input-triggerable UB.
- **INTERNAL** — only workspace (or embedding-host) code can supply the bad
  pointer/aliasing; misuse requires writing new incorrect calling code.
- **THEORETICAL** — hazard-class pattern with no live UB path today (dead
  branch, consumed-in-call, no-op body); tracked so the hazard survives
  refactors.

**Rules (R9 draft):** soundness fixes outrank count-reduction, always. The
"fixed" bar: safe code cannot cause UB in a release build — debug assertions
do not qualify; wrappers that hide the invariant do not qualify. Entries leave
this ledger only via made-safe/redesign, an honest `unsafe fn` signature, or a
Miri-harness-proven argument (never hand-reasoning, per
shaper-hook-sb-clean-interleave).

**Status:** SEED. Sources: `.port/audits/LIFETIMES_AUDIT_2026-07-10.md` (T16)
+ targeted greps (committed below). The full per-block bulk review (terminal.rs
894 / screen.rs 529 / formatter.rs 247 unsafe bodies) is T21 sweep-2 auditor #6;
retro `SAFETY(origin/dest)` tagging is T17 step 2 (deferred to the detectors
pass).

**Grep seeds (reproducible):**

```
rg -n 'clippy::not_unsafe_ptr_arg_deref' --glob '*.rs' crates    # 27 sites: safe fns deref'ing raw-ptr args (incl. 3 module-scope allows)
rg -n 'clippy::mut_from_ref'            --glob '*.rs' crates     # 3 sites: &self -> &mut accessors
rg -n -U 'pub struct [A-Za-z]+[^;{]*\{[^}]*\*(mut|const) ' --glob '*.rs' crates/rustty-vt/src crates/rustty-vt-c/src   # lifetime-less pointer-holding pub structs
```

---

## Class A — safe `pub fn`s that dereference raw-pointer arguments

The deliberate fidelity design of memory `rust-mut-args-strongly-protected`:
Zig `*Page`/`*Row`/`[]Cell` args port as raw-ptr args precisely because `&mut`
args re-read across `assert_integrity` would be SB UB. That choice is
sanctioned; what this ledger tracks is that the fns stayed **safe**, so the
"pointers must come from this page/list" contract (page.rs:58-63 module
doctrine) is enforced nowhere in the type system.

| # | Site | Surface | Unsound-safe because | Triage |
|---|------|---------|----------------------|--------|
| A1 | crates/rustty-vt/src/page_impl.rs:1282 `get_row(&self, y) -> *mut Row`; :1290 `get_cells(&self, row: *const Row) -> *mut [Cell]` | `&self` -> `*mut` escape hatches; every other A2 pointer originates here | Launders shared access into mutable pointers; two safe calls yield aliasing `*mut` into the same arena — soundness rests entirely on caller interleaving discipline | VT-STREAM |
| A2 | crates/rustty-vt/src/page_impl.rs:944 `clone_from(*const Page)`, :977 `clone_row_from`, :989 `clone_partial_row_from`, :1318 `move_cells(src_row: *mut Row, ..)`, :1416 `swap_cells`, :1492 `clear_cells`, :1584 `lookup_hyperlink(*const Cell)`, :1597 `clear_hyperlink`, :1627 `update_row_hyperlink_flag`, :1747 `set_hyperlink`, :1997 `lookup_grapheme`, :2012 `move_grapheme`, :2034 `clear_grapheme`, :2067 `update_row_grapheme_flag`, :2096 `update_row_styled_flag` | Page cell/row API family (module-wide lint allow at page.rs:63) | Safe callers can pass pointers from another page, a freed page, or overlapping src/dst; all deref unconditionally (debug-only origin asserts in get_cells) | VT-STREAM (print/erase/scroll hot path executes these on untrusted input) |
| A3 | crates/rustty-vt/src/pagelist/pin.rs:85 `untrack_pin(&mut self, p: *mut Pin)`; :629 `Pin::grapheme(&self, cell: *const Cell)`; :636 `Pin::style(&self, cell: *const Cell)` | Pin lifecycle + cell lookups through `(*self.node)` | untrack_pin `destroy`s through the arg (double-untrack = double-free from safe code, guarded only by the tracked-set remove); grapheme/style deref both the stored node ptr and the caller's cell ptr | VT-STREAM (style/grapheme reads on render/format paths) |
| A4 | crates/rustty-vt/src/pagelist/grow.rs:436 `increase_capacity(node: *mut Node)`; :572 `compact(node: *mut Node)`; erase.rs:24 `erase_row`, :150 `erase_row_bounded` | PageList structural mutation with raw node args / interior raw hops | Node args deref'd + realloc'd; a stale node ptr from a safe caller (post-prune) is UAF | VT-STREAM (resize/scrollback/index driven) |
| A5 | crates/rustty-vt/src/screen.rs:2012 `clear_cells(&mut self, page: *mut Page, row: *mut Row, cells: *mut [Cell])`; :2130 `clear_unprotected_cells(..)` | Screen-level cell clearing (ED/EL) | Trusts all three pointers to be mutually consistent (cells within row within page); derefs + mutates | VT-STREAM (ED/EL/DECSED sequences) |
| A6 | crates/rustty-vt/src/search/pagelist.rs:76 `PageListSearch::init(.., list: *mut PageList)`; search/screen.rs:256 `ScreenSearch-family init(.., screen: *mut Screen)`; search/sliding_window.rs:855 `append(node: *mut Node)`; search/thread.rs:530 `select`, :1217 `feed(.., t: *mut Terminal)` | Search subsystem: safe ctors STORE the ptr (see C7) and safe methods deref it | Search object outliving (or interleaving `&mut` with) the Terminal/Screen/PageList it points into is UAF/SB UB; nothing ties the lifetimes | INTERNAL (search is driven by the embedder's search API, not by VT bytes) |
| A7 | crates/rustty-core/src/app.rs:24 (module-scope allow) — public App fns deref `*mut Surface` args under the "you must always call hasSurface to validate it" doctrine | Core app/mailbox seam | hasSurface-then-deref is a TOCTOU-shaped contract enforced by convention; any safe caller skipping validation derefs a dangling surface | INTERNAL (apprt/embedder-supplied pointers) |
| A8 | crates/rustty-apprt/src/embedded.rs:67 (module-scope allow) — public methods deref raw `*Surface`/`*App` args (`closeSurface`/`redrawInspector`/`keyEvent`) | apprt embedded C seam | Same class as A7; host app owns pointer validity | INTERNAL |
| A9 | crates/rustty-lib/src/allocator/convenience.rs:19 (module-scope allow) | C-allocator convenience wrappers | Safe fns deref/free through caller-supplied allocator ptrs | INTERNAL (C-ABI plumbing) |
| A10 | crates/rustty-font-shaper/src/ft.rs:290 `done_mm_var(self, mm: *mut FT_MM_Var)` | Safe wrapper over `FT_Done_MM_Var` | Safe code can pass any pointer straight into a C free-style call (double-done / bogus ptr) | INTERNAL (ptr produced by the paired get_mm_var path) |
| A11 | crates/rustty-renderer/src/generic.rs:2185 `surface_init<S>(rt_surface: *mut S)`, :2198 `finalize_surface_init`, :2210 `thread_enter` — bodies are `let _ = rt_surface` no-ops today; :2368 `set_font_grid(&mut self, grid: *mut SharedGrid)` stores the ptr | Renderer surface-attach seam (W-ATTACH deferral) | No deref today (first three); set_font_grid stores a lifetime-less grid ptr the draw path later derefs | THEORETICAL (first three, until W-ATTACH lands — re-triage then); INTERNAL (set_font_grid) |

## Class B — `&self` -> `&mut` accessors (`clippy::mut_from_ref`)

| # | Site | Surface | Unsound-safe because | Triage |
|---|------|---------|----------------------|--------|
| B1 | crates/rustty-vt/src/stream_terminal.rs:254 `fn term(&self) -> &mut Terminal` (private) | The VT stream Handler's back-pointer to the Terminal that owns the Stream (self-referential, mirrors ScreenSet `active: *mut Screen`) | Any two overlapping `term()` borrows are aliasing `&mut Terminal` = instant SB UB; the "never nest two live borrows / never hold across re-reads" rule (comment :250-252) is enforced only by convention, and every stream action runs it | VT-STREAM (executes on every parsed action; UB trigger needs a code-shape mistake, not input, but it would fire ON input) |
| B2 | crates/rustty-termio/src/stream_handler.rs:378 `fn term(&self) -> &mut Terminal` (private) | termio's copy of the same precedent (comment cites rust-mut-args-strongly-protected) | Same aliasing contract, same conventional enforcement | VT-STREAM (same caveat as B1) |
| B3 | crates/rustty-vt/src/page_impl.rs:1289 (`get_cells`) | `&self -> *mut [Cell]` — the allow is mut_from_ref but the return is a raw slice ptr | Counted under A1; listed here for grep-completeness | VT-STREAM |

## Class C — lifetime-less pointer-holding `pub` structs (safe to construct/copy)

Pointer fields a safe user can read, copy, store, and outlive-the-target with;
every deref site trusts them. (The full field-level inventory is
`.port/audits/GHOSTTY_LIFETIMES.tsv`; rows tagged
`[audited-against-port 2026-07-10]`.)

| # | Site | Fields | Hazard | Triage |
|---|------|--------|--------|--------|
| C1 | crates/rustty-vt/src/screen.rs:245-247 `Cursor` | `pub page_pin: *mut Pin`, `page_row: *mut Row`, `page_cell: *mut Cell` | The hottest pointers in the port: shallow-copied (screen.rs:187 note), must be re-pointed on every page reflow/prune; a missed re-point is UAF on the next print | VT-STREAM |
| C2 | crates/rustty-vt/src/screen_set.rs:44 `ScreenSet` | `pub active: *mut Screen`, `all: [Option<*mut Screen>; COUNT]` | `active` aliases an `all` entry (PORT NOTE admits key/index discipline "finalized in vt-11"); stale `active` after screen destroy = UAF; `generations` counter is the conventional guard | VT-STREAM (mode 1047/1049 alt-screen switches are input-driven) |
| C3 | crates/rustty-vt/src/pagelist/mod.rs:630 `Pin` (`Clone, Copy`) | `pub node: *mut Node` | Copyable pin into the page list; validity across prune/reflow is maintained only for *tracked* pins — any safe-code copy held across a mutation dangles (the `garbage` flag is advisory) | VT-STREAM (viewport/selection pins move under scrollback pruning) |
| C4 | crates/rustty-vt/src/highlight.rs:70 `Tracked` | `pub start/end: *mut Pin` | Raw tracked-pin handles owned by `screen.pages`; struct outliving the screen (or double-deinit) derefs/destroys dangling pins | INTERNAL |
| C5 | crates/rustty-vt/src/formatter.rs:285/615/980/1097 `TerminalFormatter`/`ScreenFormatter`/`PageListFormatter`/`PageFormatter` | `page: *const Page` (+ terminal/screen/list ptrs) | Formatter outliving its target reads freed memory on `format()`; construction is safe | INTERNAL (dump/format APIs) |
| C6 | crates/rustty-vt/src/hash_map.rs:307 `Entry`, :319 `GetOrPutResult` | `pub key_ptr/value_ptr: *mut K/*mut V` | Zig-faithful HashMap handles: valid only until the next map mutation; safe code can hold them across `put` (Zig has the same hazard — ported, not introduced) | INTERNAL |
| C7 | crates/rustty-vt/src/search/pagelist.rs:47, search/screen.rs:95/:145 | stored `*mut PageList` / `*mut Screen` from A6 ctors | See A6 | INTERNAL |
| C8 | crates/rustty-vt-c/src/terminal.rs:92 `TerminalWrapper` | `terminal: *mut ZigTerminal`, `stream: Stream<'static>` (erased lifetime), `tracked_grid_refs: HashSet<*mut TrackedGridRef>` | The C-API root object: fabricated `'static` stream + raw back-ptrs, recovered via container_of (vtc-effects-trampoline-seam-d3); soundness = C caller contract | INTERNAL (behind unsafe extern fns; C consumer owns lifecycle) |
| C9 | crates/rustty-vt-c/src/grid_ref.rs:47-51 `CGridRef` (`node: *mut Node` + null sentinel), grid_ref_tracked.rs:41 `TrackedGridRef`, render.rs:90/:124 `RowIteratorWrapper`/`RowCellsWrapper`, kitty_graphics.rs:109 `PlacementIteratorWrapper`, sgr.rs:356 `ParserWrapper`, sys.rs:27 `Image` | C-handle structs holding raw ptrs into the terminal | Stale-handle deref after terminal mutation/free; generation counters (L8) are the conventional guard | INTERNAL (unsafe-extern surface) |
| C10 | crates/rustty-vt-c/src/terminal.rs:106 `Effects` | `userdata: *mut c_void` + callback fn ptrs | callback-ctx class: userdata validity/threading is entirely the C embedder's contract | INTERNAL |
| C11 | crates/rustty-vt-c/src/selection.rs:210/:227/:245 option structs + :876-902 `codepoint_slice`; terminal.rs:401-432 enquiry/xtversion | fabricated `Option<&'static [u32]>`/`&'static` views over caller-owned memory | LIFETIMES audit L5/L6: sound today because consumed in-call; the hazard is a future refactor STORING the fabricated `'static` | THEORETICAL (store-it-later class; SAFETY-documented) |

## Verified-clean in this pass (recorded so they are not re-raised)

- **rustty-vt-c export surface:** 154 `pub unsafe extern "C" fn` (honest
  signatures); only 3 safe `pub extern "C" fn`, none deref a caller pointer
  (lib.rs:1141 by-value enum tag; lib.rs:1326 + types.rs:621 return a cached
  `'static` CString ptr).
- **rustty-xev stream.rs:289/:751/:850:** carry the lint allow but are already
  `pub unsafe fn` — obligation is in the signature; not entries.
- **kqueue M1/M2 (cross-ref, NOT duplicated here):** real latent SB UB inside
  `unsafe` blocks (not unsound-*safe* surface) — filed in DEFERRALS.md
  "LIFETIMES-AUDIT findings (2026-07-10)" with a mirihar-proof gate.

## Follow-ups

1. T17 step 2 (deferred → detectors pass): retro `SAFETY(origin/dest)` tags;
   the census's `--enforce-safety-tags` mode then becomes the failing lint.
2. T21 sweep-2 auditor #6: per-block review of terminal.rs/screen.rs/
   formatter.rs unsafe bodies extends this ledger beyond the seed.
3. Candidate promotions to honest `unsafe fn` (Bun destination
   "stays-unsafe/consolidated") need a fidelity ruling first — the reference
   API is safe-shaped Zig, and R1/page.rs:58-63 chose lint-allow over
   signature change deliberately. Any change goes through the plan, not this
   ledger (R5: record, never weaken unilaterally).
