# New findings discovered during closure work

This file records defects found while closing a `LEDGER.tsv` row that the
original parity audit did not catch. It is **not** part of the audit's fixed
denominator (497 rows) — never add a row to `LEDGER.tsv` for something
recorded here; that file's row count must stay stable. Shape mirrors a ledger
row: id, severity, kind, oracle file:line, rustty file:line, description, how
found, and current state.

---

## N1 — `GlobalState::new` performs an extra, unfaithful environ capture

- **id**: N1
- **severity**: medium (source-level fidelity divergence; masked a `high`-rated
  behavioral defect on the only reachable success path, see note below)
- **kind**: missing-branch / extra-behavior (the reference's `.main` `InitOpts`
  arm reuses a caller-supplied environ; the port's `GlobalState::new`
  unconditionally re-captures from the live process instead)
- **oracle**: `src/global.zig` — `GlobalState.init`'s `InitOpts` union,
  `.main => |m| m.environ` (around the `self.environ = switch (opts) { ... }`
  assignment referenced in `new_with_environ`'s own doc comment,
  `crates/rustty-global/src/lib.rs:415-424`). The oracle never re-reads the
  process environment at `GlobalState` construction time — it takes whatever
  block the caller already captured.
- **rustty (pre-fix)**: `crates/rustty-global/src/lib.rs:411-413`
  (`GlobalState::new` — `Self::new_with_environ(alloc, resources_dir,
  environ::Environ::from_process())`, an unconditional FRESH capture) called
  from `crates/rustty-apprt/src/lib.rs:398` (pre-fix:
  `rustty_core::GlobalState::new((), resources_dir)`).
- **description**: `GlobalState::new` grabs a brand-new
  `Environ::from_process()` snapshot and republishes it into the shared
  process-global `ENVIRON` (via `new_with_environ`'s
  `environ::set_environ(env.clone())`) every time it is called, rather than
  reusing the environ the caller already captured/maintained. In
  `init_with_action`, this call happens unconditionally, strictly after
  `ensureLocale()` on the only reachable success path — so it accidentally
  republishes whatever the live process environment happens to be at that
  point, REGARDLESS of whether anything upstream explicitly re-synced it.
- **how found**: while closing S43-F1 (`global.init` not calling
  `sync_environ()` after `ensureLocale()`), an end-to-end regression test
  (mutate `LANG`, call `init_with_action`, assert `environ_map()` reflects the
  correction) was found to pass identically whether the `sync_environ()` fix
  was present or absent. Instrumenting both call sites showed
  `GlobalState::new`'s internal fresh capture — a few lines after
  `ensureLocale()` — was silently doing the republish job instead. This
  masking meant S43-F1's originally-claimed behavioral impact (termio
  child-process environments, a later `sync_environ()` caller seeing a stale
  locale) did not actually reproduce today; see the severity note on S43-F1's
  ledger closure.
- **state**: **CLOSED** — fixed in the same commit that closed S43-F1
  (`crates/rustty-apprt/src/lib.rs:398`, changed from `GlobalState::new((),
  resources_dir)` to `GlobalState::new_with_environ((), resources_dir, env)`
  where `env` is the already-captured, already-`sync_environ()`-resynced
  block — matching the oracle's `.main` arm exactly and removing the
  redundant/unfaithful re-capture). Commit: see the S43-F1 closure commit in
  `crates/rustty-apprt/src/lib.rs`'s history.
- **note on a deadlock found while fixing this**: the first attempt at the
  fix wrote `rustty_global::environ().clone()` inline as a call argument to
  `new_with_environ`. `rustty_global::environ()` returns an `EnvironRef`
  guard wrapping a live `RwLock` READ lock; Rust's temporary-lifetime rule
  keeps a temporary created while evaluating a `let` statement's initializer
  alive until the end of that ENCLOSING statement — so the read guard stayed
  held for the full duration of the `new_with_environ` call, which itself
  takes a WRITE lock on the same `RwLock` internally
  (`environ::set_environ`). `std::sync::RwLock` is not reentrant, so this was
  a same-thread self-deadlock (reproduced: the test hung indefinitely,
  confirmed via `sample`/`lsof`/process inspection that no other process or
  lock was involved). Fixed by binding the clone to a local in its own
  statement first (`let env = rustty_global::environ().clone(); let mut
  state = GlobalState::new_with_environ((), resources_dir, env);`), so the
  read guard is dropped before the write-locking call begins. This is purely
  an implementation detail of the fix, not a further oracle divergence — no
  new finding recorded for it — but is called out here and at the call site
  in `lib.rs` because it is exactly the kind of hazard a future edit to that
  line could reintroduce silently.

