# LIFETIMES retroactive audit — findings ledger (2026-07-10, T16 audit close)

Audit of the port against `.port/audits/GHOSTTY_LIFETIMES.tsv` (oracle:
/Users/gael/projects/ghostty @ 4749c4e9; xev shard against the vendored libxev
sources). Plan: `docs/superpowers/plans/2026-07-10-port-realignment-and-validation-hardening.md`
(T16). **20 findings: 0 high, 2 medium, 18 low.** Both mediums were
adversarially re-verified against BOTH sources in the close pass (the Zig
call sites at kqueue.zig:323/:411/:955 confirmed `*Self`-clean; the Rust
`&mut` sites and the always-`None` `thread_pool` confirmed at kqueue.rs:321,
:629-630, :738-740, :1394) and FILED — SB-class, mirihar-proof required, dead
branch today. Disposition column: FIXED (this pass), FILED
(DEFERRALS.md "LIFETIMES-AUDIT findings (2026-07-10)"), ROW-CORRECTED
(TSV rust_type updated with an `[audited-against-port 2026-07-10: ...]` tag),
NOTED (TSV row annotated, no correction needed), or VERIFIED (no TSV change).

## Medium (2) — both FILED

### M1. kqueue thread_perform: cross-thread `&mut` into the MPSC queue
- **Where:** `crates/rustty-xev/src/kqueue.rs:1394` (port of kqueue.zig:948-959).
- **TSV rows:** `src/backend/kqueue.zig Completion.task_loop` (BACKREF) +
  `src/queue_mpsc.zig Queue(T).head/tail` (INTRUSIVE).
- **Finding:** `thread_perform` runs on a pool thread and forms
  `&mut (*(*c).task_loop).thread_pool_completions` — a unique reference
  concurrent with the loop thread's accesses (UB regardless of the AtomicPtr
  ops inside), whose retag also invalidates the stored stub self-pointers.
- **Verdict:** CONFIRMED, latent (branch dead: `thread_pool` always `None`,
  `Loop::init` hardcodes `Options::default()` at kqueue.rs:321;
  `ThreadPool::schedule` is `unimplemented!()`). Zig reference is clean
  (`*Self` method calls) — port-side hazard, not a fidelity break.
- **Disposition:** FILED (DEFERRALS.md §1) with fix prescription
  (`ptr::addr_of_mut!` chain, mirihar reproduce-then-prove) and an un-defer
  gate on the §6 ThreadPool body. TSV rows cross-tagged.

### M2. kqueue tick_inner: `&mut` retags invalidate the queue's stub pointers
- **Where:** `crates/rustty-xev/src/kqueue.rs:630` (init) and `:740` (pop).
- **TSV rows:** `src/queue_mpsc.zig Queue(T).head/tail` (INTRUSIVE).
- **Finding:** each `&mut self.thread_pool_completions` coerced to `*mut Self`
  is a fresh SB retag; the stub self-pointers written by init (line 630's
  tag) are invalidated by line 740's retag (and by each tick's `&mut self`),
  making pop's deref of the stored tail SB UB even single-threaded —
  contradicting queue_mpsc.rs's own PORT NOTE (lines 14-17) and the rows'
  stable-`*mut` prescription.
- **Verdict:** CONFIRMED, latent (same dead branch as M1). The close-pass
  re-check additionally established that the prescribed one-line
  `addr_of_mut!((*self_ptr)...)` fix is NOT trivially provable: the stored
  stub pointers still carry the init-tick's provenance across later
  `tick(&mut self)` retags, so the harness must settle whether the tick
  receiver itself needs a raw seam (or head/tail need UnsafeCell-class
  fields). Exactly the class of fix doctrine says must not be hand-reasoned.
- **Disposition:** FILED (DEFERRALS.md §1, shared entry with M1). TSV rows
  cross-tagged.

## Low (18)