---

## N2 — one `log.warn` in `tmux/viewer.zig` has no restorable Rust counterpart

- **id**: N2
- **severity**: low (test-helper diagnostic only; no shipped code path)
- **kind**: unportable-by-signature (the port's test helper has a different
  error-reporting contract than the reference's, and one of its two format
  arguments has no `Debug` impl to render)
- **oracle**: `src/terminal/tmux/viewer.zig:1497` —
  `log.warn("testViewer step failed i={} step={}", .{ i, step });` inside
  `fn testViewer(viewer: *Viewer, steps: []const TestStep) !void`, in the
  `step.run(viewer) catch |err|` arm.
- **rustty**: `crates/rustty-vt/src/tmux/viewer.rs:2606-2624` — left as a
  fidelity comment with a PORT NOTE stating both obstructions.
- **description**: two independent reasons, either sufficient. (1) The port's
  `TestStep::run` returns `()` and reports a failed step by panicking — the
  port of the reference's `!void` plus `try`-assertions — so there is no error
  value to `catch` and therefore no arm for the emission to live in.
  (2) `step={}` would require `Debug` on `TestStep`, which holds a
  `Box<dyn Fn>`; closures have no `Debug` impl, so the argument cannot be
  rendered without changing the helper's representation.
- **how found**: while closing S209-F2 (all `log.*` diagnostics in the tmux
  viewer). The site is NOT among that row's cited oracle anchors — the audit
  scoped the row to non-test code — so its absence does not make S209-F2's
  closure a hole. It is recorded here so the finale re-audit sees it as a
  tracked item in its own right rather than as a footnote in a closed row's
  `owner` cell.
- **controller ruling (2026-09-09)**: S209-F2 stays **CLOSED**, not `WAIVED`.
  The two states available carry different meanings — `CLOSED` is "every cited
  anchor restored", `WAIVED` is "not a defect" — and neither describes
  "complete within scope, with one out-of-scope site permanently unportable".
  `CLOSED` is correct on the row's own anchors, the exception is written into
  the tracked ledger's `owner` cell and into a PORT NOTE at the code site, and
  this entry gives the leftover its own visible record. Cost if wrong: the
  finale re-audit re-derives this one test-only site and finds it already
  explained in three places.
- **state**: **OPEN — will not fix.** Closing it requires changing
  `TestStep::run`'s contract to return a `Result` and giving `TestStep` a
  hand-written `Debug`, both of which would make the port's test helper LESS
  faithful to the reference in order to restore one of its diagnostics.

---

## N3 — the port shares one "seen" table between two `logUnsupportedOnce` sites the reference keeps separate

- **id**: N3
- **severity**: low (exactly one suppressed diagnostic per process; no state or
  rendering difference)
- **kind**: behavioral-difference (a warning the reference emits twice per
  process — once from each of two call sites — the port emits once)
- **oracle**: `src/terminal/stream.zig:734` and `:1185`, both calling
  `logUnsupportedOnce("ignoring UTF-8-decoded C1 controls, first: 0x{x}", .{…},
  0x80)`. The helper (`stream.zig:3040`) declares its `Static.seen` table
  **inside** the generic function, so Zig monomorphizes one table per
  `@TypeOf(args)` instantiation. The two sites pass different argument types —
  `:734` passes `cp`, a `u32` element of `cp_buf: [4096]u32` (`stream.zig:671`);
  `:1185` passes `c`, a narrower integer — so they are two distinct
  instantiations with **two separate tables**.
- **rustty**: `crates/rustty-vt/src/stream.rs:1151` and `:1847` both pass
  `&SEEN_UTF8_DECODED_C1` (`stream.rs:4719`) — one shared table.
- **description**: the port's `log_unsupported_once` takes the table by
  reference rather than instantiating it per argument type, so the two sites
  share suppression state. The first site to fire claims key `0x80`; the second
  then finds the key already present and never emits. The reference emits from
  both. Net effect: exactly one warning suppressed per process, on a path that
  is itself a rare-input diagnostic.
- **how found**: during task 14r-a's fix round, while verifying that a PORT NOTE
  in `stream.rs` correctly described which statics are shared between call
  sites. The note's sharing claim was accurate about the port and wrong about
  the reference. Recorded as a `KNOWN DIVERGENCE` comment at the static's
  definition.
- **why not fixed in that round**: it is a behavioral change outside the two
  rows that round was fixing (S163-F2, S164-F1), made on a branch two other
  workers were committing to. The correct fix is to give each site its own
  static and assert both emit — small, but it needs its own test and its own
  review rather than riding along inside a test-isolation fix.
- **state**: **OPEN** — to be fixed with a test that drives both call sites in
  one process and asserts two records, not one.

---

## N5 — `S94-F1`'s mpsc stand-in rests on a justification that is false, and the real port exists

- **id**: N5
- **severity**: medium (a subsystem the port replaced with a stand-in under a PORT NOTE
  that misstates why; the faithful implementation is already in the tree and unused)
- **kind**: unported-work / stale-deferral
- **oracle**: `src/font/shaper/coretext.zig:76-81` — the `CFReleaseThread` the shaper
  starts and hands released CoreFoundation objects to.
- **rustty**: `crates/rustty-font-shaper/src/shaper/coretext.rs:36-55` — an mpsc channel
  stand-in, formerly carrying a PORT NOTE calling the CFReleaseThread seam "deferred".
- **description**: the PORT NOTE's premise was that the release thread had not been
  ported. It had: `rustty_os::cf_release_thread` is a real, inhabited implementation on
  the live kqueue backend. Task 14c-4 verified this while closing the file's logging rows
  and corrected the note in place, but did not perform the swap — replacing a channel
  stand-in with the real thread is a runtime refactor, not a logging change, and belonged
  in neither that task's scope nor a fix round on a shared branch.
- **how found**: task 14c-4, while closing `S94-F4` and the other `shaper/coretext.rs`
  rows. Found by checking the PORT NOTE's factual claim rather than accepting it —
  the same method that found [[N4]] and the `codepoint_resolver` control-flow divergence.
- **why this is recorded separately from S94-F1**: `S94-F1` stays `OPEN` and is the row
  that tracks the swap. This entry exists because the row's own text describes the defect
  as a logging omission, which it is not, and because the corrected PORT NOTE now reads as
  a satisfactory answer when it is only an accurate description of an outstanding gap.
  This project has a standing lesson that stale deferral notes manufacture fake gaps; the
  inverse also holds — a corrected note can conceal a real one.
- **state**: **OPEN — needs an owner.** The swap is a runtime refactor: replace the mpsc
  stand-in with `rustty_os::cf_release_thread`, on the real kqueue backend, with a test
  that proves released objects reach it. Schedule it as its own task; it is not logging
  work and must not ride along inside a `LOG`-class row's closure.

---

## N4 — `DeferredFace.zig` has three live `log.warn` sites and no `LOG` row names the file

- **id**: N4
- **severity**: low (three diagnostics; no state or rendering difference)
- **kind**: dropped-diagnostic / **ledger under-coverage**
- **oracle**: `src/font/DeferredFace.zig:403` (`failed to init face for codepoint
  check face={s} err={}`), `:430` (`error creating deferred face err={}`), `:463`
  (`error loading deferred face err={}`). All three are live — the file's only
  other `log` mention, `:303`, is commented out upstream.
- **rustty**: `crates/rustty-font-shaper/src/deferred_face.rs` — all three were
  carried as PORT NOTE comments, two of them explicitly justified with the (now
  false) claim "this crate wires no logger".
- **description**: the file has four rows in `src/font` — `S85-F2`, `S85-F3`,
  `S85-F5`, `S85-F6` — and **not one is `class=LOG`**. The audit that produced the
  ledger found four defects in this file and did not find its dropped
  diagnostics, so no row's cited anchors would ever have led an executor here.
  This is the under-count shape `PROCEDURE.md` blind spot 3 was written for, and
  it is the second instance found in one stage (the first being the fifth,
  unclaimed site in `face/web_canvas.zig:317`).
- **how found**: task 14c-4, by enumerating the reference's OWN `log.*` calls
  per file across the whole of `src/font` and reconciling the count against the
  live emissions in the port — not by following any row's anchors. The same
  method found N6 in `discovery.zig`.
- **disposition**: `:403` and `:463` are **restored** and live, on the
  reference's `deferred_face` scope. `:430` is **WAIVED**: its only reference
  failure mode is OOM from `alloc.dupeZ`/`alloc.create`, and Rust's `Vec`/`Box`
  ABORT on allocation failure rather than returning, so the port has no reachable
  failure arm and no `null` return — the emission could only sit on the success
  path. PORT NOTE at the site; adjudicated at
  `.port/parity-closure/ADJUDICATION.md`, batch 14c-4, heading
  `N4 — deferred_face_new's dropped warn`.