| # | TSV row | Rust file | Finding (class) | Disposition |
|---|---------|-----------|-----------------|-------------|
| L1 | page.zig `Page.memory` | rustty-vt/src/page_impl.rs:126,445-473 | Benign repr diff: `owned: bool` flag encodes Zig's "no deinit after initBuf" caller contract in-type; all free paths verified; reinit preserves the flag | VERIFIED |
| L2 | PageList.zig `Clone.tracked_pins` | rustty-vt/src/pagelist/access.rs:716,764,875-899 | Benign repr diff: out-map owned inside opts (`Option<TrackedPinsRemap>`), `&mut` lifted to the opts struct; insert-only discipline preserved | ROW-CORRECTED |
| L3 | Parser.zig `Action.DCS.C` intermediates/params (FFI) | rustty-vt/src/parser.rs:209-225 | No DCS `#[repr(C)]` mirror exists in rustty-vt-c; sound deferral (reference has NO C-ABI consumer of Action.DCS.C — no @export, no header type; lib_vt.zig:63 is a Zig-module export), but the parser.rs PORT NOTE claimed "(ported in rustty-vt-c)" — inaccurate | **FIXED** (comment corrected to deferred-until-C-DCS-surface) + ROW-CORRECTED (both rows) |
| L4 | graphics_storage.zig `Placement Location.pin` (SHARED) | rustty-vt/src/kitty/graphics_storage.rs:788-793 | evict_image removes placements without `Placement::deinit`/`untrack_pin` — exact mirror of upstream graphics_storage.zig:591-597; faithful upstream pin-pool leak, not UB, not a port regression; all other removal paths call deinit | NOTED (row tagged: do not fix unilaterally, track upstream) |
| L5 | c/selection.zig `SelectWordOptions.boundary_codepoints` (+word_between, line whitespace) | rustty-vt-c/src/selection.rs:876-902 | codepoint_slice fabricates `Option<&'static [u32]>` over caller-owned memory; sound today (consumed in-call, SAFETY-documented) — store-it-later hazard class | NOTED (rows 185/186 tagged) |
| L6 | c/terminal.zig `Effects.enquiry/xtversion` | rustty-vt-c/src/terminal.rs:401-432 | Same fabricated-`'static` pattern on trampoline returns; faithful to the Zig shape, documented, no current UB | VERIFIED (same hazard class as L5) |
| L7 | sys.zig `decode_png` (module global) | rustty-vt/src/sys.rs:58,73-104 | Discipline matches (RwLock, fn ptrs never freed); fidelity gap: non-lib comptime default `&decodePngWuffs` unwired (defaults None despite rustty-wuffs existing) | NOTED + FILED (DEFERRALS.md §2) |
| L8 | c/grid_ref.zig `CGridRef.node` | rustty-vt-c/src/grid_ref.rs:47-51,88-95 | Benign repr diff: `*mut Node` + null sentinel vs Option<NonNull>; identical ABI/semantics; never-freed-via-field confirmed | ROW-CORRECTED |
| L9 | c/selection_gesture.zig `EventWrapper.word_boundary_codepoints` | rustty-vt-c/src/selection_gesture.rs:144 | Benign repr diff: `Option<Vec<u32>>` (u21 IS a u32 alias, not a newtype); validation/free/explicit-empty discipline exact | ROW-CORRECTED |
| L10 | c/sys.zig `Global.userdata` | rustty-vt-c/src/sys.rs:101-121 | Benign repr diff: whole-struct `static Mutex<Global>` + documented `unsafe impl Send` — strictly stronger than the row's per-field AtomicPtr suggestion | ROW-CORRECTED |
| L11 | c/terminal.zig `TerminalWrapper.tracked_grid_refs` | rustty-vt-c/src/terminal.rs:96,2148-2156 | Benign repr diff: `HashSet<*mut TrackedGridRef>` (map-to-void analog, order-independent detach); non-owning detach verbatim | ROW-CORRECTED |
| L12 | lib/allocator.zig `Allocator.ctx` | rustty-lib/src/allocator.rs:158 | Benign repr diff: `*mut c_void` vs NonNull<c_void>, same ABI; vtable + 4 fn-ptr rows match exactly | ROW-CORRECTED |
| L13 | sys.zig `Image.data` (signature caveat) | rustty-vt/src/sys.rs:48,69 | DecodePngFn drops the Allocator param; `Vec<u8>` via the single global allocator satisfies the same-allocator mandate structurally; vt-c reconciles by copy+free | ROW-CORRECTED |
| L14 | KeymapDarwin.zig `Keymap.source` | rustty-input/src/keymap_darwin.rs:53,114-123 | Row wording: CFRelease lives in explicit deinit()/reload(), NOT a Drop impl (faithful to Zig); reinit error path leaks the +1 source exactly as upstream does | ROW-CORRECTED |
| L15 | font/Atlas.zig `Atlas.data` | rustty-font/src/atlas.rs:138,239 | Benign repr diff: allocator-generic `Vec<u8, A>` (Vec::new_in); ownership/free-on-drop matches | ROW-CORRECTED |
| L16 | DeferredFace.zig `Fontconfig.{pattern,charset,langset,variations}` | rustty-font-shaper/src/deferred_face.rs:29,54-76 | Comment-only/DEFERRED per the locked macOS-first fork; prescriptions correct-for-when-the-branch-lands; nothing live to audit | VERIFIED (no row change) |
| L17 | shaper/coretext.zig `RunIteratorHook.shaper` (+harfbuzz/noop) | rustty-font-shaper/src/shaper/{coretext.rs:1171-1173,harfbuzz.rs:541-542,noop.rs:182-183}; rustty-renderer/src/generic.rs:3282-3295 | Discipline VERIFIED: `*mut Shaper` hooks, raw derefs, production interleave routes via the hook's OWN pointer per shaper-hook-sb-clean-interleave | VERIFIED |
| L18 | shell_integration.zig `TmpResourcesDir.path` | rustty-termio/src/shell_integration.rs:1708,1753-1763 | Ownership faithful; byte source lossy (to_string_lossy) vs raw-OsStr-bytes rule; test-only, benign on macOS | NOTED + FILED (DEFERRALS.md §3) |
| L19 | vaxis Loop.zig `Loop(T).vaxis` | rustty-vaxis/src/loop_.rs:130-140 | Benign: SendLoop lifetime-erasure for the read thread; documented narrow projection (thread touches only lifetime-free state) | VERIFIED |

(L19 makes the low count 18 by the original audit's grouping: L5 covers three
option-struct rows and L3 covers two FFI rows; the table lists one line per
finding as delivered.)

## Close-pass actions (this commit)

1. **Fixed (1):** `crates/rustty-vt/src/parser.rs` DCS.C PORT NOTE — replaced
   the inaccurate "(ported in rustty-vt-c)" with the verified
   deferred-until-a-C-DCS-surface rationale (L3). Comment-only, zero runtime
   surface.
2. **Filed (3):** DEFERRALS.md "LIFETIMES-AUDIT findings (2026-07-10)" —
   M1+M2 (one entry, with fix prescription, the cross-tick-retag caveat the
   close pass uncovered, mirihar requirement, and a §6-gate un-defer trigger),
   L7 (wuffs default decoder parity), L18 (lossy path bytes cleanup).
3. **TSV:** 19 rows corrected/annotated with
   `[audited-against-port 2026-07-10: ...]` tags; header stamped
   `# audited-against-port: 2026-07-10`. Column integrity re-validated
   (263 data/comment lines -> 265 with stamp+tags, all data rows 7 columns).

No HIGH findings existed; the two mediums were re-verified (not just
re-read) and correctly remain medium: the UB is real but unreachable, and
per doctrine an SB-class fix without a mirihar proof would itself violate
R5/shaper-hook-sb-clean-interleave, so filing is the R5-safe close.