- **why this is recorded separately from any row**: there is no row to close.
  Without this entry the work is invisible to the finale re-audit, and the file
  would read as fully covered because its four rows are all ticked.
- **state**: **CLOSED for the two restorable sites, WAIVED for the third.** The
  residual item for the finale is not code but process: **no `LOG`-class row
  exists for `DeferredFace.zig`, and the ledger should not be read as if the
  absence of a row means the absence of a defect.**

---

## N6 — `discovery.zig`'s two table-parse `log.warn`s were discarded by `.ok()`, and no row names them

- **id**: N6
- **severity**: low (two diagnostics; the `null`/`None` degradation was already
  faithful, only the warning was lost)
- **kind**: dropped-diagnostic / **ledger under-coverage**
- **oracle**: `src/font/discovery.zig:762` (`error parsing head table: {}`) and
  `:779` (`error parsing OS/2 table: {}`), both in the `catch` arm of an
  `opentype.*.init` inside the CoreText descriptor walk. These are the file's
  ONLY two `log.*` calls, and both are live.
- **rustty**: `crates/rustty-font-shaper/src/discovery.rs` — both parses were
  `.ok()`, discarding the error and the diagnostic together, with the reference's
  `log.warn` kept as a PORT NOTE comment. The file carried no `LOG_SCOPE` at all,
  even though task 14c-4's own scope table assigns `discovery.zig` the scope
  `discovery`.
- **description**: same shape as N4 — no ledger row names either site, so no
  executor following cited anchors would have reached them. Distinct from the
  waived cases in this stage: `Head::init` and `OS2::init` both return `Result`,
  so these are genuine reachable failure arms and the "no diagnostic on a success
  path" rule does not apply.
- **how found**: task 14c-4 fix round 1, in response to review item F4, which
  itself came from applying blind spot 3's enumeration to the one `src/font` file
  the stage had not swept.
- **disposition**: both **restored**. The `catch` arms were extracted into
  `parse_head_table` / `parse_os2_table` free fns taking `&[u8]`, because
  manufacturing a CTFont with a malformed table is not something the suite can
  do — the extraction is what makes the failure arm reachable from a test.
  Covered by four tests (a malformed table warns exactly once and still degrades
  to `None`; a well-formed one emits nothing).
- **state**: **CLOSED.** As with N4, the residual for the finale is the process
  point: two files in `src/font` had live diagnostics that the ledger never
  named.

## N6 — `metal/buffer.zig`'s two null-contents warns are unrestorable and unclaimed

- **id**: N6
- **severity**: low (two diagnostics that cannot fire in rustty; no behavioural divergence
  on any reachable path)
- **kind**: unrestorable-diagnostic / uncited-site
- **oracle**: `src/renderer/metal/buffer.zig:98` and `:155` —
  `log.warn("buffer contents ptr is null", .{}); return error.MetalFailed;` inside the
  `orelse` guard on `msgSend(?[*]u8, objc.sel("contents"), .{})`, in `sync` and
  `syncFromArrayLists` respectively.
- **rustty**: `crates/rustty-renderer/src/metal/buffer.rs` — both `dst` blocks, which call
  objc2's `MTLBuffer::contents()`.
- **description**: objc2 types `contents()` as returning `NonNull<c_void>`, so the
  reference's null guard has no counterpart. There is no failure arm for the `log.warn` to
  live in, and lifting it out of the arm would put a diagnostic on the success path — the
  production-bug shape this plan has already shipped once. Both sites keep the reference
  call as an R1 comment with a PORT NOTE saying why it is not restorable.
- **how found**: task 14c-2, enumerating the ORACLE's own `log.*` calls per file rather
  than trusting the ledger's anchors (PROCEDURE.md blind spot 3). `buffer.zig` is not cited
  by `S119-F1` or by any other row.
- **why this is recorded separately**: no row claims these sites, so no row's `CLOSED`
  state rests on them and the blind-spot-1 carve-out is not in play. This entry exists so
  the gap is tracked rather than living only in a gitignored report.
- **state**: **WAIVED-by-construction.** Nothing to schedule unless the port ever moves off
  objc2's `NonNull` contract for `contents()`; if it does, restore both warns with
  `error.MetalFailed` as the return.
